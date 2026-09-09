use crate::core::changelog::{ActionKind, ChangelogManager};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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
            let backup_path = if target.extension().is_some_and(|e| e == "mcx_bak") {
                target.with_extension("mcx_bak2")
            } else {
                target.with_extension("mcx_bak")
            };
            if target.is_dir() {
                copy_dir_recursive(target, &backup_path)?;
            } else {
                fs::copy(target, &backup_path)?;
            }
            self.backups.push((target.to_path_buf(), backup_path));
        }
        Ok(())
    }

    /// Phase 1 of the ordered commit protocol: record the journal intent.
    /// The caller then commits the durable store (LMDB), and finally calls
    /// `finalize_commit` to mark the intent complete.
    pub fn prepare_commit(&mut self) -> Result<u64> {
        self.ensure_active()?;
        self.id = self
            .changelog
            .record_transaction(self.action_kind.clone(), self.affected_packages.clone())?;
        Ok(self.id)
    }

    /// Phase 3 of the ordered commit protocol. Backups are intentionally
    /// retained until process exit so that a crash after the durable commit
    /// still leaves a recoverable trail on disk.
    pub fn finalize_commit(&mut self) -> Result<()> {
        self.ensure_active()?;
        if self.id != 0 {
            self.changelog.mark_transaction_complete(self.id)?;
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        // symlink_metadata never follows links, so a directory tree cannot
        // escape via a symlinked subdirectory during backup/restore.
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            let link_target = fs::read_link(&path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link_target, &dest)?;
        } else if metadata.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else if metadata.is_file() {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

/// Copies a regular file to its destination atomically.
///
/// The temporary file gets a unique name in the destination directory
/// (same filesystem, so the final rename is atomic), refuses to follow
/// symlinks or directories as source, syncs file data before renaming and
/// flushes the parent directory entry afterwards.
pub(crate) fn atomic_copy(src: &Path, dst: &Path) -> Result<()> {
    let src_metadata =
        fs::symlink_metadata(src).with_context(|| format!("Failed to inspect {:?}", src))?;
    if !src_metadata.is_file() {
        return Err(anyhow!(
            "Refusing to copy non-regular file {:?} to {:?}",
            src,
            dst
        ));
    }

    let Some(parent) = dst.parent() else {
        return Err(anyhow!("Destination {:?} has no parent directory", dst));
    };
    fs::create_dir_all(parent)?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut src_file = fs::File::open(src)?;
        std::io::copy(&mut src_file, &mut tmp)?;
    }
    tmp.as_file().sync_all()?;
    let tmp_path = tmp.into_temp_path();
    fs::rename(&tmp_path, dst)?;

    // Flush the directory entry so the rename survives a crash.
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_copy_rejects_symlink_source() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("secret.txt");
        fs::write(&secret, b"data").unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&secret, &link).unwrap();

        let dst = dir.path().join("out.txt");
        assert!(atomic_copy(&link, &dst).is_err());
        assert!(!dst.exists());
    }

    #[test]
    fn atomic_copy_writes_regular_files_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.txt");
        fs::write(&src, b"payload").unwrap();
        let dst = dir.path().join("nested").join("dst.txt");

        atomic_copy(&src, &dst).unwrap();
        assert_eq!(fs::read_to_string(&dst).unwrap(), "payload");
        assert!(
            atomic_copy(&src, &dst).is_ok(),
            "overwriting an existing destination must succeed"
        );
    }

    #[test]
    fn rollback_restores_backups_and_removes_staged_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(root.join("usr/bin")).unwrap();
        let live = root.join("usr/bin/tool");
        fs::write(&live, b"old").unwrap();

        let mut tx = PackageTransaction::new(root.clone(), ActionKind::Installation).unwrap();
        tx.backup_file(&live).unwrap();
        tx.prepare_commit().unwrap();

        // Simulate an install overwriting the file mid-transaction.
        fs::write(&live, b"new").unwrap();
        tx.record_staged_file(PathBuf::from("usr/bin/extra"))
            .unwrap();
        fs::write(root.join("usr/bin/extra"), b"x").unwrap();

        tx.rollback().unwrap();
        assert_eq!(
            fs::read_to_string(&live).unwrap(),
            "old",
            "backup must be restored on rollback"
        );
        assert!(
            !root.join("usr/bin/extra").exists(),
            "staged files must be removed on rollback"
        );
    }
}
