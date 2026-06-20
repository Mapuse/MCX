use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use crate::core::db::Database;
use crate::core::database::{PackageMetadata, Dependency};
use crate::core::graph::DepGraph;

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
        let mut graph = DepGraph::new();
        let mut resolved = HashMap::new();
        let mut visiting = HashSet::new();
        let mut provided_virtuals = HashMap::new();

        for target in &self.targets {
            self.resolve_node(target, &mut graph, &mut resolved, &mut visiting, &mut provided_virtuals)?;
        }

        self.verify_conflicts(&resolved)?;

        let sorted = graph.compute_topological_sort()?;
        let mut plan = Vec::with_capacity(sorted.len());
        
        for pkg in sorted {
            if let Some(meta) = resolved.remove(&pkg) {
                plan.push(meta);
            }
        }
        
        Ok(plan)
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
            return Err(anyhow!(
                "Dependency Hell Alert: Cyclic dependency detected involving package '{}'!", 
                pkg_name
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
}