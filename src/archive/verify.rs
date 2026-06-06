use std::path::Path;
use anyhow::Result;
use crate::archive::hash::HashVerifier;

pub struct ContentValidator;

impl ContentValidator {
    pub fn verify_package_integrity<P: AsRef<Path>>(archive_path: P, expected_sha256: &str) -> Result<()> {
        HashVerifier::verify_integrity(archive_path, expected_sha256)
    }

    pub fn verify_extracted_manifest<P: AsRef<Path>>(manifest_path: P) -> Result<()> {
        if !manifest_path.as_ref().exists() {
            return Err(anyhow::anyhow!(
                "Manifest structural metadata missing from payload context: {:?}",
                manifest_path.as_ref()
            ));
        }
        Ok(())
    }
}