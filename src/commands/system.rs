use crate::commands::install::InstallCommand;
use crate::commands::remove::RemoveCommand;
use crate::core::cgroup::CgroupController;
use crate::core::db::Database;
use crate::core::declarative::ProfileValidator;
use crate::core::plugin::PluginManager;
use crate::core::security::SecurityMonitor;
use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;
use std::sync::Arc;

pub struct SystemCommand {
    root: PathBuf,
    db: Arc<Database>,
    plugin_mgr: Option<Arc<PluginManager>>,
}

impl SystemCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self {
            root: PathBuf::from(root),
            db,
            plugin_mgr: None,
        }
    }

    pub fn with_plugin_mgr(mut self, mgr: Arc<PluginManager>) -> Self {
        self.plugin_mgr = Some(mgr);
        self
    }

    pub async fn rebuild(&self, config_path: &str) -> Result<()> {
        let config_file = PathBuf::from(config_path);
        if !config_file.exists() {
            return Err(anyhow!(
                "Target blueprint specification blueprint missing: {}",
                config_path
            ));
        }

        let blueprint = ProfileValidator::load_profile(&config_file).with_context(|| {
            format!(
                "Failed to load structural system blueprint: {:?}",
                config_file
            )
        })?;

        let installed_packages = self
            .db
            .get_all_installed_packages()
            .unwrap_or_else(|_| vec![]);

        let current_names: Vec<String> = installed_packages
            .iter()
            .map(|p| p.pkg_name.clone())
            .collect();
        let (to_remove, to_install) =
            ProfileValidator::compile_profile_diff(&current_names, &blueprint.packages);

        let cgroup_mgr = CgroupController::new();
        let security_mon = SecurityMonitor::new();

        if !to_remove.is_empty() {
            let mut remover = RemoveCommand::new(
                self.root.to_string_lossy().into_owned(),
                Arc::clone(&self.db),
            );
            if let Some(ref mgr) = self.plugin_mgr {
                remover = remover.with_plugin_mgr(Arc::clone(mgr));
            }
            remover
                .execute(&to_remove, &cgroup_mgr, &security_mon)
                .context(
                    "Atomic removal batch execution failed during state divergence alignment",
                )?;
        }

        if !to_install.is_empty() {
            let mut installer = InstallCommand::new(
                self.root.to_string_lossy().into_owned(),
                Arc::clone(&self.db),
            )
            .with_cgroup(cgroup_mgr)
            .with_security(Arc::new(security_mon));
            if let Some(ref mgr) = self.plugin_mgr {
                installer = installer.with_plugin_mgr(Arc::clone(mgr));
            }
            installer.execute(&to_install).await.context(
                "Atomic deployment batch execution failed during state divergence alignment",
            )?;
        }

        Ok(())
    }
}
