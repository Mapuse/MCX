use crate::core::db::Database;
use crate::utils::ui::UserInterface;
use anyhow::{Context, Result};
use std::sync::Arc;

pub struct SearchCommand {
    db: Arc<Database>,
}

impl SearchCommand {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn execute(&self, query: &str) -> Result<()> {
        let normalized_query = query.to_lowercase();
        let available_packages = self
            .db
            .get_all_available_packages()
            .context("Failed to retrieve available packages index from database")?;
        // One bulk read instead of two database lookups per candidate.
        let installed_names: std::collections::HashSet<String> = self
            .db
            .get_all_installed_packages()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.pkg_name)
            .collect();

        let mut matches = Vec::new();

        for pkg_name in available_packages {
            if pkg_name.pkg_name.to_lowercase().contains(&normalized_query) {
                match self.db.get_package_manifest(&pkg_name.pkg_name) {
                    Ok(meta) => {
                        let status_suffix = if installed_names.contains(&meta.pkg_name) {
                            " [installed]"
                        } else {
                            ""
                        };
                        matches.push(format!(
                            "{} v{} - {}{}",
                            meta.pkg_name, meta.version, meta.license, status_suffix
                        ));
                    }
                    Err(e) => {
                        UserInterface::error(&format!(
                            "Failed to load metadata for {}: {}",
                            pkg_name.pkg_name, e
                        ));
                        matches.push(pkg_name.pkg_name);
                    }
                }
            }
        }

        if matches.is_empty() {
            UserInterface::info(&format!("No packages found matching query: {}", query));
        } else {
            UserInterface::render_list(&format!("Search results for '{}'", query), &matches);
        }

        Ok(())
    }
}
