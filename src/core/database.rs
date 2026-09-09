use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use heed::{Env, EnvOpenOptions, RwTxn};
use heed::types::{Str, SerdeBincode};

use crate::core::constants;
use crate::core::component::Component;
use crate::core::service::CesarService;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChecksumData {
    pub kind: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Dependency {
    pub name: String,
    pub dep_type: String,
    pub libraries: Option<Vec<String>>,
}

fn default_arch() -> String {
    "native".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackageMetadata {
    pub pkg_name: String,
    pub version: String,
    pub license: String,
    pub source: String,
    pub checksum: ChecksumData,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub files: Vec<PathBuf>,
    pub provides: Option<Vec<String>>,
    pub conflicts: Option<Vec<String>>,
    #[serde(default = "default_arch", alias = "arch")]
    pub architecture: String,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub services: Vec<CesarService>,
    #[serde(default)]
    pub binaries: Vec<String>,
    /// Per-file SHA-256 digests recorded at install time (relative path -> hex hash).
    /// Integrity verification checks files against these; legacy packages without
    /// the field are skipped gracefully.
    #[serde(default)]
    pub file_hashes: HashMap<String, String>,
}

impl PackageMetadata {
    pub fn all_services(&self) -> Vec<&CesarService> {
        self.services.iter().collect()
    }

    pub fn find_component_for_file(&self, file: &str) -> Option<&Component> {
        self.components.iter().find(|&comp| comp.files.iter().any(|f| f.to_string_lossy() == file || f.to_string_lossy().contains(file))).map(|v| v as _)
    }

    pub fn find_component_for_binary(&self, binary: &str) -> Option<&Component> {
        let bin_path = format!("usr/bin/{}", binary);
        self.components.iter().find(|&comp| comp.files.iter().any(|f| f.to_string_lossy() == bin_path || f.file_name().map(|n| n == binary).unwrap_or(false))).map(|v| v as _)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RepositoryInfo {
    pub name: String,
    pub url: String,
    pub checksum: Option<String>,
    pub enabled: bool,
}

type PkgDb = heed::Database<Str, SerdeBincode<PackageMetadata>>;
type StrDb = heed::Database<Str, SerdeBincode<String>>;

pub struct Database {
    env: Env,
    root: PathBuf,
    installed_db: PkgDb,
    available_db: PkgDb,
    virtual_db: StrDb,
}

pub struct DbTransaction<'e> {
    db: &'e Database,
    txn: Option<RwTxn<'e>>,
    tx_log: crate::core::transaction::PackageTransaction,
    committed: bool,
}

impl Database {
    pub fn open<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let db_path = root.join(crate::core::constants::PATH_DATA);
        fs::create_dir_all(&db_path)?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(constants::DB_MAP_SIZE)
                .max_dbs(constants::DB_MAX_DBS)
                .open(&db_path)?
        };

        let mut txn = env.write_txn()?;
        let installed_db = env.create_database(&mut txn, Some("installed"))?;
        let available_db = env.create_database(&mut txn, Some("available"))?;
        let virtual_db = env.create_database(&mut txn, Some("virtual"))?;
        txn.commit()?;

        Ok(Self { env, root, installed_db, available_db, virtual_db })
    }

    pub fn begin_transaction(&self) -> Result<DbTransaction<'_>> {
        let txn = self.env.write_txn()?;
        // The transaction journal and backups must live under the real target
        // root so rollback can restore files; deriving it from the LMDB path
        // with fixed ancestor arithmetic breaks for nested roots.
        let tx_log = crate::core::transaction::PackageTransaction::new(
            self.root.clone(),
            crate::core::changelog::ActionKind::Installation,
        )?;

        Ok(DbTransaction {
            db: self,
            txn: Some(txn),
            tx_log,
            committed: false,
        })
    }

    pub fn get_all_installed_packages(&self) -> Result<Vec<PackageMetadata>> {
        let txn = self.env.read_txn()?;
        let mut results = Vec::new();
        let iter = self.installed_db.iter(&txn)?;
        for result in iter {
            let (_key, meta) = result?;
            results.push(meta);
        }
        Ok(results)
    }

    pub fn get_all_available_packages(&self) -> Result<Vec<PackageMetadata>> {
        let txn = self.env.read_txn()?;
        let mut results = Vec::new();
        let iter = self.available_db.iter(&txn)?;
        for result in iter {
            let (_key, meta) = result?;
            results.push(meta);
        }
        Ok(results)
    }

    pub fn is_package_installed(&self, pkg_name: &str) -> Result<bool> {
        let txn = self.env.read_txn()?;
        let exists = self.installed_db.get(&txn, pkg_name)?.is_some();
        Ok(exists)
    }

    pub fn get_package_manifest(&self, pkg_name: &str) -> Result<PackageMetadata> {
        let txn = self.env.read_txn()?;
        if let Some(meta) = self.installed_db.get(&txn, pkg_name)? {
            return Ok(meta);
        }
        if let Some(meta) = self.available_db.get(&txn, pkg_name)? {
            return Ok(meta);
        }
        Err(anyhow!("Package '{}' not found in registry", pkg_name))
    }

    pub fn env_read_txn(&self) -> Result<heed::RoTxn<'_>> {
        Ok(self.env.read_txn()?)
    }

    pub fn get_configured_repositories(&self) -> Result<Vec<RepositoryInfo>> {
        Err(anyhow!("Not supported via LMDB; use RepositoryManager"))
    }

    pub fn has_dependent_packages(&self, pkg_name: &str) -> Result<bool> {
        let txn = self.env.read_txn()?;
        let iter = self.installed_db.iter(&txn)?;
        for result in iter {
            let (_key, meta) = result?;
            if meta.pkg_name != pkg_name
                && meta.dependencies.iter().any(|d| d.name == pkg_name) {
                    return Ok(true);
                }
        }
        Ok(false)
    }
}

impl<'e> DbTransaction<'e> {
    fn txn(&mut self) -> &mut RwTxn<'e> {
        self.txn.as_mut().expect("DbTransaction: txn already consumed or committed")
    }

    pub fn register_package_placement(&mut self, meta: &PackageMetadata) -> Result<()> {
        // Check file collisions against installed packages
        let iter = self.db.installed_db.iter(self.txn())?;
        for result in iter {
            let (_key, installed): (_, PackageMetadata) = result?;
            if installed.pkg_name != meta.pkg_name {
                for file in &meta.files {
                    if installed.files.contains(file) {
                        return Err(anyhow!("File collision error: {:?} belongs to {}", file, installed.pkg_name));
                    }
                }
            }
        }

        if let Some(provides) = &meta.provides {
            for v in provides {
                self.db.virtual_db.put(self.txn(), v, &meta.pkg_name)?;
            }
        }

        self.db.installed_db.put(self.txn(), &meta.pkg_name, meta)?;
        self.tx_log.track_package(&meta.pkg_name)?;
        Ok(())
    }

    /// Clears the entire available-package index. Call once before refilling
    /// from every repository's cached index inside a single transaction so
    /// stale entries from removed/renamed packages do not accumulate.
    pub fn clear_available_index(&mut self) -> Result<()> {
        let mut keys = Vec::new();
        {
            let iter = self.db.available_db.iter(self.txn())?;
            for result in iter {
                let (key, _): (_, PackageMetadata) = result?;
                keys.push(key.to_string());
            }
        }
        for key in keys {
            self.db.available_db.delete(self.txn(), &key)?;
        }
        Ok(())
    }

    pub fn update_repository_index(&mut self, _repo_name: &str, index_path: &str) -> Result<()> {
        let content = fs::read_to_string(index_path)?;
        let remote_pkgs: Vec<PackageMetadata> = serde_json::from_str(&content)?;
        for pkg in remote_pkgs {
            if !crate::core::arch::package_matches_host(&pkg.architecture) {
                continue;
            }
            if let Some(provides) = &pkg.provides {
                for v in provides {
                    self.db.virtual_db.put(self.txn(), v, &pkg.pkg_name)?;
                }
            }
            self.db.available_db.put(self.txn(), &pkg.pkg_name, &pkg)?;
        }
        Ok(())
    }

    pub fn stage_package_removal(&mut self, name: &str) -> Result<()> {
        let Some(meta) = self.db.installed_db.get(self.txn(), name)? else {
            return Ok(());
        };

        if let Some(provides) = &meta.provides {
            for v in provides {
                // Re-point the virtual capability at another installed provider,
                // or drop it entirely — never leave a dangling mapping.
                let replacement = {
                    let mut owner = None;
                    let iter = self.db.installed_db.iter(self.txn())?;
                    for result in iter {
                        let (_key, other): (_, PackageMetadata) = result?;
                        if other.pkg_name != meta.pkg_name
                            && let Some(vs) = &other.provides
                            && vs.contains(v)
                        {
                            owner = Some(other.pkg_name.clone());
                            break;
                        }
                    }
                    owner
                };
                match replacement {
                    Some(provider) => { self.db.virtual_db.put(self.txn(), v, &provider)?; }
                    None => { self.db.virtual_db.delete(self.txn(), v)?; }
                }
            }
        }

        self.db.installed_db.delete(self.txn(), name)?;
        self.tx_log.track_package(name)?;
        Ok(())
    }

    pub fn backup_file(&mut self, path: &Path) -> Result<()> {
        self.tx_log.backup_file(path)
    }

    pub fn record_staged_file(&mut self, path: PathBuf) -> Result<()> {
        self.tx_log.record_staged_file(path)
    }

    /// Ordered commit protocol: journal intent, then commit the durable
    /// LMDB transaction, then mark the intent complete. Backups are kept
    /// until process exit.
    pub fn commit(mut self) -> Result<()> {
        self.tx_log.prepare_commit()?;
        if let Some(txn) = self.txn.take() {
            txn.commit()?;
        }
        self.tx_log.finalize_commit()?;
        self.committed = true;
        Ok(())
    }
}

impl<'e> Drop for DbTransaction<'e> {
    fn drop(&mut self) {
        if !self.committed {
            // RwTxn drops automatically (aborts) when Option::take'd on drop
            let _ = self.txn.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_metadata_accepts_both_arch_keys() {
        let canonical = r#"{
            "pkg_name": "test", "version": "1.0", "license": "MIT",
            "source": "https://x", "checksum": {"kind": "sha256", "value": "0"},
            "architecture": "x86_64"
        }"#;
        let m1: PackageMetadata = serde_json::from_str(canonical).expect("architecture key");
        assert_eq!(m1.architecture, "x86_64");

        let legacy = r#"{
            "pkg_name": "test", "version": "1.0", "license": "MIT",
            "source": "https://x", "checksum": {"kind": "sha256", "value": "0"},
            "arch": "aarch64"
        }"#;
        let m2: PackageMetadata = serde_json::from_str(legacy).expect("arch key");
        assert_eq!(m2.architecture, "aarch64");
    }

    #[test]
    fn test_checksum_data_parsing() {
        let json = r#"{"kind": "sha256", "value": "e3b0c44298fc1c149afbf4c8996fb924"}"#;
        let cs: ChecksumData = serde_json::from_str(json).expect("parse checksum object");
        assert_eq!(cs.kind, "sha256");
        assert_eq!(cs.value, "e3b0c44298fc1c149afbf4c8996fb924");
    }
}
