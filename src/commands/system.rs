use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, Context, anyhow};
use crate::core::db::Database;
use crate::core::declarative::ProfileValidator;
use crate::core::cgroup::CgroupController;
use crate::core::overlay::OverlayManager;
use crate::core::security::SecurityMonitor;
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

        let cgroup_mgr = CgroupController::new();
        let overlay_base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let overlay_mgr = OverlayManager::new(PathBuf::from(&overlay_base).as_path());
        let security_mon = SecurityMonitor::new();

        if !to_remove.is_empty() {
            let remover = RemoveCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
            remover.execute(&to_remove, &cgroup_mgr, &overlay_mgr, &security_mon)
                .context("Atomic removal batch execution failed during state divergence alignment")? ;
        }

        if !to_install.is_empty() {
            let installer = InstallCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db))
                .with_cgroup(cgroup_mgr)
                .with_overlay(overlay_mgr)
                .with_security(Arc::new(security_mon));
            installer.execute(&to_install).await
                .context("Atomic deployment batch execution failed during state divergence alignment")? ;
        }

        Ok(())
    }
}