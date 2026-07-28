use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use sha2::{Sha256, Digest};
use crate::core::constants;

pub struct CasStore {
    cas_dir: PathBuf,
}

#[derive(Default)]
pub struct CasStats {
    pub total_files: u64,
    pub unique_files: u64,
    pub bytes_total: u64,
    pub bytes_saved: u64,
}

impl CasStore {
    pub fn new(root: &Path) -> Self {
        Self {
            cas_dir: root.join("var/lib/mcx/cas"),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.cas_dir)
            .context("Failed to allocate CAS directory hierarchy")
    }

    pub fn deduplicate_libraries(&self, pkg_root: &Path) -> Result<CasStats> {
        let mut stats = CasStats::default();
        let mut seen_hashes: HashMap<String, PathBuf> = HashMap::new();

        let lib_paths = self.collect_shared_libraries(pkg_root);
        stats.total_files = lib_paths.len() as u64;

        for lib_path in &lib_paths {
            let hash = self.hash_file(lib_path)?;
            let cas_subdir = self.cas_dir.join(&hash[..2]);
            let cas_path = cas_subdir.join(&hash);

            if cas_path.exists() {
                let original_len = lib_path.metadata().map(|m| m.len()).unwrap_or(0);
                fs::hard_link(&cas_path, lib_path)
                    .with_context(|| format!("Failed to hard-link CAS copy to {:?}", lib_path))?;
                stats.bytes_saved += original_len;
            } else {
                fs::create_dir_all(&cas_subdir)?;
                fs::copy(lib_path, &cas_path)?;
                seen_hashes.insert(hash, lib_path.clone());
            }
        }

        stats.unique_files = seen_hashes.len() as u64;
        stats.bytes_total = seen_hashes.values()
            .filter_map(|p| p.metadata().ok())
            .map(|m| m.len())
            .sum::<u64>();

        Ok(stats)
    }

    pub fn cas_stats(&self) -> Result<CasStats> {
        let mut stats = CasStats::default();
        if !self.cas_dir.exists() {
            return Ok(stats);
        }
        for entry in fs::read_dir(&self.cas_dir)? {
            let entry = entry?;
            let subdir = entry.path();
            if subdir.is_dir() {
                for file in fs::read_dir(&subdir)? {
                    let file = file?;
                    if file.path().is_file() {
                        stats.unique_files += 1;
                        stats.bytes_total += file.path().metadata().map(|m| m.len()).unwrap_or(0);
                    }
                }
            }
        }
        Ok(stats)
    }

    fn collect_shared_libraries(&self, root: &Path) -> Vec<PathBuf> {
        let mut libraries = Vec::new();
        let lib_dirs = vec![
            root.join("usr/lib"),
            root.join("lib"),
            root.join("usr/lib64"),
            root.join("lib64"),
        ];
        for dir in &lib_dirs {
            if dir.exists() {
                self.walk_for_so(dir, &mut libraries);
            }
        }
        libraries
    }

    fn walk_for_so(&self, dir: &Path, acc: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.walk_for_so(&path, acc);
                } else if path.is_file() {
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default();
                    if name.starts_with("lib") && (name.contains(".so") || name.ends_with(".so")) {
                        acc.push(path);
                    }
                }
            }
        }
    }

    fn hash_file(&self, path: &Path) -> Result<String> {
        let mut file = fs::File::open(path)
            .with_context(|| format!("Failed to open CAS candidate: {:?}", path))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; constants::CAS_HASH_BUFFER_SIZE];
        loop {
            let n = file.read(&mut buffer)
                .with_context(|| format!("Read error during CAS hashing: {:?}", path))?;
            if n == 0 { break; }
            hasher.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}
