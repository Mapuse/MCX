use crate::core::constants;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

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
            cas_dir: root.join(crate::core::constants::PATH_CAS),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.cas_dir).context("Failed to allocate CAS directory hierarchy")
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
                fs::remove_file(lib_path)?;
                if let Err(e) = fs::hard_link(&cas_path, lib_path) {
                    // Cross-device or unsupported: fall back to a plain copy
                    // instead of failing the whole dedup pass.
                    fs::copy(&cas_path, lib_path).with_context(|| {
                        format!(
                            "Failed to restore {:?} from CAS (hard-link error: {})",
                            lib_path, e
                        )
                    })?;
                }
                stats.bytes_saved += original_len;
            } else {
                fs::create_dir_all(&cas_subdir)?;
                // Stage through a unique temp file + rename: a crash mid-copy
                // must never leave truncated bytes under a content-addressed
                // name that later runs would trust blindly.
                let tmp = cas_subdir.join(format!(".{}.tmp-{}", hash, std::process::id()));
                fs::copy(lib_path, &tmp)
                    .with_context(|| format!("Failed to stage CAS entry for {:?}", lib_path))?;
                if let Err(e) = fs::rename(&tmp, &cas_path) {
                    let _ = fs::remove_file(&tmp);
                    return Err(e).context("Failed to promote staged CAS entry");
                }
                seen_hashes.insert(hash, lib_path.clone());
            }
        }

        stats.unique_files = seen_hashes.len() as u64;
        stats.bytes_total = seen_hashes
            .values()
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
                    let name = path
                        .file_name()
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
            let n = file
                .read(&mut buffer)
                .with_context(|| format!("Read error during CAS hashing: {:?}", path))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cas_dedup_roundtrip_is_content_safe() {
        let root = std::env::temp_dir().join(format!("mcx_test_cas_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let pkg_root = root.join("pkg");
        fs::create_dir_all(pkg_root.join("usr/lib")).unwrap();
        let payload: Vec<u8> = (0..64_000u32).map(|i| (i % 251) as u8).collect();
        for name in ["libone.so", "libtwo.so"] {
            fs::write(pkg_root.join("usr/lib").join(name), &payload).unwrap();
        }

        let cas = CasStore::new(&root);
        cas.initialize().unwrap();

        // First pass stores the blob; second pass links from the store.
        let first = cas.deduplicate_libraries(&pkg_root).expect("first dedup");
        assert_eq!(first.total_files, 2);
        assert_eq!(first.unique_files, 1, "identical blobs stored once");
        let second = cas.deduplicate_libraries(&pkg_root).expect("second dedup");
        assert!(
            second.bytes_saved > 0,
            "second pass must reclaim space via links"
        );

        // Content must survive both passes byte-for-byte.
        for name in ["libone.so", "libtwo.so"] {
            let back = fs::read(pkg_root.join("usr/lib").join(name)).unwrap();
            assert_eq!(back, payload, "{} corrupted by dedup", name);
        }
        let stats = cas.cas_stats().unwrap();
        assert_eq!(stats.unique_files, 1, "store holds exactly one blob");
        let _ = fs::remove_dir_all(&root);
    }
}
