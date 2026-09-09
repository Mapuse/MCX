use crate::core::cgroup::CgroupController;
use crate::core::component::ComponentFilter;
use crate::core::constants;
use crate::core::db::Database;
use crate::core::plugin::{PluginEvent, PluginHook, PluginManager};
use crate::core::security::SecurityMonitor;
use crate::utils::ui::UserInterface;
use anyhow::{Context, Result, anyhow};
use std::cmp::Reverse;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct RemoveCommand {
    root: PathBuf,
    db: Arc<Database>,
    plugin_mgr: Option<Arc<PluginManager>>,
    component_filter: Option<ComponentFilter>,
}

impl RemoveCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self {
            root: PathBuf::from(root),
            db,
            plugin_mgr: None,
            component_filter: None,
        }
    }

    pub fn with_plugin_mgr(mut self, mgr: Arc<PluginManager>) -> Self {
        self.plugin_mgr = Some(mgr);
        self
    }
    pub fn with_component_filter(mut self, filter: Option<ComponentFilter>) -> Self {
        self.component_filter = filter;
        self
    }

    pub fn execute(
        &self,
        packages: &[String],
        cgroup_mgr: &CgroupController,
        security_mon: &SecurityMonitor,
    ) -> Result<()> {
        if packages.is_empty() {
            return Err(anyhow!(
                "No target packages specified for removal transaction"
            ));
        }

        self.fire_hooks(PluginHook::PreRemove, packages);

        if let Some(ref filter) = self.component_filter
            && !filter.include.is_empty()
        {
            let result = self.execute_partial_removal(packages, filter, cgroup_mgr, security_mon);
            self.fire_hooks(PluginHook::PostRemove, packages);
            return result;
        }

        let mut transaction = self.db.begin_transaction()?;

        let orphans = self.analysis(packages)?;

        let all_targets: Vec<String> = packages
            .iter()
            .chain(orphans.iter())
            .map(|s| s.to_string())
            .collect();

        let mut residue_paths: Vec<PathBuf> = Vec::new();
        let mut services_to_unregister: Vec<String> = Vec::new();
        let active_dir = self.root.join(constants::PATH_ACTIVE);

        // Stage the whole removal inside the transaction first: back up and
        // clear each package's ACTIVE mirror (rollback-safe via the backups),
        // collect live residue paths and drop the registry entries. Live
        // filesystem deletions happen only AFTER the registry commit, so a
        // failure can never leave the database owning deleted files.
        for pkg in &all_targets {
            if !self.db.is_package_installed(pkg)? {
                continue;
            }

            let manifest = self
                .db
                .get_package_manifest(pkg)
                .with_context(|| format!("Failed to retrieve manifest for {}", pkg))?;

            let pkg_active = active_dir.join(pkg);
            if pkg_active.exists() {
                transaction.backup_file(&pkg_active)?;
                fs::remove_dir_all(&pkg_active)
                    .with_context(|| format!("Failed to purge package root: {:?}", pkg_active))?;
            }

            for file_path in &manifest.files {
                residue_paths.push(self.root.join(file_path));
            }

            for svc in manifest.all_services() {
                services_to_unregister.push(svc.name.clone());
            }

            transaction.stage_package_removal(pkg)?;
        }

        let shared_files = self.compute_non_orphaned_files(&all_targets);

        transaction.commit()?;

        // Registry no longer claims these files — now remove them from disk.
        residue_paths.sort_by_key(|a| Reverse(a.components().count()));

        for file_path in &residue_paths {
            let absolute_target = self
                .root
                .join(file_path.strip_prefix(&self.root).unwrap_or(file_path));
            if !absolute_target.exists() {
                continue;
            }
            if shared_files.contains(&absolute_target) {
                continue;
            }

            if absolute_target.is_dir() {
                if let Ok(mut entries) = fs::read_dir(&absolute_target)
                    && entries.next().is_none()
                {
                    let _ = fs::remove_dir(&absolute_target);
                }
            } else if let Err(e) = fs::remove_file(&absolute_target) {
                UserInterface::warning(&format!("Failed to remove {:?}: {}", absolute_target, e));
            }
        }

        self.scour_system_residue(&all_targets)?;
        self.cleanup_dangling_symlinks(&self.root, &all_targets)?;

        // sandbox cleanup for removed packages
        for pkg in &all_targets {
            if let Err(e) = cgroup_mgr.remove_resource_limits(pkg) {
                UserInterface::warning(&format!(
                    "Failed to remove resource limits for {}: {}",
                    pkg, e
                ));
            }
            security_mon.unregister_package(pkg);
        }

        let svc_cmd = crate::commands::service::ServiceCommand::new(
            self.root.to_string_lossy().to_string(),
            Arc::clone(&self.db),
        );
        for svc_name in &services_to_unregister {
            let _ = svc_cmd.unregister_service(svc_name);
        }

        self.fire_hooks(PluginHook::PostRemove, packages);

        Ok(())
    }

    fn fire_hooks(&self, hook: PluginHook, packages: &[String]) {
        if let Some(ref mgr) = self.plugin_mgr {
            for pkg in packages {
                let event = PluginEvent {
                    hook: hook.as_str().to_string(),
                    package: Some(pkg.clone()),
                    root: self.root.to_string_lossy().to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                mgr.fire_hook(hook, &event);
            }
        }
    }

    fn execute_partial_removal(
        &self,
        packages: &[String],
        filter: &ComponentFilter,
        cgroup_mgr: &CgroupController,
        security_mon: &SecurityMonitor,
    ) -> Result<()> {
        let active_dir = self.root.join(constants::PATH_ACTIVE);

        for pkg_name in packages {
            if !self.db.is_package_installed(pkg_name)? {
                UserInterface::warning(&format!("'{}' is not installed", pkg_name));
                continue;
            }

            let manifest = self
                .db
                .get_package_manifest(pkg_name)
                .with_context(|| format!("Failed to retrieve manifest for {}", pkg_name))?;

            let mut removed_count = 0usize;
            let mut removed_component_files: Vec<PathBuf> = Vec::new();
            let mut remaining_files: Vec<PathBuf> = Vec::new();
            let mut kept_components = Vec::new();

            for component in &manifest.components {
                if filter.include.contains(&component.name) {
                    UserInterface::info(&format!(
                        "Removing component '{}' from {}",
                        component.name, pkg_name
                    ));
                    removed_component_files.extend(component.files.iter().cloned());
                } else {
                    remaining_files.extend(component.files.iter().cloned());
                    kept_components.push(component.clone());
                }
            }

            if kept_components.is_empty() && !manifest.components.is_empty() {
                UserInterface::info(&format!(
                    "All components removed from '{}'; performing full removal",
                    pkg_name
                ));
                let mut tx = self.db.begin_transaction()?;
                let pkg_active = active_dir.join(pkg_name);
                if pkg_active.exists() {
                    tx.backup_file(&pkg_active)?;
                    fs::remove_dir_all(&pkg_active).with_context(|| {
                        format!("Failed to purge package root: {:?}", pkg_active)
                    })?;
                }
                tx.stage_package_removal(pkg_name)?;
                tx.commit()?;
            } else {
                // Persist the pruned manifest BEFORE deleting files so the
                // registry never claims ownership of paths that are about to
                // disappear from disk.
                let mut updated = manifest.clone();
                updated.components = kept_components;
                updated.files = remaining_files.clone();
                updated
                    .file_hashes
                    .retain(|key, _| remaining_files.iter().any(|f| f.to_string_lossy() == *key));

                let mut tx = self.db.begin_transaction()?;
                tx.register_package_placement(&updated)?;
                tx.commit()?;

                let pkg_active = active_dir.join(pkg_name);
                for file in &removed_component_files {
                    let abs = self.root.join(file);
                    if abs.exists() {
                        match fs::remove_file(&abs) {
                            Ok(()) => removed_count += 1,
                            Err(e) => UserInterface::warning(&format!(
                                "Failed to remove {:?}: {}",
                                abs, e
                            )),
                        }
                    }
                    let file_in_active = pkg_active.join(file);
                    if file_in_active.exists() {
                        let _ = fs::remove_file(&file_in_active);
                    }
                }

                // Drop now-empty directories left behind in the active mirror.
                if pkg_active.exists()
                    && fs::read_dir(&pkg_active)
                        .map(|mut d| d.next().is_none())
                        .unwrap_or(false)
                {
                    let _ = fs::remove_dir(&pkg_active);
                }
            }

            UserInterface::success(&format!(
                "Removed {} file(s) from '{}'",
                removed_count, pkg_name
            ));
        }

        self.cleanup_dangling_symlinks(&self.root, packages)?;

        for pkg_name in packages {
            if let Ok(manifest) = self.db.get_package_manifest(pkg_name) {
                let svc_cmd = crate::commands::service::ServiceCommand::new(
                    self.root.to_string_lossy().to_string(),
                    Arc::clone(&self.db),
                );
                for svc in manifest.all_services() {
                    let _ = svc_cmd.unregister_service(&svc.name);
                }
            }
            if let Err(e) = cgroup_mgr.remove_resource_limits(pkg_name) {
                UserInterface::warning(&format!(
                    "Failed to remove resource limits for {}: {}",
                    pkg_name, e
                ));
            }
            security_mon.unregister_package(pkg_name);
        }

        Ok(())
    }

    fn analysis(&self, targets: &[String]) -> Result<Vec<String>> {
        let all_installed = self.db.get_all_installed_packages()?;
        let target_set: HashSet<&str> = targets.iter().map(|s| s.as_str()).collect();

        let mut reverse_deps: std::collections::HashMap<&str, Vec<&str>> =
            std::collections::HashMap::new();
        for pkg in &all_installed {
            for dep in &pkg.dependencies {
                reverse_deps
                    .entry(&dep.name)
                    .or_default()
                    .push(&pkg.pkg_name);
            }
        }

        let mut orphans = Vec::new();
        let mut queue: VecDeque<&str> = VecDeque::new();

        for pkg in &all_installed {
            if target_set.contains(pkg.pkg_name.as_str()) {
                continue;
            }
            let rd = reverse_deps
                .get(pkg.pkg_name.as_str())
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let has_non_target_ref = rd.iter().any(|r| !target_set.contains(r));
            if !has_non_target_ref && !rd.is_empty() {
                queue.push_back(&pkg.pkg_name);
            }
        }

        let mut visited: HashSet<&str> = target_set.iter().copied().collect();
        while let Some(candidate) = queue.pop_front() {
            if !visited.insert(candidate) {
                continue;
            }
            if !target_set.contains(candidate) {
                orphans.push(candidate.to_string());
                if let Some(deps) = reverse_deps.get(candidate) {
                    for dep in deps {
                        if !visited.contains(dep) {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        Ok(orphans)
    }

    fn scour_system_residue(&self, removed: &[String]) -> Result<()> {
        const RESIDUE_SUFFIXES: &[&str] = &["log", "tmp", "pid", "cache"];

        let config_dirs = vec![
            self.root.join(constants::PATH_ETC_MCX),
            self.root.join(constants::PATH_LIB_MCX),
            self.root.join(constants::PATH_TMP),
            self.root.join(constants::PATH_CACHE),
        ];

        let removed_set: HashSet<&str> = removed.iter().map(|s| s.as_str()).collect();

        for dir in &config_dirs {
            if !dir.exists() {
                continue;
            }
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy();
                        for pkg in &removed_set {
                            let is_target = name_str == *pkg
                                || name_str
                                    .strip_prefix(*pkg)
                                    .and_then(|rest| rest.strip_prefix('.'))
                                    .map(|suffix| RESIDUE_SUFFIXES.contains(&suffix))
                                    .unwrap_or(false);
                            if is_target {
                                let result = if path.is_dir() {
                                    fs::remove_dir_all(&path)
                                } else {
                                    fs::remove_file(&path)
                                };
                                if let Err(e) = result {
                                    UserInterface::warning(&format!(
                                        "Failed to remove system residue {:?}: {}",
                                        path, e
                                    ));
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn compute_non_orphaned_files(&self, removed_packages: &[String]) -> HashSet<PathBuf> {
        let mut shared = HashSet::new();
        if let Ok(all) = self.db.get_all_installed_packages() {
            let removed_set: HashSet<&str> = removed_packages.iter().map(|s| s.as_str()).collect();
            for pkg in &all {
                if removed_set.contains(pkg.pkg_name.as_str()) {
                    continue;
                }
                for f in &pkg.files {
                    let full = self.root.join(f);
                    shared.insert(full);
                }
            }
        }
        shared
    }

    fn cleanup_dangling_symlinks(&self, root: &Path, packages: &[String]) -> Result<()> {
        for pkg in packages {
            let pkg_active = root.join(constants::PATH_ACTIVE).join(pkg);
            Self::remove_dangling_symlinks_recursive(&pkg_active);

            if let Ok(manifest) = self.db.get_package_manifest(pkg) {
                for file in &manifest.files {
                    let full = root.join(file);
                    if full.is_symlink()
                        && !full.exists()
                        && let Err(e) = fs::remove_file(&full)
                    {
                        UserInterface::warning(&format!(
                            "Failed to remove dangling symlink {:?}: {}",
                            full, e
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn remove_dangling_symlinks_recursive(dir: &Path) {
        if !dir.exists() {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() {
                    if !path.exists() {
                        let _ = fs::remove_file(&path);
                    }
                } else if path.is_dir() {
                    Self::remove_dangling_symlinks_recursive(&path);
                }
            }
        }
    }
}
