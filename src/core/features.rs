use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::core::database::PackageMetadata;














pub struct FeatureEngine {
    root: PathBuf,
}

impl FeatureEngine {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    
    
    

    pub fn generate_mount_service(
        &self,
        meta: &PackageMetadata,
        mount_point: &str,
    ) -> Result<PathBuf> {
        let dinit_dir = self.root.join("etc/dinit.d");
        fs::create_dir_all(&dinit_dir)
            .context("Failed to create dinit service directory")?;

        let service_file = dinit_dir.join(format!("mount-{}.dinit", meta.pkg_name));

        let script = format!(
            r#"# MCX lazy-mount for {name} v{ver}
type = script
command = /bin/mount {mount} 2>/dev/null || true
"#,
            name = meta.pkg_name,
            ver = meta.version,
            mount = mount_point,
        );

        fs::write(&service_file, &script)
            .with_context(|| format!("Failed to write lazy-mount dinit script for {}", meta.pkg_name))?;

        Ok(service_file)
    }

    pub fn remove_mount_service(&self, pkg_name: &str) -> Result<()> {
        let service_file = self.root.join(format!("etc/dinit.d/mount-{}.dinit", pkg_name));
        if service_file.exists() {
            fs::remove_file(&service_file)
                .with_context(|| format!("Failed to remove lazy-mount service for {}", pkg_name))?;
        }
        Ok(())
    }

    
    
    

    pub fn deduplicate_libraries(&self, pkg_staging: &Path, _meta: &PackageMetadata) -> Result<u64> {
        let cas_root = self.root.join("var/lib/mcx/cas");
        let mut saved_bytes: u64 = 0;

        let lib_dir = pkg_staging.join("system/lib");
        if !lib_dir.exists() {
            return Ok(0);
        }

        for entry in WalkDir::new(&lib_dir).min_depth(1).max_depth(1) {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let is_so = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "so" || path.to_string_lossy().contains(".so."))
                .unwrap_or(false);
            if !is_so {
                continue;
            }

            let hash = Self::sha256_file(path)?;
            let subdir = &hash[..2];
            let cas_path = cas_root.join(subdir).join(&hash);

            if cas_path.exists() {
                let original_size = fs::metadata(path)?.len();
                fs::remove_file(path)?;
                fs::hard_link(&cas_path, path)
                    .with_context(|| format!("Failed to create hard link for CAS dedup: {:?}", path))?;
                saved_bytes += original_size;
            } else {
                fs::create_dir_all(cas_root.join(subdir))
                    .context("Failed to create CAS subdirectory")?;
                fs::copy(path, &cas_path)
                    .with_context(|| format!("Failed to copy library to CAS store: {:?}", path))?;
            }
        }

        Ok(saved_bytes)
    }

    pub fn cas_stats(&self) -> Result<(u64, u64)> {
        let cas_root = self.root.join("var/lib/mcx/cas");
        if !cas_root.exists() {
            return Ok((0, 0));
        }

        let mut total_files: u64 = 0;
        let mut total_bytes: u64 = 0;

        for entry in WalkDir::new(&cas_root).min_depth(2).max_depth(2) {
            let entry = entry?;
            if entry.path().is_file() {
                total_files += 1;
                total_bytes += fs::metadata(entry.path())?.len();
            }
        }

        Ok((total_files, total_bytes))
    }

    
    
    

    pub fn enable_atomic_rollback(&self, meta: &PackageMetadata, pkg_installed_root: &Path) -> Result<()> {
        let gen_dir = self
            .root
            .join("var/lib/mcx/generations")
            .join(&meta.pkg_name);
        let active_link = self.root.join("var/lib/mcx/active").join(&meta.pkg_name);

        let gen_number = self.next_generation(&gen_dir)?;
        let gen_path = gen_dir.join(format!("{}", gen_number));
        fs::create_dir_all(&gen_path)
            .context("Failed to create generation directory")?;

        Self::copy_dir(pkg_installed_root, &gen_path)?;

        if active_link.exists() {
            fs::remove_file(&active_link)?;
        }
        fs::create_dir_all(active_link.parent().unwrap())?;
        let rel_target = format!("../../generations/{}/{}", meta.pkg_name, gen_number);
        symlink(&rel_target, &active_link)
            .with_context(|| format!("Failed to create atomic rollback symlink for {}", meta.pkg_name))?;

        Ok(())
    }

    pub fn rollback_to_generation(&self, pkg_name: &str, generation: u64) -> Result<()> {
        let gen_path = self
            .root
            .join("var/lib/mcx/generations")
            .join(pkg_name)
            .join(format!("{}", generation));

        if !gen_path.exists() {
            bail!("Generation {} for package '{}' does not exist", generation, pkg_name);
        }

        let active_link = self.root.join("var/lib/mcx/active").join(pkg_name);
        if active_link.exists() {
            fs::remove_file(&active_link)?;
        }
        fs::create_dir_all(active_link.parent().unwrap())?;
        let rel_target = format!("../../generations/{}/{}", pkg_name, generation);
        symlink(&rel_target, &active_link)
            .with_context(|| format!("Failed to flip rollback symlink for {}", pkg_name))?;

        Ok(())
    }

    pub fn list_generations(&self, pkg_name: &str) -> Result<Vec<u64>> {
        let gen_dir = self.root.join("var/lib/mcx/generations").join(pkg_name);
        if !gen_dir.exists() {
            return Ok(Vec::new());
        }

        let mut gens: Vec<u64> = fs::read_dir(&gen_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .collect();
        gens.sort_unstable();
        Ok(gens)
    }

    pub fn current_generation(&self, pkg_name: &str) -> Result<Option<u64>> {
        let active_link = self.root.join("var/lib/mcx/active").join(pkg_name);
        if !active_link.exists() {
            return Ok(None);
        }
        let target = fs::read_link(&active_link)?;
        target
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("Corrupted active symlink for {}", pkg_name))
    }

    
    
    

    pub fn reconstruct_delta(
        old_xcs: &Path,
        delta_xcd: &Path,
        output_path: &Path,
    ) -> Result<()> {
        let temp_dir = tempfile::tempdir().context("Failed to create temp dir for delta reconstruction")?;
        let staging = temp_dir.path().join("reconstruct");
        fs::create_dir_all(&staging)?;

        let old_file = fs::File::open(old_xcs)
            .with_context(|| format!("Failed to open old package: {:?}", old_xcs))?;
        let old_decoder = zstd::stream::Decoder::new(old_file)?;
        let mut old_archive = tar::Archive::new(old_decoder);
        old_archive.unpack(&staging)
            .context("Failed to extract old package during delta reconstruction")?;

        let delta_file = fs::File::open(delta_xcd)
            .with_context(|| format!("Failed to open delta file: {:?}", delta_xcd))?;
        let delta_decoder = zstd::stream::Decoder::new(delta_file)?;
        let mut delta_archive = tar::Archive::new(delta_decoder);

        let mut diff_meta: Option<DeltaMetadata> = None;

        for entry in delta_archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();

            if path == Path::new("diff.meta") {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                diff_meta = Some(serde_json::from_str(&content)
                    .context("Failed to parse delta metadata")?);
            } else if let Ok(rel) = path.strip_prefix("files/") {
                let dest = staging.join(rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                entry.unpack(&dest)?;
            }
        }

        let diff_meta = diff_meta.ok_or_else(|| anyhow::anyhow!("Delta archive missing diff.meta"))?;

        for deletion in &diff_meta.removed {
            let target = staging.join(deletion);
            if target.exists() {
                if target.is_dir() {
                    fs::remove_dir_all(&target)?;
                } else {
                    fs::remove_file(&target)?;
                }
            }
        }

        let out_file = fs::File::create(output_path)
            .with_context(|| format!("Failed to create output package: {:?}", output_path))?;
        let mut encoder = zstd::stream::Encoder::new(out_file, 3)?;

        {
            let mut builder = tar::Builder::new(&mut encoder);
            for entry in WalkDir::new(&staging).min_depth(1) {
                let entry = entry?;
                let path = entry.path();
                let rel = path.strip_prefix(&staging)?;
                if path.is_file() {
                    builder.append_path_with_name(path, rel)?;
                }
            }
            builder.finish()?;
        }

        encoder.finish()?;

        Ok(())
    }

    
    
    

    
    
    
    
    
    pub fn checkpoint_process(&self, pkg_name: &str, pid: u32) -> Result<PathBuf> {
        let snap_dir = self.root.join("var/lib/mcx/snapshots").join(pkg_name);
        fs::create_dir_all(&snap_dir)
            .context("Failed to create snapshot directory")?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let snap_file = snap_dir.join(format!("snap-{}.mem", timestamp));

        
        let mem_path = PathBuf::from(format!("/proc/{}/mem", pid));
        if !mem_path.exists() {
            
            let maps_path = format!("/proc/{}/maps", pid);
            if let Ok(maps) = fs::read_to_string(&maps_path) {
                let out_file = fs::File::create(&snap_file)?;
                let mut encoder = zstd::stream::Encoder::new(out_file, 3)?;
                encoder.write_all(maps.as_bytes())?;
                encoder.finish()?;
            } else {
                bail!("Process {} not accessible for checkpointing", pid);
            }
        } else {
            let out_file = fs::File::create(&snap_file)?;
            let mut encoder = zstd::stream::Encoder::new(out_file, 3)?;
            let mut mem_file = fs::File::open(&mem_path)?;
            std::io::copy(&mut mem_file, &mut encoder)?;
            encoder.finish()?;
        }

        Ok(snap_file)
    }

    
    pub fn list_snapshots(&self, pkg_name: &str) -> Result<Vec<PathBuf>> {
        let snap_dir = self.root.join("var/lib/mcx/snapshots").join(pkg_name);
        if !snap_dir.exists() {
            return Ok(Vec::new());
        }

        let mut snaps: Vec<PathBuf> = fs::read_dir(&snap_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "mem").unwrap_or(false))
            .collect();
        snaps.sort();
        Ok(snaps)
    }

    
    
    

    
    
    
    
    
    pub fn generate_stream_mount_script(
        &self,
        meta: &PackageMetadata,
        remote_url: &str,
        mount_point: &str,
    ) -> Result<PathBuf> {
        let stream_dir = self.root.join("var/lib/mcx/stream");
        fs::create_dir_all(&stream_dir)
            .context("Failed to create stream directory")?;

        let script_path = stream_dir.join(format!("{}.sh", meta.pkg_name));

        let script = format!(
            r#"#!/bin/sh
# MCX cloud-stream mount for {name} v{ver}
# Mounts remote .xcs via HTTP range-requests using squashfuse
URL="{url}"
MOUNT="{mount}"
CACHE="{cache}"
mkdir -p "$MOUNT" "$CACHE"
if ! mountpoint -q "$MOUNT" 2>/dev/null; then
    squashfuse "$URL" "$MOUNT" -o ro,allow_other,cache=cache_dir="$CACHE" 2>/dev/null || \
        echo "MCX: squashfuse not available; falling back to local cache" && \
        echo "MCX: Install squashfuse for streaming mounts"
fi
"#,
            name = meta.pkg_name,
            ver = meta.version,
            url = remote_url,
            mount = mount_point,
            cache = self.root.join("var/cache/mcx/stream").display(),
        );

        fs::write(&script_path, &script)
            .with_context(|| format!("Failed to write stream mount script for {}", meta.pkg_name))?;

        
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;

        Ok(script_path)
    }

    
    pub fn remove_stream_script(&self, pkg_name: &str) -> Result<()> {
        let script_path = self.root.join("var/lib/mcx/stream").join(format!("{}.sh", pkg_name));
        if script_path.exists() {
            fs::remove_file(&script_path)?;
        }
        Ok(())
    }

    
    
    

    
    
    
    
    
    
    pub fn create_isolated_overlay(&self, pkg_name: &str, home_overlay_path: &str) -> Result<PathBuf> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        let overlay_root = PathBuf::from(&home).join(".mcx/overlays").join(pkg_name);
        let upper = overlay_root.join("upper");
        let work = overlay_root.join("work");
        let merged = overlay_root.join("merged");

        fs::create_dir_all(&upper)
            .context("Failed to create overlay upper directory")?;
        fs::create_dir_all(&work)
            .context("Failed to create overlay work directory")?;
        fs::create_dir_all(&merged)
            .context("Failed to create overlay merged directory")?;

        
        let mount_script = overlay_root.join("mount-overlay.sh");
        let script = format!(
            r#"#!/bin/sh
# MCX isolated overlay mount for {pkg}
# Mounts overlayfs so {pkg} sees its own view of {target}
LOWER="{target}"
UPPER="{upper}"
WORK="{work}"
MERGED="{merged}"
mkdir -p "$LOWER" "$UPPER" "$WORK" "$MERGED"
mount -t overlay overlay -o lowerdir="$LOWER",upperdir="$UPPER",workdir="$WORK" "$MERGED" 2>/dev/null && \
    mount --bind "$MERGED" "$LOWER" 2>/dev/null || true
"#,
            pkg = pkg_name,
            target = home_overlay_path,
            upper = upper.display(),
            work = work.display(),
            merged = merged.display(),
        );

        fs::write(&mount_script, &script)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&mount_script, fs::Permissions::from_mode(0o755))?;

        Ok(overlay_root)
    }

    
    pub fn remove_isolated_overlay(&self, pkg_name: &str) -> Result<()> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        let overlay_root = PathBuf::from(&home).join(".mcx/overlays").join(pkg_name);
        if overlay_root.exists() {
            fs::remove_dir_all(&overlay_root)
                .with_context(|| format!("Failed to remove overlay for {}", pkg_name))?;
        }
        Ok(())
    }

    
    
    

    
    
    
    
    
    
    
    pub fn register_swarm_hash(&self, meta: &PackageMetadata, swarm_hash: &str) -> Result<()> {
        let swarm_dir = self.root.join("var/lib/mcx/swarm");
        fs::create_dir_all(&swarm_dir)?;

        let entry_path = swarm_dir.join(format!("{}.json", meta.pkg_name));
        let entry = SwarmEntry {
            pkg_name: meta.pkg_name.clone(),
            version: meta.version.clone(),
            swarm_hash: swarm_hash.to_string(),
            registered_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        let payload = serde_json::to_string_pretty(&entry)?;
        fs::write(&entry_path, payload)?;

        Ok(())
    }

    
    pub fn get_swarm_hash(&self, pkg_name: &str) -> Result<Option<String>> {
        let entry_path = self.root.join("var/lib/mcx/swarm").join(format!("{}.json", pkg_name));
        if !entry_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&entry_path)?;
        let entry: SwarmEntry = serde_json::from_str(&content)?;
        Ok(Some(entry.swarm_hash))
    }

    
    pub fn list_swarm_peers(&self) -> Result<Vec<SwarmPeer>> {
        let peer_path = self.root.join("var/lib/mcx/swarm/peers.json");
        if !peer_path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&peer_path)?;
        let peers: Vec<SwarmPeer> = serde_json::from_str(&content)?;
        Ok(peers)
    }

    
    pub fn register_swarm_peer(&self, peer: SwarmPeer) -> Result<()> {
        let swarm_dir = self.root.join("var/lib/mcx/swarm");
        fs::create_dir_all(&swarm_dir)?;

        let peer_path = swarm_dir.join("peers.json");
        let mut peers = if peer_path.exists() {
            let content = fs::read_to_string(&peer_path)?;
            serde_json::from_str::<Vec<SwarmPeer>>(&content)?
        } else {
            Vec::new()
        };

        
        if let Some(pos) = peers.iter().position(|p| p.address == peer.address) {
            peers[pos] = peer;
        } else {
            peers.push(peer);
        }

        fs::write(&peer_path, serde_json::to_string_pretty(&peers)?)?;
        Ok(())
    }

    
    
    

    pub fn enforce_resource_limits(
        &self,
        pkg_name: &str,
        max_memory_mb: u64,
        max_cpu_percent: u64,
    ) -> Result<()> {
        let cg_dir = PathBuf::from("/sys/fs/cgroup/mcx");
        let pkg_cg = cg_dir.join(sanitize_cgroup_name(pkg_name));

        
        fs::create_dir_all(&pkg_cg)
            .with_context(|| format!("Failed to create cgroup for {}", pkg_name))?;

        
        let mem_max = pkg_cg.join("memory.max");
        if mem_max.exists() {
            fs::write(&mem_max, format!("{}", max_memory_mb * 1024 * 1024))
                .with_context(|| format!("Failed to set memory limit for {}", pkg_name))?;
        }

        
        let cpu_max = pkg_cg.join("cpu.max");
        if cpu_max.exists() {
            let quota = (max_cpu_percent * 100) / 100; 
            fs::write(&cpu_max, format!("{} 100000", quota))
                .with_context(|| format!("Failed to set CPU quota for {}", pkg_name))?;
        }

        Ok(())
    }

    pub fn remove_resource_limits(&self, pkg_name: &str) -> Result<()> {
        let cg_dir = PathBuf::from("/sys/fs/cgroup/mcx");
        let pkg_cg = cg_dir.join(sanitize_cgroup_name(pkg_name));
        if pkg_cg.exists() {
            fs::remove_dir(&pkg_cg)
                .with_context(|| format!("Failed to remove cgroup for {}", pkg_name))?;
        }
        Ok(())
    }

    
    
    

    fn sha256_file(path: &Path) -> Result<String> {
        let data = fs::read(path)
            .with_context(|| format!("Failed to read file for hashing: {:?}", path))?;
        let hash = Sha256::digest(&data);
        Ok(format!("{:x}", hash))
    }

    fn next_generation(&self, gen_dir: &Path) -> Result<u64> {
        if !gen_dir.exists() {
            return Ok(1);
        }
        let max = fs::read_dir(gen_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(max + 1)
    }

    fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
        for entry in WalkDir::new(src).min_depth(1) {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(src)?;
            let dest = dst.join(rel);

            if path.is_dir() {
                fs::create_dir_all(&dest)?;
            } else if path.is_file() {
                fs::create_dir_all(dest.parent().unwrap())?;
                fs::copy(path, &dest)?;
            }
        }
        Ok(())
    }
}





#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaMetadata {
    pub pkg_name: String,
    pub from_version: String,
    pub to_version: String,
    #[serde(default)]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmEntry {
    pub pkg_name: String,
    pub version: String,
    pub swarm_hash: String,
    pub registered_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmPeer {
    pub address: String,      
    pub peer_id: String,
    pub last_seen: u64,
    pub advertised_hashes: Vec<String>,
}

fn sanitize_cgroup_name(name: &str) -> String {
    name.replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "_")
}