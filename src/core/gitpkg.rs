use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow, Context};
use serde::{Serialize, Deserialize};
use crate::core::constants;
use crate::utils::ui::UserInterface;

/// Per-package git tracking state persisted outside the LMDB database so the
/// core registry schema is left untouched (git stays a layer on top). It
/// records the package's git remote for repo-side bookkeeping (registry
/// sidecar). Package installation itself reads the `.xcs` archive from
/// `source` and applies the diff of the new archive — the git repository is
/// never consulted for installs.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GitPackageState {
    /// The archive URL the package was installed from (`meta.source`).
    pub source: String,
    /// The package's own git repository (from the registry sidecar); used for
    /// repo-side publishing bookkeeping, not for installs.
    pub git_source: String,
    /// The version this state refers to.
    pub version: String,
    /// Commit of the tag `v<version>` when the publisher recorded one; may be
    /// left empty when the repository was never inspected.
    pub commit: String,
    /// Relative paths of every file in the payload as of this state.
    pub files: Vec<String>,
}

/// Describes the difference between the installed payload and the new one,
/// so an update only touches the files that actually changed.
#[derive(Debug, Clone, Default)]
pub struct PackageDiff {
    /// Files added or modified in the new tree (relative paths).
    pub changed: Vec<PathBuf>,
    /// Files present in the old tree but gone in the new one (relative paths).
    pub removed: Vec<PathBuf>,
}

impl PackageDiff {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Repository-side git integration: a per-package registry sidecar
/// (`var/lib/mcx/gitstate/registry.json`) mapping package names to their git
/// remotes, with clones/worktrees for inspecting a published tree and a `purge`
/// that removes that state. Package installation does not consult this — every
/// install reads the `.xcs` archive from its source URL and applies only the
/// diff. Publishing lives in the `ous` (Outsider) package builder instead.
pub struct GitPackageManager {
    store_dir: PathBuf,
    state_dir: PathBuf,
}

/// Parses a package source reference. The marker is retained for
/// compatibility; installs always ship from the `.xcs` archive URL.
pub fn is_git_source(source: &str) -> bool {
    source.starts_with("git+") || source.starts_with("git://")
}

/// Strips the `git+` marker to recover the raw remote URL passed to git.
pub fn git_remote_url(source: &str) -> &str {
    source.strip_prefix("git+").unwrap_or(source)
}

/// Maps a package name onto a safe directory name for its git checkout.
fn sanitize_name(name: &str) -> String {
    let mut cleaned = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            cleaned.push(ch);
        } else {
            cleaned.push('_');
        }
    }
    if cleaned.is_empty() {
        "pkg".to_string()
    } else {
        cleaned
    }
}

impl GitPackageManager {
    pub fn new(root: &Path) -> Self {
        Self {
            store_dir: root.join(constants::PATH_GITS),
            state_dir: root.join(constants::PATH_GITSTATE),
        }
    }

    pub fn store_dir(&self) -> &Path {
        &self.store_dir
    }

    pub fn checkout_dir(&self, pkg_name: &str) -> PathBuf {
        self.store_dir.join(sanitize_name(pkg_name))
    }

    /// Detached worktrees used to materialise an arbitrary commit without
    /// disturbing the shared checkout.
    pub fn worktree_dir(&self, pkg_name: &str) -> PathBuf {
        self.store_dir.join(format!("{}.wt", sanitize_name(pkg_name)))
    }

    fn state_file(&self, pkg_name: &str) -> PathBuf {
        self.state_dir.join(format!("{}.json", sanitize_name(pkg_name)))
    }

    fn registry_file(&self) -> PathBuf {
        self.state_dir.join("registry.json")
    }

    // ── Git registry ────────────────────────────────────────────────────────

    /// The git registry maps package names to the repository used as their
    /// update/diff source. It is a JSON sidecar (`var/lib/mcx/gitstate/`), so
    /// the LMDB schema for `PackageMetadata` is never extended.
    pub fn read_registry(&self) -> Result<HashMap<String, String>> {
        let path = self.registry_file();
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&path)?;
        let map: HashMap<String, String> = serde_json::from_str(&content)
            .with_context(|| format!("Corrupt git registry {:?}", path))?;
        Ok(map)
    }

    pub fn write_registry(&self, registry: &HashMap<String, String>) -> Result<()> {
        fs::create_dir_all(&self.state_dir)?;
        let content = serde_json::to_string_pretty(registry)?;
        fs::write(self.registry_file(), content)?;
        Ok(())
    }

    pub fn git_source(&self, pkg_name: &str) -> Option<String> {
        self.read_registry().ok().and_then(|r| r.get(pkg_name).cloned())
    }

    /// Refresh the git registry from every cached repository index file. An
    /// optional `git_source` key on each package object is picked up here
    /// (the key is ignored by the regular `PackageMetadata` deserializer).
    pub fn sync_registry_from_indexes(&self, sync_dir: &Path) -> Result<()> {
        let mut registry = self.read_registry()?;
        let mut touched = false;
        if !sync_dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(sync_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().map(|e| e != "json").unwrap_or(true) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else { continue };
            let entries: Vec<IndexEntry> = match serde_json::from_str(&content) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for e in entries {
                if let Some(source) = e.git_source
                    && !source.trim().is_empty()
                {
                    registry.insert(e.pkg_name, source);
                    touched = true;
                }
            }
        }
        if touched {
            self.write_registry(&registry)?;
        }
        Ok(())
    }

    // ── Per-package state ───────────────────────────────────────────────────

    pub fn read_state(&self, pkg_name: &str) -> Result<Option<GitPackageState>> {
        let path = self.state_file(pkg_name);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let state: GitPackageState = serde_json::from_str(&content)
            .with_context(|| format!("Corrupt git state file {:?}", path))?;
        Ok(Some(state))
    }

    pub fn write_state(&self, pkg_name: &str, state: &GitPackageState) -> Result<()> {
        fs::create_dir_all(&self.state_dir)?;
        let content = serde_json::to_string_pretty(state)?;
        fs::write(self.state_file(pkg_name), content)?;
        Ok(())
    }

    // ── Repository operations ───────────────────────────────────────────────

    /// Ensure a local clone of the package repository exists. This is the one
    /// and only transfer that pulls history; every later update only fetches
    /// the new objects (`git fetch`), which is where the "diff" comes from.
    pub fn ensure_clone(&self, pkg_name: &str, url: &str) -> Result<()> {
        let checkout = self.checkout_dir(pkg_name);
        fs::create_dir_all(&self.store_dir)?;

        if checkout.join(".git").exists() && checkout.join(".git").is_dir() {
            return Ok(());
        }
        if checkout.exists() {
            fs::remove_dir_all(&checkout)?;
        }

        UserInterface::download(&format!("Fetching {} ({})", pkg_name, url));
        run_git_with_git_dir(
            &self.store_dir,
            &["clone", "--no-checkout", "--", url, checkout.to_str().context("checkout path not UTF-8")?],
        )
        .map_err(|e| anyhow!("Failed to clone {} from {}: {}", pkg_name, url, e))?;
        Ok(())
    }

    /// Pull new objects (exactly the diff delta) into the local clone.
    pub fn fetch(&self, pkg_name: &str) -> Result<()> {
        let checkout = self.checkout_dir(pkg_name);
        if !checkout.join(".git").exists() {
            return Ok(());
        }
        run_git(&checkout, &["fetch", "origin", "--prune", "--tags"])
            .map_err(|e| anyhow!("git fetch failed for {}: {}", pkg_name, e))?;
        Ok(())
    }

    /// Find the commit a released version maps to: the tag `v<version>`, then
    /// `<version>`. Panics-free: an unknown version yields `None`.
    pub fn resolve_commit(&self, pkg_name: &str, version: &str) -> Result<Option<String>> {
        let checkout = self.checkout_dir(pkg_name);
        if !checkout.join(".git").exists() {
            return Ok(None);
        }
        for candidate in [
            format!("refs/tags/v{}", version),
            format!("refs/tags/{}", version),
        ] {
            if let Ok(out) = run_git(&checkout, &["rev-parse", "--verify", &candidate]) {
                return Ok(Some(out.trim().to_string()));
            }
        }
        Ok(None)
    }

    /// The remote default branch's commit, used when the new version has no
    /// release tag yet.
    pub fn default_branch_commit(&self, pkg_name: &str) -> Result<Option<String>> {
        let checkout = self.checkout_dir(pkg_name);
        if !checkout.join(".git").exists() {
            return Ok(None);
        }
        for candidate in [
            "refs/remotes/origin/main",
            "refs/remotes/origin/master",
            "refs/remotes/origin/HEAD",
        ] {
            if let Ok(out) = run_git(&checkout, &["rev-parse", "--verify", candidate]) {
                return Ok(Some(out.trim().to_string()));
            }
        }
        Ok(None)
    }

    /// Enumerate every tracked file at a given revision (relative paths).
    pub fn tree_files(&self, pkg_name: &str, commit: &str) -> Result<Vec<String>> {
        let checkout = self.checkout_dir(pkg_name);
        if !checkout.join(".git").exists() {
            return Err(anyhow!("Package '{}' has no local git checkout", pkg_name));
        }
        let out = run_git(&checkout, &["ls-tree", "-r", "--name-only", "-z", commit])?;
        Ok(split_nul(&out))
    }

    /// Compute the set of changed/added and removed paths when moving a
    /// package from `base` to `target` in its repository.
    pub fn diff_between(&self, pkg_name: &str, base: &str, target: &str) -> Result<PackageDiff> {
        let checkout = self.checkout_dir(pkg_name);
        if !checkout.join(".git").exists() {
            return Err(anyhow!("Package '{}' has no local git checkout", pkg_name));
        }

        let mut changed = Vec::new();
        let mut removed = Vec::new();

        // --no-renames keeps the status simple: files are A/M/D, one path per
        // line, space-separated.
        let status = run_git(&checkout, &["diff", "--name-status", "--no-renames", base, target])?;
        for line in status.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // `--name-status` separates status and path with a TAB.
            let parts = line.split_once('\t')
                .or_else(|| line.split_once(' '));
            let Some((status_letter, file)) = parts else { continue };
            validate_relative_path(pkg_name, file)?;
            let first = status_letter.chars().next().unwrap_or('M');
            if first == 'D' {
                removed.push(PathBuf::from(file));
            } else {
                changed.push(PathBuf::from(file));
            }
        }

        // Any file tracked at base but absent from target is also considered
        // removed, guarding against status output variants.
        let base_files = self.tree_files(pkg_name, base)?;
        let target_files = self.tree_files(pkg_name, target)?;
        for f in &base_files {
            let path = PathBuf::from(f);
            if !target_files.contains(f) && !removed.contains(&path) {
                removed.push(path);
            }
        }

        changed.sort();
        changed.dedup();
        removed.sort();
        removed.dedup();
        // A path cannot be both added and removed.
        removed.retain(|r| !changed.contains(r));

        Ok(PackageDiff { changed, removed })
    }

    /// Materialise a detached worktree of `commit` for reading files.
    pub fn create_worktree(&self, pkg_name: &str, commit: &str) -> Result<PathBuf> {
        let checkout = self.checkout_dir(pkg_name);
        let wt = self.worktree_dir(pkg_name);
        if !checkout.join(".git").exists() {
            return Err(anyhow!("Package '{}' has no local git checkout", pkg_name));
        }
        if wt.exists() {
            fs::remove_dir_all(&wt)?;
        }
        if let Some(parent) = wt.parent() {
            fs::create_dir_all(parent)?;
        }
        run_git(
            &checkout,
            &["worktree", "add", "--detach", "--force", wt.to_str().context("worktree path not UTF-8")?, commit],
        )
        .map_err(|e| anyhow!("Failed to materialise {}@{}: {}", pkg_name, commit, e))?;
        Ok(wt)
    }

    pub fn remove_worktree(&self, pkg_name: &str) {
        let wt = self.worktree_dir(pkg_name);
        let _ = fs::remove_dir_all(&wt);
        let checkout = self.checkout_dir(pkg_name);
        if checkout.join(".git").exists() {
            let _ = run_git(&checkout, &["worktree", "prune"]);
        }
    }

    /// Remove the local git checkout, worktree and state for a package.
    pub fn purge(&self, pkg_name: &str) -> Result<()> {
        let checkout = self.checkout_dir(pkg_name);
        if checkout.exists() {
            fs::remove_dir_all(&checkout)?;
        }
        let wt = self.worktree_dir(pkg_name);
        if wt.exists() {
            fs::remove_dir_all(&wt)?;
        }
        let state = self.state_file(pkg_name);
        if state.exists() {
            fs::remove_file(&state)?;
        }
        Ok(())
    }
}

/// Optional field published inside repository index JSON objects. It is
/// ignored by the regular `PackageMetadata` deserializer and harvested into
/// the git registry sidecar by `sync_registry_from_indexes`.
#[derive(Deserialize)]
struct IndexEntry {
    #[serde(default)]
    pkg_name: String,
    #[serde(default)]
    git_source: Option<String>,
}

/// Reject any payload path that is absolute or that escapes via `..`.
fn validate_relative_path(pkg_name: &str, file: &str) -> Result<()> {
    let path = Path::new(file);
    if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(anyhow!("Unsafe git diff path in {}: {:?}", pkg_name, path));
    }
    Ok(())
}

fn run_git_in(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let output = std::process::Command::new(constants::TOOL_GIT)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| anyhow!("Failed to execute git: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    Ok(output)
}

fn run_git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = run_git_in(dir, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_git_with_git_dir(work_root: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(constants::TOOL_GIT)
        .args(args)
        .current_dir(work_root)
        .output()
        .map_err(|e| anyhow!("Failed to execute git: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn split_nul(s: &str) -> Vec<String> {
    s.split('\0').filter(|p| !p.is_empty()).map(|p| p.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_source_detection() {
        assert!(is_git_source("git+https://example.com/pkg"));
        assert!(is_git_source("git+ssh://git@example.com/pkg"));
        assert!(is_git_source("git+file:///srv/pkg"));
        assert!(is_git_source("git://example.com/pkg"));
        assert!(!is_git_source("https://example.com/pkg"));
        assert!(!is_git_source("pool/x86_64/pkg-1.0.xcs"));
        assert_eq!(git_remote_url("git+https://example.com/pkg"), "https://example.com/pkg");
        assert_eq!(git_remote_url("https://x"), "https://x");
    }

    #[test]
    fn validate_relative_path_rejects_traversal() {
        assert!(validate_relative_path("pkg", "usr/bin/hello").is_ok());
        assert!(validate_relative_path("pkg", "a/./b.txt").is_ok());
        assert!(validate_relative_path("pkg", "/etc/passwd").is_err());
        assert!(validate_relative_path("pkg", "../etc/passwd").is_err());
    }

    #[test]
    fn registry_index_harvesting() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state_dir = root.join(constants::PATH_GITSTATE);
        let sync_dir = root.join("sync");
        fs::create_dir_all(&sync_dir).unwrap();
        fs::write(
            sync_dir.join("main-base.json"),
            r#"[
                {"pkg_name":"nginx","version":"1.0.0","source":"https://x/nginx-1.0.0.xcs","checksum":{"kind":"sha256","value":"0"},"license":"MIT","architecture":"native","git_source":"https://x/nginx.git"},
                {"pkg_name":"foo","version":"1.0.0","source":"https://x/foo-1.0.0.xcs","checksum":{"kind":"sha256","value":"0"},"license":"MIT","architecture":"native"},
                {"pkg_name":"side","version":"2.0.0","source":"https://x/side-2.0.0.xcs","checksum":{"kind":"sha256","value":"0"},"license":"MIT","architecture":"native","git_source":"https://x/side.git"}
            ]"#,
        ).unwrap();

        let mgr = GitPackageManager::new(root);
        mgr.sync_registry_from_indexes(&sync_dir).unwrap();

        let registry = mgr.read_registry().unwrap();
        assert_eq!(registry.get("nginx").map(String::as_str), Some("https://x/nginx.git"));
        assert_eq!(registry.get("side").map(String::as_str), Some("https://x/side.git"));
        assert!(!registry.contains_key("foo"), "entries without git_source are skipped");
        assert!(state_dir.join("registry.json").is_file());
    }
}