use std::env;
use std::fs::DirEntry;
use std::path::Path;
use std::{fs, path::PathBuf};

use dotenvy::from_filename;
use eyre::{Result, eyre};
use helix_db::helix_engine::traversal_core::config::Config;
use helix_db::utils::styled_string::StyledString;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use toml::toml;

use crate::commands::build::{collect_hx_files, generate_content};
use crate::project::ProjectContext;

pub struct HelixManager<'a> {
    project: &'a ProjectContext,
}

#[derive(Deserialize, Debug)]
pub struct Credentials {
    #[serde(rename = "HELIX_USER_ID")]
    user_id: String,
    #[serde(rename = "HELIX_USER_KEY")]
    helix_admin_key: String,
}

impl From<PathBuf> for Credentials {
    fn from(path: PathBuf) -> Self {
        let content = fs::read_to_string(path).unwrap();
        toml::from_str(&content).unwrap()
    }
}

impl Credentials {
    fn is_authenticated(&self) -> bool {
        !self.user_id.is_empty() && !self.helix_admin_key.is_empty()
    }
}

impl<'a> HelixManager<'a> {
    pub fn new(project: &'a ProjectContext) -> Self {
        Self { project }
    }

    fn cluster_id(id: &str) -> String {
        format!("{id}")
    }

    fn credentials_path(&self) -> PathBuf {
        self.project.helix_dir.join("credentials")
    }

    pub fn check_auth(&self) -> Result<()> {
        let credentials_path = self.credentials_path();
        if !credentials_path.exists() {
            return Err(eyre!("Credentials file not found"));
        }

        let credentials = Credentials::from(credentials_path);
        if !credentials.is_authenticated() {
            return Err(eyre!("Credentials file is not authenticated"));
        }

        Ok(())
    }

    fn deploy(&self, path: Option<String>) -> Result<()> {
        let path = match get_path_or_cwd(path.as_ref()) {
            Ok(path) => path,
            Err(e) => {
                println!("{}", "Error: failed to get path".red().bold());
                println!("└── {e}");
                return Err(eyre!("Error: failed to get path"));
            }
        };
        let files = collect_hx_files(&path).unwrap_or_default();

        let content = match generate_content(&files) {
            Ok(content) => content,
            Err(e) => {
                println!("{}", "Error generating content".red().bold());
                println!("└── {e}");
                return Err(eyre!("Error: failed to generate content"));
            }
        };

        // get config from ~/.helix/credentials
        let home_dir = std::env::var("HOME").unwrap_or("~/".to_string());
        let config_path = &format!("{home_dir}/.helix");
        let config_path = Path::new(config_path);
        let config_path = config_path.join("credentials");
        if !config_path.exists() {
            println!("{}", "No credentials found".yellow().bold());
            println!(
                "{}",
                "Please run `helix config` to set your credentials"
                    .yellow()
                    .bold()
            );
            return Err(eyre!("Error: no credentials found"));
        }

        // TODO: probable could make this more secure
        // reads credentials from ~/.helix/credentials
        let config = fs::read_to_string(config_path).unwrap();
        let user_id = config
            .split("helix_user_id=")
            .nth(1)
            .unwrap()
            .split("\n")
            .next()
            .unwrap();
        let user_key = config
            .split("helix_user_key=")
            .nth(1)
            .unwrap()
            .split("\n")
            .next()
            .unwrap();

        // read config.hx.json
        let config = match Config::from_files(
            PathBuf::from(path.clone()).join("config.hx.json"),
            PathBuf::from(path.clone()).join("schema.hx"),
        ) {
            Ok(config) => config,
            Err(e) => {
                println!("Error loading config: {e}");
                println!("{}", "Error loading config".red().bold());
                return Err(eyre!("Error: failed to load config"));
            }
        };

        let deployment = DeployCloudEvent {
            cluster_id: cluster_id,
            queries_string: content
                .source
                .queries
                .iter()
                .map(|q| q.name.clone())
                .collect::<Vec<String>>()
                .join("\n"),
            num_of_queries: content.source.queries.len() as u32,
            time_taken_sec: 0,
            success: true,
            error_messages: None,
        };

        // upload queries to central server
        let payload = json!({
            "user_id": user_id,
            "queries": content.files,
            "cluster_id": cluster,
            "version": "0.1.0",
            "helix_config": config.to_json()
        });
        let client = reqwest::Client::new();

        let cloud_url = format!("http://{}/clusters/deploy-queries", *CLOUD_AUTHORITY);

        match client
            .post(cloud_url)
            .header("x-api-key", user_key) // used to verify user
            .header("x-cluster-id", &cluster) // used to verify instance with user
            .header("Content-Type", "application/json")
            .body(sonic_rs::to_string(&payload).unwrap())
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    sp.stop_with_message(
                        "Queries uploaded to remote db".green().bold().to_string(),
                    );
                    HELIX_METRICS_CLIENT.send_event(EventType::DeployCloud, deployment);
                } else {
                    sp.stop_with_message(
                        "Error uploading queries to remote db"
                            .red()
                            .bold()
                            .to_string(),
                    );
                    println!("└── {}", response.text().await.unwrap());
                    return Err("".to_string());
                }
            }
            Err(e) => {
                sp.stop_with_message(
                    "Error uploading queries to remote db"
                        .red()
                        .bold()
                        .to_string(),
                );
                println!("└── {e}");
                return Err("".to_string());
            }
        };
    }
}

/// Returns the path or the current working directory if no path is provided
pub fn get_path_or_cwd(path: Option<&String>) -> Result<PathBuf> {
    match path {
        Some(p) => Ok(PathBuf::from(p)),
        None => Ok(env::current_dir()?),
    }
}
