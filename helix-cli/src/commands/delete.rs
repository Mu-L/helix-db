use eyre::Result;
use std::io::{self, Write};
use crate::docker::DockerManager;
use crate::project::ProjectContext;
use crate::utils::{print_status, print_success, print_warning};

pub async fn run(instance_name: String) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;
    
    // Validate instance exists
    let _instance_config = project.config.get_instance(&instance_name)?;
    
    print_warning(&format!("This will permanently delete instance '{}' and ALL its data!", instance_name));
    println!("  - Docker containers and images");
    println!("  - Persistent volumes (databases, files)");
    println!("  This action cannot be undone.");
    println!();
    
    print!("Are you sure you want to delete instance '{}'? (y/N): ", instance_name);
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    if input.trim().to_lowercase() != "y" {
        println!("Deletion cancelled.");
        return Ok(());
    }
    
    print_status("DELETE", &format!("Deleting instance '{}'", instance_name));
    
    // Stop and remove Docker containers and volumes
    if DockerManager::check_docker_available().is_ok() {
        let docker = DockerManager::new(&project);
        
        // Remove containers and Docker volumes
        docker.prune_instance(&instance_name, true)?;
        
        // Remove Docker images
        docker.remove_instance_images(&instance_name)?;
    }
    
    // Remove instance workspace
    let workspace = project.instance_workspace(&instance_name);
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace)?;
        print_status("DELETE", "Removed workspace directory");
    }
    
    // Remove instance volumes (permanent data loss)
    let volume = project.instance_volume(&instance_name);
    if volume.exists() {
        std::fs::remove_dir_all(&volume)?;
        print_status("DELETE", "Removed persistent volumes");
    }
    
    print_success(&format!("Instance '{}' deleted successfully", instance_name));
    
    Ok(())
}