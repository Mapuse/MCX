use std::path::{Path, PathBuf};
use std::fs;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::io::{Read, Write};
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use crate::core::constants;
use crate::core::changelog::{ChangelogManager, ActionKind};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum TransactionState {
    Active,
    Committed,
    Aborted,
    RolledBack,
}

pub struct ParallelFileOp {
    pub src: PathBuf,
    pub dst: PathBuf,
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
    completed_ops: Arc<AtomicU64>,
    total_ops: Arc<AtomicU64>,
    aborted: Arc<AtomicBool>,
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
            completed_ops: Arc::new(AtomicU64::new(0)),
            total_ops: Arc::new(AtomicU64::new(0)),
            aborted: Arc::new(AtomicBool::new(false)),
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

    pub fn parallel_copy(&self, ops: &[ParallelFileOp]) -> Result<()> {
        self.ensure_active()?;
        self.total_ops.store(ops.len() as u64, Ordering::Relaxed);
        self.completed_ops.store(0, Ordering::Relaxed);
        self.aborted.store(false, Ordering::Relaxed);

        let completed = Arc::clone(&self.completed_ops);
        let aborted = Arc::clone(&self.aborted);

        let chunk_size = (ops.len() / num_cpus::get().max(1)).max(1);
        let mut handles = Vec::new();

        for chunk in ops.chunks(chunk_size) {
            let chunk_owned: Vec<(PathBuf, PathBuf)> = chunk.iter().map(|op| (op.src.clone(), op.dst.clone())).collect();
            let completed = Arc::clone(&completed);
            let aborted = Arc::clone(&aborted);

            handles.push(std::thread::spawn(move || -> Result<()> {
                for (src, dst) in &chunk_owned {
                    if aborted.load(Ordering::Relaxed) {
                        return Err(anyhow!("Operation aborted"));
                    }
                    if let Some(parent) = dst.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    atomic_copy(src, dst)?;
                    completed.fetch_add(1, Ordering::Release);
                }
                Ok(())
            }));
        }

        for h in handles {
            match h.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    self.aborted.store(true, Ordering::Release);
                    return Err(e);
                }
                Err(_) => {
                    self.aborted.store(true, Ordering::Release);
                    return Err(anyhow!("Thread panicked during parallel copy"));
                }
            }
        }

        Ok(())
    }

    pub fn progress(&self) -> (u64, u64) {
        (self.completed_ops.load(Ordering::Acquire), self.total_ops.load(Ordering::Acquire))
    }

    pub fn commit(&mut self) -> Result<()> {
        self.ensure_active()?;
        self.id = self.changelog.record_transaction(self.action_kind.clone(), self.affected_packages.clone())?;

        for (_, backup) in &self.backups {
            if backup.is_dir() {
                let _ = fs::remove_dir_all(backup);
            } else {
                let _ = fs::remove_file(backup);
            }
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
        if path.is_symlink() {
            let link_target = fs::read_link(&path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link_target, &dest)?;
        } else if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else if path.is_file() {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

fn atomic_copy(src: &Path, dst: &Path) -> Result<()> {
    let tmp = dst.with_extension("mcx_tmp");
    if src.is_file() {
        let mut src_file = fs::File::open(src)?;
        let mut dst_file = fs::File::create(&tmp)?;
        let mut buffer = vec![0u8; constants::TRANSACTION_HASH_BUFFER_SIZE];
        loop {
            let n = src_file.read(&mut buffer)?;
            if n == 0 { break; }
            dst_file.write_all(&buffer[..n])?;
        }
        dst_file.sync_all()?;
        fs::rename(&tmp, dst)?;
    }
    Ok(())
}

pub struct AtomicFileWriter {
    path: PathBuf,
    tmp_path: PathBuf,
    written: bool,
}

impl AtomicFileWriter {
    pub fn new(path: PathBuf) -> Self {
        let tmp_path = path.with_extension("mcx_atomic");
        Self { path, tmp_path, written: false }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        if let Some(parent) = self.tmp_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.tmp_path, data)?;
        self.written = true;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        if !self.written { return Ok(()); }
        fs::rename(&self.tmp_path, &self.path)?;
        self.written = false;
        Ok(())
    }
}

impl Drop for AtomicFileWriter {
    fn drop(&mut self) {
        if self.written {
            let _ = fs::remove_file(&self.tmp_path);
        }
    }
}
