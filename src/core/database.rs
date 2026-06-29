use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use crate::core::transaction::ParallelFileOp;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChecksumData {
    pub kind: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Dependency {
    pub name: String,
    pub dep_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackageMetadata {
    pub pkg_name: String,
    pub version: String,
    pub license: String,
    pub source: String,
    pub checksum: ChecksumData,
    pub dependencies: Vec<Dependency>,
    pub files: Vec<PathBuf>,
    pub provides: Option<Vec<String>>,
    pub conflicts: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RepositoryInfo {
    pub name: String,
    pub url: String,
    pub checksum: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct LedgerState {
    pub installed: std::collections::HashMap<String, PackageMetadata>,
    pub available: std::collections::HashMap<String, PackageMetadata>,
    pub repositories: Vec<RepositoryInfo>,
    pub virtual_provides: std::collections::HashMap<String, String>,
}

pub struct Database {
    pub registry_path: PathBuf,
    pub state: Mutex<LedgerState>,
}

pub struct DbTransaction<'a> {
    pub db: &'a Database,
    pub staging_state: LedgerState,
    pub committed: bool,
    pub tx_log: crate::core::transaction::PackageTransaction,
}

impl Database {
    pub fn open<P: AsRef<Path>>(root: P) -> Result<Self> {
        let registry_path = root.as_ref().join("var/lib/mcx/local.json");
        if let Some(parent) = registry_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let state = if registry_path.exists() {
            let content = fs::read_to_string(&registry_path)?;
            serde_json::from_str(&content)?
        } else {
            LedgerState::default()
        };
        Ok(Self {
            registry_path,
            state: Mutex::new(state),
        })
    }

    pub fn begin_transaction(&self) -> Result<DbTransaction<'_>> {
        let staging_state = self.state.lock().map_err(|_| anyhow!("Lock failure"))?.clone();
        let mut root = self.registry_path.clone();
        for _ in 0..4 {
            root.pop();
        }

        let tx_log = crate::core::transaction::PackageTransaction::new(root, crate::core::changelog::ActionKind::Installation)?;

        Ok(DbTransaction {
            db: self,
            staging_state,
            committed: false,
            tx_log,
        })
    }

    pub fn get_all_installed_packages(&self) -> Result<Vec<PackageMetadata>> {
        let guard = self.state.lock().map_err(|_| anyhow!("Lock failure"))?;
        Ok(guard.installed.values().cloned().collect())
    }

    pub fn get_all_available_packages(&self) -> Result<Vec<PackageMetadata>> {
        let guard = self.state.lock().map_err(|_| anyhow!("Lock failure"))?;
        Ok(guard.available.values().cloned().collect())
    }

    pub fn is_package_installed(&self, pkg_name: &str) -> Result<bool> {
        let guard = self.state.lock().map_err(|_| anyhow!("Lock failure"))?;
        Ok(guard.installed.contains_key(pkg_name))
    }

    pub fn get_package_manifest(&self, pkg_name: &str) -> Result<PackageMetadata> {
        let guard = self.state.lock().map_err(|_| anyhow!("Lock failure"))?;
        if let Some(pkg) = guard.installed.get(pkg_name) {
            return Ok(pkg.clone());
        }
        if let Some(pkg) = guard.available.get(pkg_name) {
            return Ok(pkg.clone());
        }
        Err(anyhow!("Package '{}' not found in registry", pkg_name))
    }

    pub fn get_configured_repositories(&self) -> Result<Vec<RepositoryInfo>> {
        let guard = self.state.lock().map_err(|_| anyhow!("Lock failure"))?;
        Ok(guard.repositories.clone())
    }

    pub fn has_dependent_packages(&self, pkg_name: &str) -> Result<bool> {
        let guard = self.state.lock().map_err(|_| anyhow!("Lock failure"))?;
        for (installed_name, installed_pkg) in &guard.installed {
            if installed_name != pkg_name {
                if installed_pkg.dependencies.iter().any(|d| d.name == pkg_name) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl<'a> DbTransaction<'a> {
    pub fn register_package_placement(&mut self, meta: &PackageMetadata) -> Result<()> {
        for (inst_name, inst_pkg) in &self.staging_state.installed {
            if inst_name != &meta.pkg_name {
                for file in &meta.files {
                    if inst_pkg.files.contains(file) {
                        return Err(anyhow!("File collision error: {:?} belongs to {}", file, inst_name));
                    }
                }
            }
        }
        if let Some(provides) = &meta.provides {
            for v in provides {
                self.staging_state.virtual_provides.insert(v.clone(), meta.pkg_name.clone());
            }
        }
        self.staging_state.installed.insert(meta.pkg_name.clone(), meta.clone());
        self.tx_log.track_package(&meta.pkg_name)?;
        Ok(())
    }

    pub fn update_repository_index(&mut self, _repo_name: &str, index_path: &str) -> Result<()> {
        let content = fs::read_to_string(index_path)?;
        let remote_pkgs: Vec<PackageMetadata> = serde_json::from_str(&content)?;
        for pkg in remote_pkgs {
            if let Some(provides) = &pkg.provides {
                for v in provides {
                    self.staging_state.virtual_provides.insert(v.clone(), pkg.pkg_name.clone());
                }
            }
            self.staging_state.available.insert(pkg.pkg_name.clone(), pkg);
        }
        Ok(())
    }

    pub fn stage_package_removal(&mut self, name: &str) -> Result<()> {
        if let Some(meta) = self.staging_state.installed.remove(name) {
            if let Some(provides) = &meta.provides {
                for v in provides {
                    self.staging_state.virtual_provides.remove(v);
                }
            }
            self.tx_log.track_package(name)?;
        }
        Ok(())
    }

    pub fn parallel_copy(&self, ops: &[ParallelFileOp]) -> Result<()> {
        self.tx_log.parallel_copy(ops)
    }

    pub fn backup_file(&mut self, path: &Path) -> Result<()> {
        self.tx_log.backup_file(path)
    }

    pub fn record_staged_file(&mut self, path: PathBuf) -> Result<()> {
        self.tx_log.record_staged_file(path)
    }

    pub fn commit(mut self) -> Result<()> {
        self.tx_log.commit()?;
        let payload = serde_json::to_string_pretty(&self.staging_state)?;
        let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(&self.db.registry_path)?;
        file.write_all(payload.as_bytes())?;
        let mut guard = self.db.state.lock().map_err(|_| anyhow!("Lock failure"))?;
        *guard = self.staging_state;
        Ok(())
    }
}