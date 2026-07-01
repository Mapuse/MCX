use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};
use crate::network::download::Downloader;

pub struct SelfUpdateManager;

impl SelfUpdateManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn binary(binary_url: &str, dest: &Path) -> Result<PathBuf> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let downloader = Downloader::new();
        let _ = downloader.package(binary_url, dest).await?;

        if !dest.exists() {
            return Err(anyhow!("Downloaded binary not found at {:?}", dest));
        }

        let verify = Command::new(dest)
            .arg("--version")
            .output()
            .map_err(|e| anyhow!("Verification execution failed: {}", e))?;
        if !verify.status.success() {
            let _ = std::fs::remove_file(dest);
            return Err(anyhow!("Downloaded binary failed version check"));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
        }

        Ok(dest.to_path_buf())
    }
}
