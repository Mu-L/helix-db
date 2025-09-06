use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelixConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub local: HashMap<String, LocalInstanceConfig>,
    #[serde(default)]
    pub cloud: HashMap<String, CloudInstanceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalInstanceConfig {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default = "default_debug_build_mode")]
    pub build_mode: BuildMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInstanceConfig {
    pub cluster_id: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default = "default_release_build_mode")]
    pub build_mode: BuildMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildMode {
    Debug,
    Release,
}

fn default_debug_build_mode() -> BuildMode {
    BuildMode::Debug
}

fn default_release_build_mode() -> BuildMode {
    BuildMode::Release
}

#[derive(Debug, Clone)]
pub enum InstanceInfo<'a> {
    Local(&'a LocalInstanceConfig),
    Cloud(&'a CloudInstanceConfig),
}

impl<'a> InstanceInfo<'a> {
    pub fn build_mode(&self) -> &BuildMode {
        match self {
            InstanceInfo::Local(config) => &config.build_mode,
            InstanceInfo::Cloud(config) => &config.build_mode,
        }
    }
    
    pub fn port(&self) -> Option<u16> {
        match self {
            InstanceInfo::Local(config) => config.port,
            InstanceInfo::Cloud(_) => None,
        }
    }
    
    pub fn cluster_id(&self) -> Option<&str> {
        match self {
            InstanceInfo::Local(_) => None,
            InstanceInfo::Cloud(config) => Some(&config.cluster_id),
        }
    }
    
    pub fn is_local(&self) -> bool {
        matches!(self, InstanceInfo::Local(_))
    }
    
    pub fn is_cloud(&self) -> bool {
        matches!(self, InstanceInfo::Cloud(_))
    }
}

impl HelixConfig {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| eyre!("Failed to read helix.toml: {}", e))?;
        
        let config: HelixConfig = toml::from_str(&content)
            .map_err(|e| eyre!("Failed to parse helix.toml: {}", e))?;
        
        config.validate()?;
        Ok(config)
    }
    
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| eyre!("Failed to serialize helix.toml: {}", e))?;
        
        fs::write(path, content)
            .map_err(|e| eyre!("Failed to write helix.toml: {}", e))?;
        
        Ok(())
    }
    
    fn validate(&self) -> Result<()> {
        // Validate project config
        if self.project.name.is_empty() {
            return Err(eyre!("Project name cannot be empty"));
        }
        
        
        // Validate instances
        if self.local.is_empty() && self.cloud.is_empty() {
            return Err(eyre!("At least one instance must be defined"));
        }
        
        // Validate local instances
        for name in self.local.keys() {
            if name.is_empty() {
                return Err(eyre!("Instance name cannot be empty"));
            }
        }
        
        // Validate cloud instances
        for (name, cloud_config) in &self.cloud {
            if name.is_empty() {
                return Err(eyre!("Instance name cannot be empty"));
            }
            if cloud_config.cluster_id.is_empty() {
                return Err(eyre!("Cloud instance '{}' must have a non-empty cluster_id", name));
            }
        }
        
        Ok(())
    }
    
    pub fn get_instance(&self, name: &str) -> Result<InstanceInfo> {
        if let Some(local_config) = self.local.get(name) {
            return Ok(InstanceInfo::Local(local_config));
        }
        
        if let Some(cloud_config) = self.cloud.get(name) {
            return Ok(InstanceInfo::Cloud(cloud_config));
        }
        
        Err(eyre!("Instance '{}' not found in helix.toml", name))
    }
    
    pub fn list_instances(&self) -> Vec<&String> {
        let mut instances = Vec::new();
        instances.extend(self.local.keys());
        instances.extend(self.cloud.keys());
        instances
    }
    
    pub fn default_config(project_name: &str) -> Self {
        let mut local = HashMap::new();
        local.insert("dev".to_string(), LocalInstanceConfig {
            port: Some(6969),
            build_mode: BuildMode::Debug,
        });
        
        HelixConfig {
            project: ProjectConfig {
                name: project_name.to_string(),
            },
            local,
            cloud: HashMap::new(),
        }
    }
}