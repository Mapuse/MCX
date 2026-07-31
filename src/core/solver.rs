use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use memmap2::Mmap;
use sha2::{Sha256, Digest};
use crate::core::constants;
use crate::core::arch::package_matches_host;
use crate::core::database::{Database, PackageMetadata, Dependency};
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
        let mut deadlocks_detected: Vec<String> = Vec::new();
        let mut cycles_broken: usize = 0;

        let index = self.build_library_index()?;

        for target in targets {
            match self.resolve_node(
                target, &mut graph, &mut resolved, &mut visiting,
                &mut provided_virtuals, &index, &mut cycles_broken,
            ) {
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

        let sorted = graph.compute_topological_sort().unwrap_or_else(|_| {
            let mut fallback: Vec<String> = resolved.keys().cloned().collect();
            fallback.sort();
            fallback
        });

        let mut plan = Vec::with_capacity(sorted.len());
        let mut topological_order = Vec::with_capacity(sorted.len());

        for pkg in &sorted {
            if let Some(meta) = resolved.remove(pkg) {
                topological_order.push(pkg.clone());
                plan.push(meta);
            }
        }
        for (_k, meta) in resolved.drain() {
            topological_order.push(meta.pkg_name.clone());
            plan.push(meta);
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

    fn build_library_index(&self) -> Result<LibraryIndex> {
        let mut lib_to_pkgs: HashMap<String, Vec<String>> = HashMap::new();

        let all_installed = self.db.get_all_installed_packages().unwrap_or_default();
        let all_available = self.db.get_all_available_packages().unwrap_or_default();

        for meta in all_installed.iter().chain(all_available.iter()) {
            if !package_matches_host(&meta.architecture) {
                continue;
            }
            if let Some(provides) = &meta.provides {
                for prov in provides {
                    let normalized = normalize_library(prov);
                    for n in normalized {
                        lib_to_pkgs.entry(n).or_default().push(meta.pkg_name.clone());
                    }
                }
            }
            for file in &meta.files {
                if let Some(fname) = file.file_name().and_then(|n| n.to_str())
                    && (fname.contains(".so") || fname.ends_with(".dll") || fname.ends_with(".dylib") || fname.ends_with(".a")) {
                        let normalized = normalize_library(fname);
                        for n in normalized {
                            lib_to_pkgs.entry(n).or_default().push(meta.pkg_name.clone());
                        }
                    }
            }
        }

        for (_lib, pkgs) in lib_to_pkgs.iter_mut() {
            pkgs.sort();
            pkgs.dedup();
        }

        Ok(LibraryIndex { lib_to_pkgs })
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_node(
        &self,
        target: &str,
        graph: &mut DepGraph,
        resolved: &mut HashMap<String, PackageMetadata>,
        visiting: &mut HashSet<String>,
        provided: &mut HashMap<String, String>,
        index: &LibraryIndex,
        cycles_broken: &mut usize,
    ) -> Result<()> {
        let pkg_name = provided.get(target)
            .cloned()
            .unwrap_or_else(|| target.to_string());

        if resolved.contains_key(&pkg_name) {
            return Ok(());
        }

        if !visiting.insert(pkg_name.clone()) {
            if self.break_cycle(&pkg_name, visiting, graph, cycles_broken) {
                return Ok(());
            }
            return Err(anyhow!(
                "Cyclic dependency detected involving '{}'", pkg_name
            ));
        }

        let meta = self.db.get_package_manifest(&pkg_name)
            .map_err(|_| anyhow!("Package '{}' not found in registry", pkg_name))?;

        if let Some(provides) = &meta.provides {
            for v_pkg in provides {
                provided.insert(v_pkg.clone(), pkg_name.clone());
            }
        }

        let resolved_deps = self.resolve_dependencies(&meta, index, provided)?;

        for dep in &resolved_deps {
            if dep.name == pkg_name { continue; }
            graph.add_edge(&pkg_name, &dep.name);
            self.resolve_node(&dep.name, graph, resolved, visiting, provided, index, cycles_broken)?;
        }

        let consolidated = self.consolidate_dependencies(resolved_deps);

        let mut meta = meta;
        meta.dependencies = consolidated;
        resolved.insert(pkg_name.clone(), meta);
        visiting.remove(&pkg_name);
        Ok(())
    }

    fn break_cycle(
        &self,
        pkg_name: &str,
        visiting: &mut HashSet<String>,
        _graph: &mut DepGraph,
        cycles_broken: &mut usize,
    ) -> bool {
        visiting.remove(pkg_name);
        *cycles_broken += 1;
        true
    }

    fn resolve_dependencies(
        &self,
        meta: &PackageMetadata,
        index: &LibraryIndex,
        provided: &HashMap<String, String>,
    ) -> Result<Vec<Dependency>> {
        let mut results: Vec<Dependency> = Vec::new();

        for dep in &meta.dependencies {
            if let Some(mapped) = provided.get(&dep.name) {
                results.push(Dependency {
                    name: mapped.clone(),
                    dep_type: dep.dep_type.clone(),
                    libraries: dep.libraries.clone(),
                });
                continue;
            }

            if self.db.get_package_manifest(&dep.name).is_ok() {
                results.push(dep.clone());
                continue;
            }

            if let Ok(resolved) = self.resolve_library_to_package(&dep.name, index) {
                results.push(Dependency {
                    name: resolved,
                    dep_type: dep.dep_type.clone(),
                    libraries: None,
                });
            } else {
                results.push(dep.clone());
            }
        }

        Ok(results)
    }

    fn resolve_library_to_package(&self, library: &str, index: &LibraryIndex) -> Result<String> {
        let normalized = normalize_library(library);
        let mut matches: Vec<String> = Vec::new();

        for candidate in &normalized {
            if let Some(pkgs) = index.lib_to_pkgs.get(candidate) {
                for pkg in pkgs {
                    if !matches.contains(pkg) {
                        matches.push(pkg.clone());
                    }
                }
            }
        }

        if let Ok(installed) = self.db.get_all_installed_packages() {
            for pkg in &installed {
                for file in &pkg.files {
                    if let Some(fname) = file.file_name().and_then(|n| n.to_str()) {
                        let fname_normalized = normalize_library(fname);
                        let lib_normalized = normalize_library(library);
                        if (fname_normalized.iter().any(|fn_item| lib_normalized.contains(fn_item))
                            || lib_normalized.iter().any(|ln_item| fname_normalized.contains(ln_item)))
                            && !matches.contains(&pkg.pkg_name) {
                                matches.push(pkg.pkg_name.clone());
                            }
                    }
                }
            }
        }

        match matches.len() {
            0 => Err(anyhow!("No package provides library '{}'", library)),
            1 => Ok(matches.into_iter().next().expect("single match present")),
            _ => {
                let installed: HashSet<String> = self.db.get_all_installed_packages()
                    .unwrap_or_default().into_iter().map(|p| p.pkg_name).collect();
                if let Some(preferred) = matches.iter().find(|m| installed.contains(*m)) {
                    Ok(preferred.clone())
                } else {
                    matches.sort();
                    Ok(matches.into_iter().next().expect("non-empty match list"))
                }
            }
        }
    }

    fn consolidate_dependencies(&self, deps: Vec<Dependency>) -> Vec<Dependency> {
        let mut pkg_deps: HashMap<String, (String, Vec<String>)> = HashMap::new();
        let mut lone_deps: Vec<Dependency> = Vec::new();

        for dep in deps {
            let is_library = dep.dep_type.starts_with("Library");
            if is_library {
                if let Some(libs) = &dep.libraries {
                    let entry = pkg_deps.entry(dep.name.clone()).or_insert_with(|| (dep.dep_type.clone(), Vec::new()));
                    for lib in libs {
                        if !entry.1.contains(lib) {
                            entry.1.push(lib.clone());
                        }
                    }
                } else {
                    pkg_deps.entry(dep.name.clone()).or_insert_with(|| (dep.dep_type.clone(), Vec::new()));
                }
            } else {
                lone_deps.push(dep);
            }
        }

        let mut consolidated: Vec<Dependency> = Vec::new();
        for (name, (dep_type, libs)) in pkg_deps {
            if libs.len() >= 2 {
                let mut sorted_libs = libs.clone();
                sorted_libs.sort();
                sorted_libs.dedup();
                consolidated.push(Dependency {
                    name,
                    dep_type,
                    libraries: Some(sorted_libs),
                });
            } else {
                consolidated.push(Dependency {
                    name,
                    dep_type,
                    libraries: None,
                });
            }
        }

        consolidated.extend(lone_deps);
        consolidated.sort_by(|a, b| a.name.cmp(&b.name));
        consolidated
    }

    fn verify_conflicts(&self, resolved: &HashMap<String, PackageMetadata>) -> Result<()> {
        let active_pkgs: HashSet<&String> = resolved.keys().collect();

        for meta in resolved.values() {
            if let Some(conflicts) = &meta.conflicts {
                for conflict in conflicts {
                    if active_pkgs.contains(conflict) {
                        return Err(anyhow!(
                            "Conflict: '{}' conflicts with '{}'", meta.pkg_name, conflict
                        ));
                    }
                }
            }

            let installed = self.db.get_all_installed_packages().unwrap_or_default();
            for installed_pkg in &installed {
                if !active_pkgs.contains(&installed_pkg.pkg_name)
                    && let Some(conflicts) = &installed_pkg.conflicts
                        && conflicts.contains(&meta.pkg_name) {
                            return Err(anyhow!(
                                "Conflict: installed '{}' conflicts with '{}'",
                                installed_pkg.pkg_name, meta.pkg_name
                            ));
                        }
            }
        }

        Ok(())
    }

    pub fn compute_upgrade_path(&self, package: &str) -> Result<UpgradePath> {
        let current = self.db.get_package_manifest(package)
            .map_err(|_| anyhow!("Package '{}' not found", package))?;
        let current_ver = semver_parse(&current.version);

        let available = self.db.get_all_available_packages()?;
        let mut candidates: Vec<PackageMetadata> = available.into_iter()
            .filter(|meta| meta.pkg_name == package && semver_parse(&meta.version) > current_ver)
            .collect();

        candidates.sort_by(|a, b| semver_parse(&b.version).cmp(&semver_parse(&a.version)));

        let best = candidates.into_iter().next()
            .ok_or_else(|| anyhow!("No upgrade available for '{}'", package))?;

        let conflict_free = !self.has_conflicts_with_installed(&best);

        Ok(UpgradePath {
            package: package.to_string(),
            from_version: current.version.clone(),
            to_version: best.version.clone(),
            steps: vec![UpgradeEdge {
                from_version: current.version.clone(),
                to_version: best.version.clone(),
                stability_index: 1.0,
            }],
            conflict_free,
        })
    }

    fn compute_upgrade_paths(&self, plan: &[PackageMetadata], targets: &[String]) -> Result<Vec<UpgradePath>> {
        let mut paths = Vec::new();
        let installed = self.db.get_all_installed_packages().unwrap_or_default();
        let installed_map: HashMap<&str, &PackageMetadata> = installed.iter()
            .map(|p| (p.pkg_name.as_str(), p)).collect();

        for meta in plan {
            if targets.contains(&meta.pkg_name)
                && let Some(current) = installed_map.get(meta.pkg_name.as_str())
                    && current.version != meta.version {
                        paths.push(UpgradePath {
                            package: meta.pkg_name.clone(),
                            from_version: current.version.clone(),
                            to_version: meta.version.clone(),
                            steps: vec![UpgradeEdge {
                                from_version: current.version.clone(),
                                to_version: meta.version.clone(),
                                stability_index: 1.0,
                            }],
                            conflict_free: !self.has_conflicts_with_installed(meta),
                        });
                    }
        }
        Ok(paths)
    }

    fn has_conflicts_with_installed(&self, pkg: &PackageMetadata) -> bool {
        if let Some(conflicts) = &pkg.conflicts
            && let Ok(installed) = self.db.get_all_installed_packages() {
                for installed_pkg in &installed {
                    if conflicts.contains(&installed_pkg.pkg_name) {
                        return true;
                    }
                }
            }
        false
    }

    pub fn scan_installed_dependencies(&self, pkg_name: &str) -> Result<Vec<Dependency>> {
        let meta = self.db.get_package_manifest(pkg_name)?;
        let active_dir = PathBuf::from(constants::PATH_ACTIVE).join(pkg_name);

        let index = self.build_library_index()?;

        let mut all_deps: Vec<Dependency> = Vec::new();
        for dep in &meta.dependencies {
            all_deps.push(dep.clone());
        }

        if active_dir.exists() {
            let elf_deps = self.scan_directory_elf_deps(&active_dir, &index)?;
            for dep in elf_deps {
                if !all_deps.iter().any(|d| d.name == dep.name) {
                    all_deps.push(dep);
                }
            }
        }

        let consolidated = self.consolidate_dependencies(all_deps);
        Ok(consolidated)
    }

    fn scan_directory_elf_deps(&self, dir: &Path, index: &LibraryIndex) -> Result<Vec<Dependency>> {
        let mut deps_map: HashMap<String, HashSet<String>> = HashMap::new();
        let mut pkg_libs: HashMap<String, Vec<String>> = HashMap::new();

        let elf_files = self.find_elf_binaries(dir);

        for elf_path in &elf_files {
            let needed = read_elf_needed(elf_path)?;
            let dlopen = scan_elf_strings(elf_path)?;

            for lib in needed.iter().chain(dlopen.iter()) {
                if is_core_system_lib(lib) { continue; }

                let normalized = normalize_library(lib);

                let mut resolved = false;
                for candidate in &normalized {
                    if let Some(pkgs) = index.lib_to_pkgs.get(candidate) {
                        for pkg_name in pkgs {
                            if pkg_name == "_self" { continue; }
                            deps_map.entry(pkg_name.clone()).or_default().insert(format!("Library ({})", lib));
                            pkg_libs.entry(pkg_name.clone()).or_default().push(lib.clone());
                            resolved = true;
                        }
                    }
                }

                if !resolved {
                    deps_map.entry(lib.clone()).or_default().insert("Library".to_string());
                }
            }
        }

        let mut results = Vec::new();
        for (name, types) in deps_map {
            let mut types_vec: Vec<String> = types.into_iter().collect();
            types_vec.sort();
            let libraries = pkg_libs.get(&name).and_then(|libs| {
                if libs.len() >= 2 {
                    let mut sorted = libs.clone();
                    sorted.sort();
                    sorted.dedup();
                    Some(sorted)
                } else { None }
            });
            results.push(Dependency {
                name,
                dep_type: types_vec.join(" & "),
                libraries,
            });
        }

        results.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(results)
    }

    fn find_elf_binaries(&self, dir: &Path) -> Vec<PathBuf> {
        let mut results = Vec::new();
        let mut stack = vec![dir.to_path_buf()];

        while let Some(current) = stack.pop() {
            if let Ok(entries) = fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.is_file()
                        && is_elf_file(&path) {
                            results.push(path);
                        }
                }
            }
        }

        results
    }
}

fn is_elf_file(path: &Path) -> bool {
    let mut buf = [0u8; 4];
    if let Ok(mut f) = fs::File::open(path)
        && f.read_exact(&mut buf).is_ok() {
            return buf == constants::ELF_MAGIC;
        }
    false
}

fn read_elf_needed(path: &Path) -> Result<Vec<String>> {
    let file = fs::File::open(path)
        .map_err(|e| anyhow!("Failed to open {}: {}", path.display(), e))?;
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| anyhow!("Failed to mmap {}: {}", path.display(), e))?;
    let data = &mmap[..];

    if data.len() < constants::ELF_MIN_HEADER_SIZE {
        return Ok(Vec::new());
    }

    // e_ident[EI_CLASS] lives at offset 4; this parser only handles ELFCLASS64.
    if data.get(4) != Some(&constants::ELFCLASS64) {
        return Ok(Vec::new());
    }

    let phoff = read_u64(data.get(constants::ELF64_PHOFF_RANGE).unwrap_or(&[]));
    let phentsize = read_u16(data.get(constants::ELF64_PHENTSIZE_RANGE).unwrap_or(&[]));
    let phnum = read_u16(data.get(constants::ELF64_PHNUM_RANGE).unwrap_or(&[]));

    let mut dyn_vaddr: Option<u64> = None;
    let mut dyn_size: Option<u64> = None;

    for i in 0..phnum as u64 {
        let offset = phoff.saturating_add(i.saturating_mul(phentsize as u64));
        let off = offset as usize;
        if off.saturating_add(phentsize as usize) > data.len() { break; }

        let p_type = read_u32(data.get(off..off.saturating_add(4)).unwrap_or(&[]));
        let p_vaddr = read_u64(data.get(off.saturating_add(16)..off.saturating_add(24)).unwrap_or(&[]));
        let p_filesz = read_u64(data.get(off.saturating_add(32)..off.saturating_add(40)).unwrap_or(&[]));

        if p_type == constants::ELF_PT_DYNAMIC {
            dyn_vaddr = Some(p_vaddr);
            dyn_size = Some(p_filesz);
        }
    }

    let (dyn_vaddr, dyn_size) = match (dyn_vaddr, dyn_size) {
        (Some(v), Some(s)) => (v, s),
        _ => return Ok(Vec::new()),
    };

    let dyn_file_off = find_file_offset(data, phoff, phentsize, phnum, dyn_vaddr)?;
    let dyn_start = dyn_file_off as usize;
    let dyn_end = dyn_start.saturating_add(dyn_size as usize);
    if dyn_end > data.len() { return Ok(Vec::new()); }

    let mut strtab_vaddr: Option<u64> = None;
    let mut strtab_size: Option<u64> = None;
    let mut str_offsets: Vec<u64> = Vec::new();

    for off in (dyn_start..dyn_end).step_by(constants::ELF_DYN_ENTRY_SIZE) {
        if off.saturating_add(constants::ELF_DYN_ENTRY_SIZE) > data.len() { break; }
        let d_tag = read_u64(&data[off..off + 8]);
        let d_val = read_u64(&data[off + 8..off + 16]);

        if d_tag == constants::ELF_DT_STRTAB {
            strtab_vaddr = Some(d_val);
        } else if d_tag == constants::ELF_DT_STRSZ {
            strtab_size = Some(d_val);
        } else if d_tag == constants::ELF_DT_NEEDED {
            str_offsets.push(d_val);
        } else if d_tag == constants::ELF_DT_NULL {
            break;
        }
    }

    let (strtab_vaddr, strtab_size) = match (strtab_vaddr, strtab_size) {
        (Some(v), Some(s)) => (v, s),
        _ => return Ok(Vec::new()),
    };

    let strtab_file_off = find_file_offset(data, phoff, phentsize, phnum, strtab_vaddr)? as usize;
    if strtab_file_off.saturating_add(strtab_size as usize) > data.len() {
        return Ok(Vec::new());
    }
    let strtab = &data[strtab_file_off..strtab_file_off + strtab_size as usize];

    let mut result = Vec::new();
    for str_off in str_offsets {
        let s = str_off as usize;
        if s < strtab.len() {
            let end = strtab[s..].iter().position(|&b| b == 0).unwrap_or(strtab.len() - s);
            if end >= 4
                && let Ok(name) = std::str::from_utf8(&strtab[s..s + end]) {
                    result.push(name.to_string());
                }
        }
    }

    Ok(result)
}
fn find_file_offset(data: &[u8], phoff: u64, phentsize: u16, phnum: u16, vaddr: u64) -> Result<u64> {
    for i in 0..phnum as u64 {
        let offset = phoff.saturating_add(i.saturating_mul(phentsize as u64));
        let off = offset as usize;
        if off.saturating_add(phentsize as usize) > data.len() { break; }

        let p_type = read_u32(data.get(off..off.saturating_add(4)).unwrap_or(&[]));
        let p_vaddr = read_u64(data.get(off.saturating_add(16)..off.saturating_add(24)).unwrap_or(&[]));
        let _p_filesz = read_u64(data.get(off.saturating_add(32)..off.saturating_add(40)).unwrap_or(&[]));
        let p_offset = read_u64(data.get(off.saturating_add(8)..off.saturating_add(16)).unwrap_or(&[]));
        let p_memsz = read_u64(data.get(off.saturating_add(40)..off.saturating_add(48)).unwrap_or(&[]));

        if (p_type == constants::ELF_PT_LOAD || p_type == constants::ELF_PT_DYNAMIC)
            && vaddr >= p_vaddr && vaddr < p_vaddr.saturating_add(p_memsz) {
                return Ok(p_offset.saturating_add(vaddr - p_vaddr));
            }
    }
    Err(anyhow!("Cannot resolve virtual address {:#x} to file offset", vaddr))
}

fn scan_elf_strings(path: &Path) -> Result<Vec<String>> {
    let file = fs::File::open(path)
        .map_err(|e| anyhow!("Failed to open {}: {}", path.display(), e))?;
    let mmap = unsafe { Mmap::map(&file) }
        .map_err(|e| anyhow!("Failed to mmap {}: {}", path.display(), e))?;
    let data = &mmap[..];

    let mut strings: Vec<String> = Vec::new();
    let mut current = Vec::new();

    for &byte in data.iter() {
        if byte.is_ascii_graphic() || byte == b'/' || byte == b'.' || byte == b'-' || byte == b'_' || byte == b' ' {
            current.push(byte);
        } else {
            if current.len() >= 4
                && let Ok(s) = String::from_utf8(current.clone()) {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && (trimmed.contains(".so") || trimmed.contains("lib")) {
                            strings.push(trimmed.to_string());
                        }
                }
            current.clear();
        }
    }

    if current.len() >= 4
        && let Ok(s) = String::from_utf8(current) {
            let trimmed = s.trim();
            if !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_digit() || c == '.')
                && (trimmed.contains(".so") || trimmed.contains("lib")) {
                    strings.push(trimmed.to_string());
                }
        }

    strings.sort();
    strings.dedup();
    Ok(strings)
}

fn normalize_library(lib: &str) -> Vec<String> {
    let mut normalized = Vec::new();
    normalized.push(lib.to_string());

    if let Some(stripped) = lib.strip_prefix("lib") {
        normalized.push(stripped.to_string());
        if let Some(pos) = stripped.find('.') {
            normalized.push(format!("lib{}", &stripped[..pos]));
        }
    }

    if let Some(pos) = lib.find(".so") {
        normalized.push(lib[..pos + 3].to_string());
    }

    normalized.sort();
    normalized.dedup();
    normalized
}

fn is_core_system_lib(lib: &str) -> bool {
    let core_prefixes = [
        "libc.so", "libm.so", "libpthread.so", "libdl.so", "librt.so", "libutil.so",
        "libstdc++.so", "libgcc_s.so", "libatomic.so", "libgomp.so", "libquadmath.so",
        "libasan.so", "libubsan.so", "liblsan.so", "libtsan.so",
        "libz.so", "libzstd.so", "liblzma.so", "libbz2.so",
        "libssl.so", "libcrypto.so",
        "libpcre.so", "libpcre2.so", "libexpat.so", "libffi.so",
        "libiconv.so", "libintl.so",
        "libncurses.so", "libtinfo.so", "libreadline.so", "libhistory.so",
        "ld-linux", "ld-musl",
        "libnss_", "libnss3.so", "libnssutil3.so",
        "libselinux.so", "libsepol.so", "libpam.so", "libcap.so",
        "libacl.so", "libattr.so", "libmount.so", "libblkid.so", "libuuid.so",
        "libjson-c.so", "libdbus-1.so",
        "libEGL.so", "libGL.so", "libdrm_", "libX11.so", "libxcb.so", "libwayland-",
        "libpulse.so", "libasound.so", "libsndfile.so",
        "libfreetype.so", "libfontconfig.so", "libharfbuzz.so",
        "libpng", "libjpeg", "libwebp", "libtiff", "libgif",
        "libpython", "libperl.so",
        "libsqlite3.so",
    ];

    for prefix in &core_prefixes {
        if lib.starts_with(prefix) {
            return true;
        }
    }
    false
}

struct LibraryIndex {
    lib_to_pkgs: HashMap<String, Vec<String>>,
}

fn read_u32(buf: &[u8]) -> u32 {
    if buf.len() < 4 { return 0; }
    u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
}

fn read_u64(buf: &[u8]) -> u64 {
    if buf.len() < 8 { return 0; }
    u64::from_le_bytes([
        buf[0], buf[1], buf[2], buf[3],
        buf[4], buf[5], buf[6], buf[7],
    ])
}

fn read_u16(buf: &[u8]) -> u16 {
    if buf.len() < 2 { return 0; }
    u16::from_le_bytes([buf[0], buf[1]])
}

fn semver_parse(version: &str) -> Vec<u64> {
    version.trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

pub fn compute_depsig(deps: &[Dependency]) -> String {
    let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    let joined = sorted.join("|");
    let hash = Sha256::digest(joined.as_bytes());
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn scan_package_directory(path: &Path) -> Result<Vec<String>> {
    let mut libs = HashSet::new();
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    stack.push(entry_path);
                    continue;
                }

                if let Some(name) = entry_path.file_name().and_then(|n| n.to_str())
                    && (name.ends_with(".so") || name.contains(".so.") || name.ends_with(".dll") || name.ends_with(".dylib") || name.ends_with(".a")) {
                        libs.insert(name.to_string());
                    }

                if is_elf_file(&entry_path) {
                    if let Ok(needed) = read_elf_needed(&entry_path) {
                        for lib in needed {
                            libs.insert(lib);
                        }
                    }
                    if let Ok(strings) = scan_elf_strings(&entry_path) {
                        for lib in strings {
                            if lib.contains(".so") {
                        let clean = lib.trim_start_matches(constants::LIB_PATH_PREFIXES[0])
                            .trim_start_matches(constants::LIB_PATH_PREFIXES[1])
                            .trim_start_matches(constants::LIB_PATH_PREFIXES[2]);
                                if clean.contains(".so") {
                                    libs.insert(clean.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<String> = libs.into_iter().filter(|l| !is_core_system_lib(l)).collect();
    result.sort();
    Ok(result)
}

pub fn build_dependency_graph(meta: &PackageMetadata, db: &Database) -> Result<DepGraph> {
    let mut graph = DepGraph::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back(meta.pkg_name.clone());

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current.clone()) { continue; }

        if let Ok(current_meta) = db.get_package_manifest(&current) {
            for dep in &current_meta.dependencies {
                graph.add_edge(&current, &dep.name);
                queue.push_back(dep.name.clone());
            }
        }
    }

    Ok(graph)
}
