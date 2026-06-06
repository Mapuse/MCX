use std::fs;
use std::path::PathBuf;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use crate::network::download::Downloader;
use crate::archive::hash::HashVerifier;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReleasePayload {
    pub version: String,
    pub target_architecture: String,
    pub download_url: String,
    pub checksum: String,
}

pub struct SelfUpdateManager {
    current_version: String,
    binary_path: PathBuf,
}

impl SelfUpdateManager {
    pub fn new(current_version: &str) -> Result<Self> {
        let binary_path = std::env::current_exe()?;
        Ok(Self {
            current_version: current_version.to_string(),
            binary_path,
        })
    }

    pub async fn check_for_updates(&self, update_channel_url: &str) -> Result<Option<ReleasePayload>> {
        let downloader = Downloader::new();
        let temp_dir = std::env::temp_dir();
        let metadata_target = temp_dir.join("mcx-update-check.json");
        downloader.download_package(update_channel_url, &metadata_target).await?;
        let content = fs::read_to_string(&metadata_target)?;
        let release: ReleasePayload = serde_json::from_str(&content)?;
        if release.version != self.current_version {
            Ok(Some(release))
        } else {
            Ok(None)
        }
    }

    pub async fn deploy_update(&self, release: &ReleasePayload) -> Result<()> {
        let parent_dir = self.binary_path.parent().ok_or_else(|| anyhow!("Invalid execution path context"))?;
        let temp_download_target = parent_dir.join("mcx.update_tmp");
        let backup_target = parent_dir.join("mcx.old");
        let downloader = Downloader::new();
        downloader.download_package(&release.download_url, &temp_download_target).await?;
        HashVerifier::verify_integrity(&temp_download_target, &release.checksum)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp_download_target, fs::Permissions::from_mode(0o755))?;
        }
        if self.binary_path.exists() {
            fs::rename(&self.binary_path, &backup_target)?;
        }
        if let Err(e) = fs::rename(&temp_download_target, &self.binary_path) {
            if backup_target.exists() {
                let _ = fs::rename(&backup_target, &self.binary_path);
            }
            return Err(e.into());
        }
        if backup_target.exists() {
            let _ = fs::remove_file(&backup_target);
        }
        Ok(())
    }
}