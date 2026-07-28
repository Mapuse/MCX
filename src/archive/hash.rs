use std::fs::File;
use std::io::Read;
use std::path::Path;
use sha2::{Sha256, Digest};
use sha1::Sha1;
use md5::Md5;
use anyhow::{Result, anyhow};
use crate::core::constants;

pub struct HashVerifier;

impl HashVerifier {
    pub fn calculate<P: AsRef<Path>>(path: P, kind: &str) -> Result<String> {
        let mut file = File::open(&path)?;
        let mut buffer = vec![0; constants::HASH_BUFFER_SIZE];

        match kind {
            "sha256" | "sha-256" => {
                let mut hasher = Sha256::new();
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 { break; }
                    hasher.update(&buffer[..count]);
                }
                Ok(format!("{:064x}", hasher.finalize()))
            }
            "sha1" | "sha-1" => {
                let mut hasher = Sha1::new();
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 { break; }
                    hasher.update(&buffer[..count]);
                }
                Ok(format!("{:040x}", hasher.finalize()))
            }
            "md5" => {
                let mut hasher = Md5::new();
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 { break; }
                    hasher.update(&buffer[..count]);
                }
                Ok(format!("{:032x}", hasher.finalize()))
            }
            other => Err(anyhow!("Unsupported checksum kind: '{}' (supported: sha256, sha1, md5)", other)),
        }
    }

    pub fn verify_integrity<P: AsRef<Path>>(path: P, kind: &str, expected_hash: &str) -> Result<()> {
        let actual_hash = Self::calculate(&path, kind)?;

        if actual_hash.eq_ignore_ascii_case(expected_hash) {
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