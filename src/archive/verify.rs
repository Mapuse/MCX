use std::path::Path;
use anyhow::Result;
use crate::archive::hash::HashVerifier;

pub struct ContentValidator;

impl ContentValidator {
    pub fn verify_package_integrity<P: AsRef<Path>>(archive_path: P, kind: &str, expected_hash: &str) -> Result<()> {
        HashVerifier::verify_integrity(archive_path, kind, expected_hash)
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