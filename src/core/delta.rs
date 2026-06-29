use std::collections::HashSet;
use std::fs;
use std::path::Path;
use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeltaManifest {
    pub pkg_name: String,
    pub from_version: String,
    pub to_version: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
    pub checksum: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeltaPackage {
    pub manifest: DeltaManifest,
    pub delta_data: Vec<DeltaBlock>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeltaBlock {
    pub path: String,
    pub offset: u64,
    pub length: u64,
    pub data: Vec<u8>,
    pub checksum: String,
}

pub struct DeltaEngine;

impl DeltaEngine {
    pub fn compute_delta(
        old_dir: &Path,
        new_dir: &Path,
        pkg_name: &str,
        from_version: &str,
        to_version: &str,
    ) -> Result<DeltaPackage> {
        let old_files = Self::walk_files(old_dir);
        let new_files = Self::walk_files(new_dir);

        let old_set: HashSet<&str> = old_files.keys().map(|s| s.as_str()).collect();
        let new_set: HashSet<&str> = new_files.keys().map(|s| s.as_str()).collect();

        let added: Vec<String> = new_set.difference(&old_set).map(|s| s.to_string()).collect();
        let removed: Vec<String> = old_set.difference(&new_set).map(|s| s.to_string()).collect();

        let mut modified = Vec::new();
        let mut delta_blocks = Vec::new();

        for path in new_set.intersection(&old_set) {
            let old_hash = &old_files[*path];
            let new_hash = &new_files[*path];
            if old_hash != new_hash {
                modified.push(path.to_string());
                let full_path = new_dir.join(path);
                let data = fs::read(&full_path)
                    .with_context(|| format!("Failed to read modified file: {:?}", full_path))?;
                let block_hash = format!("{:x}", Sha256::digest(&data));
                delta_blocks.push(DeltaBlock {
                    path: path.to_string(),
                    offset: 0,
                    length: data.len() as u64,
                    data,
                    checksum: block_hash,
                });
            }
        }

        for path in &added {
            let full_path = new_dir.join(path);
            let data = fs::read(&full_path)
                .with_context(|| format!("Failed to read added file: {:?}", full_path))?;
            let block_hash = format!("{:x}", Sha256::digest(&data));
            delta_blocks.push(DeltaBlock {
                path: path.to_string(),
                offset: 0,
                length: data.len() as u64,
                data,
                checksum: block_hash,
            });
        }

        let mut hasher = Sha256::new();
        for block in &delta_blocks {
            hasher.update(&block.checksum);
        }
        let checksum = format!("{:x}", hasher.finalize());

        Ok(DeltaPackage {
            manifest: DeltaManifest {
                pkg_name: pkg_name.to_string(),
                from_version: from_version.to_string(),
                to_version: to_version.to_string(),
                added,
                removed,
                modified,
                checksum,
            },
            delta_data: delta_blocks,
        })
    }

    pub fn apply_delta(
        base_dir: &Path,
        delta: &DeltaPackage,
        output_dir: &Path,
    ) -> Result<()> {
        if output_dir.exists() {
            fs::remove_dir_all(output_dir)?;
        }
        Self::copy_dir(base_dir, output_dir)?;

        for removal in &delta.manifest.removed {
            let target = output_dir.join(removal);
            if target.exists() {
                if target.is_dir() {
                    fs::remove_dir_all(&target)?;
                } else {
                    fs::remove_file(&target)?;
                }
            }
        }

        for block in &delta.delta_data {
            let target = output_dir.join(&block.path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, &block.data)?;
        }

        let mut hasher = Sha256::new();
        for block in &delta.delta_data {
            hasher.update(&block.checksum);
        }
        let computed = format!("{:x}", hasher.finalize());
        if computed != delta.manifest.checksum {
            anyhow::bail!(
                "Delta checksum mismatch: expected {}, computed {}",
                delta.manifest.checksum, computed
            );
        }

        Ok(())
    }

    pub fn write_delta(delta: &DeltaPackage, output_path: &Path) -> Result<()> {
        let payload = serde_json::to_string_pretty(delta)?;
        let compressed = zstd::encode_all(payload.as_bytes(), 3)?;
        fs::write(output_path, compressed)?;
        Ok(())
    }

    pub fn read_delta(path: &Path) -> Result<DeltaPackage> {
        let compressed = fs::read(path)?;
        let decompressed = zstd::decode_all(compressed.as_slice())?;
        let delta: DeltaPackage = serde_json::from_slice(&decompressed)?;
        Ok(delta)
    }

    fn walk_files(dir: &Path) -> std::collections::HashMap<String, String> {
        let mut files = std::collections::HashMap::new();
        if !dir.exists() { return files; }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    for (rel, hash) in Self::walk_files(&path) {
                        let full = format!("{}/{}",
                            path.file_name().unwrap().to_string_lossy(),
                            rel);
                        files.insert(full, hash);
                    }
                } else if path.is_file() {
                    let rel = path.strip_prefix(dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    if let Ok(data) = fs::read(&path) {
                        let hash = format!("{:x}", Sha256::digest(&data));
                        files.insert(rel, hash);
                    }
                }
            }
        }
        files
    }

    fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
        if !src.exists() { return Ok(()); }
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(src)
                .map_err(|_| anyhow!("Path strip error"))?;
            let dest = dst.join(rel);
            if path.is_dir() {
                fs::create_dir_all(&dest)?;
                Self::copy_dir(&path, &dest)?;
            } else if path.is_file() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&path, &dest)?;
            }
        }
        Ok(())
    }
}
