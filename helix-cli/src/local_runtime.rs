use crate::config::{ContainerRuntime, LocalInstanceConfig};
use crate::errors::CliError;
use crate::output::Step;
use crate::project::ProjectContext;
use eyre::{Result, eyre};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

    pub fn run_foreground(&self, instance_name: &str, config: &LocalInstanceConfig) -> Result<()> {
        Self::check_available(self.runtime)?;
        self.pull_image(config)?;

        let name = self.container_name(instance_name);
        let image = config.image_ref();
        let _ = self.remove_container(&name);

        let status = Command::new(self.runtime.binary())
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
            .status()
            .map_err(|e| eyre!("Failed to run {name}: {e}"))?;

        if !status.success() {
            return Err(eyre!("{name} exited with status {status}"));
        }

        Ok(())
    }

    pub fn stop(&self, instance_name: &str) -> Result<()> {
        let name = self.container_name(instance_name);
        let stop = Command::new(self.runtime.binary())
            .args(["stop", &name])
            .output()
            .map_err(|e| eyre!("Failed to stop {name}: {e}"))?;

        if !stop.status.success() {
            let stderr = String::from_utf8_lossy(&stop.stderr);
            if !stderr.contains("No such container") {
                return Err(eyre!("Failed to stop {name}:\n{stderr}"));
            }
        }

        self.remove_container(&name)
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

    pub fn prune_instance(&self, instance_name: &str) -> Result<()> {
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

    fn remove_container(&self, name: &str) -> Result<()> {
        let output = Command::new(self.runtime.binary())
            .args(["rm", "-f", name])
            .output()
            .map_err(|e| eyre!("Failed to remove {name}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("No such container") {
                return Err(eyre!("Failed to remove {name}:\n{stderr}"));
            }
        }
        Ok(())
    }

    fn wait_ready(&self, port: u16) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
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
}
