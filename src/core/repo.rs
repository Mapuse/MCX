use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context, anyhow};
use futures_util::future::join_all;
use crate::core::database::{PackageMetadata, RepositoryInfo};
use crate::network::download::Downloader;
use crate::archive::hash::HashVerifier;

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

    /// Adds a new repository
    pub fn add_repository(&self, repo: RepositoryInfo) -> Result<()> {
        let mut repos = self.load_repositories()?;
        if repos.iter().any(|r| r.name == repo.name) {
            return Err(anyhow!("Repository '{}' already exists", repo.name));
        }
        repos.push(repo);
        self.save_repositories(&repos)
    }

    /// Removes a repository by name
    pub fn remove_repository(&self, name: &str) -> Result<()> {
        let mut repos = self.load_repositories()?;
        let len_before = repos.len();
        repos.retain(|r| r.name != name);
        if repos.len() == len_before {
            return Err(anyhow!("Repository '{}' not found", name));
        }
        self.save_repositories(&repos)
    }

    /// Syncs all repositories in parallel, returns (repos_synced, errors)
    pub async fn sync_all_parallel(&self) -> Result<(usize, Vec<String>)> {
        let repos = self.load_repositories()?;
        if repos.is_empty() {
            return Ok((0, vec!["No repositories configured".into()]));
        }

        fs::create_dir_all(&self.sync_dir)?;
        let mut tasks = Vec::new();

        for repo in repos {
            let sync_dir = self.sync_dir.clone();
            let repo_name = repo.name.clone();
            let repo_url = repo.url.clone();
            let checksum = repo.checksum.clone();
            let downloader = Downloader::new();

            tasks.push(tokio::spawn(async move {
                let temp_path = sync_dir.join(format!("{}.tmp", repo_name));
                let final_path = sync_dir.join(format!("{}.json", repo_name));

                let result = match downloader.download_package(&repo_url, &temp_path).await {
                    Ok(_) => {
                        if let Some(ref expected_hash) = checksum {
                            if let Err(e) = HashVerifier::verify_integrity(&temp_path, expected_hash) {
                                let _ = fs::remove_file(&temp_path);
                                Err(format!("{}: checksum mismatch: {}", repo_name, e))
                            } else {
                                if let Err(e) = fs::rename(&temp_path, &final_path) {
                                    Err(format!("{}: rename failed: {}", repo_name, e))
                                } else {
                                    Ok(repo_name)
                                }
                            }
                        } else {
                            if let Err(e) = fs::rename(&temp_path, &final_path) {
                                Err(format!("{}: rename failed: {}", repo_name, e))
                            } else {
                                Ok(repo_name)
                            }
                        }
                    }
                    Err(e) => {
                        let _ = fs::remove_file(&temp_path);
                        Err(format!("{}: download failed: {}", repo_name, e))
                    }
                };
                result
            }));
        }

        let mut synced = 0usize;
        let mut errors = Vec::new();

        for result in join_all(tasks).await {
            match result {
                Ok(Ok(name)) => {
                    synced += 1;
                    println!("  Synced: {}", name);
                }
                Ok(Err(e)) => errors.push(format!("{}", e)),
                Err(e) => errors.push(format!("Join error: {}", e)),
            }
        }

        Ok((synced, errors))
    }

    /// Cross-repository package search: searches all synced indexes
    pub fn search_across_repos(&self, query: &str) -> Result<Vec<(String, PackageMetadata)>> {
        let mut results = Vec::new();
        if !self.sync_dir.exists() {
            return Ok(results);
        }

        let q = query.to_lowercase();
        for entry in fs::read_dir(&self.sync_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(pkgs) = serde_json::from_str::<Vec<PackageMetadata>>(&content) {
                        let repo_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
                        for pkg in pkgs {
                            if pkg.pkg_name.to_lowercase().contains(&q)
                                || pkg.version.to_lowercase().contains(&q)
                                || pkg.license.to_lowercase().contains(&q)
                            {
                                results.push((repo_name.to_string(), pkg));
                            }
                        }
                    }
                }
            }
        }
        Ok(results)
    }

    /// Cross-repository install resolution: returns all matching packages
    /// across all synced repos for a given package name
    pub fn resolve_across_repos(&self, pkg_name: &str) -> Result<Vec<PackageMetadata>> {
        let mut results = Vec::new();
        if !self.sync_dir.exists() {
            return Ok(results);
        }

        for entry in fs::read_dir(&self.sync_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(pkgs) = serde_json::from_str::<Vec<PackageMetadata>>(&content) {
                        for pkg in pkgs {
                            if pkg.pkg_name == pkg_name {
                                results.push(pkg);
                            }
                        }
                    }
                }
            }
        }
        Ok(results)
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