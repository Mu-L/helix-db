use crate::commands::auth::require_auth;
use crate::config::{DEFAULT_QUERY_AUTH_ENV, DEFAULT_QUERY_AUTH_HEADER};
use crate::enterprise_cloud::{
    cloud_base_url, fetch_project_clusters, fetch_workspace_clusters, find_enterprise_cluster_by_id,
};
use crate::project::ProjectContext;
use eyre::Result;

pub async fn run(instance: Option<String>) -> Result<()> {
    let credentials = require_auth().await?;
    let mut project = ProjectContext::find_and_load(None)?;
    let client = reqwest::Client::new();
    let names: Vec<String> = match instance {
        Some(instance) => vec![instance],
        None => project.config.enterprise.keys().cloned().collect(),
    };

    for name in names {
        let Some(config) = project.config.enterprise.get_mut(&name) else {
            return Err(eyre::eyre!("Enterprise instance '{name}' not found"));
        };

        let remote = if let Some(project_id) = &config.project_id {
            let clusters = fetch_project_clusters(
                &client,
                &cloud_base_url(),
                &credentials.helix_admin_key,
                project_id,
            )
            .await?;
            find_enterprise_cluster_by_id(&clusters.enterprise, &config.cluster_id).cloned()
        } else if let Some(workspace_id) = &config.workspace_id {
            let clusters = fetch_workspace_clusters(
                &client,
                &cloud_base_url(),
                &credentials.helix_admin_key,
                workspace_id,
            )
            .await?;
            find_enterprise_cluster_by_id(&clusters.enterprise, &config.cluster_id).cloned()
        } else {
            None
        };

        if let Some(remote) = remote {
            config.gateway_url = remote.gateway_url.or_else(|| config.gateway_url.clone());
            config.query_auth_header = remote
                .query_auth_header
                .unwrap_or_else(|| DEFAULT_QUERY_AUTH_HEADER.to_string());
            config.query_auth_env = remote
                .query_auth_env
                .unwrap_or_else(|| DEFAULT_QUERY_AUTH_ENV.to_string());
            config.availability_mode = remote.availability_mode;
            config.gateway_node_type = remote.gateway_node_type;
            config.db_node_type = remote.db_node_type;
        }

        if config.gateway_url.is_none() {
            crate::output::warning(&format!(
                "Enterprise instance '{name}' is synced, but gateway_url is still missing. Set it in helix.toml before using 'helix query {name}'."
            ));
        } else {
            crate::output::success(&format!("Synced Enterprise instance '{name}'"));
        }
    }

    project
        .config
        .save_to_file(&project.root.join("helix.toml"))?;
    Ok(())
}
