use eyre::Result;
use crate::commands::integrations::fly::FlyManager;
use crate::config::{CloudConfig, InstanceInfo};
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::{print_status, print_success, print_warning, print_error};

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
    print_status("START", &format!("Starting local instance '{}'", instance_name));
    
    let docker = DockerManager::new(project);
    
    // Check Docker availability
    DockerManager::check_docker_available()?;
    
    // Check if instance is built (has docker-compose.yml)
    let workspace = project.instance_workspace(instance_name);
    let compose_file = workspace.join("docker-compose.yml");
    
    if !compose_file.exists() {
        print_error(&format!("Instance '{}' has not been built yet", instance_name));
        println!("  Run 'helix build {}' first to build the instance.", instance_name);
        return Err(eyre::eyre!("Instance '{}' not built", instance_name));
    }
    
    // Start the instance
    docker.start_instance(instance_name)?;
    
    // Get the instance configuration to show connection info
    let instance_config = project.config.get_instance(instance_name)?;
    let port = instance_config.port().unwrap_or(6969);
    
    print_success(&format!("Instance '{}' is now running", instance_name));
    println!("  Local URL: http://localhost:{}", port);
    println!("  Container: helix_{}_{}", project.config.project.name, instance_name);
    println!("  Data volume: {}", project.instance_volume(instance_name).display());
    
    Ok(())
}

async fn start_cloud_instance(project: &ProjectContext, instance_name: &str, instance_config: InstanceInfo<'_>) -> Result<()> {
    print_status("CLOUD", &format!("Starting cloud instance '{}'", instance_name));
    
    let cluster_id = instance_config.cluster_id()
        .ok_or_else(|| eyre::eyre!("Cloud instance '{}' must have a cluster_id", instance_name))?;
    
    // TODO: Implement cloud instance start
    // This would involve:
    // 1. Connecting to the cloud API
    // 2. Starting the instance on the specified cluster
    // 3. Waiting for the instance to be ready
    
    print_status("STARTING", &format!("Starting instance on cluster: {}", cluster_id));
   
    match project.config.get_instance(instance_name).unwrap() {
        InstanceInfo::FlyIo(config) => {
            let fly = FlyManager::new(project, config.auth_type.clone()).await?;
            fly.start_instance(instance_name).await?;
        }
        InstanceInfo::HelixCloud(config) => {
            todo!()
        }
        _ => { unimplemented!() }
    }
    
    Ok(())
}