use std::path::Path;
use std::process::Command;
use anyhow::{Result, Context, bail};
use serde::{Deserialize, Serialize};
use crate::core::database::PackageMetadata;



#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RLineMetadata {
    pub pkg_name: String,
    pub version: String,
    pub source: String,
    pub license: String,
    pub build_type: String,
    pub build_date: String,
    pub checksum: String,
    pub pkg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<RLineMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<Vec<String>>,
    pub profile: Vec<String>,
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub externals: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundled: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depsig: Option<String>,
    pub prefix: String,
}

impl RLineMetadata {
    
    
    pub fn read_from_xcs(xcs_path: &Path) -> Result<Self> {
        let output = Command::new("unsquashfs")
            .arg("-cat")
            .arg(xcs_path)
            .arg("metadata.json")
            .output()
            .context("Failed to execute unsquashfs - is squashfs-tools installed?")?;

        if !output.status.success() {
            bail!("Failed to read metadata.json from {:?}: {}", xcs_path, 
                  String::from_utf8_lossy(&output.stderr));
        }

        let json = String::from_utf8(output.stdout)
            .context("metadata.json is not valid UTF-8")?;
        
        let meta: RLineMetadata = serde_json::from_str(&json)
            .context("Failed to parse metadata.json from capsule")?;
        
        Ok(meta)
    }

    
    pub fn is_meta_package(&self) -> bool {
        self.pkg_type == "meta" || self.pkg_type == "bundle"
    }

    
    pub fn get_all_components(&self) -> Vec<&RLineMetadata> {
        let mut result = Vec::new();
        if let Some(ref components) = self.components {
            for comp in components {
                result.push(comp);
                result.extend(comp.get_all_components());
            }
        }
        result
    }

    
    pub fn verify_depsig(&self) -> Result<bool> {
        match &self.depsig {
            None => Ok(true), 
            Some(expected_sig) => {
                let computed = self.compute_depsig()?;
                Ok(computed == *expected_sig)
            }
        }
    }

    
    pub fn compute_depsig(&self) -> Result<String> {
        let empty_vec = Vec::new();
        let externals = self.externals.as_ref().unwrap_or(&empty_vec);
        let mut sorted = externals.clone();
        sorted.sort();
        let joined = sorted.join("|");
        
        use sha2::{Sha256, Digest};
        let hash = Sha256::digest(joined.as_bytes());
        Ok(format!("{:x}", hash))
    }

    
    pub fn to_legacy_metadata(&self) -> PackageMetadata {
        PackageMetadata {
            pkg_name: self.pkg_name.clone(),
            version: self.version.clone(),
            license: self.license.clone(),
            source: self.source.clone(),
            checksum: crate::core::database::ChecksumData {
                kind: "sha256".to_string(),
                value: self.checksum.clone(),
            },
            dependencies: Vec::new(),
            files: Vec::new(),
            provides: None,
            conflicts: None,
            features: self.features.clone(),
        }
    }
}


pub struct MetadataIngestionEngine {
    system_db_path: std::path::PathBuf,
}

impl MetadataIngestionEngine {
    pub fn new<P: AsRef<std::path::Path>>(system_root: P) -> Self {
        Self {
            system_db_path: system_root.as_ref().join("system/storage/mcx/metadata.db"),
        }
    }

    
    pub fn ingest(&self, xcs_path: &Path) -> Result<RLineMetadata> {
        let meta = RLineMetadata::read_from_xcs(xcs_path)?;
        
        
        if !meta.verify_depsig()? {
            bail!("Dependency signature verification failed for {}", meta.pkg_name);
        }

        
        self.store_in_db(&meta)?;

        
        if meta.is_meta_package() {
            if let Some(ref components) = meta.components {
                for comp in components {
                    
                    
                    eprintln!("MCX: Meta package contains component: {}", comp.pkg_name);
                }
            }
        }

        Ok(meta)
    }

    
    fn store_in_db(&self, meta: &RLineMetadata) -> Result<()> {
        use std::fs;
        use serde_json;
        
        if let Some(parent) = self.system_db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        
        let mut db: Vec<RLineMetadata> = if self.system_db_path.exists() {
            let content = fs::read_to_string(&self.system_db_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        
        if let Some(pos) = db.iter().position(|m| m.pkg_name == meta.pkg_name && m.version == meta.version) {
            db[pos] = meta.clone();
        } else {
            db.push(meta.clone());
        }

        
        let json = serde_json::to_string_pretty(&db)?;
        fs::write(&self.system_db_path, json)?;

        Ok(())
    }

    
    pub fn query(&self, pkg_name: &str) -> Result<Option<RLineMetadata>> {
        use std::fs;
        use serde_json;
        
        if !self.system_db_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&self.system_db_path)?;
        let db: Vec<RLineMetadata> = serde_json::from_str(&content)?;
        
        Ok(db.into_iter().find(|m| m.pkg_name == pkg_name))
    }

    
    pub fn get_prefix(&self, xcs_path: &Path) -> Result<String> {
        let meta = RLineMetadata::read_from_xcs(xcs_path)?;
        Ok(meta.prefix)
    }

    
    pub fn get_depsig(&self, xcs_path: &Path) -> Result<Option<String>> {
        let meta = RLineMetadata::read_from_xcs(xcs_path)?;
        Ok(meta.depsig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depsig_computation() {
        let meta = RLineMetadata {
            pkg_name: "test".to_string(),
            version: "1.0".to_string(),
            source: "".to_string(),
            license: "MIT".to_string(),
            build_type: "make".to_string(),
            build_date: "2024-01-01".to_string(),
            checksum: "abc123".to_string(),
            pkg_type: "plain".to_string(),
            components: None,
            services: None,
            profile: vec!["isolated-rootfs".to_string()],
            features: vec!["lazy-mount".to_string()],
            externals: Some(vec!["libfoo.so".to_string(), "libbar.so".to_string()]),
            bundled: None,
            depsig: None,
            prefix: "system".to_string(),
        };

        let sig = meta.compute_depsig().unwrap();
        assert!(!sig.is_empty());
        assert_eq!(sig.len(), 64); 
    }
}