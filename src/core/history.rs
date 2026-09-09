use crate::core::changelog::{ActionKind, ChangelogManager, RegistryTransactionRecord};
use crate::core::db::Database;
use anyhow::{Result, anyhow};
use std::path::Path;

pub struct HistoryEngine {
    changelog: ChangelogManager,
    pub db: std::sync::Arc<Database>,
}

impl HistoryEngine {
    pub fn new<P: AsRef<Path>>(root: P, db: std::sync::Arc<Database>) -> Self {
        Self {
            changelog: ChangelogManager::new(&root),
            db,
        }
    }

    pub fn fetch_ordered_log(&self) -> Result<Vec<RegistryTransactionRecord>> {
        let mut history = self.changelog.get_history()?;
        history.sort_by_key(|record| record.timestamp);
        Ok(history)
    }

    pub fn compute_rollback_plan(
        &self,
        target_transaction_id: u64,
    ) -> Result<Vec<(ActionKind, Vec<String>)>> {
        let history = self.fetch_ordered_log()?;

        let target_index = history
            .iter()
            .position(|r| r.transaction_id == target_transaction_id)
            .ok_or_else(|| {
                anyhow!(
                    "Target historic state marker not registered in log sequence: {}",
                    target_transaction_id
                )
            })?;

        let mut operations_pipeline = Vec::new();

        for record in history.iter().skip(target_index + 1).rev() {
            match record.action {
                ActionKind::Installation => {
                    operations_pipeline.push((ActionKind::Removal, record.targets.clone()));
                }
                ActionKind::Removal => {
                    operations_pipeline.push((ActionKind::Installation, record.targets.clone()));
                }
                ActionKind::Rollback | ActionKind::Sync => {}
            }
        }

        Ok(operations_pipeline)
    }
}
