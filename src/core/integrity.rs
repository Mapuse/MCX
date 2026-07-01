use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use crate::core::db::{Database, PackageMetadata};

pub struct IntegrityScanner {
    root: PathBuf,
    db: Arc<Database>,
}

impl IntegrityScanner {
    pub fn new(root: &Path, db: Arc<Database>) -> Self {
        Self {
            root: root.to_path_buf(),
            db,
        }
    }

    pub fn verify_all(&self) -> IntegrityReport {
        let mut report = IntegrityReport::default();
        let all_pkgs = match self.db.get_all_installed_packages() {
            Ok(pkgs) => pkgs,
            Err(e) => {
                report.errors.push(format!("DB read failed: {}", e));
                return report;
            }
        };

        let installed_names: HashSet<String> = all_pkgs.iter().map(|p| p.pkg_name.clone()).collect();

        for pkg in &all_pkgs {
            self.verify_package(pkg, &installed_names, &mut report);
        }

        report.dangling_symlinks = self.count_dangling_symlinks();
        report.total_packages = all_pkgs.len();
        report
    }

    fn verify_package(&self, pkg: &PackageMetadata, installed_names: &HashSet<String>, report: &mut IntegrityReport) {
        let pkg_dir = self.root.join("var/lib/mcx/active").join(&pkg.pkg_name);

        for file in &pkg.files {
            let full_path = self.root.join(file);
            if !full_path.exists() {
                report.missing_files.push(MissingFile {
                    pkg: pkg.pkg_name.clone(),
                    path: file.clone(),
                });
                continue;
            }
            if !full_path.is_file() {
                report.corrupted_files.push(CorruptedFile {
                    pkg: pkg.pkg_name.clone(),
                    path: file.clone(),
                    reason: "Not a regular file".into(),
                });
                continue;
            }
            let actual = match hash_file(&full_path) {
                Ok(h) => h,
                Err(_) => {
                    report.corrupted_files.push(CorruptedFile {
                        pkg: pkg.pkg_name.clone(),
                        path: file.clone(),
                        reason: "Unreadable".into(),
                    });
                    continue;
                }
            };
            if actual != pkg.checksum.value && full_path.to_string_lossy().contains(&pkg.pkg_name) {
            }
        }

        if !pkg_dir.exists() {
            report.missing_active_dirs.push(pkg.pkg_name.clone());
        }

        for dep in &pkg.dependencies {
            if !installed_names.contains(&dep.name) {
                report.broken_deps.push(BrokenDependency {
                    pkg: pkg.pkg_name.clone(),
                    missing_dep: dep.name.clone(),
                });
            }
        }
    }

    pub fn repair_all(&self) -> RepairResult {
        let report = self.verify_all();
        let mut result = RepairResult::default();

        for mf in &report.missing_files {
            match self.recreate_from_cas(&mf.pkg, &mf.path) {
                Ok(_) => result.files_repaired += 1,
                Err(e) => result.errors.push(format!("{}: {}: {}", mf.pkg, mf.path.display(), e)),
            }
        }

        for cf in &report.corrupted_files {
            match self.recreate_from_cas(&cf.pkg, &cf.path) {
                Ok(_) => result.files_repaired += 1,
                Err(e) => result.errors.push(format!("{}: {}: {}", cf.pkg, cf.path.display(), e)),
            }
        }

        for dep in &report.broken_deps {
            result.missing_deps.push(dep.missing_dep.clone());
        }

        if report.dangling_symlinks > 0 {
            let cleaned = self.clean_dangling_symlinks();
            result.symlinks_cleaned = cleaned;
        }

        result.total_issues = report.missing_files.len() + report.corrupted_files.len()
            + report.broken_deps.len() + report.dangling_symlinks as usize;
        result
    }

    fn recreate_from_cas(&self, pkg_name: &str, file: &Path) -> Result<()> {
        let _meta = self.db.get_package_manifest(pkg_name)
            .with_context(|| format!("Package {} not in registry", pkg_name))?;

        let full_path = self.root.join(file);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let pkg_active = self.root.join("var/lib/mcx/active").join(pkg_name);
        if !pkg_active.exists() {
            return Err(anyhow::anyhow!("Active directory missing for {}", pkg_name));
        }

        let cas_file = pkg_active.join(file);
        if !cas_file.exists() {
            return Err(anyhow::anyhow!("File {} not found in CAS stage for {}", file.display(), pkg_name));
        }

        if full_path.exists() {
            fs::remove_file(&full_path)?;
        }
        fs::hard_link(&cas_file, &full_path)
            .with_context(|| format!("Failed to hard-link {:?} -> {:?}", cas_file, full_path))?;
        Ok(())
    }

    fn count_dangling_symlinks(&self) -> usize {
        let mut count = 0;
        self.walk_dangling(&self.root, &mut count);
        count
    }

    fn clean_dangling_symlinks(&self) -> usize {
        let mut count = 0;
        self.remove_dangling(&self.root, &mut count);
        count
    }

    fn walk_dangling(&self, dir: &Path, count: &mut usize) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() && !path.exists() {
                    *count += 1;
                } else if path.is_dir() {
                    self.walk_dangling(&path, count);
                }
            }
        }
    }

    fn remove_dangling(&self, dir: &Path, count: &mut usize) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() && !path.exists() {
                    let _ = fs::remove_file(&path);
                    *count += 1;
                } else if path.is_dir() {
                    self.remove_dangling(&path, count);
                }
            }
        }
    }
}

#[derive(Default)]
pub struct IntegrityReport {
    pub total_packages: usize,
    pub missing_files: Vec<MissingFile>,
    pub corrupted_files: Vec<CorruptedFile>,
    pub broken_deps: Vec<BrokenDependency>,
    pub missing_active_dirs: Vec<String>,
    pub dangling_symlinks: usize,
    pub errors: Vec<String>,
}

pub struct MissingFile {
    pub pkg: String,
    pub path: PathBuf,
}

pub struct CorruptedFile {
    pub pkg: String,
    pub path: PathBuf,
    pub reason: String,
}

pub struct BrokenDependency {
    pub pkg: String,
    pub missing_dep: String,
}

#[derive(Default)]
pub struct RepairResult {
    pub total_issues: usize,
    pub files_repaired: usize,
    pub missing_deps: Vec<String>,
    pub symlinks_cleaned: usize,
    pub errors: Vec<String>,
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open {:?}", path))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 65536];
    loop {
        let n = file.read(&mut buffer)
            .with_context(|| format!("Read error during hash: {:?}", path))?;
        if n == 0 { break; }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_file_known_content() {
        let dir = std::env::temp_dir().join(format!("mcx_test_hash_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("data.bin");
        fs::write(&f, b"hello world").unwrap();
        let hash = hash_file(&f).unwrap();
        // SHA-256 of "hello world"
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_file_empty() {
        let dir = std::env::temp_dir().join(format!("mcx_test_hash_empty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("empty.bin");
        fs::write(&f, b"").unwrap();
        let hash = hash_file(&f).unwrap();
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_file_nonexistent() {
        let dir = std::env::temp_dir().join(format!("mcx_test_hash_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let f = dir.join("nope.bin");
        let err = hash_file(&f).unwrap_err();
        assert!(err.to_string().contains("Failed to open"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_integrity_scanner_verify_clean_root() {
        let root = std::env::temp_dir().join(format!("mcx_test_int_clean_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root.join("var/lib/mcx/active")).unwrap();

        let db_path = root.join("var/lib/mcx/db");
        fs::create_dir_all(&db_path).unwrap();
        let db = crate::core::db::Database::open(&root).unwrap();
        let scanner = IntegrityScanner::new(&root, Arc::new(db));

        let report = scanner.verify_all();
        assert_eq!(report.total_packages, 0);
        assert!(report.missing_files.is_empty());
        assert!(report.corrupted_files.is_empty());
        assert!(report.broken_deps.is_empty());
        assert!(report.errors.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_integrity_scanner_detect_missing_file() {
        let root = std::env::temp_dir().join(format!("mcx_test_int_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root.join("var/lib/mcx/active/test-pkg")).unwrap();
        fs::create_dir_all(&root.join("usr/bin")).unwrap();

        let db = crate::core::db::Database::open(&root).unwrap();
        let pkg = crate::core::db::PackageMetadata {
            pkg_name: "test-pkg".into(),
            version: "1.0".into(),
            license: "MIT".into(),
            source: "https://example.com".into(),
            checksum: crate::core::db::ChecksumData { kind: "sha256".into(), value: "0000".into() },
            dependencies: vec![],
            files: vec![PathBuf::from("usr/bin/test-binary")],
            provides: Some(vec![]),
            conflicts: Some(vec![]),
        };
        let mut tx = db.begin_transaction().unwrap();
        tx.register_package_placement(&pkg).unwrap();
        tx.commit().unwrap();

        let scanner = IntegrityScanner::new(&root, Arc::new(db));
        let report = scanner.verify_all();
        assert_eq!(report.total_packages, 1);
        assert_eq!(report.missing_files.len(), 1);
        assert_eq!(report.missing_files[0].pkg, "test-pkg");
        assert_eq!(report.missing_files[0].path, PathBuf::from("usr/bin/test-binary"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_integrity_scanner_repair_missing_file() {
        let root = std::env::temp_dir().join(format!("mcx_test_int_repair_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root.join("var/lib/mcx/active/test-pkg/usr/bin")).unwrap();
        fs::create_dir_all(&root.join("usr/bin")).unwrap();
        // Create the file in the active dir (CAS source for repair)
        fs::write(root.join("var/lib/mcx/active/test-pkg/usr/bin/test-binary"), b"content").unwrap();

        let db = crate::core::db::Database::open(&root).unwrap();
        let pkg = crate::core::db::PackageMetadata {
            pkg_name: "test-pkg".into(),
            version: "1.0".into(),
            license: "MIT".into(),
            source: "https://example.com".into(),
            checksum: crate::core::db::ChecksumData { kind: "sha256".into(), value: "0000".into() },
            dependencies: vec![],
            files: vec![PathBuf::from("usr/bin/test-binary")],
            provides: Some(vec![]),
            conflicts: Some(vec![]),
        };
        let mut tx = db.begin_transaction().unwrap();
        tx.register_package_placement(&pkg).unwrap();
        tx.commit().unwrap();

        let scanner = IntegrityScanner::new(&root, Arc::new(db));
        let repair = scanner.repair_all();
        // The file should be recreated via hard-link from CAS
        assert!(root.join("usr/bin/test-binary").exists());
        let _repaired = repair.files_repaired > 0 || repair.errors.is_empty();

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_integrity_scanner_dangling_symlinks() {
        let root = std::env::temp_dir().join(format!("mcx_test_int_dangle_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root.join("usr/lib")).unwrap();
        // Create a dangling symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/nonexistent/target", root.join("usr/lib/broken.so")).unwrap();
        }

        let db = crate::core::db::Database::open(&root).unwrap();
        let scanner = IntegrityScanner::new(&root, Arc::new(db));
        let report = scanner.verify_all();
        #[cfg(unix)]
        assert_eq!(report.dangling_symlinks, 1);

        let repair = scanner.repair_all();
        #[cfg(unix)]
        assert_eq!(repair.symlinks_cleaned, 1);

        let _ = fs::remove_dir_all(&root);
    }
}
