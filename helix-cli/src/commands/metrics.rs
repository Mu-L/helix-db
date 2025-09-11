use eyre::Result;
use crate::{
    MetricsAction, 
    metrics_sender::{load_metrics_config, save_metrics_config},
    utils::{print_status, print_success}
};

pub async fn run(action: MetricsAction) -> Result<()> {
    match action {
        MetricsAction::On => enable_metrics().await,
        MetricsAction::Off => disable_metrics().await,
        MetricsAction::Status => show_metrics_status().await,
    }
}

async fn enable_metrics() -> Result<()> {
    print_status("METRICS", "Enabling metrics collection");
    
    let mut config = load_metrics_config().unwrap_or_default();
    config.enabled = true;
    config.last_updated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    
    save_metrics_config(&config)?;
    
    print_success("Metrics collection enabled");
    println!("  Anonymous usage data will help improve Helix");
    
    Ok(())
}

async fn disable_metrics() -> Result<()> {
    print_status("METRICS", "Disabling metrics collection");
    
    let mut config = load_metrics_config().unwrap_or_default();
    config.enabled = false;
    config.last_updated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    
    save_metrics_config(&config)?;
    
    print_success("Metrics collection disabled");
    
    Ok(())
}

async fn show_metrics_status() -> Result<()> {
    let config = load_metrics_config().unwrap_or_default();
    
    println!("Metrics Status");
    println!("  Enabled: {}", if config.enabled { "Yes" } else { "No" });
    
    if let Some(user_id) = &config.user_id {
        println!("  User ID: {}", user_id);
    }
    
    let last_updated = std::time::UNIX_EPOCH + std::time::Duration::from_secs(config.last_updated);
    if let Ok(datetime) = last_updated.duration_since(std::time::UNIX_EPOCH) {
        println!("  Last updated: {} seconds ago", datetime.as_secs());
    }
    
    Ok(())
}