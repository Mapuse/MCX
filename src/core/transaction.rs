use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use crate::core::changelog::{ChangelogManager, ActionKind};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum TransactionState {
    Active,
    Committed,
    Aborted,
    RolledBack,
}

pub struct PackageTransaction {
    id: u64,
    root: PathBuf,
    action_kind: ActionKind,
    affected_packages: Vec<String>,
    staged_files: Vec<PathBuf>,
    backups: Vec<(PathBuf, PathBuf)>,
    state: TransactionState,
    changelog: ChangelogManager,
}

impl PackageTransaction {
    pub fn new(root: PathBuf, action_kind: ActionKind) -> Result<Self> {
        let changelog = ChangelogManager::new(&root);
        Ok(Self {
            id: 0,
            root,
            action_kind,
            affected_packages: Vec::new(),
            staged_files: Vec::new(),
            backups: Vec::new(),
            state: TransactionState::Active,
            changelog,
        })
    }

    pub fn track_package(&mut self, pkg_name: &str) -> Result<()> {
        self.ensure_active()?;
        if !self.affected_packages.contains(&pkg_name.to_string()) {
            self.affected_packages.push(pkg_name.to_string());
        }
        Ok(())
    }

    pub fn record_staged_file(&mut self, path: PathBuf) -> Result<()> {
        self.ensure_active()?;
        self.staged_files.push(path);
        Ok(())
    }

    pub fn backup_file(&mut self, target: &Path) -> Result<()> {
        self.ensure_active()?;
        if target.exists() {
            let backup_path = target.with_extension("mcx_bak");
            fs::copy(target, &backup_path)?;
            self.backups.push((target.to_path_buf(), backup_path));
        }
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        self.ensure_active()?;
        self.id = self.changelog.record_transaction(self.action_kind.clone(), self.affected_packages.clone())?;
        
        for (_, backup) in &self.backups {
            let _ = fs::remove_file(backup);
        }
        
        self.state = TransactionState::Committed;
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<()> {
        if self.state != TransactionState::Active && self.state != TransactionState::Aborted {
            return Err(anyhow!("Cannot rollback a committed transaction"));
        }

        for file in &self.staged_files {
            let full_path = self.root.join(file);
            if full_path.exists() {
                let _ = fs::remove_file(full_path);
            }
        }

        for (original, backup) in &self.backups {
            if backup.exists() {
                let _ = fs::rename(backup, original);
            }
        }

        self.state = TransactionState::RolledBack;
        Ok(())
    }

    fn ensure_active(&self) -> Result<()> {
        if self.state != TransactionState::Active {
            Err(anyhow!("Transaction is not active"))
        } else {
            Ok(())
        }
    }
}

impl Drop for PackageTransaction {
    fn drop(&mut self) {
        if self.state == TransactionState::Active {
            let _ = self.rollback();
        }
    }
}