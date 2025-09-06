use crate::config::{BuildMode, InstanceInfo};
use crate::project::ProjectContext;
use eyre::{Result, eyre};
use std::path::Path;
use std::process::Command;

pub struct DockerManager<'a> {
    project: &'a ProjectContext,
}

impl<'a> DockerManager<'a> {
    pub fn new(project: &'a ProjectContext) -> Self {
        Self { project }
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
        let project_name = &self.project.config.project.name;
        let port = instance_config.port().unwrap_or(6969);
        let volume_path = self.project.instance_volume(instance_name);
        let _workspace_path = self.project.instance_workspace(instance_name);

        let cargo_cache_path = self.project.helix_dir.join(".cargo-cache");

        let compose = format!(
            r#"# Generated docker-compose.yml for Helix instance: {instance_name}
services:
  helix-{instance_name}:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: "helix-{project_name}-{instance_name}"
    ports:
      - "{port}:{port}"
    volumes:
      - "{volume_path}:/data"
      - "{cargo_cache_path}:/cargo-cache"
    environment:
      - HELIX_PORT={port}
      - HELIX_DATA_DIR=/data
      - HELIX_INSTANCE={instance_name}
      - HELIX_PROJECT={project_name}
    restart: unless-stopped
    networks:
      - helix-network

networks:
  helix-network:
    driver: bridge
"#,
            volume_path = volume_path.display(),
            cargo_cache_path = cargo_cache_path.display(),
        );

        Ok(compose)
    }

    /// Build Docker image for an instance
    pub fn build_image(&self, instance_name: &str) -> Result<()> {
        let workspace = self.project.instance_workspace(instance_name);

        println!("[DOCKER] Building image for instance '{}'...", instance_name);

        let output = Command::new("docker-compose")
            .args(["build"])
            .current_dir(&workspace)
            .output()
            .map_err(|e| eyre!("Failed to run docker-compose build: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Docker build failed:\n{}", stderr));
        }

        println!("[DOCKER] Image built successfully");
        Ok(())
    }

    /// Start instance using docker-compose
    pub fn start_instance(&self, instance_name: &str) -> Result<()> {
        let workspace = self.project.instance_workspace(instance_name);

        println!("[DOCKER] Starting instance '{}'...", instance_name);

        let output = Command::new("docker-compose")
            .args(["up", "-d"])
            .current_dir(&workspace)
            .output()
            .map_err(|e| eyre!("Failed to run docker-compose up: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to start instance:\n{}", stderr));
        }

        println!("[DOCKER] Instance '{}' started successfully", instance_name);
        Ok(())
    }

    /// Stop instance using docker-compose
    pub fn stop_instance(&self, instance_name: &str) -> Result<()> {
        let workspace = self.project.instance_workspace(instance_name);

        println!("[DOCKER] Stopping instance '{}'...", instance_name);

        let output = Command::new("docker-compose")
            .args(["down"])
            .current_dir(&workspace)
            .output()
            .map_err(|e| eyre!("Failed to run docker-compose down: {}", e))?;

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

        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--format",
                "table {{.Names}}\t{{.Status}}\t{{.Ports}}",
                "--filter",
                &format!("name=helix-{}", project_name),
            ])
            .output()
            .map_err(|e| eyre!("Failed to get container status: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to get container status:\n{}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut statuses = Vec::new();

        // Skip the header line
        for line in stdout.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let name = parts[0].trim();
                let status = parts[1].trim();
                let ports = parts[2].trim();

                // Extract instance name from container name
                let instance_name = name
                    .strip_prefix(&format!("helix-{}-", project_name))
                    .unwrap_or(name);

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
        let workspace = self.project.instance_workspace(instance_name);

        println!("[DOCKER] Pruning instance '{}'...", instance_name);

        // Stop and remove containers
        let mut args = vec!["down"];
        if remove_volumes {
            args.push("--volumes");
            args.push("--remove-orphans");
        }

        let output = Command::new("docker-compose")
            .args(&args)
            .current_dir(&workspace)
            .output()
            .map_err(|e| eyre!("Failed to run docker-compose down: {}", e))?;

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
