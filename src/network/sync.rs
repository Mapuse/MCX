use std::sync::Arc;
use std::path::PathBuf;
use anyhow::Result;
use futures::future::join_all;
use tokio::fs;
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

            tasks.push(tokio::spawn(async move {
                dl.download_package(&index_target_url, &local_index_path).await?;
                Ok::<_, anyhow::Error>((repo_name, local_index_path))
            }));
        }

        let results = join_all(tasks).await;
        let mut tx = self.db.begin_transaction()?;

        for res in results {
            let (repo_name, index_path) = res??;
            tx.update_repository_index(&repo_name, index_path.to_str().unwrap_or(""))?;
        }

        tx.commit()?;
        Ok(())
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
                            let _ = fs::remove_file(file_to_remove).await;
                        }
                    }));
                }
            }
            join_all(tasks).await;
        }
        Ok(())
    }
}