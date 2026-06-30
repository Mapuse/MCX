use std::path::Path;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use crate::core::swarm::SwarmManager;
use crate::network::download::Downloader;

#[derive(Clone)]
pub struct DownloadPipeline {
    downloader: Downloader,
    swarm_mgr: Option<Arc<SwarmManager>>,
}

impl DownloadPipeline {
    pub fn new(swarm_mgr: Option<Arc<SwarmManager>>) -> Self {
        Self {
            downloader: Downloader::new(),
            swarm_mgr,
        }
    }

    pub async fn fetch(&self, url: &str, pkg_name: &str, version: &str, destination: &Path) -> Result<()> {
        // Stage 1: HTTPS via reqwest
        match self.downloader.download_package(url, destination).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                let msg = format!("HTTPS primary failed: {}", e);
                // Stage 2: P2P swarm fallback
                if let Some(ref swarm) = self.swarm_mgr {
                    if let Ok(Some(swarm_hash)) = swarm.get_swarm_hash(pkg_name) {
                        if let Ok(peers) = swarm.list_swarm_peers() {
                            for peer in &peers {
                                if peer.advertised_hashes.contains(&swarm_hash) {
                                    let peer_url = format!("http://{}/pkg/{}/{}/{}.xcs",
                                        peer.address, pkg_name, version, swarm_hash);
                                    if self.downloader.download_package(&peer_url, destination).await.is_ok() {
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
                // Stage 3: git clone fallback
                let dest_dir = destination.parent().unwrap_or(Path::new("/tmp"));
                let git_url = if url.starts_with("https://") || url.starts_with("http://") {
                    // Try to convert HTTPS download URL to git clone URL
                    // Strip filename from URL to get the repo URL
                    let mut repo_url = url.to_string();
                    if let Some(last_slash) = repo_url.rfind('/') {
                        repo_url.truncate(last_slash);
                        // Try removing /releases/download/... path
                        if repo_url.contains("/releases/download") {
                            if let Some(pos) = repo_url.rfind("/releases/download") {
                                repo_url.truncate(pos);
                            }
                        }
                    }
                    repo_url
                } else {
                    url.to_string()
                };

                let archive_name = format!("{}-{}.tar.gz", pkg_name, version);
                let tmp_dir = dest_dir.join(format!("git-{}-{}", pkg_name, version));
                let clone_result = std::process::Command::new("git")
                    .args(["clone", "--depth", "1", &git_url, &tmp_dir.to_string_lossy()])
                    .status();

                match clone_result {
                    Ok(status) if status.success() => {
                        let _ = std::process::Command::new("tar")
                            .args(["-czf", &tmp_dir.with_file_name(&archive_name).to_string_lossy(), "-C", &tmp_dir.to_string_lossy(), "."])
                            .status();
                        let _ = std::fs::rename(tmp_dir.with_file_name(&archive_name), destination);
                        let _ = std::fs::remove_dir_all(&tmp_dir);
                        return Ok(());
                    }
                    _ => {
                        let _ = std::fs::remove_dir_all(&tmp_dir);
                        return Err(anyhow!("All download stages exhausted: {}", msg));
                    }
                }
            }
        }
    }
}
