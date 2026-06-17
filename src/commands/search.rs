use std::sync::Arc;
use anyhow::{Result, Context};
use crate::core::database::Database;
use crate::utils::ui::UserInterface;

pub struct SearchCommand {
    db: Arc<Database>,
}

impl SearchCommand {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn execute(&self, query: &str) -> Result<()> {
        let normalized_query = query.to_lowercase();
        let available_packages = self.db.get_all_available_packages()
            .context("Failed to retrieve available packages index from database")?;

        let mut matches = Vec::new();

        for pkg_name in available_packages {
            if pkg_name.pkg_name.to_lowercase().contains(&normalized_query) {
                if let Ok(meta) = self.db.get_package_manifest(&pkg_name.pkg_name) {
                    let is_installed = self.db.is_package_installed(&meta.pkg_name).unwrap_or(false);
                    let _status_suffix = if is_installed { " [installed]" } else { "" };
                    matches.push(format!("{} v{} - {}", meta.pkg_name, meta.version, meta.license));
                } else {
                    matches.push(pkg_name.pkg_name); 
                }
            }
        }

        if matches.is_empty() {
            UserInterface::display_info(&format!("No packages found matching query: {}", query));
        } else {
            UserInterface::render_list(&format!("Search results for '{}'", query), &matches);
        }

        Ok(())
    }
}