use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageStatus {
    Installed,
    Available,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackageEntity {
    pub pkg_name: String,
    pub version: String,
    pub license: String,
    pub build_type: String,
    pub build_date: String,
    pub checksum: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default = "default_status")]
    pub status: PackageStatus,
}

fn default_status() -> PackageStatus {
    PackageStatus::Unknown
}

impl PackageEntity {
    pub fn matches_constraint(&self, operation: &str, target_version: &str) -> Result<bool> {
        let current_parts: Vec<u32> = self.version
            .split('.')
            .map(|s| s.parse::<u32>().unwrap_or(0))
            .collect();
            
        let target_parts: Vec<u32> = target_version
            .split('.')
            .map(|s| s.parse::<u32>().unwrap_or(0))
            .collect();

        match operation {
            "==" => Ok(current_parts == target_parts),
            ">=" => Ok(current_parts >= target_parts),
            "<=" => Ok(current_parts <= target_parts),
            ">" => Ok(current_parts > target_parts),
            "<" => Ok(current_parts < target_parts),
            _ => Err(anyhow!("Unsupported structural version evaluation operator: {}", operation)),
        }
    }

    pub fn compute_relative_file_paths(&self, root_prefix: &str) -> Vec<PathBuf> {
        let base_path = std::path::Path::new(root_prefix);
        self.files
            .iter()
            .map(|f| base_path.join(f))
            .collect()
    }
}