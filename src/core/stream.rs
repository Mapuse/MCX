use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

pub struct StreamManager {
    stream_dir: PathBuf,
}

impl StreamManager {
    pub fn new(root: &Path) -> Self {
        Self {
            stream_dir: root.join("var/lib/mcx/stream"),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.stream_dir)
            .context("Failed to allocate stream mount script directory")
    }

    pub fn generate_stream_mount_script(&self, pkg_name: &str, version: &str, url: &str) -> Result<PathBuf> {
        let script_path = self.stream_dir.join(format!("{}.sh", pkg_name));
        let mount_point = format!("/mnt/{}", pkg_name);
        let cache_dir = "/var/cache/mcx/stream";

        let script = format!(
            r#"#!/bin/sh
# MCX cloud-stream mount for {} v{}
URL="{}"
MOUNT="{}"
CACHE="{}"
mkdir -p "$MOUNT" "$CACHE"
if command -v squashfuse >/dev/null 2>&1; then
    squashfuse "$URL" "$MOUNT" -o ro,allow_other,cache=cache_dir="$CACHE"
    echo "Mounted {} at $MOUNT"
else
    echo "Warning: squashfuse not installed. Falling back to direct download."
    wget -q -O - "$URL" | tar -xz -C "$MOUNT"
fi
"#,
            pkg_name, version, url, mount_point, cache_dir, pkg_name
        );

        fs::write(&script_path, &script)
            .with_context(|| format!("Failed to write stream mount script: {:?}", script_path))?;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permissions on stream mount script")?;

        Ok(script_path)
    }

    pub fn remove_stream_script(&self, pkg_name: &str) -> Result<()> {
        let script_path = self.stream_dir.join(format!("{}.sh", pkg_name));
        if script_path.exists() {
            fs::remove_file(&script_path)
                .with_context(|| format!("Failed to remove stream mount script: {:?}", script_path))?;
        }
        Ok(())
    }

    pub fn list_stream_scripts(&self) -> Result<Vec<PathBuf>> {
        if !self.stream_dir.exists() {
            return Ok(Vec::new());
        }
        let mut scripts = Vec::new();
        for entry in fs::read_dir(&self.stream_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "sh").unwrap_or(false) {
                scripts.push(path);
            }
        }
        Ok(scripts)
    }
}
