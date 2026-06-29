use std::fs;
use std::path::Path;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use futures::future::join_all;
use crate::core::solver::DependencySolver;
use crate::core::db::Database;
use crate::core::profiler::SystemProfile;
use crate::network::download::Downloader;
use crate::archive::extract::Extractor;
use crate::archive::hash::HashVerifier;

pub struct InstallCommand {
    root: String,
    db: Arc<Database>,
}

impl InstallCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self { root, db }
    }

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

        let downloader = Downloader::new();

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
                join_all(vec![tokio::spawn(async move {
                    dl.download_package(&url, &dest).await
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

        let mut transaction = self.db.begin_transaction()?;
        for (meta, _) in &pending {
            transaction.register_package_placement(meta)?;
        }
        transaction.commit()?;

        Ok(())
    }
}
