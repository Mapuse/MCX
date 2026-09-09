use crate::archive::hash::HashVerifier;
use crate::network::download::Downloader;
use anyhow::{Context, Result, anyhow};
use rand::Rng;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct SelfUpdateManager;

impl Default for SelfUpdateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfUpdateManager {
    pub fn new() -> Self {
        Self
    }

    fn unique_stage_path(stage_dir: &Path) -> PathBuf {
        stage_dir.join(format!(
            "mcx.new.{}",
            rand::rng().random_range(1_000_000_000_000u64..u64::MAX)
        ))
    }

    /// Downloads the self-update payload into a private staging directory and
    /// verifies it before it is ever executed or promoted.
    ///
    /// Safety protocol:
    /// 1. The checksum is fetched from `<url>.sha256` over the same origin;
    ///    a repository that publishes no checksum is refused outright —
    ///    unverified binaries are never executed.
    /// 2. Staging happens in a root-owned `var/tmp/mcx` directory with mode
    ///    0700 and a unique random file name, so other local users can neither
    ///    plant nor swap the payload while it is being written.
    /// 3. The staged bytes are SHA-256-verified against the published digest
    ///    before the executable bit is set.
    /// 4. Only then is `--version` probed; failure discards the payload.
    ///
    /// Returns the path of the verified staged binary for promotion via
    /// [`SelfUpdateManager::promote`].
    pub async fn stage_verified_binary(binary_url: &str, stage_dir: &Path) -> Result<PathBuf> {
        fs::create_dir_all(stage_dir)
            .with_context(|| format!("Failed to create staging directory {:?}", stage_dir))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(stage_dir, fs::Permissions::from_mode(0o700))
                .with_context(|| format!("Failed to restrict staging directory {:?}", stage_dir))?;
        }

        let downloader = Downloader::new();

        // Phase 1: obtain the published digest for this exact build.
        let checksum_url = format!("{}.sha256", binary_url);
        let checksum_tmp = Self::unique_stage_path(stage_dir);
        let _ = fs::remove_file(&checksum_tmp);
        downloader.package(&checksum_url, &checksum_tmp).await.with_context(|| {
            format!("No published checksum available at {}; refusing to install an unverified binary", checksum_url)
        })?;
        let expected = fs::read_to_string(&checksum_tmp)
            .context("Published checksum file unreadable")?
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let _ = fs::remove_file(&checksum_tmp);
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(anyhow!("Published checksum file is malformed"));
        }

        // Phase 2: download into the unique staging slot.
        let staged_path = Self::unique_stage_path(stage_dir);
        downloader
            .package(binary_url, &staged_path)
            .await
            .with_context(|| format!("Failed to download {}", binary_url))?;

        // Phase 3: verify content before it becomes executable.
        if let Err(e) = HashVerifier::verify_integrity(&staged_path, "sha256", &expected) {
            let _ = fs::remove_file(&staged_path);
            return Err(e.context("Checksum mismatch on downloaded self-update binary"));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staged_path, fs::Permissions::from_mode(0o755))?;
        }

        // Phase 4: sanity probe of the verified payload.
        let probe = Command::new(&staged_path)
            .arg("--version")
            .output()
            .map_err(|e| {
                let _ = fs::remove_file(&staged_path);
                anyhow!("Downloaded binary failed to execute: {}", e)
            })?;
        if !probe.status.success() {
            let _ = fs::remove_file(&staged_path);
            return Err(anyhow!("Downloaded binary failed its --version check"));
        }

        Ok(staged_path)
    }

    /// Promotes a verified staged binary onto `target` through a unique,
    /// same-filesystem temporary name and an atomic rename.
    pub fn promote(staged: &Path, target: &Path) -> Result<()> {
        let parent = target
            .parent()
            .ok_or_else(|| anyhow!("Target {:?} has no parent directory", target))?;
        fs::create_dir_all(parent)?;

        let name = target
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let new_path = parent.join(format!(
            ".{}.mcx-new-{}",
            name,
            rand::rng().random_range(1_000_000_000_000u64..u64::MAX)
        ));

        fs::copy(staged, &new_path)
            .with_context(|| format!("Failed to stage {:?} at {:?}", staged, new_path))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&new_path, fs::Permissions::from_mode(0o755))?;
        }
        if let Err(e) = fs::rename(&new_path, target) {
            let _ = fs::remove_file(&new_path);
            return Err(anyhow!("Atomic rename failed: {}", e));
        }
        let _ = fs::remove_file(staged);
        Ok(())
    }
}
