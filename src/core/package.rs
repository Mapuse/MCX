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
    #[serde(default = "default_arch")]
    pub architecture: String,
}

fn default_arch() -> String {
    "native".to_string()
}

fn default_status() -> PackageStatus {
    PackageStatus::Unknown
}

impl PackageEntity {
    pub fn matches_constraint(&self, operation: &str, target_version: &str) -> Result<bool> {
        let ordering = compare_versions(&self.version, target_version);

        match operation {
            "==" => Ok(ordering == std::cmp::Ordering::Equal),
            ">=" => Ok(ordering != std::cmp::Ordering::Less),
            "<=" => Ok(ordering != std::cmp::Ordering::Greater),
            ">" => Ok(ordering == std::cmp::Ordering::Greater),
            "<" => Ok(ordering == std::cmp::Ordering::Less),
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum VersionSegment {
    Prerelease(u64, u8, String),
    Release(u64),
}

fn prerelease_rank(suffix: &str) -> (u8, String) {
    let clean: String = suffix
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_string();
    let rank = if clean.starts_with("alpha") || clean == "a" {
        0
    } else if clean.starts_with("beta") || clean == "b" {
        1
    } else if clean.starts_with("rc") || clean.starts_with("pre") || clean == "r" {
        2
    } else {
        3
    };
    (rank, clean)
}

fn parse_version(version: &str) -> Vec<VersionSegment> {
    version
        .trim_start_matches('v')
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                let (rank, suffix) = prerelease_rank(part);
                VersionSegment::Prerelease(0, rank, suffix)
            } else {
                let numeric = digits.parse::<u64>().unwrap_or(0);
                let rest = &part[digits.len()..];
                if rest.is_empty() {
                    VersionSegment::Release(numeric)
                } else {
                    let (rank, suffix) = prerelease_rank(rest);
                    VersionSegment::Prerelease(numeric, rank, suffix)
                }
            }
        })
        .collect()
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts = parse_version(a);
    let b_parts = parse_version(b);
    let len = a_parts.len().max(b_parts.len());
    for i in 0..len {
        let a_seg = a_parts.get(i).unwrap_or(&VersionSegment::Release(0));
        let b_seg = b_parts.get(i).unwrap_or(&VersionSegment::Release(0));
        match a_seg.cmp(b_seg) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(version: &str) -> PackageEntity {
        PackageEntity {
            pkg_name: "test-pkg".to_string(),
            version: version.to_string(),
            license: "MIT".to_string(),
            build_type: "static".to_string(),
            build_date: "2026-01-01".to_string(),
            checksum: "abc".to_string(),
            files: vec![],
            status: PackageStatus::Unknown,
            architecture: "native".to_string(),
        }
    }

    #[test]
    fn test_version_equality_pads_zeros() {
        assert!(pkg("1.2").matches_constraint("==", "1.2.0").expect("constraint eval"));
        assert!(pkg("1.2.0").matches_constraint("==", "1.2").expect("constraint eval"));
        assert!(pkg("1.2").matches_constraint("==", "1.2").expect("constraint eval"));
    }

    #[test]
    fn test_version_ordering() {
        assert!(pkg("1.2").matches_constraint("<", "1.3").expect("constraint eval"));
        assert!(pkg("1.10").matches_constraint(">", "1.9").expect("constraint eval"));
        assert!(pkg("2.0").matches_constraint(">=", "1.99").expect("constraint eval"));
        assert!(pkg("1.2").matches_constraint("<=", "1.2.0").expect("constraint eval"));
    }

    #[test]
    fn test_version_prerelease_ordering() {
        assert!(pkg("1.2.0-rc1").matches_constraint("<", "1.2.0").expect("constraint eval"));
        assert!(pkg("1.2.0-alpha").matches_constraint("<", "1.2.0-beta").expect("constraint eval"));
        assert!(pkg("1.2.0-beta").matches_constraint("<", "1.2.0-rc1").expect("constraint eval"));
        assert!(pkg("1.rc1").matches_constraint("<", "1.0").expect("constraint eval"));
    }
}