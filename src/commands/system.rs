use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, Context, anyhow};
use crate::core::db::Database;
use crate::core::declarative::SystemProfile;
use crate::commands::install::InstallCommand;
use crate::commands::remove::RemoveCommand;

pub struct SystemCommand {
    root: PathBuf,
    db: Arc<Database>,
}

impl SystemCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self {
            root: PathBuf::from(root),
            db,
        }
    }

    pub async fn rebuild(&self, config_path: &str) -> Result<()> {
        let config_file = PathBuf::from(config_path);
        if !config_file.exists() {
            return Err(anyhow!("Target blueprint specification blueprint missing: {}", config_path));
        }

        let content = fs::read_to_string(&config_file)
            .with_context(|| format!("Failed to read structural system blueprint profile: {:?}", config_file))? ;
            
        let blueprint: SystemProfile = serde_json::from_str(&content)
            .context("Failed to parse structural system blueprint template schema")? ;

        let installed_packages = self.db.get_all_installed_packages()
            .unwrap_or_else(|_| vec![]);

        let target_set: std::collections::HashSet<String> = blueprint.packages.into_iter().collect();
        let current_set: std::collections::HashSet<String> = installed_packages.into_iter().map(|pkg| pkg.pkg_name).collect();

        let to_remove: Vec<String> = current_set.difference(&target_set).cloned().collect();
        let to_install: Vec<String> = target_set.difference(&current_set).cloned().collect();

        if !to_remove.is_empty() {
            let remover = RemoveCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
            remover.execute(&to_remove)
                .context("Atomic removal batch execution failed during state divergence alignment")? ;
        }

        if !to_install.is_empty() {
            let installer = InstallCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
            installer.execute(&to_install).await
                .context("Atomic deployment batch execution failed during state divergence alignment")? ;
        }

        Ok(())
    }
}