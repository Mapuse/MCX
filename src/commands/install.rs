use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use futures::future::join_all;
use crate::core::solver::DependencySolver;
use crate::core::db::Database;
use crate::core::profiler::SystemProfile;
use crate::core::cgroup::CgroupController;
use crate::core::overlay::OverlayManager;
use crate::core::declarative::ProfileValidator;
use crate::core::security::SecurityMonitor;
use crate::network::pipeline::DownloadPipeline;
use crate::archive::extract::Extractor;
use crate::archive::hash::HashVerifier;
use crate::utils::ui::UserInterface;

pub struct InstallCommand {
    root: String,
    db: Arc<Database>,
    cgroup_mgr: CgroupController,
    overlay_mgr: OverlayManager,
    security_mon: Option<Arc<SecurityMonitor>>,
    profile_path: Option<PathBuf>,
}

impl InstallCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        let overlay_base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            root,
            db,
            cgroup_mgr: CgroupController::new(),
            overlay_mgr: OverlayManager::new(Path::new(&overlay_base)),
            security_mon: None,
            profile_path: None,
        }
    }

    pub fn with_cgroup(mut self, mgr: CgroupController) -> Self { self.cgroup_mgr = mgr; self }
    pub fn with_overlay(mut self, mgr: OverlayManager) -> Self { self.overlay_mgr = mgr; self }
    pub fn with_security(mut self, mon: Arc<SecurityMonitor>) -> Self { self.security_mon = Some(mon); self }
    pub fn with_profile(mut self, path: PathBuf) -> Self { self.profile_path = Some(path); self }

    pub async fn execute(&self, packages: &[String]) -> Result<()> {
        if packages.is_empty() {
            return Err(anyhow!("Target specification empty"));
        }

        let sys_profile = SystemProfile::probe();
        let mut solver = DependencySolver::new(Arc::clone(&self.db));
        for pkg in packages {
            solver = solver.add_target(pkg);
        }

        let plan = solver.solve()?;
        let root_path = Path::new(&self.root);
        let cache_dir = root_path.join("var/cache/mcx");
        fs::create_dir_all(&cache_dir)?;

        let pipeline = DownloadPipeline::new(None);
        let downloader = pipeline;

        let mut pending: Vec<(crate::core::database::PackageMetadata, std::path::PathBuf)> = Vec::new();

        for meta in &plan {
            if self.db.is_package_installed(&meta.pkg_name)? {
                let active_dir = root_path.join("var/lib/mcx/active").join(&meta.pkg_name);
                if active_dir.exists() {
                    continue;
                }
            }
            let archive_name = format!("{}-{}.xcs", meta.pkg_name, meta.version);
            let target_path = cache_dir.join(&archive_name);
            pending.push((meta.clone(), target_path.clone()));

            if !target_path.exists() {
                let dl = downloader.clone();
                let url = meta.source.clone();
                let dest = target_path.clone();
                let pkg = meta.pkg_name.clone();
                let ver = meta.version.clone();
                join_all(vec![tokio::spawn(async move {
                    dl.fetch(&url, &pkg, &ver, &dest).await
                })]).await;
            }
        }

        let use_parallel = sys_profile.cpu_count >= 4 && sys_profile.available_ram_mb >= 1024;

        if use_parallel {
            let mut handles = Vec::new();
            for (meta, path) in &pending {
                let meta_clone = meta.clone();
                let path_clone = path.clone();
                let root = self.root.clone();
                let ext = Extractor::new(&self.root);

                handles.push(tokio::task::spawn_blocking(move || -> Result<()> {
                    HashVerifier::verify_integrity(&path_clone, &meta_clone.checksum.kind)?;
                    let root_path = Path::new(&root);
                    let stage_dir = root_path.join("var/tmp/mcx/stage");
                    let installed_root = root_path.join("var/lib/mcx/active");

                    let pkg_stage = stage_dir.join(&meta_clone.pkg_name);
                    if pkg_stage.exists() { fs::remove_dir_all(&pkg_stage)?; }
                    fs::create_dir_all(&pkg_stage)?;

                    let extracted = ext.extract_zstd_archive(&path_clone, &stage_dir)?;
                    ext.verify_no_collisions(&extracted)?;

                    for file in &extracted {
                        let src = stage_dir.join(file);
                        let dest = pkg_stage.join(file);
                        if let Some(parent) = dest.parent() { fs::create_dir_all(parent)?; }
                        if src.is_file() { fs::copy(&src, &dest)?; }
                    }

                    let pkg_active = installed_root.join(&meta_clone.pkg_name);
                    if pkg_active.exists() { fs::remove_dir_all(&pkg_active)?; }
                    fs::create_dir_all(&pkg_active)?;

                    for file in &extracted {
                        let src = pkg_stage.join(file);
                        let dst = root_path.join(file);
                        if let Some(parent) = dst.parent() { fs::create_dir_all(parent)?; }
                        if src.is_file() {
                            std::io::copy(&mut fs::File::open(&src)?, &mut fs::File::create(&dst)?)?;
                        }
                        let dst_active = pkg_active.join(file);
                        if let Some(parent) = dst_active.parent() { fs::create_dir_all(parent)?; }
                        if src.is_file() {
                            std::io::copy(&mut fs::File::open(&src)?, &mut fs::File::create(&dst_active)?)?;
                        }
                    }

                    if stage_dir.exists() { let _ = fs::remove_dir_all(&stage_dir); }
                    Ok(())
                }));
            }

            for handle in handles {
                handle.await.map_err(|e| anyhow!("Task failed: {}", e))??;
            }
        } else {
            let ext = Extractor::new(&self.root);
            let stage_dir = root_path.join("var/tmp/mcx/stage");
            let installed_root = root_path.join("var/lib/mcx/active");

            for (meta, path) in &pending {
                HashVerifier::verify_integrity(path, &meta.checksum.kind)?;

                let pkg_stage = stage_dir.join(&meta.pkg_name);
                if pkg_stage.exists() { fs::remove_dir_all(&pkg_stage)?; }
                fs::create_dir_all(&pkg_stage)?;

                let extracted = ext.extract_zstd_archive(path, &stage_dir)?;
                ext.verify_no_collisions(&extracted)?;

                for file in &extracted {
                    let src = stage_dir.join(file);
                    let dest = pkg_stage.join(file);
                    if let Some(parent) = dest.parent() { fs::create_dir_all(parent)?; }
                    if src.is_file() { fs::copy(&src, &dest)?; }
                }

                let pkg_active = installed_root.join(&meta.pkg_name);
                if pkg_active.exists() { fs::remove_dir_all(&pkg_active)?; }
                fs::create_dir_all(&pkg_active)?;

                for file in &extracted {
                    let src = pkg_stage.join(file);
                    let dst = root_path.join(file);
                    if let Some(parent) = dst.parent() { fs::create_dir_all(parent)?; }
                    if src.is_file() {
                        std::io::copy(&mut fs::File::open(&src)?, &mut fs::File::create(&dst)?)?;
                    }
                    let dst_active = pkg_active.join(file);
                    if let Some(parent) = dst_active.parent() { fs::create_dir_all(parent)?; }
                    if src.is_file() {
                        std::io::copy(&mut fs::File::open(&src)?, &mut fs::File::create(&dst_active)?)?;
                    }
                }

                if stage_dir.exists() { let _ = fs::remove_dir_all(&stage_dir); }
            }
        }

        // sandbox setup for each installed package
        for (meta, _) in &pending {
            // cgroup: enforce resource limits (best-effort, may fail without root)
            let _ = self.cgroup_mgr.enforce_resource_limits(&meta.pkg_name, 512, 80);

            // overlay: create sandbox directory structure and mount script
            if let Ok(merged) = self.overlay_mgr.create_isolated_overlay(
                &meta.pkg_name,
                root_path,
            ) {
                UserInterface::sandbox(&format!("Overlay sandbox for {} at {}", meta.pkg_name, merged.display()));
                UserInterface::sandbox(&format!("Activate: sudo {}", self.overlay_mgr.overlays_base
                    .join(sanitize(&meta.pkg_name))
                    .join("mount-overlay.sh").display()));
            }

            // security monitor: register package
            if let Some(ref mon) = self.security_mon {
                mon.register_package(&meta.pkg_name);
            }
        }

        // profile validation: detect drift between declared and actual state
        if let Some(ref profile_path) = self.profile_path {
            if profile_path.exists() {
                match ProfileValidator::load_profile(profile_path) {
                    Ok(profile) => {
                        let current: Vec<String> = self.db.get_all_installed_packages()
                            .unwrap_or_default()
                            .iter()
                            .map(|p| p.pkg_name.clone())
                            .collect();
                        let (to_install, to_remove) = ProfileValidator::compile_profile_diff(&current, &profile.packages);
                        if !to_install.is_empty() || !to_remove.is_empty() {
                            UserInterface::profile(&format!("Drift: {} to install, {} to remove",
                                to_install.len(), to_remove.len()));
                        }
                    }
                    Err(e) => {
                        UserInterface::warning(&format!("Profile validation skipped: {}", e));
                    }
                }
            }
        }

        let mut transaction = self.db.begin_transaction()?;
        for (meta, _) in &pending {
            transaction.register_package_placement(meta)?;
        }
        transaction.commit()?;

        Ok(())
    }
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
