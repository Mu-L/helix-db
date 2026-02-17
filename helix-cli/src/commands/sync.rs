use crate::commands::auth::require_auth;
use crate::commands::integrations::helix::cloud_base_url;
use crate::config::{
    AvailabilityMode, BuildMode, CloudConfig, CloudInstanceConfig, DbConfig,
    EnterpriseInstanceConfig, HelixConfig, InstanceInfo, WorkspaceConfig,
};
use crate::output::{Operation, Step};
use crate::project::ProjectContext;
use crate::prompts;
use crate::utils::print_warning;
use eyre::{Result, eyre};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize)]
struct SyncResponse {
    helix_toml: Option<String>,
    hx_files: HashMap<String, String>,
}

#[derive(Deserialize)]
struct EnterpriseSyncResponse {
    rs_files: HashMap<String, String>,
    helix_toml: Option<String>,
}

#[derive(Deserialize)]
struct CliEnterpriseCluster {
    pub cluster_id: String,
    pub cluster_name: String,
    pub project_name: String,
    #[allow(dead_code)]
    pub availability_mode: String,
}

#[derive(Deserialize)]
struct CliWorkspaceClusters {
    standard: Vec<CliCluster>,
    enterprise: Vec<CliEnterpriseCluster>,
}

#[derive(Deserialize)]
pub struct CliWorkspace {
    pub id: String,
    pub name: String,
    #[allow(dead_code)]
    pub url_slug: String,
}

#[derive(Deserialize)]
pub struct CliCluster {
    pub cluster_id: String,
    pub cluster_name: String,
    pub project_name: String,
}

#[derive(Clone, Deserialize)]
struct CliProject {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct CreateProjectResponse {
    id: String,
}

#[derive(Deserialize)]
struct CliProjectClusters {
    #[allow(dead_code)]
    project_id: String,
    project_name: String,
    standard: Vec<CliProjectStandardCluster>,
    enterprise: Vec<CliProjectEnterpriseCluster>,
}

#[derive(Deserialize)]
struct CliProjectStandardCluster {
    cluster_id: String,
    cluster_name: String,
    build_mode: String,
    #[allow(dead_code)]
    max_memory_gb: u32,
    #[allow(dead_code)]
    max_vcpus: f32,
}

#[derive(Deserialize)]
struct CliProjectEnterpriseCluster {
    cluster_id: String,
    cluster_name: String,
    availability_mode: String,
    gateway_node_type: String,
    db_node_type: String,
    min_instances: u64,
    max_instances: u64,
}

#[derive(Deserialize)]
struct CliClusterProject {
    #[allow(dead_code)]
    cluster_id: String,
    project_id: String,
    #[allow(dead_code)]
    project_name: String,
    #[allow(dead_code)]
    workspace_id: String,
}

const DEFAULT_QUERIES_DIR: &str = "db";

fn sanitize_relative_path(relative_path: &Path) -> Result<PathBuf> {
    let relative = relative_path;

    if relative.is_absolute() {
        return Err(eyre!("Refusing absolute path: {}", relative.display()));
    }

    let mut sanitized = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(eyre!(
                    "Refusing unsafe relative path: {}",
                    relative.display()
                ));
            }
        }
    }

    if sanitized.as_os_str().is_empty() {
        return Err(eyre!("Refusing empty path: {}", relative.display()));
    }

    Ok(sanitized)
}

fn safe_join_relative(base_dir: &Path, relative_path: &str) -> Result<PathBuf> {
    Ok(base_dir.join(sanitize_relative_path(Path::new(relative_path))?))
}

fn parse_and_sanitize_remote_config(
    remote_toml: &str,
    source: &str,
) -> Option<crate::config::HelixConfig> {
    let mut remote_config = match toml::from_str::<crate::config::HelixConfig>(remote_toml) {
        Ok(config) => config,
        Err(e) => {
            print_warning(&format!(
                "Ignoring remote helix.toml from {}: failed to parse ({})",
                source, e
            ));
            return None;
        }
    };

    match sanitize_relative_path(&remote_config.project.queries) {
        Ok(queries_relative) => {
            remote_config.project.queries = queries_relative;
        }
        Err(e) => {
            print_warning(&format!(
                "Ignoring unsafe remote project.queries '{}' from {}: {}. Using '{}'.",
                remote_config.project.queries.display(),
                source,
                e,
                DEFAULT_QUERIES_DIR
            ));
            remote_config.project.queries = PathBuf::from(DEFAULT_QUERIES_DIR);
        }
    }

    Some(remote_config)
}

fn serialize_remote_config(
    remote_config: &crate::config::HelixConfig,
    source: &str,
) -> Option<String> {
    match toml::to_string_pretty(remote_config) {
        Ok(serialized) => Some(serialized),
        Err(e) => {
            print_warning(&format!(
                "Failed to serialize sanitized remote helix.toml from {}: {}",
                source, e
            ));
            None
        }
    }
}

fn resolve_remote_queries_dir(
    base_dir: &Path,
    remote_config: Option<&crate::config::HelixConfig>,
) -> PathBuf {
    let Some(remote_config) = remote_config else {
        return base_dir.join(DEFAULT_QUERIES_DIR);
    };

    match sanitize_relative_path(&remote_config.project.queries) {
        Ok(queries_relative) => base_dir.join(queries_relative),
        Err(e) => {
            print_warning(&format!(
                "Ignoring unsafe remote project.queries '{}': {}. Using '{}'.",
                remote_config.project.queries.display(),
                e,
                DEFAULT_QUERIES_DIR
            ));
            base_dir.join(DEFAULT_QUERIES_DIR)
        }
    }
}

fn confirm_overwrite_or_abort(
    differing_files: &[String],
    assume_yes: bool,
    prompt: &str,
) -> Result<()> {
    if differing_files.is_empty() {
        return Ok(());
    }

    println!();
    println!("The following local files will be overwritten:");
    for file in differing_files {
        println!("  - {}", file);
    }
    println!();

    if assume_yes {
        crate::output::info("Proceeding with overwrite because --yes was provided.");
        return Ok(());
    }

    if !prompts::is_interactive() {
        return Err(eyre!(
            "Sync would overwrite {} files. Re-run with '--yes' to continue in non-interactive mode.",
            differing_files.len()
        ));
    }

    if !crate::prompts::confirm(prompt)? {
        return Err(eyre!("Sync aborted by user"));
    }

    Ok(())
}

async fn fetch_workspaces(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<CliWorkspace>> {
    let workspaces: Vec<CliWorkspace> = client
        .get(format!("{}/api/cli/workspaces", base_url))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch workspaces: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to fetch workspaces: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse workspaces response: {}", e))?;

    Ok(workspaces)
}

async fn resolve_workspace_id(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    workspace_config: &mut WorkspaceConfig,
) -> Result<String> {
    let workspaces = fetch_workspaces(client, base_url, api_key).await?;

    if workspaces.is_empty() {
        return Err(eyre!(
            "No workspaces found. Create a workspace in the dashboard first."
        ));
    }

    if let Some(cached_workspace_id) = workspace_config.workspace_id.clone() {
        if workspaces.iter().any(|w| w.id == cached_workspace_id) {
            return Ok(cached_workspace_id);
        }

        print_warning(
            "Saved workspace selection is no longer available. Please select a workspace again.",
        );
        workspace_config.workspace_id = None;
        workspace_config.save()?;
    }

    let selected = prompts::select_workspace(&workspaces)?;
    workspace_config.workspace_id = Some(selected.clone());
    workspace_config.save()?;
    Ok(selected)
}

fn update_project_name_in_helix_toml(project_root: &Path, new_project_name: &str) -> Result<()> {
    let helix_toml_path = project_root.join("helix.toml");
    let mut config = HelixConfig::from_file(&helix_toml_path)
        .map_err(|e| eyre!("Failed to load helix.toml for project rename: {}", e))?;

    config.project.name = new_project_name.to_string();
    config
        .save_to_file(&helix_toml_path)
        .map_err(|e| eyre!("Failed to update project name in helix.toml: {}", e))?;

    Ok(())
}

async fn resolve_or_create_project_for_sync(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    workspace_id: &str,
    project_name_from_toml: &str,
    project_root: &Path,
) -> Result<CliProject> {
    let projects: Vec<CliProject> = client
        .get(format!(
            "{}/api/cli/workspaces/{}/projects",
            base_url, workspace_id
        ))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch projects: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to fetch projects: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse projects response: {}", e))?;

    if let Some(existing) = projects
        .iter()
        .find(|p| p.name == project_name_from_toml)
        .cloned()
    {
        crate::output::info(&format!(
            "Using project '{}' from your selected workspace.",
            existing.name
        ));
        return Ok(existing);
    }

    let should_create = prompts::confirm(&format!(
        "Project '{}' was not found. Create it?",
        project_name_from_toml
    ))?;

    if !should_create {
        return Err(eyre!("Project selection cancelled"));
    }

    let chosen_name = prompts::input_project_name(project_name_from_toml)?;
    let created: CreateProjectResponse = client
        .post(format!(
            "{}/api/cli/workspaces/{}/projects",
            base_url, workspace_id
        ))
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "name": chosen_name }))
        .send()
        .await
        .map_err(|e| eyre!("Failed to create project: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to create project: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse create project response: {}", e))?;

    if chosen_name != project_name_from_toml {
        update_project_name_in_helix_toml(project_root, &chosen_name)?;
        crate::output::info(&format!(
            "Updated helix.toml project name to '{}' to match your selected cloud project name.",
            chosen_name
        ));
    }

    Ok(CliProject {
        id: created.id,
        name: chosen_name,
    })
}

async fn fetch_project_clusters(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    project_id: &str,
) -> Result<CliProjectClusters> {
    let project_clusters: CliProjectClusters = client
        .get(format!(
            "{}/api/cli/projects/{}/clusters",
            base_url, project_id
        ))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch project clusters: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to fetch project clusters: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse project clusters response: {}", e))?;

    Ok(project_clusters)
}

async fn fetch_cluster_project(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    cluster_id: &str,
) -> Result<CliClusterProject> {
    let cluster_project: CliClusterProject = client
        .get(format!(
            "{}/api/cli/clusters/{}/project",
            base_url, cluster_id
        ))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch cluster project: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to fetch cluster project: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse cluster project response: {}", e))?;

    Ok(cluster_project)
}

fn build_mode_from_cloud(value: &str) -> BuildMode {
    match value {
        "dev" => BuildMode::Dev,
        "release" => BuildMode::Release,
        _ => BuildMode::Release,
    }
}

fn availability_mode_from_cloud(value: &str) -> AvailabilityMode {
    match value {
        "ha" => AvailabilityMode::Ha,
        _ => AvailabilityMode::Dev,
    }
}

fn insert_unique_cloud_instance_name(
    cloud: &mut HashMap<String, CloudConfig>,
    preferred_name: &str,
    cluster_id: &str,
    config: CloudConfig,
) {
    let mut name = preferred_name.to_string();
    if cloud.contains_key(&name) {
        let suffix = cluster_id.chars().take(8).collect::<String>();
        name = format!("{}-{}", preferred_name, suffix);
    }
    cloud.insert(name, config);
}

fn insert_unique_enterprise_instance_name(
    enterprise: &mut HashMap<String, EnterpriseInstanceConfig>,
    preferred_name: &str,
    cluster_id: &str,
    config: EnterpriseInstanceConfig,
) {
    let mut name = preferred_name.to_string();
    if enterprise.contains_key(&name) {
        let suffix = cluster_id.chars().take(8).collect::<String>();
        name = format!("{}-{}", preferred_name, suffix);
    }
    enterprise.insert(name, config);
}

fn reconcile_project_config_from_cloud(
    project: &ProjectContext,
    project_clusters: &CliProjectClusters,
) -> Result<()> {
    let helix_toml_path = project.root.join("helix.toml");
    let mut config = HelixConfig::from_file(&helix_toml_path)
        .map_err(|e| eyre!("Failed to load helix.toml: {}", e))?;

    config.project.name = project_clusters.project_name.clone();
    config.cloud.clear();
    config.enterprise.clear();

    for cluster in &project_clusters.standard {
        let instance_config = CloudInstanceConfig {
            cluster_id: cluster.cluster_id.clone(),
            region: Some("us-east-1".to_string()),
            build_mode: build_mode_from_cloud(&cluster.build_mode),
            env_vars: HashMap::new(),
            db_config: DbConfig::default(),
        };

        insert_unique_cloud_instance_name(
            &mut config.cloud,
            &cluster.cluster_name,
            &cluster.cluster_id,
            CloudConfig::Helix(instance_config),
        );
    }

    for cluster in &project_clusters.enterprise {
        let instance_config = EnterpriseInstanceConfig {
            cluster_id: cluster.cluster_id.clone(),
            availability_mode: availability_mode_from_cloud(&cluster.availability_mode),
            gateway_node_type: cluster.gateway_node_type.clone(),
            db_node_type: cluster.db_node_type.clone(),
            min_instances: cluster.min_instances,
            max_instances: cluster.max_instances,
            db_config: DbConfig::default(),
        };

        insert_unique_enterprise_instance_name(
            &mut config.enterprise,
            &cluster.cluster_name,
            &cluster.cluster_id,
            instance_config,
        );
    }

    config
        .save_to_file(&helix_toml_path)
        .map_err(|e| eyre!("Failed to write helix.toml: {}", e))?;

    Ok(())
}

async fn sync_cluster_into_project(
    api_key: &str,
    cluster_id: &str,
    project: &ProjectContext,
    assume_yes: bool,
) -> Result<()> {
    let op = Operation::new("Syncing", cluster_id);
    let client = reqwest::Client::new();
    let sync_url = format!("{}/api/cli/clusters/{}/sync", cloud_base_url(), cluster_id);

    let mut sync_step = Step::with_messages("Fetching source files", "Source files fetched");
    sync_step.start();

    let response = client
        .get(&sync_url)
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to connect to Helix Cloud: {}", e))?;

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

    let sync_response: SyncResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse sync response: {}", e))?;

    sync_step.done();

    let queries_dir = project.root.join(&project.config.project.queries);
    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }

    let mut differing_files: Vec<String> = Vec::new();
    for (filename, content) in &sync_response.hx_files {
        let file_path = safe_join_relative(&queries_dir, filename)?;
        if file_path.exists()
            && let Ok(local_content) = std::fs::read_to_string(&file_path)
            && local_content != *content
        {
            differing_files.push(filename.clone());
        }
    }

    if let Err(e) = confirm_overwrite_or_abort(
        &differing_files,
        assume_yes,
        "Local changes in these files will be lost. Continue with overwrite?",
    ) {
        op.failure();
        return Err(e);
    }

    let mut write_step = Step::with_messages("Writing source files", "Source files written");
    write_step.start();

    let mut files_written = 0;
    for (filename, content) in &sync_response.hx_files {
        let file_path = safe_join_relative(&queries_dir, filename)?;
        if let Some(parent) = file_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&file_path, content)
            .map_err(|e| eyre!("Failed to write {}: {}", filename, e))?;
        files_written += 1;
        Step::verbose_substep(&format!("  Wrote {}", filename));
    }

    write_step.done_with_info(&format!("{} files", files_written));
    op.success();

    println!();
    crate::output::info(&format!(
        "Synced {} files from cluster '{}'",
        files_written, cluster_id
    ));
    crate::output::info(&format!("Files saved to: {}", queries_dir.display()));

    Ok(())
}

async fn run_project_sync_flow(project: &ProjectContext, assume_yes: bool) -> Result<()> {
    prompts::intro(
        "helix sync",
        Some(&format!(
            "Using project '{}' from helix.toml. Select a cluster to sync from.",
            project.config.project.name
        )),
    )?;

    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let base_url = cloud_base_url();

    let mut workspace_config = WorkspaceConfig::load()?;
    let workspace_id = resolve_workspace_id(
        &client,
        &base_url,
        &credentials.helix_admin_key,
        &mut workspace_config,
    )
    .await?;

    let resolved_project = resolve_or_create_project_for_sync(
        &client,
        &base_url,
        &credentials.helix_admin_key,
        &workspace_id,
        &project.config.project.name,
        &project.root,
    )
    .await?;

    let project_clusters = fetch_project_clusters(
        &client,
        &base_url,
        &credentials.helix_admin_key,
        &resolved_project.id,
    )
    .await?;

    if project_clusters.standard.is_empty() && project_clusters.enterprise.is_empty() {
        return Err(eyre!(
            "No clusters found in project '{}'. Create and deploy a cluster first.",
            resolved_project.name
        ));
    }

    let standard_items: Vec<(String, String, String)> = project_clusters
        .standard
        .iter()
        .map(|cluster| {
            (
                cluster.cluster_id.clone(),
                cluster.cluster_name.clone(),
                project_clusters.project_name.clone(),
            )
        })
        .collect();

    let enterprise_items: Vec<(String, String, String)> = project_clusters
        .enterprise
        .iter()
        .map(|cluster| {
            (
                cluster.cluster_id.clone(),
                cluster.cluster_name.clone(),
                project_clusters.project_name.clone(),
            )
        })
        .collect();

    let (cluster_id, is_enterprise) =
        prompts::select_cluster_from_workspace(&standard_items, &enterprise_items)?;

    if is_enterprise {
        sync_enterprise_from_cluster_id(&credentials.helix_admin_key, &cluster_id).await?;
    } else {
        sync_cluster_into_project(
            &credentials.helix_admin_key,
            &cluster_id,
            project,
            assume_yes,
        )
        .await?;
    }

    reconcile_project_config_from_cloud(project, &project_clusters)?;
    crate::output::info(
        "Updated helix.toml with canonical project and cluster metadata from Helix Cloud.",
    );

    Ok(())
}

pub async fn run(instance_name: Option<String>, assume_yes: bool) -> Result<()> {
    // Try to load project context
    let project = ProjectContext::find_and_load(None).ok();

    if let Some(instance_name) = instance_name {
        let project = project.ok_or_else(|| {
            eyre!("No helix.toml found. Run 'helix init' to create a project first.")
        })?;

        let instance_config = project.config.get_instance(&instance_name)?;
        if instance_config.is_local() {
            return pull_from_local_instance(&project, &instance_name).await;
        }

        return pull_from_cloud_instance(&project, &instance_name, instance_config, assume_yes)
            .await;
    }

    if !prompts::is_interactive() {
        return Err(eyre!(
            "No instance specified. Run 'helix sync <instance>' or run interactively in a project directory."
        ));
    }

    if let Some(ref project) = project {
        run_project_sync_flow(project, assume_yes).await
    } else {
        run_workspace_sync_flow().await
    }
}

/// Interactive flow when no project/instance is available: prompt workspace → cluster selection.
async fn run_workspace_sync_flow() -> Result<()> {
    prompts::intro(
        "helix sync",
        Some("No helix.toml found. Select a workspace and cluster to sync from."),
    )?;

    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let base_url = cloud_base_url();

    // Load or prompt for workspace
    let mut workspace_config = WorkspaceConfig::load()?;

    let workspace_id = resolve_workspace_id(
        &client,
        &base_url,
        &credentials.helix_admin_key,
        &mut workspace_config,
    )
    .await?;

    // Fetch clusters for workspace (both standard and enterprise)
    let workspace_clusters: CliWorkspaceClusters = client
        .get(format!(
            "{}/api/cli/workspaces/{}/clusters",
            base_url, workspace_id
        ))
        .header("x-api-key", &credentials.helix_admin_key)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch clusters: {}", e))?
        .error_for_status()
        .map_err(|e| eyre!("Failed to fetch clusters: {}", e))?
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse clusters response: {}", e))?;

    if workspace_clusters.standard.is_empty() && workspace_clusters.enterprise.is_empty() {
        return Err(eyre!(
            "No clusters found in this workspace. Deploy a cluster first with 'helix push'."
        ));
    }

    // Build prompt data
    let standard_items: Vec<(String, String, String)> = workspace_clusters
        .standard
        .iter()
        .map(|c| {
            (
                c.cluster_id.clone(),
                c.cluster_name.clone(),
                c.project_name.clone(),
            )
        })
        .collect();
    let enterprise_items: Vec<(String, String, String)> = workspace_clusters
        .enterprise
        .iter()
        .map(|c| {
            (
                c.cluster_id.clone(),
                c.cluster_name.clone(),
                c.project_name.clone(),
            )
        })
        .collect();

    let (cluster_id, is_enterprise) =
        prompts::select_cluster_from_workspace(&standard_items, &enterprise_items)?;

    if is_enterprise {
        // Enterprise sync
        sync_enterprise_from_cluster_id(&credentials.helix_admin_key, &cluster_id).await
    } else {
        // Standard sync
        sync_from_cluster_id(&credentials.helix_admin_key, &cluster_id).await
    }
}

/// Sync directly from a cluster ID without a project context.
async fn sync_from_cluster_id(api_key: &str, cluster_id: &str) -> Result<()> {
    let op = Operation::new("Syncing", cluster_id);

    let client = reqwest::Client::new();
    let sync_url = format!("{}/api/cli/clusters/{}/sync", cloud_base_url(), cluster_id);

    let mut sync_step = Step::with_messages("Fetching source files", "Source files fetched");
    sync_step.start();

    let response = match client
        .get(&sync_url)
        .header("x-api-key", api_key)
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

    let sync_response: SyncResponse = match response.json().await {
        Ok(resp) => resp,
        Err(e) => {
            sync_step.fail();
            op.failure();
            return Err(eyre!("Failed to parse sync response: {}", e));
        }
    };

    sync_step.done();

    // Write files to current directory
    let cwd = std::env::current_dir()?;
    let remote_config = sync_response
        .helix_toml
        .as_deref()
        .and_then(|remote_toml| parse_and_sanitize_remote_config(remote_toml, "cluster sync"));
    let queries_dir = resolve_remote_queries_dir(&cwd, remote_config.as_ref());

    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }

    let mut write_step = Step::with_messages("Writing source files", "Source files written");
    write_step.start();

    let mut files_written = 0;
    for (filename, content) in &sync_response.hx_files {
        let file_path = safe_join_relative(&queries_dir, filename)?;
        if let Some(parent) = file_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, content)
            .map_err(|e| eyre!("Failed to write {}: {}", filename, e))?;
        files_written += 1;
        Step::verbose_substep(&format!("  Wrote {}", filename));
    }

    if let Some(remote_config) = remote_config.as_ref()
        && let Some(remote_toml) = serialize_remote_config(remote_config, "cluster sync")
    {
        let helix_toml_path = cwd.join("helix.toml");
        std::fs::write(&helix_toml_path, remote_toml)
            .map_err(|e| eyre!("Failed to write helix.toml: {}", e))?;
        files_written += 1;
        Step::verbose_substep("  Wrote helix.toml");
    }

    write_step.done_with_info(&format!("{} files", files_written));
    op.success();

    println!();
    crate::output::info(&format!(
        "Synced {} files from cluster '{}'",
        files_written, cluster_id
    ));
    crate::output::info(&format!("Files saved to: {}", queries_dir.display()));

    Ok(())
}

/// Sync .rs files from an enterprise cluster by ID (no project context).
async fn sync_enterprise_from_cluster_id(api_key: &str, cluster_id: &str) -> Result<()> {
    let op = Operation::new("Syncing", cluster_id);

    let client = reqwest::Client::new();
    let sync_url = format!(
        "{}/api/cli/enterprise-clusters/{}/sync",
        cloud_base_url(),
        cluster_id
    );

    let mut sync_step = Step::with_messages("Fetching .rs files", ".rs files fetched");
    sync_step.start();

    let response = match client
        .get(&sync_url)
        .header("x-api-key", api_key)
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

    match response.status() {
        reqwest::StatusCode::OK => {}
        status => {
            let error_text = response.text().await.unwrap_or_default();
            sync_step.fail();
            op.failure();
            return Err(eyre!("Enterprise sync failed ({}): {}", status, error_text));
        }
    }

    let sync_response: EnterpriseSyncResponse = match response.json().await {
        Ok(resp) => resp,
        Err(e) => {
            sync_step.fail();
            op.failure();
            return Err(eyre!("Failed to parse sync response: {}", e));
        }
    };

    sync_step.done();

    let cwd = std::env::current_dir()?;
    let remote_config = sync_response.helix_toml.as_deref().and_then(|remote_toml| {
        parse_and_sanitize_remote_config(remote_toml, "enterprise cluster sync")
    });
    let queries_dir = resolve_remote_queries_dir(&cwd, remote_config.as_ref());

    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }

    let mut write_step = Step::with_messages("Writing .rs files", ".rs files written");
    write_step.start();

    let mut files_written = 0;
    for (filename, content) in &sync_response.rs_files {
        let file_path = safe_join_relative(&queries_dir, filename)?;
        if let Some(parent) = file_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, content)
            .map_err(|e| eyre!("Failed to write {}: {}", filename, e))?;
        files_written += 1;
        Step::verbose_substep(&format!("  Wrote {}", filename));
    }

    if let Some(remote_config) = remote_config.as_ref()
        && let Some(remote_toml) = serialize_remote_config(remote_config, "enterprise cluster sync")
    {
        let helix_toml_path = cwd.join("helix.toml");
        std::fs::write(&helix_toml_path, remote_toml)
            .map_err(|e| eyre!("Failed to write helix.toml: {}", e))?;
        files_written += 1;
        Step::verbose_substep("  Wrote helix.toml");
    }

    write_step.done_with_info(&format!("{} files", files_written));
    op.success();

    crate::output::info(&format!(
        "Synced {} .rs files from enterprise cluster '{}'",
        files_written, cluster_id
    ));

    Ok(())
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
    assume_yes: bool,
) -> Result<()> {
    let op = Operation::new("Syncing", instance_name);

    // Handle enterprise instances separately
    if let InstanceInfo::Enterprise(config) = &instance_config {
        return pull_from_enterprise_instance(project, instance_name, config, assume_yes).await;
    }

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
        InstanceInfo::Local(_) | InstanceInfo::Enterprise(_) => {
            op.failure();
            return Err(eyre!("Sync is only supported for cloud instances"));
        }
    };

    // Check auth
    let credentials = require_auth().await?;

    Step::verbose_substep(&format!("Downloading from cluster: {cluster_id}"));

    // Make API request to sync endpoint
    let client = reqwest::Client::new();
    let sync_url = format!("{}/api/cli/clusters/{}/sync", cloud_base_url(), cluster_id);

    let mut sync_step = Step::with_messages("Fetching source files", "Source files fetched");
    sync_step.start();

    let response = match client
        .get(&sync_url)
        .header("x-api-key", &credentials.helix_admin_key)
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

    let cluster_project = fetch_cluster_project(
        &client,
        &cloud_base_url(),
        &credentials.helix_admin_key,
        cluster_id,
    )
    .await?;
    let project_clusters = fetch_project_clusters(
        &client,
        &cloud_base_url(),
        &credentials.helix_admin_key,
        &cluster_project.project_id,
    )
    .await?;

    // Get the queries directory from project config
    let queries_dir = project.root.join(&project.config.project.queries);

    // Create queries directory if it doesn't exist
    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }

    // Collect files that differ from local
    let mut differing_files: Vec<String> = Vec::new();
    for (filename, content) in &sync_response.hx_files {
        let file_path = safe_join_relative(&queries_dir, filename)?;
        if file_path.exists()
            && let Ok(local_content) = std::fs::read_to_string(&file_path)
            && local_content != *content
        {
            differing_files.push(filename.clone());
        }
    }

    if let Err(e) = confirm_overwrite_or_abort(
        &differing_files,
        assume_yes,
        "Overwrite local files that differ from remote?",
    ) {
        op.failure();
        return Err(e);
    }

    // Write .hx files
    let mut write_step = Step::with_messages("Writing source files", "Source files written");
    write_step.start();

    let mut files_written = 0;
    for (filename, content) in &sync_response.hx_files {
        let file_path = safe_join_relative(&queries_dir, filename)?;

        // Create parent directories if needed
        if let Some(parent) = file_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&file_path, content)
            .map_err(|e| eyre!("Failed to write {}: {}", filename, e))?;

        files_written += 1;
        Step::verbose_substep(&format!("  Wrote {}", filename));
    }

    reconcile_project_config_from_cloud(project, &project_clusters)?;
    files_written += 1;
    Step::verbose_substep("  Wrote helix.toml (canonical cloud metadata)");

    write_step.done_with_info(&format!("{} files", files_written));

    op.success();

    // Print summary
    println!();
    crate::output::info(&format!(
        "Synced {} files from cluster '{}'",
        files_written, cluster_id
    ));
    crate::output::info(&format!("Files saved to: {}", queries_dir.display()));

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

async fn pull_from_enterprise_instance(
    project: &ProjectContext,
    instance_name: &str,
    config: &crate::config::EnterpriseInstanceConfig,
    _assume_yes: bool,
) -> Result<()> {
    let op = Operation::new("Syncing", instance_name);
    let credentials = require_auth().await?;

    Step::verbose_substep(&format!(
        "Downloading .rs files from enterprise cluster: {}",
        config.cluster_id
    ));

    let client = reqwest::Client::new();
    let sync_url = format!(
        "{}/api/cli/enterprise-clusters/{}/sync",
        cloud_base_url(),
        config.cluster_id
    );

    let mut sync_step = Step::with_messages("Fetching source files", "Source files fetched");
    sync_step.start();

    let response = match client
        .get(&sync_url)
        .header("x-api-key", &credentials.helix_admin_key)
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

    match response.status() {
        reqwest::StatusCode::OK => {}
        status => {
            let error_text = response.text().await.unwrap_or_default();
            sync_step.fail();
            op.failure();
            return Err(eyre!("Enterprise sync failed ({}): {}", status, error_text));
        }
    }

    let sync_response: EnterpriseSyncResponse = match response.json().await {
        Ok(resp) => resp,
        Err(e) => {
            sync_step.fail();
            op.failure();
            return Err(eyre!("Failed to parse sync response: {}", e));
        }
    };

    sync_step.done();

    let remote_config = sync_response.helix_toml.as_deref().and_then(|remote_toml| {
        parse_and_sanitize_remote_config(remote_toml, "enterprise instance sync")
    });

    let queries_dir = project.root.join(&project.config.project.queries);
    if !queries_dir.exists() {
        std::fs::create_dir_all(&queries_dir)?;
    }

    let mut write_step = Step::with_messages("Writing source files", "Source files written");
    write_step.start();

    let mut files_written = 0;
    for (filename, content) in &sync_response.rs_files {
        let file_path = safe_join_relative(&queries_dir, filename)?;
        if let Some(parent) = file_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, content)
            .map_err(|e| eyre!("Failed to write {}: {}", filename, e))?;
        files_written += 1;
        Step::verbose_substep(&format!("  Wrote {}", filename));
    }

    if let Some(remote_config) = remote_config.as_ref()
        && let Some(remote_toml) =
            serialize_remote_config(remote_config, "enterprise instance sync")
    {
        let helix_toml_path = project.root.join("helix.toml");
        std::fs::write(&helix_toml_path, remote_toml)
            .map_err(|e| eyre!("Failed to write helix.toml: {}", e))?;
        files_written += 1;
        Step::verbose_substep("  Wrote helix.toml");
    }

    write_step.done_with_info(&format!("{} files", files_written));
    op.success();

    crate::output::info(&format!(
        "Synced {} .rs files from enterprise cluster '{}'",
        files_written, config.cluster_id
    ));

    Ok(())
}
