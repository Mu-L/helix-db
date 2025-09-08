use crate::config::InstanceInfo;
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::print_error;
use eyre::Result;

pub async fn run() -> Result<()> {
    // Load project context
    let project = match ProjectContext::find_and_load(None) {
        Ok(project) => project,
        Err(_) => {
            print_error("Not in a Helix project directory. Run 'helix init' to create one.");
            return Ok(());
        }
    };

    println!("Helix Project Status");
    println!("  Project: {}", project.config.project.name);
    println!("  Root: {}", project.root.display());
    println!();

    // Show configured instances
    println!("Configured Instances:");

    // Show local instances
    for (name, config) in &project.config.local {
        let port = config.port.unwrap_or(6969);
        println!("  {} (Local) - port {}", name, port);
    }

    // Show cloud instances
    let mut helix_cloud_instances = Vec::new();
    let mut flyio_instances = Vec::new();

    for (name, config) in &project.config.cloud {
        match config {
            crate::config::CloudConfig::HelixCloud(helix_config) => {
                helix_cloud_instances.push((name, &helix_config.cluster_id));
            }
            crate::config::CloudConfig::FlyIo(fly_config) => {
                flyio_instances.push((name, &fly_config.cluster_id));
            }
        }
    }

    for (name, cluster_id) in helix_cloud_instances {
        println!("  {} (Helix Cloud) - cluster {}", name, cluster_id);
    }

    for (name, cluster_id) in flyio_instances {
        println!("  {} (Fly.io) - cluster {}", name, cluster_id);
    }
    println!();

    // Show running containers (for local instances)
    show_container_status(&project).await?;

    Ok(())
}

async fn show_container_status(project: &ProjectContext) -> Result<()> {
    // Check if Docker is available
    if DockerManager::check_docker_available().is_err() {
        println!("Docker Status: Not available");
        return Ok(());
    }

    let docker = DockerManager::new(project);

    let statuses = match docker.get_project_status() {
        Ok(statuses) => statuses,
        Err(e) => {
            println!("Container Status: Error getting status ({})", e);
            return Ok(());
        }
    };

    if statuses.is_empty() {
        println!("Running Containers: None");
        return Ok(());
    }

    println!("Running Containers:");
    for status in statuses {
        let status_icon = if status.status.contains("Up") {
            "[UP]"
        } else {
            "[DOWN]"
        };

        println!(
            "  {} {} - {} ({})",
            status_icon,
            status.instance_name,
            status.status,
            if status.ports.is_empty() {
                "no ports"
            } else {
                &status.ports
            }
        );
    }

    Ok(())
}
