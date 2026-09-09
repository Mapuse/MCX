use crate::core::constants;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct WorkspaceManager {
    pub root: PathBuf,
    pub build_dir: PathBuf,
    pub stage_dir: PathBuf,
}

impl WorkspaceManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        let root_path = root.as_ref().to_path_buf();
        Self {
            build_dir: root_path.join(constants::PATH_BUILD),
            stage_dir: root_path.join(constants::PATH_MCX_STAGE),
            root: root_path,
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.build_dir)
            .context("Failed to allocate transient build compilation runtime workspace")?;
        fs::create_dir_all(&self.stage_dir).context(
            "Failed to allocate isolated intermediate staging deployment matrix workspace",
        )?;
        Ok(())
    }

    pub fn create_package_build_space(&self, pkg_name: &str) -> Result<PathBuf> {
        let space = self.build_dir.join(pkg_name);
        if space.exists() {
            fs::remove_dir_all(&space).with_context(|| {
                format!(
                    "Failed to evict pre-existing stale compilation block: {:?}",
                    space
                )
            })?;
        }
        fs::create_dir_all(&space).with_context(|| {
            format!(
                "Failed to anchor temporal tracking workspace for package: {}",
                pkg_name
            )
        })?;
        Ok(space)
    }

    pub fn create_package_stage_space(&self, pkg_name: &str) -> Result<PathBuf> {
        let space = self.stage_dir.join(pkg_name);
        if space.exists() {
            fs::remove_dir_all(&space).with_context(|| {
                format!(
                    "Failed to evict pre-existing stale staging block: {:?}",
                    space
                )
            })?;
        }
        fs::create_dir_all(&space).with_context(|| {
            format!(
                "Failed to anchor temporal placement environment for package: {}",
                pkg_name
            )
        })?;
        Ok(space)
    }

    pub fn purge_package_workspaces(&self, pkg_name: &str) -> Result<()> {
        let build_space = self.build_dir.join(pkg_name);
        let stage_space = self.stage_dir.join(pkg_name);

        if build_space.exists() {
            fs::remove_dir_all(&build_space).with_context(|| {
                format!(
                    "Failed to clear workspace workspace allocation: {:?}",
                    build_space
                )
            })?;
        }
        if stage_space.exists() {
            fs::remove_dir_all(&stage_space).with_context(|| {
                format!(
                    "Failed to clear intermediate allocation frame: {:?}",
                    stage_space
                )
            })?;
        }
        Ok(())
    }

    pub fn clean_global_workspaces(&self) -> Result<()> {
        if self.build_dir.exists() {
            fs::remove_dir_all(&self.build_dir)
                .context("Failed to wipe transient structural workspace sector safely")?;
        }
        if self.stage_dir.exists() {
            fs::remove_dir_all(&self.stage_dir)
                .context("Failed to wipe intermediate staging framework sector safely")?;
        }
        self.initialize()
    }
}
