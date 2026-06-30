use std::path::PathBuf;
use std::sync::Arc;
use crate::core::db::Database;
use crate::core::repo::RepositoryManager;

pub struct SyncCommand {
    root: PathBuf,
    db: Arc<Database>,
}

impl SyncCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self { root: PathBuf::from(root), db }
    }

    pub async fn execute(&self) -> Result<(), anyhow::Error> {
        let mgr = RepositoryManager::new(&self.root);
        let (synced, errors) = mgr.sync_all_parallel().await?;
        if !errors.is_empty() {
            for e in &errors {
                eprintln!("Sync error: {}", e);
            }
        }
        if synced == 0 {
            anyhow::bail!("No repositories synced successfully");
        }

        // Load the synced indexes into the database
        let repos = mgr.load_repositories()?;
        let mut tx = self.db.begin_transaction()?;
        for repo in &repos {
            let index_path = mgr.get_local_index_path(&repo.name);
            if index_path.exists() {
                tx.update_repository_index(&repo.name, index_path.to_str().unwrap())?;
            }
        }
        tx.commit()?;

        Ok(())
    }
}
