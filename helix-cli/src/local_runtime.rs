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
        let image = config.image_ref();
        Step::verbose_substep(&format!("Pulling {image}"));
        let output = Command::new(self.runtime.binary())
            .args(["pull", &image])
            .output()
            .map_err(|e| eyre!("Failed to pull {image}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to pull {image}:\n{stderr}"));
        }

        Ok(())
    }

    pub fn run_detached(&self, instance_name: &str, config: &LocalInstanceConfig) -> Result<()> {
        Self::check_available(self.runtime)?;
        self.pull_image(config)?;

        let name = self.container_name(instance_name);
        let image = config.image_ref();
        let _ = self.remove_container(&name);

        let output = Command::new(self.runtime.binary())
            .args([
                "run",
                "-d",
                "--restart",
                "unless-stopped",
                "--name",
                &name,
                "-p",
                &format!("{}:{CONTAINER_PORT}", config.port),
                &image,
            ])
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

        let mut child = TokioCommand::new(self.runtime.binary())
            .args([
                "run",
                "--rm",
                "--name",
                &name,
                "-p",
                &format!("{}:{CONTAINER_PORT}", config.port),
                &image,
            ])
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
                    return Err(eyre!("{name} exited with status {status}"));
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal?;
                crate::output::info("Stopping foreground local Helix instance");
                let _ = self.remove_container(&name);
                match tokio::time::timeout(Duration::from_secs(10), &mut wait).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => return Err(eyre!("Failed to wait for {name} to stop: {e}")),
                    Err(_) => return Err(eyre!("Timed out waiting for {name} to stop")),
                }
            }
        }

        Ok(())
    }

    pub fn stop(&self, instance_name: &str) -> Result<bool> {
        let name = self.container_name(instance_name);
        let stop = Command::new(self.runtime.binary())
            .args(["stop", &name])
            .output()
            .map_err(|e| eyre!("Failed to stop {name}: {e}"))?;

        let existed = if stop.status.success() {
            true
        } else {
            let stderr = String::from_utf8_lossy(&stop.stderr);
            if stderr.contains("No such container") {
                false
            } else if stderr.contains("is not running") {
                true
            } else {
                return Err(eyre!("Failed to stop {name}:\n{stderr}"));
            }
        };

        let removed = self.remove_container(&name)?;
        Ok(existed || removed)
    }

    pub fn restart(&self, instance_name: &str, config: &LocalInstanceConfig) -> Result<()> {
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
        self.remove_container(&name)
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

    fn remove_container(&self, name: &str) -> Result<bool> {
        let output = Command::new(self.runtime.binary())
            .args(["rm", "-f", name])
            .output()
            .map_err(|e| eyre!("Failed to remove {name}: {e}"))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such container") {
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
