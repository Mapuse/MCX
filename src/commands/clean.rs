use std::fs;
use std::path::PathBuf;
use anyhow::{Result, Context, anyhow};
use crate::utils::ui::UserInterface;

pub struct CleanCommand {
    cache_dir: PathBuf,
    history_dir: PathBuf,
}

impl CleanCommand {
    pub fn new(root: &str) -> Self {
        let base = PathBuf::from(root);
        Self {
            cache_dir: base.join("var/cache/mcx"),
            history_dir: base.join("var/log/mcx/history"),
        }
    }

    pub fn execute(&self, clean_cache: bool, clean_history: bool) -> Result<()> {
        if !clean_cache && !clean_history {
            return Err(anyhow!("No cleaning targets specified. Provide flags for cache or history."));
        }

        if clean_cache {
            if self.cache_dir.exists() {
                fs::remove_dir_all(&self.cache_dir)
                    .with_context(|| format!("Failed to completely clear binary cache footprint at: {:?}", self.cache_dir))?;
                fs::create_dir_all(&self.cache_dir)
                    .context("Failed to re-allocate clean binary cache staging structures")?;
                UserInterface::success("Successfully cleared package cache directory downloads.");
            }
        }

        if clean_history {
            if self.history_dir.exists() {
                fs::remove_dir_all(&self.history_dir)
                    .with_context(|| format!("Failed to clear mutable historic log references at: {:?}", self.history_dir))?;
                fs::create_dir_all(&self.history_dir)
                    .context("Failed to re-allocate pristine log timeline checkpoints")?;
                UserInterface::success("Successfully purged system transaction history state ledgers.");
            }
        }

        Ok(())
    }
}