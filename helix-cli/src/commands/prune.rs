use eyre::Result;
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::{print_status, print_success};

pub async fn run(instance: Option<String>, all: bool) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;
    
    if all {
        prune_all_instances(&project).await
    } else if let Some(instance_name) = instance {
        prune_instance(&project, &instance_name).await
    } else {
        prune_unused_resources(&project).await
    }
}

async fn prune_instance(project: &ProjectContext, instance_name: &str) -> Result<()> {
    print_status("PRUNE", &format!("Pruning instance '{}'", instance_name));
    
    // Validate instance exists
    let _instance_config = project.config.get_instance(instance_name)?;
    
    // Check Docker availability
    if DockerManager::check_docker_available().is_ok() {
        let docker = DockerManager::new(project);
        docker.prune_instance(instance_name, true)?;
    }
    
    // Remove instance workspace (but keep volumes)
    let workspace = project.instance_workspace(instance_name);
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace)?;
    }
    
    print_success(&format!("Instance '{}' pruned successfully", instance_name));
    Ok(())
}

async fn prune_all_instances(project: &ProjectContext) -> Result<()> {
    print_status("PRUNE", "Pruning all instances");
    
    for instance_name in project.config.list_instances() {
        print_status("PRUNE", &format!("Pruning instance '{}'", instance_name));
        
        if DockerManager::check_docker_available().is_ok() {
            let docker = DockerManager::new(project);
            let _ = docker.prune_instance(instance_name, true);
        }
    }
    
    // Remove entire .helix directory
    if project.helix_dir.exists() {
        std::fs::remove_dir_all(&project.helix_dir)?;
    }
    
    print_success("All instances pruned successfully");
    Ok(())
}

async fn prune_unused_resources(project: &ProjectContext) -> Result<()> {
    print_status("PRUNE", "Pruning unused Docker resources");
    
    // Check Docker availability
    DockerManager::check_docker_available()?;
    
    // Use centralized docker command
    let docker = DockerManager::new(project);
    let output = docker.run_docker_command(&["system", "prune", "-f"])?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre::eyre!("Failed to prune Docker resources:\n{}", stderr));
    }
    
    print_success("Unused Docker resources pruned");
    Ok(())
}