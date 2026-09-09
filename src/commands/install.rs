use crate::archive::extract::Extractor;
use crate::archive::hash::HashVerifier;
use crate::core::cgroup::CgroupController;
use crate::core::component::ComponentFilter;
use crate::core::constants;
use crate::core::database::PackageMetadata;
use crate::core::db::{Database, DbTransaction};
use crate::core::declarative::ProfileValidator;
use crate::core::package::compare_versions;
use crate::core::plugin::{PluginEvent, PluginHook, PluginManager};
use crate::core::profiler::SystemProfile;
use crate::core::security::SecurityMonitor;
use crate::core::solver::DependencySolver;
use crate::core::vendor::VendorManager;
use crate::network::download::Downloader;
use crate::utils::ui::UserInterface;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A package fully prepared in its private staging directory: archive hash
/// verified, contents extracted, component filtering applied and per-file
/// digests computed. Nothing outside the stage area has been touched yet.
struct StagedPackage {
    meta: PackageMetadata,
}

pub struct InstallCommand {
    root: String,
    db: Arc<Database>,
    cgroup_mgr: CgroupController,
    security_mon: Option<Arc<SecurityMonitor>>,
    profile_path: Option<PathBuf>,
    plugin_mgr: Option<Arc<PluginManager>>,
    component_filter: Option<ComponentFilter>,
}

impl InstallCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self {
            root,
            db,
            cgroup_mgr: CgroupController::new(),
            security_mon: None,
            profile_path: None,
            plugin_mgr: None,
            component_filter: None,
        }
    }

    pub fn with_cgroup(mut self, mgr: CgroupController) -> Self {
        self.cgroup_mgr = mgr;
        self
    }
    pub fn with_security(mut self, mon: Arc<SecurityMonitor>) -> Self {
        self.security_mon = Some(mon);
        self
    }
    pub fn with_profile(mut self, path: PathBuf) -> Self {
        self.profile_path = Some(path);
        self
    }
    pub fn with_plugin_mgr(mut self, mgr: Arc<PluginManager>) -> Self {
        self.plugin_mgr = Some(mgr);
        self
    }
    pub fn with_component_filter(mut self, filter: ComponentFilter) -> Self {
        self.component_filter = Some(filter);
        self
    }

    pub async fn execute(&self, packages: &[String]) -> Result<()> {
        if packages.is_empty() {
            return Err(anyhow!("Target specification empty"));
        }

        self.fire_hooks(PluginHook::PreInstall, packages);
        let sys_profile = SystemProfile::probe();
        let mut solver = DependencySolver::new(Arc::clone(&self.db));
        for pkg in packages {
            solver = solver.add_target(pkg);
        }

        let mut plan = solver.solve()?;
        for meta in &plan {
            if !crate::core::arch::package_matches_host(&meta.architecture) {
                return Err(anyhow!(
                    "Package '{}' architecture '{}' is not compatible with this host",
                    meta.pkg_name,
                    meta.architecture
                ));
            }
        }
        let root_path = Path::new(&self.root);
        let cache_dir = root_path.join(constants::PATH_CACHE);
        fs::create_dir_all(&cache_dir)?;

        // The solver pins an already-installed target to its installed
        // manifest, so upgrades are resolved to the newest *available* version
        // here. The new `.xcs` archive is then read straight from its source
        // and only the diff against the installed content is placed.
        let all_available = self.db.get_all_available_packages()?;
        let mut newest_available: HashMap<String, PackageMetadata> = HashMap::new();
        for meta in &all_available {
            if !crate::core::arch::package_matches_host(&meta.architecture) {
                continue;
            }
            match newest_available.get(&meta.pkg_name) {
                Some(existing)
                    if compare_versions(&existing.version, &meta.version)
                        != std::cmp::Ordering::Less => {}
                _ => {
                    newest_available.insert(meta.pkg_name.clone(), meta.clone());
                }
            }
        }
        for meta in &mut plan {
            if self.db.is_package_installed(&meta.pkg_name)?
                && let Some(newer) = newest_available.get(&meta.pkg_name)
                && compare_versions(&meta.version, &newer.version) == std::cmp::Ordering::Less
            {
                *meta = newer.clone();
            }
        }

        // ── Phase 1: acquire archives. Cached copies win, then locally
        // vendored payloads (offline installs), and only then the network —
        // all downloads are issued as one bounded batch.
        let downloader = Downloader::new();
        let vendor_mgr = VendorManager::new(&self.root);

        let mut pending: Vec<(PackageMetadata, PathBuf)> = Vec::new();
        let mut downloads: Vec<(String, PathBuf)> = Vec::new();

        for meta in &plan {
            if self.db.is_package_installed(&meta.pkg_name)? {
                // Skip only when the installed version already satisfies the
                // resolved target. An older installed version must be
                // re-installed so `mcx install <pkg>` doubles as an upgrade.
                if let Ok(installed) = self.db.get_package_manifest(&meta.pkg_name)
                    && compare_versions(&installed.version, &meta.version)
                        != std::cmp::Ordering::Less
                {
                    continue;
                }
            }
            let archive_name = format!("{}-{}.xcs", meta.pkg_name, meta.version);
            let target_path = cache_dir.join(&archive_name);

            if !target_path.exists() {
                match vendor_mgr.lookup_vendor_archive(&meta.pkg_name, &meta.version) {
                    Some(vendored) => {
                        crate::core::transaction::atomic_copy(&vendored, &target_path)?;
                        UserInterface::info(&format!(
                            "Using vendored offline archive for {}",
                            meta.pkg_name
                        ));
                    }
                    None => {
                        downloads.push((meta.source.clone(), target_path.clone()));
                        downloads.push((
                            sidecar_url_for(&meta.source),
                            cache_dir.join(sidecar_path_for(&meta.pkg_name, &meta.version)),
                        ));
                    }
                }
            }
            pending.push((meta.clone(), target_path));
        }

        if !downloads.is_empty() {
            let results = downloader.download_many(&downloads).await;
            for (_idx, result) in results {
                result.map_err(|e| anyhow!("Download failed: {}", e))?;
            }
        }

        // ── Phase 2: verify and extract into per-package stage directories.
        // No live-system path is modified in this phase, so a failure here
        // leaves the installation untouched.
        let stage_base = root_path.join(constants::PATH_STAGE);
        if stage_base.exists() {
            fs::remove_dir_all(&stage_base)?;
        }
        fs::create_dir_all(&stage_base)?;

        let use_parallel = sys_profile.cpu_count >= constants::CPU_THRESHOLD_LOW
            && sys_profile.available_ram_mb >= constants::RAM_THRESHOLD_MEDIUM_MB;

        let mut staged: Vec<StagedPackage> = Vec::with_capacity(pending.len());

        if use_parallel {
            let mut handles = Vec::new();
            let filter_clone = self.component_filter.clone();
            for (meta, path) in &pending {
                let meta_clone = meta.clone();
                let path_clone = path.clone();
                let root = self.root.clone();
                let filter = filter_clone.clone();

                handles.push(tokio::task::spawn_blocking(
                    move || -> Result<StagedPackage> {
                        stage_package(Path::new(&root), meta_clone, &path_clone, filter.as_ref())
                    },
                ));
            }

            for handle in handles {
                staged.push(handle.await.map_err(|e| anyhow!("Task failed: {}", e))??);
            }
        } else {
            for (meta, path) in &pending {
                staged.push(stage_package(
                    root_path,
                    meta.clone(),
                    path,
                    self.component_filter.as_ref(),
                )?);
            }
        }

        // ── Phase 3: the committed window. Collision checks run against the
        // database BEFORE any live write; every placed path is journaled so
        // dropping the transaction rolls the system back. Upgrades diff the
        // staged archive against the previously installed content and only
        // touch the files that actually changed — the new `.xcs` archive is
        // read from its source and the diff is applied straight from it.
        let installed_map: HashMap<String, PackageMetadata> = self
            .db
            .get_all_installed_packages()?
            .into_iter()
            .map(|m| (m.pkg_name.clone(), m))
            .collect();
        let mut transaction = self.db.begin_transaction()?;
        let installed_root = root_path.join(constants::PATH_ACTIVE);
        let mut replaced_links: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut diff_report: Vec<(String, usize, usize)> = Vec::new();

        for sp in &staged {
            transaction.register_package_placement(&sp.meta)?;
        }

        let placement_result = (|| -> Result<()> {
            for sp in &staged {
                let prev = installed_map.get(&sp.meta.pkg_name);
                let (changed, removed) = place_package(
                    root_path,
                    &installed_root,
                    sp,
                    &mut transaction,
                    &mut replaced_links,
                    prev,
                )?;
                if prev.is_some() {
                    diff_report.push((sp.meta.pkg_name.clone(), changed, removed));
                }
            }
            Ok(())
        })();

        if let Err(e) = placement_result {
            // Restore symlink destinations we replaced; the transaction's
            // Drop handler then restores backed-up files and removes newly
            // placed ones.
            for (link, original_target) in replaced_links.iter().rev() {
                let _ = fs::remove_file(link);
                let _ = std::os::unix::fs::symlink(original_target, link);
            }
            return Err(e.context("Install rolled back"));
        }

        transaction.commit()?;

        for (name, changed, removed) in &diff_report {
            UserInterface::success(&format!(
                "{}: {} changed, {} removed (diff)",
                name, changed, removed
            ));
        }

        // sandbox setup for each installed package
        for sp in &staged {
            // cgroup: apply resource limits; surface failures instead of
            // silently pretending enforcement succeeded.
            if let Err(e) = self.cgroup_mgr.enforce_resource_limits(
                &sp.meta.pkg_name,
                constants::DEFAULT_CGROUP_MAX_MEMORY_MB,
                constants::DEFAULT_CGROUP_MAX_CPU_PERCENT,
            ) {
                UserInterface::warning(&format!(
                    "Resource limits were NOT enforced for {} (cgroup setup failed): {}",
                    sp.meta.pkg_name, e
                ));
            }

            // security monitor: register package
            if let Some(ref mon) = self.security_mon {
                mon.register_package(&sp.meta.pkg_name);
            }
        }

        // profile validation: detect drift between declared and actual state
        if let Some(ref profile_path) = self.profile_path
            && profile_path.exists()
        {
            match ProfileValidator::load_profile(profile_path) {
                Ok(profile) => {
                    let current: Vec<String> = self
                        .db
                        .get_all_installed_packages()
                        .unwrap_or_default()
                        .iter()
                        .map(|p| p.pkg_name.clone())
                        .collect();
                    let (to_install, to_remove) =
                        ProfileValidator::compile_profile_diff(&current, &profile.packages);
                    if !to_install.is_empty() || !to_remove.is_empty() {
                        UserInterface::profile(&format!(
                            "Drift: {} to install, {} to remove",
                            to_install.len(),
                            to_remove.len()
                        ));
                    }
                }
                Err(e) => {
                    UserInterface::warning(&format!("Profile validation skipped: {}", e));
                }
            }
        }

        for sp in &staged {
            for svc in sp.meta.all_services() {
                let svc_cmd = crate::commands::service::ServiceCommand::new(
                    self.root.clone(),
                    Arc::clone(&self.db),
                );
                if let Err(e) = svc_cmd.register_service(svc) {
                    UserInterface::warning(&format!(
                        "Failed to register service '{}' for {}: {}",
                        svc.name, sp.meta.pkg_name, e
                    ));
                }
            }
        }

        self.fire_hooks(PluginHook::PostInstall, packages);

        Ok(())
    }

    fn fire_hooks(&self, hook: PluginHook, packages: &[String]) {
        if let Some(ref mgr) = self.plugin_mgr {
            for pkg in packages {
                let event = PluginEvent {
                    hook: hook.as_str().to_string(),
                    package: Some(pkg.clone()),
                    root: self.root.clone(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                mgr.fire_hook(hook, &event);
            }
        }
    }
}

fn sidecar_url_for(source: &str) -> String {
    format!("{}.sha256", source)
}

fn sidecar_path_for(name: &str, version: &str) -> PathBuf {
    PathBuf::from(format!("{}-{}.xcs.sha256", name, version))
}

fn stage_package(
    root: &Path,
    meta: PackageMetadata,
    archive_path: &Path,
    filter: Option<&ComponentFilter>,
) -> Result<StagedPackage> {
    {
        let sidecar_path = archive_path.with_file_name(format!(
            "{}.sha256",
            archive_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        let sidecar_bytes = fs::read_to_string(&sidecar_path);
        match sidecar_bytes {
            Ok(expected) => {
                let expected = expected.trim().to_string();
                if !expected.is_empty()
                    && let Err(e) =
                        HashVerifier::verify_integrity(archive_path, "sha256", &expected)
                {
                    let _ = fs::remove_file(archive_path);
                    return Err(e);
                }
            }
            Err(_) => {
                UserInterface::warning(&format!(
                    "Sidecar checksum not found for {}; transport integrity verification skipped",
                    archive_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ));
            }
        }
    }

    let pkg_stage = root.join(constants::PATH_STAGE).join(&meta.pkg_name);
    if pkg_stage.exists() {
        fs::remove_dir_all(&pkg_stage)?;
    }
    fs::create_dir_all(&pkg_stage)?;

    // Cross-package collisions are rejected later by the database
    // transaction before any live write, so upgrade/reinstall overwrites of
    // this package's own files are allowed.
    let extracted = Extractor::new(root).extract_zstd_archive(archive_path, &pkg_stage)?;
    let total_extracted = extracted.len();

    let files_to_install: Vec<PathBuf> = match filter {
        Some(f) => {
            crate::core::component::filter_files_by_components(&extracted, &meta.components, f)
        }
        None => extracted,
    };

    let skipped = total_extracted.saturating_sub(files_to_install.len());
    if skipped > 0 {
        UserInterface::info(&format!(
            "{}: {} files skipped by component filter",
            meta.pkg_name, skipped
        ));
    }

    // Per-file SHA-256 digests recorded at install time so integrity
    // verification later checks actual placed content instead of treating
    // the archive checksum as a file-level guarantee.
    let mut file_hashes = HashMap::new();
    for file in &files_to_install {
        let staged_file = pkg_stage.join(file);
        if let Ok(md) = fs::symlink_metadata(&staged_file)
            && !md.file_type().is_symlink()
            && md.is_file()
        {
            let hash = HashVerifier::calculate(&staged_file, "sha256")?;
            file_hashes.insert(file.to_string_lossy().into_owned(), hash);
        }
    }

    let mut meta = meta;
    // Record only the files actually staged (component-filtered) plus their
    // digests, so remove/upgrade/verify operate on real placed state.
    meta.files = files_to_install;
    meta.file_hashes = file_hashes;

    Ok(StagedPackage { meta })
}

/// Place a staged package into the target root. The first install is always
/// a complete install: the full payload is materialised. Every later update
/// is git-pull style: the new active generation is seeded from the currently
/// active tree (an instant rename, no data copy) and only the files that
/// actually changed are overlaid on top; unchanged files are never rewritten
/// and files no longer in the new payload are dropped. Returns
/// (changed, removed).
fn place_package(
    root_path: &Path,
    installed_root: &Path,
    staged: &StagedPackage,
    transaction: &mut DbTransaction<'_>,
    replaced_links: &mut Vec<(PathBuf, PathBuf)>,
    prev: Option<&PackageMetadata>,
) -> Result<(usize, usize)> {
    let name = &staged.meta.pkg_name;
    let pkg_stage = root_path.join(constants::PATH_STAGE).join(name);
    let pkg_active_new = installed_root.join(format!("{}.mcx-new", name));
    let pkg_active_old = installed_root.join(format!("{}.mcx-old", name));
    let pkg_active = installed_root.join(name);

    // Seed the new generation from the current one when it exists (first
    // install materialises a fresh tree instead); unchanged files are thus
    // carried over without being re-copied from the stage.
    let _ = fs::remove_dir_all(&pkg_active_new);
    let _ = fs::remove_dir_all(&pkg_active_old);
    match fs::rename(&pkg_active, &pkg_active_new) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(&pkg_active_new)?;
        }
        Err(e) => {
            return Err(anyhow!(
                "Failed to seed update generation for {}: {}",
                name,
                e
            ));
        }
    }

    let staged_hashes = &staged.meta.file_hashes;
    let mut changed = 0usize;
    let mut removed = 0usize;

    for file in &staged.meta.files {
        let src = pkg_stage.join(file);
        let Ok(src_meta) = fs::symlink_metadata(&src) else {
            continue;
        };

        // Content-identical to the previously installed file? Then it needs
        // no root write; the digest comparison only skips regular files that
        // carried a digest before, so symlinks and irregular files are always
        // refreshed.
        let key = file.to_string_lossy().into_owned();
        let unchanged = staged_hashes.get(&key).is_some()
            && matches!(prev, Some(p) if p.file_hashes.get(&key) == staged_hashes.get(&key));

        if src_meta.file_type().is_symlink() {
            // Recreate symlinks as symlinks instead of copying through them.
            let target = fs::read_link(&src)?;
            let dst_active = pkg_active_new.join(file);
            if let Some(parent) = dst_active.parent() {
                fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(&target, &dst_active)?;

            if unchanged {
                continue;
            }
            changed += 1;

            let dst = root_path.join(file);
            if dst.symlink_metadata().is_ok() {
                // Preserve prior destination state for rollback.
                if let Ok(md) = fs::symlink_metadata(&dst)
                    && md.file_type().is_symlink()
                {
                    replaced_links.push((dst.clone(), fs::read_link(&dst)?));
                } else {
                    transaction.backup_file(&dst)?;
                }
                fs::remove_file(&dst)?;
            }
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(&target, &dst)?;
            transaction.record_staged_file(file.clone())?;
            continue;
        }

        if !src_meta.is_file() {
            continue;
        } // directories appear implicitly

        let dst_active = pkg_active_new.join(file);
        if unchanged {
            // The generation was seeded from the previous active tree; a
            // regular file already present there is kept as-is. Anything
            // irregular (missing, a symlink, or a type change) is refreshed
            // from the stage.
            let refresh = match fs::symlink_metadata(&dst_active) {
                Ok(md) => md.file_type().is_symlink() || !md.is_file(),
                Err(_) => true,
            };
            if !refresh {
                continue;
            }
        }

        if let Some(parent) = dst_active.parent() {
            fs::create_dir_all(parent)?;
        }
        crate::core::transaction::atomic_copy(&src, &dst_active)?;

        if unchanged {
            continue;
        }
        changed += 1;

        let dst = root_path.join(file);
        if dst.symlink_metadata().is_ok() {
            // Preserve prior content of overwritten paths for rollback.
            transaction.backup_file(&dst)?;
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        crate::core::transaction::atomic_copy(&src, &dst)?;
        transaction.record_staged_file(file.clone())?;
    }

    // Files that existed before but are absent from the new payload are
    // removed from the live root and from the seeded generation.
    if let Some(prev_meta) = prev {
        for file in &prev_meta.files {
            if !staged.meta.files.contains(file) {
                let dst = root_path.join(file);
                if dst.symlink_metadata().is_ok() {
                    transaction.backup_file(&dst)?;
                    let _ = fs::remove_file(&dst);
                    removed += 1;
                }
                let gen_path = pkg_active_new.join(file);
                if gen_path.symlink_metadata().is_ok() {
                    let _ = fs::remove_file(&gen_path);
                }
            }
        }
    }

    // Promote the merged generation. Because it was seeded from the previous
    // tree, a failure can simply be retried without losing the prior state;
    // the transaction rollback restores the live root.
    match fs::rename(&pkg_active_new, &pkg_active) {
        Ok(()) => {}
        Err(e) => {
            return Err(anyhow!(
                "Failed to activate new generation for {}: {}",
                name,
                e
            ));
        }
    }
    let _ = fs::remove_dir_all(&pkg_active_old);

    // This package's staging copy is fully consumed.
    let _ = fs::remove_dir_all(&pkg_stage);

    Ok((changed, removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidecar_url_for() {
        let source = "https://packages.cudane.org/pool/x86_64/foo/foo-1.0.0.xcs";
        let url = sidecar_url_for(source);
        assert_eq!(
            url,
            "https://packages.cudane.org/pool/x86_64/foo/foo-1.0.0.xcs.sha256"
        );
    }

    #[test]
    fn test_sidecar_path_for() {
        let p = sidecar_path_for("foo", "1.0.0");
        assert_eq!(p, PathBuf::from("foo-1.0.0.xcs.sha256"));
    }

    #[test]
    fn test_sidecar_enqueued_when_archive_missing() {
        let cache_dir = tempfile::tempdir().unwrap();
        let meta = PackageMetadata {
            pkg_name: "foo".into(),
            version: "1.0.0".into(),
            license: "MIT".into(),
            source: "https://example.com/pool/x86_64/foo/foo-1.0.0.xcs".into(),
            checksum: crate::core::database::ChecksumData {
                kind: "sha256".into(),
                value: "abc".into(),
            },
            dependencies: vec![],
            files: vec![],
            provides: None,
            conflicts: None,
            architecture: "x86_64".into(),
            components: vec![],
            services: vec![],
            binaries: vec![],
            file_hashes: HashMap::new(),
            provenance: None,
        };

        let mut downloads: Vec<(String, PathBuf)> = Vec::new();
        let mut pending: Vec<(PackageMetadata, PathBuf)> = Vec::new();

        let archive_name = format!("{}-{}.xcs", meta.pkg_name, meta.version);
        let target_path = cache_dir.path().join(&archive_name);

        if !target_path.exists() {
            downloads.push((meta.source.clone(), target_path.clone()));
            downloads.push((
                sidecar_url_for(&meta.source),
                cache_dir
                    .path()
                    .join(sidecar_path_for(&meta.pkg_name, &meta.version)),
            ));
        }
        pending.push((meta.clone(), target_path));

        assert_eq!(downloads.len(), 2);
        assert_eq!(
            downloads[0].0,
            "https://example.com/pool/x86_64/foo/foo-1.0.0.xcs"
        );
        assert_eq!(
            downloads[1].0,
            "https://example.com/pool/x86_64/foo/foo-1.0.0.xcs.sha256"
        );
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_sidecar_not_enqueued_when_archive_cached() {
        let cache_dir = tempfile::tempdir().unwrap();
        let meta = PackageMetadata {
            pkg_name: "bar".into(),
            version: "2.0.0".into(),
            license: "MIT".into(),
            source: "https://example.com/pool/x86_64/bar/bar-2.0.0.xcs".into(),
            checksum: crate::core::database::ChecksumData {
                kind: "sha256".into(),
                value: "def".into(),
            },
            dependencies: vec![],
            files: vec![],
            provides: None,
            conflicts: None,
            architecture: "x86_64".into(),
            components: vec![],
            services: vec![],
            binaries: vec![],
            file_hashes: HashMap::new(),
            provenance: None,
        };

        let archive_name = format!("{}-{}.xcs", meta.pkg_name, meta.version);
        let target_path = cache_dir.path().join(&archive_name);
        fs::write(&target_path, b"cached").unwrap();

        let mut downloads: Vec<(String, PathBuf)> = Vec::new();
        let mut pending: Vec<(PackageMetadata, PathBuf)> = Vec::new();

        if !target_path.exists() {
            downloads.push((meta.source.clone(), target_path.clone()));
            downloads.push((
                sidecar_url_for(&meta.source),
                cache_dir
                    .path()
                    .join(sidecar_path_for(&meta.pkg_name, &meta.version)),
            ));
        }
        pending.push((meta.clone(), target_path));

        assert!(downloads.is_empty());
        assert_eq!(pending.len(), 1);
    }
}
