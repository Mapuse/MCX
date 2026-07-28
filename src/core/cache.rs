use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, Duration};
use anyhow::{Result, Context};
use crate::core::constants;

pub struct CacheManager {
    cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            cache_dir: root.as_ref().join(constants::PATH_CACHE),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.cache_dir)
            .context("Failed to allocate global cache structural directories")
    }

    pub fn get_cached_package(&self, pkg_name: &str, version: &str) -> Option<PathBuf> {
        let expected_file = self.cache_dir.join(format!("{}-{}.tar.gz", pkg_name, version));
        if expected_file.exists() && expected_file.is_file() {
            Some(expected_file)
        } else {
            Option::None
        }
    }

    pub fn clean_all(&self) -> Result<()> {
        if !self.cache_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.cache_dir)
            .context("Failed to open cache directory for cleanup operations")?;

        for entry_result in entries {
            let entry = entry_result.context("Corrupted entry found during cache directory traversal")?;
            let path = entry.path();
            
            if path.is_dir() {
                fs::remove_dir_all(&path)
                    .with_context(|| format!("Failed to sweep directory block from cache: {:?}", path))?;
            } else {
                fs::remove_file(&path)
                    .with_context(|| format!("Failed to purge asset node from cache: {:?}", path))?;
            }
        }
        Ok(())
    }

    pub fn prune_old_assets(&self, max_age: Duration) -> Result<()> {
        if !self.cache_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.cache_dir)
            .context("Failed to open cache directory for expiration auditing")?;

        let now = SystemTime::now();

        for entry_result in entries {
            let entry = entry_result.context("Corrupted entry found during cache expiration pass")?;
            let path = entry.path();
            
            let metadata = fs::metadata(&path)
                .with_context(|| format!("Failed to query filesystem metadata for cache entry: {:?}", path))?;

            let modified_time = metadata.modified()
                .context("Filesystem timeline metadata not accessible on this block")?;

            if let Ok(elapsed) = now.duration_since(modified_time) {
                if elapsed > max_age {
                    if path.is_dir() {
                        fs::remove_dir_all(&path)
                            .with_context(|| format!("Failed to evict expired structural directory from cache: {:?}", path))?;
                    } else {
                        fs::remove_file(&path)
                            .with_context(|| format!("Failed to evict expired leaf file from cache: {:?}", path))?;
                    }
                }
            }
        }
        Ok(())
    }
}