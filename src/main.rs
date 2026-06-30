pub mod core;
pub mod network;
pub mod archive;
pub mod utils;
pub mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::io::Write;
use crate::utils::ui::UserInterface;
use crate::core::database::Database;
use crate::core::config::ConfigManager;
use crate::core::profiler::{SystemProfile, DecisionEngine, NetworkProber};
use crate::core::plugin::{PluginRegistry, CurlFetcher, DefaultBuilder, ZstdPacker};
use crate::commands::add::AddLocalCommand;
use crate::commands::clean::CleanCommand;
use crate::commands::install::InstallCommand;
use crate::commands::remove::RemoveCommand;
use crate::commands::search::SearchCommand;
use crate::commands::sync::SyncCommand;
use crate::commands::system::SystemCommand;
use crate::core::delta::DeltaEngine;

fn default_root() -> String {
    if is_root_process() {
        "/".to_string()
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp/.mcx".to_string());
        PathBuf::from(home).join(".mcx").to_string_lossy().to_string()
    }
}

#[derive(Parser)]
#[command(name = "mcx", version = "3.0.0", disable_version_flag = true)]
struct Cli {
    #[arg(long, global = true, default_value_t = default_root())]
    root: String,
    #[arg(long = "version", short = 'v', help = "Print version")]
    version: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(short_flag = 'i', long_flag = "install", aliases = ["in", "add"])]
    Install { packages: Vec<String> },

    #[command(short_flag = 'a', long_flag = "add", aliases = ["local", "package", "xcs"])]
    AddLocal { file: String },

    #[command(short_flag = 'r', long_flag = "remove", aliases = ["rm", "uninstall", "delete"])]
    Remove { packages: Vec<String> },

    #[command(short_flag = 's', long_flag = "search", aliases = ["find", "look"])]
    Search { query: String },

    #[command(short_flag = 'u', long_flag = "update", aliases = ["refresh", "sync"])]
    Update { packages: Option<Vec<String>> },

    #[command(short_flag = 'U', long_flag = "upgrade", aliases = ["up", "dist-upgrade"])]
    Upgrade { packages: Option<Vec<String>> },

    #[command(short_flag = 'q', long_flag = "query", aliases = ["info", "show"])]
    Query { package: String },

    #[command(short_flag = 'c', long_flag = "clean", aliases = ["wipe", "clear"])]
    Clean,

    #[command(short_flag = 'V', long_flag = "verify", aliases = ["check", "certify"])]
    Verify,

    #[command(short_flag = 'f', long_flag = "fix", aliases = ["fix-deps", "repair"])]
    FixDeps,

    #[command(short_flag = 'C', long_flag = "config", aliases = ["cfg", "settings"])]
    Config,

    #[command(short_flag = 'H', long_flag = "history", aliases = ["log", "record"])]
    History {
        #[arg(long)]
        rollback: Option<String>,
        #[arg(long)]
        prune: Option<usize>,
        #[arg(long)]
        current_gen: Option<String>,
    },

    #[command(short_flag = 'b', long_flag = "build", aliases = ["make", "create"])]
    Build { config: String },

    #[command(long_flag = "repo-add", aliases = ["ra"])]
    RepoAdd { name: String, url: String },

    #[command(long_flag = "repo-remove", aliases = ["rr"])]
    RepoRemove { name: String },

    #[command(long_flag = "repo-list", aliases = ["rl"])]
    RepoList,

    #[command(long_flag = "self-update", aliases = ["update-self"])]
    SelfUpdate,

    #[command(long_flag = "vendor", aliases = ["vnd"])]
    Vendor {
        #[command(subcommand)]
        action: VendorAction,
    },

    #[command(long_flag = "completion", aliases = ["comp"])]
    Completion { shell: String },

    #[command(long_flag = "snapshot", aliases = ["snap"])]
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },

    #[command(long_flag = "swarm", aliases = ["p2p"])]
    Swarm {
        #[command(subcommand)]
        action: SwarmAction,
    },

    #[command(long_flag = "overlay", aliases = ["ovl"])]
    Overlay {
        #[command(subcommand)]
        action: OverlayAction,
    },

    #[command(long_flag = "cgroup", aliases = ["cg"])]
    Cgroup {
        #[command(subcommand)]
        action: CgroupAction,
    },

    #[command(long_flag = "stream", aliases = ["str"])]
    Stream {
        #[command(subcommand)]
        action: StreamAction,
    },
}

#[derive(Subcommand)]
pub enum VendorAction {
    Add { package: String, source: String },
    Remove { package: String },
    List,
}

#[derive(Subcommand)]
pub enum SnapshotAction {
    Take { package: String, pid: u32 },
    List { package: String },
    Restore { package: String, snapshot: String, pid: u32 },
    Remove { package: String },
}

#[derive(Subcommand)]
pub enum SwarmAction {
    RegisterHash { package: String, version: String, hash: String },
    GetHash { package: String },
    RemoveHash { package: String },
    RegisterPeer { address: String, peer_id: String },
    ListPeers,
}

#[derive(Subcommand)]
pub enum OverlayAction {
    Create { package: String, lower: String },
    Remove { package: String },
    List,
}

#[derive(Subcommand)]
pub enum CgroupAction {
    Enforce { package: String, max_memory_mb: u64, max_cpu_percent: u8 },
    EnforceMem { package: String, max_memory_mb: u64 },
    EnforceCpu { package: String, max_cpu_percent: u8 },
    Remove { package: String },
    Status,
}

#[derive(Subcommand)]
pub enum StreamAction {
    Generate { package: String, version: String, url: String },
    Remove { package: String },
    List,
}

struct EngineContext {
    db: Arc<Database>,
    config_mgr: ConfigManager,
    plugin_registry: PluginRegistry,
    sys_profile: SystemProfile,
}

impl EngineContext {
    fn new(root: &PathBuf) -> Self {
        let sys_profile = SystemProfile::probe();

        let config_mgr = ConfigManager::new(root)
            .unwrap_or_else(|e| { UserInterface::error(&format!("Config error: {}", e)); process::exit(1); });

        let mut plugin_registry = PluginRegistry::new();
        plugin_registry.register_fetcher(Arc::new(CurlFetcher));
        plugin_registry.register_builder(Arc::new(DefaultBuilder));
        plugin_registry.register_packer(Arc::new(ZstdPacker));

        let db = match Database::open(root) {
            Ok(database) => Arc::new(database),
            Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
        };

        let _ = crate::core::lifecycle::LifecycleEngine::new();

        Self { db, config_mgr, plugin_registry, sys_profile }
    }
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    if args.version {
        UserInterface::version("mcx 3.0.0");
        return;
    }
    let root_path = PathBuf::from(&args.root);

    let ctx = EngineContext::new(&root_path);

    let _ = &ctx.config_mgr;
    let _ = &ctx.plugin_registry;
    let security_mon = Arc::new(crate::core::security::SecurityMonitor::new());
    let overlay_base = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let cgroup_mgr = crate::core::cgroup::CgroupController::new();
    let overlay_mgr = crate::core::overlay::OverlayManager::new(
        &PathBuf::from(&overlay_base)
    );

    match args.command {
        Commands::Install { packages } => {
            UserInterface::info(&format!("Installing: {:?}", packages));
            let decisions = DecisionEngine::evaluate_thread_strategy(
                &ctx.sys_profile,
                &NetworkProber::probe("https://packages.cudane.org", std::time::Duration::from_secs(5)).await,
                &Default::default(),
            );
            if DecisionEngine::should_use_parallel(&decisions) {
                UserInterface::info("Parallel install strategy selected");
            }
            let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db))
                .with_cgroup(cgroup_mgr)
                .with_overlay(overlay_mgr)
                .with_security(Arc::clone(&security_mon));
            match cmd.execute(&packages).await {
                Ok(_) => {
                    let cas = crate::core::cas::CasStore::new(&root_path);
                    if let Ok(stats) = cas.deduplicate_libraries(&root_path.join("usr/lib")) {
                    UserInterface::cas(&format!(
                        "CAS dedup: {} unique files, {} bytes saved",
                        stats.unique_files, stats.bytes_saved
                    ));
                    }
                    UserInterface::separator();

                    // profile validation after install
                    let profile_path = root_path.join("etc/mcx/profile.json");
                    if profile_path.exists() {
                        match crate::core::declarative::ProfileValidator::load_profile(&profile_path) {
                            Ok(profile) => {
                                let current: Vec<String> = ctx.db.get_all_installed_packages()
                                    .unwrap_or_default()
                                    .iter()
                                    .map(|p| p.pkg_name.clone())
                                    .collect();
                                let (to_install, to_remove) =
                                    crate::core::declarative::ProfileValidator::compile_profile_diff(
                                        &current, &profile.packages);
                                if !to_install.is_empty() || !to_remove.is_empty() {
                                    UserInterface::info(&format!(
                                        "Profile drift: {} to install, {} to remove",
                                        to_install.len(), to_remove.len()));
                                }
                            }
                            Err(e) => {
                                UserInterface::info(&format!("Profile check: {}", e));
                            }
                        }
                    }
                    UserInterface::security(&format!("Tracking {} active packages", security_mon.active_count()));
                    UserInterface::success("Installation committed.");
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Remove { packages } => {
            UserInterface::info(&format!("Removing: {:?}", packages));
            let cmd = RemoveCommand::new(args.root.clone(), Arc::clone(&ctx.db));
            match cmd.execute(&packages, &cgroup_mgr, &overlay_mgr, &security_mon) {
                Ok(_) => {
                    UserInterface::separator();

                    // profile validation after removal
                    let profile_path = root_path.join("etc/mcx/profile.json");
                    if profile_path.exists() {
                        match crate::core::declarative::ProfileValidator::load_profile(&profile_path) {
                            Ok(profile) => {
                                let current: Vec<String> = ctx.db.get_all_installed_packages()
                                    .unwrap_or_default()
                                    .iter()
                                    .map(|p| p.pkg_name.clone())
                                    .collect();
                                let (to_install, to_remove) =
                                    crate::core::declarative::ProfileValidator::compile_profile_diff(
                                        &current, &profile.packages);
                                if !to_install.is_empty() || !to_remove.is_empty() {
                                    UserInterface::info(&format!(
                                        "Profile drift: {} to install, {} to remove",
                                        to_install.len(), to_remove.len()));
                                }
                            }
                            Err(e) => {
                                UserInterface::info(&format!("Profile check: {}", e));
                            }
                        }
                    }
                    UserInterface::security(&format!("Tracking {} active packages", security_mon.active_count()));
                    UserInterface::success("Packages removed.");
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Build { config } => {
            UserInterface::info(&format!("Building from: {}", config));
            let ws = crate::core::workspace::WorkspaceManager::new(&root_path);
            if let Err(e) = ws.initialize() {
                UserInterface::error(&format!("Workspace init failed: {e}"));
                process::exit(1);
            }
            let cmd = SystemCommand::new(args.root.clone(), Arc::clone(&ctx.db));
            match cmd.rebuild(&config).await {
                Ok(_) => {
                    if let Err(e) = ws.clean_global_workspaces() {
                        UserInterface::error(&format!("Workspace cleanup failed: {e}"));
                    }
                    UserInterface::success("System aligned.");
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::AddLocal { file } => {
            UserInterface::info(&format!("Installing local: {}", file));
            let cmd = AddLocalCommand::new(args.root.clone(), Arc::clone(&ctx.db));
            match cmd.execute(&file) {
                Ok(_) => {
                    let staging = PathBuf::from(&args.root).join("var/tmp/mcx/stage");
                    let installed_root = PathBuf::from(&args.root).join("var/lib/mcx/active");
                    if let Ok(pkgs) = ctx.db.get_all_installed_packages() {
                        if let Some(last) = pkgs.last() {
                            let pkg_path = staging.join(&last.pkg_name);
                            if pkg_path.exists() {
                                if let Err(e) = std::fs::rename(&pkg_path, installed_root.join(&last.pkg_name)) {
                                    UserInterface::error(&format!("Failed to move package from staging: {e}"));
                                    process::exit(1);
                                }
                            }
                        }
                    }
                    let rollback_mgr = crate::core::rollback::RollbackManager::new(&root_path);
                    let gen_root = root_path.join("var/lib/mcx/active");
                    if gen_root.exists() {
                        let _ = rollback_mgr.enable_atomic_rollback("local", &gen_root);
                    }
                    UserInterface::success("Local package installed.");
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Search { query } => {
            let cmd = SearchCommand::new(Arc::clone(&ctx.db));
            if let Err(e) = cmd.execute(&query) { UserInterface::error(&format!("{}", e)); process::exit(1); }
        }
        Commands::Update { packages } => {
            if let Some(pkgs) = packages {
                UserInterface::info(&format!("Updating specific packages: {:?}", pkgs));
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db));
                match cmd.execute(&pkgs).await {
                    Ok(_) => UserInterface::success("Packages updated."),
                    Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
                }
            } else {
                UserInterface::info("Syncing repositories in parallel...");
                let cmd = SyncCommand::new(args.root.clone(), Arc::clone(&ctx.db));
                match cmd.execute().await {
                    Ok(_) => UserInterface::success("Repositories synced."),
                    Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
                }
            }
        }
        Commands::Upgrade { packages } => {
            let pkgs_to_upgrade: Vec<String> = if let Some(pkgs) = packages {
                pkgs
            } else {
                ctx.db.get_all_installed_packages()
                    .unwrap_or_default().into_iter().map(|p| p.pkg_name).collect()
            };

            UserInterface::info(&format!("Upgrading: {:?}", pkgs_to_upgrade));

            let active_base = root_path.join("var/lib/mcx/active");
            let backup_base = root_path.join("var/tmp/mcx/upgrade-backup");
            let deltas_dir = root_path.join("var/lib/mcx/deltas");
            let _ = fs::remove_dir_all(&backup_base);

            let mut old_state: Vec<(String, String, PathBuf)> = Vec::new();
            for pkg in &pkgs_to_upgrade {
                if let Ok(meta) = ctx.db.get_package_manifest(pkg) {
                    let active_dir = active_base.join(pkg);
                    if active_dir.exists() {
                        let backup = backup_base.join(pkg);
                        let _ = fs::remove_dir_all(&backup);
                        let _ = recursive_copy(&active_dir, &backup);
                        old_state.push((pkg.clone(), meta.version.clone(), backup));
                    }
                }
            }

            let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db));
            match cmd.execute(&pkgs_to_upgrade).await {
                Ok(_) => {
                    let _ = fs::create_dir_all(&deltas_dir);
                    for (pkg_name, old_ver, backup_dir) in &old_state {
                        let new_active = active_base.join(pkg_name);
                        if !new_active.exists() || !backup_dir.exists() {
                            continue;
                        }
                        let new_ver = match ctx.db.get_package_manifest(pkg_name) {
                            Ok(m) => m.version.clone(),
                            _ => continue,
                        };
                        match DeltaEngine::compute_delta(backup_dir, &new_active, pkg_name, old_ver, &new_ver) {
                            Ok(delta) => {
                                let added = delta.manifest.added.len();
                                let removed = delta.manifest.removed.len();
                                let modified = delta.manifest.modified.len();
                                UserInterface::info(&format!(
                                    "{}: {} → {} ({} added, {} removed, {} modified)",
                                    pkg_name, old_ver, new_ver, added, removed, modified,
                                ));
                                let xcd_path = deltas_dir.join(format!("{}-{}-{}.xcd", pkg_name, old_ver, new_ver));
                                if let Err(e) = DeltaEngine::write_delta(&delta, &xcd_path) {
                                    UserInterface::error(&format!("Delta persist failed: {e}"));
                                }
                            }
                            Err(e) => {
                                UserInterface::error(&format!("Delta compute failed for {}: {e}", pkg_name));
                            }
                        }
                    }
                    let _ = fs::remove_dir_all(&backup_base);
                    UserInterface::success("Upgrade complete.");
                }
                Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
            }
        }
        Commands::Query { package } => {
            match ctx.db.get_package_manifest(&package) {
                Ok(meta) => {
                    let file_count = meta.files.len().to_string();
                    let dep_count = meta.dependencies.len().to_string();

                    let rdepends: Vec<String> = ctx.db.get_all_installed_packages()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| p.dependencies.iter().any(|d| d.name == package))
                        .map(|p| p.pkg_name)
                        .collect();
                    let rdeps_str = if rdepends.is_empty() {
                        "none".to_string()
                    } else {
                        rdepends.join(", ")
                    };

                    let dep_list: Vec<String> = meta.dependencies.iter()
                        .map(|d| {
                            let ver = ctx.db.get_package_manifest(&d.name)
                                .ok()
                                .map(|m| m.version)
                                .unwrap_or_default();
                            format!("{} {} ({})", d.name, ver, d.dep_type)
                        })
                        .collect();

                    let pairs = [
                        ("Package", meta.pkg_name.as_str()),
                        ("Version", meta.version.as_str()),
                        ("License", meta.license.as_str()),
                        ("Source", meta.source.as_str()),
                        ("Files", &file_count),
                        ("Dependencies", &dep_count),
                        ("Reverse deps", &rdeps_str),
                    ];
                    UserInterface::render_key_values(&format!("Package: {}", meta.pkg_name), &pairs);

                    let table_rows = vec![
                        vec![meta.pkg_name.clone(), meta.version.clone(), meta.license.clone()],
                    ];
                    UserInterface::table("Package summary", &["Name", "Version", "License"], &table_rows);

                    if !dep_list.is_empty() {
                        UserInterface::render_list("Dependency tree", &dep_list);
                    }
                    if !rdepends.is_empty() {
                        UserInterface::render_list("Required by", &rdepends);
                    }
                    if !meta.files.is_empty() {
                        let file_strings: Vec<String> = meta.files.iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();
                        UserInterface::render_list("Installed files", &file_strings);
                    }
                }
                Err(_) => UserInterface::error("Not installed."),
            }
        }
        Commands::Clean => {
            let cmd = CleanCommand::new(&args.root);
            match cmd.execute(true, true) {
                Ok(_) => {
                    let ws = crate::core::workspace::WorkspaceManager::new(&root_path);
                    let _ = ws.clean_global_workspaces();
                    UserInterface::success("Cache cleared.");
                }
                Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
            }
        }
        Commands::Verify => {
            let all_pkgs = ctx.db.get_all_installed_packages().unwrap_or_default();
            let installed_names: std::collections::HashSet<String> = all_pkgs.iter().map(|p| p.pkg_name.clone()).collect();
            let active_base = root_path.join("var/lib/mcx/active");
            let mut errors: Vec<String> = Vec::new();

            let total = all_pkgs.len();
            for (idx, pkg) in all_pkgs.iter().enumerate() {
                UserInterface::progress(idx + 1, total, "Verifying packages");
                for file in &pkg.files {
                    let full = root_path.join(file);
                    if !full.exists() {
                        errors.push(format!("{}: missing file {}", pkg.pkg_name, file.display()));
                    }
                }
                let pkg_active = active_base.join(&pkg.pkg_name);
                if !pkg_active.exists() {
                    errors.push(format!("{}: missing active directory", pkg.pkg_name));
                }
                for dep in &pkg.dependencies {
                    if !installed_names.contains(&dep.name) {
                        errors.push(format!("{}: missing dependency {}", pkg.pkg_name, dep.name));
                    }
                }
            }
            // clear progress line
            print!("\r\x1b[K");
            let _ = std::io::stdout().flush();

            let dangling_count = count_dangling_symlinks(&root_path);
            if dangling_count > 0 {
                errors.push(format!("{} dangling symlink(s) found", dangling_count));
            }

            if errors.is_empty() {
                UserInterface::block("Verification summary", &[
                    &format!("{} packages checked", all_pkgs.len()),
                    "No broken dependencies",
                    "No missing files",
                    "No dangling symlinks",
                ]);
                UserInterface::success(&format!("All {} packages intact.", all_pkgs.len()));
            } else {
                for e in &errors {
                    UserInterface::error(e);
                }
                UserInterface::error(&format!("{} issues found. Run mcx -f to repair.", errors.len()));
            }
        }
        Commands::FixDeps => {
            let all_pkgs = ctx.db.get_all_installed_packages().unwrap_or_default();
            let installed_names: Vec<String> = all_pkgs.iter().map(|p| p.pkg_name.clone()).collect();
            let mut missing_deps = Vec::new();
            let mut missing_files_pkgs = Vec::new();

            for pkg in &all_pkgs {
                let mut missing_files = false;
                for file in &pkg.files {
                    if !root_path.join(file).exists() {
                        missing_files = true;
                        break;
                    }
                }
                if missing_files {
                    missing_files_pkgs.push(pkg.pkg_name.clone());
                }

                for dep in &pkg.dependencies {
                    if !installed_names.contains(&dep.name) {
                        if !missing_deps.contains(&dep.name) {
                            missing_deps.push(dep.name.clone());
                        }
                    }
                }
            }

            let mut fixed = 0usize;
            if !missing_deps.is_empty() {
                UserInterface::info(&format!("Installing {} missing dependencies...", missing_deps.len()));
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db));
                if let Err(e) = cmd.execute(&missing_deps).await {
                    UserInterface::error(&format!("Dependency install failed: {e}"));
                } else {
                    fixed += missing_deps.len();
                }
            }

            if !missing_files_pkgs.is_empty() {
                UserInterface::info(&format!("Reinstalling {} packages with missing files...", missing_files_pkgs.len()));
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db));
                if let Err(e) = cmd.execute(&missing_files_pkgs).await {
                    UserInterface::error(&format!("Reinstall failed: {e}"));
                } else {
                    fixed += missing_files_pkgs.len();
                }
            }

            if fixed > 0 {
                UserInterface::success(&format!("Repair complete: {} issues resolved.", fixed));
            } else {
                UserInterface::success("All packages intact. No repair needed.");
            }
        }
        Commands::Config => UserInterface::success("Configuration saved."),

        Commands::History { rollback, prune, current_gen } => {
            let rollback_mgr = crate::core::rollback::RollbackManager::new(&root_path);
            let _ = rollback_mgr.initialize();

            if let Some(keep) = prune {
                let all_pkgs = ctx.db.get_all_installed_packages().unwrap_or_default();
                let pkg_names: Vec<String> = all_pkgs.iter().map(|p| p.pkg_name.clone()).collect();
                let mut total = 0;
                for name in &pkg_names {
                    match rollback_mgr.prune_generations(name, keep) {
                        Ok(n) => total += n,
                        Err(e) => UserInterface::error(&format!("Prune failed for {}: {e}", name)),
                    }
                }
                UserInterface::success(&format!("Pruned {} old generations (keeping {})", total, keep));
            } else if let Some(pkg_name) = current_gen {
                match rollback_mgr.current_generation(&pkg_name) {
                    Ok(Some(generation_id)) => UserInterface::success(&format!("{}: current generation {}", pkg_name, generation_id.0)),
                    Ok(None) => UserInterface::info("No generations recorded."),
                    Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                }
            } else if let Some(tx_id) = rollback {
                UserInterface::info(&format!("Rolling back to transaction {}", tx_id));
                let history = crate::core::history::HistoryEngine::new(&root_path, Arc::clone(&ctx.db));
                match tx_id.parse::<u64>() {
                    Ok(id) => {
                        match history.compute_rollback_plan(id) {
                            Ok(plan) => {
                                for (action, targets) in &plan {
                                    match action {
                                        crate::core::changelog::ActionKind::Installation => {
                                            UserInterface::info(&format!("Rollback: install {:?}", targets));
                                        }
                                        crate::core::changelog::ActionKind::Removal => {
                                            UserInterface::info(&format!("Rollback: remove {:?}", targets));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                UserInterface::error(&format!("Rollback plan failed: {e}"));
                                process::exit(1);
                            }
                        }
                    }
                    Err(_) => {
                        UserInterface::error("Invalid transaction ID");
                        process::exit(1);
                    }
                }
                UserInterface::success("Rollback complete.");
            } else {
                let history = crate::core::history::HistoryEngine::new(&root_path, Arc::clone(&ctx.db));
                match history.fetch_ordered_log() {
                    Ok(records) => {
                        let items: Vec<String> = records.iter()
                            .map(|r| format!("#{} {}: {:?}", r.transaction_id, r.timestamp, r.targets))
                            .collect();
                        UserInterface::render_list("Transaction history", &items);
                    }
                    Err(e) => {
                        UserInterface::error(&format!("History fetch failed: {e}"));
                        process::exit(1);
                    }
                }
            }
        }

        Commands::RepoAdd { name, url } => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.add_repository(crate::core::database::RepositoryInfo {
                name, url, checksum: None,
            }) {
                Ok(_) => UserInterface::success("Repository added."),
                Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
            }
        }
        Commands::RepoRemove { name } => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.remove_repository(&name) {
                Ok(_) => UserInterface::success("Repository removed."),
                Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
            }
        }
        Commands::RepoList => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.load_repositories() {
                Ok(repos) => {
                    let items = repos
                        .into_iter()
                        .map(|r| format!("{} -> {}", r.name, r.url))
                        .collect::<Vec<_>>();
                    UserInterface::render_list("Repositories", &items);
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }

        Commands::SelfUpdate => {
            let output_path = PathBuf::from("/system/bin/mcx");
            if !is_root_process() {
                elevate_for("self-update");
            }

            UserInterface::self_update("Building from source (codeberg.org/Cudane/MCX)...");
            let tmp = std::env::temp_dir().join("mcx-self-update");
            let _ = fs::remove_dir_all(&tmp);

            let clone_status = std::process::Command::new("git")
                .args(["clone", "https://codeberg.org/Cudane/MCX", &tmp.to_string_lossy()])
                .status()
                .unwrap_or_else(|_| { UserInterface::error("git not found"); process::exit(1); });
            if !clone_status.success() {
                UserInterface::error("Clone failed");
                process::exit(1);
            }

            UserInterface::info("Compiling (cargo build --release --target x86_64-unknown-linux-musl)...");
            let build_status = std::process::Command::new("cargo")
                .args(["build", "--release", "--target", "x86_64-unknown-linux-musl"])
                .current_dir(&tmp)
                .status()
                .unwrap_or_else(|_| { UserInterface::error("cargo not found"); process::exit(1); });
            if !build_status.success() {
                UserInterface::error("Build failed");
                let _ = fs::remove_dir_all(&tmp);
                process::exit(1);
            }

            let built = tmp.join("target/x86_64-unknown-linux-musl/release/mcx");
            if !built.exists() {
                UserInterface::error("Built binary not found");
                let _ = fs::remove_dir_all(&tmp);
                process::exit(1);
            }

            // verify the built binary is functional before swapping
            let verify = std::process::Command::new(&built)
                .arg("--version")
                .output();
            match verify {
                Ok(out) if out.status.success() => {
                    let ver = String::from_utf8_lossy(&out.stdout);
                    UserInterface::info(&format!("Built: {}", ver.trim()));
                }
                _ => {
                    UserInterface::error("Built binary failed verification");
                    let _ = fs::remove_dir_all(&tmp);
                    process::exit(1);
                }
            }

            // atomic swap: write to .new, rename over target (atomic on same filesystem)
            let new_path = output_path.with_extension("mcx.new");
            if let Some(parent) = new_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if new_path.exists() {
                let _ = fs::remove_file(&new_path);
            }
            fs::copy(&built, &new_path).unwrap_or_else(|e| {
                UserInterface::error(&format!("Copy failed: {}", e));
                let _ = fs::remove_dir_all(&tmp);
                process::exit(1);
            });
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&new_path, fs::Permissions::from_mode(0o755));
            }
            fs::rename(&new_path, &output_path).unwrap_or_else(|e| {
                UserInterface::error(&format!("Atomic rename failed: {}", e));
                let _ = fs::remove_dir_all(&tmp);
                process::exit(1);
            });

            let _ = fs::remove_dir_all(&tmp);
            UserInterface::self_update("Self-update complete. New binary at /system/bin/mcx");
        }

        Commands::Vendor { action } => {
            let vendor = crate::core::vendor::VendorManager::new(&root_path);
            if let Err(e) = vendor.initialize() {
                UserInterface::error(&format!("Vendor init failed: {e}"));
                process::exit(1);
            }
            match action {
                VendorAction::Add { package, source } => {
                    let src = PathBuf::from(&source);
                    match vendor.register_vendor_package(&package, &src) {
                        Ok(_) => UserInterface::success(&format!("Vendored {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                VendorAction::Remove { package } => {
                    match vendor.remove_vendor_package(&package) {
                        Ok(_) => UserInterface::success(&format!("Removed vendored {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                VendorAction::List => {
                    let items: Vec<String> = ctx.db.get_all_installed_packages()
                        .unwrap_or_default()
                        .iter()
                        .filter(|p| vendor.verify_vendor_presence(&p.pkg_name))
                        .map(|p| format!("{} {} (vendored)", p.pkg_name, p.version))
                        .collect();
                    UserInterface::render_list("Vendored packages", &items);
                }
            }
        }

        Commands::Completion { shell } => {
            let comp = crate::core::completion::CompletionEngine::new(Arc::clone(&ctx.db));
            match comp.generate_shell_blueprint(&shell) {
                Ok(script) => {
                    println!("{}", script);
                }
                Err(e) => {
                    UserInterface::error(&format!("Completion generation failed: {e}"));
                    process::exit(1);
                }
            }
        }

        Commands::Snapshot { action } => {
            let snap_mgr = crate::core::snapshot::SnapshotManager::new(&root_path);
            if let Err(e) = snap_mgr.initialize() {
                UserInterface::error(&format!("Snapshot init failed: {e}"));
                process::exit(1);
            }
            match action {
                SnapshotAction::Take { package, pid } => {
                    match snap_mgr.checkpoint_process(&package, pid) {
                        Ok(path) => UserInterface::snapshot(&format!("Snapshot saved: {:?}", path)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                SnapshotAction::List { package } => {
                    match snap_mgr.list_snapshots(&package) {
                        Ok(snapshots) => {
                            let items: Vec<String> = snapshots.iter().map(|p| p.to_string_lossy().to_string()).collect();
                            UserInterface::render_list(&format!("Snapshots for {}", package), &items);
                        }
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                SnapshotAction::Restore { package: _, snapshot, pid } => {
                    let snap_path = PathBuf::from(&snapshot);
                    match snap_mgr.restore_snapshot(&snap_path, pid) {
                        Ok(_) => UserInterface::snapshot("Snapshot restored."),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                SnapshotAction::Remove { package } => {
                    match snap_mgr.remove_snapshots(&package) {
                        Ok(_) => UserInterface::snapshot(&format!("Snapshots removed for {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }

        Commands::Swarm { action } => {
            let swarm_mgr = crate::core::swarm::SwarmManager::new(&root_path);
            if let Err(e) = swarm_mgr.initialize() {
                UserInterface::error(&format!("Swarm init failed: {e}"));
                process::exit(1);
            }
            match action {
                SwarmAction::RegisterHash { package, version, hash } => {
                    match swarm_mgr.register_swarm_hash(&package, &version, &hash) {
                        Ok(_) => UserInterface::success(&format!("Swarm hash registered for {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                SwarmAction::GetHash { package } => {
                    match swarm_mgr.get_swarm_hash(&package) {
                        Ok(Some(hash)) => UserInterface::success(&format!("{}: {}", package, hash)),
                        Ok(None) => UserInterface::info("No swarm hash registered."),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                SwarmAction::RemoveHash { package } => {
                    match swarm_mgr.remove_swarm_entry(&package) {
                        Ok(_) => UserInterface::success(&format!("Swarm hash removed for {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                SwarmAction::RegisterPeer { address, peer_id } => {
                    let peer = crate::core::swarm::SwarmPeer {
                        address,
                        peer_id,
                        last_seen: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                        advertised_hashes: Vec::new(),
                    };
                    match swarm_mgr.register_swarm_peer(peer) {
                        Ok(_) => UserInterface::success("Peer registered."),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                SwarmAction::ListPeers => {
                    match swarm_mgr.list_swarm_peers() {
                        Ok(peers) => {
                            let items: Vec<String> = peers.iter()
                                .map(|p| format!("{} @ {} [{} hashes]", p.peer_id, p.address, p.advertised_hashes.len()))
                                .collect();
                            UserInterface::render_list("Swarm peers", &items);
                        }
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }

        Commands::Overlay { action } => {
            if !is_root_process() {
                elevate_for("overlay");
            }
            let overlay_mgr = crate::core::overlay::OverlayManager::new(&root_path.join(
                std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
            ));
            if let Err(e) = overlay_mgr.initialize() {
                UserInterface::error(&format!("Overlay init failed: {e}"));
                process::exit(1);
            }
            match action {
                OverlayAction::Create { package, lower } => {
                    match overlay_mgr.create_isolated_overlay(&package, PathBuf::from(&lower).as_path()) {
                        Ok(merged) => UserInterface::success(&format!("Overlay created: {:?}", merged)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                OverlayAction::Remove { package } => {
                    match overlay_mgr.remove_isolated_overlay(&package) {
                        Ok(_) => UserInterface::success("Overlay removed."),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                OverlayAction::List => {
                    match overlay_mgr.list_overlays() {
                        Ok(overlays) => {
                            let items: Vec<String> = overlays.iter()
                                .map(|p| p.to_string_lossy().to_string())
                                .collect();
                            UserInterface::render_list("Active overlays", &items);
                        }
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }

        Commands::Cgroup { action } => {
            if !is_root_process() {
                elevate_for("cgroup");
            }
            let cg_mgr = crate::core::cgroup::CgroupController::new();
            match action {
                CgroupAction::Enforce { package, max_memory_mb, max_cpu_percent } => {
                    if !cg_mgr.is_cgroup_v2_available() {
                        UserInterface::error("cgroup v2 not available on this system");
                        process::exit(1);
                    }
                    match cg_mgr.enforce_resource_limits(&package, max_memory_mb, max_cpu_percent) {
                        Ok(_) => UserInterface::cgroup(&format!("Limits enforced for {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::EnforceMem { package, max_memory_mb } => {
                    if !cg_mgr.is_cgroup_v2_available() {
                        UserInterface::error("cgroup v2 not available on this system");
                        process::exit(1);
                    }
                    match cg_mgr.enforce_memory_limit(&package, max_memory_mb) {
                        Ok(_) => UserInterface::cgroup(&format!("Memory limit enforced for {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::EnforceCpu { package, max_cpu_percent } => {
                    if !cg_mgr.is_cgroup_v2_available() {
                        UserInterface::error("cgroup v2 not available on this system");
                        process::exit(1);
                    }
                    match cg_mgr.enforce_cpu_limit(&package, max_cpu_percent) {
                        Ok(_) => UserInterface::cgroup(&format!("CPU limit enforced for {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::Remove { package } => {
                    match cg_mgr.remove_resource_limits(&package) {
                        Ok(_) => UserInterface::cgroup(&format!("Limits removed for {}", package)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::Status => {
                    if cg_mgr.is_cgroup_v2_available() {
                        UserInterface::cgroup("cgroup v2 available at /sys/fs/cgroup/mcx");
                    } else {
                        UserInterface::info("cgroup v2 not available");
                    }
                }
            }
        }

        Commands::Stream { action } => {
            let stream_mgr = crate::core::stream::StreamManager::new(&root_path);
            if let Err(e) = stream_mgr.initialize() {
                UserInterface::error(&format!("Stream init failed: {e}"));
                process::exit(1);
            }
            match action {
                StreamAction::Generate { package, version, url } => {
                    match stream_mgr.generate_stream_mount_script(&package, &version, &url) {
                        Ok(path) => UserInterface::success(&format!("Stream script generated: {:?}", path)),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                StreamAction::Remove { package } => {
                    match stream_mgr.remove_stream_script(&package) {
                        Ok(_) => UserInterface::success("Stream script removed."),
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                StreamAction::List => {
                    match stream_mgr.list_stream_scripts() {
                        Ok(scripts) => {
                            let items: Vec<String> = scripts.iter()
                                .map(|p| p.to_string_lossy().to_string())
                                .collect();
                            UserInterface::render_list("Stream mount scripts", &items);
                        }
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }
    }
}

fn elevate_for(command: &str) -> ! {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("mcx"));
    let all_args: Vec<String> = std::env::args().collect();
    let status = std::process::Command::new("sudo")
        .arg(&exe)
        .args(&all_args[1..])
        .status()
        .unwrap_or_else(|e| {
            UserInterface::error(&format!("sudo escalation failed for {}: {}", command, e));
            process::exit(1);
        });
    process::exit(status.code().unwrap_or(1));
}

fn is_root_process() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false)
}

fn count_dangling_symlinks(root: &Path) -> usize {
    let mut count = 0usize;
    let dirs = ["usr", "etc", "var"];
    for d in &dirs {
        let target = root.join(d);
        if !target.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&target) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() {
                    if !path.exists() {
                        count += 1;
                    }
                } else if path.is_dir() {
                    count += count_dangling_symlinks(&path);
                }
            }
        }
    }
    count
}

fn recursive_copy(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(src).unwrap();
        let dest = dst.join(rel);
        if path.is_dir() {
            recursive_copy(&path, &dest)?;
        } else if path.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}
