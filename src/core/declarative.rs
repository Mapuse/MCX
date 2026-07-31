use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use anyhow::{Result, anyhow};
use crate::core::arch::Architecture;

#[derive(Clone, Debug)]
pub struct SystemProfile {
    pub version: String,
    pub architecture: Architecture,
    pub packages: Vec<String>,
}

pub struct ProfileValidator;

impl ProfileValidator {
    pub fn load_profile<P: AsRef<Path>>(path: P) -> Result<SystemProfile> {
        let content = fs::read_to_string(&path)?;
        let profile = Self::parse_ini_profile(&content)?;
        Self::validate_blueprint(&profile)?;
        Ok(profile)
    }

    fn parse_ini_profile(content: &str) -> Result<SystemProfile> {
        let mut version = String::new();
        let mut architecture = Architecture::host();
        let mut packages = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') {
                continue;
            }
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();
                match key {
                    "version" => version = value.to_string(),
                    "architecture" => architecture = Architecture::from_str(value)?,
                    "packages" => {
                        packages = value.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    _ => {}
                }
            }
        }

        Ok(SystemProfile { version, architecture, packages })
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
