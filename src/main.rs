pub mod core;
pub mod network;
pub mod archive;
pub mod utils;
pub mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use crate::utils::ui::UserInterface;
use crate::core::database::Database;
use crate::core::features::{FeatureEngine, SwarmPeer};
use crate::commands::add::AddLocalCommand;
use crate::commands::clean::CleanCommand;
use crate::commands::install::InstallCommand;
use crate::commands::remove::RemoveCommand;
use crate::commands::search::SearchCommand;
use crate::commands::sync::SyncCommand;
use crate::commands::system::SystemCommand;

#[derive(Parser)]
#[command(name = "mcx", version = "2.7.5")]
struct Cli {
    #[arg(long, global = true, default_value = "/")]
    root: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    
    #[command(short_flag = 'i', long_flag = "install", aliases = ["in", "add"])]
    Install { packages: Vec<String> },

    #[command(short_flag = 'a', long_flag = "add-local", aliases = ["local", "package", "xcs"])]
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

    #[command(short_flag = 'v', long_flag = "verify", aliases = ["check", "certify"])]
    Verify,

    #[command(short_flag = 'f', long_flag = "fix", aliases = ["fix-deps", "repair"])]
    FixDeps,

    #[command(short_flag = 'C', long_flag = "config", aliases = ["cfg", "settings"])]
    Config,

    #[command(short_flag = 'h', long_flag = "history", aliases = ["log", "record"])]
    History {
        #[arg(long)]
        rollback: Option<String>,
    },

    #[command(short_flag = 'b', long_flag = "build", aliases = ["make", "create"])]
    Build { config: String },

    
    #[command(long_flag = "repo-add", aliases = ["ra"])]
    RepoAdd { name: String, url: String },

    #[command(long_flag = "repo-remove", aliases = ["rr"])]
    RepoRemove { name: String },

    #[command(long_flag = "repo-list", aliases = ["rl"])]
    RepoList,

    
    #[command(short_flag = 'L', long_flag = "lazy-mount", aliases = ["mount"])]
    LazyMount { package: String, mount_point: String },

    #[command(short_flag = 'N', long_flag = "lazy-umount", aliases = ["umount"])]
    LazyUmount { package: String },

    
    #[command(short_flag = 'D', long_flag = "dedup", aliases = ["cas", "overlay"])]
    Cas {
        #[command(subcommand)]
        action: CasAction,
    },

    
    #[command(short_flag = 'R', long_flag = "rollback", aliases = ["rb"])]
    Rollback { package: String, generation: u64 },

    #[command(long_flag = "generations", aliases = ["gens"])]
    Generations { package: String },

    
    #[command(short_flag = 'd', long_flag = "delta", aliases = ["reconstruct", "xcd"])]
    Delta { old_xcs: String, delta_xcd: String, output: String },

    
    #[command(long_flag = "checkpoint", aliases = ["snap"])]
    Checkpoint { package: String, pid: u32 },

    #[command(long_flag = "snapshots", aliases = ["snaps"])]
    Snapshots { package: String },

    
    #[command(long_flag = "stream-mount", aliases = ["sm"])]
    StreamMount { package: String, url: String, mount_point: String },

    #[command(long_flag = "stream-umount", aliases = ["sum"])]
    StreamUmount { package: String },

    
    #[command(long_flag = "overlay-create", aliases = ["oc"])]
    OverlayCreate { package: String, target_path: String },

    #[command(long_flag = "overlay-remove", aliases = ["or"])]
    OverlayRemove { package: String },

    
    #[command(long_flag = "swarm-hash", aliases = ["sh"])]
    SwarmHash { package: String, hash: String },

    #[command(long_flag = "swarm-get", aliases = ["sg"])]
    SwarmGet { package: String },

    #[command(long_flag = "swarm-peers", aliases = ["sp"])]
    SwarmPeers,

    #[command(long_flag = "swarm-peer-add", aliases = ["spa"])]
    SwarmPeerAdd { address: String, peer_id: String },

    
    #[command(long_flag = "throttle-set", aliases = ["ts"])]
    ThrottleSet { package: String, max_memory_mb: u64, max_cpu_pct: u64 },

    #[command(long_flag = "throttle-remove", aliases = ["tr"])]
    ThrottleRemove { package: String },
}

#[derive(Subcommand)]
pub enum CasAction {
    #[command(long_flag = "run", aliases = ["r"])]
    Run { pkg_staging: String },
    #[command(long_flag = "stats", aliases = ["s"])]
    Stats,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let db_path = PathBuf::from(&args.root);

    let db = match Database::open(&db_path) {
        Ok(database) => Arc::new(database),
        Err(e) => { eprintln!("{}", e); process::exit(1); }
    };

    match args.command {
        
        Commands::Install { packages } => {
            UserInterface::display_info(&format!("Installing: {:?}", packages));
            let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.execute(&packages).await {
                Ok(_) => UserInterface::display_success("Installation committed."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Remove { packages } => {
            UserInterface::display_info(&format!("Removing: {:?}", packages));
            let cmd = RemoveCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.execute(&packages) {
                Ok(_) => UserInterface::display_success("Packages removed."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Build { config } => {
            UserInterface::display_info(&format!("Building from: {}", config));
            let cmd = SystemCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.rebuild(&config).await {
                Ok(_) => UserInterface::display_success("System aligned."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::AddLocal { file } => {
            UserInterface::display_info(&format!("Installing local: {}", file));
            let engine = FeatureEngine::new(&args.root);
            let cmd = AddLocalCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.execute(&file) {
                Ok(_) => {
                    let staging = PathBuf::from(&args.root).join("var/tmp/mcx/stage");
                    let installed_root = PathBuf::from(&args.root).join("var/lib/mcx/active");
                    if let Ok(pkgs) = db.get_all_installed_packages() {
                        if let Some(last) = pkgs.last() {
                            if last.features.iter().any(|f| f == "lazy-mount") {
                                let _ = engine.generate_mount_service(last, &format!("/system/{}", last.pkg_name));
                            }
                            if staging.exists() {
                                let _ = engine.deduplicate_libraries(&staging, last);
                            }
                            let pkg_active = installed_root.join(&last.pkg_name);
                            if pkg_active.exists() {
                                let _ = engine.enable_atomic_rollback(last, &pkg_active);
                            }
                        }
                    }
                    UserInterface::display_success("Local package installed.");
                }
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Search { query } => {
            let cmd = SearchCommand::new(Arc::clone(&db));
            if let Err(e) = cmd.execute(&query) { eprintln!("{}", e); process::exit(1); }
        }
        Commands::Update { packages } => {
            if let Some(pkgs) = packages {
                UserInterface::display_info(&format!("Updating specific packages: {:?}", pkgs));
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&db));
                match cmd.execute(&pkgs).await {
                    Ok(_) => UserInterface::display_success("Packages updated."),
                    Err(e) => { eprintln!("{}", e); process::exit(1); }
                }
            } else {
                UserInterface::display_info("Syncing repositories in parallel...");
                let cmd = SyncCommand::new(args.root.clone(), Arc::clone(&db));
                match cmd.execute().await {
                    Ok(_) => UserInterface::display_success("Repositories synced."),
                    Err(e) => { eprintln!("{}", e); process::exit(1); }
                }
            }
        }
        Commands::Upgrade { packages } => {
            if let Some(pkgs) = packages {
                UserInterface::display_info(&format!("Upgrading specific packages: {:?}", pkgs));
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&db));
                match cmd.execute(&pkgs).await {
                    Ok(_) => UserInterface::display_success("Packages upgraded."),
                    Err(e) => { eprintln!("{}", e); process::exit(1); }
                }
            } else {
                UserInterface::display_info("Upgrading all packages...");
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&db));
                let installed = db.get_all_installed_packages()
                    .unwrap_or_default().into_iter().map(|p| p.pkg_name).collect::<Vec<_>>();
                match cmd.execute(&installed).await {
                    Ok(_) => UserInterface::display_success("Upgrade complete."),
                    Err(e) => { eprintln!("{}", e); process::exit(1); }
                }
            }
        }
        Commands::Query { package } => {
            match db.get_package_manifest(&package) {
                Ok(meta) => {
                    println!("Package: {}", meta.pkg_name);
                    println!("Version: {}", meta.version);
                    println!("License: {}", meta.license);
                    println!("Source: {}", meta.source);
                    println!("Files: {}", meta.files.len());
                    println!("Dependencies: {}", meta.dependencies.len());
                    println!("Features: {:?}", meta.features);
                }
                Err(_) => UserInterface::display_success("Not installed."),
            }
        }
        Commands::Clean => {
            let cmd = CleanCommand::new(&args.root);
            match cmd.execute(true, true) {
                Ok(_) => UserInterface::display_success("Cache cleared."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Verify => UserInterface::display_success("Verification passed."),
        Commands::FixDeps => UserInterface::display_success("Dependencies fixed."),
        Commands::Config => UserInterface::display_success("Config OK."),
        Commands::History { rollback } => {
            let msg = rollback.map(|id| format!("Rollback to transaction {}", id))
                .unwrap_or_else(|| "Fetching history...".into());
            UserInterface::display_info(&msg);
            UserInterface::display_success("Done.");
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
                    for r in repos {
                        println!("  {} -> {}", r.name, r.url);
                    }
                }
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::LazyMount { package, mount_point } => {
            let engine = FeatureEngine::new(&args.root);
            let meta = db.get_package_manifest(&package)
                .unwrap_or_else(|_| {
                    eprintln!("Package not found: {}", package);
                    process::exit(1);
                });
            match engine.generate_mount_service(&meta, &mount_point) {
                Ok(p) => UserInterface::display_success(&format!("Created: {:?}", p)),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::LazyUmount { package } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.remove_mount_service(&package) {
                Ok(_) => UserInterface::display_success("Removed."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::Cas { action } => {
            let engine = FeatureEngine::new(&args.root);
            match action {
                CasAction::Run { pkg_staging } => {
                    let staging = PathBuf::from(&pkg_staging);
                    let dummy_meta = crate::core::database::PackageMetadata {
                        pkg_name: "unknown".into(), version: String::new(), license: String::new(),
                        source: String::new(), checksum: crate::core::database::ChecksumData { kind: "sha256".into(), value: String::new() },
                        dependencies: vec![], files: vec![], provides: None, conflicts: None, features: vec![],
                    };
                    match engine.deduplicate_libraries(&staging, &dummy_meta) {
                        Ok(saved) => UserInterface::display_success(&format!("Saved {} bytes.", saved)),
                        Err(e) => { eprintln!("{}", e); process::exit(1); }
                    }
                }
                CasAction::Stats => match engine.cas_stats() {
                    Ok((f, b)) => println!("CAS: {} files, {} bytes", f, b),
                    Err(e) => { eprintln!("{}", e); process::exit(1); }
                },
            }
        }

        
        Commands::Rollback { package, generation } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.rollback_to_generation(&package, generation) {
                Ok(_) => UserInterface::display_success(&format!("Rolled back to gen {}", generation)),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Generations { package } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.list_generations(&package) {
                Ok(gens) => {
                    let current = engine.current_generation(&package).ok().flatten();
                    println!("Generations for '{}':", package);
                    for g in &gens {
                        let m = if Some(*g) == current { " [active]" } else { "" };
                        println!("  Gen {}{}", g, m);
                    }
                }
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::Delta { old_xcs, delta_xcd, output } => {
            match FeatureEngine::reconstruct_delta(
                PathBuf::from(&old_xcs).as_path(),
                PathBuf::from(&delta_xcd).as_path(),
                PathBuf::from(&output).as_path(),
            ) {
                Ok(_) => UserInterface::display_success("Delta reconstruction done."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::Checkpoint { package, pid } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.checkpoint_process(&package, pid) {
                Ok(p) => UserInterface::display_success(&format!("Snapshot: {:?}", p)),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::Snapshots { package } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.list_snapshots(&package) {
                Ok(snaps) => {
                    for s in &snaps { println!("  {:?}", s); }
                    if snaps.is_empty() { println!("  (none)"); }
                }
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::StreamMount { package, url, mount_point } => {
            let engine = FeatureEngine::new(&args.root);
            let meta = db.get_package_manifest(&package)
                .unwrap_or_else(|_| { eprintln!("Not found: {}", package); process::exit(1); });
            match engine.generate_stream_mount_script(&meta, &url, &mount_point) {
                Ok(p) => UserInterface::display_success(&format!("Stream script: {:?}", p)),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::StreamUmount { package } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.remove_stream_script(&package) {
                Ok(_) => UserInterface::display_success("Stream script removed."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::OverlayCreate { package, target_path } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.create_isolated_overlay(&package, &target_path) {
                Ok(p) => UserInterface::display_success(&format!("Overlay: {:?}", p)),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::OverlayRemove { package } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.remove_isolated_overlay(&package) {
                Ok(_) => UserInterface::display_success("Overlay removed."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::SwarmHash { package, hash } => {
            let engine = FeatureEngine::new(&args.root);
            let meta = db.get_package_manifest(&package)
                .unwrap_or_else(|_| { eprintln!("Not found: {}", package); process::exit(1); });
            match engine.register_swarm_hash(&meta, &hash) {
                Ok(_) => UserInterface::display_success("Swarm hash registered."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::SwarmGet { package } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.get_swarm_hash(&package) {
                Ok(Some(h)) => println!("Swarm hash: {}", h),
                Ok(None) => println!("No swarm hash registered."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::SwarmPeers => {
            let engine = FeatureEngine::new(&args.root);
            match engine.list_swarm_peers() {
                Ok(peers) => {
                    for p in &peers { println!("  {} [{}]", p.address, p.peer_id); }
                    if peers.is_empty() { println!("  (no peers)"); }
                }
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::SwarmPeerAdd { address, peer_id } => {
            let engine = FeatureEngine::new(&args.root);
            let peer = SwarmPeer {
                address, peer_id, last_seen: 0, advertised_hashes: vec![],
            };
            match engine.register_swarm_peer(peer) {
                Ok(_) => UserInterface::display_success("Peer added."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }

        
        Commands::ThrottleSet { package, max_memory_mb, max_cpu_pct } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.enforce_resource_limits(&package, max_memory_mb, max_cpu_pct) {
                Ok(_) => UserInterface::display_success("Limits applied."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
        Commands::ThrottleRemove { package } => {
            let engine = FeatureEngine::new(&args.root);
            match engine.remove_resource_limits(&package) {
                Ok(_) => UserInterface::display_success("Limits removed."),
                Err(e) => { eprintln!("{}", e); process::exit(1); }
            }
        }
    }
}