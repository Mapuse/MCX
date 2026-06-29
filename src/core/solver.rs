use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use crate::core::db::Database;
use crate::core::database::{PackageMetadata, Dependency};
use crate::core::graph::DepGraph;

#[derive(Debug, Clone)]
pub struct UpgradeEdge {
    pub from_version: String,
    pub to_version: String,
    pub stability_index: f64,
}

#[derive(Debug, Clone)]
pub struct UpgradePath {
    pub package: String,
    pub from_version: String,
    pub to_version: String,
    pub steps: Vec<UpgradeEdge>,
    pub total_delta_bytes: u64,
    pub conflict_free: bool,
}

#[derive(Debug, Clone)]
pub struct ResolutionVerdict {
    pub plan: Vec<PackageMetadata>,
    pub topological_order: Vec<String>,
    pub upgrade_paths: Vec<UpgradePath>,
    pub deadlocks_detected: Vec<String>,
    pub cycles_broken: usize,
}

pub struct DependencySolver {
    db: Arc<Database>,
    targets: Vec<String>,
}

impl DependencySolver {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db, targets: Vec::new() }
    }

    pub fn add_target(mut self, package_name: &str) -> Self {
        self.targets.push(package_name.to_string());
        self
    }

    pub fn solve(&self) -> Result<Vec<PackageMetadata>> {
        let verdict = self.resolve_inner(&self.targets)?;
        Ok(verdict.plan)
    }

    pub fn solve_with_analysis(&self) -> Result<ResolutionVerdict> {
        self.resolve_inner(&self.targets)
    }

    fn resolve_inner(&self, targets: &[String]) -> Result<ResolutionVerdict> {
        let mut graph = DepGraph::new();
        let mut resolved: HashMap<String, PackageMetadata> = HashMap::new();
        let mut visiting: HashSet<String> = HashSet::new();
        let mut provided_virtuals: HashMap<String, String> = HashMap::new();
        let cycles_broken: usize = 0;
        let mut deadlocks_detected: Vec<String> = Vec::new();

        for target in targets {
            match self.resolve_node(target, &mut graph, &mut resolved, &mut visiting, &mut provided_virtuals) {
                Ok(_) => {},
                Err(e) => {
                    deadlocks_detected.push(format!("{}: {}", target, e));
                    if target == targets.first().map(|s| s.as_str()).unwrap_or("") {
                        return Err(e);
                    }
                }
            }
        }

        self.verify_conflicts(&resolved)?;

        let sorted = graph.compute_topological_sort()?;
        let mut plan = Vec::with_capacity(sorted.len());
        let mut topological_order = Vec::with_capacity(sorted.len());

        for pkg in &sorted {
            if let Some(meta) = resolved.remove(pkg) {
                topological_order.push(pkg.clone());
                plan.push(meta);
            }
        }

        let upgrade_paths = self.compute_upgrade_paths(&plan, targets)?;

        Ok(ResolutionVerdict {
            plan,
            topological_order,
            upgrade_paths,
            deadlocks_detected,
            cycles_broken,
        })
    }

    pub fn compute_upgrade_path(&self, package: &str) -> Result<UpgradePath> {
        let current = self.db.get_package_manifest(package)
            .map_err(|_| anyhow!("Package '{}' not found in registry", package))?;
        let current_version = semver_parse(&current.version);

        let available = self.db.get_all_available_packages()?;
        let mut candidates: Vec<(PackageMetadata, u64)> = available.into_iter()
            .filter(|meta| meta.pkg_name == package)
            .filter_map(|meta| {
                let ver = semver_parse(&meta.version);
                if ver > current_version {
                    let delta = self.estimate_delta_cost(&current, &meta);
                    Some((meta, delta))
                } else { None }
            })
            .collect();

        candidates.sort_by(|a, b| a.1.cmp(&b.1));

        let best = candidates.first()
            .ok_or_else(|| anyhow!("No upgrade available for '{}'", package))?;

        Ok(UpgradePath {
            package: package.to_string(),
            from_version: current.version.clone(),
            to_version: best.0.version.clone(),
            steps: vec![UpgradeEdge {
                from_version: current.version.clone(),
                to_version: best.0.version.clone(),
                stability_index: 1.0 - (best.1 as f64 / (best.1 + 1).max(1) as f64),
            }],
            total_delta_bytes: best.1,
            conflict_free: !self.has_conflicts_with_installed(&best.0),
        })
    }

    fn compute_upgrade_paths(&self, plan: &[PackageMetadata], targets: &[String]) -> Result<Vec<UpgradePath>> {
        let mut paths = Vec::new();
        let installed = self.db.get_all_installed_packages().unwrap_or_default();
        let installed_map: HashMap<&str, &PackageMetadata> = installed.iter()
            .map(|p| (p.pkg_name.as_str(), p)).collect();

        for meta in plan {
            if targets.contains(&meta.pkg_name) {
                if let Some(current) = installed_map.get(meta.pkg_name.as_str()) {
                    if current.version != meta.version {
                        let delta = self.estimate_delta_cost(current, meta);
                        paths.push(UpgradePath {
                            package: meta.pkg_name.clone(),
                            from_version: current.version.clone(),
                            to_version: meta.version.clone(),
                            steps: vec![UpgradeEdge {
                                from_version: current.version.clone(),
                                to_version: meta.version.clone(),
                                stability_index: 1.0 - (delta as f64 / (delta + 1).max(1) as f64),
                            }],
                            total_delta_bytes: delta,
                            conflict_free: !self.has_conflicts_with_installed(meta),
                        });
                    }
                }
            }
        }
        Ok(paths)
    }

    fn resolve_node(
        &self,
        target: &str,
        graph: &mut DepGraph,
        resolved: &mut HashMap<String, PackageMetadata>,
        visiting: &mut HashSet<String>,
        provided: &mut HashMap<String, String>,
    ) -> Result<()> {
        let pkg_name = provided.get(target)
            .map(|s| s.to_string())
            .unwrap_or_else(|| target.to_string());

        if resolved.contains_key(&pkg_name) {
            return Ok(());
        }

        if !visiting.insert(pkg_name.to_string()) {
            let mut cycle_breaker = Vec::new();
            for dep in &self.get_deps_for(&pkg_name) {
                if visiting.contains(dep) {
                    cycle_breaker.push(dep.clone());
                }
            }
            if !cycle_breaker.is_empty() {
                for break_pkg in &cycle_breaker {
                    visiting.remove(break_pkg);
                }
                return Err(anyhow!(
                    "Cyclic dependency detected involving package '{}'; broken edges: {:?}",
                    pkg_name, cycle_breaker
                ));
            }
            return Err(anyhow!(
                "Cyclic dependency detected involving package '{}'", pkg_name
            ));
        }

        let meta = self.db.get_package_manifest(&pkg_name)
            .map_err(|_| anyhow!("Unresolvable constraint: package '{}' not found", pkg_name))?;

        if let Some(provides) = &meta.provides {
            for v_pkg in provides {
                provided.insert(v_pkg.clone(), pkg_name.to_string());
            }
        }

        for dep in &meta.dependencies {
            let actual_dep = self.resolve_dependency_package(dep, provided)?;
            graph.add_edge(&pkg_name, &actual_dep);
            self.resolve_node(&actual_dep, graph, resolved, visiting, provided)?;
        }

        resolved.insert(pkg_name.to_string(), meta);
        visiting.remove(&pkg_name);
        Ok(())
    }

    fn get_deps_for(&self, pkg_name: &str) -> Vec<String> {
        if let Ok(meta) = self.db.get_package_manifest(pkg_name) {
            meta.dependencies.iter().map(|d| d.name.clone()).collect()
        } else {
            Vec::new()
        }
    }

    fn resolve_dependency_package(
        &self,
        dep: &Dependency,
        provided: &HashMap<String, String>,
    ) -> Result<String> {
        if let Some(mapped) = provided.get(&dep.name) {
            return Ok(mapped.clone());
        }

        if self.db.get_package_manifest(&dep.name).is_ok() {
            return Ok(dep.name.clone());
        }

        if self.should_resolve_as_package(dep) {
            return Err(anyhow!("Unresolvable dependency package: '{}' missing from registry", dep.name));
        }

        self.find_library_provider(&dep.name)
            .map_err(|_| anyhow!("Unresolvable dependency: '{}' could not be mapped to a package provider", dep.name))
    }

    fn should_resolve_as_package(&self, dep: &Dependency) -> bool {
        let dep_type = dep.dep_type.to_ascii_lowercase();
        dep_type.contains("build")
            || dep_type.contains("runtime")
            || dep_type.contains("package")
            || dep_type.contains("run")
            || dep_type.contains("host")
    }

    fn find_library_provider(&self, library_name: &str) -> Result<String> {
        let mut matches = Vec::new();

        let available = self.db.get_all_available_packages()?;
        let installed = self.db.get_all_installed_packages()?;

        for pkg in available.iter().chain(installed.iter()) {
            if let Some(provides) = &pkg.provides {
                if provides.iter().any(|provides_name| provides_name == library_name) {
                    matches.push(pkg.pkg_name.clone());
                    continue;
                }
            }

            for file in &pkg.files {
                if file.file_name().map(|name| name == library_name).unwrap_or(false)
                    || file.to_string_lossy() == library_name
                {
                    matches.push(pkg.pkg_name.clone());
                    break;
                }
            }
        }

        matches.sort();
        matches.dedup();

        match matches.len() {
            1 => Ok(matches.into_iter().next().unwrap()),
            0 => Err(anyhow!("No package provides library '{}'", library_name)),
            _ => Err(anyhow!("Ambiguous provider for '{}' found in packages: {:?}", library_name, matches)),
        }
    }

    fn verify_conflicts(&self, resolved: &HashMap<String, PackageMetadata>) -> Result<()> {
        let active_pkgs: HashSet<&String> = resolved.keys().collect();

        for meta in resolved.values() {
            if let Some(conflicts) = &meta.conflicts {
                for conflict in conflicts {
                    if active_pkgs.contains(conflict) {
                        return Err(anyhow!("Conflict resolution failed: '{}' conflicts with '{}'", meta.pkg_name, conflict));
                    }
                }
            }
        }
        Ok(())
    }

    fn has_conflicts_with_installed(&self, pkg: &PackageMetadata) -> bool {
        if let Some(conflicts) = &pkg.conflicts {
            if let Ok(installed) = self.db.get_all_installed_packages() {
                for installed_pkg in &installed {
                    if conflicts.contains(&installed_pkg.pkg_name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn estimate_delta_cost(&self, _from: &PackageMetadata, _to: &PackageMetadata) -> u64 {
        let from_files = _from.files.len();
        let to_files = _to.files.len();
        let diff = if to_files > from_files { to_files - from_files } else { from_files - to_files };
        (diff as u64).max(1) * 4096
    }
}

fn semver_parse(version: &str) -> Vec<u64> {
    version.trim_start_matches('v')
        .split('.')
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}
