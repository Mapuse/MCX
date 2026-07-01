use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, Context};
use futures::future::join_all;
use crate::core::db::Database;
use crate::core::repo::RepositoryManager;
use crate::network::download::Downloader;

pub struct NetworkSyncEngine {
    db: Arc<Database>,
    repo_mgr: RepositoryManager,
    downloader: Downloader,
    root: PathBuf,
}

impl NetworkSyncEngine {
    pub fn new(db: Arc<Database>, root: String) -> Self {
        Self {
            db,
            repo_mgr: RepositoryManager::new(&root),
            downloader: Downloader::new(),
            root: PathBuf::from(root),
        }
    }

    pub async fn synchronize_repositories(&self) -> Result<()> {
        let configured_repos = self.repo_mgr.load_repositories()?;
        let mut tasks = Vec::with_capacity(configured_repos.len());

        for repo in configured_repos {
            let dl = self.downloader.clone();
            let repo_name = repo.name.clone();
            let index_target_url = format!("{}/index.json", repo.url.trim_end_matches('/'));
            let local_index_path = self.repo_mgr.get_local_index_path(&repo.name);
            let etag_path = local_index_path.with_extension("json.etag");

            let stored_etag = if etag_path.exists() {
                std::fs::read_to_string(&etag_path).ok()
            } else {
                None
            };
            let stored_etag2 = stored_etag.clone();

            tasks.push(tokio::spawn(async move {
                let mut req = dl.get_client().head(&index_target_url);
                if let Some(ref tag) = stored_etag2 {
                    req = req.header("If-None-Match", tag);
                }
                let head = req.send().await?;
                if head.status() == 304 {
                    return Ok::<_, anyhow::Error>((repo_name, local_index_path, false));
                }
                let _ = dl.package(&index_target_url, &local_index_path).await?;
                let new_etag = head.headers().get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.trim_matches('"').to_string());
                if let Some(tag) = new_etag {
                    let _ = std::fs::write(&etag_path, &tag);
                } else if stored_etag.is_some() {
                    let _ = std::fs::remove_file(&etag_path);
                }
                Ok((repo_name, local_index_path, true))
            }));
        }

        let mut loaded = 0usize;
        let results = join_all(tasks).await;
        let mut tx = self.db.begin_transaction()?;

        for res in results {
            let (repo_name, index_path, changed) = res??;
            if changed || !self.ldex(&repo_name)? {
                tx.update_repository_index(&repo_name, index_path.to_str().unwrap_or(""))
                    .with_context(|| format!("Failed to load index for {}", repo_name))?;
                loaded += 1;
            }
        }

        tx.commit()?;

        if loaded == 0 {
            // all indexes were already current
        }

        Ok(())
    }

    pub fn ldex(&self, repo_name: &str) -> Result<bool> {
        let _txn = self.db.env_read_txn()?;
        let sync_dir = self.root.join("var/lib/mcx/sync");
        let index_path = sync_dir.join(format!("{}.json", repo_name));
        Ok(index_path.exists())
    }

    pub async fn verify_remote_mirrors(&self) -> Result<Vec<(String, bool)>> {
        let configured_repos = self.repo_mgr.load_repositories()?;
        let mut tasks = Vec::with_capacity(configured_repos.len());

        for repo in configured_repos {
            let dl = self.downloader.clone();
            tasks.push(tokio::spawn(async move {
                let status = dl.check_endpoint_availability(&repo.url).await;
                (repo.name, status)
            }));
        }

        let mut status_matrix = Vec::with_capacity(tasks.len());
        for res in join_all(tasks).await {
            status_matrix.push(res?);
        }

        Ok(status_matrix)
    }

    pub async fn cleanup_stale_package_files(&self, pkg_name: &str, new_installed_files: &[PathBuf]) -> Result<()> {
        if let Ok(old_metadata) = self.db.get_package_manifest(pkg_name) {
            let mut tasks = Vec::new();

            for old_file in old_metadata.files {
                if !new_installed_files.contains(&old_file) {
                    let file_to_remove = self.root.join(&old_file);
                    tasks.push(tokio::spawn(async move {
                        if file_to_remove.exists() && file_to_remove.is_file() {
                            let _ = tokio::fs::remove_file(file_to_remove).await;
                        }
                    }));
                }
            }
            join_all(tasks).await;
        }
        Ok(())
    }
}
