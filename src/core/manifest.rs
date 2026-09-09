use crate::core::package::PackageEntity;
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;

pub struct ManifestParser;

impl ManifestParser {
    pub fn parse_embedded_manifest<P: AsRef<Path>>(extracted_root: P) -> Result<PackageEntity> {
        let metadata_path = extracted_root.as_ref().join("metadata.json");
        if !metadata_path.exists() {
            return Err(anyhow!(
                "Package metadata specifications file missing from ous payload structure: {:?}",
                metadata_path
            ));
        }

        let content = fs::read_to_string(&metadata_path).with_context(|| {
            format!(
                "Failed to read structural package metadata file stream: {:?}",
                metadata_path
            )
        })?;

        let metadata: PackageEntity = serde_json::from_str(&content)
            .context("Package structural metadata conversion failed: JSON layout syntax error")?;

        Self::verify_schema_integrity(&metadata)?;
        Ok(metadata)
    }

    pub fn generate_manifest_blueprint(meta: &PackageEntity, output_path: &Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).context(
                "Failed to allocate parent directories for manifest serialization runtime",
            )?;
        }

        let serialized_payload = serde_json::to_string_pretty(meta)
            .context("Failed to serialize tracking metadata map state block")?;

        fs::write(output_path, serialized_payload).with_context(|| {
            format!(
                "Failed to commit system tracking entry metadata frame onto disk storage: {:?}",
                output_path
            )
        })?;

        Ok(())
    }

    fn verify_schema_integrity(meta: &PackageEntity) -> Result<()> {
        if meta.pkg_name.trim().is_empty() {
            return Err(anyhow!(
                "Structural defect inside package metadata: Component name token cannot be empty"
            ));
        }
        if meta.version.trim().is_empty() {
            return Err(anyhow!(
                "Structural defect inside package metadata: Constraint version sequence field empty"
            ));
        }
        if meta.license.trim().is_empty() {
            return Err(anyhow!(
                "Structural defect inside package metadata: Explicit license classification undefined"
            ));
        }
        if meta.checksum.trim().is_empty() {
            return Err(anyhow!(
                "Structural defect inside package metadata: Integrity checksum signature block empty"
            ));
        }
        Ok(())
    }
}
