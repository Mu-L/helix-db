//! Dashboard management for Helix projects

use crate::DashboardAction;
use crate::commands::auth::Credentials;
use crate::commands::integrations::helix::CLOUD_AUTHORITY;
use crate::config::{ContainerRuntime, InstanceInfo};
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::{
    print_field, print_header, print_info, print_newline, print_status, print_success,
    print_warning,
};
use eyre::{Result, eyre};
use std::process::Command;

// Dashboard configuration constants
const DASHBOARD_IMAGE: &str = "public.ecr.aws/p8l2s5f1/helix-dashboard";
const DASHBOARD_TAG: &str = "latest";
const DASHBOARD_CONTAINER_NAME: &str = "helix-dashboard";
const DEFAULT_HELIX_PORT: u16 = 6969;

struct DisplayInfo {
    host: String,
    helix_port: u16,
    instance_name: Option<String>,
    mode: String,
}

pub async fn run(action: DashboardAction) -> Result<()> {
    match action {
        DashboardAction::Start {
            instance,
            port,
            host,
            helix_port,
            attach,
            restart,
        } => start(instance, port, host, helix_port, attach, restart).await,
        DashboardAction::Stop => stop(),
        DashboardAction::Status => status(),
    }
}

async fn start(
    instance: Option<String>,
    port: u16,
    host: Option<String>,
    helix_port: u16,
    attach: bool,
    restart: bool,
) -> Result<()> {
    // Detect runtime (works without project)
    let runtime = detect_runtime()?;

    // Check Docker/Podman availability
    DockerManager::check_runtime_available(runtime)?;

    // Check if dashboard is already running
    if is_dashboard_running(runtime)? {
        if restart {
            print_status("DASHBOARD", "Stopping existing dashboard...");
            stop_dashboard_container(runtime)?;
        } else {
            print_warning("Dashboard is already running");
            if let Ok(existing_port) = get_dashboard_port(runtime) {
                print_info(&format!("Access it at: http://localhost:{existing_port}"));
            }
            print_info("Use 'helix dashboard stop' to stop it, or '--restart' to restart");
            return Ok(());
        }
    }

    // Warn if --helix-port is specified without --host
    if host.is_none() && helix_port != DEFAULT_HELIX_PORT {
        print_warning("--helix-port is ignored without --host; using project config or defaults");
    }

    // Prepare environment variables based on connection mode
    let (env_vars, display_info) = if let Some(host) = host {
        // Direct connection mode - no project needed
        prepare_direct_env_vars(&host, helix_port, runtime)?
    } else {
        // Try to use project config, or fall back to defaults
        prepare_env_vars_from_context(instance, runtime)?
    };

    // Pull the dashboard image
    pull_dashboard_image(runtime)?;

    // Start the dashboard container
    start_dashboard_container(runtime, port, &env_vars, attach)?;

    if !attach {
        let url = format!("http://localhost:{port}");

        print_success("Dashboard started successfully");
        print_field("URL", &url);
        print_field("Helix Host", &display_info.host);
        print_field("Helix Port", &display_info.helix_port.to_string());
        if let Some(instance_name) = &display_info.instance_name {
            print_field("Instance", instance_name);
        }
        print_field("Mode", &display_info.mode);
        print_newline();
        print_info("Run 'helix dashboard stop' to stop the dashboard");

        // Open the dashboard in the default browser
        if let Err(e) = open::that(&url) {
            print_warning(&format!("Could not open browser: {e}"));
        }
    }

    Ok(())
}

fn prepare_direct_env_vars(
    host: &str,
    helix_port: u16,
    runtime: ContainerRuntime,
) -> Result<(Vec<String>, DisplayInfo)> {
    // Use host.docker.internal for Docker, host.containers.internal for Podman
    // when connecting to localhost
    let docker_host = if host == "localhost" || host == "127.0.0.1" {
        match runtime {
            ContainerRuntime::Docker => "host.docker.internal",
            ContainerRuntime::Podman => "host.containers.internal",
        }
    } else {
        host
    };

    let env_vars = vec![
        format!("HELIX_HOST={docker_host}"),
        format!("HELIX_PORT={helix_port}"),
    ];

    let display_info = DisplayInfo {
        host: host.to_string(),
        helix_port,
        instance_name: None,
        mode: "Direct".to_string(),
    };

    Ok((env_vars, display_info))
}

fn prepare_env_vars_from_context(
    instance: Option<String>,
    runtime: ContainerRuntime,
) -> Result<(Vec<String>, DisplayInfo)> {
    // Try to load project context
    match ProjectContext::find_and_load(None) {
        Ok(project) => {
            // Resolve instance from project
            let (instance_name, instance_config) = resolve_instance(&project, instance)?;
            let env_vars = prepare_environment_vars(&project, &instance_name, &instance_config)?;

            let (host, helix_port, mode) = if instance_config.is_local() {
                let port = instance_config.port().unwrap_or(DEFAULT_HELIX_PORT);
                ("localhost".to_string(), port, "Local".to_string())
            } else {
                ("cloud".to_string(), 443, "Cloud".to_string())
            };

            let display_info = DisplayInfo {
                host,
                helix_port,
                instance_name: Some(instance_name),
                mode,
            };

            Ok((env_vars, display_info))
        }
        Err(_) => {
            // No project found - use defaults
            print_info(&format!(
                "No helix.toml found, using default connection (localhost:{DEFAULT_HELIX_PORT})"
            ));
            prepare_direct_env_vars("localhost", DEFAULT_HELIX_PORT, runtime)
        }
    }
}

fn resolve_instance<'a>(
    project: &'a ProjectContext,
    instance: Option<String>,
) -> Result<(String, InstanceInfo<'a>)> {
    match instance {
        Some(name) => {
            let config = project.config.get_instance(&name)?;
            Ok((name, config))
        }
        None => {
            // Try to find a running local instance, or use first local instance
            let local_instances: Vec<_> = project.config.local.keys().collect();

            if local_instances.is_empty() {
                // No local instances, try cloud instances
                let cloud_instances: Vec<_> = project.config.cloud.keys().collect();
                if cloud_instances.is_empty() {
                    return Err(eyre!("No instances configured in helix.toml"));
                }

                let name = cloud_instances[0].clone();
                let config = project.config.get_instance(&name)?;
                print_info(&format!("Using cloud instance: {name}"));
                Ok((name, config))
            } else {
                let name = local_instances[0].clone();
                let config = project.config.get_instance(&name)?;
                print_info(&format!("Using local instance: {name}"));
                Ok((name, config))
            }
        }
    }
}

fn prepare_environment_vars(
    project: &ProjectContext,
    instance_name: &str,
    instance_config: &InstanceInfo,
) -> Result<Vec<String>> {
    let mut env_vars = Vec::new();

    if instance_config.is_local() {
        // Local instance - connect via Docker host networking
        let port = instance_config.port().unwrap_or(DEFAULT_HELIX_PORT);

        // Use host.docker.internal for Docker, host.containers.internal for Podman
        let host = match project.config.project.container_runtime {
            ContainerRuntime::Docker => "host.docker.internal",
            ContainerRuntime::Podman => "host.containers.internal",
        };

        env_vars.push(format!("HELIX_HOST={host}"));
        env_vars.push(format!("HELIX_PORT={port}"));
        env_vars.push(format!("HELIX_INSTANCE={instance_name}"));
    } else {
        // Cloud instance - use cloud URL and API key
        let credentials = load_cloud_credentials()?;

        // Get cloud URL based on instance type
        let cloud_url = get_cloud_url(instance_config)?;

        env_vars.push(format!("HELIX_CLOUD_URL={cloud_url}"));
        env_vars.push(format!("HELIX_API_KEY={}", credentials.helix_admin_key));
        env_vars.push(format!("HELIX_USER_ID={}", credentials.user_id));
        env_vars.push(format!("HELIX_INSTANCE={instance_name}"));

        // Add cluster ID for Helix Cloud instances
        if let Some(cluster_id) = instance_config.cluster_id() {
            env_vars.push(format!("HELIX_CLUSTER_ID={cluster_id}"));
        }
    }

    Ok(env_vars)
}

fn load_cloud_credentials() -> Result<Credentials> {
    let home = dirs::home_dir().ok_or_else(|| eyre!("Cannot find home directory"))?;
    let credentials_path = home.join(".helix").join("credentials");

    if !credentials_path.exists() {
        return Err(eyre!(
            "Not authenticated with Helix Cloud. Run 'helix auth login' first."
        ));
    }

    Credentials::try_read_from_file(&credentials_path)
        .ok_or_else(|| eyre!("Failed to read credentials. Try 'helix auth login' again."))
}

fn get_cloud_url(instance_config: &InstanceInfo) -> Result<String> {
    match instance_config {
        InstanceInfo::Helix(config) => Ok(format!(
            "https://{}/clusters/{}",
            *CLOUD_AUTHORITY, config.cluster_id
        )),
        InstanceInfo::FlyIo(_) => Err(eyre!(
            "Fly.io instances are not yet supported for the dashboard"
        )),
        InstanceInfo::Ecr(_) => Err(eyre!(
            "ECR instances are not yet supported for the dashboard"
        )),
        InstanceInfo::Local(_) => Err(eyre!("Local instances should not call get_cloud_url")),
    }
}

fn is_dashboard_running(runtime: ContainerRuntime) -> Result<bool> {
    let output = Command::new(runtime.binary())
        .args([
            "ps",
            "-q",
            "-f",
            &format!("name={DASHBOARD_CONTAINER_NAME}"),
        ])
        .output()
        .map_err(|e| eyre!("Failed to check dashboard status: {e}"))?;

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn get_dashboard_port(runtime: ContainerRuntime) -> Result<u16> {
    let output = Command::new(runtime.binary())
        .args(["port", DASHBOARD_CONTAINER_NAME, "3000"])
        .output()
        .map_err(|e| eyre!("Failed to get dashboard port: {e}"))?;

    let port_mapping = String::from_utf8_lossy(&output.stdout);
    // Parse "0.0.0.0:3000" format
    port_mapping
        .trim()
        .split(':')
        .next_back()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| eyre!("Failed to parse dashboard port"))
}

fn pull_dashboard_image(runtime: ContainerRuntime) -> Result<()> {
    print_status("DASHBOARD", "Pulling dashboard image...");

    let _ = Command::new(runtime.binary())
        .args(["logout", "public.ecr.aws"])
        .output();

    let image = format!("{DASHBOARD_IMAGE}:{DASHBOARD_TAG}");
    let output = Command::new(runtime.binary())
        .args(["pull", &image])
        .output()
        .map_err(|e| eyre!("Failed to pull dashboard image: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!("Failed to pull dashboard image:\n{stderr}"));
    }

    print_status("DASHBOARD", "Image pulled successfully");
    Ok(())
}

fn start_dashboard_container(
    runtime: ContainerRuntime,
    port: u16,
    env_vars: &[String],
    attach: bool,
) -> Result<()> {
    print_status("DASHBOARD", "Starting dashboard container...");

    let image = format!("{DASHBOARD_IMAGE}:{DASHBOARD_TAG}");

    let mut args = vec![
        "run".to_string(),
        "--name".to_string(),
        DASHBOARD_CONTAINER_NAME.to_string(),
        "-p".to_string(),
        format!("{port}:3000"),
        "--rm".to_string(),
    ];

    // Add detach flag if not attaching
    if !attach {
        args.push("-d".to_string());
    }

    // Add environment variables
    for env in env_vars {
        args.push("-e".to_string());
        args.push(env.clone());
    }

    // Add the image name
    args.push(image);

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    if attach {
        // Run in foreground - use spawn and wait
        let status = Command::new(runtime.binary())
            .args(&args_refs)
            .status()
            .map_err(|e| eyre!("Failed to start dashboard: {e}"))?;

        if !status.success() {
            return Err(eyre!("Dashboard exited with error"));
        }
    } else {
        // Run detached
        let output = Command::new(runtime.binary())
            .args(&args_refs)
            .output()
            .map_err(|e| eyre!("Failed to start dashboard: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to start dashboard:\n{stderr}"));
        }
    }

    Ok(())
}

fn stop_dashboard_container(runtime: ContainerRuntime) -> Result<()> {
    let output = Command::new(runtime.binary())
        .args(["stop", DASHBOARD_CONTAINER_NAME])
        .output()
        .map_err(|e| eyre!("Failed to stop dashboard: {e}"))?;

    if !output.status.success() {
        // Container might already be stopped, which is fine
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("No such container") && !stderr.contains("no such container") {
            return Err(eyre!("Failed to stop dashboard:\n{stderr}"));
        }
    }

    Ok(())
}

fn stop() -> Result<()> {
    // Detect runtime - try to load project config, fallback to checking available runtimes
    let runtime = detect_runtime()?;

    if !is_dashboard_running(runtime)? {
        print_info("Dashboard is not running");
        return Ok(());
    }

    print_status("DASHBOARD", "Stopping dashboard...");
    stop_dashboard_container(runtime)?;
    print_success("Dashboard stopped");

    Ok(())
}

fn detect_runtime() -> Result<ContainerRuntime> {
    // Try to load project config for runtime preference
    if let Ok(project) = ProjectContext::find_and_load(None) {
        return Ok(project.config.project.container_runtime);
    }

    // Fallback: check if Docker is available, then Podman
    if let Ok(output) = Command::new("docker").arg("--version").output()
        && output.status.success()
    {
        return Ok(ContainerRuntime::Docker);
    }

    if let Ok(output) = Command::new("podman").arg("--version").output()
        && output.status.success()
    {
        return Ok(ContainerRuntime::Podman);
    }

    Err(eyre!("Neither Docker nor Podman is available"))
}

fn status() -> Result<()> {
    let runtime = detect_runtime()?;

    print_header("Dashboard Status");

    if !is_dashboard_running(runtime)? {
        print_field("Status", "Not running");
        return Ok(());
    }

    print_field("Status", "Running");

    // Get port
    if let Ok(port) = get_dashboard_port(runtime) {
        print_field("URL", &format!("http://localhost:{port}"));
    }

    // Get container info
    let output = Command::new(runtime.binary())
        .args([
            "inspect",
            DASHBOARD_CONTAINER_NAME,
            "--format",
            "{{range .Config.Env}}{{println .}}{{end}}",
        ])
        .output();

    if let Ok(output) = output {
        let env_output = String::from_utf8_lossy(&output.stdout);

        // Extract connection info from environment
        for line in env_output.lines() {
            if let Some(instance) = line.strip_prefix("HELIX_INSTANCE=") {
                print_field("Instance", instance);
            }
            if let Some(host) = line.strip_prefix("HELIX_HOST=") {
                print_field("Helix Host", host);
            }
            if let Some(port) = line.strip_prefix("HELIX_PORT=") {
                print_field("Helix Port", port);
            }
            if line.starts_with("HELIX_CLOUD_URL=") {
                print_field("Mode", "Cloud");
            }
        }
    }

    Ok(())
}
