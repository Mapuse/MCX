use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use anyhow::Result;
use serde::{Serialize, Deserialize};
use crate::core::constants;
use crate::core::db::Database;
use crate::utils::ui::UserInterface;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BinaryEntry {
    pub binary: String,
    pub package: String,
    pub version: String,
    pub component: String,
}

pub struct BinaryIndex {
    root: String,
    db: Arc<Database>,
}

impl BinaryIndex {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self { root, db }
    }

    pub fn lookup(&self, binary_name: &str) -> Vec<BinaryEntry> {
        let mut results = Vec::new();
        if let Ok(packages) = self.db.get_all_installed_packages() {
            for pkg in &packages {
                if pkg.binaries.iter().any(|b| b == binary_name) {
                    results.push(BinaryEntry {
                        binary: binary_name.to_string(),
                        package: pkg.pkg_name.clone(),
                        version: pkg.version.clone(),
                        component: "core".to_string(),
                    });
                }
                for comp in &pkg.components {
                    if comp.files.iter().any(|f| {
                        f.file_name().map(|n| n.to_string_lossy() == binary_name).unwrap_or(false)
                    }) {
                        results.push(BinaryEntry {
                            binary: binary_name.to_string(),
                            package: pkg.pkg_name.clone(),
                            version: pkg.version.clone(),
                            component: comp.name.clone(),
                        });
                    }
                }
            }
        }
        results
    }

    pub fn rebuild(&self) -> Result<usize> {
        let index_path = Path::new(&self.root).join(constants::PATH_BININDEX);
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut index: HashMap<String, Vec<BinaryEntry>> = HashMap::new();

        if let Ok(packages) = self.db.get_all_installed_packages() {
            for pkg in &packages {
                for binary in &pkg.binaries {
                    let entry = BinaryEntry {
                        binary: binary.clone(),
                        package: pkg.pkg_name.clone(),
                        version: pkg.version.clone(),
                        component: "core".to_string(),
                    };
                    index.entry(binary.clone()).or_default().push(entry);
                }
                for comp in &pkg.components {
                    for file in &comp.files {
                        if let Some(name) = file.file_name() {
                            let name_str = name.to_string_lossy();
                            if is_binary(file) {
                                let entry = BinaryEntry {
                                    binary: name_str.to_string(),
                                    package: pkg.pkg_name.clone(),
                                    version: pkg.version.clone(),
                                    component: comp.name.clone(),
                                };
                                index.entry(name_str.to_string()).or_default().push(entry);
                            }
                        }
                    }
                }
            }
        }

        let json = serde_json::to_string_pretty(&index)?;
        fs::write(&index_path, json)?;
        Ok(index.len())
    }
}

fn is_binary(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let no_ext = !name.contains('.');
    let looks_binary = path.starts_with("usr/bin")
        || path.starts_with("bin")
        || path.starts_with("sbin")
        || path.starts_with("usr/sbin");
    looks_binary && no_ext
}

pub fn command_not_found_handler(root: &str, db: Arc<Database>, command: &str) -> Result<()> {
    let index = BinaryIndex::new(root.to_string(), Arc::clone(&db));
    let entries = index.lookup(command);

    if entries.is_empty() {
        UserInterface::error(&format!("'{}' is not installed and no package provides it.", command));
        return Ok(());
    }

    let unique_pkgs: Vec<&BinaryEntry> = entries.iter().collect::<std::collections::HashSet<_>>().into_iter().collect();
    let primary = &entries[0];

    UserInterface::info(&format!("'{}' is not installed.", command));
    println!();
    UserInterface::info(&format!("The following package{} provide '{}':",
        if unique_pkgs.len() > 1 { "s" } else { "" }, command));

    let mut shown = std::collections::HashSet::new();
    for entry in &entries {
        if shown.insert(entry.package.clone()) {
            println!("  {} {} [component: {}]",
                entry.package, entry.version, entry.component);
        }
    }

    println!();
    if entries.len() == 1 && primary.component == "core" {
        UserInterface::info(&format!("To install, run: mcx install {}", primary.package));
    } else {
        let comp_names: Vec<String> = entries.iter()
            .map(|e| e.component.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        UserInterface::info(&format!("To install only '{}', run: mcx install {} --only {}",
            command, primary.package, command));
        if comp_names.len() > 1 {
            UserInterface::info(&format!("To install specific components: mcx install {} --components {}",
                primary.package, comp_names.join(",")));
        }
    }

    Ok(())
}

pub fn scan_system_binaries(root: &str) -> Vec<String> {
    let mut binaries = Vec::new();
    let bin_dirs = constants::BINARY_SCAN_DIRS;
    for dir in bin_dirs {
        let full = Path::new(root).join(dir);
        if let Ok(entries) = fs::read_dir(&full) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(meta) = fs::metadata(&path)
                            && meta.permissions().mode() & 0o111 != 0
                                && let Some(name) = path.file_name() {
                                    binaries.push(name.to_string_lossy().to_string());
                                }
                    }
                    #[cfg(not(unix))]
                    {
                        if let Some(name) = path.file_name() {
                            binaries.push(name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }
    binaries
}

pub fn detect_new_binaries(root: &str, before: &[String]) -> Vec<String> {
    let after = scan_system_binaries(root);
    let before_set: std::collections::HashSet<&String> = before.iter().collect();
    after.into_iter().filter(|b| !before_set.contains(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_binary() {
        assert!(is_binary(Path::new("usr/bin/ls")));
        assert!(is_binary(Path::new("bin/sh")));
        assert!(!is_binary(Path::new("usr/lib/libfoo.so.1")));
        assert!(!is_binary(Path::new("etc/config.conf")));
    }
}
