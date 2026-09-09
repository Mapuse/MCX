use crate::core::constants;
use crate::core::db::{Database, PackageMetadata};
use anyhow::{Context, Result};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

        let installed_names: HashSet<String> =
            all_pkgs.iter().map(|p| p.pkg_name.clone()).collect();

        for pkg in &all_pkgs {
            self.verify_package(pkg, &installed_names, &mut report);
        }

        report.dangling_symlinks = self.find_dangling_symlinks().len();
        report.total_packages = all_pkgs.len();
        report
    }

    fn verify_package(
        &self,
        pkg: &PackageMetadata,
        installed_names: &HashSet<String>,
        report: &mut IntegrityReport,
    ) {
        let pkg_dir = self.root.join(constants::PATH_ACTIVE).join(&pkg.pkg_name);

        if pkg.file_hashes.is_empty() {
            // Legacy package installed before per-file digests were recorded:
            // perform existence checks only. Re-hashing each placed file
            // against the whole-archive checksum produced false corruption
            // reports because archives embed metadata alongside payloads.
            for file in &pkg.files {
                let full_path = self.root.join(file);
                if !full_path.exists() {
                    report.missing_files.push(MissingFile {
                        pkg: pkg.pkg_name.clone(),
                        path: file.clone(),
                    });
                }
            }
        } else {
            for file in &pkg.files {
                let full_path = self.root.join(file);
                if !full_path.exists() {
                    report.missing_files.push(MissingFile {
                        pkg: pkg.pkg_name.clone(),
                        path: file.clone(),
                    });
                    continue;
                }
                let md = match fs::symlink_metadata(&full_path) {
                    Ok(md) => md,
                    Err(_) => {
                        report.corrupted_files.push(CorruptedFile {
                            pkg: pkg.pkg_name.clone(),
                            path: file.clone(),
                            reason: "Unreadable".into(),
                        });
                        continue;
                    }
                };
                // Symlinks are recreated verbatim at install time and carry
                // no digest entry; only regular files are hash-verified.
                if md.file_type().is_symlink() || !md.is_file() {
                    continue;
                }
                let Some(expected) = pkg.file_hashes.get(&file.to_string_lossy().into_owned())
                else {
                    continue;
                };
                let actual = match hash_file(&full_path, "sha256") {
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
                if actual != *expected {
                    report.corrupted_files.push(CorruptedFile {
                        pkg: pkg.pkg_name.clone(),
                        path: file.clone(),
                        reason: format!(
                            "Hash mismatch (sha256): got {}, expected {}",
                            actual, expected
                        ),
                    });
                }
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
                Err(e) => result
                    .errors
                    .push(format!("{}: {}: {}", mf.pkg, mf.path.display(), e)),
            }
        }

        for cf in &report.corrupted_files {
            match self.recreate_from_cas(&cf.pkg, &cf.path) {
                Ok(_) => result.files_repaired += 1,
                Err(e) => result
                    .errors
                    .push(format!("{}: {}: {}", cf.pkg, cf.path.display(), e)),
            }
        }

        for dep in &report.broken_deps {
            result.missing_deps.push(dep.missing_dep.clone());
        }

        if report.dangling_symlinks > 0 {
            let cleaned = self.clean_dangling_symlinks();
            result.symlinks_cleaned = cleaned;
        }

        result.total_issues = report.missing_files.len()
            + report.corrupted_files.len()
            + report.broken_deps.len()
            + report.dangling_symlinks;
        result
    }

    fn recreate_from_cas(&self, pkg_name: &str, file: &Path) -> Result<()> {
        let _meta = self
            .db
            .get_package_manifest(pkg_name)
            .with_context(|| format!("Package {} not in registry", pkg_name))?;

        let full_path = self.root.join(file);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let pkg_active = self.root.join(constants::PATH_ACTIVE).join(pkg_name);
        if !pkg_active.exists() {
            return Err(anyhow::anyhow!("Active directory missing for {}", pkg_name));
        }

        let cas_file = pkg_active.join(file);
        if !cas_file.exists() {
            return Err(anyhow::anyhow!(
                "File {} not found in CAS stage for {}",
                file.display(),
                pkg_name
            ));
        }

        if full_path.exists() {
            fs::remove_file(&full_path)?;
        }
        fs::hard_link(&cas_file, &full_path)
            .with_context(|| format!("Failed to hard-link {:?} -> {:?}", cas_file, full_path))?;
        Ok(())
    }

    /// Prefixes mcx manages; dangling-link scans never leave these trees.
    fn managed_prefixes(&self) -> Vec<PathBuf> {
        ["usr", "etc", "var"]
            .iter()
            .map(|p| self.root.join(p))
            .collect()
    }

    fn find_dangling_symlinks(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for prefix in self.managed_prefixes() {
            Self::walk_dangling(&prefix, &mut found);
        }
        found
    }

    /// Depth-first walk that uses `symlink_metadata`, so symlinked
    /// directories are never descended into — cycles and escapes out of the
    /// managed tree are impossible by construction.
    fn walk_dangling(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(md) = fs::symlink_metadata(&path) else {
                continue;
            };
            if md.file_type().is_symlink() {
                if !path.exists() {
                    out.push(path);
                }
            } else if md.is_dir() {
                Self::walk_dangling(&path, out);
            }
        }
    }

    /// Deletes dangling symlinks, but ONLY ones recorded in an installed
    /// package's manifest — unmanaged links belong to the user or another
    /// tool and must be left alone.
    fn clean_dangling_symlinks(&self) -> usize {
        let candidates = self.find_dangling_symlinks();
        if candidates.is_empty() {
            return 0;
        }

        let mut owned: HashSet<PathBuf> = HashSet::new();
        if let Ok(all) = self.db.get_all_installed_packages() {
            for pkg in &all {
                for f in &pkg.files {
                    owned.insert(self.root.join(f));
                }
            }
        }

        let mut cleaned = 0;
        for path in candidates {
            if !owned.contains(&path) {
                continue;
            }
            if let Err(e) = fs::remove_file(&path) {
                eprintln!(
                    "Warning: failed to remove dangling symlink {:?}: {}",
                    path, e
                );
            } else {
                cleaned += 1;
            }
        }
        cleaned
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

#[derive(Debug)]
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

fn hash_file(path: &Path, kind: &str) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("Failed to open {:?}", path))?;
    let mut buffer = vec![0u8; constants::INTEGRITY_HASH_BUFFER_SIZE];

    match kind {
        "sha256" | "sha-256" => {
            let mut hasher = Sha256::new();
            loop {
                let n = file
                    .read(&mut buffer)
                    .with_context(|| format!("Read error during hash: {:?}", path))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(format!("{:x}", hasher.finalize()))
        }
        "sha1" | "sha-1" => {
            let mut hasher = Sha1::new();
            loop {
                let n = file
                    .read(&mut buffer)
                    .with_context(|| format!("Read error during hash: {:?}", path))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(format!("{:x}", hasher.finalize()))
        }
        "md5" => {
            let mut hasher = Md5::new();
            loop {
                let n = file
                    .read(&mut buffer)
                    .with_context(|| format!("Read error during hash: {:?}", path))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(format!("{:x}", hasher.finalize()))
        }
        other => Err(anyhow::anyhow!("Unsupported checksum kind: '{}'", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_hash_file_known_content() {
        let dir = std::env::temp_dir().join(format!("mcx_test_hash_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let f = dir.join("data.bin");
        fs::write(&f, b"hello world").expect("write temp file");
        let hash = hash_file(&f, "sha256").expect("hash temp file");
        // SHA-256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_file_sha1() {
        let dir = std::env::temp_dir().join(format!("mcx_test_hash_sha1_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let f = dir.join("data.bin");
        fs::write(&f, b"hello world").expect("write temp file");
        let hash = hash_file(&f, "sha1").expect("hash temp file");
        // SHA-1 of "hello world"
        assert_eq!(hash, "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_file_md5() {
        let dir = std::env::temp_dir().join(format!("mcx_test_hash_md5_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let f = dir.join("data.bin");
        fs::write(&f, b"hello world").expect("write temp file");
        let hash = hash_file(&f, "md5").expect("hash temp file");
        // MD5 of "hello world"
        assert_eq!(hash, "5eb63bbbe01eeed093cb22bb8f5acdc3");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_file_empty() {
        let dir = std::env::temp_dir().join(format!("mcx_test_hash_empty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let f = dir.join("empty.bin");
        fs::write(&f, b"").expect("write temp file");
        let hash = hash_file(&f, "sha256").expect("hash temp file");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_file_nonexistent() {
        let dir =
            std::env::temp_dir().join(format!("mcx_test_hash_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let f = dir.join("nope.bin");
        let err = hash_file(&f, "sha256").expect_err("hash missing file");
        assert!(err.to_string().contains("Failed to open"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_file_unsupported_kind() {
        let dir =
            std::env::temp_dir().join(format!("mcx_test_hash_badkind_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let f = dir.join("data.bin");
        fs::write(&f, b"test").expect("write temp file");
        let err = hash_file(&f, "blake2").expect_err("hash unsupported kind");
        assert!(err.to_string().contains("Unsupported checksum kind"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_integrity_scanner_verify_clean_root() {
        let root = std::env::temp_dir().join(format!("mcx_test_int_clean_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("var/lib/mcx/active")).expect("create active dir");

        let db_path = root.join("var/lib/mcx/db");
        fs::create_dir_all(&db_path).expect("create db dir");
        let db = crate::core::db::Database::open(&root).expect("open test database");
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
        let root =
            std::env::temp_dir().join(format!("mcx_test_int_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("var/lib/mcx/active/test-pkg")).expect("create active dir");
        fs::create_dir_all(root.join("usr/bin")).expect("create usr bin dir");

        let db = crate::core::db::Database::open(&root).expect("open test database");
        let pkg = crate::core::db::PackageMetadata {
            pkg_name: "test-pkg".into(),
            version: "1.0".into(),
            license: "MIT".into(),
            source: "https://example.com".into(),
            checksum: crate::core::db::ChecksumData {
                kind: "sha256".into(),
                value: "0000".into(),
            },
            dependencies: vec![],
            files: vec![PathBuf::from("usr/bin/test-binary")],
            provides: Some(vec![]),
            conflicts: Some(vec![]),
            architecture: "native".to_string(),
            components: Vec::new(),
            services: Vec::new(),
            binaries: Vec::new(),
            file_hashes: HashMap::new(),
            provenance: None,
        };
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.register_package_placement(&pkg)
            .expect("register package placement");
        tx.commit().expect("commit transaction");

        let scanner = IntegrityScanner::new(&root, Arc::new(db));
        let report = scanner.verify_all();
        assert_eq!(report.total_packages, 1);
        assert_eq!(report.missing_files.len(), 1);
        assert_eq!(report.missing_files[0].pkg, "test-pkg");
        assert_eq!(
            report.missing_files[0].path,
            PathBuf::from("usr/bin/test-binary")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_integrity_scanner_repair_missing_file() {
        let root = std::env::temp_dir().join(format!("mcx_test_int_repair_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("var/lib/mcx/active/test-pkg/usr/bin"))
            .expect("create active dir");
        fs::create_dir_all(root.join("usr/bin")).expect("create usr bin dir");
        // Create the file in the active dir (CAS source for repair)
        fs::write(
            root.join("var/lib/mcx/active/test-pkg/usr/bin/test-binary"),
            b"content",
        )
        .expect("write temp file");

        let db = crate::core::db::Database::open(&root).expect("open test database");
        let pkg = crate::core::db::PackageMetadata {
            pkg_name: "test-pkg".into(),
            version: "1.0".into(),
            license: "MIT".into(),
            source: "https://example.com".into(),
            checksum: crate::core::db::ChecksumData {
                kind: "sha256".into(),
                value: "0000".into(),
            },
            dependencies: vec![],
            files: vec![PathBuf::from("usr/bin/test-binary")],
            provides: Some(vec![]),
            conflicts: Some(vec![]),
            architecture: "native".to_string(),
            components: Vec::new(),
            services: Vec::new(),
            binaries: Vec::new(),
            file_hashes: HashMap::new(),
            provenance: None,
        };
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.register_package_placement(&pkg)
            .expect("register package placement");
        tx.commit().expect("commit transaction");

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
        fs::create_dir_all(root.join("usr/lib")).expect("create usr lib dir");
        // Create a dangling symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/nonexistent/target", root.join("usr/lib/broken.so"))
                .expect("create symlink");
        }

        let db = crate::core::db::Database::open(&root).expect("open test database");
        let pkg = crate::core::db::PackageMetadata {
            pkg_name: "link-pkg".into(),
            version: "1.0".into(),
            license: "MIT".into(),
            source: "https://example.com".into(),
            checksum: crate::core::db::ChecksumData {
                kind: "sha256".into(),
                value: "0000".into(),
            },
            dependencies: vec![],
            files: vec![PathBuf::from("usr/lib/broken.so")],
            provides: Some(vec![]),
            conflicts: Some(vec![]),
            architecture: "native".to_string(),
            components: Vec::new(),
            services: Vec::new(),
            binaries: Vec::new(),
            file_hashes: HashMap::new(),
            provenance: None,
        };
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.register_package_placement(&pkg)
            .expect("register package placement");
        tx.commit().expect("commit transaction");

        let scanner = IntegrityScanner::new(&root, Arc::new(db));
        let report = scanner.verify_all();
        #[cfg(unix)]
        assert_eq!(report.dangling_symlinks, 1);

        let repair = scanner.repair_all();
        #[cfg(unix)]
        assert_eq!(repair.symlinks_cleaned, 1);

        let _ = fs::remove_dir_all(&root);
    }

    /// Unmanaged dangling symlinks must be reported but never deleted.
    #[cfg(unix)]
    #[test]
    fn test_repair_leaves_unowned_dangling_symlinks() {
        let root =
            std::env::temp_dir().join(format!("mcx_test_int_unowned_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("usr/lib")).expect("create usr lib dir");
        std::os::unix::fs::symlink("/nonexistent/target", root.join("usr/lib/user-link.so"))
            .expect("create symlink");

        let db = crate::core::db::Database::open(&root).expect("open test database");
        let scanner = IntegrityScanner::new(&root, Arc::new(db));

        let report = scanner.verify_all();
        assert_eq!(
            report.dangling_symlinks, 1,
            "unowned links are still counted"
        );

        let repair = scanner.repair_all();
        assert_eq!(
            repair.symlinks_cleaned, 0,
            "deletion is restricted to package-owned paths"
        );
        assert!(root.join("usr/lib/user-link.so").symlink_metadata().is_ok());

        let _ = fs::remove_dir_all(&root);
    }

    /// Packages installed with per-file digests get content verification;
    /// tampered content must be flagged as corrupted.
    #[test]
    fn test_verify_uses_recorded_file_hashes() {
        let root = std::env::temp_dir().join(format!("mcx_test_int_hashes_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("usr/bin")).expect("create dir");
        fs::write(root.join("usr/bin/tool"), b"original").expect("write file");

        let mut hashes = HashMap::new();
        hashes.insert(
            "usr/bin/tool".to_string(),
            hash_file(&root.join("usr/bin/tool"), "sha256").expect("hash"),
        );

        let db = crate::core::db::Database::open(&root).expect("open test database");
        let pkg = crate::core::db::PackageMetadata {
            pkg_name: "hashed-pkg".into(),
            version: "1.0".into(),
            license: "MIT".into(),
            source: "https://example.com".into(),
            checksum: crate::core::db::ChecksumData {
                kind: "sha256".into(),
                value: "deadbeef".into(),
            },
            dependencies: vec![],
            files: vec![PathBuf::from("usr/bin/tool")],
            provides: Some(vec![]),
            conflicts: Some(vec![]),
            architecture: "native".to_string(),
            components: Vec::new(),
            services: Vec::new(),
            binaries: Vec::new(),
            file_hashes: hashes,
            provenance: None,
        };
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.register_package_placement(&pkg)
            .expect("register placement");
        tx.commit().expect("commit");

        let scanner = IntegrityScanner::new(&root, Arc::new(db));

        // Intact state: no corruption reported despite the archive checksum
        // field being unrelated to file content.
        let clean = scanner.verify_all();
        assert!(
            clean.corrupted_files.is_empty(),
            "unexpected corruption: {:?}",
            clean.corrupted_files
        );

        // Tamper with the placed file: per-file digest must catch it.
        fs::write(root.join("usr/bin/tool"), b"tampered").expect("tamper");
        let bad = scanner.verify_all();
        assert_eq!(bad.corrupted_files.len(), 1, "tampering must be detected");
        assert!(bad.corrupted_files[0].reason.contains("Hash mismatch"));

        let _ = fs::remove_dir_all(&root);
    }
}
