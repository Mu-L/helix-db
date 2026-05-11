use crate::InitTarget;
use crate::commands::auth::require_auth;
use crate::config::{
    DEFAULT_QUERY_AUTH_ENV, DEFAULT_QUERY_AUTH_HEADER, EnterpriseInstanceConfig, HelixConfig,
    LocalInstanceConfig,
};
use crate::output::Operation;
use crate::utils::print_instructions;
use eyre::Result;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

pub async fn run(path: Option<String>, target: Option<InitTarget>) -> Result<()> {
    let project_dir = match path {
        Some(path) => std::path::PathBuf::from(path),
        None => env::current_dir()?,
    };
    let project_name = project_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("helix-project")
        .to_string();
    let config_path = project_dir.join("helix.toml");

    if config_path.exists() {
        return Err(eyre::eyre!(
            "helix.toml already exists in {}",
            project_dir.display()
        ));
    }

    fs::create_dir_all(&project_dir)?;
    fs::create_dir_all(project_dir.join(".helix"))?;

    let op = Operation::new("Initializing", &project_name);
    let mut config = HelixConfig::default_config(&project_name);

    match target.unwrap_or(InitTarget::Local {
        name: "dev".to_string(),
        port: crate::config::DEFAULT_LOCAL_PORT,
    }) {
        InitTarget::Local { name, port } => {
            config.local.clear();
            config.local.insert(
                name,
                LocalInstanceConfig {
                    port,
                    ..LocalInstanceConfig::default()
                },
            );
            write_example_request(&project_dir)?;
        }
        InitTarget::Enterprise {
            name,
            cluster_id,
            gateway_url,
        } => {
            require_auth().await?;
            config.local.clear();
            config.enterprise.insert(
                name,
                EnterpriseInstanceConfig {
                    cluster_id,
                    workspace_id: None,
                    project_id: None,
                    gateway_url,
                    query_auth_header: DEFAULT_QUERY_AUTH_HEADER.to_string(),
                    query_auth_env: DEFAULT_QUERY_AUTH_ENV.to_string(),
                    availability_mode: None,
                    gateway_node_type: None,
                    db_node_type: None,
                },
            );
        }
    }

    config.save_to_file(&config_path)?;
    append_gitignore(&project_dir)?;
    op.success();

    print_instructions(
        "Next steps:",
        &[
            "Run 'helix run dev' to start local Helix Enterprise dev",
            "Create or edit a dynamic query JSON request",
            "Run 'helix query dev --file examples/request.json'",
        ],
    );

    Ok(())
}

fn write_example_request(project_dir: &Path) -> Result<()> {
    let examples_dir = project_dir.join("examples");
    fs::create_dir_all(&examples_dir)?;
    let request_path = examples_dir.join("request.json");
    if request_path.exists() {
        return Ok(());
    }

    let request = serde_json::json!({
        "request_type": "read",
        "query": {
            "queries": [{
                "Query": {
                    "name": "node_count",
                    "steps": ["Count"],
                    "condition": null
                }
            }],
            "returns": ["node_count"]
        },
        "parameters": {}
    });

    fs::write(&request_path, serde_json::to_string_pretty(&request)?)?;
    Ok(())
}

fn append_gitignore(project_dir: &Path) -> Result<()> {
    let gitignore_path = project_dir.join(".gitignore");
    let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();
    let entries = [".helix/", "target/", "*.log"];
    let missing: Vec<&str> = entries
        .into_iter()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore_path)?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        writeln!(file)?;
    }
    for entry in missing {
        writeln!(file, "{entry}")?;
    }
    Ok(())
}
