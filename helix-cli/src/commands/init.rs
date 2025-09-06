use eyre::Result;
use std::env;
use std::fs;
use std::path::Path;
use crate::config::HelixConfig;
use crate::utils::{print_success, print_status};
use crate::utils::{DeploymentType, Template};


pub async fn run(path: Option<String>, template: Option<Template>, deployment_type: Option<DeploymentType>) -> Result<()> {
    let project_dir = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => env::current_dir()?,
    };
    
    let project_name = project_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("helix-project");
    
    let config_path = project_dir.join("helix.toml");
    
    if config_path.exists() {
        return Err(eyre::eyre!("helix.toml already exists in {}", project_dir.display()));
    }
    
    print_status("INIT", &format!("Initializing Helix project: {}", project_name));
    
    // Create project directory if it doesn't exist
    fs::create_dir_all(&project_dir)?;
    
    // Create default helix.toml
    let config = HelixConfig::default_config(project_name);
    config.save_to_file(&config_path)?;
    
    // Create project structure
    create_project_structure(&project_dir)?;
    
    print_success(&format!("Helix project initialized in {}", project_dir.display()));
    println!();
    println!("Next steps:");
    println!("  1. Edit schema.hx to define your data model");
    println!("  2. Add queries to queries.hx");
    println!("  3. Run 'helix build dev' to compile your project");
    println!("  4. Run 'helix push dev' to start your development instance");
    
    Ok(())
}

fn create_project_structure(project_dir: &Path) -> Result<()> {
    // Create directories
    fs::create_dir_all(project_dir.join(".helix"))?;
    
    // Create default schema.hx with proper Helix syntax
    let default_schema = r#"// Start building your schema here.
//
// The schema is used to to ensure a level of type safety in your queries.
//
// The schema is made up of Node types, denoted by N::,
// and Edge types, denoted by E::
//
// Under the Node types you can define fields that
// will be stored in the database.
//
// Under the Edge types you can define what type of node
// the edge will connect to and from, and also the
// properties that you want to store on the edge.
//
// Example:
//
// N::User {
//     Name: String,
//     Label: String,
//     Age: I64,
//     IsAdmin: Boolean,
// }
//
// E::Knows {
//     From: User,
//     To: User,
//     Properties: {
//         Since: I64,
//     }
// }
"#;
    fs::write(project_dir.join("schema.hx"), default_schema)?;
    
    // Create default queries.hx with proper Helix query syntax
    let default_queries = r#"// Start writing your queries here.
//
// You can use the schema to help you write your queries.
//
// Queries take the form:
//     QUERY {query name}({input name}: {input type}) =>
//         {variable} <- {traversal}
//         RETURN {variable}
//
// Example:
//     QUERY GetUserFriends(user_id: String) =>
//         friends <- N<User>(user_id)::Out<Knows>
//         RETURN friends
//
//
// For more information on how to write queries,
// see the documentation at https://docs.helix-db.com
// or checkout our GitHub at https://github.com/HelixDB/helix-db
"#;
    fs::write(project_dir.join("queries.hx"), default_queries)?;
    
    // Create .gitignore
    let gitignore = r#".helix/
target/
*.log
"#;
    fs::write(project_dir.join(".gitignore"), gitignore)?;
    
    Ok(())
}

