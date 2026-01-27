use crate::commands::auth::require_auth;
use crate::commands::integrations::helix::CLOUD_AUTHORITY;
use crate::config::InstanceInfo;
use crate::output::{Operation, Step};
use crate::project::ProjectContext;
use crate::utils::print_warning;
use eyre::{Result, eyre};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct SyncResponse {
    #[allow(dead_code)]
    helix_toml: Option<String>,
    hx_files: HashMap<String, String>,
    #[allow(dead_code)]
    instance_name: String,
}

pub async fn run(instance_name: String) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;

    // Get instance config
    let instance_config = project.config.get_instance(&instance_name)?;

    if instance_config.is_local() {
        pull_from_local_instance(&project, &instance_name).await
    } else {
        pull_from_cloud_instance(&project, &instance_name, instance_config).await
    }
}

async fn pull_from_local_instance(project: &ProjectContext, instance_name: &str) -> Result<()> {
    let op = Operation::new("Syncing", instance_name);

    // For local instances, we'd need to extract the .hx files from the running container
    // or from the compiled workspace

    let workspace = project.instance_workspace(instance_name);
    let container_dir = workspace.join("helix-container");

    if !container_dir.exists() {
        op.failure();
        return Err(eyre!(
            "Instance '{instance_name}' has not been built yet. Run 'helix build {instance_name}' first."
        ));
    }

    // TODO: Implement extraction of .hx files from compiled container
    // This would reverse-engineer the queries from the compiled Rust code
    // or maintain source files alongside compiled versions

    print_warning("Local instance query extraction not yet implemented");
    println!("  Local instances compile queries into Rust code.");
    println!("  Query extraction from compiled code is not currently supported.");

    Ok(())
}

async fn pull_from_cloud_instance(
    project: &ProjectContext,
    instance_name: &str,
    instance_config: InstanceInfo<'_>,
) -> Result<()> {
    let op = Operation::new("Syncing", instance_name);

    // Verify this is a Helix Cloud instance
    let cluster_id = match &instance_config {
        InstanceInfo::Helix(config) => &config.cluster_id,
        InstanceInfo::FlyIo(_) => {
            op.failure();
            return Err(eyre!(
                "Sync is only supported for Helix Cloud instances, not Fly.io deployments"
            ));
        }
        InstanceInfo::Ecr(_) => {
            op.failure();
            return Err(eyre!(
                "Sync is only supported for Helix Cloud instances, not ECR deployments"
            ));
        }
        InstanceInfo::Local(_) => {
            op.failure();
            return Err(eyre!("Sync is only supported for cloud instances"));
        }
    };

    // Check auth
    let credentials = require_auth().await?;

    Step::verbose_substep(&format!("Downloading from cluster: {cluster_id}"));

    // Make API request to sync endpoint
    let client = reqwest::Client::new();
    let sync_url = format!("https://{}/api/clusters/{}/sync", *CLOUD_AUTHORITY, cluster_id);

    let mut sync_step = Step::with_messages("Fetching source files", "Source files fetched");
    sync_step.start();

    let response = match client
        .get(&sync_url)
        .header("x-api-key", &credentials.helix_admin_key)
        .header("x-cluster-id", cluster_id)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            sync_step.fail();
            op.failure();
            return Err(eyre!("Failed to connect to Helix Cloud: {}", e));
        }
    };

    // Handle response status
    match response.status() {
        reqwest::StatusCode::OK => {}
        reqwest::StatusCode::NOT_FOUND => {
            sync_step.fail();
            op.failure();
            return Err(eyre!(
                "No source files found for cluster '{}'. Make sure you have deployed at least once with `helix push`.",
                cluster_id
            ));
        }
        reqwest::StatusCode::UNAUTHORIZED => {
            sync_step.fail();
            op.failure();
            return Err(eyre!(
                "Authentication failed. Run 'helix auth login' to re-authenticate."
            ));
        }
        reqwest::StatusCode::FORBIDDEN => {
            sync_step.fail();
            op.failure();
            return Err(eyre!(
                "Access denied to cluster '{}'. Make sure you have permission to access this cluster.",
                cluster_id
            ));
        }
        status => {
            let error_text = response.text().await.unwrap_or_default();
            sync_step.fail();
            op.failure();
            return Err(eyre!("Sync failed ({}): {}", status, error_text));
        }
    }

    // Parse response
    let sync_response: SyncResponse = match response.json().await {
        Ok(resp) => resp,
        Err(e) => {
            sync_step.fail();
            op.failure();
            return Err(eyre!("Failed to parse sync response: {}", e));
        }
    };

    sync_step.done();

    // Get the queries directory from project config
    let queries_dir = project.root.join(&project.config.project.queries);

    // Create queries directory if it doesn't exist
    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }

    // Write .hx files
    let mut write_step = Step::with_messages("Writing source files", "Source files written");
    write_step.start();

    let mut files_written = 0;
    for (filename, content) in &sync_response.hx_files {
        let file_path = queries_dir.join(filename);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        std::fs::write(&file_path, content)
            .map_err(|e| eyre!("Failed to write {}: {}", filename, e))?;

        files_written += 1;
        Step::verbose_substep(&format!("  Wrote {}", filename));
    }

    write_step.done_with_info(&format!("{} files", files_written));

    op.success();

    // Print summary
    println!();
    crate::output::info(&format!(
        "Synced {} files from cluster '{}'",
        files_written, cluster_id
    ));
    crate::output::info(&format!(
        "Files saved to: {}",
        queries_dir.display()
    ));

    // List files that were synced
    if !sync_response.hx_files.is_empty() {
        println!();
        println!("Files synced:");
        for filename in sync_response.hx_files.keys() {
            println!("  - {}", filename);
        }
    }

    Ok(())
}
