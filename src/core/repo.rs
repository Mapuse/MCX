use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context, anyhow};
use crate::core::database::{PackageMetadata, RepositoryInfo};

pub struct RepositoryManager {
    config_file: PathBuf,
    sync_dir: PathBuf,
}

impl RepositoryManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            config_file: root.as_ref().join("etc/mcx/repo.json"),
            sync_dir: root.as_ref().join("var/lib/mcx/sync"),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.config_file.parent() {
            fs::create_dir_all(parent)
                .context("Failed to allocate workspace configuration directories for repositories")?;
        }
        fs::create_dir_all(&self.sync_dir)
            .context("Failed to allocate repository metadata sync runway")?;
        Ok(())
    }

    pub fn load_repositories(&self) -> Result<Vec<RepositoryInfo>> {
        if !self.config_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.config_file)
            .with_context(|| format!("Failed to read repository registry configurations: {:?}", self.config_file))?;

        let repos: Vec<RepositoryInfo> = serde_json::from_str(&content)
            .context("Repository mapping definitions failed schema structural validation")?;

        Ok(repos)
    }

    pub fn save_repositories(&self, repos: &[RepositoryInfo]) -> Result<()> {
        let serialized_payload = serde_json::to_string_pretty(repos)
            .context("Failed to serialize repository tracking metadata blocks")?;

        fs::write(&self.config_file, serialized_payload)
            .with_context(|| format!("Failed to write repository layout configuration back to disk: {:?}", self.config_file))?;

        Ok(())
    }

    pub fn get_local_index_path(&self, repo_name: &str) -> PathBuf {
        self.sync_dir.join(format!("{}.json", repo_name))
    }

    pub fn read_cached_index(&self, repo_name: &str) -> Result<Vec<PackageMetadata>> {
        let index_path = self.get_local_index_path(repo_name);
        if !index_path.exists() {
            return Err(anyhow!("Synchronized remote manifest index not found locally for: {}", repo_name));
        }

        let content = fs::read_to_string(&index_path)
            .with_context(|| format!("Failed to read synchronized storage index file stream: {:?}", index_path))?;

        let metadata: Vec<PackageMetadata> = serde_json::from_str(&content)
            .context("Cached index stream data allocation matched an invalid metadata schema layout")?;

        Ok(metadata)
    }
}