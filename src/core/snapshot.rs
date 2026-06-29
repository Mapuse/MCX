use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Result, Context, anyhow};

pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

impl SnapshotManager {
    pub fn new(root: &Path) -> Self {
        Self {
            snapshots_dir: root.join("var/lib/mcx/snapshots"),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.snapshots_dir)
            .context("Failed to allocate snapshot storage directory infrastructure")
    }

    pub fn checkpoint_process(&self, pkg_name: &str, pid: u32) -> Result<PathBuf> {
        let pkg_dir = self.snapshots_dir.join(sanitise(pkg_name));
        fs::create_dir_all(&pkg_dir)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("Timestamp error: {}", e))?
            .as_nanos();
        let snapshot_path = pkg_dir.join(format!("snap-{}.mem", timestamp));

        let mem_path = PathBuf::from(format!("/proc/{}/mem", pid));
        let maps_path = PathBuf::from(format!("/proc/{}/maps", pid));

        let data = if mem_path.exists() {
            let mut buf = Vec::new();
            let mut file = fs::File::open(&mem_path)
                .with_context(|| format!("Failed to open process memory channel: {:?}", mem_path))?;
            file.read_to_end(&mut buf)?;
            buf
        } else if maps_path.exists() {
            let mut buf = Vec::new();
            let mut file = fs::File::open(&maps_path)
                .with_context(|| format!("Failed to open process maps channel: {:?}", maps_path))?;
            file.read_to_end(&mut buf)?;
            buf
        } else {
            return Err(anyhow!("No accessible process state interface for pid {}", pid));
        };

        let compressed = zstd::encode_all(data.as_slice(), 3)
            .context("Compression pipeline failure during snapshot encoding")?;
        fs::write(&snapshot_path, &compressed)
            .with_context(|| format!("Failed to persist snapshot payload to: {:?}", snapshot_path))?;

        Ok(snapshot_path)
    }

    pub fn list_snapshots(&self, pkg_name: &str) -> Result<Vec<PathBuf>> {
        let pkg_dir = self.snapshots_dir.join(sanitise(pkg_name));
        if !pkg_dir.exists() {
            return Ok(Vec::new());
        }
        let mut snapshots = Vec::new();
        for entry in fs::read_dir(&pkg_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "mem").unwrap_or(false) {
                snapshots.push(path);
            }
        }
        snapshots.sort();
        Ok(snapshots)
    }

    pub fn restore_snapshot(&self, snapshot_path: &Path, target_pid: u32) -> Result<()> {
        if !snapshot_path.exists() {
            return Err(anyhow!("Snapshot target not present: {:?}", snapshot_path));
        }
        let compressed = fs::read(snapshot_path)
            .with_context(|| format!("Failed to read snapshot archive: {:?}", snapshot_path))?;
        let data = zstd::decode_all(compressed.as_slice())
            .context("Decompression failure during snapshot restoration")?;

        let mem_path = PathBuf::from(format!("/proc/{}/mem", target_pid));
        if !mem_path.exists() {
            return Err(anyhow!("Target process not accessible: pid {}", target_pid));
        }
        fs::write(&mem_path, &data)
            .with_context(|| format!("Failed to write snapshot data to process memory: pid {}", target_pid))?;

        Ok(())
    }

    pub fn remove_snapshots(&self, pkg_name: &str) -> Result<()> {
        let pkg_dir = self.snapshots_dir.join(sanitise(pkg_name));
        if pkg_dir.exists() {
            fs::remove_dir_all(&pkg_dir)
                .with_context(|| format!("Failed to purge snapshot directory: {:?}", pkg_dir))?;
        }
        Ok(())
    }
}

fn sanitise(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
