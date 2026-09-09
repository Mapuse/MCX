use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::constants;

static LIFECYCLE_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageState {
    Unknown,
    Resolved,
    Staged,
    Installed,
    Active,
    MarkedForRemoval,
    Removed,
    Purged,
}

impl PackageState {
    pub fn can_transition_to(&self, target: PackageState) -> bool {
        matches!(
            (self, target),
            (PackageState::Unknown, PackageState::Resolved)
                | (PackageState::Resolved, PackageState::Staged)
                | (PackageState::Staged, PackageState::Installed)
                | (PackageState::Installed, PackageState::Active)
                | (PackageState::Active, PackageState::MarkedForRemoval)
                | (PackageState::Active, PackageState::Resolved)
                | (PackageState::Installed, PackageState::MarkedForRemoval)
                | (PackageState::MarkedForRemoval, PackageState::Removed)
                | (PackageState::Removed, PackageState::Purged)
                | (PackageState::Resolved, PackageState::Unknown)
                | (PackageState::Staged, PackageState::Unknown)
                | (PackageState::Installed, PackageState::Staged)
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, PackageState::Purged)
    }

    pub fn is_installed(&self) -> bool {
        matches!(self, PackageState::Installed | PackageState::Active)
    }

    pub fn is_removed(&self) -> bool {
        matches!(self, PackageState::Removed | PackageState::Purged)
    }
}

#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    ResolutionComplete,
    StagePrepared,
    InstallationCommitted,
    ActivationApplied,
    RemovalMarked,
    DeletionCommitted,
    PurgeCompleted,
}

#[derive(Debug, Clone)]
pub struct LifecycleTransition {
    pub id: u64,
    pub package: String,
    pub from: PackageState,
    pub to: PackageState,
    pub timestamp: u64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct LifecycleEntry {
    pub package: String,
    pub version: String,
    pub state: PackageState,
    pub generation: u64,
    pub checksum: Option<String>,
}

type HookVec = Vec<Box<dyn Fn(&str, PackageState, PackageState) -> Result<()> + Send + Sync>>;

#[derive(Serialize, Deserialize, Clone)]
struct LifecycleJournalRecord {
    package: String,
    version: String,
    state: PackageState,
    checksum: Option<String>,
    generation: u64,
    timestamp: u64,
}

pub struct LifecycleEngine {
    entries: HashMap<String, LifecycleEntry>,
    transitions: Vec<LifecycleTransition>,
    pre_hooks: HookVec,
    post_hooks: HookVec,
    journal: Option<PathBuf>,
}

impl Default for LifecycleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleEngine {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            transitions: Vec::new(),
            pre_hooks: Vec::new(),
            post_hooks: Vec::new(),
            journal: None,
        }
    }

    pub fn new_with_root(root: &Path) -> Self {
        let mut engine = Self::new();
        engine.journal = Some(root.join(constants::PATH_LIFECYCLE));
        engine.load_journal();
        engine
    }

    fn now_ts(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn load_journal(&mut self) {
        let Some(path) = self.journal.clone() else {
            return;
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return;
        };
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(record): Result<LifecycleJournalRecord, _> = serde_json::from_str(line) else {
                continue;
            };
            if record.generation >= LIFECYCLE_GENERATION.load(Ordering::Relaxed) {
                LIFECYCLE_GENERATION.store(record.generation + 1, Ordering::Relaxed);
            }
            self.entries.insert(
                record.package.clone(),
                LifecycleEntry {
                    package: record.package,
                    version: record.version,
                    state: record.state,
                    generation: record.generation,
                    checksum: record.checksum,
                },
            );
        }
    }

    fn append_journal(
        &self,
        package: &str,
        version: &str,
        state: PackageState,
        checksum: Option<String>,
        generation: u64,
    ) {
        let Some(path) = &self.journal else { return };
        let record = LifecycleJournalRecord {
            package: package.to_string(),
            version: version.to_string(),
            state,
            checksum,
            generation,
            timestamp: self.now_ts(),
        };
        if let Ok(payload) = serde_json::to_string(&record) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", payload);
                let _ = file.sync_all();
            }
        }
    }

    pub fn register_package(&mut self, name: &str, version: &str) -> u64 {
        let gen_id = LIFECYCLE_GENERATION.fetch_add(1, Ordering::Relaxed);
        self.entries.insert(
            name.to_string(),
            LifecycleEntry {
                package: name.to_string(),
                version: version.to_string(),
                state: PackageState::Unknown,
                generation: gen_id,
                checksum: None,
            },
        );
        self.append_journal(name, version, PackageState::Unknown, None, gen_id);
        gen_id
    }

    pub fn state(&self, name: &str) -> Option<PackageState> {
        self.entries.get(name).map(|e| e.state)
    }

    pub fn entry(&self, name: &str) -> Option<&LifecycleEntry> {
        self.entries.get(name)
    }

    pub fn all_entries(&self) -> impl Iterator<Item = &LifecycleEntry> {
        self.entries.values()
    }

    pub fn transition(&mut self, package: &str, target: PackageState) -> Result<u64> {
        let entry = self
            .entries
            .get_mut(package)
            .ok_or_else(|| anyhow!("Package '{}' not registered in lifecycle engine", package))?;

        let current = entry.state;
        if current == target {
            return Ok(0);
        }
        if !current.can_transition_to(target) {
            return Err(anyhow!(
                "Invalid lifecycle transition: {:?} -> {:?} for package '{}'",
                current,
                target,
                package
            ));
        }

        for hook in &self.pre_hooks {
            hook(package, current, target)?;
        }

        let id = LIFECYCLE_GENERATION.fetch_add(1, Ordering::Relaxed);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        entry.state = target;

        self.transitions.push(LifecycleTransition {
            id,
            package: package.to_string(),
            from: current,
            to: target,
            timestamp: ts,
            metadata: HashMap::new(),
        });

        let version = entry.version.clone();
        let checksum = entry.checksum.clone();
        let generation = entry.generation;

        self.append_journal(package, &version, target, checksum, generation);

        for hook in &self.post_hooks {
            hook(package, current, target)?;
        }

        Ok(id)
    }

    pub fn add_pre_hook<F>(&mut self, hook: F)
    where
        F: Fn(&str, PackageState, PackageState) -> Result<()> + Send + Sync + 'static,
    {
        self.pre_hooks.push(Box::new(hook));
    }

    pub fn add_post_hook<F>(&mut self, hook: F)
    where
        F: Fn(&str, PackageState, PackageState) -> Result<()> + Send + Sync + 'static,
    {
        self.post_hooks.push(Box::new(hook));
    }

    pub fn history(&self, package: &str) -> Vec<&LifecycleTransition> {
        self.transitions
            .iter()
            .filter(|t| t.package == package)
            .collect()
    }

    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }

    pub fn installed_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state.is_installed())
            .count()
    }

    pub fn removed_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state.is_removed())
            .count()
    }
}

#[derive(Debug, Clone)]
pub struct OrphanSet {
    pub packages: Vec<String>,
    pub reachable: Vec<String>,
    pub purged: Vec<String>,
}

pub struct DependencyGraph {
    edges: HashMap<String, Vec<String>>,
    reverse: HashMap<String, Vec<String>>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
            reverse: HashMap::new(),
        }
    }

    pub fn add_dep(&mut self, from: &str, to: &str) {
        self.edges
            .entry(from.to_string())
            .or_default()
            .push(to.to_string());
        self.reverse
            .entry(to.to_string())
            .or_default()
            .push(from.to_string());
    }

    pub fn reachable_from(&self, roots: &[String]) -> Vec<String> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = roots.to_vec();
        while let Some(node) = stack.pop() {
            if visited.insert(node.clone())
                && let Some(deps) = self.edges.get(&node)
            {
                stack.extend(deps.iter().cloned());
            }
        }
        let mut result: Vec<String> = visited.into_iter().collect();
        result.sort();
        result
    }

    pub fn orphans(&self, roots: &[String], all_packages: &[String]) -> OrphanSet {
        let reachable: std::collections::HashSet<String> =
            self.reachable_from(roots).into_iter().collect();
        let mut orphaned = Vec::new();
        let mut reachable_pkgs = Vec::new();
        let purged = Vec::new();

        for pkg in all_packages {
            if roots.contains(pkg) || reachable.contains(pkg) {
                reachable_pkgs.push(pkg.clone());
            } else {
                orphaned.push(pkg.clone());
            }
        }

        OrphanSet {
            packages: orphaned,
            reachable: reachable_pkgs,
            purged,
        }
    }
}
