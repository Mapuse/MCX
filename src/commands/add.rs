use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use crate::core::database::Database;
use crate::core::database::PackageMetadata;

#[derive()]
pub struct AddLocalCommand {
    db: Arc<Database>,
    root: PathBuf,
}

impl AddLocalCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self {
            root: PathBuf::from(root),
            db,
        }
    }

    pub fn execute(&self, file_path: &str) -> Result<()> {
        let package_path = Path::new(file_path);
        if !package_path.exists() {
            return Err(anyhow!("Target local package payload missing: {:?}", package_path));
        }

        let stage_dir = self.root.join("var/tmp/mcx/stage");
        if stage_dir.exists() {
            fs::remove_dir_all(&stage_dir)?;
        }
        fs::create_dir_all(&stage_dir)?;

        
        
        let file = fs::File::open(package_path)?;
        let decoder = zstd::stream::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&stage_dir)?;

        
        let mut installed_files = Vec::new();
        Self::collect_relative_files(&stage_dir, &stage_dir, &mut installed_files)?;

        
        for rel_path in &installed_files {
            let src = stage_dir.join(rel_path);
            let dest = self.root.join(rel_path);
            
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            
            if src.is_file() {
                fs::copy(&src, &dest)?;
            }
        }

        
        let metadata_file = stage_dir.join("metadata.json");
        let (pkg_name, version, license, checksum_kind, checksum_value) = if metadata_file.exists() {
            let content = fs::read_to_string(&metadata_file)?;
            #[derive(serde::Deserialize)]
            struct EmbeddedMeta {
                #[serde(default)]
                pkg_name: String,
                #[serde(default)]
                version: String,
                #[serde(default)]
                license: String,
                #[serde(default)]
                checksum: String,
            }
            let emb: EmbeddedMeta = serde_json::from_str(&content)?;
            (emb.pkg_name, emb.version, emb.license, "sha256".to_string(), emb.checksum)
        } else {
            let name = package_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            (name, "0.0.0".to_string(), "Unknown".to_string(), "sha256".to_string(), "none".to_string())
        };

        let mut db_tx = self.db.begin_transaction()?;

        for rel_path in &installed_files {
            db_tx.record_staged_file(rel_path.clone())?;
        }

        let db_metadata = PackageMetadata {
            pkg_name,
            version,
            source: format!("local:{}", file_path),
            license,
            files: installed_files,
            dependencies: Vec::new(),
            checksum: crate::core::database::ChecksumData { kind: checksum_kind, value: checksum_value },
            provides: Some(Vec::new()),
            conflicts: Some(Vec::new()),
            features: Vec::new(),
        };

        db_tx.register_package_placement(&db_metadata)?;
        db_tx.commit()?;

        if stage_dir.exists() {
            let _ = fs::remove_dir_all(&stage_dir);
        }

        Ok(())
    }

    fn collect_relative_files(dir: &Path, base: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(base)
                .map_err(|_| anyhow!("Path strip error"))?
                .to_path_buf();
            if path.is_dir() {
                Self::collect_relative_files(&path, base, files)?;
            } else {
                files.push(rel);
            }
        }
        Ok(())
    }
}