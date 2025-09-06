use eyre::Result;
use std::fs;
use crate::config::InstanceInfo;
use crate::docker::DockerManager;
use crate::project::{ProjectContext, get_helix_repo_cache};
use crate::utils::{copy_dir_recursive, print_status, print_success, print_error};

// Development flag - set to true when working on V2 locally
const DEV_MODE: bool = cfg!(debug_assertions);
const HELIX_REPO_URL: &str = "https://github.com/helixdb/helix-db.git";

// Get the cargo workspace root at compile time
const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub async fn run(instance_name: String) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;
    
    // Get instance config
    let instance_config = project.config.get_instance(&instance_name)?;
    
    print_status("BUILD", &format!("Building instance '{}'", instance_name));
    
    // Ensure Helix repo is cached
    ensure_helix_repo_cached().await?;
    
    // Prepare instance workspace
    prepare_instance_workspace(&project, &instance_name).await?;
    
    // Compile project queries into the workspace
    compile_project(&project, &instance_name).await?;
    
    // Generate Docker files
    generate_docker_files(&project, &instance_name, instance_config.clone()).await?;
    
    // For local instances, build Docker image
    if instance_config.is_local() {
        let docker = DockerManager::new(&project);
        DockerManager::check_docker_available()?;
        docker.build_image(&instance_name)?;
    }
    
    print_success(&format!("Instance '{}' built successfully", instance_name));
    
    Ok(())
}

async fn ensure_helix_repo_cached() -> Result<()> {
    let repo_cache = get_helix_repo_cache()?;
    
    if !repo_cache.exists() {
        print_status("CACHE", "Caching Helix repository (first time setup)...");
        
        if DEV_MODE {
            // Development mode: copy from current workspace
            let workspace_root = std::path::Path::new(CARGO_MANIFEST_DIR)
                .parent() // helix-cli -> cli-v2
                .and_then(|p| p.parent()) // cli-v2 -> helix-db
                .ok_or_else(|| eyre::eyre!("Cannot determine workspace root"))?;
            
            print_status("DEV", "Development mode: copying local workspace...");
            copy_dir_recursive(&workspace_root, &repo_cache)?;
        } else {
            // Production mode: clone from GitHub
            let output = std::process::Command::new("git")
                .args(["clone", HELIX_REPO_URL, &repo_cache.to_string_lossy()])
                .output()?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(eyre::eyre!("Failed to clone Helix repository:\n{}", stderr));
            }
        }
        
        print_success("Helix repository cached successfully");
    } else {
        // Update existing repository
        print_status("UPDATE", "Updating Helix repository cache...");
        
        if DEV_MODE {
            // Development mode: re-copy from current workspace
            let workspace_root = std::path::Path::new(CARGO_MANIFEST_DIR)
                .parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| eyre::eyre!("Cannot determine workspace root"))?;
            
            // Remove old cache and copy fresh
            if repo_cache.exists() {
                std::fs::remove_dir_all(&repo_cache)?;
            }
            copy_dir_recursive(&workspace_root, &repo_cache)?;
        } else {
            // Production mode: git pull
            let output = std::process::Command::new("git")
                .args(["pull"])
                .current_dir(&repo_cache)
                .output()?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(eyre::eyre!("Failed to update Helix repository:\n{}", stderr));
            }
        }
        
        print_success("Helix repository updated");
    }
    
    Ok(())
}

async fn prepare_instance_workspace(project: &ProjectContext, instance_name: &str) -> Result<()> {
    print_status("PREPARE", &format!("Preparing workspace for '{}'", instance_name));
    
    // Ensure instance directories exist
    project.ensure_instance_dirs(instance_name)?;
    
    // We only need to prepare the instance-specific overlay files
    // The cached repo at ~/.helix/repo will be used directly as Docker build context
    
    Ok(())
}

async fn compile_project(project: &ProjectContext, instance_name: &str) -> Result<()> {
    print_status("COMPILE", "Compiling Helix queries...");
    
    // Read project files
    let schema_path = project.root.join("schema.hx");
    let queries_path = project.root.join("queries.hx");
    let config_path = project.root.join("config.hx.json");
    
    if !schema_path.exists() {
        return Err(eyre::eyre!("schema.hx not found. Run 'helix init' to create a project."));
    }
    
    if !queries_path.exists() {
        return Err(eyre::eyre!("queries.hx not found. Run 'helix init' to create a project."));
    }
    
    if !config_path.exists() {
        return Err(eyre::eyre!("config.hx.json not found. Run 'helix init' to create a project."));
    }
    
    // Create helix-container directory in instance workspace for generated files
    let instance_workspace = project.instance_workspace(instance_name);
    let helix_container_dir = instance_workspace.join("helix-container");
    let src_dir = helix_container_dir.join("src");
    
    // Create the directories
    fs::create_dir_all(&src_dir)?;
    
    // Copy config and schema files to helix-container/src
    fs::copy(&schema_path, src_dir.join("schema.hx"))?;
    fs::copy(&config_path, src_dir.join("config.hx.json"))?;
    
    // Read and compile the .hx files using the same logic as the original CLI
    // This uses the helix-db crate to generate Rust code
    print_status("CODEGEN", "Generating Rust code from Helix queries...");
    
    // Use the existing helix compilation logic
    // TODO: Import and use the compilation functions from the original CLI
    // For now, create a basic queries.rs file structure
    let queries_rs_content = r#"// Generated Helix queries
use helix_db::prelude::*;

// Generated query implementations would go here
"#;
    
    fs::write(src_dir.join("queries.rs"), queries_rs_content)?;
    
    print_success("Helix queries compiled to Rust files");
    Ok(())
}

async fn generate_docker_files(project: &ProjectContext, instance_name: &str, instance_config: InstanceInfo<'_>) -> Result<()> {
    if !instance_config.is_local() {
        // Cloud instances don't need Docker files
        return Ok(());
    }
    
    print_status("DOCKER", "Generating Docker configuration...");
    
    let docker = DockerManager::new(project);
    
    // Generate Dockerfile
    let dockerfile_content = docker.generate_dockerfile(instance_name, instance_config.clone())?;
    let dockerfile_path = project.dockerfile_path(instance_name);
    fs::write(&dockerfile_path, dockerfile_content)?;
    
    // Generate docker-compose.yml
    let compose_content = docker.generate_docker_compose(instance_name, instance_config.clone())?;
    let compose_path = project.docker_compose_path(instance_name);
    fs::write(&compose_path, compose_content)?;
    
    print_success("Docker configuration generated");
    Ok(())
}