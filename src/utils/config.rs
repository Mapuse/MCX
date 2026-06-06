use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SystemConfig {
    pub root_dir: PathBuf,
    pub cache_limit_bytes: u64,
    pub allow_unverified_packages: bool,
    pub concurrent_downloads: usize,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("/"),
            cache_limit_bytes: 1024 * 1024 * 1024 * 5,
            allow_unverified_packages: false,
            concurrent_downloads: 4,
        }
    }
}

pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            config_path: root.as_ref().join("etc/mcx/config.json"),
        }
    }

    pub fn load(&self) -> Result<SystemConfig> {
        if !self.config_path.exists() {
            if let Some(parent) = self.config_path.parent() {
                fs::create_dir_all(parent)
                    .context("Failed to allocate default system layout directories for configuration mapping")?;
            }
            let default_config = SystemConfig::default();
            self.save(&default_config)?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(&self.config_path)
            .with_context(|| format!("Failed to read core configuration payload stream from: {:?}", self.config_path))?;

        let config: SystemConfig = serde_json::from_str(&content)
            .context("Core configuration file framework structural alignment failure")?;

        Ok(config)
    }

    pub fn save(&self, config: &SystemConfig) -> Result<()> {
        let serialized_payload = serde_json::to_string_pretty(config)
            .context("Failed to serialize target engine configuration state map")?;

        fs::write(&self.config_path, serialized_payload)
            .with_context(|| format!("Failed to commit system configuration changes back to disk storage: {:?}", self.config_path))?;

        Ok(())
    }
}