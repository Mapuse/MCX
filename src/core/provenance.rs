use std::path::Path;
use serde::{Serialize, Deserialize};
use crate::core::database::{PackageMetadata, RepositoryInfo};
use crate::core::localsrc::{LocalSourceManager, SourceMode};
use crate::core::repo::RepositoryManager;
use crate::utils::ui::UserInterface;

/// Origin provenance block embedded by the producer (Outsider) inside each
/// package's `metadata.json`. `source_url` is the ORIGINAL source-of-origin;
/// `metadata.source` is the registry/pool download URL which the publisher may
/// rewrite. Old archives may lack the block entirely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PackageProvenance {
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub built_at: Option<String>,
    #[serde(default)]
    pub builder: Option<String>,
}

/// Registry layout marker: the Outsider pool lives at `…/pool/{arch}/{name}/…`.
/// The repository base is everything before the `/pool/` segment.
const POOL_SEGMENT: &str = "/pool/";

/// Returns the repository base (everything before `/pool/`) when the URL points
/// into an Outsider pool, plus the remainder after the pool segment.
fn pool_base(url: &str) -> Option<(&str, &str)> {
    url.find(POOL_SEGMENT).map(|idx| (&url[..idx], &url[idx + POOL_SEGMENT.len()..]))
}

/// Derive a repository name from a base URL. The INI section / index file name
/// grammar is strictly `[a-zA-Z0-9._-]`; anything else collapses to a dot.
fn derive_repo_name(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    let host_and_path = trimmed
        .split("://")
        .nth(1)
        .unwrap_or(trimmed)
        .trim_start_matches("git+");
    let cleaned: String = host_and_path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '.'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        return NEEDED_FALLBACK_NAME.to_string();
    }
    if cleaned.len() > 100 {
        let hash = crate::archive::hash::hash_bytes(cleaned.as_bytes(), "sha256")
            .unwrap_or_else(|_| "unknown".to_string());
        return format!("{}.{}", &cleaned[..80], &hash[..16]);
    }
    cleaned
}

const NEEDED_FALLBACK_NAME: &str = "provenance-repo";

/// Best-effort origin auto-linking performed at a successful LOCAL package
/// install:
///
/// 1. If `provenance.source_url` (fallback `metadata.source`) points into an
///    Outsider pool (`/pool/…`), auto `repo-add` the repository whose base URL
///    is everything before `/pool/` — but only when that exact URL is not
///    already registered.
/// 2. Independently record a source-mode local-source entry named after the
///    package with `path` = `provenance.source_url` (fallback `metadata.source`
///    minus any `/pool/` rewrite) unless an entry with that path already exists.
///    When a registry link exists/registry drives updates the local source is
///    recorded disabled (`enabled = false`); otherwise `enabled = true`.
///
/// Every step is idempotent and best-effort: failures only log a warning and
/// never fail the already-succeeded install. Both links write to the same
/// per-root config (`etc/mcx/repo.ini`, `etc/mcx/localsources.ini`).
///
/// `embedded_source` is the origin URL carried in the archive's `metadata.json`
/// `source` field (the DB `meta.source` is only the transport location);
/// `meta.provenance.source_url` (when present) is authoritative over it.
pub fn auto_link_origin(meta: &PackageMetadata, embedded_source: &str, root: &Path) {
    let provenance_url = meta
        .provenance
        .as_ref()
        .map(|p| p.source_url.trim())
        .filter(|s| !s.is_empty());
    let embedded = embedded_source.trim();
    let source_url = provenance_url.or(if embedded.is_empty() || !is_linkable_source(embedded) {
        None
    } else {
        Some(embedded)
    });
    let Some(source_url) = source_url else {
        // No provenance block and no usable origin URL in metadata.source:
        // the package cannot be traced to a fetchable origin, so nothing is
        // auto-linked (e.g. old prebuilt archives carrying marker sources).
        return;
    };

    let registry_added = link_registry_repo(source_url, root);
    record_local_source(meta, source_url, provenance_url.is_none(), registry_added, root);
}

fn is_linkable_source(s: &str) -> bool {
    s.contains("://") || s.starts_with("git@") || s.ends_with(".git")
}

fn link_registry_repo(source_url: &str, root: &Path) -> bool {
    let Some((base, _rest)) = pool_base(source_url) else {
        return false;
    };
    let mgr = RepositoryManager::new(root);
    if let Err(e) = mgr.initialize() {
        UserInterface::warning(&format!("Auto-link: could not initialize repository config: {e}"));
        return false;
    }
    let repo_base = base.to_string();
    let existing = match mgr.load_repositories() {
        Ok(repos) => repos,
        Err(e) => {
            UserInterface::warning(&format!("Auto-link: could not read repositories: {e}"));
            return false;
        }
    };
    if existing.iter().any(|r| r.url.trim_end_matches('/') == repo_base.trim_end_matches('/')) {
        // Exact URL already registered: nothing to add, but the registry link exists.
        return true;
    }
    let repo_name = derive_repo_name(&repo_base);
    match mgr.add_repository(RepositoryInfo {
        name: repo_name,
        url: repo_base,
        checksum: None,
        enabled: true,
    }) {
        Ok(_) => true,
        Err(e) => {
            UserInterface::warning(&format!("Auto-link: could not auto-add repository: {e}"));
            false
        }
    }
}

fn record_local_source(
    meta: &PackageMetadata,
    source_url: &str,
    source_is_metadata: bool,
    registry_link_present: bool,
    root: &Path,
) {
    // Local-source path: provenance.source_url verbatim, or metadata.source
    // with any "/pool/…" rewrite stripped.
    let source_path = if source_is_metadata {
        match pool_base(source_url) {
            Some((base, _)) => base.to_string(),
            None => source_url.to_string(),
        }
    } else {
        source_url.to_string()
    };
    if source_path.trim().is_empty() {
        return;
    }

    let mgr = LocalSourceManager::new(root);
    if let Err(e) = mgr.initialize() {
        UserInterface::warning(&format!("Auto-link: could not initialize local sources config: {e}"));
        return;
    }
    let sources = match mgr.load_sources() {
        Ok(sources) => sources,
        Err(e) => {
            UserInterface::warning(&format!("Auto-link: could not read local sources: {e}"));
            return;
        }
    };
    if sources.iter().any(|s| s.path == source_path) {
        // An entry with this exact path already exists: leave it untouched.
        return;
    }
    let entry = crate::core::localsrc::LocalSource {
        name: meta.pkg_name.clone(),
        path: source_path,
        mode: SourceMode::Source,
        enabled: !registry_link_present,
        last_built: None,
    };
    if let Err(e) = mgr.add_source(entry) {
        UserInterface::warning(&format!(
            "Auto-link: could not record local source for '{}': {e}",
            meta.pkg_name
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provenance_parses_optional_fields() {
        let json = r#"{
            "source_type": "git",
            "source_url": "https://git.example.com/srv/proj.git",
            "source_revision": "abc123",
            "builder": "ous-1.2.3"
        }"#;
        let prov: PackageProvenance = serde_json::from_str(json).expect("parse");
        assert_eq!(prov.source_type, "git");
        assert_eq!(prov.source_url, "https://git.example.com/srv/proj.git");
        assert_eq!(prov.source_revision.as_deref(), Some("abc123"));
        assert_eq!(prov.built_at, None);
        assert_eq!(prov.builder.as_deref(), Some("ous-1.2.3"));

        let empty: PackageProvenance = serde_json::from_str("{}").expect("empty object");
        assert_eq!(empty.source_type, "");
        assert_eq!(empty.source_revision, None);
    }

    #[test]
    fn test_pool_base_detection() {
        let url = "https://packages.example.org/mirror/pool/x86_64/hello-pkg/hello-pkg-1.0.0.xcs";
        let (base, rest) = pool_base(url).expect("pool url");
        assert_eq!(base, "https://packages.example.org/mirror");
        assert!(rest.starts_with("x86_64/hello-pkg/"));
        assert!(pool_base("https://example.com/plain/file.tar.gz").is_none());
    }

    #[test]
    fn test_derive_repo_name_sanitizes() {
        assert_eq!(derive_repo_name("https://packages.example.org/mirror"), "packages.example.org.mirror");
        assert_eq!(derive_repo_name("http://git.example.com/a_b-c"), "git.example.com.a_b-c");
        assert_eq!(derive_repo_name("https://x/y:"), "x.y");
    }
}