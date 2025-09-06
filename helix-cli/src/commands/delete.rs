use eyre::Result;
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::{print_status, print_success, print_warning};

pub async fn run(instance_name: String) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;
    
    // Validate instance exists
    let _instance_config = project.config.get_instance(&instance_name)?;
    
    print_warning(&format!("This will permanently delete instance '{}' and all its data!", instance_name));
    println!("  This action cannot be undone.");
    println!();
    
    // TODO: Add confirmation prompt in a real implementation
    // For now, just proceed with deletion
    
    print_status("DELETE", &format!("Deleting instance '{}'", instance_name));
    
    // Stop and remove Docker containers
    if DockerManager::check_docker_available().is_ok() {
        let docker = DockerManager::new(&project);
        let _ = docker.prune_instance(&instance_name, true);
    }
    
    // Remove instance workspace
    let workspace = project.instance_workspace(&instance_name);
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace)?;
    }
    
    // Remove instance volumes
    let volume = project.instance_volume(&instance_name);
    if volume.exists() {
        std::fs::remove_dir_all(&volume)?;
    }
    
    print_success(&format!("Instance '{}' deleted successfully", instance_name));
    
    Ok(())
}