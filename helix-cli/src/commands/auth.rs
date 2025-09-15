use crate::{
    AuthAction,
    utils::{print_status, print_success, print_warning},
};
use eyre::Result;

pub async fn run(action: AuthAction) -> Result<()> {
    match action {
        AuthAction::Login => login().await,
        AuthAction::Logout => logout().await,
        AuthAction::CreateKey { cluster } => create_key(&cluster).await,
    }
}

async fn login() -> Result<()> {
    print_status("LOGIN", "Logging into Helix Cloud");

    // TODO: Implement cloud authentication
    // This would:
    // 1. Open browser for OAuth flow
    // 2. Handle callback and store credentials
    // 3. Save to ~/.helix/credentials.toml

    print_warning("Cloud authentication not yet implemented");
    println!("  This will open your browser to authenticate with Helix Cloud.");

    Ok(())
}

async fn logout() -> Result<()> {
    print_status("LOGOUT", "Logging out of Helix Cloud");

    // Remove credentials file
    let home = dirs::home_dir().ok_or_else(|| eyre::eyre!("Cannot find home directory"))?;
    let credentials_path = home.join(".helix").join("credentials.toml");

    if credentials_path.exists() {
        std::fs::remove_file(&credentials_path)?;
        print_success("Logged out successfully");
    } else {
        println!("[INFO] Not currently logged in");
    }

    Ok(())
}

async fn create_key(cluster: &str) -> Result<()> {
    print_status(
        "API_KEY",
        &format!("Creating API key for cluster: {}", cluster),
    );

    // TODO: Implement API key creation
    // This would:
    // 1. Authenticate with cloud
    // 2. Create new API key for specified cluster
    // 3. Display the key to the user

    print_warning("API key creation not yet implemented");
    println!("  This will create a new API key for cluster: {}", cluster);

    Ok(())
}
