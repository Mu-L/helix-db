use crate::commands::auth::Credentials;
use crate::commands::integrations::helix::CLOUD_AUTHORITY;
use crate::config::{AvailabilityMode, BuildMode, WorkspaceConfig};
use crate::prompts;
use eyre::{Result, eyre};
use serde::Deserialize;

// ============================================================================
// Result types
// ============================================================================

pub struct StandardClusterResult {
    pub cluster_id: String,
    pub instance_name: String,
    pub build_mode: BuildMode,
}

pub struct EnterpriseClusterResult {
    pub cluster_id: String,
    pub instance_name: String,
    pub availability_mode: AvailabilityMode,
    pub gateway_node_type: String,
    pub db_node_type: String,
    pub min_instances: u64,
    pub max_instances: u64,
}

pub enum ClusterResult {
    Standard(StandardClusterResult),
    Enterprise(EnterpriseClusterResult),
}

// ============================================================================
// API response types
// ============================================================================

#[derive(Deserialize)]
struct CliWorkspace {
    id: String,
    name: String,
    #[allow(dead_code)]
    url_slug: String,
}

#[derive(Deserialize)]
struct CliBillingResponse {
    has_billing: bool,
    workspace_type: String,
    #[allow(dead_code)]
    plan: String,
}

#[derive(Deserialize)]
struct CliProject {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct CreateProjectResponse {
    id: String,
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize)]
struct CreateClusterResponse {
    cluster_id: String,
}

// ============================================================================
// Main flow
// ============================================================================

/// Run the workspace → project → cluster selection/creation flow.
/// Returns a ClusterResult describing the created cluster.
pub async fn run_workspace_project_cluster_flow(
    project_name: &str,
    credentials: &Credentials,
) -> Result<ClusterResult> {
    let client = reqwest::Client::new();
    let base_url = format!("https://{}", *CLOUD_AUTHORITY);

    // Step 1: Workspace selection
    let workspace_id = select_or_load_workspace(&client, &base_url, credentials).await?;

    // Step 2: Billing check
    let billing = check_billing(&client, &base_url, credentials, &workspace_id).await?;

    // Step 3: Project matching
    let project_id =
        match_or_create_project(&client, &base_url, credentials, &workspace_id, project_name)
            .await?;

    // Step 4: Cluster type selection
    let is_enterprise = billing.workspace_type == "enterprise";
    let cluster_type = if is_enterprise {
        prompts::select_cluster_type()?
    } else {
        "standard"
    };

    // Step 5/6: Configure and create cluster
    match cluster_type {
        "enterprise" => {
            create_enterprise_cluster_flow(&client, &base_url, credentials, &project_id).await
        }
        _ => create_standard_cluster_flow(&client, &base_url, credentials, &project_id).await,
    }
}

async fn select_or_load_workspace(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &Credentials,
) -> Result<String> {
    let mut workspace_config = WorkspaceConfig::load()?;

    if workspace_config.has_workspace_id() {
        return Ok(workspace_config.workspace_id.clone().unwrap());
    }

    // Fetch workspaces
    let workspaces: Vec<CliWorkspace> = client
        .get(format!("{}/api/cli/workspaces", base_url))
        .header("x-api-key", &credentials.helix_admin_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch workspaces: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to fetch workspaces: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse workspaces: {}", e))?;

    if workspaces.is_empty() {
        return Err(eyre!(
            "No workspaces found. Go to the dashboard to create a workspace first."
        ));
    }

    // Convert for prompt
    let ws_for_prompt: Vec<crate::commands::sync::CliWorkspace> = workspaces
        .iter()
        .map(|w| crate::commands::sync::CliWorkspace {
            id: w.id.clone(),
            name: w.name.clone(),
            url_slug: w.url_slug.clone(),
        })
        .collect();

    let selected = prompts::select_workspace(&ws_for_prompt)?;

    // Save selection
    workspace_config.workspace_id = Some(selected.clone());
    workspace_config.save()?;

    Ok(selected)
}

async fn check_billing(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &Credentials,
    workspace_id: &str,
) -> Result<CliBillingResponse> {
    let billing: CliBillingResponse = client
        .get(format!(
            "{}/api/cli/workspaces/{}/billing",
            base_url, workspace_id
        ))
        .header("x-api-key", &credentials.helix_admin_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to check billing: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to check billing: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse billing response: {}", e))?;

    if !billing.has_billing {
        return Err(eyre!(
            "No active billing found for this workspace. Go to the dashboard to set up billing first."
        ));
    }

    Ok(billing)
}

async fn match_or_create_project(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &Credentials,
    workspace_id: &str,
    project_name: &str,
) -> Result<String> {
    // Fetch projects
    let projects: Vec<CliProject> = client
        .get(format!(
            "{}/api/cli/workspaces/{}/projects",
            base_url, workspace_id
        ))
        .header("x-api-key", &credentials.helix_admin_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch projects: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to fetch projects: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse projects: {}", e))?;

    // Try to find matching project by name
    if let Some(existing) = projects.iter().find(|p| p.name == project_name) {
        let use_existing = prompts::confirm(&format!("Use existing project '{}'?", existing.name))?;

        if use_existing {
            return Ok(existing.id.clone());
        }

        // User doesn't want existing — prompt for new name
        crate::output::warning(
            "Note: choosing a different name will overwrite the project name in your helix.toml to avoid conflicts.",
        );
        let new_name = prompts::input_project_name(project_name)?;
        let project_id =
            create_project(client, base_url, credentials, workspace_id, &new_name).await?;
        return Ok(project_id);
    }

    // Project not found — offer to create
    let should_create =
        prompts::confirm(&format!("Project '{}' not found. Create it?", project_name))?;

    if !should_create {
        return Err(eyre!("Project creation cancelled"));
    }

    let project_id =
        create_project(client, base_url, credentials, workspace_id, project_name).await?;
    Ok(project_id)
}

async fn create_project(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &Credentials,
    workspace_id: &str,
    name: &str,
) -> Result<String> {
    let resp: CreateProjectResponse = client
        .post(format!(
            "{}/api/cli/workspaces/{}/projects",
            base_url, workspace_id
        ))
        .header("x-api-key", &credentials.helix_admin_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .map_err(|e| eyre!("Failed to create project: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to create project: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse create project response: {}", e))?;

    crate::output::success(&format!("Project '{}' created", name));
    Ok(resp.id)
}

async fn create_standard_cluster_flow(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &Credentials,
    project_id: &str,
) -> Result<ClusterResult> {
    let cluster_name = prompts::input_cluster_name("prod")?;
    let build_mode = prompts::select_build_mode()?;

    let build_mode_str = match build_mode {
        BuildMode::Dev => "dev",
        BuildMode::Release => "release",
        BuildMode::Debug => "dev",
    };

    let resp: CreateClusterResponse = client
        .post(format!(
            "{}/api/cli/projects/{}/clusters",
            base_url, project_id
        ))
        .header("x-api-key", &credentials.helix_admin_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "cluster_name": cluster_name,
            "build_mode": build_mode_str,
        }))
        .send()
        .await
        .map_err(|e| eyre!("Failed to create cluster: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to create cluster: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse create cluster response: {}", e))?;

    crate::output::success(&format!(
        "Cluster '{}' created (ID: {})",
        cluster_name, resp.cluster_id
    ));

    Ok(ClusterResult::Standard(StandardClusterResult {
        cluster_id: resp.cluster_id,
        instance_name: cluster_name,
        build_mode,
    }))
}

async fn create_enterprise_cluster_flow(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &Credentials,
    project_id: &str,
) -> Result<ClusterResult> {
    let cluster_name = prompts::input_cluster_name("prod")?;
    let availability_mode = prompts::select_availability_mode()?;
    let is_ha = availability_mode == AvailabilityMode::Ha;

    let gateway_node_type = prompts::select_gateway_node_type(is_ha)?;
    let db_node_type = prompts::select_db_node_type(is_ha)?;

    let (min_instances, max_instances) = if is_ha {
        let min = prompts::input_min_instances()?;
        let max = prompts::input_max_instances(min)?;
        (min, max)
    } else {
        (1, 1)
    };

    // Show summary
    println!();
    crate::output::info(&format!("Cluster: {}", cluster_name));
    crate::output::info(&format!("Mode: {}", availability_mode));
    crate::output::info(&format!("Gateway: {}", gateway_node_type));
    crate::output::info(&format!("DB: {}", db_node_type));
    if is_ha {
        crate::output::info(&format!("Instances: {} - {}", min_instances, max_instances));
    }
    println!();

    if !prompts::confirm("Create this enterprise cluster?")? {
        return Err(eyre!("Cluster creation cancelled"));
    }

    let availability_mode_str = match availability_mode {
        AvailabilityMode::Dev => "dev",
        AvailabilityMode::Ha => "ha",
    };

    let resp: CreateClusterResponse = client
        .post(format!(
            "{}/api/cli/projects/{}/enterprise-clusters",
            base_url, project_id
        ))
        .header("x-api-key", &credentials.helix_admin_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "cluster_name": cluster_name,
            "availability_mode": availability_mode_str,
            "gateway_node_type": gateway_node_type,
            "db_node_type": db_node_type,
            "min_instances": min_instances,
            "max_instances": max_instances,
        }))
        .send()
        .await
        .map_err(|e| eyre!("Failed to create enterprise cluster: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to create enterprise cluster: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse response: {}", e))?;

    crate::output::success(&format!(
        "Enterprise cluster '{}' created (ID: {})",
        cluster_name, resp.cluster_id
    ));

    Ok(ClusterResult::Enterprise(EnterpriseClusterResult {
        cluster_id: resp.cluster_id,
        instance_name: cluster_name,
        availability_mode,
        gateway_node_type,
        db_node_type,
        min_instances,
        max_instances,
    }))
}
