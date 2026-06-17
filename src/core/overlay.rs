use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use walkdir::WalkDir;






pub struct CasOverlayEngine {
    root: PathBuf,
    cas_root: PathBuf,
    shared_libs_dir: PathBuf,
}

impl CasOverlayEngine {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        let root = root.as_ref().to_path_buf();
        let cas_root = root.join("system/storage/shared_libs");
        let shared_libs_dir = root.join("system/storage/cas");
        
        Self {
            root,
            cas_root,
            shared_libs_dir,
        }
    }

    
    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.cas_root)
            .context("Failed to create CAS root directory")?;
        fs::create_dir_all(&self.shared_libs_dir)
            .context("Failed to create shared libs directory")?;
        Ok(())
    }

    
    
    
    
    
    
    
    
    pub fn deduplicate_package(&self, pkg_staging: &Path) -> Result<u64> {
        self.initialize()?;
        let mut saved_bytes = 0u64;

        let lib_dir = pkg_staging.join("system/lib");
        if !lib_dir.exists() {
            return Ok(0);
        }

        for entry in WalkDir::new(&lib_dir).min_depth(1).max_depth(1) {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            
            if !Self::is_shared_library(path) {
                continue;
            }

            let hash = self.compute_file_hash(path)?;
            let cas_subdir = self.cas_root.join(&hash[..2]);
            let cas_path = cas_subdir.join(&hash);

            if cas_path.exists() {
                
                let original_size = fs::metadata(path)?.len();
                fs::remove_file(path)?;
                fs::hard_link(&cas_path, path)
                    .with_context(|| format!("Failed to hard link to CAS: {:?}", path))?;
                saved_bytes += original_size;
            } else {
                
                fs::create_dir_all(&cas_subdir)?;
                fs::copy(path, &cas_path)
                    .with_context(|| format!("Failed to copy to CAS: {:?}", path))?;
            }
        }

        Ok(saved_bytes)
    }

    
    
    
    
    
    
    
    
    
    pub fn create_overlay_mount(
        &self,
        capsule_path: &Path,
        mount_point: &Path,
        prefix: &str,
    ) -> Result<OverlayMount> {
        fs::create_dir_all(mount_point)?;

        let upper_dir = self.root.join("var/tmp/mcx/overlay/upper")
            .join(format!("{}", mount_point.file_name().unwrap().to_str().unwrap()));
        let work_dir = self.root.join("var/tmp/mcx/overlay/work")
            .join(format!("{}", mount_point.file_name().unwrap().to_str().unwrap()));

        fs::create_dir_all(&upper_dir)?;
        fs::create_dir_all(&work_dir)?;

        
        let squashfs_mount = self.mount_squashfs(capsule_path, prefix)?;

        
        let lowerdir = squashfs_mount.display().to_string();
        let upperdir = upper_dir.display().to_string();
        let workdir = work_dir.display().to_string();

        let mount_opts = format!(
            "lowerdir={},upperdir={},workdir={}",
            lowerdir, upperdir, workdir
        );

        
        let status = Command::new("mount")
            .arg("-t")
            .arg("overlay")
            .arg("overlay")
            .arg(mount_point)
            .arg("-o")
            .arg(&mount_opts)
            .status()
            .context("Failed to execute mount command")?;

        if !status.success() {
            bail!("OverlayFS mount failed for {:?}", mount_point);
        }

        Ok(OverlayMount {
            mount_point: mount_point.to_path_buf(),
            _squashfs_mount: squashfs_mount,
            upper_dir,
            work_dir,
        })
    }

    
    pub fn unmount_overlay(mount_point: &Path) -> Result<()> {
        let status = Command::new("umount")
            .arg(mount_point)
            .status()
            .context("Failed to execute umount command")?;

        if !status.success() {
            bail!("Failed to unmount {:?}", mount_point);
        }

        Ok(())
    }

    
    fn mount_squashfs(&self, capsule_path: &Path, prefix: &str) -> Result<PathBuf> {
        let mount_point = self.root.join("var/tmp/mcx/squashfs")
            .join(format!("{}-{}", 
                capsule_path.file_stem().unwrap().to_str().unwrap(),
                prefix
            ));

        fs::create_dir_all(&mount_point)?;

        let status = Command::new("mount")
            .arg("-t")
            .arg("squashfs")
            .arg("-o")
            .arg("ro")
            .arg(capsule_path)
            .arg(&mount_point)
            .status()
            .context("Failed to mount SquashFS")?;

        if !status.success() {
            bail!("SquashFS mount failed for {:?}", capsule_path);
        }

        Ok(mount_point)
    }

    
    pub fn library_exists_in_cas(&self, _library_name: &str, expected_hash: &str) -> Result<bool> {
        let cas_subdir = self.cas_root.join(&expected_hash[..2]);
        let cas_path = cas_subdir.join(expected_hash);
        Ok(cas_path.exists())
    }

    
    pub fn get_cas_stats(&self) -> Result<CasStats> {
        if !self.cas_root.exists() {
            return Ok(CasStats::default());
        }

        let mut total_files = 0u64;
        let mut total_bytes = 0u64;

        for entry in WalkDir::new(&self.cas_root).min_depth(2).max_depth(2) {
            let entry = entry?;
            if entry.path().is_file() {
                total_files += 1;
                total_bytes += fs::metadata(entry.path())?.len();
            }
        }

        Ok(CasStats {
            total_files,
            total_bytes,
            dedup_ratio: 1.0, 
        })
    }

    
    fn compute_file_hash(&self, path: &Path) -> Result<String> {
        let data = fs::read(path)?;
        let hash = Sha256::digest(&data);
        Ok(format!("{:x}", hash))
    }

    
    fn is_shared_library(path: &Path) -> bool {
        if let Some(ext) = path.extension() {
            if ext == "so" {
                return true;
            }
        }
        
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        filename.contains(".so.")
    }
}


pub struct OverlayMount {
    pub mount_point: PathBuf,
    _squashfs_mount: PathBuf,
    upper_dir: PathBuf,
    work_dir: PathBuf,
}

impl Drop for OverlayMount {
    fn drop(&mut self) {
        
        let _ = CasOverlayEngine::unmount_overlay(&self.mount_point);
        let _ = fs::remove_dir_all(&self.upper_dir);
        let _ = fs::remove_dir_all(&self.work_dir);
    }
}


#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CasStats {
    pub total_files: u64,
    pub total_bytes: u64,
    pub dedup_ratio: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_cas_structure() {
        let temp = env::temp_dir().join("test_mcx_cas");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = CasOverlayEngine::new(&temp);
        engine.initialize().unwrap();

        assert!(temp.join("system/storage/shared_libs").exists());
        assert!(temp.join("system/storage/cas").exists());

        let _ = fs::remove_dir_all(&temp);
    }
}