use std::path::PathBuf;
use std::sync::Arc;
use futures_util::future::join_all;
use crate::core::database::Database;
use crate::network::download::Downloader;
use crate::archive::hash::HashVerifier;

pub struct NetworkSyncEngine {
    root: PathBuf,
    db: Arc<Database>,
}

impl NetworkSyncEngine {
    pub fn new(root: PathBuf, db: Arc<Database>) -> Self {
        Self { root, db }
    }

    
    pub async fn sync_all_repositories(&self) -> Result<(), anyhow::Error> {
        let meta_dir = self.root.join("var/lib/mcx/sync");
        std::fs::create_dir_all(&meta_dir)?;

        let remotes = self.db.get_configured_repositories()?;
        let downloader = Arc::new(Downloader::new());
        let mut tasks = Vec::new();

        for repo in remotes {
            let temp_path = meta_dir.join(format!("{}.tmp", repo.name));
            let final_path = meta_dir.join(format!("{}.json", repo.name));
            let dl = Arc::clone(&downloader);
            let repo_name = repo.name.clone();
            let checksum = repo.checksum.clone();

            tasks.push(tokio::spawn(async move {
                dl.download_package(&repo.url, &temp_path).await?;
                
                if let Some(ref expected_hash) = checksum {
                    HashVerifier::verify_integrity(&temp_path, expected_hash)?;
                }

                std::fs::rename(&temp_path, &final_path)?;
                Ok::<_, anyhow::Error>((repo_name, final_path))
            }));
        }

        let mut transaction = self.db.begin_transaction()?;
        for result in join_all(tasks).await {
            let (ref repo_name, ref index_path) = result??;
            transaction.update_repository_index(repo_name, index_path.to_str().unwrap_or(""))?;
        }

        transaction.commit()?;
        Ok(())
    }
}