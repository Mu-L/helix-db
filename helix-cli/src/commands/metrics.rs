use eyre::Result;
use serde::{Serialize, Deserialize};
use crate::{MetricsAction, utils::{print_status, print_success}};
use dirs::home_dir;
use std::fs;

#[derive(Serialize, Deserialize)]
struct MetricsConfig {
    enabled: bool,
    user_id: Option<String>,
    last_updated: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            user_id: None,
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

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

fn get_metrics_config_path() -> Result<std::path::PathBuf> {
    let home = home_dir().ok_or_else(|| eyre::eyre!("Cannot find home directory"))?;
    let helix_dir = home.join(".helix");
    fs::create_dir_all(&helix_dir)?;
    Ok(helix_dir.join("metrics.toml"))
}

fn load_metrics_config() -> Result<MetricsConfig> {
    let config_path = get_metrics_config_path()?;
    
    if !config_path.exists() {
        return Ok(MetricsConfig::default());
    }
    
    let content = fs::read_to_string(&config_path)?;
    let config = toml::from_str(&content)?;
    Ok(config)
}

fn save_metrics_config(config: &MetricsConfig) -> Result<()> {
    let config_path = get_metrics_config_path()?;
    let content = toml::to_string_pretty(config)?;
    fs::write(&config_path, content)?;
    Ok(())
}