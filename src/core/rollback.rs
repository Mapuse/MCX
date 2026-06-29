use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GenerationId(pub u64);

pub struct RollbackManager {
    active_dir: PathBuf,
    generations_dir: PathBuf,
}

impl RollbackManager {
    pub fn new(root: &Path) -> Self {
        Self {
            active_dir: root.join("var/lib/mcx/active"),
            generations_dir: root.join("var/lib/mcx/generations"),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.active_dir)
            .context("Failed to allocate active symlink directory")?;
        fs::create_dir_all(&self.generations_dir)
            .context("Failed to allocate generations snapshot directory")?;
        Ok(())
    }

    pub fn enable_atomic_rollback(&self, pkg_name: &str, source_dir: &Path) -> Result<GenerationId> {
        let pkg_dir = self.generations_dir.join(sanitise(pkg_name));
        fs::create_dir_all(&pkg_dir)?;

        let next_id = self.next_generation_id(pkg_name);
        let target = pkg_dir.join(next_id.to_string());

        if source_dir.exists() {
            Self::copy_hardlinks(source_dir, &target)
                .with_context(|| format!("Failed to create generation snapshot for {}", pkg_name))?;
        } else {
            fs::create_dir_all(&target)?;
        }

        let active_symlink = self.active_dir.join(sanitise(pkg_name));
        let relative_target = format!("../generations/{}/{}", sanitise(pkg_name), next_id);

        if active_symlink.exists() {
            fs::remove_file(&active_symlink)?;
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&relative_target, &active_symlink)
                .with_context(|| format!("Failed to create active symlink: {:?}", active_symlink))?;
        }

        Ok(GenerationId(next_id))
    }

    pub fn rollback_to_generation(&self, pkg_name: &str, generation: GenerationId) -> Result<()> {
        let pkg_dir = self.generations_dir.join(sanitise(pkg_name));
        let target = pkg_dir.join(generation.0.to_string());
        if !target.exists() {
            anyhow::bail!("Generation {} not found for package {}", generation.0, pkg_name);
        }

        let active_symlink = self.active_dir.join(sanitise(pkg_name));
        let relative_target = format!("../generations/{}/{}", sanitise(pkg_name), generation.0);

        if active_symlink.exists() {
            fs::remove_file(&active_symlink)?;
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&relative_target, &active_symlink)?;
        }

        Ok(())
    }

    pub fn list_generations(&self, pkg_name: &str) -> Result<Vec<GenerationId>> {
        let pkg_dir = self.generations_dir.join(sanitise(pkg_name));
        if !pkg_dir.exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        for entry in fs::read_dir(&pkg_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(id) = name.parse::<u64>() {
                        ids.push(GenerationId(id));
                    }
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    pub fn current_generation(&self, pkg_name: &str) -> Result<Option<GenerationId>> {
        let active_symlink = self.active_dir.join(sanitise(pkg_name));
        if !active_symlink.exists() {
            return Ok(None);
        }
        let target = fs::read_link(&active_symlink)?;
        if let Some(gen_str) = target.file_name().and_then(|n| n.to_str()) {
            if let Ok(id) = gen_str.parse::<u64>() {
                return Ok(Some(GenerationId(id)));
            }
        }
        Ok(None)
    }

    pub fn prune_generations(&self, pkg_name: &str, keep: usize) -> Result<usize> {
        let mut ids = self.list_generations(pkg_name)?;
        if ids.len() <= keep {
            return Ok(0);
        }

        let current = self.current_generation(pkg_name)?;
        ids.sort();

        let pkg_dir = self.generations_dir.join(sanitise(pkg_name));
        let mut removed = 0;

        for id in ids.iter().take(ids.len().saturating_sub(keep)) {
            if Some(*id) == current {
                continue;
            }
            let target = pkg_dir.join(id.0.to_string());
            if target.exists() {
                fs::remove_dir_all(&target)?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    fn next_generation_id(&self, pkg_name: &str) -> u64 {
        let pkg_dir = self.generations_dir.join(sanitise(pkg_name));
        if !pkg_dir.exists() {
            return 1;
        }
        let mut max_id = 0u64;
        if let Ok(entries) = fs::read_dir(&pkg_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if let Ok(id) = name.parse::<u64>() {
                            max_id = max_id.max(id);
                        }
                    }
                }
            }
        }
        max_id + 1
    }

    fn copy_hardlinks(src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(src)
                .map_err(|_| anyhow::anyhow!("Path strip error"))?;
            let dest = dst.join(rel);

            if path.is_dir() {
                Self::copy_hardlinks(&path, &dest)?;
            } else if path.is_file() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::hard_link(&path, &dest)
                    .or_else(|_| fs::copy(&path, &dest).map(|_| ()))
                    .with_context(|| format!("Failed to link file {:?} into generation", path))?;
            }
        }
        Ok(())
    }
}

fn sanitise(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
