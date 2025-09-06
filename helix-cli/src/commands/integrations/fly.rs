use std::{default, process::Stdio, sync::LazyLock};

use async_trait::async_trait;
use eyre::Result;
use serde::Serialize;
use serde_json::json;
use tokio::{io::AsyncWriteExt, process::Command};
use crate::{docker::DockerManager, project::ProjectContext};

static FLY_MACHINES_API_URL: &str = "https://api.machines.dev/v1/";
static FLY_REGISTRY_URL: &str = "registry.fly.io";

/**
1. check fly auth via cli or check keys in /.helix/helix.env
2. create fly app with given name or name of project root (like how cargo init works)
3. write information to helix.toml
4. build docker container like normal
5. push to fly's registry "registry.fly.io/name:tag
6. deploy image "flyctl deploy --image <reg_url:tag>"
 */
pub enum FlyAuthentication {
    ApiKey(String),
    Cli,
}

pub enum FlyClient {
    ApiClient(reqwest::Client),
    Cli,
}

pub struct FlyIoClient {
    pub authentication: FlyAuthentication,
    pub client: FlyClient,
}

#[derive(Default)]
pub enum FlyIoClientSetup {
    ApiKey,
    #[default]
    Cli,
}

pub struct FlyIoInstanceConfig {
    vm_cpus: Option<u16>,
    vm_cpu_kind: Option<u16>,
    vm_size: Option<u16>,
    volume: Option<String>,
    volume_initial_size: Option<u16>,
    no_public_ip: bool,
}



impl FlyIoClient {
    pub async fn new(project: &ProjectContext, setup: Option<FlyIoClientSetup>) -> Result<Self> {
        let (authentication, client) = match setup.unwrap_or_default() {
            FlyIoClientSetup::ApiKey => {
                let env = match std::fs::read_to_string(project.helix_dir.join("helix.env")) {
                    Ok(env) => env,
                    Err(_) => {
                        return Err(eyre::eyre!(
                            "File {}/helix.env not found",
                            project.helix_dir.display()
                        ));
                    }
                };
                // parse env by reading lines and checking for FLY_API_KEY
                let api_key = env
                    .lines()
                    .find(|line| line.starts_with("FLY_API_KEY="))
                    .map(|line| line.splitn(2, "=").nth(1).unwrap_or_default().to_string()); // TODO: handle error
                match api_key {
                    Some(api_key) => (
                        FlyAuthentication::ApiKey(api_key),
                        FlyClient::ApiClient(reqwest::Client::new()),
                    ),
                    None => {
                        return Err(eyre::eyre!(
                            "No api key found in {}/helix.env",
                            project.helix_dir.display()
                        ));
                    }
                }
            }

            FlyIoClientSetup::Cli => match authenticate_flyio_via_cli().await {
                Ok(()) => (FlyAuthentication::Cli, FlyClient::Cli),
                Err(e) => return Err(e),
            },
        };
        Ok(Self {
            authentication,
            client,
        })
    }

    fn api_key(&self) -> &str {
        match &self.authentication {
            FlyAuthentication::ApiKey(api_key) => api_key,
            FlyAuthentication::Cli => unreachable!(),
        }
    }

    async fn init_flyio(&self, project: &ProjectContext, app_name: &str) -> Result<()> {
        // create app
        match &self.client {
            FlyClient::ApiClient(client) => {
                let request = json!({
                    "app_name": app_name,
                    "org_slug": "default",
                    "network": "default",
                });
                client
                    .post(format!("{}/apps", FLY_MACHINES_API_URL))
                    .header("Authorization", format!("Bearer {}", self.api_key()))
                    .json(&request)
                    .send()
                    .await?;
            }
            FlyClient::Cli => {
                Command::new("flyctl")
                    .arg("apps")
                    .arg("create")
                    .arg(app_name)
                    .spawn()?;
                Command::new("flyctl")
                    .arg("launch")
                    .arg("--no-deploy")
                    .arg("--path")
                    .arg(project.helix_dir.display().to_string())
                    .spawn()?;
            }
        }
        // write app name to helix.toml

        Ok(())
    }

    async fn deploy_flyio(
        &self,
        docker: &DockerManager<'_>,
        image_name: &str,
        image_tag: &str,
    ) -> Result<()> {
        match &self.client {
            FlyClient::ApiClient(client) => {
                todo!()
            }
            FlyClient::Cli => {
                docker.tag(image_name, image_tag, FLY_REGISTRY_URL).await?;
                docker.push(image_name, image_tag, FLY_REGISTRY_URL).await?;
                Command::new("flyctl")
                    .arg("deploy")
                    .arg("--image")
                    .arg(format!("{FLY_REGISTRY_URL}/{image_name}:{image_tag}"))
                    .spawn()?;
            }
        }
        Ok(())
    }
}


async fn authenticate_flyio_via_cli() -> Result<()> {
    let mut child = Command::new("flyctl")
        .arg("auth")
        .arg("whoami")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(b"N\n").await?;
    }

    let status = child.wait().await?;
    match status.success() {
        true => Ok(()),
        false => Err(eyre::eyre!("Failed to authenticate via CLI")),
    }
}
