use std::path::PathBuf;
use std::sync::Arc;
use futures_util::future::join_all;
use crate::core::database::Database;
use crate::network::download::Downloader;
use crate::archive::hash::HashVerifier;

pub struct SyncCommand {
    root: PathBuf,
    db: Arc<Database>,
}

impl SyncCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self { root: PathBuf::from(root), db }
    }

    pub async fn execute(&self) -> Result<(), anyhow::Error> {
        let meta_dir = self.root.join("var/lib/mcx/sync");
        std::fs::create_dir_all(&meta_dir)?;

        let remotes = self.db.get_configured_repositories()?;
        let downloader = Arc::new(Downloader::new());
        let mut tasks = Vec::new();

        for repo in remotes {
            let temp_manifest = meta_dir.join(format!("{}.tmp.json", repo.name));
            let final_manifest = meta_dir.join(format!("{}.json", repo.name));
            let index_path = self.root.join("var/lib/mcx/repos").join(format!("{}.index", repo.name));
            let dl = Arc::clone(&downloader);
            let repo_name = repo.name.clone();

            tasks.push(tokio::spawn(async move {
                dl.download_package(&repo.url, &temp_manifest).await?;
                
                if let Some(expected_hash) = &repo.checksum {
                    HashVerifier::verify_integrity(&temp_manifest, &expected_hash)?;
                }

                std::fs::rename(&temp_manifest, &final_manifest)?;
                Ok::<_, anyhow::Error>((repo_name, index_path))
            }));
        }

        let mut transaction = self.db.begin_transaction()?;
        for result in join_all(tasks).await {
            let (ref repo_name, ref index_path) = result??;
            transaction.update_repository_index(repo_name, index_path.to_str().unwrap())?;
        }

        transaction.commit()?;
        Ok(())
    }
}