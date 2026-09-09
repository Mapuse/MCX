use crate::core::constants;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LAST_TRANSACTION_ID: AtomicU64 = AtomicU64::new(0);

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
    #[serde(default)]
    pub targets: Vec<String>,
    /// Set on a completion marker that finalizes the referenced intent.
    /// The commit protocol is ordered: intent -> durable store commit -> marker.
    #[serde(default)]
    pub completes: Option<u64>,
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

    fn append_record(&self, record: &RegistryTransactionRecord) -> Result<()> {
        self.initialize()?;
        let serialized_payload = format!("{}\n", serde_json::to_string(record)?);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.journal_file)?;
        file.write_all(serialized_payload.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    fn fresh_transaction_id(&self) -> Result<u64> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let pid_bits = (std::process::id() as u64) & 0x3F_FFFF;
        let base = (timestamp << 22) | pid_bits;
        let transaction_id = base.max(LAST_TRANSACTION_ID.load(Ordering::Relaxed) + 1);
        LAST_TRANSACTION_ID.store(transaction_id, Ordering::Relaxed);
        Ok(transaction_id)
    }

    /// Phase 1 of the commit protocol: record the intent. The record is not
    /// treated as committed until `mark_transaction_complete` succeeds.
    pub fn record_transaction(&self, action: ActionKind, targets: Vec<String>) -> Result<u64> {
        let transaction_id = self.fresh_transaction_id()?;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let record = RegistryTransactionRecord {
            transaction_id,
            timestamp,
            action,
            targets,
            completes: None,
        };
        self.append_record(&record)?;
        Ok(transaction_id)
    }

    /// Phase 3 of the commit protocol: mark the intent as durably committed.
    pub fn mark_transaction_complete(&self, id: u64) -> Result<()> {
        let marker_id = self.fresh_transaction_id()?;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let record = RegistryTransactionRecord {
            transaction_id: marker_id,
            timestamp,
            action: ActionKind::Sync,
            targets: Vec::new(),
            completes: Some(id),
        };
        self.append_record(&record)?;
        Ok(())
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
            // A single corrupt or truncated line must not hide the rest of
            // the user's history.
            match serde_json::from_str::<RegistryTransactionRecord>(line) {
                Ok(record) => records.push(record),
                Err(e) => eprintln!("Warning: skipping malformed history entry: {}", e),
            }
        }

        let has_protocol_markers = records.iter().any(|r| r.completes.is_some());
        if !has_protocol_markers {
            // Legacy journal written before the two-phase commit protocol;
            // every entry there represents a finished transaction.
            return Ok(records);
        }

        let completed: HashSet<u64> = records.iter().filter_map(|r| r.completes).collect();
        let mut visible = Vec::new();
        for record in records {
            if record.completes.is_some() {
                continue;
            }
            if completed.contains(&record.transaction_id) {
                visible.push(record);
            } else {
                eprintln!(
                    "Warning: ignoring transaction {} with no completion marker (interrupted commit)",
                    record.transaction_id
                );
            }
        }
        Ok(visible)
    }

    pub fn look_up_transaction(&self, id: u64) -> Result<RegistryTransactionRecord> {
        self.get_history()?
            .into_iter()
            .find(|record| record.transaction_id == id)
            .ok_or_else(|| anyhow!("Target history identifier transaction not found"))
    }
}
