use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};

pub struct SelfUpdateManager;

impl SelfUpdateManager {
    pub fn new() -> Self {
        Self
    }

    pub fn build_from_source(repo_url: &str, target: &str, output_path: &Path) -> Result<PathBuf> {
        let tmp = std::env::temp_dir().join("mcx-self-update-src");
        let _ = std::fs::remove_dir_all(&tmp);

        let clone = Command::new("git")
            .args(["clone", repo_url, &tmp.to_string_lossy()])
            .status()
            .map_err(|e| anyhow!("git execution failed: {}", e))?;
        if !clone.success() {
            return Err(anyhow!("git clone exited with status {}", clone));
        }

        let build = Command::new("cargo")
            .args(["build", "--release", "--target", target])
            .current_dir(&tmp)
            .status()
            .map_err(|e| anyhow!("cargo execution failed: {}", e))?;
        if !build.success() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(anyhow!("cargo build exited with status {}", build));
        }

        let built = tmp.join(format!("target/{}/release/mcx", target));
        if !built.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(anyhow!("Built binary not found at target/{}/release/mcx", target));
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&built, output_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(output_path, std::fs::Permissions::from_mode(0o755))?;
        }

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(output_path.to_path_buf())
    }
}
