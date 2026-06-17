use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use crate::network::download::Downloader;
use crate::archive::hash::HashVerifier;
use crate::core::database::Database;
use crate::commands::install::InstallCommand;
use crate::commands::sync::SyncCommand;
use crate::commands::add::AddLocalCommand;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReleasePayload {
    pub version: String,
    pub target_architecture: String,
    pub download_url: String,
    pub checksum: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateSource {
    pub source_type: String,
    pub url: Option<String>,
    pub local_path: Option<String>,
    pub package_name: Option<String>,
}

pub struct UpdateManager {
    current_version: String,
    binary_path: PathBuf,
    root: PathBuf,
    db: Arc<Database>,
}

impl UpdateManager {
    pub fn new(current_version: &str, root: PathBuf, db: Arc<Database>) -> Result<Self> {
        let binary_path = std::env::current_exe()?;
        Ok(Self {
            current_version: current_version.to_string(),
            binary_path,
            root,
            db,
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

    pub async fn update_from_source(&self, source: &UpdateSource) -> Result<()> {
        match source.source_type.as_str() {
            "repository" => {
                if let Some(url) = &source.url {
                    self.update_from_repository(url).await
                } else {
                    Err(anyhow!("Repository URL not provided"))
                }
            }
            "local" => {
                if let Some(path) = &source.local_path {
                    self.update_from_local(path).await
                } else {
                    Err(anyhow!("Local path not provided"))
                }
            }
            "delta" => {
                if let (Some(old_pkg), Some(delta_url), Some(output)) = 
                    (&source.package_name, &source.url, &source.local_path) {
                    self.update_from_delta(old_pkg, delta_url, output).await
                } else {
                    Err(anyhow!("Delta update requires package_name, url, and output"))
                }
            }
            _ => Err(anyhow!("Unsupported update source type: {}", source.source_type))
        }
    }

    async fn update_from_repository(&self, repo_url: &str) -> Result<()> {
        let sync_cmd = SyncCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
        let repo_info = crate::core::database::RepositoryInfo {
            name: "update-repo".to_string(),
            url: repo_url.to_string(),
            checksum: None,
        };
        let mgr = crate::core::repo::RepositoryManager::new(&self.root);
        mgr.add_repository(repo_info)?;
        sync_cmd.execute().await?;
        let install_cmd = InstallCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
        let installed = self.db.get_all_installed_packages()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.pkg_name)
            .collect::<Vec<_>>();
        install_cmd.execute(&installed).await?;
        Ok(())
    }

    async fn update_from_local(&self, package_path: &str) -> Result<()> {
        let add_cmd = AddLocalCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
        add_cmd.execute(package_path)?;
        Ok(())
    }

    async fn update_from_delta(&self, old_package: &str, delta_url: &str, output_path: &str) -> Result<()> {
        let temp_dir = std::env::temp_dir();
        let delta_file = temp_dir.join("update.xcd");
        let downloader = Downloader::new();
        downloader.download_package(delta_url, &delta_file).await?;
        let old_xcs = self.root.join("system/storage/packages")
            .join(format!("{}.xcs", old_package));
        if !old_xcs.exists() {
            return Err(anyhow!("Old package not found for delta update: {}", old_package));
        }
        let reconstructor = crate::core::delta::DeltaReconstructor::new(&self.root);
        reconstructor.reconstruct(&old_xcs, &delta_file, &PathBuf::from(output_path))?;
        let add_cmd = AddLocalCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
        add_cmd.execute(output_path)?;
        Ok(())
    }

    pub async fn upgrade_all(&self) -> Result<()> {
        let install_cmd = InstallCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
        let installed = self.db.get_all_installed_packages()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.pkg_name)
            .collect::<Vec<_>>();
        install_cmd.execute(&installed).await?;
        Ok(())
    }

    pub async fn sync_repositories(&self) -> Result<()> {
        let sync_cmd = SyncCommand::new(self.root.to_string_lossy().into_owned(), Arc::clone(&self.db));
        sync_cmd.execute().await?;
        Ok(())
    }

    pub fn get_current_version(&self) -> &str {
        &self.current_version
    }
}