use std::collections::HashSet;
use std::fs;
use std::path::Path;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SystemProfile {
    pub version: String,
    pub architecture: String,
    pub packages: Vec<String>,
}

pub struct ProfileValidator;

impl ProfileValidator {
    pub fn load_profile<P: AsRef<Path>>(path: P) -> Result<SystemProfile> {
        let content = fs::read_to_string(&path)?;
        let profile: SystemProfile = serde_json::from_str(&content)?;
        Self::validate_blueprint(&profile)?;
        Ok(profile)
    }

    pub fn compile_profile_diff(current: &[String], target: &[String]) -> (Vec<String>, Vec<String>) {
        let current_set: HashSet<String> = current.iter().cloned().collect();
        let target_set: HashSet<String> = target.iter().cloned().collect();
        let to_install = target_set.difference(&current_set).cloned().collect();
        let to_remove = current_set.difference(&target_set).cloned().collect();
        (to_install, to_remove)
    }

    fn validate_blueprint(profile: &SystemProfile) -> Result<()> {
        if profile.version.trim().is_empty() {
            return Err(anyhow!("Profile version field is empty"));
        }
        if profile.architecture.trim().is_empty() {
            return Err(anyhow!("Target architecture context is undefined"));
        }
        let mut unique_packages = HashSet::new();
        for pkg in &profile.packages {
            if pkg.trim().is_empty() {
                return Err(anyhow!("Empty package token discovered inside target array"));
            }
            if !unique_packages.insert(pkg.clone()) {
                return Err(anyhow!("Profile declaration duplication detected for: {}", pkg));
            }
        }
        Ok(())
    }
}