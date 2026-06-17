use std::fs;
use std::path::Path;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use futures_util::future::join_all;
use crate::core::solver::DependencySolver;
use crate::core::database::Database;
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

        let mut solver = DependencySolver::new(Arc::clone(&self.db));
        for pkg in packages {
            solver = solver.add_target(pkg);
        }

        let plan = solver.solve()?;
        let cache_dir = Path::new(&self.root).join("var/cache/mcx");
        fs::create_dir_all(&cache_dir)?;

        let downloader = Downloader::new();
        let extractor = Extractor::new(&self.root);
        
        let mut download_tasks = Vec::new();
        let mut pending_installs = Vec::new();

        for meta in &plan {
            if self.db.is_package_installed(&meta.pkg_name)? {
                continue;
            }
            
            let archive_name = format!("{}-{}.xcs", meta.pkg_name, meta.version);
            let target_path = cache_dir.join(&archive_name);
            pending_installs.push((meta.clone(), target_path.clone()));

            if !target_path.exists() {
                let dl = downloader.clone();
                let url = meta.source.clone();
                let dest = target_path.clone();
                download_tasks.push(tokio::spawn(async move {
                    dl.download_package(&url, &dest).await
                }));
            }
        }

        for result in join_all(download_tasks).await {
            result??;
        }

        for (meta, path) in &pending_installs {
            HashVerifier::verify_integrity(path, &meta.checksum.kind)?;
        }

        let mut transaction = self.db.begin_transaction()?;
        let stage_dir = Path::new(&self.root).join("var/tmp/mcx/stage");

        for (meta, path) in pending_installs {
            if stage_dir.exists() { fs::remove_dir_all(&stage_dir)?; }
            fs::create_dir_all(&stage_dir)?;

            let extracted_files = extractor.extract_zstd_archive(&path, &stage_dir)?;
            extractor.verify_no_collisions(&extracted_files)?;

            for file in &extracted_files {
                let target = Path::new(&self.root).join(file);
                transaction.backup_file(&target)?;
                
                let src = stage_dir.join(file);
                if let Some(p) = target.parent() { fs::create_dir_all(p)?; }
                fs::copy(&src, &target)?;
                transaction.record_staged_file(file.clone())?;
            }

            transaction.register_package_placement(&meta)?;
        }

        transaction.commit()?;
        
        if stage_dir.exists() { fs::remove_dir_all(&stage_dir)?; }
        
        Ok(())
    }
}