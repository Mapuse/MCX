use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, Context, anyhow};
use crate::core::db::Database;

pub struct RemoveCommand {
    root: PathBuf,
    db: Arc<Database>,
}

impl RemoveCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self {
            root: PathBuf::from(root),
            db,
        }
    }

    pub fn execute(&self, packages: &[String]) -> Result<()> {
        if packages.is_empty() {
            return Err(anyhow!("No target packages specified for removal transaction"));
        }

        let mut transaction = self.db.begin_transaction()?;
        let mut files_to_purge = Vec::new();

        for pkg in packages {
            if !self.db.is_package_installed(pkg)? {
                return Err(anyhow!("Target package not discovered in ledger: {}", pkg));
            }

            if self.db.has_dependent_packages(pkg)? {
                return Err(anyhow!("Aborting removal: Broken link hazard detected for dependents of {}", pkg));
            }

            let manifest = self.db.get_package_manifest(pkg)
                .with_context(|| format!("Failed to retrieve structural file manifest for {}", pkg))?;
            
            files_to_purge.extend(manifest.files);
            transaction.stage_package_removal(pkg)?;
        }

        files_to_purge.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

        for file_path in files_to_purge {
            let absolute_target = self.root.join(&file_path);
            if !absolute_target.exists() {
                continue;
            }

            if absolute_target.is_dir() {
                if let Ok(mut entries) = fs::read_dir(&absolute_target) {
                    if entries.next().is_none() {
                        fs::remove_dir(&absolute_target)
                            .with_context(|| format!("Failed to prune empty system branch: {:?}", absolute_target))?;
                    }
                }
            } else {
                fs::remove_file(&absolute_target)
                    .with_context(|| format!("Failed to purge atomic node entity: {:?}", absolute_target))?;
            }
        }

        transaction.commit()?;
        Ok(())
    }
}