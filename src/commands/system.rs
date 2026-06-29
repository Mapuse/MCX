use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, Context, anyhow};
use crate::core::db::Database;
use crate::core::declarative::ProfileValidator;
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

        let blueprint = ProfileValidator::load_profile(&config_file)
            .with_context(|| format!("Failed to load structural system blueprint: {:?}", config_file))?;

        let installed_packages = self.db.get_all_installed_packages()
            .unwrap_or_else(|_| vec![]);

        let current_names: Vec<String> = installed_packages.iter().map(|p| p.pkg_name.clone()).collect();
        let (to_remove, to_install) = ProfileValidator::compile_profile_diff(&current_names, &blueprint.packages);

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