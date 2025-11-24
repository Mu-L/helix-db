use crate::{
    commands::integrations::helix::CLOUD_AUTHORITY,
    config::{CloudInstanceConfig, DbConfig},
    project::ProjectContext,
    sse_client::{SseClient, SseEvent, SseProgressHandler},
    utils::{print_error, print_info, print_status, print_success},
};
use eyre::{OptionExt, Result, eyre};
use serde_json::json;

/// Create a new cluster in Helix Cloud
pub async fn run(instance_name: &str, region: Option<String>) -> Result<()> {
    print_status("CREATE", &format!("Creating cluster: {}", instance_name));

    // Load project context
    let project = ProjectContext::find_and_load(None)?;

    // Check if this instance already exists
    if project.config.cloud.contains_key(instance_name) {
        return Err(eyre!(
            "Instance '{}' already exists in helix.toml. Use a different name or remove the existing instance.",
            instance_name
        ));
    }

    // Get credentials
    let home = dirs::home_dir().ok_or_eyre("Cannot find home directory")?;
    let cred_path = home.join(".helix").join("credentials");

    if !cred_path.exists() {
        print_error("Not logged in. Please run 'helix auth login' first.");
        return Err(eyre!("Not authenticated"));
    }

    let credentials = crate::commands::auth::Credentials::read_from_file(&cred_path);

    if !credentials.is_authenticated() {
        print_error("Invalid credentials. Please run 'helix auth login' again.");
        return Err(eyre!("Invalid credentials"));
    }

    // Generate cluster ID
    let cluster_id = uuid::Uuid::new_v4().to_string();

    // Prepare create cluster request
    let region = region.unwrap_or_else(|| "us-east-1".to_string());
    let create_request = json!({
        "name": instance_name,
        "cluster_id": &cluster_id,
        "region": &region,
        "instance_type": "small",
        "user_id": credentials.user_id
    });

    // POST to create-cluster endpoint
    let client = reqwest::Client::new();
    let create_url = format!("http://{}/create-cluster", *CLOUD_AUTHORITY);

    let response = client
        .post(&create_url)
        .header("x-api-key", &credentials.helix_admin_key)
        .header("Content-Type", "application/json")
        .json(&create_request)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(eyre!("Failed to initiate cluster creation: {}", error_text));
    }

    // Parse response to get Stripe checkout URL
    let response_data: serde_json::Value = response.json().await?;
    let stripe_url = response_data
        .get("stripe_url")
        .and_then(|v| v.as_str())
        .ok_or_eyre("No Stripe checkout URL in response")?;

    print_info(&format!("Opening Stripe checkout in your browser..."));
    print_info(&format!("If the browser doesn't open, visit: {}", stripe_url));

    // Open browser to Stripe checkout
    if let Err(e) = webbrowser::open(stripe_url) {
        print_error(&format!("Failed to open browser: {}", e));
        print_info(&format!("Please manually open: {}", stripe_url));
    }

    // Connect to SSE stream for provisioning status
    print_status("WAITING", "Waiting for payment confirmation...");

    let sse_url = format!("http://{}/create-cluster-status/{}", *CLOUD_AUTHORITY, cluster_id);
    let sse_client = SseClient::new(sse_url)
        .header("x-api-key", &credentials.helix_admin_key)
        .timeout(std::time::Duration::from_secs(1800)); // 30 minutes

    let progress = SseProgressHandler::new("Provisioning cluster...");
    let mut cluster_ready = false;

    sse_client
        .connect(|event| {
            match event {
                SseEvent::Progress { percentage, message } => {
                    progress.set_progress(percentage);
                    if let Some(msg) = message {
                        progress.set_message(&msg);
                    }
                    Ok(true)
                }
                SseEvent::Log { message, .. } => {
                    progress.println(&message);
                    Ok(true)
                }
                SseEvent::StatusTransition { to, message, .. } => {
                    let msg = message.unwrap_or_else(|| format!("Status: {}", to));
                    progress.println(&msg);
                    Ok(true)
                }
                SseEvent::Success { .. } => {
                    cluster_ready = true;
                    progress.finish("Cluster provisioned successfully!");
                    Ok(false) // Stop processing
                }
                SseEvent::Error { message, .. } => {
                    progress.finish_error(&format!("Error: {}", message));
                    Err(eyre!("Cluster creation failed: {}", message))
                }
                _ => Ok(true), // Ignore other events
            }
        })
        .await?;

    if !cluster_ready {
        return Err(eyre!("Cluster creation did not complete successfully"));
    }

    // Save cluster configuration to helix.toml
    let config = CloudInstanceConfig {
        cluster_id: cluster_id.clone(),
        region: Some(region.clone()),
        build_mode: crate::config::BuildMode::Release,
        db_config: DbConfig::default(),
    };

    // Update helix.toml
    let mut helix_config = project.config.clone();
    helix_config.cloud.insert(
        instance_name.to_string(),
        crate::config::CloudConfig::Helix(config),
    );

    let config_path = project.root.join("helix.toml");
    let toml_string = toml::to_string_pretty(&helix_config)?;
    std::fs::write(&config_path, toml_string)?;

    print_success(&format!(
        "Cluster '{}' created successfully! (ID: {})",
        instance_name, cluster_id
    ));
    print_info(&format!("Region: {}", region));
    print_info(&format!("Configuration saved to helix.toml"));
    print_info(&format!("You can now deploy with: helix push {}", instance_name));

    Ok(())
}
