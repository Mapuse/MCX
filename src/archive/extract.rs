use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};

pub struct Extractor {
    root: PathBuf,
}

impl Extractor {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn extract_zstd_archive<P: AsRef<Path>>(&self, archive_path: P, dest_dir: &Path) -> Result<Vec<PathBuf>> {
        let file = fs::File::open(archive_path)?;
        let decoder = zstd::stream::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        let mut extracted_files = Vec::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();

            if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err(anyhow!("Structural hazard: Invalid path template detected"));
            }

            let destination = dest_dir.join(&path);
            
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }

            entry.unpack(&destination)?;
            extracted_files.push(path);
        }
        
        Ok(extracted_files)
    }

    pub fn verify_no_collisions(&self, files: &[PathBuf]) -> Result<()> {
        for file in files {
            let target = self.root.join(file);
            if target.exists() {
                return Err(anyhow!("File collision detected: {:?} already exists on the filesystem", target));
            }
        }
        Ok(())
    }
}