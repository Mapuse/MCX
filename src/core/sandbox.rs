use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, Context, bail};

use crate::core::metadata::RLineMetadata;








pub struct SandboxEnforcer {
    root: PathBuf,
}

impl SandboxEnforcer {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    
    
    
    
    pub fn execute_sandboxed(
        &self,
        capsule_path: &Path,
        meta: &RLineMetadata,
        command: &str,
        args: &[String],
    ) -> Result<SandboxedProcess> {
        let profile = &meta.profile;
        
        
        let isolate_network = !profile.contains(&"network".to_string());
        let isolate_display = !profile.contains(&"wayland".to_string());
        let isolate_rootfs = true; 

        
        let rootfs_dir = self.create_rootfs(capsule_path, &meta.prefix)?;

        
        let mut ns_flags = Vec::new();
        if isolate_network {
            ns_flags.push("CLONE_NEWNET");
        }
        if isolate_rootfs {
            ns_flags.push("CLONE_NEWNS");
        }

        
        let cgroup_path = self.create_cgroup(meta)?;

        
        self.execute_with_namespaces(
            &rootfs_dir,
            cgroup_path,
            ns_flags,
            isolate_display,
            command,
            args,
        )
    }

    
    fn create_rootfs(&self, capsule_path: &Path, prefix: &str) -> Result<PathBuf> {
        let rootfs_dir = self.root.join("var/tmp/mcx/rootfs")
            .join(format!("{}-{}", 
                capsule_path.file_stem().unwrap().to_str().unwrap(),
                prefix
            ));

        fs::create_dir_all(&rootfs_dir)?;

        
        let squashfs_mount = rootfs_dir.join("squashfs");
        fs::create_dir_all(&squashfs_mount)?;

        let status = Command::new("mount")
            .arg("-t")
            .arg("squashfs")
            .arg("-o")
            .arg("ro")
            .arg(capsule_path)
            .arg(&squashfs_mount)
            .status()
            .context("Failed to mount SquashFS for sandbox")?;

        if !status.success() {
            bail!("SquashFS mount failed for sandbox");
        }

        
        fs::create_dir_all(rootfs_dir.join("dev"))?;
        fs::create_dir_all(rootfs_dir.join("proc"))?;
        fs::create_dir_all(rootfs_dir.join("sys"))?;
        fs::create_dir_all(rootfs_dir.join("tmp"))?;
        fs::create_dir_all(rootfs_dir.join("var/tmp"))?;

        
        
        let prefix_path = squashfs_mount.join(prefix);
        if prefix_path.exists() {
            
            self.bind_mount_directory(&prefix_path, &rootfs_dir)?;
        }

        Ok(rootfs_dir)
    }

    
    fn bind_mount_directory(&self, src: &Path, dst: &Path) -> Result<()> {
        for entry in walkdir::WalkDir::new(src).min_depth(1) {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(src)?;
            let dest = dst.join(rel);

            if path.is_dir() {
                fs::create_dir_all(&dest)?;
                
                let _ = Command::new("mount")
                    .arg("--bind")
                    .arg(path)
                    .arg(&dest)
                    .status();
            } else if path.is_file() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(path, &dest)?;
            }
        }

        Ok(())
    }

    
    fn create_cgroup(&self, meta: &RLineMetadata) -> Result<PathBuf> {
        let cgroup_base = PathBuf::from("/sys/fs/cgroup/mcx");
        let cgroup_name = format!("{}-{}", meta.pkg_name, meta.version);
        let cgroup_path = cgroup_base.join(&cgroup_name);

        fs::create_dir_all(&cgroup_path)
            .context("Failed to create cgroup directory")?;

        
        let mem_limit = cgroup_path.join("memory.max");
        if mem_limit.exists() {
            fs::write(&mem_limit, "536870912")?; 
        }

        
        let cpu_max = cgroup_path.join("cpu.max");
        if cpu_max.exists() {
            fs::write(&cpu_max, "50000 100000")?; 
        }

        
        let pids_max = cgroup_path.join("pids.max");
        if pids_max.exists() {
            fs::write(&pids_max, "100")?; 
        }

        Ok(cgroup_path)
    }

    
    fn execute_with_namespaces(
        &self,
        rootfs: &Path,
        cgroup: PathBuf,
        ns_flags: Vec<&str>,
        isolate_display: bool,
        command: &str,
        args: &[String],
    ) -> Result<SandboxedProcess> {
        
        let mut cmd = Command::new("unshare");
        
        for flag in &ns_flags {
            cmd.arg(flag.to_lowercase());
        }
        
        cmd.arg("-f"); 
        cmd.arg("-r"); 

        
        cmd.env("HOME", "/");
        cmd.env("PATH", "/system/bin:/bin");
        
        
        if isolate_display {
            cmd.env("DISPLAY", "");
            cmd.env("WAYLAND_DISPLAY", "");
        }

        
        cmd.arg("--mount-proc");
        cmd.arg(rootfs);

        
        cmd.arg(command);
        cmd.args(args);

        
        let child = cmd.spawn()
            .context("Failed to spawn sandboxed process")?;

        Ok(SandboxedProcess {
            pid: child.id(),
            cgroup_path: cgroup,
            rootfs_dir: rootfs.to_path_buf(),
        })
    }

    
    pub fn cleanup_sandbox(&self, process: &SandboxedProcess) -> Result<()> {
        
        if process.cgroup_path.exists() {
            let _ = fs::remove_dir(&process.cgroup_path);
        }

        
        let _ = Command::new("umount")
            .arg("-l") 
            .arg(&process.rootfs_dir)
            .status();

        
        let _ = fs::remove_dir_all(&process.rootfs_dir);

        Ok(())
    }

    
    pub fn describe_profile(&self, meta: &RLineMetadata) -> String {
        let mut desc = String::new();
        let profile = &meta.profile;

        desc.push_str("Sandbox Profile for ");
        desc.push_str(&meta.pkg_name);
        desc.push_str(":\n");

        if profile.contains(&"isolated-rootfs".to_string()) {
            desc.push_str("  ✓ Root filesystem isolated\n");
        }

        if profile.contains(&"network".to_string()) {
            desc.push_str("  ✓ Network access allowed\n");
        } else {
            desc.push_str("  ✗ Network isolated (CLONE_NEWNET)\n");
        }

        if profile.contains(&"wayland".to_string()) {
            desc.push_str("  ✓ Display server access allowed\n");
        } else {
            desc.push_str("  ✗ Display isolated\n");
        }

        if profile.contains(&"audio".to_string()) {
            desc.push_str("  ✓ Audio access allowed\n");
        }

        desc
    }
}


pub struct SandboxedProcess {
    pub pid: u32,
    cgroup_path: PathBuf,
    rootfs_dir: PathBuf,
}

impl Drop for SandboxedProcess {
    fn drop(&mut self) {
        
        
        let _ = fs::remove_dir_all(&self.rootfs_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_sandbox_structure() {
        let temp = env::temp_dir().join("test_mcx_sandbox");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let enforcer = SandboxEnforcer::new(&temp);
        
        let meta = RLineMetadata {
            pkg_name: "test".to_string(),
            version: "1.0".to_string(),
            source: "".to_string(),
            license: "MIT".to_string(),
            build_type: "make".to_string(),
            build_date: "2024-01-01".to_string(),
            checksum: "abc123".to_string(),
            pkg_type: "plain".to_string(),
            components: None,
            services: None,
            profile: vec!["isolated-rootfs".to_string()],
            features: vec![],
            externals: None,
            bundled: None,
            depsig: None,
            prefix: "system".to_string(),
        };

        let desc = enforcer.describe_profile(&meta);
        assert!(desc.contains("Root filesystem isolated"));
        assert!(desc.contains("Network isolated"));

        let _ = fs::remove_dir_all(&temp);
    }
}