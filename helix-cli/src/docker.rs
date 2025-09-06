use crate::config::{BuildMode, InstanceInfo};
use crate::project::ProjectContext;
use eyre::{Result, eyre};
use std::process::{Command, Output};

pub struct DockerManager<'a> {
    project: &'a ProjectContext,
}

impl<'a> DockerManager<'a> {
    pub fn new(project: &'a ProjectContext) -> Self {
        Self { project }
    }

    // === CENTRALIZED NAMING METHODS ===
    
    /// Get the compose project name for an instance
    fn compose_project_name(&self, instance_name: &str) -> String {
        format!("helix-{}-{}", self.project.config.project.name, instance_name)
    }
    
    /// Get the service name (always "app")
    fn service_name() -> &'static str {
        "app"
    }
    
    /// Get the image name for an instance
    fn image_name(&self, instance_name: &str, build_mode: BuildMode) -> String {
        let tag = match build_mode {
            BuildMode::Debug => "debug",
            BuildMode::Release => "latest",
        };
        format!("{}:{}", self.compose_project_name(instance_name), tag)
    }
    
    /// Get the container name for an instance
    fn container_name(&self, instance_name: &str) -> String {
        format!("{}-app", self.compose_project_name(instance_name))
    }
    
    /// Get the data volume name for an instance
    fn data_volume_name(&self, instance_name: &str) -> String {
        format!("{}-data", self.compose_project_name(instance_name))
    }
    
    /// Get the cache volume name for an instance
    fn cache_volume_name(&self, instance_name: &str) -> String {
        format!("{}-cache", self.compose_project_name(instance_name))
    }
    
    /// Get the network name for an instance
    fn network_name(&self, instance_name: &str) -> String {
        format!("{}-net", self.compose_project_name(instance_name))
    }

    // === CENTRALIZED DOCKER COMMAND EXECUTION ===
    
    /// Run a docker command with consistent error handling
    pub fn run_docker_command(&self, args: &[&str]) -> Result<Output> {
        let output = Command::new("docker")
            .args(args)
            .output()
            .map_err(|e| eyre!("Failed to run docker {}: {}", args.join(" "), e))?;
        Ok(output)
    }
    
    /// Run a docker-compose command with proper project naming
    fn run_compose_command(&self, instance_name: &str, args: &[&str]) -> Result<Output> {
        let workspace = self.project.instance_workspace(instance_name);
        let project_name = self.compose_project_name(instance_name);
        
        let mut full_args = vec!["--project-name", &project_name];
        full_args.extend(args);
        
        let output = Command::new("docker-compose")
            .args(&full_args)
            .current_dir(&workspace)
            .output()
            .map_err(|e| eyre!("Failed to run docker-compose {}: {}", args.join(" "), e))?;
        Ok(output)
    }

    /// Check if Docker is installed and running
    pub fn check_docker_available() -> Result<()> {
        let output = Command::new("docker")
            .args(["--version"])
            .output()
            .map_err(|_| eyre!("Docker is not installed or not available in PATH"))?;

        if !output.status.success() {
            return Err(eyre!("Docker is installed but not working properly"));
        }

        // Check if Docker daemon is running
        let output = Command::new("docker")
            .args(["info"])
            .output()
            .map_err(|_| eyre!("Failed to check Docker daemon status"))?;

        if !output.status.success() {
            return Err(eyre!("Docker daemon is not running. Please start Docker."));
        }

        Ok(())
    }

    /// Generate Dockerfile for an instance
    pub fn generate_dockerfile(
        &self,
        instance_name: &str,
        instance_config: InstanceInfo<'_>,
    ) -> Result<String> {
        let build_flag = match instance_config.build_mode() {
            BuildMode::Debug => "",
            BuildMode::Release => "--release",
        };
        let build_mode = match instance_config.build_mode() {
            BuildMode::Debug => "debug",
            BuildMode::Release => "release",
        };

        let dockerfile = format!(
            r#"# Generated Dockerfile for Helix instance: {instance_name}
FROM rust:1.88-slim as builder

WORKDIR /build

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set up cargo cache - mounted from host
ENV CARGO_HOME=/cargo-cache
ENV CARGO_TARGET_DIR=/cargo-cache/target

# First, copy the entire cached repo workspace (includes all dependencies)
# This uses the copied repo directory
COPY helix-repo-copy/ ./

# Then overlay instance-specific files from current directory
# This overwrites any files with the instance-specific versions
COPY . ./

# Build the helix-container package from the workspace
# All workspace dependencies are now available
RUN cargo build {build_flag} --package helix-container

# Runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary
COPY --from=builder /cargo-cache/target/{build_mode}/helix-container /usr/local/bin/helix-container

# Create data directory
RUN mkdir -p /data

# Expose port (will be overridden by docker-compose)
EXPOSE 6969

# Run the application
CMD ["helix-container"]
"#
        );

        Ok(dockerfile)
    }

    /// Generate docker-compose.yml for an instance
    pub fn generate_docker_compose(
        &self,
        instance_name: &str,
        instance_config: InstanceInfo<'_>,
    ) -> Result<String> {
        let port = instance_config.port().unwrap_or(6969);
        let volume_path = self.project.instance_volume(instance_name);
        let cargo_cache_path = self.project.helix_dir.join(".cargo-cache");
        
        // Use centralized naming methods
        let service_name = Self::service_name();
        let image_name = self.image_name(instance_name, instance_config.build_mode());
        let container_name = self.container_name(instance_name);
        let data_volume_name = self.data_volume_name(instance_name);
        let cache_volume_name = self.cache_volume_name(instance_name);
        let network_name = self.network_name(instance_name);

        let compose = format!(
            r#"# Generated docker-compose.yml for Helix instance: {instance_name}
version: '3.8'

services:
  {service_name}:
    build:
      context: .
      dockerfile: Dockerfile
    image: {image_name}
    container_name: {container_name}
    ports:
      - "{port}:{port}"
    volumes:
      - {data_volume_name}:/data
      - {cache_volume_name}:/cargo-cache
    environment:
      - HELIX_PORT={port}
      - HELIX_DATA_DIR=/data
      - HELIX_INSTANCE={instance_name}
      - HELIX_PROJECT={project_name}
    restart: unless-stopped
    networks:
      - {network_name}

volumes:
  {data_volume_name}:
    driver: local
    driver_opts:
      type: bind
      device: {volume_path}
      o: bind
  {cache_volume_name}:
    driver: local
    driver_opts:
      type: bind
      device: {cargo_cache_path}
      o: bind

networks:
  {network_name}:
    driver: bridge
"#,
            volume_path = volume_path.display(),
            cargo_cache_path = cargo_cache_path.display(),
            project_name = self.project.config.project.name,
        );

        Ok(compose)
    }

    /// Build Docker image for an instance
    pub fn build_image(&self, instance_name: &str) -> Result<()> {
        println!("[DOCKER] Building image for instance '{}'...", instance_name);

        let output = self.run_compose_command(instance_name, &["build"])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Docker build failed:\n{}", stderr));
        }

        println!("[DOCKER] Image built successfully");
        Ok(())
    }

    /// Start instance using docker-compose
    pub fn start_instance(&self, instance_name: &str) -> Result<()> {
        println!("[DOCKER] Starting instance '{}'...", instance_name);

        let output = self.run_compose_command(instance_name, &["up", "-d"])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to start instance:\n{}", stderr));
        }

        println!("[DOCKER] Instance '{}' started successfully", instance_name);
        Ok(())
    }

    /// Stop instance using docker-compose
    pub fn stop_instance(&self, instance_name: &str) -> Result<()> {
        println!("[DOCKER] Stopping instance '{}'...", instance_name);

        let output = self.run_compose_command(instance_name, &["down"])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to stop instance:\n{}", stderr));
        }

        println!("[DOCKER] Instance '{}' stopped successfully", instance_name);
        Ok(())
    }

    /// Get status of all Docker containers for this project
    pub fn get_project_status(&self) -> Result<Vec<ContainerStatus>> {
        let project_name = &self.project.config.project.name;
        let filter = format!("name=helix-{}-", project_name);

        let output = self.run_docker_command(&[
            "ps",
            "-a",
            "--format",
            "{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Image}}",
            "--filter",
            &filter,
        ])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to get container status:\n{}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut statuses = Vec::new();

        // Process each line (no header with non-table format)
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }

            // Tab-separated output since we removed "table" format
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let name = parts[0].trim();
                let status = parts[1].trim();
                let ports = parts[2].trim();

                // Extract instance name from new container naming scheme: helix-{project}-{instance}-app
                let expected_prefix = format!("helix-{}-", project_name);
                
                let instance_name = if let Some(suffix) = name.strip_prefix(&expected_prefix) {
                    // Remove the trailing "-app" if it exists
                    suffix.strip_suffix("-app").unwrap_or(suffix)
                } else {
                    name
                };

                statuses.push(ContainerStatus {
                    instance_name: instance_name.to_string(),
                    container_name: name.to_string(),
                    status: status.to_string(),
                    ports: ports.to_string(),
                });
            }
        }

        Ok(statuses)
    }

    /// Remove instance containers and optionally volumes
    pub fn prune_instance(&self, instance_name: &str, remove_volumes: bool) -> Result<()> {
        println!("[DOCKER] Pruning instance '{}'...", instance_name);

        // Stop and remove containers
        let mut args = vec!["down"];
        if remove_volumes {
            args.push("--volumes");
            args.push("--remove-orphans");
        }

        let output = self.run_compose_command(instance_name, &args)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to prune instance:\n{}", stderr));
        }

        println!("[DOCKER] Instance '{}' pruned successfully", instance_name);
        Ok(())
    }
}

#[derive(Debug)]
pub struct ContainerStatus {
    pub instance_name: String,
    pub container_name: String,
    pub status: String,
    pub ports: String,
}
