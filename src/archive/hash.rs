use crate::core::constants;
use anyhow::{Result, anyhow};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub struct HashVerifier;

impl HashVerifier {
    /// SHA-256 is the preferred digest kind for new metadata.
    pub fn calculate<P: AsRef<Path>>(path: P, kind: &str) -> Result<String> {
        let mut file = File::open(&path)?;
        let mut buffer = vec![0; constants::HASH_BUFFER_SIZE];

        match kind {
            "sha256" | "sha-256" => {
                let mut hasher = Sha256::new();
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    hasher.update(&buffer[..count]);
                }
                Ok(format!("{:064x}", hasher.finalize()))
            }
            "sha1" | "sha-1" => {
                let mut hasher = Sha1::new();
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    hasher.update(&buffer[..count]);
                }
                Ok(format!("{:040x}", hasher.finalize()))
            }
            "md5" => {
                let mut hasher = Md5::new();
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    hasher.update(&buffer[..count]);
                }
                Ok(format!("{:032x}", hasher.finalize()))
            }
            other => Err(anyhow!(
                "Unsupported checksum kind: '{}' (supported: sha256, sha1, md5)",
                other
            )),
        }
    }

    /// Length-independent, constant-time digest comparison to avoid leaking
    /// match prefixes through timing.
    fn constant_time_eq(a: &str, b: &str) -> bool {
        let (a_bytes, b_bytes) = (a.as_bytes(), b.as_bytes());
        let mut diff = (a_bytes.len() ^ b_bytes.len()) as u8;
        let len = a_bytes.len().max(b_bytes.len());
        for i in 0..len {
            diff |= a_bytes.get(i).unwrap_or(&0) ^ b_bytes.get(i).unwrap_or(&0);
        }
        diff == 0
    }

    pub fn verify_integrity<P: AsRef<Path>>(
        path: P,
        kind: &str,
        expected_hash: &str,
    ) -> Result<()> {
        let actual_hash = Self::calculate(&path, kind)?;

        if Self::constant_time_eq(
            &actual_hash.to_lowercase(),
            &expected_hash.trim().to_lowercase(),
        ) {
            Ok(())
        } else {
            Err(anyhow!(
                "Integrity violation on {:?}. Kind: {}, Expected: {}, Computed: {}",
                path.as_ref(),
                kind,
                expected_hash,
                actual_hash
            ))
        }
    }
}

pub fn hash_bytes(data: &[u8], kind: &str) -> Result<String> {
    match kind {
        "sha256" | "sha-256" => Ok(format!("{:064x}", Sha256::digest(data))),
        "sha1" | "sha-1" => Ok(format!("{:040x}", Sha1::digest(data))),
        "md5" => Ok(format!("{:032x}", Md5::digest(data))),
        other => Err(anyhow!("Unsupported checksum kind: '{}'", other)),
    }
}
