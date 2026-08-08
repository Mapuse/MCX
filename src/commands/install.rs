use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use futures::future::join_all;
use crate::core::solver::DependencySolver;
use crate::core::db::Database;
use crate::core::profiler::SystemProfile;
use crate::core::cgroup::CgroupController;
use crate::core::declarative::ProfileValidator;
use crate::core::security::SecurityMonitor;
use crate::core::plugin::{PluginManager, PluginHook, PluginEvent};
use crate::core::component::ComponentFilter;
use crate::network::download::Downloader;
use crate::archive::extract::Extractor;
use crate::archive::hash::HashVerifier;
use crate::core::constants;
use crate::utils::ui::UserInterface;
use rand::Rng;

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

    pub fn with_cgroup(mut self, mgr: CgroupController) -> Self { self.cgroup_mgr = mgr; self }
    pub fn with_security(mut self, mon: Arc<SecurityMonitor>) -> Self { self.security_mon = Some(mon); self }
    pub fn with_profile(mut self, path: PathBuf) -> Self { self.profile_path = Some(path); self }
    pub fn with_plugin_mgr(mut self, mgr: Arc<PluginManager>) -> Self { self.plugin_mgr = Some(mgr); self }
    pub fn with_component_filter(mut self, filter: ComponentFilter) -> Self { self.component_filter = Some(filter); self }

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

        let plan = solver.solve()?;
        for meta in &plan {
            if !crate::core::arch::package_matches_host(&meta.architecture) {
                return Err(anyhow!(
                    "Package '{}' architecture '{}' is not compatible with this host",
                    meta.pkg_name, meta.architecture
                ));
            }
        }
        let root_path = Path::new(&self.root);
        let cache_dir = root_path.join(constants::PATH_CACHE);
        fs::create_dir_all(&cache_dir)?;

        let downloader = Downloader::new();

        let mut pending: Vec<(crate::core::database::PackageMetadata, std::path::PathBuf)> = Vec::new();

        for meta in &plan {
            if self.db.is_package_installed(&meta.pkg_name)? {
                // Skip only when the installed version already satisfies the
                // resolved target. An older installed version must be
                // re-installed so `mcx install <pkg>` doubles as an upgrade.
                if let Ok(installed) = self.db.get_package_manifest(&meta.pkg_name)
                    && version_cmp(&installed.version, &meta.version) != std::cmp::Ordering::Less
                {
                    continue;
                }
            }
            let archive_name = format!("{}-{}.xcs", meta.pkg_name, meta.version);
            let target_path = cache_dir.join(&archive_name);
            pending.push((meta.clone(), target_path.clone()));

            if !target_path.exists() {
                let dl = downloader.clone();
                let url = meta.source.clone();
                let dest = target_path.clone();
                let results = join_all(vec![tokio::spawn(async move {
                    dl.package(&url, &dest).await?;
                    Ok::<(), anyhow::Error>(())
                })]).await;
                for result in results {
                    result.map_err(|e| anyhow!("Task join error: {}", e))??;
                }
            }
        }

        let use_parallel = sys_profile.cpu_count >= constants::CPU_THRESHOLD_LOW && sys_profile.available_ram_mb >= constants::RAM_THRESHOLD_MEDIUM_MB;

        let mut installed_files: std::collections::HashMap<String, Vec<PathBuf>> = std::collections::HashMap::new();

        if use_parallel {
            let mut handles = Vec::new();
            let filter_clone = self.component_filter.clone();
            for (meta, path) in &pending {
                let meta_clone = meta.clone();
                let path_clone = path.clone();
                let root = self.root.clone();
                let ext = Extractor::new(&self.root);
                let filter = filter_clone.clone();

                handles.push(tokio::task::spawn_blocking(move || -> Result<(String, Vec<PathBuf>)> {
                    if let Err(e) = HashVerifier::verify_integrity(&path_clone, &meta_clone.checksum.kind, &meta_clone.checksum.value) {
                        // Never keep a corrupt archive in the cache.
                        let _ = fs::remove_file(&path_clone);
                        return Err(e);
                    }
                    let root_path = Path::new(&root);
                    let stage_base = root_path.join(constants::PATH_STAGE);
                    let installed_root = root_path.join(constants::PATH_ACTIVE);

                    let pkg_stage = stage_base.join(&meta_clone.pkg_name);
                    if pkg_stage.exists() { fs::remove_dir_all(&pkg_stage)?; }
                    fs::create_dir_all(&pkg_stage)?;

                    // Extract directly into the per-package stage directory.
                    // Cross-package collisions are still rejected later by the
                    // database transaction, so upgrade/reinstall overwrites of
                    // this package's own files are allowed.
                    let extracted = ext.extract_zstd_archive(&path_clone, &pkg_stage)?;
                    let total_extracted = extracted.len();

                    let files_to_install: Vec<PathBuf> = if let Some(ref f) = filter {
                        crate::core::component::filter_files_by_components(
                            &extracted,
                            &meta_clone.components,
                            f,
                        )
                    } else {
                        extracted
                    };

                    let skipped = total_extracted.saturating_sub(files_to_install.len());
                    if skipped > 0 {
                        UserInterface::info(&format!("{}: {} files skipped by component filter",
                            meta_clone.pkg_name, skipped));
                    }

                    let pkg_active = installed_root.join(&meta_clone.pkg_name);
                    if pkg_active.exists() { fs::remove_dir_all(&pkg_active)?; }
                    fs::create_dir_all(&pkg_active)?;

                    for file in &files_to_install {
                        let src = pkg_stage.join(file);
                        let dst = root_path.join(file);
                        if src.is_file() {
                            atomic_copy(&src, &dst)?;
                        }
                        let dst_active = pkg_active.join(file);
                        if src.is_file() {
                            atomic_copy(&src, &dst_active)?;
                        }
                    }

                    if stage_base.exists() { let _ = fs::remove_dir_all(&stage_base); }
                    Ok((meta_clone.pkg_name, files_to_install))
                }));
            }

            for handle in handles {
                let (name, files) = handle.await.map_err(|e| anyhow!("Task failed: {}", e))??;
                installed_files.insert(name, files);
            }
        } else {
            let ext = Extractor::new(&self.root);
            let stage_dir = root_path.join(constants::PATH_STAGE);
            let installed_root = root_path.join(constants::PATH_ACTIVE);

            for (meta, path) in &pending {
                if let Err(e) = HashVerifier::verify_integrity(path, &meta.checksum.kind, &meta.checksum.value) {
                    let _ = fs::remove_file(path);
                    return Err(e);
                }

                let pkg_stage = stage_dir.join(&meta.pkg_name);
                if pkg_stage.exists() { fs::remove_dir_all(&pkg_stage)?; }
                fs::create_dir_all(&pkg_stage)?;

                let extracted = ext.extract_zstd_archive(path, &pkg_stage)?;
                let total_extracted = extracted.len();

                let files_to_install: Vec<PathBuf> = if let Some(ref filter) = self.component_filter {
                    crate::core::component::filter_files_by_components(
                        &extracted,
                        &meta.components,
                        filter,
                    )
                } else {
                    extracted
                };

                let skipped = total_extracted.saturating_sub(files_to_install.len());
                if skipped > 0 {
                    UserInterface::info(&format!("{}: {} files skipped by component filter",
                        meta.pkg_name, skipped));
                }

                let pkg_active = installed_root.join(&meta.pkg_name);
                if pkg_active.exists() { fs::remove_dir_all(&pkg_active)?; }
                fs::create_dir_all(&pkg_active)?;

                for file in &files_to_install {
                    let src = pkg_stage.join(file);
                    let dst = root_path.join(file);
                    if src.is_file() {
                        atomic_copy(&src, &dst)?;
                    }
                    let dst_active = pkg_active.join(file);
                    if src.is_file() {
                        atomic_copy(&src, &dst_active)?;
                    }
                }

                if stage_dir.exists() { let _ = fs::remove_dir_all(&stage_dir); }
                installed_files.insert(meta.pkg_name.clone(), files_to_install);
            }
        }

        // sandbox setup for each installed package
        for (meta, _) in &pending {
            // cgroup: enforce resource limits (best-effort, may fail without root)
            let _ = self.cgroup_mgr.enforce_resource_limits(&meta.pkg_name, constants::DEFAULT_CGROUP_MAX_MEMORY_MB, constants::DEFAULT_CGROUP_MAX_CPU_PERCENT);

            // security monitor: register package
            if let Some(ref mon) = self.security_mon {
                mon.register_package(&meta.pkg_name);
            }
        }

        // profile validation: detect drift between declared and actual state
        if let Some(ref profile_path) = self.profile_path
            && profile_path.exists() {
                match ProfileValidator::load_profile(profile_path) {
                    Ok(profile) => {
                        let current: Vec<String> = self.db.get_all_installed_packages()
                            .unwrap_or_default()
                            .iter()
                            .map(|p| p.pkg_name.clone())
                            .collect();
                        let (to_install, to_remove) = ProfileValidator::compile_profile_diff(&current, &profile.packages);
                        if !to_install.is_empty() || !to_remove.is_empty() {
                            UserInterface::profile(&format!("Drift: {} to install, {} to remove",
                                to_install.len(), to_remove.len()));
                        }
                    }
                    Err(e) => {
                        UserInterface::warning(&format!("Profile validation skipped: {}", e));
                    }
                }
            }

        let mut transaction = self.db.begin_transaction()?;
        for (meta, _) in &pending {
            let mut meta_reg = meta.clone();
            // Record only the files that were actually placed on the system so
            // a component-filtered install never has remove/upgrade clean up
            // files that were deliberately skipped.
            if let Some(files) = installed_files.get(&meta.pkg_name) {
                meta_reg.files = files.clone();
            }
            transaction.register_package_placement(&meta_reg)?;
        }
        transaction.commit()?;

        for (meta, _) in &pending {
            for svc in meta.all_services() {
                let svc_cmd = crate::commands::service::ServiceCommand::new(
                    self.root.clone(),
                    Arc::clone(&self.db),
                );
                if let Err(e) = svc_cmd.register_service(svc) {
                    UserInterface::warning(&format!("Failed to register service '{}' for {}: {}",
                        svc.name, meta.pkg_name, e));
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

fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn parse(s: &str) -> Vec<u64> {
        s.trim_start_matches('v')
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    }
    parse(a).cmp(&parse(b))
}

fn atomic_copy(src: &Path, dst: &Path) -> Result<()> {
    let parent = dst.parent()
        .ok_or_else(|| anyhow!("No parent directory for {:?}", dst))?;
    fs::create_dir_all(parent)?;
    let name = dst.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let tmp = parent.join(format!(".{}.mcx.{}.tmp", name, rand::rng().random_range(1_000_000_000u64..10_000_000_000)));
    fs::copy(src, &tmp)?;
    fs::rename(&tmp, dst)?;
    Ok(())
}

