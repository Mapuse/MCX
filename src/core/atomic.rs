use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Result, Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

use crate::core::metadata::RLineMetadata;










pub struct AtomicInstaller {
    root: PathBuf,
    packages_dir: PathBuf,
    active_dir: PathBuf,
}

impl AtomicInstaller {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        let root = root.as_ref().to_path_buf();
        let packages_dir = root.join("system/storage/packages");
        let active_dir = root.join("system/storage/active");
        
        Self {
            root,
            packages_dir,
            active_dir,
        }
    }

    
    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.packages_dir)
            .context("Failed to create packages directory")?;
        fs::create_dir_all(&self.active_dir)
            .context("Failed to create active directory")?;
        Ok(())
    }

    
    
    
    
    
    
    pub fn install(&self, xcs_path: &Path, meta: &RLineMetadata) -> Result<()> {
        self.initialize()?;

        
        let checksum = self.compute_file_checksum(xcs_path)?;
        
        
        let capsule_name = format!("{}-{}-{}.xcs", meta.pkg_name, meta.version, &checksum[..16]);
        let capsule_path = self.packages_dir.join(&capsule_name);
        
        
        if !capsule_path.exists() {
            fs::copy(xcs_path, &capsule_path)
                .with_context(|| format!("Failed to copy capsule to {}", capsule_path.display()))?;
        }

        
        let active_link = self.active_dir.join(&meta.pkg_name);
        if active_link.exists() {
            fs::remove_file(&active_link)?;
        }
        
        
        let rel_target = format!("../../packages/{}", capsule_name);
        symlink(&rel_target, &active_link)
            .with_context(|| format!("Failed to create active symlink for {}", meta.pkg_name))?;

        
        self.record_generation(meta, &capsule_name)?;

        Ok(())
    }

    
    
    pub fn rollback(&self, pkg_name: &str, generation: u64) -> Result<()> {
        let gen_dir = self.root.join("var/lib/mcx/generations").join(pkg_name);
        let gen_file = gen_dir.join(format!("{}.json", generation));
        
        if !gen_file.exists() {
            bail!("Generation {} for package '{}' not found", generation, pkg_name);
        }

        
        let content = fs::read_to_string(&gen_file)?;
        let gen_info: GenerationInfo = serde_json::from_str(&content)?;

        
        let active_link = self.active_dir.join(pkg_name);
        if active_link.exists() {
            fs::remove_file(&active_link)?;
        }

        let rel_target = format!("../../packages/{}", gen_info.capsule_name);
        symlink(&rel_target, &active_link)
            .with_context(|| format!("Failed to rollback symlink for {}", pkg_name))?;

        Ok(())
    }

    
    pub fn get_active_capsule(&self, pkg_name: &str) -> Result<Option<PathBuf>> {
        let active_link = self.active_dir.join(pkg_name);
        if !active_link.exists() {
            return Ok(None);
        }

        let target = fs::read_link(&active_link)?;
        let resolved = self.root.join("system/storage").join(&target);
        
        if resolved.exists() {
            Ok(Some(resolved))
        } else {
            Ok(None)
        }
    }

    
    pub fn list_generations(&self, pkg_name: &str) -> Result<Vec<u64>> {
        let gen_dir = self.root.join("var/lib/mcx/generations").join(pkg_name);
        if !gen_dir.exists() {
            return Ok(Vec::new());
        }

        let mut gens: Vec<u64> = fs::read_dir(&gen_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|s| s.strip_suffix(".json"))
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .collect();
        
        gens.sort_unstable();
        Ok(gens)
    }

    
    pub fn current_generation(&self, pkg_name: &str) -> Result<Option<u64>> {
        let active_link = self.active_dir.join(pkg_name);
        if !active_link.exists() {
            return Ok(None);
        }

        
        let target = fs::read_link(&active_link)?;
        let capsule_name = target.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid symlink target"))?;

        
        let gen_dir = self.root.join("var/lib/mcx/generations").join(pkg_name);
        if !gen_dir.exists() {
            return Ok(None);
        }

        for entry in fs::read_dir(&gen_dir)? {
            let entry = entry?;
            let content = fs::read_to_string(entry.path())?;
            if let Ok(gen_info) = serde_json::from_str::<GenerationInfo>(&content) {
                if gen_info.capsule_name == capsule_name {
                    return gen_info.generation_number.parse().map(Some).map_err(|_| {
                        anyhow::anyhow!("Invalid generation number")
                    });
                }
            }
        }

        Ok(None)
    }

    
    fn record_generation(&self, meta: &RLineMetadata, capsule_name: &str) -> Result<()> {
        let gen_dir = self.root.join("var/lib/mcx/generations").join(&meta.pkg_name);
        fs::create_dir_all(&gen_dir)?;

        let next_gen = self.next_generation_number(&gen_dir)?;
        let gen_file = gen_dir.join(format!("{}.json", next_gen));

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let gen_info = GenerationInfo {
            pkg_name: meta.pkg_name.clone(),
            version: meta.version.clone(),
            capsule_name: capsule_name.to_string(),
            generation_number: next_gen.to_string(),
            installed_at: timestamp,
            checksum: meta.checksum.clone(),
        };

        let json = serde_json::to_string_pretty(&gen_info)?;
        fs::write(&gen_file, json)?;

        Ok(())
    }

    fn next_generation_number(&self, gen_dir: &Path) -> Result<u64> {
        let mut max_gen = 0u64;
        for entry in fs::read_dir(gen_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(num_str) = name.strip_suffix(".json") {
                    if let Ok(num) = num_str.parse::<u64>() {
                        max_gen = max_gen.max(num);
                    }
                }
            }
        }
        Ok(max_gen + 1)
    }

    fn compute_file_checksum(&self, path: &Path) -> Result<String> {
        let data = fs::read(path)?;
        let hash = Sha256::digest(&data);
        Ok(format!("{:x}", hash))
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
struct GenerationInfo {
    pkg_name: String,
    version: String,
    capsule_name: String,
    generation_number: String,
    installed_at: u64,
    checksum: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_atomic_structure() {
        let temp = env::temp_dir().join("test_mcx_atomic");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let installer = AtomicInstaller::new(&temp);
        installer.initialize().unwrap();

        assert!(temp.join("system/storage/packages").exists());
        assert!(temp.join("system/storage/active").exists());

        let _ = fs::remove_dir_all(&temp);
    }
}