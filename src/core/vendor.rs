use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context, anyhow};
use crate::core::constants;
use crate::core::package::PackageEntity;
use crate::core::manifest::ManifestParser;

pub struct VendorManager {
    vendor_dir: PathBuf,
}

impl VendorManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            vendor_dir: root.as_ref().join(constants::PATH_VENDOR),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.vendor_dir)
            .context("Failed to allocate global vendor integration structural spaces")
    }

    pub fn register_vendor_package(&self, pkg_name: &str, source_payload: &Path) -> Result<()> {
        if !source_payload.exists() {
            return Err(anyhow!("Target vendor deployment source node missing from frame: {:?}", source_payload));
        }

        let target_destination = self.vendor_dir.join(format!("{}.xcs", pkg_name));
        
        fs::copy(source_payload, &target_destination)
            .with_context(|| format!("Failed to route vendor asset block into matrix workspace: {:?}", target_destination))?;

        Ok(())
    }

    pub fn extract_and_parse_vendor(&self, pkg_name: &str, staging_extraction_area: &Path) -> Result<PackageEntity> {
        let vendor_payload = self.vendor_dir.join(format!("{}.xcs", pkg_name));
        if !vendor_payload.exists() {
            return Err(anyhow!("Requested isolated vendor registration allocation not stored locally: {}", pkg_name));
        }

        fs::create_dir_all(staging_extraction_area)
            .context("Failed to anchor temporal workspace paths for vendor unpacking operation")?;

        let archive_file = fs::File::open(&vendor_payload)
            .with_context(|| format!("Failed to link connection channel onto vendor archive block: {:?}", vendor_payload))?;
        
        let zstd_decoder = zstd::stream::Decoder::new(archive_file)
            .context("Failed to initialize Zstd decoder for vendor package")?;
            
        let mut archive = tar::Archive::new(zstd_decoder);

        let entries = archive.entries()
            .with_context(|| format!("Decompression framework breakdown during vendor unpacking phase inside: {:?}", staging_extraction_area))?;

        for entry in entries {
            let mut entry = entry
                .with_context(|| format!("Failed to read tar entry during vendor unpacking: {:?}", staging_extraction_area))?;
            let path = entry.path()
                .with_context(|| "Failed to read tar entry path")?
                .into_owned();
            if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                anyhow::bail!("Path traversal detected in vendor archive: {:?}", path);
            }
            let dest = staging_extraction_area.join(&path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create parent dirs for vendor extraction: {:?}", parent))?;
            }
            entry.unpack(&dest)
                .with_context(|| format!("Failed to extract vendor entry {:?}", path))?;
        }

        let metadata = ManifestParser::parse_embedded_manifest(staging_extraction_area)
            .context("Structural vendor payload identification failure during embedded manifestation pass")?;

        Ok(metadata)
    }

    pub fn remove_vendor_package(&self, pkg_name: &str) -> Result<()> {
        let target_destination = self.vendor_dir.join(format!("{}.xcs", pkg_name));
        if target_destination.exists() {
            fs::remove_file(&target_destination)
                .with_context(|| format!("Failed to clear localized vendor block allocations from storage: {:?}", target_destination))?;
        }
        Ok(())
    }

    pub fn verify_vendor_presence(&self, pkg_name: &str) -> bool {
        self.vendor_dir.join(format!("{}.xcs", pkg_name)).exists()
    }
}