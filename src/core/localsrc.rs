use crate::archive::hash::hash_bytes;
use crate::core::constants;
use crate::core::package::compare_versions;
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceMode {
    Source,
    Prebuilt,
}

impl SourceMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceMode::Source => "source",
            SourceMode::Prebuilt => "prebuilt",
        }
    }

    pub fn parse(s: &str) -> Option<SourceMode> {
        match s.trim().to_lowercase().as_str() {
            "source" => Some(SourceMode::Source),
            "prebuilt" => Some(SourceMode::Prebuilt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSource {
    pub name: String,
    pub path: String,
    pub mode: SourceMode,
    pub enabled: bool,
    pub last_built: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSourceAction {
    Skip,
    Rebuild,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalSourceResult {
    Skipped(String),
    Built,
    InstalledPrebuilt,
}

pub fn validate_source_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 100
        && !name.chars().all(|c| c == '.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "Invalid local source name {:?}: allowed characters are [a-zA-Z0-9._-]",
            name
        ))
    }
}

pub fn looks_like_url(s: &str) -> bool {
    s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("git@")
        || s.starts_with("file://")
        || s.ends_with(".git")
}

/// The universal local-source kinds accepted by `--local-source-add`:
/// local directories (or a `manifest.json` file), git URLs, downloadable
/// archives (`http(s)`/`file` tarballs), `.json`/bare-manifest URLs, and
/// plain http URLs treated like downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Git,
    Dir,
    FileDir,
    Archive,
    Http,
    JsonUrl,
}

fn is_archive_filename(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".tar.gz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tgz")
}

/// Strip the `file://` scheme, returning the plain filesystem path.
pub fn strip_file_scheme(s: &str) -> String {
    s.strip_prefix("file://").unwrap_or(s).to_string()
}

/// Classify any path-or-url accepted by `--local-source-add`:
/// git vs dir vs archive tarball vs `.json` URL vs plain http.
pub fn classify_source_url(s: &str) -> SourceKind {
    let trimmed = s.trim();
    if trimmed.starts_with("git+") || trimmed.starts_with("git@") || trimmed.starts_with("git://") {
        return SourceKind::Git;
    }
    if let Some(rest) = trimmed.strip_prefix("file://") {
        if rest.ends_with(".git") {
            return SourceKind::Git;
        }
        if is_archive_filename(rest) {
            return SourceKind::Archive;
        }
        if rest.ends_with(".json")
            || Path::new(rest)
                .file_name()
                .map(|n| n == "manifest.json")
                .unwrap_or(false)
        {
            return SourceKind::JsonUrl;
        }
        return SourceKind::FileDir;
    }
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        if trimmed.ends_with(".git") {
            return SourceKind::Git;
        }
        if is_archive_filename(trimmed) {
            return SourceKind::Archive;
        }
        if trimmed.ends_with(".json") {
            return SourceKind::JsonUrl;
        }
        return SourceKind::Http;
    }
    if trimmed.ends_with(".git") {
        return SourceKind::Git;
    }
    SourceKind::Dir
}

/// Resolve a path-or-url's `manifest.json` for local (non-remote) sources.
pub fn source_manifest(path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_dir() {
        path.join("manifest.json")
    } else {
        path.to_path_buf()
    }
}

fn git_clone_url(path_or_url: &str) -> String {
    let trimmed = path_or_url.trim();
    if let Some(rest) = trimmed.strip_prefix("git+") {
        rest.to_string()
    } else if let Some(rest) = trimmed.strip_prefix("file://") {
        rest.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn update_git_mirror(url: &str, mirror: &Path) -> Result<()> {
    if mirror.join(".git").exists() {
        let fetch = std::process::Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(mirror)
            .status()
            .context("failed to run git fetch")?;
        if !fetch.success() {
            return Err(anyhow!("git fetch failed for mirror {:?}", mirror));
        }
        let _ = std::process::Command::new("git")
            .args(["remote", "set-head", "origin", "--auto"])
            .current_dir(mirror)
            .status();
        let reset = std::process::Command::new("git")
            .args(["reset", "--hard", "origin/HEAD"])
            .current_dir(mirror)
            .status()
            .context("failed to run git reset")?;
        if !reset.success() {
            return Err(anyhow!("git reset failed for mirror {:?}", mirror));
        }
    } else {
        if let Some(parent) = mirror.parent() {
            fs::create_dir_all(parent)?;
        }
        let clone = std::process::Command::new("git")
            .args(["clone", url])
            .arg(mirror)
            .status()
            .context("failed to run git clone")?;
        if !clone.success() {
            return Err(anyhow!("git clone failed for '{}'", url));
        }
    }
    Ok(())
}

pub fn git_head_fingerprint(mirror: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "origin/HEAD"])
        .current_dir(mirror)
        .output()
        .context("failed to run git rev-parse")?;
    if !output.status.success() {
        return Err(anyhow!("git rev-parse failed for mirror {:?}", mirror));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Whether `curl` (and/or `tar`) is available for download-style sources.
/// Returns a human-readable reason when a needed tool is missing.
pub fn check_download_tools(path_or_url: &str) -> Option<String> {
    let kind = classify_source_url(path_or_url);
    let is_file_scheme = path_or_url.trim().starts_with("file://");
    let needs_curl = !is_file_scheme
        && matches!(
            kind,
            SourceKind::Archive | SourceKind::Http | SourceKind::JsonUrl
        );
    let needs_tar = matches!(kind, SourceKind::Archive);
    if needs_curl && !find_tool_on_path(constants::TOOL_CURL) {
        return Some(format!(
            "curl not found on PATH; cannot download '{}'",
            path_or_url
        ));
    }
    if needs_tar && !find_tool_on_path(constants::TOOL_TAR) {
        return Some(format!(
            "tar not found on PATH; cannot extract '{}'",
            path_or_url
        ));
    }
    None
}

fn find_tool_on_path(candidate: &str) -> bool {
    if Path::new(candidate).is_file() {
        return true;
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(candidate).is_file()))
        .unwrap_or(false)
}

/// Result of materializing a local source into a concrete `manifest.json` plus
/// the changed-detection fingerprint (artifact SHA-256 bytes for download
/// sources, HEAD commit for git, manifest hash for dirs).
#[derive(Debug, Clone)]
pub struct MaterializedSource {
    pub fingerprint: String,
    pub manifest: PathBuf,
}

/// Materialize any local source kind:
/// - git → updated mirror under `mirrors/<name>/`, fingerprint = HEAD commit;
/// - dir / file:// dir → existing manifest, fingerprint = manifest hash;
/// - archive (http/file tarball) → download+extract into `work/<name>/`;
/// - json / plain http → downloaded artifact placed as `work/<name>/manifest.json`;
///   fingerprint for download sources is the sha256 of the artifact BYTES.
pub fn materialize_source(
    mgr: &LocalSourceManager,
    source: &LocalSource,
) -> Result<MaterializedSource> {
    match classify_source_url(&source.path) {
        SourceKind::Git => {
            let url = git_clone_url(&source.path);
            let mirror = mgr.mirrors_dir().join(&source.name);
            update_git_mirror(&url, &mirror)?;
            let manifest = mirror.join("manifest.json");
            if !manifest.exists() {
                return Err(anyhow!("Mirror of '{}' has no manifest.json", source.path));
            }
            let fingerprint = match git_head_fingerprint(&mirror) {
                Ok(head) => head,
                Err(_) => compute_manifest_fingerprint(&manifest)?,
            };
            Ok(MaterializedSource {
                fingerprint,
                manifest,
            })
        }
        SourceKind::Dir | SourceKind::FileDir => {
            let local = if source.path.trim().starts_with("file://") {
                strip_file_scheme(source.path.trim())
            } else {
                source.path.trim().to_string()
            };
            let manifest = source_manifest(&local);
            if !manifest.exists() {
                return Err(anyhow!("Source has no manifest.json: '{}'", local));
            }
            let fingerprint = compute_manifest_fingerprint(&manifest)?;
            Ok(MaterializedSource {
                fingerprint,
                manifest,
            })
        }
        SourceKind::Archive | SourceKind::Http | SourceKind::JsonUrl => {
            let (artifact, fingerprint) = download_artifact(mgr, source)?;
            let work_dir = mgr.work_dir().join(&source.name);
            if classify_source_url(&source.path) == SourceKind::Archive {
                let extracted = extract_archive(mgr, source, &artifact)?;
                let manifest = extracted.join("manifest.json");
                if !manifest.exists() {
                    return Err(anyhow!(
                        "Archive for '{}' extracted but has no manifest.json",
                        source.name
                    ));
                }
                Ok(MaterializedSource {
                    fingerprint,
                    manifest,
                })
            } else {
                if work_dir.exists() {
                    fs::remove_dir_all(&work_dir)?;
                }
                fs::create_dir_all(&work_dir)?;
                let manifest = work_dir.join("manifest.json");
                fs::copy(&artifact, &manifest)?;
                Ok(MaterializedSource {
                    fingerprint,
                    manifest,
                })
            }
        }
    }
}

fn download_dest_ext(path_or_url: &str) -> &'static str {
    let lower = path_or_url.to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        if lower.ends_with(".tgz") {
            ".tgz"
        } else {
            ".tar.gz"
        }
    } else if lower.ends_with(".tar.xz") {
        ".tar.xz"
    } else if lower.ends_with(".tar.zst") {
        ".tar.zst"
    } else {
        ".json"
    }
}

fn download_artifact(mgr: &LocalSourceManager, source: &LocalSource) -> Result<(PathBuf, String)> {
    let path = source.path.trim();
    let dest = mgr
        .downloads_dir()
        .join(format!("{}{}", source.name, download_dest_ext(path)));
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow!("Downloads dir has no parent"))?;
    fs::create_dir_all(parent)?;

    if source.path.trim().starts_with("file://") {
        let local = strip_file_scheme(path);
        let src = Path::new(&local);
        if !src.exists() {
            return Err(anyhow!("Local source file not found: '{}'", local));
        }
        if src.is_dir() {
            return Err(anyhow!(
                "Expected a file archive for '{}', got a directory",
                local
            ));
        }
        fs::copy(src, &dest).with_context(|| format!("Failed to copy '{}'", local))?;
    } else {
        let status = std::process::Command::new(constants::TOOL_CURL)
            .args(["-fSL", "-o"])
            .arg(&dest)
            .arg(path)
            .status()
            .with_context(|| format!("failed to run curl for '{}'", path))?;
        if !status.success() {
            return Err(anyhow!("curl download failed for '{}'", path));
        }
    }

    let fingerprint = crate::archive::hash::HashVerifier::calculate(&dest, "sha256")?;
    Ok((dest, fingerprint))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Gzip,
    Xz,
    Zstd,
    PlainTar,
}

fn detect_archive_kind(path: &Path) -> Result<ArchiveKind> {
    let mut file = fs::File::open(path)?;
    let mut buf = [0u8; 512];
    let n = {
        use std::io::Read;
        file.read(&mut buf)?
    };
    let head = &buf[..n];
    if head.starts_with(&[0x1f, 0x8b]) {
        return Ok(ArchiveKind::Gzip);
    }
    if head.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        return Ok(ArchiveKind::Xz);
    }
    if head.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        return Ok(ArchiveKind::Zstd);
    }
    if n >= 262 && &buf[257..262] == b"ustar" {
        return Ok(ArchiveKind::PlainTar);
    }
    Err(anyhow!(
        "'{}' is not a recognized archive (gzip/xz/zstd/tar)",
        path.display()
    ))
}

fn extract_archive(
    mgr: &LocalSourceManager,
    source: &LocalSource,
    artifact: &Path,
) -> Result<PathBuf> {
    let kind = detect_archive_kind(artifact)?;
    let work_dir = mgr.work_dir().join(&source.name);
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir)?;
    }
    fs::create_dir_all(&work_dir)?;

    let flag = match kind {
        ArchiveKind::Gzip => "-xzf",
        ArchiveKind::Xz => "-xJf",
        ArchiveKind::PlainTar => "-xf",
        ArchiveKind::Zstd => "--zstd",
    };
    let mut args: Vec<String> = vec![];
    if kind == ArchiveKind::Zstd {
        args.push("--zstd".to_string());
        args.push("-xf".to_string());
    } else {
        args.push(flag.to_string());
    }
    args.push(artifact.to_string_lossy().to_string());
    args.push("-C".to_string());
    args.push(work_dir.to_string_lossy().to_string());

    let output = std::process::Command::new(constants::TOOL_TAR)
        .args(&args)
        .current_dir(mgr.work_dir())
        .output()
        .with_context(|| format!("failed to run tar for '{}'", source.name))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "tar extraction failed for '{}': {}",
            source.name,
            stderr.trim()
        ));
    }
    Ok(work_dir)
}

pub fn parse_xcs_filename(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.strip_suffix(".xcs")?;
    let bytes = stem.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b'-'
            && bytes
                .get(idx + 1)
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            let (name_part, version_part) = stem.split_at(idx);
            if name_part.is_empty() || version_part.is_empty() {
                return None;
            }
            return Some((name_part.to_string(), version_part[1..].to_string()));
        }
    }
    None
}

pub fn predict_output_names(manifest_path: &Path) -> Result<Vec<String>> {
    if !manifest_path.exists() {
        return Err(anyhow!("Manifest not found: {}", manifest_path.display()));
    }
    let content = fs::read_to_string(manifest_path)?;
    let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        anyhow!(
            "Invalid manifest JSON in {}: {}",
            manifest_path.display(),
            e
        )
    })?;
    let mut names = Vec::new();
    match value.get("packages") {
        Some(serde_json::Value::Array(packages)) => {
            for pkg in packages {
                let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("");
                if !name.is_empty() && !version.is_empty() {
                    names.push(format!("{}-{}.xcs", name, version));
                }
            }
        }
        _ => {
            return Err(anyhow!(
                "Manifest {} has no 'packages' array",
                manifest_path.display()
            ));
        }
    }
    Ok(names)
}

pub fn compute_manifest_fingerprint(manifest: &Path) -> Result<String> {
    let content = fs::read(manifest)?;
    hash_bytes(&content, "sha256")
}

pub fn compute_prebuilt_fingerprint(dir: &Path) -> Result<String> {
    let mut entries: Vec<(String, u64)> = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "xcs").unwrap_or(false) {
                let len = entry.metadata()?.len();
                entries.push((entry.file_name().to_string_lossy().into_owned(), len));
            }
        }
    }
    entries.sort();
    let mut data = String::new();
    for (name, len) in entries {
        data.push_str(&format!("{}:{};", name, len));
    }
    hash_bytes(data.as_bytes(), "sha256")
}

pub fn scan_xcs_dir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "xcs").unwrap_or(false) {
                paths.push(path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn decide_action(
    fingerprint: Option<&str>,
    last_built: Option<&str>,
    force: bool,
    versions_outdated: bool,
) -> LocalSourceAction {
    if force || versions_outdated {
        return LocalSourceAction::Rebuild;
    }
    match (fingerprint, last_built) {
        (Some(fp), Some(lb)) if fp == lb => LocalSourceAction::Skip,
        _ => LocalSourceAction::Rebuild,
    }
}

pub fn build_ous_args(manifest: &str, outdir: &str) -> Vec<String> {
    vec![
        "-m".into(),
        manifest.into(),
        "-o".into(),
        outdir.into(),
        "-c".into(),
    ]
}

pub fn find_ous_binary() -> Option<String> {
    if let Ok(candidate) = std::env::var(constants::OUS_BIN_ENV)
        && !candidate.is_empty()
        && Path::new(&candidate).is_file()
    {
        return Some(candidate);
    }
    for candidate in constants::OUS_BINARY_CANDIDATES {
        if Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
        if !candidate.contains('/') {
            let on_path = std::env::var_os("PATH")
                .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(candidate).is_file()))
                .unwrap_or(false);
            if on_path {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

pub fn versions_outdated(
    db: &crate::core::database::Database,
    packages: &[(String, String)],
) -> bool {
    for (name, version) in packages {
        match db.get_package_manifest(name) {
            Err(_) => return true,
            Ok(installed) => {
                if compare_versions(&installed.version, version) == std::cmp::Ordering::Less {
                    return true;
                }
            }
        }
    }
    false
}

pub struct LocalSourceManager {
    config_file: PathBuf,
    base_dir: PathBuf,
    mirrors_dir: PathBuf,
    out_dir: PathBuf,
    downloads_dir: PathBuf,
    work_dir: PathBuf,
}

impl LocalSourceManager {
    pub fn new(root: &Path) -> Self {
        let base_dir = root.join(constants::PATH_LOCAL_SRC);
        Self {
            config_file: root.join(constants::PATH_LOCAL_SOURCES_INI),
            base_dir: base_dir.clone(),
            mirrors_dir: base_dir.join("mirrors"),
            out_dir: base_dir.join("out"),
            downloads_dir: base_dir.join("downloads"),
            work_dir: base_dir.join("work"),
        }
    }

    pub fn config_file(&self) -> &Path {
        &self.config_file
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn mirrors_dir(&self) -> &Path {
        &self.mirrors_dir
    }

    pub fn out_dir(&self) -> &Path {
        &self.out_dir
    }

    pub fn downloads_dir(&self) -> &Path {
        &self.downloads_dir
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    pub fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.config_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&self.base_dir)?;
        fs::create_dir_all(&self.mirrors_dir)?;
        fs::create_dir_all(&self.out_dir)?;
        fs::create_dir_all(&self.downloads_dir)?;
        fs::create_dir_all(&self.work_dir)?;
        Ok(())
    }

    pub fn load_sources(&self) -> Result<Vec<LocalSource>> {
        if !self.config_file.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.config_file).map_err(|e| {
            anyhow!(
                "Failed to read local sources config {}: {}",
                self.config_file.display(),
                e
            )
        })?;
        Self::parse_ini(&content)
    }

    fn parse_ini(content: &str) -> Result<Vec<LocalSource>> {
        let mut sources = Vec::new();
        let mut name: Option<String> = None;
        let mut path: Option<String> = None;
        let mut mode = SourceMode::Source;
        let mut enabled = true;
        let mut last_built: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                if let (Some(name), Some(path)) = (name.take(), path.take()) {
                    sources.push(LocalSource {
                        name,
                        path,
                        mode,
                        enabled,
                        last_built: last_built.take(),
                    });
                }
                let section = line[1..line.len() - 1].trim().to_string();
                validate_source_name(&section)?;
                name = Some(section);
                path = None;
                mode = SourceMode::Source;
                enabled = true;
                last_built = None;
                continue;
            }
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim().to_string();
                match key {
                    "path" => path = Some(value),
                    "mode" => {
                        mode = SourceMode::parse(&value)
                            .ok_or_else(|| anyhow!("Invalid local source mode {:?}", value))?;
                    }
                    "enabled" => enabled = value.to_lowercase() == "true" || value == "1",
                    "last_built" => {
                        last_built = if value.is_empty() { None } else { Some(value) };
                    }
                    _ => {}
                }
            }
        }

        if let (Some(name), Some(path)) = (name, path) {
            sources.push(LocalSource {
                name,
                path,
                mode,
                enabled,
                last_built,
            });
        }

        Ok(sources)
    }

    fn format_ini(sources: &[LocalSource]) -> String {
        let mut output = String::new();
        for source in sources {
            output.push_str(&format!("[{}]\n", source.name));
            output.push_str(&format!("path = {}\n", source.path));
            output.push_str(&format!("mode = {}\n", source.mode.as_str()));
            output.push_str(&format!("enabled = {}\n", source.enabled));
            if let Some(ref last_built) = source.last_built {
                output.push_str(&format!("last_built = {}\n", last_built));
            }
            output.push('\n');
        }
        output
    }

    pub fn save_sources(&self, sources: &[LocalSource]) -> Result<()> {
        let payload = Self::format_ini(sources);
        fs::write(&self.config_file, payload).map_err(|e| {
            anyhow!(
                "Failed to write local sources config {}: {}",
                self.config_file.display(),
                e
            )
        })
    }

    pub fn add_source(&self, source: LocalSource) -> Result<()> {
        validate_source_name(&source.name)?;
        let mut sources = self.load_sources()?;
        if sources.iter().any(|s| s.name == source.name) {
            return Err(anyhow!("Local source '{}' already exists", source.name));
        }
        sources.push(source);
        self.save_sources(&sources)
    }

    pub fn remove_source(&self, name: &str) -> Result<()> {
        let mut sources = self.load_sources()?;
        let before = sources.len();
        sources.retain(|s| s.name != name);
        if sources.len() == before {
            return Err(anyhow!("Local source '{}' not found", name));
        }
        self.save_sources(&sources)
    }

    pub fn get_source(&self, name: &str) -> Result<LocalSource> {
        let sources = self.load_sources()?;
        sources
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("Local source '{}' not found", name))
    }

    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let mut sources = self.load_sources()?;
        let source = sources
            .iter_mut()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("Local source '{}' not found", name))?;
        source.enabled = enabled;
        self.save_sources(&sources)
    }

    pub fn update_last_built(&self, name: &str, fingerprint: &str) -> Result<()> {
        let mut sources = self.load_sources()?;
        let source = sources
            .iter_mut()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("Local source '{}' not found", name))?;
        source.last_built = Some(fingerprint.to_string());
        self.save_sources(&sources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_source_manager(identifier: &str) -> (LocalSourceManager, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "mcx_lsrc_{}_{}",
            identifier,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let mgr = LocalSourceManager::new(&dir);
        mgr.initialize().expect("init manager");
        (mgr, dir)
    }

    fn sample_source(name: &str) -> LocalSource {
        LocalSource {
            name: name.to_string(),
            path: "/srv/src/pkg".to_string(),
            mode: SourceMode::Source,
            enabled: true,
            last_built: None,
        }
    }

    #[test]
    fn test_validate_source_name() {
        for evil in ["../evil", "a/b", "", ".", "..", "space in name"] {
            assert!(
                validate_source_name(evil).is_err(),
                "{:?} must be rejected",
                evil
            );
        }
        for good in ["hello-pkg", "main_repo.2", "a", "x".repeat(100).as_str()] {
            assert!(
                validate_source_name(good).is_ok(),
                "{:?} must be accepted",
                good
            );
        }
        assert!(validate_source_name(&"y".repeat(101)).is_err());
    }

    #[test]
    fn test_looks_like_url() {
        for url in [
            "https://example.com/repo.git",
            "http://host/path",
            "git@github.com:user/repo.git",
            "file:///srv/src",
            "repo.git",
        ] {
            assert!(looks_like_url(url), "{url} must be a URL");
        }
        for local in ["/usr/local/src/pkg", "./relative/dir", "~/src/pkg", "pkg"] {
            assert!(!looks_like_url(local), "{local} must be a local path");
        }
    }

    #[test]
    fn test_classify_source_url() {
        use SourceKind::*;
        let cases = [
            ("git@github.com:user/repo.git", Git),
            ("git://host/repo.git", Git),
            ("git+https://host/repo", Git),
            ("https://host/repo.git", Git),
            ("file:///srv/some.src.git", Git),
            ("https://host/pkg-1.0.0.tar.gz", Archive),
            ("http://host/pkg.tgz", Archive),
            ("file:///srv/pkg-1.0.0.tar.xz", Archive),
            ("file:///srv/pkg-1.0.0.tar.zst", Archive),
            ("https://host/manifest.json", JsonUrl),
            ("file:///srv/mirror/manifest.json", JsonUrl),
            ("https://host/anything/else", Http),
            ("http://host/plain", Http),
            ("/usr/local/src/pkg", Dir),
            ("./relative/pkg", Dir),
            ("file:///srv/some.src", FileDir),
            ("/srv/pkg-1.0.0.tar.gz", Dir),
        ];
        for (input, expected) in cases {
            assert_eq!(classify_source_url(input), expected, "{input}");
        }
        assert_eq!(strip_file_scheme("file:///srv/a"), "/srv/a");
        assert_eq!(strip_file_scheme("/srv/a"), "/srv/a");
    }

    #[test]
    fn test_source_manifest_resolution() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("manifest.json"), "{}").expect("write manifest");
        assert_eq!(
            source_manifest(dir.path().to_str().unwrap()),
            dir.path().join("manifest.json")
        );
        let file = dir.path().join("other.json");
        fs::write(&file, "{}").expect("write other");
        assert_eq!(source_manifest(file.to_str().unwrap()), file);
    }

    #[test]
    fn test_parse_xcs_filename() {
        assert_eq!(
            parse_xcs_filename("hello-pkg-1.2.0.xcs"),
            Some(("hello-pkg".to_string(), "1.2.0".to_string()))
        );
        assert_eq!(
            parse_xcs_filename("pkg-1.xcs"),
            Some(("pkg".to_string(), "1".to_string()))
        );
        assert_eq!(parse_xcs_filename("no-version.xcs"), None);
        assert_eq!(parse_xcs_filename("pkg.xcs"), None);
        assert_eq!(parse_xcs_filename("pkg-abc.xcs"), None);
    }

    #[test]
    fn test_build_ous_args() {
        let args = build_ous_args("/tmp/m/manifest.json", "/var/lib/out");
        assert_eq!(
            args,
            vec!["-m", "/tmp/m/manifest.json", "-o", "/var/lib/out", "-c"]
        );
        assert!(
            !args.iter().any(|a| a == "-n" || a == "--no-auto"),
            "no-auto must not be passed"
        );
    }

    #[test]
    fn test_decide_action() {
        assert_eq!(
            decide_action(Some("fp"), Some("fp"), false, false),
            LocalSourceAction::Skip
        );
        assert_eq!(
            decide_action(Some("fp"), Some("other"), false, false),
            LocalSourceAction::Rebuild
        );
        assert_eq!(
            decide_action(Some("fp"), Some("fp"), true, false),
            LocalSourceAction::Rebuild
        );
        assert_eq!(
            decide_action(Some("fp"), Some("fp"), false, true),
            LocalSourceAction::Rebuild
        );
        assert_eq!(
            decide_action(Some("fp"), None, false, false),
            LocalSourceAction::Rebuild
        );
        assert_eq!(
            decide_action(None, Some("fp"), false, false),
            LocalSourceAction::Rebuild
        );
    }

    #[test]
    fn test_source_mode_parse() {
        assert_eq!(SourceMode::parse("source"), Some(SourceMode::Source));
        assert_eq!(SourceMode::parse("prebuilt"), Some(SourceMode::Prebuilt));
        assert_eq!(SourceMode::parse("SOURCE"), Some(SourceMode::Source));
        assert_eq!(SourceMode::parse("bogus"), None);
    }

    #[test]
    fn test_config_round_trip_with_unknown_keys() {
        let (mgr, dir) = temp_source_manager("roundtrip");
        let content = "\
# comment\n\
[first]\n\
path = /srv/a\n\
mode = source\n\
enabled = true\n\
bogus = ignored\n\
\n\
[second]\n\
path = /srv/b\n\
mode = prebuilt\n\
enabled = false\n\
last_built = deadbeef\n\
priority = 100\n\
";
        fs::write(&mgr.config_file, content).expect("write config");
        let sources = mgr.load_sources().expect("load sources");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].name, "first");
        assert_eq!(sources[0].mode, SourceMode::Source);
        assert!(sources[0].enabled);
        assert_eq!(sources[0].last_built, None);
        assert_eq!(sources[1].name, "second");
        assert_eq!(sources[1].mode, SourceMode::Prebuilt);
        assert!(!sources[1].enabled);
        assert_eq!(sources[1].last_built.as_deref(), Some("deadbeef"));

        mgr.save_sources(&sources).expect("save sources");
        let reloaded = mgr.load_sources().expect("reload sources");
        assert_eq!(reloaded, sources);

        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn test_add_remove_and_duplicate() {
        let (mgr, dir) = temp_source_manager("addremove");
        mgr.add_source(sample_source("alpha")).expect("add alpha");
        mgr.add_source(sample_source("beta")).expect("add beta");
        assert!(
            mgr.add_source(sample_source("alpha")).is_err(),
            "duplicate rejected"
        );
        assert!(
            mgr.add_source(LocalSource {
                name: "../evil".to_string(),
                ..sample_source("..")
            })
            .is_err(),
            "invalid name rejected"
        );

        let loaded = mgr.load_sources().expect("load");
        assert_eq!(loaded.len(), 2);

        mgr.remove_source("alpha").expect("remove alpha");
        assert!(mgr.remove_source("alpha").is_err(), "missing rejected");
        assert_eq!(mgr.load_sources().expect("load").len(), 1);

        mgr.update_last_built("beta", "abc123")
            .expect("record last built");
        assert_eq!(
            mgr.get_source("beta")
                .expect("get beta")
                .last_built
                .as_deref(),
            Some("abc123")
        );

        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn test_manifest_fingerprint_is_deterministic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = dir.path().join("manifest.json");
        fs::write(
            &manifest,
            r#"{"packages":[{"name":"p","version":"1.0.0"}]}"#,
        )
        .expect("write manifest");
        let first = compute_manifest_fingerprint(&manifest).expect("fingerprint");
        let second = compute_manifest_fingerprint(&manifest).expect("fingerprint again");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        fs::write(
            &manifest,
            r#"{"packages":[{"name":"p","version":"1.1.0"}]}"#,
        )
        .expect("rewrite manifest");
        let changed = compute_manifest_fingerprint(&manifest).expect("changed fingerprint");
        assert_ne!(first, changed);
    }

    #[test]
    fn test_prebuilt_fingerprint_is_sorted_and_content_based() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("b-1.0.0.xcs"), "bbbb").expect("write b");
        fs::write(dir.path().join("a-1.0.0.xcs"), "aaaa").expect("write a");
        let first = compute_prebuilt_fingerprint(dir.path()).expect("fingerprint");
        let second = compute_prebuilt_fingerprint(dir.path()).expect("fingerprint again");
        assert_eq!(first, second);
        fs::write(dir.path().join("a-1.0.0.xcs"), "aaaaa").expect("grow a");
        let changed = compute_prebuilt_fingerprint(dir.path()).expect("changed fingerprint");
        assert_ne!(first, changed);
    }

    #[test]
    fn test_predict_output_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = dir.path().join("manifest.json");
        fs::write(
            &manifest,
            r#"{"packages":[{"name":"alpha","version":"1.2.0"},{"name":"beta","version":"2.0.0"}]}"#,
        )
        .expect("write manifest");
        assert_eq!(
            predict_output_names(&manifest).expect("names"),
            vec!["alpha-1.2.0.xcs", "beta-2.0.0.xcs"]
        );
        fs::write(&manifest, "not json").expect("write bad manifest");
        assert!(predict_output_names(&manifest).is_err());
    }

    #[test]
    fn test_missing_config_loads_empty() {
        let (mgr, dir) = temp_source_manager("empty");
        assert!(mgr.load_sources().expect("load").is_empty());
        fs::remove_dir_all(&dir).expect("cleanup");
    }
}
