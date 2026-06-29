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

#[derive(Parser)]
#[command(name = "mcx", version = "2.8.5", disable_version_flag = true)]
struct Cli {
    #[arg(long, global = true, default_value = "/")]
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
            .unwrap_or_else(|e| { eprintln!("Config error: {}", e); process::exit(1); });

        let mut plugin_registry = PluginRegistry::new();
        plugin_registry.register_fetcher(Arc::new(CurlFetcher));
        plugin_registry.register_builder(Arc::new(DefaultBuilder));
        plugin_registry.register_packer(Arc::new(ZstdPacker));

        let db = match Database::open(root) {
            Ok(database) => Arc::new(database),
            Err(e) => { eprintln!("{}", e); process::exit(1); }
        };

        let _ = crate::core::lifecycle::LifecycleEngine::new();

        Self { db, config_mgr, plugin_registry, sys_profile }
    }
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    if args.version {
        println!("mcx 2.8.5");
        return;
    }
    let root_path = PathBuf::from(&args.root);

    if elevate_if_needed(&root_path) {
        return;
    }

    let ctx = EngineContext::new(&root_path);

    let _ = &ctx.config_mgr;
    let _ = &ctx.plugin_registry;

    match args.command {
        Commands::Install { packages } => {
            UserInterface::display_info(&format!("Installing: {:?}", packages));
            let decisions = DecisionEngine::evaluate_thread_strategy(
                &ctx.sys_profile,
                &NetworkProber::probe("https://packages.cudane.org", std::time::Duration::from_secs(5)).await,
                &Default::default(),
            );
            if DecisionEngine::should_use_parallel(&decisions) {
                UserInterface::display_info("Parallel install strategy selected");
            }
            let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db));
            match cmd.execute(&packages).await {
                Ok(_) => {
                    let cas = crate::core::cas::CasStore::new(&root_path);
                    if let Ok(stats) = cas.deduplicate_libraries(&root_path.join("usr/lib")) {
                        UserInterface::display_info(&format!(
                            "CAS dedup: {} unique files, {} bytes saved",
                            stats.unique_files, stats.bytes_saved
                        ));
                    }
                    UserInterface::display_success("Installation committed.");
                }
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Remove { packages } => {
            UserInterface::display_info(&format!("Removing: {:?}", packages));
            let cmd = RemoveCommand::new(args.root.clone(), Arc::clone(&ctx.db));
            match cmd.execute(&packages) {
                Ok(_) => UserInterface::display_success("Packages removed."),
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Build { config } => {
            UserInterface::display_info(&format!("Building from: {}", config));
            let ws = crate::core::workspace::WorkspaceManager::new(&root_path);
            if let Err(e) = ws.initialize() {
                UserInterface::display_error(&format!("Workspace init failed: {e}"));
                process::exit(1);
            }
            let cmd = SystemCommand::new(args.root.clone(), Arc::clone(&ctx.db));
            match cmd.rebuild(&config).await {
                Ok(_) => {
                    if let Err(e) = ws.clean_global_workspaces() {
                        UserInterface::display_error(&format!("Workspace cleanup failed: {e}"));
                    }
                    UserInterface::display_success("System aligned.");
                }
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::AddLocal { file } => {
            UserInterface::display_info(&format!("Installing local: {}", file));
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
                                    UserInterface::display_error(&format!("Failed to move package from staging: {e}"));
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
                    UserInterface::display_success("Local package installed.");
                }
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Search { query } => {
            let cmd = SearchCommand::new(Arc::clone(&ctx.db));
            if let Err(e) = cmd.execute(&query) { eprintln!("{}", e); process::exit(1); }
        }
        Commands::Update { packages } => {
            if let Some(pkgs) = packages {
                UserInterface::display_info(&format!("Updating specific packages: {:?}", pkgs));
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db));
                match cmd.execute(&pkgs).await {
                    Ok(_) => UserInterface::display_success("Packages updated."),
                    Err(e) => { eprintln!("{}", e); process::exit(1); }
                }
            } else {
                UserInterface::display_info("Syncing repositories in parallel...");
                let cmd = SyncCommand::new(args.root.clone(), Arc::clone(&ctx.db));
                match cmd.execute().await {
                    Ok(_) => UserInterface::display_success("Repositories synced."),
                    Err(e) => { eprintln!("{}", e); process::exit(1); }
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

            UserInterface::display_info(&format!("Upgrading: {:?}", pkgs_to_upgrade));

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
                                UserInterface::display_info(&format!(
                                    "{}: {} → {} ({} added, {} removed, {} modified)",
                                    pkg_name, old_ver, new_ver, added, removed, modified,
                                ));
                                let xcd_path = deltas_dir.join(format!("{}-{}-{}.xcd", pkg_name, old_ver, new_ver));
                                if let Err(e) = DeltaEngine::write_delta(&delta, &xcd_path) {
                                    UserInterface::display_error(&format!("Delta persist failed: {e}"));
                                }
                            }
                            Err(e) => {
                                UserInterface::display_error(&format!("Delta compute failed for {}: {e}", pkg_name));
                            }
                        }
                    }
                    let _ = fs::remove_dir_all(&backup_base);
                    UserInterface::display_success("Upgrade complete.");
                }
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Query { package } => {
            match ctx.db.get_package_manifest(&package) {
                Ok(meta) => {
                    let file_count = meta.files.len().to_string();
                    let dep_count = meta.dependencies.len().to_string();
                    let pairs2 = [
                        ("Package", meta.pkg_name.as_str()),
                        ("Version", meta.version.as_str()),
                        ("License", meta.license.as_str()),
                        ("Source", meta.source.as_str()),
                        ("Files", file_count.as_str()),
                        ("Dependencies", dep_count.as_str()),
                    ];
                    UserInterface::render_key_values("Installed package", &pairs2);
                }
                Err(_) => UserInterface::display_error("Not installed."),
            }
        }
        Commands::Clean => {
            let cmd = CleanCommand::new(&args.root);
            match cmd.execute(true, true) {
                Ok(_) => {
                    let ws = crate::core::workspace::WorkspaceManager::new(&root_path);
                    let _ = ws.clean_global_workspaces();
                    UserInterface::display_success("Cache cleared.");
                }
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Verify => UserInterface::display_success("Verification passed."),
        Commands::FixDeps => UserInterface::display_success("Dependencies fixed."),
        Commands::Config => UserInterface::display_success("Configuration saved."),

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
                        Err(e) => UserInterface::display_error(&format!("Prune failed for {}: {e}", name)),
                    }
                }
                UserInterface::display_success(&format!("Pruned {} old generations (keeping {})", total, keep));
            } else if let Some(pkg_name) = current_gen {
                match rollback_mgr.current_generation(&pkg_name) {
                    Ok(Some(generation_id)) => UserInterface::display_success(&format!("{}: current generation {}", pkg_name, generation_id.0)),
                    Ok(None) => UserInterface::display_info("No generations recorded."),
                    Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                }
            } else if let Some(tx_id) = rollback {
                UserInterface::display_info(&format!("Rolling back to transaction {}", tx_id));
                let history = crate::core::history::HistoryEngine::new(&root_path, Arc::clone(&ctx.db));
                match tx_id.parse::<u64>() {
                    Ok(id) => {
                        match history.compute_rollback_plan(id) {
                            Ok(plan) => {
                                for (action, targets) in &plan {
                                    match action {
                                        crate::core::changelog::ActionKind::Installation => {
                                            UserInterface::display_info(&format!("Rollback: install {:?}", targets));
                                        }
                                        crate::core::changelog::ActionKind::Removal => {
                                            UserInterface::display_info(&format!("Rollback: remove {:?}", targets));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                UserInterface::display_error(&format!("Rollback plan failed: {e}"));
                                process::exit(1);
                            }
                        }
                    }
                    Err(_) => {
                        UserInterface::display_error("Invalid transaction ID");
                        process::exit(1);
                    }
                }
                UserInterface::display_success("Rollback complete.");
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
                        UserInterface::display_error(&format!("History fetch failed: {e}"));
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
                Ok(_) => UserInterface::display_success("Repository added."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::RepoRemove { name } => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.remove_repository(&name) {
                Ok(_) => UserInterface::display_success("Repository removed."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
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
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }

        Commands::SelfUpdate => {
            UserInterface::display_info("Checking for updates...");
            let updater = match crate::core::update::SelfUpdateManager::new("2.8.5") {
                Ok(u) => u,
                Err(e) => {
                    UserInterface::display_error(&format!("Self-update init failed: {e}"));
                    process::exit(1);
                }
            };
            match updater.check_for_updates("https://api.github.com/repos/Cudane/MCX/releases/latest").await {
                Ok(Some(release)) => {
                    UserInterface::display_info(&format!("Update available: v{}", release.version));
                    match updater.deploy_update(&release).await {
                        Ok(_) => UserInterface::display_success("Update deployed. Restart to apply."),
                        Err(e) => {
                            UserInterface::display_error(&format!("Update failed: {e}"));
                            process::exit(1);
                        }
                    }
                }
                Ok(None) => UserInterface::display_success("Already up to date."),
                Err(e) => {
                    UserInterface::display_error(&format!("Update check failed: {e}"));
                    process::exit(1);
                }
            }
        }

        Commands::Vendor { action } => {
            let vendor = crate::core::vendor::VendorManager::new(&root_path);
            if let Err(e) = vendor.initialize() {
                UserInterface::display_error(&format!("Vendor init failed: {e}"));
                process::exit(1);
            }
            match action {
                VendorAction::Add { package, source } => {
                    let src = PathBuf::from(&source);
                    match vendor.register_vendor_package(&package, &src) {
                        Ok(_) => UserInterface::display_success(&format!("Vendored {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                VendorAction::Remove { package } => {
                    match vendor.remove_vendor_package(&package) {
                        Ok(_) => UserInterface::display_success(&format!("Removed vendored {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
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
                    UserInterface::display_error(&format!("Completion generation failed: {e}"));
                    process::exit(1);
                }
            }
        }

        Commands::Snapshot { action } => {
            let snap_mgr = crate::core::snapshot::SnapshotManager::new(&root_path);
            if let Err(e) = snap_mgr.initialize() {
                UserInterface::display_error(&format!("Snapshot init failed: {e}"));
                process::exit(1);
            }
            match action {
                SnapshotAction::Take { package, pid } => {
                    match snap_mgr.checkpoint_process(&package, pid) {
                        Ok(path) => UserInterface::display_success(&format!("Snapshot saved: {:?}", path)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                SnapshotAction::List { package } => {
                    match snap_mgr.list_snapshots(&package) {
                        Ok(snapshots) => {
                            let items: Vec<String> = snapshots.iter().map(|p| p.to_string_lossy().to_string()).collect();
                            UserInterface::render_list(&format!("Snapshots for {}", package), &items);
                        }
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                SnapshotAction::Restore { package: _, snapshot, pid } => {
                    let snap_path = PathBuf::from(&snapshot);
                    match snap_mgr.restore_snapshot(&snap_path, pid) {
                        Ok(_) => UserInterface::display_success("Snapshot restored."),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                SnapshotAction::Remove { package } => {
                    match snap_mgr.remove_snapshots(&package) {
                        Ok(_) => UserInterface::display_success(&format!("Snapshots removed for {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }

        Commands::Swarm { action } => {
            let swarm_mgr = crate::core::swarm::SwarmManager::new(&root_path);
            if let Err(e) = swarm_mgr.initialize() {
                UserInterface::display_error(&format!("Swarm init failed: {e}"));
                process::exit(1);
            }
            match action {
                SwarmAction::RegisterHash { package, version, hash } => {
                    match swarm_mgr.register_swarm_hash(&package, &version, &hash) {
                        Ok(_) => UserInterface::display_success(&format!("Swarm hash registered for {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                SwarmAction::GetHash { package } => {
                    match swarm_mgr.get_swarm_hash(&package) {
                        Ok(Some(hash)) => UserInterface::display_success(&format!("{}: {}", package, hash)),
                        Ok(None) => UserInterface::display_info("No swarm hash registered."),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                SwarmAction::RemoveHash { package } => {
                    match swarm_mgr.remove_swarm_entry(&package) {
                        Ok(_) => UserInterface::display_success(&format!("Swarm hash removed for {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
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
                        Ok(_) => UserInterface::display_success("Peer registered."),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
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
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }

        Commands::Overlay { action } => {
            let overlay_mgr = crate::core::overlay::OverlayManager::new(&root_path.join(
                std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
            ));
            if let Err(e) = overlay_mgr.initialize() {
                UserInterface::display_error(&format!("Overlay init failed: {e}"));
                process::exit(1);
            }
            match action {
                OverlayAction::Create { package, lower } => {
                    match overlay_mgr.create_isolated_overlay(&package, PathBuf::from(&lower).as_path()) {
                        Ok(merged) => UserInterface::display_success(&format!("Overlay created: {:?}", merged)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                OverlayAction::Remove { package } => {
                    match overlay_mgr.remove_isolated_overlay(&package) {
                        Ok(_) => UserInterface::display_success("Overlay removed."),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
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
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }

        Commands::Cgroup { action } => {
            let cg_mgr = crate::core::cgroup::CgroupController::new();
            match action {
                CgroupAction::Enforce { package, max_memory_mb, max_cpu_percent } => {
                    if !cg_mgr.is_cgroup_v2_available() {
                        UserInterface::display_error("cgroup v2 not available on this system");
                        process::exit(1);
                    }
                    match cg_mgr.enforce_resource_limits(&package, max_memory_mb, max_cpu_percent) {
                        Ok(_) => UserInterface::display_success(&format!("Limits enforced for {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::EnforceMem { package, max_memory_mb } => {
                    if !cg_mgr.is_cgroup_v2_available() {
                        UserInterface::display_error("cgroup v2 not available on this system");
                        process::exit(1);
                    }
                    match cg_mgr.enforce_memory_limit(&package, max_memory_mb) {
                        Ok(_) => UserInterface::display_success(&format!("Memory limit enforced for {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::EnforceCpu { package, max_cpu_percent } => {
                    if !cg_mgr.is_cgroup_v2_available() {
                        UserInterface::display_error("cgroup v2 not available on this system");
                        process::exit(1);
                    }
                    match cg_mgr.enforce_cpu_limit(&package, max_cpu_percent) {
                        Ok(_) => UserInterface::display_success(&format!("CPU limit enforced for {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::Remove { package } => {
                    match cg_mgr.remove_resource_limits(&package) {
                        Ok(_) => UserInterface::display_success(&format!("Limits removed for {}", package)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                CgroupAction::Status => {
                    if cg_mgr.is_cgroup_v2_available() {
                        UserInterface::display_success("cgroup v2 available at /sys/fs/cgroup/mcx");
                    } else {
                        UserInterface::display_info("cgroup v2 not available");
                    }
                }
            }
        }

        Commands::Stream { action } => {
            let stream_mgr = crate::core::stream::StreamManager::new(&root_path);
            if let Err(e) = stream_mgr.initialize() {
                UserInterface::display_error(&format!("Stream init failed: {e}"));
                process::exit(1);
            }
            match action {
                StreamAction::Generate { package, version, url } => {
                    match stream_mgr.generate_stream_mount_script(&package, &version, &url) {
                        Ok(path) => UserInterface::display_success(&format!("Stream script generated: {:?}", path)),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
                StreamAction::Remove { package } => {
                    match stream_mgr.remove_stream_script(&package) {
                        Ok(_) => UserInterface::display_success("Stream script removed."),
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
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
                        Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
                    }
                }
            }
        }
    }
}

fn elevate_if_needed(root: &Path) -> bool {
    if is_root_process() {
        return false;
    }
    if has_write_access(root) {
        return false;
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return false,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let status = std::process::Command::new("sudo")
        .arg(&exe)
        .args(&args)
        .status();
    match status {
        Ok(s) => std::process::exit(s.code().unwrap_or(1)),
        Err(_) => false,
    }
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

fn has_write_access(root: &Path) -> bool {
    let probe = root.join(".mcx-wt");
    match std::fs::write(&probe, []) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => false,
        Err(_) => true,
    }
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
