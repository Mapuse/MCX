use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};

pub struct Extractor;

impl Extractor {
    pub fn new<P: AsRef<Path>>(_root: P) -> Self {
        Self
    }

    pub fn extract_zstd_archive<P: AsRef<Path>>(&self, archive_path: P, dest_dir: &Path) -> Result<Vec<PathBuf>> {
        let file = fs::File::open(archive_path)?;
        let decoder = zstd::stream::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        let mut extracted_files = Vec::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let rel = path.strip_prefix("./").unwrap_or(&path);

            if rel.is_absolute() || rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err(anyhow!("Structural hazard: Invalid path template detected"));
            }

            // Validate link targets before unpack creates them: a symlink or
            // hard link whose target escapes `dest_dir` would let later
            // archive entries write through it outside the staging area.
            match entry.header().entry_type() {
                tar::EntryType::Symlink | tar::EntryType::Link => {
                    if let Some(target) = entry.link_name()?
                        && (target.is_absolute()
                            || target.components().any(|c| matches!(c, std::path::Component::ParentDir)))
                    {
                        return Err(anyhow!(
                            "Structural hazard: link {:?} targets {:?} outside the package root",
                            path,
                            target
                        ));
                    }
                }
                _ => {}
            }

            if rel == Path::new("metadata.json") {
                continue;
            }

            let destination = dest_dir.join(rel);

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }

            entry.unpack(&destination)?;
            extracted_files.push(rel.to_path_buf());
        }

        Ok(extracted_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rejects_escaping_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        fs::create_dir_all(&stage).unwrap();

        // Build an archive containing a symlink pointing outside the stage.
        let payload = dir.path().join("evil.tar");
        let f = fs::File::create(&payload).unwrap();
        let mut builder = tar::Builder::new(f);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        header.set_path("usr/escape").unwrap();
        header.set_link_name("/etc/shadow").unwrap();
        builder.append_data(&mut header, "usr/escape", std::io::empty()).unwrap();
        builder.finish().unwrap();
        drop(builder);

        // Compress to zstd so extract_zstd_archive can read it.
        let xcs = dir.path().join("evil.tar.zst");
        {
            let mut input = fs::File::open(&payload).unwrap();
            let output = fs::File::create(&xcs).unwrap();
            let mut enc = zstd::stream::Encoder::new(output, 1).unwrap();
            std::io::copy(&mut input, &mut enc).unwrap();
            enc.finish().unwrap();
        }

        let ext = Extractor::new(dir.path());
        let result = ext.extract_zstd_archive(&xcs, &stage);
        assert!(result.is_err(), "escaping symlink must be rejected");
        assert!(!stage.join("usr/escape").exists());
    }

    #[test]
    fn test_extract_accepts_relative_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        fs::create_dir_all(&stage).unwrap();

        let payload = dir.path().join("good.tar");
        let f = fs::File::create(&payload).unwrap();
        let mut builder = tar::Builder::new(f);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        header.set_path("usr/lib/libfoo.so").unwrap();
        header.set_link_name("libfoo.so.1").unwrap();
        builder.append_data(&mut header, "usr/lib/libfoo.so", std::io::empty()).unwrap();

        let mut header2 = tar::Header::new_gnu();
        header2.set_size(5);
        header2.set_entry_type(tar::EntryType::Regular);
        header2.set_mode(0o644);
        builder.append_data(&mut header2, "usr/lib/libfoo.so.1", b"bytes".as_slice()).unwrap();
        builder.finish().unwrap();
        drop(builder);

        let xcs = dir.path().join("good.tar.zst");
        {
            let mut input = fs::File::open(&payload).unwrap();
            let output = fs::File::create(&xcs).unwrap();
            let mut enc = zstd::stream::Encoder::new(output, 1).unwrap();
            std::io::copy(&mut input, &mut enc).unwrap();
            enc.finish().unwrap();
        }

        let ext = Extractor::new(dir.path());
        let files = ext.extract_zstd_archive(&xcs, &stage).expect("relative symlinks are fine");
        assert_eq!(files.len(), 2);
        assert!(fs::symlink_metadata(stage.join("usr/lib/libfoo.so")).unwrap().file_type().is_symlink());
    }
}
