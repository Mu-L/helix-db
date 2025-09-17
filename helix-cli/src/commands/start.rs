use crate::commands::integrations::fly::FlyManager;
use crate::config::InstanceInfo;
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::{print_error, print_status, print_success};
use eyre::Result;

pub async fn run(instance_name: String) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;

    // Get instance config
    let instance_config = project.config.get_instance(&instance_name)?;

    if instance_config.is_local() {
        start_local_instance(&project, &instance_name).await
    } else {
        start_cloud_instance(&project, &instance_name, instance_config).await
    }
}

async fn start_local_instance(project: &ProjectContext, instance_name: &str) -> Result<()> {
    print_status(
        "START",
        &format!("Starting local instance '{instance_name}'"),
    );

    let docker = DockerManager::new(project);

    // Check Docker availability
    DockerManager::check_docker_available()?;

    // Check if instance is built (has docker-compose.yml)
    let workspace = project.instance_workspace(instance_name);
    let compose_file = workspace.join("docker-compose.yml");

    if !compose_file.exists() {
        print_error(&format!(
            "Instance '{instance_name}' has not been built yet"
        ));
        println!("  Run 'helix build {instance_name}' first to build the instance.");
        return Err(eyre::eyre!("Instance '{instance_name}' not built"));
    }

    // Start the instance
    docker.start_instance(instance_name)?;

    // Get the instance configuration to show connection info
    let instance_config = project.config.get_instance(instance_name)?;
    let port = instance_config.port().unwrap_or(6969);

    print_success(&format!("Instance '{instance_name}' is now running"));
    println!("  Local URL: http://localhost:{port}");
    let project_name = &project.config.project.name;
    println!("  Container: helix_{project_name}_{instance_name}");
    println!(
        "  Data volume: {}",
        project.instance_volume(instance_name).display()
    );

    Ok(())
}

async fn start_cloud_instance(
    project: &ProjectContext,
    instance_name: &str,
    instance_config: InstanceInfo<'_>,
) -> Result<()> {
    print_status(
        "CLOUD",
        &format!("Starting cloud instance '{instance_name}'"),
    );

    let cluster_id = instance_config
        .cluster_id()
        .ok_or_else(|| eyre::eyre!("Cloud instance '{instance_name}' must have a cluster_id"))?;

    // TODO: Implement cloud instance start
    // This would involve:
    // 1. Connecting to the cloud API
    // 2. Starting the instance on the specified cluster
    // 3. Waiting for the instance to be ready

    print_status(
        "STARTING",
        &format!("Starting instance on cluster: {cluster_id}"),
    );

    match project.config.get_instance(instance_name).unwrap() {
        InstanceInfo::FlyIo(config) => {
            let fly = FlyManager::new(project, config.auth_type.clone()).await?;
            fly.start_instance(instance_name).await?;
        }
        InstanceInfo::HelixCloud(_config) => {
            todo!()
        }
        _ => {
            unimplemented!()
        }
    }

    Ok(())
}
