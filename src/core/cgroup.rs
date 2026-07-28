use std::fs;
use std::path::PathBuf;
use anyhow::{Result, Context};
use crate::core::constants;

pub struct CgroupController {
    base_path: PathBuf,
}

impl CgroupController {
    pub fn new() -> Self {
        Self {
            base_path: PathBuf::from(constants::CGROUP_ROOT),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path)
            .context("Failed to allocate cgroup v2 base hierarchy")
    }

    pub fn enforce_resource_limits(&self, pkg_name: &str, max_memory_mb: u64, max_cpu_percent: u8) -> Result<()> {
        let cgroup_path = self.base_path.join(sanitise(pkg_name));
        fs::create_dir_all(&cgroup_path)
            .with_context(|| format!("Failed to create cgroup directory: {:?}", cgroup_path))?;

        let memory_bytes = (max_memory_mb as u128) * 1024 * 1024;
        let memory_max_path = cgroup_path.join("memory.max");
        fs::write(&memory_max_path, memory_bytes.to_string())
            .with_context(|| format!("Failed to write memory.max to {:?}", memory_max_path))?;

        let cpu_quota = (max_cpu_percent as u64) * constants::CGROUP_CPU_QUOTA_FACTOR;
        let cpu_max_path = cgroup_path.join("cpu.max");
        fs::write(&cpu_max_path, format!("{} {}", cpu_quota, constants::CGROUP_PERIOD_US))
            .with_context(|| format!("Failed to write cpu.max to {:?}", cpu_max_path))?;

        Ok(())
    }

    pub fn remove_resource_limits(&self, pkg_name: &str) -> Result<()> {
        let cgroup_path = self.base_path.join(sanitise(pkg_name));
        if cgroup_path.exists() {
            let children_path = cgroup_path.join("cgroup.kill");
            if children_path.exists() {
                let _ = fs::write(&children_path, "1");
            }
            fs::remove_dir(&cgroup_path)
                .with_context(|| format!("Failed to remove cgroup directory: {:?}", cgroup_path))?;
        }
        Ok(())
    }

    pub fn enforce_memory_limit(&self, pkg_name: &str, max_memory_mb: u64) -> Result<()> {
        let cgroup_path = self.base_path.join(sanitise(pkg_name));
        fs::create_dir_all(&cgroup_path)?;
        let memory_max_path = cgroup_path.join("memory.max");
        let memory_bytes = (max_memory_mb as u128) * 1024 * 1024;
        fs::write(&memory_max_path, memory_bytes.to_string())?;
        Ok(())
    }

    pub fn enforce_cpu_limit(&self, pkg_name: &str, max_cpu_percent: u8) -> Result<()> {
        let cgroup_path = self.base_path.join(sanitise(pkg_name));
        fs::create_dir_all(&cgroup_path)?;
        let cpu_max_path = cgroup_path.join("cpu.max");
        fs::write(&cpu_max_path, format!("{} {}", (max_cpu_percent as u64) * constants::CGROUP_CPU_QUOTA_FACTOR, constants::CGROUP_PERIOD_US))?;
        Ok(())
    }

    pub fn is_cgroup_v2_available(&self) -> bool {
        self.base_path.parent().map_or(false, |p| p.exists())
    }
}

fn sanitise(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
