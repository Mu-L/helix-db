use crate::config::{ContainerRuntime, LocalInstanceConfig};
use crate::errors::CliError;
use crate::output::Step;
use crate::project::ProjectContext;
use eyre::{Result, eyre};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tokio::process::Command as TokioCommand;

pub const CONTAINER_PORT: u16 = 8080;
const MINIO_IMAGE: &str = "minio/minio:latest";
const MINIO_MC_IMAGE: &str = "minio/mc:latest";
const MINIO_ACCESS_KEY: &str = "minioadmin";
const MINIO_SECRET_KEY: &str = "minioadmin";
const LOCAL_S3_BUCKET: &str = "helix-db";
const LOCAL_S3_REGION: &str = "us-east-1";
const LOCAL_DB_PATH: &str = "db/";

#[derive(Debug, Clone)]
pub struct LocalRuntime {
    runtime: ContainerRuntime,
    project_name: String,
}

#[derive(Debug, Clone)]
pub struct LocalStatus {
    pub instance_name: String,
    pub container_name: String,
    pub status: String,
    pub ports: String,
}

#[derive(Debug, Clone)]
struct DiskRuntimeResources {
    minio_container: String,
    network: String,
    volume: String,
}

impl LocalRuntime {
    pub fn new(project: &ProjectContext) -> Self {
        Self {
            runtime: project.config.project.container_runtime,
            project_name: project.config.project.name.clone(),
        }
    }

    pub fn check_available(runtime: ContainerRuntime) -> Result<()> {
        let output = Command::new(runtime.binary())
            .arg("info")
            .output()
            .map_err(|e| {
                eyre!(
                    "{} is not available. Install/start {} and try again: {e}",
                    runtime.label(),
                    runtime.binary()
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("{} is not running:\n{}", runtime.label(), stderr));
        }

        Ok(())
    }

    pub fn runtime(&self) -> ContainerRuntime {
        self.runtime
    }

    pub fn container_name(&self, instance_name: &str) -> String {
        format!("helix-{}-{}", self.project_name, instance_name)
    }

    pub fn pull_image(&self, config: &LocalInstanceConfig) -> Result<()> {
        self.pull_image_ref(&config.image_ref())
    }

    fn pull_image_ref(&self, image: &str) -> Result<()> {
        Step::verbose_substep(&format!("Pulling {image}"));
        let output = Command::new(self.runtime.binary())
            .args(["pull", image])
            .output()
            .map_err(|e| eyre!("Failed to pull {image}: {e}"))?;

        if !output.status.success() {
            if self.image_exists(image) {
                Step::verbose_substep(&format!("Using local image {image}"));
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to pull {image}:\n{stderr}"));
        }

        Ok(())
    }

    fn image_exists(&self, image: &str) -> bool {
        Command::new(self.runtime.binary())
            .args(["image", "inspect", image])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn run_detached(&self, instance_name: &str, config: &LocalInstanceConfig) -> Result<()> {
        Self::check_available(self.runtime)?;
        self.pull_image(config)?;

        let name = self.container_name(instance_name);
        let image = config.image_ref();
        let _ = self.remove_container(&name);
        let disk_resources = if config.storage.is_disk() {
            Some(self.start_disk_dependencies(instance_name)?)
        } else {
            let _ = self.remove_disk_resources(instance_name, false);
            None
        };

        let args = helix_run_args(&name, &image, config.port, true, disk_resources.as_ref());
        let output = Command::new(self.runtime.binary())
            .args(&args)
            .output()
            .map_err(|e| eyre!("Failed to start {name}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to start {name}:\n{stderr}"));
        }

        self.wait_ready(config.port)?;
        Ok(())
    }

    pub async fn run_foreground(
        &self,
        instance_name: &str,
        config: &LocalInstanceConfig,
    ) -> Result<()> {
        Self::check_available(self.runtime)?;
        self.pull_image(config)?;

        let name = self.container_name(instance_name);
        let image = config.image_ref();
        let _ = self.remove_container(&name);
        let disk_resources = if config.storage.is_disk() {
            Some(self.start_disk_dependencies(instance_name)?)
        } else {
            let _ = self.remove_disk_resources(instance_name, false);
            None
        };
        let args = helix_run_args(&name, &image, config.port, false, disk_resources.as_ref());

        let mut child = TokioCommand::new(self.runtime.binary())
            .args(&args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| eyre!("Failed to run {name}: {e}"))?;

        let mut wait = Box::pin(child.wait());
        tokio::select! {
            status = &mut wait => {
                let status = status?;
                if !status.success() {
                    if config.storage.is_disk() {
                        let _ = self.remove_disk_resources(instance_name, false);
                    }
                    return Err(eyre!("{name} exited with status {status}"));
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal?;
                crate::output::info("Stopping foreground local Helix instance");
                let _ = self.remove_container(&name);
                if config.storage.is_disk() {
                    let _ = self.remove_disk_resources(instance_name, false);
                }
                match tokio::time::timeout(Duration::from_secs(10), &mut wait).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => return Err(eyre!("Failed to wait for {name} to stop: {e}")),
                    Err(_) => return Err(eyre!("Timed out waiting for {name} to stop")),
                }
            }
        }

        if config.storage.is_disk() {
            let _ = self.remove_disk_resources(instance_name, false);
        }

        Ok(())
    }

    pub fn stop(&self, instance_name: &str) -> Result<bool> {
        let name = self.container_name(instance_name);
        let removed_helix = self.remove_container(&name)?;
        let removed_disk_resources = self.remove_disk_resources(instance_name, false)?;
        Ok(removed_helix || removed_disk_resources)
    }

    pub fn restart(&self, instance_name: &str, config: &LocalInstanceConfig) -> Result<()> {
        if config.storage.is_disk() {
            return self.run_detached(instance_name, config);
        }

        let name = self.container_name(instance_name);
        let output = Command::new(self.runtime.binary())
            .args(["restart", &name])
            .output()
            .map_err(|e| eyre!("Failed to restart {name}: {e}"))?;

        if output.status.success() {
            self.wait_ready(config.port)?;
            return Ok(());
        }

        self.run_detached(instance_name, config)
    }

    pub fn logs(&self, instance_name: &str, follow: bool) -> Result<()> {
        let name = self.container_name(instance_name);
        let mut command = Command::new(self.runtime.binary());
        command.arg("logs");
        if follow {
            command.arg("-f");
        }
        command.arg(&name);
        let status = command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| eyre!("Failed to read logs for {name}: {e}"))?;

        if !status.success() {
            return Err(eyre!(
                "{} logs exited with status {status}",
                self.runtime.binary()
            ));
        }
        Ok(())
    }

    pub fn status(&self, instance_name: &str) -> Result<Option<LocalStatus>> {
        let name = self.container_name(instance_name);
        let output = Command::new(self.runtime.binary())
            .args([
                "ps",
                "-a",
                "--format",
                "{{.Names}}\t{{.Status}}\t{{.Ports}}",
                "--filter",
                &format!("name=^{name}$"),
            ])
            .output()
            .map_err(|e| eyre!("Failed to inspect {name}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to inspect {name}:\n{stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let Some(line) = stdout.lines().find(|line| !line.trim().is_empty()) else {
            return Ok(None);
        };
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            return Ok(None);
        }

        Ok(Some(LocalStatus {
            instance_name: instance_name.to_string(),
            container_name: parts[0].to_string(),
            status: parts[1].to_string(),
            ports: parts[2].to_string(),
        }))
    }

    pub fn prune_instance(&self, instance_name: &str) -> Result<bool> {
        let name = self.container_name(instance_name);
        let removed_helix = self.remove_container(&name)?;
        let removed_disk_resources = self.remove_disk_resources(instance_name, true)?;
        Ok(removed_helix || removed_disk_resources)
    }

    pub fn run_command(&self, args: &[&str]) -> Result<Output> {
        Command::new(self.runtime.binary())
            .args(args)
            .output()
            .map_err(|e| {
                eyre!(
                    "Failed to run {} {}: {e}",
                    self.runtime.binary(),
                    args.join(" ")
                )
            })
    }

    fn disk_resources(&self, instance_name: &str) -> DiskRuntimeResources {
        let base = self.container_name(instance_name);
        DiskRuntimeResources {
            minio_container: format!("{base}-minio"),
            network: format!("{base}-net"),
            volume: format!("{base}-minio-data"),
        }
    }

    fn start_disk_dependencies(&self, instance_name: &str) -> Result<DiskRuntimeResources> {
        let resources = self.disk_resources(instance_name);
        self.pull_image_ref(MINIO_IMAGE)?;
        self.pull_image_ref(MINIO_MC_IMAGE)?;
        self.ensure_network(&resources.network)?;
        self.ensure_volume(&resources.volume)?;
        let _ = self.remove_container(&resources.minio_container);

        let args = minio_run_args(&resources);
        let output = Command::new(self.runtime.binary())
            .args(&args)
            .output()
            .map_err(|e| eyre!("Failed to start {}: {e}", resources.minio_container))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!(
                "Failed to start {}:\n{stderr}",
                resources.minio_container
            ));
        }

        self.ensure_minio_bucket(&resources)?;
        Ok(resources)
    }

    fn ensure_network(&self, network: &str) -> Result<()> {
        if self.resource_exists(&["network", "inspect", network]) {
            return Ok(());
        }

        let output = Command::new(self.runtime.binary())
            .args(["network", "create", network])
            .output()
            .map_err(|e| eyre!("Failed to create network {network}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.to_ascii_lowercase().contains("already exists") {
                return Err(eyre!("Failed to create network {network}:\n{stderr}"));
            }
        }

        Ok(())
    }

    fn ensure_volume(&self, volume: &str) -> Result<()> {
        let output = Command::new(self.runtime.binary())
            .args(["volume", "create", volume])
            .output()
            .map_err(|e| eyre!("Failed to create volume {volume}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to create volume {volume}:\n{stderr}"));
        }

        Ok(())
    }

    fn ensure_minio_bucket(&self, resources: &DiskRuntimeResources) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let args = minio_bucket_init_args(resources);
        let mut last_stderr = String::new();

        while Instant::now() < deadline {
            let output = Command::new(self.runtime.binary())
                .args(&args)
                .output()
                .map_err(|e| eyre!("Failed to initialize local MinIO bucket: {e}"))?;

            if output.status.success() {
                return Ok(());
            }

            last_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            thread::sleep(Duration::from_millis(500));
        }

        Err(eyre!(
            "Timed out initializing local MinIO bucket {LOCAL_S3_BUCKET}:\n{last_stderr}"
        ))
    }

    fn remove_disk_resources(&self, instance_name: &str, include_volume: bool) -> Result<bool> {
        let resources = self.disk_resources(instance_name);
        let removed_minio = self.remove_container(&resources.minio_container)?;
        let removed_network = self.remove_network(&resources.network)?;
        let removed_volume = if include_volume {
            self.remove_volume(&resources.volume)?
        } else {
            false
        };

        Ok(removed_minio || removed_network || removed_volume)
    }

    fn remove_network(&self, network: &str) -> Result<bool> {
        let output = Command::new(self.runtime.binary())
            .args(["network", "rm", network])
            .output()
            .map_err(|e| eyre!("Failed to remove network {network}: {e}"))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if missing_resource(&stderr) {
            return Ok(false);
        }

        if !output.status.success() {
            return Err(eyre!("Failed to remove network {network}:\n{stderr}"));
        }
        Ok(true)
    }

    fn remove_volume(&self, volume: &str) -> Result<bool> {
        let output = Command::new(self.runtime.binary())
            .args(["volume", "rm", volume])
            .output()
            .map_err(|e| eyre!("Failed to remove volume {volume}: {e}"))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if missing_resource(&stderr) {
            return Ok(false);
        }

        if !output.status.success() {
            return Err(eyre!("Failed to remove volume {volume}:\n{stderr}"));
        }
        Ok(true)
    }

    fn resource_exists(&self, args: &[&str]) -> bool {
        Command::new(self.runtime.binary())
            .args(args)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn remove_container(&self, name: &str) -> Result<bool> {
        let output = Command::new(self.runtime.binary())
            .args(["rm", "-f", name])
            .output()
            .map_err(|e| eyre!("Failed to remove {name}: {e}"))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if missing_resource(&stderr) {
            return Ok(false);
        }

        if !output.status.success() {
            return Err(eyre!("Failed to remove {name}:\n{stderr}"));
        }
        Ok(true)
    }

    fn wait_ready(&self, port: u16) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if self.query_endpoint_ready(port) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }

        Err(CliError::new("local Helix did not become ready in time")
            .with_hint(format!(
                "check logs with 'helix logs' or verify port {port} is reachable"
            ))
            .into())
    }

    fn query_endpoint_ready(&self, port: u16) -> bool {
        let Ok(mut stream) = TcpStream::connect_timeout(
            &(std::net::Ipv4Addr::LOCALHOST, port).into(),
            Duration::from_millis(500),
        ) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(750)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(750)));

        let body = r#"{"request_type":"read","query":{"queries":[{"Query":{"name":"readiness","steps":[{"NWhere":{"Eq":["$label",{"String":"__HelixReadiness__"}]}},"Count"],"condition":null}}],"returns":["readiness"]},"parameters":{}}"#;
        let request = format!(
            "POST /v1/query HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );

        if stream.write_all(request.as_bytes()).is_err() {
            return false;
        }

        let mut response = String::new();
        if stream.read_to_string(&mut response).is_err() {
            return false;
        }

        response.starts_with("HTTP/1.1 2") || response.starts_with("HTTP/1.0 2")
    }
}

fn helix_run_args(
    name: &str,
    image: &str,
    port: u16,
    detached: bool,
    disk_resources: Option<&DiskRuntimeResources>,
) -> Vec<String> {
    let mut args = vec!["run".to_string()];
    if detached {
        args.extend([
            "-d".to_string(),
            "--restart".to_string(),
            "unless-stopped".to_string(),
        ]);
    } else {
        args.push("--rm".to_string());
    }

    args.extend([
        "--name".to_string(),
        name.to_string(),
        "-p".to_string(),
        format!("{port}:{CONTAINER_PORT}"),
    ]);

    if let Some(resources) = disk_resources {
        args.extend(["--network".to_string(), resources.network.clone()]);
        for (key, value) in disk_env(resources) {
            args.extend(["-e".to_string(), format!("{key}={value}")]);
        }
    }

    args.push(image.to_string());
    args
}

fn minio_run_args(resources: &DiskRuntimeResources) -> Vec<String> {
    vec![
        "run".to_string(),
        "-d".to_string(),
        "--restart".to_string(),
        "unless-stopped".to_string(),
        "--name".to_string(),
        resources.minio_container.clone(),
        "--network".to_string(),
        resources.network.clone(),
        "-e".to_string(),
        format!("MINIO_ROOT_USER={MINIO_ACCESS_KEY}"),
        "-e".to_string(),
        format!("MINIO_ROOT_PASSWORD={MINIO_SECRET_KEY}"),
        "-v".to_string(),
        format!("{}:/data", resources.volume),
        MINIO_IMAGE.to_string(),
        "server".to_string(),
        "/data".to_string(),
        "--console-address".to_string(),
        ":9001".to_string(),
    ]
}

fn minio_bucket_init_args(resources: &DiskRuntimeResources) -> Vec<String> {
    let endpoint = format!("http://{}:9000", resources.minio_container);
    let command = format!(
        "mc alias set local {} {} {} && mc mb --ignore-existing local/{}",
        shell_quote(&endpoint),
        shell_quote(MINIO_ACCESS_KEY),
        shell_quote(MINIO_SECRET_KEY),
        LOCAL_S3_BUCKET
    );

    vec![
        "run".to_string(),
        "--rm".to_string(),
        "--network".to_string(),
        resources.network.clone(),
        "--entrypoint".to_string(),
        "/bin/sh".to_string(),
        MINIO_MC_IMAGE.to_string(),
        "-c".to_string(),
        command,
    ]
}

fn disk_env(resources: &DiskRuntimeResources) -> Vec<(&'static str, String)> {
    vec![
        ("S3_BUCKET", LOCAL_S3_BUCKET.to_string()),
        ("S3_REGION", LOCAL_S3_REGION.to_string()),
        ("DB_PATH", LOCAL_DB_PATH.to_string()),
        ("AWS_ACCESS_KEY_ID", MINIO_ACCESS_KEY.to_string()),
        ("AWS_SECRET_ACCESS_KEY", MINIO_SECRET_KEY.to_string()),
        (
            "AWS_ENDPOINT",
            format!("http://{}:9000", resources.minio_container),
        ),
        ("AWS_ALLOW_HTTP", "true".to_string()),
    ]
}

fn missing_resource(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("no such") || stderr.contains("not found") || stderr.contains("does not exist")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk_resources() -> DiskRuntimeResources {
        DiskRuntimeResources {
            minio_container: "helix-demo-dev-minio".to_string(),
            network: "helix-demo-dev-net".to_string(),
            volume: "helix-demo-dev-minio-data".to_string(),
        }
    }

    fn has_pair(args: &[String], key: &str, value: &str) -> bool {
        args.windows(2)
            .any(|window| window[0] == key && window[1] == value)
    }

    #[test]
    fn memory_helix_args_match_existing_run_shape() {
        let args = helix_run_args(
            "helix-demo-dev",
            "ghcr.io/helixdb/enterprise-dev:latest",
            9090,
            true,
            None,
        );

        assert_eq!(
            args,
            vec![
                "run",
                "-d",
                "--restart",
                "unless-stopped",
                "--name",
                "helix-demo-dev",
                "-p",
                "9090:8080",
                "ghcr.io/helixdb/enterprise-dev:latest",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn disk_helix_args_include_network_and_s3_env() {
        let resources = disk_resources();
        let args = helix_run_args(
            "helix-demo-dev",
            "ghcr.io/helixdb/enterprise-dev:latest",
            8080,
            true,
            Some(&resources),
        );

        assert!(has_pair(&args, "--network", "helix-demo-dev-net"));
        assert!(args.contains(&"S3_BUCKET=helix-db".to_string()));
        assert!(args.contains(&"S3_REGION=us-east-1".to_string()));
        assert!(args.contains(&"DB_PATH=db/".to_string()));
        assert!(args.contains(&"AWS_ACCESS_KEY_ID=minioadmin".to_string()));
        assert!(args.contains(&"AWS_SECRET_ACCESS_KEY=minioadmin".to_string()));
        assert!(args.contains(&"AWS_ENDPOINT=http://helix-demo-dev-minio:9000".to_string()));
        assert!(args.contains(&"AWS_ALLOW_HTTP=true".to_string()));
    }

    #[test]
    fn minio_args_include_persistent_volume() {
        let resources = disk_resources();
        let args = minio_run_args(&resources);

        assert!(has_pair(&args, "--network", "helix-demo-dev-net"));
        assert!(args.contains(&"MINIO_ROOT_USER=minioadmin".to_string()));
        assert!(args.contains(&"MINIO_ROOT_PASSWORD=minioadmin".to_string()));
        assert!(args.contains(&"helix-demo-dev-minio-data:/data".to_string()));
    }

    #[test]
    fn minio_bucket_init_uses_shell_entrypoint() {
        let resources = disk_resources();
        let args = minio_bucket_init_args(&resources);

        assert!(has_pair(&args, "--entrypoint", "/bin/sh"));
        assert!(args.contains(&"minio/mc:latest".to_string()));
        assert!(args.iter().any(|arg| arg.contains("mc alias set local")));
    }
}
