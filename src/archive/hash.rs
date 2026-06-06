use std::fs::File;
use std::io::Read;
use std::path::Path;
use sha2::{Sha256, Digest};
use anyhow::{Result, anyhow};

pub struct HashVerifier;

impl HashVerifier {
    pub fn calculate_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
        let mut file = File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 8 * 1024 * 1024];

        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }

        Ok(format!("{:064x}", hasher.finalize()))
    }

    pub fn verify_integrity<P: AsRef<Path>>(path: P, expected_hash: &str) -> Result<()> {
        let actual_hash = Self::calculate_sha256(&path)?;
        
        if actual_hash.eq_ignore_ascii_case(expected_hash) {
            Ok(())
        } else {
            Err(anyhow!(
                "Integrity violation detected on block {:?}. Expected: {}, Computed: {}",
                path.as_ref(),
                expected_hash,
                actual_hash
            ))
        }
    }
}