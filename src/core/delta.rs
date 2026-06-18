use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use tar::Archive;
use zstd::stream::Decoder;










pub struct DeltaReconstructor {
    _root: PathBuf,
}

impl DeltaReconstructor {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            _root: root.as_ref().to_path_buf(),
        }
    }

    
    
    
    
    
    
    pub fn reconstruct(
        &self,
        old_xcs: &Path,
        delta_xcd: &Path,
        output_xcd: &Path,
    ) -> Result<()> {
        eprintln!("MCX: Starting delta reconstruction...");
        eprintln!("  Old package: {:?}", old_xcs);
        eprintln!("  Delta patch: {:?}", delta_xcd);
        eprintln!("  Output: {:?}", output_xcd);

        
        let temp_dir = tempfile::tempdir()
            .context("Failed to create temporary directory for delta reconstruction")?;
        let work_dir = temp_dir.path().join("delta_work");
        fs::create_dir_all(&work_dir)?;

        
        eprintln!("MCX: Extracting old package...");
        let old_staging = work_dir.join("old");
        fs::create_dir_all(&old_staging)?;
        self.extract_xcs(old_xcs, &old_staging)?;

        
        eprintln!("MCX: Reading delta metadata...");
        let delta_meta = self.read_delta_metadata(delta_xcd)?;

        
        eprintln!("MCX: Applying delta patches...");
        self.apply_delta_patches(&old_staging, delta_xcd, &delta_meta)?;

        
        eprintln!("MCX: Processing removed files...");
        for removed in &delta_meta.meta.removed_files {
            let path = old_staging.join(removed);
            if path.exists() {
                if path.is_dir() {
                    fs::remove_dir_all(&path)?;
                } else {
                    fs::remove_file(&path)?;
                }
            }
        }

        
        eprintln!("MCX: Repackaging into new .xcs...");
        self.create_xcs(&old_staging, output_xcd)?;

        eprintln!("MCX: Delta reconstruction complete!");
        eprintln!("  Output: {:?}", output_xcd);

        Ok(())
    }

    
    fn extract_xcs(&self, xcs_path: &Path, dest: &Path) -> Result<()> {
        
        let squashfs_result = Command::new("unsquashfs")
            .arg("-d")
            .arg(dest)
            .arg("-f")
            .arg(xcs_path)
            .status();

        if let Ok(status) = squashfs_result {
            if status.success() {
                return Ok(());
            }
        }

        
        let file = fs::File::open(xcs_path)
            .with_context(|| format!("Failed to open {:?}", xcs_path))?;
        let decoder = Decoder::new(file)
            .context("Failed to create zstd decoder")?;
        let mut archive = Archive::new(decoder);
        archive.unpack(dest)
            .with_context(|| format!("Failed to unpack {:?}", xcs_path))?;

        Ok(())
    }

    
    fn read_delta_metadata(&self, delta_xcd: &Path) -> Result<DeltaMetadata> {
        let file = fs::File::open(delta_xcd)
            .with_context(|| format!("Failed to open delta file {:?}", delta_xcd))?;
        let decoder = Decoder::new(file)
            .context("Failed to create zstd decoder for delta")?;
        let mut archive = Archive::new(decoder);

        let mut meta = None;
        let mut patches = Vec::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();

            if path.file_name() == Some(std::ffi::OsStr::new("delta.meta")) {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                meta = Some(serde_json::from_str(&content)
                    .context("Failed to parse delta.meta")?);
            } else if path.starts_with("patches/") {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                patches.push((path, data));
            }
        }

        let meta = meta.ok_or_else(|| anyhow::anyhow!("Delta file missing delta.meta"))?;

        Ok(DeltaMetadata {
            meta,
            patches,
        })
    }

    
    fn apply_delta_patches(
        &self,
        staging: &Path,
        _delta_xcd: &Path,
        delta_meta: &DeltaMetadata,
    ) -> Result<()> {
        
        
        
        
        for (patch_path, patch_data) in &delta_meta.patches {
            
            let rel_path = patch_path.strip_prefix("patches/")
                .map_err(|_| anyhow::anyhow!("Invalid patch path"))?;
            let target = staging.join(rel_path);

            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }

            
            fs::write(&target, patch_data)?;
        }

        
        for modified in &delta_meta.meta.modified_files {
            let target = staging.join(modified);
            if target.exists() {
                
                
                eprintln!("MCX: Modified: {}", modified);
            }
        }

        Ok(())
    }

    
    fn create_xcs(&self, staging: &Path, output: &Path) -> Result<()> {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }

        
        if output.exists() {
            fs::remove_file(output)?;
        }

        
        let status = Command::new("mksquashfs")
            .arg(staging)
            .arg(output)
            .arg("-comp")
            .arg("zstd")
            .arg("-Xcompression-level")
            .arg("3")
            .arg("-noappend")
            .status()
            .context("Failed to execute mksquashfs")?;

        if !status.success() {
            bail!("mksquashfs failed to create package");
        }

        Ok(())
    }

    
    pub fn verify_delta(&self, delta_xcd: &Path, expected_hash: &str) -> Result<bool> {
        let data = fs::read(delta_xcd)?;
        let hash = Sha256::digest(&data);
        let computed = format!("{:x}", hash);
        Ok(computed == expected_hash)
    }

    
    pub fn get_delta_info(&self, delta_xcd: &Path) -> Result<DeltaInfo> {
        let meta = self.read_delta_metadata(delta_xcd)?;
        
        let delta_size = fs::metadata(delta_xcd)?.len();
        
        Ok(DeltaInfo {
            pkg_name: meta.meta.pkg_name,
            from_version: meta.meta.from_version,
            to_version: meta.meta.to_version,
            delta_size,
            modified_files: meta.meta.modified_files.len(),
            removed_files: meta.meta.removed_files.len(),
            added_files: meta.meta.added_files.len(),
        })
    }
}


#[derive(Debug, Clone, Deserialize, Serialize)]
struct DeltaMetadata {
    meta: DeltaFileMeta,
    patches: Vec<(PathBuf, Vec<u8>)>,
}


#[derive(Debug, Clone, Deserialize, Serialize)]
struct DeltaFileMeta {
    pub pkg_name: String,
    pub from_version: String,
    pub to_version: String,
    pub base_checksum: String,
    pub target_checksum: String,
    #[serde(default)]
    pub modified_files: Vec<String>,
    #[serde(default)]
    pub removed_files: Vec<String>,
    #[serde(default)]
    pub added_files: Vec<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaInfo {
    pub pkg_name: String,
    pub from_version: String,
    pub to_version: String,
    pub delta_size: u64,
    pub modified_files: usize,
    pub removed_files: usize,
    pub added_files: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_delta_structure() {
        let temp = env::temp_dir().join("test_mcx_delta");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let _reconstructor = DeltaReconstructor::new(&temp);
        
        let meta = DeltaFileMeta {
            pkg_name: "test".to_string(),
            from_version: "1.0".to_string(),
            to_version: "1.1".to_string(),
            base_checksum: "abc123".to_string(),
            target_checksum: "def456".to_string(),
            modified_files: vec!["bin/app".to_string()],
            removed_files: vec![],
            added_files: vec![],
        };

        assert_eq!(meta.from_version, "1.0");
        assert_eq!(meta.to_version, "1.1");

        let _ = fs::remove_dir_all(&temp);
    }
}