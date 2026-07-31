use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context, anyhow};
use futures_util::future::join_all;
use crate::core::constants;
use crate::core::arch::host_architecture;
use crate::core::database::{PackageMetadata, RepositoryInfo};
use crate::network::download::Downloader;
use crate::archive::hash::HashVerifier;
use crate::utils::ui::UserInterface;

pub struct RepositoryManager {
    config_file: PathBuf,
    sync_dir: PathBuf,
}

impl RepositoryManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            config_file: root.as_ref().join(constants::PATH_REPO_INI),
            sync_dir: root.as_ref().join(constants::PATH_SYNC),
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
            .with_context(|| format!("Failed to read repository registry: {:?}", self.config_file))?;
        let repos = Self::parse_ini(&content)?;
        Ok(repos)
    }

    fn parse_ini(content: &str) -> Result<Vec<RepositoryInfo>> {
        let mut repos = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_url: Option<String> = None;
        let mut current_checksum: Option<String> = None;
        let mut current_enabled = true;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                if let (Some(name), Some(url)) = (current_name.take(), current_url.take()) {
                    repos.push(RepositoryInfo {
                        name,
                        url,
                        checksum: current_checksum.take(),
                        enabled: current_enabled,
                    });
                }
                current_name = Some(line[1..line.len()-1].trim().to_string());
                current_url = None;
                current_checksum = None;
                current_enabled = true;
                continue;
            }
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim().to_string();
                match key {
                    "url" => current_url = Some(value),
                    "checksum" => current_checksum = Some(value),
                    "enabled" => current_enabled = value.to_lowercase() == "true" || value == "1",
                    _ => {}
                }
            }
        }

        if let (Some(name), Some(url)) = (current_name, current_url) {
            repos.push(RepositoryInfo { name, url, checksum: current_checksum, enabled: current_enabled });
        }

        Ok(repos)
    }

    fn format_ini(repos: &[RepositoryInfo]) -> String {
        let mut output = String::new();
        for repo in repos {
            output.push_str(&format!("[{}]\n", repo.name));
            output.push_str(&format!("url = {}\n", repo.url));
            output.push_str(&format!("enabled = {}\n", repo.enabled));
            output.push_str("priority = 100\n");
            if let Some(ref checksum) = repo.checksum {
                output.push_str(&format!("checksum = {}\n", checksum));
            }
            output.push('\n');
        }
        output
    }

    pub fn save_repositories(&self, repos: &[RepositoryInfo]) -> Result<()> {
        let payload = Self::format_ini(repos);
        fs::write(&self.config_file, payload)
            .with_context(|| format!("Failed to write repo config: {:?}", self.config_file))?;
        Ok(())
    }

    pub fn add_repository(&self, repo: RepositoryInfo) -> Result<()> {
        let mut repos = self.load_repositories()?;
        if repos.iter().any(|r| r.name == repo.name) {
            return Err(anyhow!("Repository '{}' already exists", repo.name));
        }
        repos.push(repo);
        self.save_repositories(&repos)
    }

    pub fn remove_repository(&self, name: &str) -> Result<()> {
        let mut repos = self.load_repositories()?;
        let len_before = repos.len();
        repos.retain(|r| r.name != name);
        if repos.len() == len_before {
            return Err(anyhow!("Repository '{}' not found", name));
        }
        self.save_repositories(&repos)
    }

    pub async fn sync_all_parallel(&self) -> Result<(usize, Vec<String>)> {
        let repos = self.load_repositories()?;
        let repos: Vec<_> = repos.into_iter().filter(|r| r.enabled).collect();
        if repos.is_empty() {
            return Ok((0, vec!["No repositories configured or enabled".into()]));
        }

        let host_arch = host_architecture();
        fs::create_dir_all(&self.sync_dir)?;
        let mut tasks = Vec::new();

        for repo in repos {
            let sync_dir = self.sync_dir.clone();
            let repo_name = repo.name.clone();
            let repo_url = repo.url.clone();
            let checksum = repo.checksum.clone();
            let arch = host_arch.clone();
            let downloader = Downloader::new();

            tasks.push(tokio::spawn(async move {
                let temp_path = sync_dir.join(format!("{}.tmp", repo_name));
                let final_path = sync_dir.join(format!("{}.json", repo_name));
                let index_url = format!("{}/{}", repo_url.trim_end_matches('/'), arch.index_filename());

                
                match downloader.package(&index_url, &temp_path).await {
                    Ok(_) => {
                        if let Some(ref expected_hash) = checksum {
                            if let Err(e) = HashVerifier::verify_integrity(&temp_path, "sha256", expected_hash) {
                                let _ = fs::remove_file(&temp_path);
                                Err(format!("{}: checksum mismatch: {}", repo_name, e))
                            } else if let Err(e) = fs::rename(&temp_path, &final_path) {
                                Err(format!("{}: rename failed: {}", repo_name, e))
                            } else {
                                Ok(repo_name)
                            }
                        } else if let Err(e) = fs::rename(&temp_path, &final_path) {
                            Err(format!("{}: rename failed: {}", repo_name, e))
                        } else {
                            Ok(repo_name)
                        }
                    }
                    Err(e) => {
                        let _ = fs::remove_file(&temp_path);
                        Err(format!("{}: download failed: {}", repo_name, e))
                    }
                }
            }));
        }

        let mut synced = 0usize;
        let mut errors = Vec::new();

        for result in join_all(tasks).await {
            match result {
                Ok(Ok(name)) => {
                    synced += 1;
                    UserInterface::download(&format!("Repo synced: {}", name));
                }
                Ok(Err(e)) => errors.push(e.to_string()),
                Err(e) => errors.push(format!("Join error: {}", e)),
            }
        }

        Ok((synced, errors))
    }

    pub fn search_across_repos(&self, query: &str) -> Result<Vec<(String, PackageMetadata)>> {
        let mut results = Vec::new();
        if !self.sync_dir.exists() {
            return Ok(results);
        }

        let q = query.to_lowercase();
        for entry in fs::read_dir(&self.sync_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false)
                && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(pkgs) = serde_json::from_str::<Vec<PackageMetadata>>(&content) {
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
        Ok(results)
    }

    pub fn resolve_across_repos(&self, pkg_name: &str) -> Result<Vec<PackageMetadata>> {
        let mut results = Vec::new();
        if !self.sync_dir.exists() {
            return Ok(results);
        }

        for entry in fs::read_dir(&self.sync_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false)
                && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(pkgs) = serde_json::from_str::<Vec<PackageMetadata>>(&content) {
                        for pkg in pkgs {
                            if pkg.pkg_name == pkg_name {
                                results.push(pkg);
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
            .with_context(|| format!("Failed to read synchronized index: {:?}", index_path))?;
        let metadata: Vec<PackageMetadata> = serde_json::from_str(&content)
            .context("Cached index data matched an invalid metadata schema")?;
        Ok(metadata)
    }

    pub async fn sync_single(&self, repo_name: &str) -> Result<()> {
        let repos = self.load_repositories()?;
        let repo = repos.iter().find(|r| r.name == repo_name)
            .ok_or_else(|| anyhow!("Repository '{}' not found", repo_name))?;
        let repo = repo.clone();

        let host_arch = host_architecture();
        fs::create_dir_all(&self.sync_dir)?;

        let temp_path = self.sync_dir.join(format!("{}.tmp", repo_name));
        let final_path = self.sync_dir.join(format!("{}.json", repo_name));
        let index_url = format!("{}/{}", repo.url.trim_end_matches('/'), host_arch.index_filename());

        let downloader = Downloader::new();
        downloader.package(&index_url, &temp_path).await
            .map_err(|e| anyhow!("Download failed: {}", e))?;

        if let Some(ref expected_hash) = repo.checksum
            && let Err(e) = HashVerifier::verify_integrity(&temp_path, "sha256", expected_hash) {
                let _ = fs::remove_file(&temp_path);
                return Err(anyhow!("Checksum mismatch: {}", e));
            }

        fs::rename(&temp_path, &final_path)?;
        UserInterface::download(&format!("Repo synced: {}", repo_name));
        Ok(())
    }

    pub fn set_enabled(&self, repo_name: &str, enabled: bool) -> Result<()> {
        let mut repos = self.load_repositories()?;
        let repo = repos.iter_mut().find(|r| r.name == repo_name)
            .ok_or_else(|| anyhow!("Repository '{}' not found", repo_name))?;
        repo.enabled = enabled;
        self.save_repositories(&repos)
    }

    pub fn info(&self, repo_name: &str) -> Result<RepositoryInfo> {
        let repos = self.load_repositories()?;
        repos.into_iter().find(|r| r.name == repo_name)
            .ok_or_else(|| anyhow!("Repository '{}' not found", repo_name))
    }
}
