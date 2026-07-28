use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Result, anyhow};
use crate::core::constants;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum ActionKind {
    Installation,
    Removal,
    Rollback,
    Sync,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegistryTransactionRecord {
    pub transaction_id: u64,
    pub timestamp: u64,
    pub action: ActionKind,
    pub targets: Vec<String>,
}

pub struct ChangelogManager {
    journal_file: PathBuf,
}

impl ChangelogManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            journal_file: root.as_ref().join(constants::PATH_HISTORY),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.journal_file.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    pub fn record_transaction(&self, action: ActionKind, targets: Vec<String>) -> Result<u64> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;
        let transaction_id = timestamp;
        let record = RegistryTransactionRecord {
            transaction_id,
            timestamp,
            action,
            targets,
        };
        self.initialize()?;
        let serialized_payload = format!("{}\n", serde_json::to_string(&record)?);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.journal_file)?;
        file.write_all(serialized_payload.as_bytes())?;
        file.sync_all()?;
        Ok(transaction_id)
    }

    pub fn get_history(&self) -> Result<Vec<RegistryTransactionRecord>> {
        if !self.journal_file.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.journal_file)?;
        let mut records = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let record: RegistryTransactionRecord = serde_json::from_str(line)?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn look_up_transaction(&self, id: u64) -> Result<RegistryTransactionRecord> {
        self.get_history()?
            .into_iter()
            .find(|record| record.transaction_id == id)
            .ok_or_else(|| anyhow!("Target history identifier transaction not found"))
    }
}