use crate::commands::auth::require_auth;
use crate::config::WorkspaceConfig;
use crate::enterprise_cloud::{
    CliEnterpriseCluster, cloud_base_url, fetch_project_clusters, fetch_projects,
    fetch_workspace_clusters, fetch_workspaces, find_project_by_id, find_project_by_name,
    find_workspace_by_id, find_workspace_by_slug,
};
use crate::project::ProjectContext;
use crate::prompts;
use crate::{
    ClusterConfigAction, ConfigAction, ConfigOutputFormat, ProjectConfigAction,
    WorkspaceConfigAction,
};
use color_eyre::owo_colors::OwoColorize;
use eyre::{Result, eyre};
use serde::Serialize;

pub async fn run(action: Option<ConfigAction>) -> Result<()> {
    match action {
        Some(ConfigAction::Workspace { action }) => run_workspace(Some(action)).await,
        Some(ConfigAction::Project { action }) => run_project(Some(action)).await,
        Some(ConfigAction::Cluster { action }) => run_cluster(Some(action)).await,
        None if prompts::is_interactive() => interactive_config().await,
        None => Err(eyre!(
            "Specify a config command: 'helix workspace', 'helix project', or 'helix cluster'"
        )),
    }
}

pub async fn run_workspace(action: Option<WorkspaceConfigAction>) -> Result<()> {
    match action {
        Some(WorkspaceConfigAction::List { format }) => workspace_list(format).await,
        Some(WorkspaceConfigAction::Show { format }) => workspace_show(format).await,
        Some(WorkspaceConfigAction::Switch { workspace, id }) => {
            workspace_switch(&workspace, id).await
        }
        None if prompts::is_interactive() => workspace_select().await,
        None => Err(eyre!(
            "Specify a workspace command: 'helix workspace list', 'helix workspace show', or 'helix workspace switch <workspace>'"
        )),
    }
}

pub async fn run_project(action: Option<ProjectConfigAction>) -> Result<()> {
    match action {
        Some(ProjectConfigAction::List {
            workspace_id,
            format,
        }) => project_list(workspace_id, format).await,
        Some(ProjectConfigAction::Show { format }) => project_show(format).await,
        Some(ProjectConfigAction::Switch { project, id }) => project_switch(&project, id).await,
        None if prompts::is_interactive() => project_select().await,
        None => Err(eyre!(
            "Specify a project command: 'helix project list', 'helix project show', or 'helix project switch <project>'"
        )),
    }
}

pub async fn run_cluster(action: Option<ClusterConfigAction>) -> Result<()> {
    match action {
        Some(ClusterConfigAction::List {
            workspace_id,
            project_id,
            format,
        }) => cluster_list(workspace_id, project_id, format).await,
        None if prompts::is_interactive() => cluster_select().await,
        None => Err(eyre!("Specify a cluster command: 'helix cluster list'")),
    }
}

async fn interactive_config() -> Result<()> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum ConfigTarget {
        Workspace,
        Project,
        Cluster,
    }

    let target = cliclack::select("What would you like to configure?")
        .item(
            ConfigTarget::Workspace,
            "Workspace",
            "Choose active Enterprise Cloud workspace",
        )
        .item(
            ConfigTarget::Project,
            "Project",
            "Link this project to Enterprise Cloud",
        )
        .item(
            ConfigTarget::Cluster,
            "Cluster",
            "Inspect Enterprise Cloud clusters",
        )
        .interact()?;

    match target {
        ConfigTarget::Workspace => workspace_select().await,
        ConfigTarget::Project => project_select().await,
        ConfigTarget::Cluster => cluster_select().await,
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

async fn workspace_list(format: ConfigOutputFormat) -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let workspaces =
        fetch_workspaces(&client, &cloud_base_url(), &credentials.helix_admin_key).await?;
    if format == ConfigOutputFormat::Json {
        return print_json(&workspaces);
    }
    println!("{}", "Workspaces".bold());
    for workspace in workspaces {
        println!("  {} ({})", workspace.name, workspace.url_slug);
    }
    Ok(())
}

async fn workspace_show(format: ConfigOutputFormat) -> Result<()> {
    let config = WorkspaceConfig::load()?;
    if format == ConfigOutputFormat::Json {
        return print_json(&config);
    }
    match config.workspace_id {
        Some(id) => println!("Selected workspace: {id}"),
        None => println!("No workspace selected"),
    }
    Ok(())
}

async fn workspace_switch(selector: &str, use_id: bool) -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let workspaces =
        fetch_workspaces(&client, &cloud_base_url(), &credentials.helix_admin_key).await?;
    let selected = if use_id {
        find_workspace_by_id(&workspaces, selector)
    } else {
        find_workspace_by_slug(&workspaces, selector)
    }
    .ok_or_else(|| eyre!("Workspace '{selector}' was not found"))?;

    let config = WorkspaceConfig {
        workspace_id: Some(selected.id.clone()),
    };
    config.save()?;
    crate::output::success(&format!("Selected workspace '{}'", selected.name));
    Ok(())
}

async fn workspace_select() -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let workspaces =
        fetch_workspaces(&client, &cloud_base_url(), &credentials.helix_admin_key).await?;
    let items: Vec<(String, String, String)> = workspaces
        .iter()
        .map(|workspace| {
            (
                workspace.id.clone(),
                workspace.name.clone(),
                workspace.url_slug.clone(),
            )
        })
        .collect();
    let selected_id = prompts::select_workspace(&items)?;
    let selected = workspaces
        .iter()
        .find(|workspace| workspace.id == selected_id)
        .ok_or_else(|| eyre!("Selected workspace was not found"))?;
    WorkspaceConfig {
        workspace_id: Some(selected.id.clone()),
    }
    .save()?;
    crate::output::success(&format!("Selected workspace '{}'", selected.name));
    Ok(())
}

async fn project_list(workspace_id: Option<String>, format: ConfigOutputFormat) -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let workspace_id = workspace_id
        .or_else(|| {
            WorkspaceConfig::load()
                .ok()
                .and_then(|config| config.workspace_id)
        })
        .ok_or_else(|| {
            eyre!("No workspace selected. Run 'helix config workspace switch <workspace>'.")
        })?;
    let projects = fetch_projects(
        &client,
        &cloud_base_url(),
        &credentials.helix_admin_key,
        &workspace_id,
    )
    .await?;
    if format == ConfigOutputFormat::Json {
        return print_json(&projects);
    }
    println!("{}", "Projects".bold());
    for project in projects {
        println!("  {} ({})", project.name, project.id);
    }
    Ok(())
}

async fn project_show(format: ConfigOutputFormat) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    if format == ConfigOutputFormat::Json {
        return print_json(&project.config.project);
    }
    println!("Project: {}", project.config.project.name);
    if let Some(id) = &project.config.project.id {
        println!("ID: {id}");
    }
    if let Some(workspace_id) = &project.config.project.workspace_id {
        println!("Workspace ID: {workspace_id}");
    }
    Ok(())
}

async fn project_switch(selector: &str, use_id: bool) -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let workspace_id = WorkspaceConfig::load()?.workspace_id.ok_or_else(|| {
        eyre!("No workspace selected. Run 'helix config workspace switch <workspace>'.")
    })?;
    let projects = fetch_projects(
        &client,
        &cloud_base_url(),
        &credentials.helix_admin_key,
        &workspace_id,
    )
    .await?;
    let selected = if use_id {
        find_project_by_id(&projects, selector)
    } else {
        find_project_by_name(&projects, selector)
    }
    .ok_or_else(|| eyre!("Project '{selector}' was not found"))?;

    let mut project = ProjectContext::find_and_load(None)?;
    project.config.project.id = Some(selected.id.clone());
    project.config.project.workspace_id = Some(workspace_id);
    project.config.project.name = selected.name.clone();
    project
        .config
        .save_to_file(&project.root.join("helix.toml"))?;
    crate::output::success(&format!("Linked project '{}'", selected.name));
    Ok(())
}

async fn project_select() -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let workspace_id = WorkspaceConfig::load()?.workspace_id.ok_or_else(|| {
        eyre!(
            "No workspace selected. Run 'helix workspace' or 'helix workspace switch <workspace>'."
        )
    })?;
    let projects = fetch_projects(
        &client,
        &cloud_base_url(),
        &credentials.helix_admin_key,
        &workspace_id,
    )
    .await?;
    let items: Vec<(String, String)> = projects
        .iter()
        .map(|project| (project.id.clone(), project.name.clone()))
        .collect();
    let selected_id = prompts::select_project(&items)?;
    let selected = projects
        .iter()
        .find(|project| project.id == selected_id)
        .ok_or_else(|| eyre!("Selected project was not found"))?;

    let mut project = ProjectContext::find_and_load(None)?;
    project.config.project.id = Some(selected.id.clone());
    project.config.project.workspace_id = Some(workspace_id);
    project.config.project.name = selected.name.clone();
    project
        .config
        .save_to_file(&project.root.join("helix.toml"))?;
    crate::output::success(&format!("Linked project '{}'", selected.name));
    Ok(())
}

async fn cluster_list(
    workspace_id: Option<String>,
    project_id: Option<String>,
    format: ConfigOutputFormat,
) -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let clusters = if let Some(project_id) = project_id {
        fetch_project_clusters(
            &client,
            &cloud_base_url(),
            &credentials.helix_admin_key,
            &project_id,
        )
        .await?
        .enterprise
    } else {
        let workspace_id = workspace_id
            .or_else(|| {
                WorkspaceConfig::load()
                    .ok()
                    .and_then(|config| config.workspace_id)
            })
            .ok_or_else(|| {
                eyre!("No workspace selected. Run 'helix config workspace switch <workspace>'.")
            })?;
        fetch_workspace_clusters(
            &client,
            &cloud_base_url(),
            &credentials.helix_admin_key,
            &workspace_id,
        )
        .await?
        .enterprise
    };

    if format == ConfigOutputFormat::Json {
        return print_json(&clusters);
    }
    print_enterprise_clusters(&clusters);
    Ok(())
}

fn print_enterprise_clusters(clusters: &[CliEnterpriseCluster]) {
    println!("{}", "Enterprise clusters".bold());
    for cluster in clusters {
        println!("  {} ({})", cluster.name, cluster.cluster_id);
        if let Some(gateway_url) = &cluster.gateway_url {
            println!("    gateway: {gateway_url}");
        }
    }
}

async fn cluster_select() -> Result<()> {
    let credentials = require_auth().await?;
    let client = reqwest::Client::new();
    let project_context = ProjectContext::find_and_load(None).ok();
    let clusters = if let Some(project_id) = project_context
        .as_ref()
        .and_then(|project| project.config.project.id.as_deref())
    {
        fetch_project_clusters(
            &client,
            &cloud_base_url(),
            &credentials.helix_admin_key,
            project_id,
        )
        .await?
        .enterprise
    } else {
        let workspace_id = WorkspaceConfig::load()?.workspace_id.ok_or_else(|| {
            eyre!("No workspace selected. Run 'helix workspace' or 'helix workspace switch <workspace>'.")
        })?;
        fetch_workspace_clusters(
            &client,
            &cloud_base_url(),
            &credentials.helix_admin_key,
            &workspace_id,
        )
        .await?
        .enterprise
    };

    let items: Vec<(String, String, String)> = clusters
        .iter()
        .map(|cluster| {
            let hint = cluster
                .project_name
                .as_deref()
                .unwrap_or("Enterprise cluster")
                .to_string();
            (cluster.cluster_id.clone(), cluster.name.clone(), hint)
        })
        .collect();
    let selected_id = prompts::select_cluster(&items)?;
    let selected = clusters
        .iter()
        .find(|cluster| cluster.cluster_id == selected_id)
        .ok_or_else(|| eyre!("Selected Enterprise cluster was not found"))?;

    println!("{}", "Enterprise cluster".bold());
    println!("  Name: {}", selected.name);
    println!("  ID: {}", selected.cluster_id);
    if let Some(project_name) = &selected.project_name {
        println!("  Project: {project_name}");
    }
    if let Some(gateway_url) = &selected.gateway_url {
        println!("  Gateway: {gateway_url}");
    }
    Ok(())
}
