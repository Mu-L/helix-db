use std::{default, process::Stdio, sync::LazyLock};

use crate::{docker::DockerManager, project::ProjectContext};
use async_trait::async_trait;
use eyre::Result;
use iota;
use serde::Serialize;
use serde_json::json;
use tokio::{io::AsyncWriteExt, process::Command};

const FLY_MACHINES_API_URL: &str = "https://api.machines.dev/v1/";
const FLY_REGISTRY_URL: &str = "registry.fly.io";
const FLY_DEFAULT_VOLUME_INITIAL_SIZE: u16 = 20;

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
pub enum FlyIoClientAuth {
    ApiKey,
    #[default]
    Cli,
}

impl From<String> for FlyIoClientAuth {
    fn from(value: String) -> Self {
        match value.as_str() {
            "api_key" => Self::ApiKey,
            "cli" => Self::Cli,
            _ => Self::default(),
        }
    }
}

pub(crate) struct FlyIoInstanceConfig {
    /// size of the machine
    vm_size: VmSize,
    /// volume to mount in the form of <volume_name>:/path/inside/machine[:<options>]
    volume: String,
    volume_initial_size: u16,
    /// whether to set a public IP
    no_public_ip: Privacy,
}

#[derive(Default)]
pub(crate) enum Privacy {
    /// Sets a public IP
    #[default]
    Public,
    /// Doesn't set a public IP
    Private,
}
impl Privacy {
    fn no_public_ip_command(&self) -> Vec<&'static str> {
        match self {
            Privacy::Public => vec![],
            Privacy::Private => vec!["--no-public-ip"],
        }
    }
}
impl From<String> for Privacy {
    fn from(value: String) -> Self {
        match value.as_str() {
            "public" | "pub" => Self::Public,
            "private" | "priv" => Self::Private,
            _ => Self::default(),
        }
    }
}
impl From<bool> for Privacy {
    fn from(value: bool) -> Self {
        if value { Self::Public } else { Self::Private }
    }
}
#[derive(Default)]
pub(crate) enum VmSize {
    /// 1 CPU, 256MB RAM
    SharedCpu1x,
    /// 2 CPU, 512MB RAM
    SharedCpu2x,
    /// 4 CPU, 1GB RAM
    SharedCpu4x,
    /// 8 CPU, 2GB RAM
    SharedCpu8x,
    /// 1 CPU, 2GB RAM
    PerformanceCpu1x,
    /// 2 CPU, 4GB RAM
    PerformanceCpu2x,
    /// 4 CPU, 8GB RAM
    #[default]
    PerformanceCpu4x,
    /// 8 CPU, 16GB RAM
    PerformanceCpu8x,
    /// 16 CPU, 32GB RAM
    PerformanceCpu16x,
    /// 8 CPU, 32GB RAM, a10 GPU
    A10,
    /// 8 CPU, 32GB RAM, a100 pcie 40GB GPU
    A10040Gb,
    /// 8 CPU, 32GB RAM, a100 sxm 80GB GPU
    A10080Gb,
    /// 8 CPU, 32GB RAM, l40s GPU
    L40s,
}

impl From<String> for VmSize {
    fn from(value: String) -> Self {
        match value.as_str() {
            "shared-cpu-1x" => Self::SharedCpu1x,
            "shared-cpu-2x" => Self::SharedCpu2x,
            "shared-cpu-4x" => Self::SharedCpu4x,
            "shared-cpu-8x" => Self::SharedCpu8x,
            "performance-1x" => Self::PerformanceCpu1x,
            "performance-2x" => Self::PerformanceCpu2x,
            "performance-4x" => Self::PerformanceCpu4x,
            "performance-8x" => Self::PerformanceCpu8x,
            "performance-16x" => Self::PerformanceCpu16x,
            "a10" => Self::A10,
            "a100-40gb" => Self::A10040Gb,
            "a100-80gb" => Self::A10080Gb,
            "l40s" => Self::L40s,
            _ => Self::default(),
        }
    }
}

impl VmSize {
    fn into_command_args(&self) -> [&'static str; 2] {
        const VM_SIZE_COMMAND: &'static str = "--vm-size";
        let vm_size_arg = match self {
            VmSize::SharedCpu1x => "shared-cpu-1x",
            VmSize::SharedCpu2x => "shared-cpu-2x",
            VmSize::SharedCpu4x => "shared-cpu-4x",
            VmSize::SharedCpu8x => "shared-cpu-8x",
            VmSize::PerformanceCpu1x => "performance-1x",
            VmSize::PerformanceCpu2x => "performance-2x",
            VmSize::PerformanceCpu4x => "performance-4x",
            VmSize::PerformanceCpu8x => "performance-8x",
            VmSize::PerformanceCpu16x => "performance-16x",
            VmSize::A10 => "a10",
            VmSize::A10040Gb => "a100-40gb",
            VmSize::A10080Gb => "a100-80gb",
            VmSize::L40s => "l40s",
        };
        [VM_SIZE_COMMAND, vm_size_arg]
    }
}

impl FlyIoInstanceConfig {
    pub fn new(
        docker: &DockerManager<'_>,
        app_name: &str,
        volume_name: &str,
        volume_initial_size: u16,
        vm_size: VmSize,
        privacy: Privacy,
    ) -> Self {
        let volume = format!(
            "{}:{}:/data",
            volume_name,
            docker.data_volume_name(app_name)
        );
        Self {
            vm_size,
            volume,
            volume_initial_size: volume_initial_size,
            no_public_ip: privacy,
        }
    }

    fn volume_initial_size_command(&self) -> &'static str {
        "--volume-initial-size"
    }
    fn volume_command(&self) -> &'static str {
        "--volume"
    }
}

impl FlyIoClient {
    pub async fn new(project: &ProjectContext, auth: FlyIoClientAuth) -> Result<Self> {
        let (authentication, client) = match auth {
            FlyIoClientAuth::ApiKey => {
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

            FlyIoClientAuth::Cli => match authenticate_flyio_via_cli().await {
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

    pub(crate) async fn init_flyio(
        &self,
        project: &ProjectContext,
        app_name: &str,
        instance_config: FlyIoInstanceConfig,
    ) -> Result<()> {
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
                    .spawn()?
                    .wait()
                    .await?;
                Command::new("flyctl")
                    .arg("launch")
                    .arg("--no-deploy")
                    .args(["--path", project.helix_dir.display().to_string().as_str()])
                    // vm size
                    .args(instance_config.vm_size.into_command_args())
                    // volume
                    .args([
                        instance_config.volume_command(),
                        instance_config.volume.as_str(),
                    ])
                    // volume initial size
                    .args([
                        instance_config.volume_initial_size_command(),
                        instance_config.volume_initial_size.to_string().as_str(),
                    ])
                    // no public ip
                    .args(instance_config.no_public_ip.no_public_ip_command())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?
                    .wait()
                    .await?;
            }
        }
        // write app name to helix.toml

        Ok(())
    }

    pub(crate) async fn deploy_flyio(
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
                    .spawn()?
                    .wait()
                    .await?;
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
