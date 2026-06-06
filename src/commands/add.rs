use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use crate::core::db::Database;
use crate::core::database::PackageMetadata;

#[derive()]
pub struct AddLocalCommand {
    db: Arc<Database>,
    root: PathBuf,
}

impl AddLocalCommand {
    pub fn execute(&self, file_path: &str) -> Result<()> {
        let package_path = Path::new(file_path);
        if !package_path.exists() {
            return Err(anyhow!("Target local package payload missing: {:?}", package_path));
        }

        let stage_dir = self.root.join("var/mcx/local");
        if stage_dir.exists() {
            fs::remove_dir_all(&stage_dir)?;
        }
        fs::create_dir_all(&stage_dir)?;

        let file = fs::File::open(package_path)?;
        let decoder = zstd::stream::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&stage_dir)?;

        let mut db_tx = self.db.begin_transaction()?;

        let stage_dir = &stage_dir; 
        let mut installed_files = Vec::new();

        let relative_files: Vec<PathBuf> = Vec::new();

        for rel_path in &relative_files {
            let src = stage_dir.join(rel_path);
            let dest = self.root.join(rel_path);
            
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            
            if src.is_file() {
                fs::copy(&src, &dest)?;
            }

            db_tx.record_staged_file(rel_path.clone())?;
            installed_files.push(rel_path.clone());
        }

        let db_metadata = PackageMetadata {
            pkg_name: "metadata.pkg_name".to_string(),
            version: "metadata.version".to_string(),
            source: "local".to_string(),
            license: "metadata.license".to_string(),
            files: installed_files, 
            dependencies: Vec::<crate::core::database::Dependency>::new(),
            checksum: crate::core::database::ChecksumData { kind: "sha256".to_string(), value: "hash".to_string() },
            provides: Some(Vec::<String>::new()),
            conflicts: Some(Vec::<String>::new()),
        };

        db_tx.register_package_placement(&db_metadata)?;
        db_tx.commit()?;

        if stage_dir.exists() {
            let _ = fs::remove_dir_all(&stage_dir);
        }

        Ok(())
    }
}