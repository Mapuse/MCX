use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, Context};
use crate::core::db::Database;
use crate::core::constants;
use crate::utils::ui::UserInterface;

#[derive(Debug, Clone)]
pub struct AutoRemoveReport {
    pub orphaned_packages: Vec<OrphanedPackage>,
    pub unnecessary_libs: Vec<UnnecessaryLib>,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct OrphanedPackage {
    pub name: String,
    pub version: String,
    pub reason: OrphanReason,
}

#[derive(Debug, Clone)]
pub enum OrphanReason {
    NoReverseDeps,
    DependencyOfRemoved { parent: String },
    UnusedLibrary { libs: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct UnnecessaryLib {
    pub package: String,
    pub library: String,
    pub path: String,
}

const SYSTEM_PREFIXES: &[&str] = &["glibc", "musl", "kernel", "systemd", "cesar"];

fn is_protected(name: &str) -> bool {
    SYSTEM_PREFIXES.iter().any(|p| name.starts_with(p))
}

fn is_library_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.starts_with("lib") && name.ends_with(".so") {
        return true;
    }
    if name.starts_with("lib") {
        let rest = &name[3..];
        if let Some(pos) = rest.find(".so") {
            let suffix = &rest[pos..];
            return suffix == ".so" || suffix.starts_with(".so.");
        }
    }
    false
}

fn is_core_library(name: &str) -> bool {
    const CORE: &[&str] = &[
        "libc.so", "libm.so", "libdl.so", "libpthread.so",
        "librt.so", "libresolv.so", "libnss_", "libcrypt.so",
        "libutil.so", "libgcc_s.so", "ld-linux",
    ];
    CORE.iter().any(|c| name.starts_with(c))
}

pub struct AutoRemoveAnalyzer {
    db: Arc<Database>,
    root: String,
}

impl AutoRemoveAnalyzer {
    pub fn new(db: Arc<Database>, root: &str) -> Self {
        Self { db, root: root.to_string() }
    }

    pub fn analyze(&self) -> Result<AutoRemoveReport> {
        let orphaned = self.find_orphans()?;
        let unnecessary = self.find_unnecessary_libs()?;
        let total = self.compute_orphan_size(&orphaned)?;

        Ok(AutoRemoveReport {
            orphaned_packages: orphaned,
            unnecessary_libs: unnecessary,
            total_size_bytes: total,
        })
    }

    pub fn find_orphans(&self) -> Result<Vec<OrphanedPackage>> {
        let installed = self.db.get_all_installed_packages()?;

        let mut reverse_deps: HashMap<String, Vec<String>> = HashMap::new();
        for pkg in &installed {
            for dep in &pkg.dependencies {
                reverse_deps.entry(dep.name.clone()).or_default().push(pkg.pkg_name.clone());
            }
        }

        let mut orphans: Vec<OrphanedPackage> = Vec::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        for pkg in &installed {
            if is_protected(&pkg.pkg_name) {
                continue;
            }
            let deps_of = reverse_deps.get(&pkg.pkg_name).map(|v| v.len()).unwrap_or(0);
            if deps_of == 0 && pkg.dependencies.is_empty() {
                continue;
            }
            if deps_of == 0 && pkg.provides.is_some() {
                continue;
            }

            let rdeps = reverse_deps.get(&pkg.pkg_name).map(|v| v.as_slice()).unwrap_or(&[]);
            if rdeps.is_empty() {
                orphans.push(OrphanedPackage {
                    name: pkg.pkg_name.clone(),
                    version: pkg.version.clone(),
                    reason: OrphanReason::NoReverseDeps,
                });
                queue.push_back(pkg.pkg_name.clone());
            }
        }

        let mut visited: HashSet<String> = orphans.iter().map(|o| o.name.clone()).collect();
        while let Some(candidate) = queue.pop_front() {
            if !visited.contains(&candidate) {
                continue;
            }
            if let Some(depended_by) = reverse_deps.get(&candidate) {
                for parent_name in depended_by {
                    if visited.contains(parent_name) || is_protected(parent_name) {
                        continue;
                    }
                    if let Some(parent_meta) = installed.iter().find(|p| p.pkg_name == *parent_name) {
                        let parent_rdeps = reverse_deps.get(parent_name).map(|v| v.as_slice()).unwrap_or(&[]);
                        let still_needed = parent_rdeps.iter().any(|r| !visited.contains(r));
                        if !still_needed {
                            orphans.push(OrphanedPackage {
                                name: parent_meta.pkg_name.clone(),
                                version: parent_meta.version.clone(),
                                reason: OrphanReason::DependencyOfRemoved { parent: candidate.to_string() },
                            });
                            visited.insert(parent_name.clone());
                            queue.push_back(parent_name.clone());
                        }
                    }
                }
            }
        }

        Ok(orphans)
    }

    pub fn find_unnecessary_libs(&self) -> Result<Vec<UnnecessaryLib>> {
        let installed = self.db.get_all_installed_packages()?;

        let mut all_library_deps: HashSet<String> = HashSet::new();
        for pkg in &installed {
            for dep in &pkg.dependencies {
                if let Some(ref libs) = dep.libraries {
                    for lib in libs {
                        all_library_deps.insert(lib.clone());
                    }
                }
            }
        }

        let mut unnecessary = Vec::new();
        for pkg in &installed {
            for file_path in &pkg.files {
                let path_str = file_path.to_string_lossy();
                let fname = path_str.rsplit('/').next().unwrap_or(&path_str);
                if !is_library_file(fname) || is_core_library(fname) {
                    continue;
                }
                let lib_stem = fname.strip_suffix(".so").unwrap_or(fname);
                let needed = all_library_deps.iter().any(|d| {
                    d == fname || d.starts_with(&format!("{}.", lib_stem)) || d.starts_with(&format!("{}-", lib_stem))
                });
                if !needed {
                    unnecessary.push(UnnecessaryLib {
                        package: pkg.pkg_name.clone(),
                        library: fname.to_string(),
                        path: path_str.to_string(),
                    });
                }
            }
        }

        Ok(unnecessary)
    }

    pub fn compute_orphan_size(&self, orphans: &[OrphanedPackage]) -> Result<u64> {
        let root = std::path::Path::new(&self.root);
        let active_dir = root.join(constants::PATH_ACTIVE);
        let mut total: u64 = 0;

        for orphan in orphans {
            if let Ok(meta) = self.db.get_package_manifest(&orphan.name) {
                total += meta.files.len() as u64;
            }
            let pkg_active = active_dir.join(&orphan.name);
            if let Ok(meta) = fs::metadata(&pkg_active) {
                total += meta.len();
            }
        }

        Ok(total)
    }

    pub fn print_report(&self, report: &AutoRemoveReport) {
        if report.orphaned_packages.is_empty() && report.unnecessary_libs.is_empty() {
            UserInterface::success("No unnecessary packages found. System is clean.");
            return;
        }

        if !report.orphaned_packages.is_empty() {
            UserInterface::info(&format!("Found {} orphaned package(s):", report.orphaned_packages.len()));
            for orphan in &report.orphaned_packages {
                let reason_str = match &orphan.reason {
                    OrphanReason::NoReverseDeps => "no reverse dependencies".to_string(),
                    OrphanReason::DependencyOfRemoved { parent } => format!("dependency of removed '{}'", parent),
                    OrphanReason::UnusedLibrary { libs } => format!("unused library: {}", libs.join(", ")),
                };
                UserInterface::info(&format!("  {} {} — {}", orphan.name, orphan.version, reason_str));
            }
            UserInterface::info(&format!("Total reclaimable: {} file(s)", report.total_size_bytes));
        }

        if !report.unnecessary_libs.is_empty() {
            UserInterface::info(&format!("Found {} unnecessary library file(s):", report.unnecessary_libs.len()));
            let mut by_package: std::collections::BTreeMap<&str, Vec<&UnnecessaryLib>> = std::collections::BTreeMap::new();
            for lib in &report.unnecessary_libs {
                by_package.entry(lib.package.as_str()).or_default().push(lib);
            }
            for (pkg, libs) in &by_package {
                UserInterface::info(&format!("  {}:", pkg));
                for lib in libs {
                    UserInterface::info(&format!("    {} ({})", lib.library, lib.path));
                }
            }
        }
    }

    pub fn execute_removal(&self, report: &AutoRemoveReport) -> Result<usize> {
        if report.orphaned_packages.is_empty() {
            UserInterface::info("Nothing to remove.");
            return Ok(0);
        }

        let root = std::path::Path::new(&self.root);
        let active_dir = root.join(constants::PATH_ACTIVE);
        let services_dir = root.join(constants::CESAR_SERVICES_DIR);
        let mut tx = self.db.begin_transaction()?;

        let mut removed = 0usize;

        let removed_set: HashSet<&str> = report.orphaned_packages.iter().map(|o| o.name.as_str()).collect();
        let shared_files = self.compute_shared_files(&removed_set)?;

        for orphan in &report.orphaned_packages {
            if !self.db.is_package_installed(&orphan.name)? {
                UserInterface::warning(&format!("'{}' is no longer installed, skipping", orphan.name));
                continue;
            }

            let manifest = self.db.get_package_manifest(&orphan.name)
                .with_context(|| format!("Failed to retrieve manifest for '{}'", orphan.name))?;

            UserInterface::info(&format!("Removing '{}'...", orphan.name));

            let pkg_active = active_dir.join(&orphan.name);
            if pkg_active.exists() {
                let _ = fs::remove_dir_all(&pkg_active);
            }

            let mut file_paths: Vec<PathBuf> = manifest.files.iter()
                .map(|f| root.join(f))
                .collect();
            file_paths.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

            for abs in &file_paths {
                if !abs.exists() || shared_files.contains(abs) {
                    continue;
                }
                if abs.is_dir() {
                    if let Ok(mut entries) = fs::read_dir(abs) {
                        if entries.next().is_none() {
                            let _ = fs::remove_dir(abs);
                        }
                    }
                } else {
                    let _ = fs::remove_file(abs);
                }
            }

            for svc in manifest.all_services() {
                let ini_path = services_dir.join(format!("{}.ini", svc.name));
                if ini_path.exists() {
                    let _ = fs::remove_file(&ini_path);
                }
            }

            tx.stage_package_removal(&orphan.name)?;
            removed += 1;

            UserInterface::success(&format!("Removed '{}' {}", orphan.name, orphan.version));
        }

        tx.commit()?;
        Ok(removed)
    }

    fn compute_shared_files(&self, exclude: &HashSet<&str>) -> Result<HashSet<PathBuf>> {
        let all = self.db.get_all_installed_packages()?;
        let root = std::path::Path::new(&self.root);
        let mut shared = HashSet::new();
        for pkg in &all {
            if exclude.contains(pkg.pkg_name.as_str()) {
                continue;
            }
            for f in &pkg.files {
                shared.insert(root.join(f));
            }
        }
        Ok(shared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orphan_reason_debug() {
        let r1 = OrphanReason::NoReverseDeps;
        let r2 = OrphanReason::DependencyOfRemoved { parent: "foo".into() };
        let r3 = OrphanReason::UnusedLibrary { libs: vec!["libbar.so".into()] };
        assert!(!format!("{:?}", r1).is_empty());
        assert!(!format!("{:?}", r2).is_empty());
        assert!(!format!("{:?}", r3).is_empty());
    }

    #[test]
    fn test_report_empty() {
        let report = AutoRemoveReport {
            orphaned_packages: vec![],
            unnecessary_libs: vec![],
            total_size_bytes: 0,
        };
        assert!(report.orphaned_packages.is_empty());
        assert!(report.total_size_bytes == 0);
    }

    #[test]
    fn test_is_protected() {
        assert!(is_protected("glibc"));
        assert!(is_protected("glibc-dev"));
        assert!(is_protected("musl"));
        assert!(is_protected("kernel"));
        assert!(is_protected("systemd"));
        assert!(is_protected("cesar"));
        assert!(!is_protected("vim"));
        assert!(!is_protected("openssl"));
    }

    #[test]
    fn test_is_library_file() {
        assert!(is_library_file("libfoo.so"));
        assert!(is_library_file("usr/lib/libbar.so.1.2"));
        assert!(is_library_file("libssl.so.3"));
        assert!(!is_library_file("usr/bin/ls"));
        assert!(!is_library_file("libfoo.a"));
        assert!(!is_library_file("libfoo"));
    }

    #[test]
    fn test_is_core_library() {
        assert!(is_core_library("libc.so.6"));
        assert!(is_core_library("libm.so"));
        assert!(is_core_library("libpthread.so.0"));
        assert!(!is_core_library("libfoo.so"));
        assert!(!is_core_library("libssl.so.3"));
    }

    #[test]
    fn test_orphan_reason_clone() {
        let r = OrphanReason::UnusedLibrary { libs: vec!["a".into(), "b".into()] };
        let cloned = r.clone();
        match cloned {
            OrphanReason::UnusedLibrary { libs } => assert_eq!(libs.len(), 2),
            _ => panic!("clone failed"),
        }
    }
}
