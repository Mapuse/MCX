use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use heed::{Env, EnvOpenOptions, RwTxn};
use heed::types::{Str, SerdeBincode};

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
    pub dependencies: Vec<Dependency>,
    pub files: Vec<PathBuf>,
    pub provides: Option<Vec<String>>,
    pub conflicts: Option<Vec<String>>,
    #[serde(default = "default_arch")]
    pub architecture: String,
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
        let db_path = root.as_ref().join("var/lib/mcx/data");
        fs::create_dir_all(&db_path)?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(10 * 1024 * 1024)
                .max_dbs(4)
                .open(&db_path)?
        };

        let mut txn = env.write_txn()?;
        let installed_db = env.create_database(&mut txn, Some("installed"))?;
        let available_db = env.create_database(&mut txn, Some("available"))?;
        let virtual_db = env.create_database(&mut txn, Some("virtual"))?;
        txn.commit()?;

        Ok(Self { env, installed_db, available_db, virtual_db })
    }

    pub fn begin_transaction(&self) -> Result<DbTransaction<'_>> {
        let txn = self.env.write_txn()?;
        let env_path = self.env.path();
        let tx_root = env_path
            .ancestors()
            .nth(3)
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let tx_log = crate::core::transaction::PackageTransaction::new(tx_root, crate::core::changelog::ActionKind::Installation)?;

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
            if meta.pkg_name != pkg_name {
                if meta.dependencies.iter().any(|d| d.name == pkg_name) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl<'e> DbTransaction<'e> {
    fn txn(&mut self) -> &mut RwTxn<'e> {
        self.txn.as_mut().unwrap()
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
        let existing = self.db.installed_db.get(self.txn(), name)?;
        if let Some(meta) = existing {
            if let Some(provides) = &meta.provides {
                for v in provides {
                    self.db.virtual_db.delete(self.txn(), v)?;
                }
            }
            self.db.installed_db.delete(self.txn(), name)?;
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
        if let Some(txn) = self.txn.take() {
            txn.commit()?;
        }
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
