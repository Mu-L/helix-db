use eyre::Result;
use crate::project::ProjectContext;
use crate::utils::{print_status, print_success, print_error};
use std::path::Path;

pub async fn run(instance: Option<String>) -> Result<()> {
    // Load project context
    let project = ProjectContext::find_and_load(None)?;
    
    match instance {
        Some(instance_name) => check_instance(&project, &instance_name).await,
        None => check_all_instances(&project).await,
    }
}

async fn check_instance(project: &ProjectContext, instance_name: &str) -> Result<()> {
    print_status("CHECK", &format!("Checking instance '{}'", instance_name));
    
    // Validate instance exists in config
    let _instance_config = project.config.get_instance(instance_name)?;
    
    // Check project files
    check_project_files(&project.root)?;
    
    // TODO: Validate queries and schema syntax
    // This would use the helix-db crate to parse and validate
    
    print_success(&format!("Instance '{}' configuration is valid", instance_name));
    Ok(())
}

async fn check_all_instances(project: &ProjectContext) -> Result<()> {
    print_status("CHECK", "Checking all instances");
    
    // Check project files
    check_project_files(&project.root)?;
    
    // Check each instance
    for instance_name in project.config.list_instances() {
        print_status("CHECK", &format!("Validating instance '{}'", instance_name));
        let _instance_config = project.config.get_instance(instance_name)?;
    }
    
    print_success("All instances are valid");
    Ok(())
}

fn check_project_files(project_root: &Path) -> Result<()> {
    let schema_path = project_root.join("schema.hx");
    let queries_path = project_root.join("queries.hx");
    
    if !schema_path.exists() {
        print_error("schema.hx not found");
        return Err(eyre::eyre!("Missing schema.hx file"));
    }
    
    if !queries_path.exists() {
        print_error("queries.hx not found");
        return Err(eyre::eyre!("Missing queries.hx file"));
    }
    
    // TODO: Parse and validate syntax
    
    Ok(())
}