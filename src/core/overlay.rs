use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

pub struct OverlayManager {
    overlays_base: PathBuf,
}

impl OverlayManager {
    pub fn new(home: &Path) -> Self {
        Self {
            overlays_base: home.join(".mcx/overlays"),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.overlays_base)
            .context("Failed to allocate overlayfs base directory")
    }

    pub fn create_isolated_overlay(&self, pkg_name: &str, lower_root: &Path) -> Result<PathBuf> {
        let overlay_dir = self.overlays_base.join(sanitise(pkg_name));
        let upper = overlay_dir.join("upper");
        let work = overlay_dir.join("work");
        let merged = overlay_dir.join("merged");

        fs::create_dir_all(&upper)
            .with_context(|| format!("Failed to create overlay upper layer: {:?}", upper))?;
        fs::create_dir_all(&work)
            .with_context(|| format!("Failed to create overlay work dir: {:?}", work))?;
        fs::create_dir_all(&merged)
            .with_context(|| format!("Failed to create overlay merged view: {:?}", merged))?;

        let script_path = overlay_dir.join("mount-overlay.sh");
        let script = format!(
            r#"#!/bin/sh
# MCX overlay mount for {pkg}
LOWER="{lower}"
UPPER="{upper}"
WORK="{work}"
MERGED="{merged}"
mkdir -p "$MERGED" "$UPPER" "$WORK"
mount -t overlay overlay -o lowerdir="$LOWER",upperdir="$UPPER",workdir="$WORK" "$MERGED"
echo "Overlay mounted: {pkg} -> $MERGED"
"#,
            pkg = pkg_name,
            lower = lower_root.display(),
            upper = upper.display(),
            work = work.display(),
            merged = merged.display(),
        );

        fs::write(&script_path, &script)
            .with_context(|| format!("Failed to write overlay mount script: {:?}", script_path))?;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permissions on overlay mount script")?;

        Ok(merged)
    }

    pub fn remove_isolated_overlay(&self, pkg_name: &str) -> Result<()> {
        let overlay_dir = self.overlays_base.join(sanitise(pkg_name));
        let merged = overlay_dir.join("merged");
        if merged.exists() {
            let _ = std::process::Command::new("umount")
                .arg(&merged)
                .output();
        }
        if overlay_dir.exists() {
            fs::remove_dir_all(&overlay_dir)
                .with_context(|| format!("Failed to remove overlay directory: {:?}", overlay_dir))?;
        }
        Ok(())
    }

    pub fn list_overlays(&self) -> Result<Vec<PathBuf>> {
        if !self.overlays_base.exists() {
            return Ok(Vec::new());
        }
        let mut overlays = Vec::new();
        for entry in fs::read_dir(&self.overlays_base)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                overlays.push(path);
            }
        }
        Ok(overlays)
    }
}

fn sanitise(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
