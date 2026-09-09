use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageStatus {
    Installed,
    Available,
    Unknown,
}

/// Flexible checksum field that accepts either a plain string (legacy) or
/// an `{"kind":"sha256","value":"<hash>"}` object (Outsider format).
/// Serializes to the structured object form.
mod flexible_checksum {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct ChecksumObj {
        kind: String,
        value: String,
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        match v {
            serde_json::Value::String(s) => Ok(s),
            serde_json::Value::Object(_) => {
                let kind = v.pointer("/kind").and_then(|x| x.as_str()).unwrap_or("sha256");
                let value = v.pointer("/value").and_then(|x| x.as_str()).unwrap_or("");
                if value.is_empty() {
                    Err(serde::de::Error::custom("checksum value is empty"))
                } else {
                    Ok(format!("{}:{}", kind, value))
                }
            }
            _ => Err(serde::de::Error::custom("checksum must be a string or {kind,value} object")),
        }
    }

    pub fn serialize<S>(checksum: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (kind, value) = if let Some(pos) = checksum.find(':') {
            (&checksum[..pos], &checksum[pos + 1..])
        } else {
            ("sha256", checksum)
        };
        ChecksumObj { kind: kind.to_string(), value: value.to_string() }.serialize(serializer)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackageEntity {
    pub pkg_name: String,
    pub version: String,
    pub license: String,
    pub build_type: String,
    pub build_date: String,
    #[serde(with = "flexible_checksum")]
    pub checksum: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default = "default_status")]
    pub status: PackageStatus,
    #[serde(default = "default_arch", alias = "arch")]
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

/// Canonical version comparison used across the codebase (install checks,
/// solver candidate ordering, constraint evaluation).
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
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

    /// Equivalence test for the consolidated implementation: the old
    /// numeric-tuple comparisons used by install/solver must behave like
    /// compare_versions for plain release versions.
    #[test]
    fn test_compare_versions_matches_legacy_semantics() {
        use std::cmp::Ordering;

        // Old solver/install semantics: split on non-digits, compare tuples,
        // shorter tuple that is a prefix compares as Less (no zero padding).
        fn legacy_parse(version: &str) -> Vec<u64> {
            version.trim_start_matches('v')
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|s| s.parse::<u64>().ok())
                .collect()
        }

        // Numeric ordering must match the old solver/install tuple compare
        // (prefix-equality cases like 1.0 vs 1.0.0 are intentionally improved
        // below via zero padding).
        let cases = [
            ("1.0", "1.0"), ("1.2", "1.10"),
            ("2.1", "1.9"), ("v3.4", "3.4"), ("1.2.3", "1.2.4"),
        ];
        for (a, b) in cases {
            assert_eq!(
                compare_versions(a, b),
                legacy_parse(a).cmp(&legacy_parse(b)),
                "compare_versions({a}, {b}) diverged from legacy semantics"
            );
        }

        // Zero-padding behaviour for equal versions.
        assert_eq!(compare_versions("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.2", "1.10"), Ordering::Less);
        assert_eq!(compare_versions("2.1", "1.9"), Ordering::Greater);
    }

    #[test]
    fn test_package_entity_accepts_both_arch_keys() {
        let canonical = r#"{
            "pkg_name": "test", "version": "1.0", "license": "MIT",
            "build_type": "static", "build_date": "2026-01-01",
            "checksum": {"kind": "sha256", "value": "abc"},
            "architecture": "x86_64"
        }"#;
        let p1: PackageEntity = serde_json::from_str(canonical).expect("architecture key");
        assert_eq!(p1.architecture, "x86_64");
        assert_eq!(p1.checksum, "sha256:abc");

        let legacy = r#"{
            "pkg_name": "test", "version": "1.0", "license": "MIT",
            "build_type": "static", "build_date": "2026-01-01",
            "checksum": {"kind": "sha256", "value": "abc"},
            "arch": "aarch64"
        }"#;
        let p2: PackageEntity = serde_json::from_str(legacy).expect("arch key");
        assert_eq!(p2.architecture, "aarch64");
        assert_eq!(p2.checksum, "sha256:abc");

        // Flat string checksum (legacy format) is also accepted
        let legacy_flat = r#"{
            "pkg_name": "test", "version": "1.0", "license": "MIT",
            "build_type": "static", "build_date": "2026-01-01",
            "checksum": "abc123"
        }"#;
        let p3: PackageEntity = serde_json::from_str(legacy_flat).expect("flat checksum");
        assert_eq!(p3.checksum, "abc123");
    }
}