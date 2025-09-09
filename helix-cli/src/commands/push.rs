use crate::commands::integrations::ecr::EcrManager;
use crate::commands::integrations::fly::FlyManager;
use crate::config::{CloudConfig, InstanceInfo};
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::{print_status, print_success};
use eyre::Result;

pub async fn run(instance_name: String) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;

    // Get instance config
    let instance_config = project.config.get_instance(&instance_name)?;

    if instance_config.is_local() {
        push_local_instance(&project, &instance_name).await
    } else {
        push_cloud_instance(&project, &instance_name, instance_config).await
    }
}

async fn push_local_instance(project: &ProjectContext, instance_name: &str) -> Result<()> {
    print_status(
        "DEPLOY",
        &format!("Deploying local instance '{}'", instance_name),
    );

    let docker = DockerManager::new(project);

    // Check Docker availability
    DockerManager::check_docker_available()?;

    // Build the instance first (this ensures it's up to date)
    crate::commands::build::run(instance_name.to_string()).await?;

    // Start the instance
    docker.start_instance(instance_name)?;

    // Get the instance configuration to show connection info
    let instance_config = project.config.get_instance(instance_name)?;
    let port = instance_config.port().unwrap_or(6969);

    print_success(&format!("Instance '{}' is now running", instance_name));
    println!("  Local URL: http://localhost:{}", port);
    println!(
        "  Container: helix_{}_{}",
        project.config.project.name, instance_name
    );
    println!(
        "  Data volume: {}",
        project.instance_volume(instance_name).display()
    );

    Ok(())
}

async fn push_cloud_instance(
    project: &ProjectContext,
    instance_name: &str,
    instance_config: InstanceInfo<'_>,
) -> Result<()> {
    print_status(
        "CLOUD",
        &format!("Deploying to cloud instance '{}'", instance_name),
    );

    let cluster_id = instance_config
        .cluster_id()
        .ok_or_else(|| eyre::eyre!("Cloud instance '{}' must have a cluster_id", instance_name))?;

    // Build the instance first to ensure Docker image exists
    crate::commands::build::run(instance_name.to_string()).await?;

    // TODO: Implement cloud deployment
    // This would involve:
    // 1. Reading compiled queries from the container directory
    // 2. Uploading them to the cloud cluster
    // 3. Triggering deployment on the cloud

    let config = project.config.cloud.get(instance_name).unwrap();
    match config {
        CloudConfig::FlyIo(config) => {
            let fly = FlyManager::new(project, config.auth_type.clone()).await?;
            let docker = DockerManager::new(project);
            // Get the correct image name from docker compose project name
            let image_name = docker.image_name(instance_name, config.build_mode);

            fly.deploy_image(&docker, &config, instance_name, &image_name)
                .await?;
        }
        CloudConfig::Ecr(config) => {
            let ecr = EcrManager::new(project, config.auth_type.clone()).await?;
            let docker = DockerManager::new(project);
            // Get the correct image name from docker compose project name
            let image_name = docker.image_name(instance_name, config.build_mode);

            ecr.deploy_image(&docker, &config, instance_name, &image_name, None)
                .await?;
        }
        CloudConfig::HelixCloud(_config) => {
            todo!()
        }
    }

    print_status("UPLOAD", &format!("Uploading to cluster: {}", cluster_id));
  

    Ok(())
}
