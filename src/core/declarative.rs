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

/// JSON representation of a system blueprint (see README "Declarative
/// profiles"): {"version": "1.0", "architecture": "native", "packages": [...]}
#[derive(Debug, serde::Deserialize)]
struct JsonProfile {
    version: String,
    #[serde(default)]
    architecture: Option<String>,
    #[serde(default)]
    packages: Vec<String>,
}

pub struct ProfileValidator;

impl ProfileValidator {
    pub fn load_profile<P: AsRef<Path>>(path: P) -> Result<SystemProfile> {
        let content = fs::read_to_string(&path)?;
        // Sniff the format: blueprints may be written as JSON or INI.
        let profile = if content.trim_start().starts_with('{') {
            Self::parse_json_profile(&content)?
        } else {
            Self::parse_ini_profile(&content)?
        };
        Self::validate_blueprint(&profile)?;
        Ok(profile)
    }

    fn parse_json_profile(content: &str) -> Result<SystemProfile> {
        let json: JsonProfile = serde_json::from_str(content)
            .map_err(|e| anyhow!("Invalid JSON profile: {}", e))?;
        let architecture = match &json.architecture {
            Some(a) => Architecture::from_str(a)?,
            None => Architecture::host(),
        };
        Ok(SystemProfile {
            version: json.version,
            architecture,
            packages: json.packages,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_json_profile() {
        let dir = std::env::temp_dir().join(format!("mcx_test_json_profile_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profile.json");
        fs::write(&path, r#"{"version": "1.0", "architecture": "amd64", "packages": ["a", "b"]}"#).unwrap();

        let profile = ProfileValidator::load_profile(&path).expect("load json profile");
        assert_eq!(profile.version, "1.0");
        assert_eq!(profile.packages, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(profile.architecture, Architecture::Amd64);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_ini_profile_still_supported() {
        let dir = std::env::temp_dir().join(format!("mcx_test_ini_profile_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profile.ini");
        fs::write(&path, "version = 2.0\narchitecture = native\npackages = a, b\n").unwrap();

        let profile = ProfileValidator::load_profile(&path).expect("load ini profile");
        assert_eq!(profile.version, "2.0");
        assert_eq!(profile.packages.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_architecture_string_rejected() {
        assert!("not an arch".parse::<Architecture>().is_err());
    }
}
