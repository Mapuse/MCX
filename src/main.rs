pub mod core;
pub mod network;
pub mod archive;
pub mod utils;
pub mod commands;

use std::collections::HashMap;
use std::path::Path;

use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use crate::utils::ui::UserInterface;
use crate::core::database::Database;
use crate::core::config::ConfigManager;
use crate::core::constants;
use crate::core::profiler::{SystemProfile, DecisionEngine, NetworkProber};
use crate::core::plugin::{PluginManager, PluginHook, PluginEvent};
use crate::commands::add::AddLocalCommand;
use crate::commands::clean::CleanCommand;
use crate::commands::configuration::{ConfigEditorCommand, ConfigTarget};
use crate::core::component::ComponentFilter;
use crate::commands::install::InstallCommand;
use crate::commands::remove::RemoveCommand;
use crate::commands::search::SearchCommand;
use crate::commands::service::ServiceCommand;
use crate::commands::sync::SyncCommand;
use crate::commands::system::SystemCommand;
use crate::core::gitpkg::GitPackageManager;

struct UiReporter;

impl cps::Reporter for UiReporter {
    fn info(&self, msg: &str) {
        UserInterface::info(msg);
    }
    fn warning(&self, msg: &str) {
        UserInterface::warning(msg);
    }
    fn error(&self, msg: &str) {
        UserInterface::error(msg);
    }
}

#[derive(Parser)]
    #[command(name = constants::APP_NAME, version = constants::APP_VERSION, disable_version_flag = true)]
struct Cli {
    #[arg(long, global = true, default_value = "")]
    root: String,
    #[arg(long = "user-mode", global = true, conflicts_with = "system_mode_flag")]
    user_mode_flag: bool,
    #[arg(long = "system-mode", global = true)]
    system_mode_flag: bool,
    #[arg(long = "version", short = 'v', help = "Print version")]
    version: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(short_flag = 'i', long_flag = "install", aliases = ["in", "add"])]
    Install {
        packages: Vec<String>,
        #[arg(long, help = "Install only required components")]
        minimal: bool,
        #[arg(long, help = "Install all components including development")]
        dev: bool,
        #[arg(long, help = "Install specific components (comma-separated)")]
        components: Option<String>,
        #[arg(long, help = "Exclude specific components (comma-separated)")]
        exclude: Option<String>,
        #[arg(long, help = "Install only the specific binary/tool from the package")]
        only: Option<String>,
    },

    #[command(short_flag = 'a', long_flag = "add", aliases = ["local", "package", "xcs"])]
    AddLocal { file: String },

    #[command(short_flag = 'r', long_flag = "remove", aliases = ["rm", "uninstall", "delete"])]
    Remove {
        packages: Vec<String>,
        #[arg(long, help = "Remove specific components only (comma-separated)")]
        components: Option<String>,
        #[arg(long, help = "Purge broken files and caches")]
        purge_broken: bool,
    },

    #[command(long_flag = "purge", aliases = ["full-remove"])]
    Purge {
        packages: Vec<String>,
    },

    #[command(short_flag = 's', long_flag = "search", aliases = ["find", "look"])]
    Search { query: String },

    #[command(short_flag = 'u', long_flag = "update", aliases = ["refresh", "sync"])]
    Update { packages: Option<Vec<String>> },

    #[command(short_flag = 'U', long_flag = "upgrade", aliases = ["up", "dist-upgrade"])]
    Upgrade {
        packages: Option<Vec<String>>,
        #[arg(long, help = "Upgrade only specific components (comma-separated)")]
        components: Option<String>,
        #[arg(long, help = "Upgrade only the specific binary/tool")]
        only: Option<String>,
    },

    #[command(short_flag = 'q', long_flag = "query", aliases = ["info", "show"])]
    Query { package: String },

    #[command(short_flag = 'c', long_flag = "clean", aliases = ["wipe", "clear"])]
    Clean,

    #[command(short_flag = 'V', long_flag = "verify", aliases = ["check", "certify"])]
    Verify,

    #[command(short_flag = 'f', long_flag = "fix", aliases = ["repair"])]
    FixDeps,

    #[command(short_flag = 'C', long_flag = "config", aliases = ["cfg", "settings"])]
    Config {
        #[arg(long, help = "Generate default config.ini, repo.ini, and profile.ini")]
        init: bool,
    },

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

    #[command(long_flag = "repo-sync", aliases = ["rs"])]
    RepoSync { name: String },

    #[command(long_flag = "repo-enable", aliases = ["re"])]
    RepoEnable { name: String },

    #[command(long_flag = "repo-disable", aliases = ["rd"])]
    RepoDisable { name: String },

    #[command(long_flag = "repo-info", aliases = ["ri"])]
    RepoInfo { name: String },

    #[command(long_flag = "self-update", aliases = ["update-self"])]
    SelfUpdate,

    #[command(long_flag = "vendor", aliases = ["vnd"])]
    Vendor {
        #[command(subcommand)]
        action: VendorAction,
    },

    #[command(long_flag = "completion", aliases = ["comp"])]
    Completion { shell: String },

    #[command(long_flag = "cgroup", aliases = ["cg"])]
    Cgroup {
        #[command(subcommand)]
        action: CgroupAction,
    },

    #[command(long_flag = "hook-plugin", aliases = ["hp"])]
    HookPlugin {
        #[command(subcommand)]
        action: HookPluginAction,
    },

    #[command(subcommand)]
    Plugin(PluginCommand),

    #[command(subcommand)]
    Theme(ThemeCommand),

    #[command(subcommand)]
    Tui(TuiCommand),

    #[command(long_flag = "service", aliases = ["svc"])]
    Service {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    #[command(long_flag = "command-not-found", aliases = ["cnf"])]
    CommandNotFound {
        command: String,
    },

    #[command(long_flag = "binindex", aliases = ["bi"])]
    BinIndex,

    #[command(long_flag = "autoremove", aliases = ["ar", "autoclean"])]
    AutoRemove {
        #[arg(long, help = "Actually remove orphaned packages (default: dry-run)")]
        apply: bool,
    },

    #[command(long_flag = "mode", aliases = ["toggle", "mode-toggle"])]
    Mode {
        #[arg(value_parser = ["user", "system", "status"])]
        action: String,
    },
}

#[derive(Subcommand)]
pub enum VendorAction {
    Add { package: String, source: String },
    Remove { package: String },
    List,
}

#[derive(Subcommand)]
pub enum HookPluginAction {
    List,
    Info { name: String },
    Run { name: String, hook: Option<String> },
    Reload,
    ReloadConfig,
    Add { source: String },
    Remove { name: String },
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
pub enum PluginCommand {
    List,
    Run(PluginRunArgs),
    Install(PluginInstallArgs),
    Remove(PluginRemoveArgs),
    Info(PluginInfoArgs),
}

#[derive(clap::Args)]
pub struct PluginRunArgs {
    pub alias: String,
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    pub args: Vec<String>,
}

#[derive(clap::Args)]
pub struct PluginInstallArgs {
    pub path: String,
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    #[arg(short = 'a', long = "alias")]
    pub alias: Option<String>,
    #[arg(short = 'A', long = "aliases", value_parser = parse_key_val)]
    pub aliases: Vec<(String, String)>,
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(clap::Args)]
pub struct PluginRemoveArgs {
    pub name: String,
}

#[derive(clap::Args)]
pub struct PluginInfoArgs {
    pub name: String,
}

#[derive(Subcommand)]
pub enum ThemeCommand {
    List,
    Apply(ThemeApplyArgs),
    Install(ThemeInstallArgs),
    Remove(ThemeRemoveArgs),
    Info(ThemeInfoArgs),
}

#[derive(clap::Args)]
pub struct ThemeApplyArgs {
    pub name: String,
}

#[derive(clap::Args)]
pub struct ThemeInstallArgs {
    pub path: String,
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(clap::Args)]
pub struct ThemeRemoveArgs {
    pub name: String,
}

#[derive(clap::Args)]
pub struct ThemeInfoArgs {
    pub name: String,
}

#[derive(Subcommand)]
pub enum TuiCommand {
    List,
    Apply(TuiApplyArgs),
    Install(TuiInstallArgs),
    Remove(TuiRemoveArgs),
    Info(TuiInfoArgs),
}

#[derive(clap::Args)]
pub struct TuiApplyArgs {
    pub name: String,
}

#[derive(clap::Args)]
pub struct TuiInstallArgs {
    pub path: String,
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(clap::Args)]
pub struct TuiRemoveArgs {
    pub name: String,
}

#[derive(clap::Args)]
pub struct TuiInfoArgs {
    pub name: String,
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let mut parts = s.splitn(2, '=');
    let key = parts.next().ok_or("missing key")?.to_string();
    let val = parts.next().ok_or("missing value")?.to_string();
    Ok((key, val))
}

struct EngineContext {
    db: Arc<Database>,
    config_mgr: ConfigManager,
    plugin_mgr: Arc<PluginManager>,
    sys_profile: SystemProfile,
    lifecycle: crate::core::lifecycle::LifecycleEngine,
}

impl EngineContext {
    fn new(root: &PathBuf) -> Self {
        let sys_profile = SystemProfile::probe();

        let config_mgr = ConfigManager::new(root)
            .unwrap_or_else(|e| { UserInterface::error(&format!("Config error: {}", e)); process::exit(1); });

        let plugin_mgr = Arc::new(PluginManager::new(root));

        let db = match Database::open(root) {
            Ok(database) => Arc::new(database),
            Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
        };

        let lifecycle = crate::core::lifecycle::LifecycleEngine::new_with_root(root);

        Self { db, config_mgr, plugin_mgr, sys_profile, lifecycle }
    }
}

fn record_lifecycle_install(ctx: &mut EngineContext, packages: &[String]) {
    for pkg in packages {
        if let Ok(meta) = ctx.db.get_package_manifest(pkg) {
            if ctx.lifecycle.state(pkg).is_none() {
                ctx.lifecycle.register_package(pkg, &meta.version);
            }
            for target in [
                crate::core::lifecycle::PackageState::Resolved,
                crate::core::lifecycle::PackageState::Staged,
                crate::core::lifecycle::PackageState::Installed,
                crate::core::lifecycle::PackageState::Active,
            ] {
                match ctx.lifecycle.transition(pkg, target) {
                    Ok(_) => {}
                    Err(e) => {
                        // Already past this stage (e.g. reinstall): stop walking.
                        UserInterface::warning(&format!("Lifecycle: {e}"));
                        break;
                    }
                }
            }
        }
    }
}

fn record_lifecycle_remove(ctx: &mut EngineContext, packages: &[String]) {
    for pkg in packages {
        if ctx.lifecycle.state(pkg).is_none() {
            ctx.lifecycle.register_package(pkg, "");
        }
        for target in [
            crate::core::lifecycle::PackageState::MarkedForRemoval,
            crate::core::lifecycle::PackageState::Removed,
            crate::core::lifecycle::PackageState::Purged,
        ] {
            match ctx.lifecycle.transition(pkg, target) {
                Ok(_) => {}
                Err(e) => {
                    UserInterface::warning(&format!("Lifecycle: {e}"));
                    break;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    cps::configure(cps::Options::new("mcx").with_reporter(Arc::new(UiReporter)));

    let mut args = Cli::parse();
    if args.version {
        UserInterface::version(&format!("{} {}", constants::APP_NAME, constants::APP_VERSION));
        return;
    }

    let mode_flag = if args.user_mode_flag {
        Some(true)
    } else if args.system_mode_flag {
        Some(false)
    } else {
        None
    };

    // The mode command is resolved before any root is selected: it reads and
    // persists the toggle in the user's home config and never needs the
    // engine context.
    let mode_action = match &args.command {
        Commands::Mode { action } => Some(action.clone()),
        _ => None,
    };
    if let Some(action) = &mode_action {
        handle_mode_command(action, mode_flag);
        return;
    }

    let mode = crate::core::mode::resolve_mode(mode_flag);
    let selection = crate::core::mode::read_user_selection();
    let mode_cfg = crate::core::mode::read_mode_config();
    if crate::core::mode::user_selection_path().exists() && selection.is_none() {
        UserInterface::warning(&format!(
            "Invalid user selection config {:?}; falling back to system mode.",
            crate::core::mode::user_selection_path().display()
        ));
    }
    if let Err(e) = crate::core::mode::ensure_mode_fields() {
        UserInterface::warning(&format!("Could not ensure configurable settings in user config: {e}"));
    }
    let explicit_root = if args.root.is_empty() {
        None
    } else {
        Some(args.root.as_str())
    };
    let read_only = subcommand_is_read_only(&args.command);
    let root_path = crate::core::mode::resolve_root(explicit_root, mode, &mode_cfg, read_only);
    if mode.is_user() && let Some(requested) = explicit_root {
        let requested = crate::core::mode::normalize_root(Path::new(requested));
        if requested != root_path {
            UserInterface::warning(&format!(
                "User mode always operates inside ~/.mcx ({}); ignoring --root {}.",
                root_path.display(),
                requested.display()
            ));
        }
    }
    args.root = root_path.to_string_lossy().into_owned();
    let root_path = PathBuf::from(&args.root);

    crate::core::sudo::root_access(&root_path, &mode);

    let mut ctx = EngineContext::new(&root_path);

    let _ = &ctx.config_mgr;
    let security_mon = Arc::new(crate::core::security::SecurityMonitor::new());
    let cgroup_mgr = crate::core::cgroup::CgroupController::new();

    if let Err(e) = expand_wildcards(&mut args.command, &ctx.db) {
        UserInterface::error(&format!("{}", e));
        process::exit(1);
    }

    match args.command {
        Commands::Install { packages, minimal, dev, components, exclude, only } => {
            UserInterface::info(&format!("Installing: {:?}", packages));
            let decisions = DecisionEngine::evaluate_thread_strategy(
                &ctx.sys_profile,
                &NetworkProber::probe(constants::MAIN_REPO_URL, std::time::Duration::from_secs(constants::PROBE_TIMEOUT_SECS)).await,
                &Default::default(),
            );
            if DecisionEngine::should_use_parallel(&decisions) {
                UserInterface::info("Parallel install strategy selected");
            }

            let filter = if let Some(ref binary_name) = only {
                let mut resolved = ComponentFilter { minimal: false, include_dev: false, include: Vec::new(), exclude: Vec::new() };
                for pkg_name in &packages {
                    if let Ok(meta) = ctx.db.get_package_manifest(pkg_name) {
                        if let Some(comp) = meta.find_component_for_binary(binary_name) {
                            UserInterface::info(&format!("'{}' is in component '{}' of {}", binary_name, comp.name, pkg_name));
                            resolved.include.push(comp.name.clone());
                        } else {
                            UserInterface::warning(&format!("Binary '{}' not found in package '{}'; installing all components", binary_name, pkg_name));
                        }
                    }
                }
                resolved
            } else if minimal {
                ComponentFilter { minimal: true, include_dev: false, include: Vec::new(), exclude: Vec::new() }
            } else if dev {
                ComponentFilter { minimal: false, include_dev: true, include: Vec::new(), exclude: Vec::new() }
            } else if let Some(ref comp_str) = components {
                let comps: Vec<String> = comp_str.split(',').map(|s| s.trim().to_string()).collect();
                let excl: Vec<String> = exclude.as_ref()
                    .map(|e| e.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default();
                ComponentFilter { minimal: false, include_dev: false, include: comps, exclude: excl }
            } else {
                ComponentFilter::none()
            };

                    let bin_before = crate::core::binindex::scan_system_binaries(&args.root);
            let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db))
                .with_cgroup(cgroup_mgr)
                .with_security(Arc::clone(&security_mon))
                .with_plugin_mgr(Arc::clone(&ctx.plugin_mgr))
                .with_component_filter(filter);
            match cmd.execute(&packages).await {
                Ok(_) => {
                    record_lifecycle_install(&mut ctx, &packages);

                    // CAS dedup must never touch host system directories. When
                    // operating on the real root ("/"), dedup only the shared
                    // libraries inside each package's active mirror instead.
                    let cas = crate::core::cas::CasStore::new(&root_path);
                    let is_host_root = args.root == "/" || args.root.is_empty() || args.root == "/.";
                    if is_host_root {
                        let active_root = root_path.join(constants::PATH_ACTIVE);
                        if let Ok(entries) = std::fs::read_dir(&active_root) {
                            for entry in entries.flatten() {
                                let pkg_dir = entry.path();
                                if pkg_dir.is_dir()
                                    && let Ok(stats) = cas.deduplicate_libraries(&pkg_dir) {
                                        UserInterface::cas(&format!(
                                            "CAS dedup {}: {} unique files, {} bytes saved",
                                            entry.file_name().to_string_lossy(),
                                            stats.unique_files, stats.bytes_saved
                                        ));
                                    }
                            }
                        }
                    } else if let Ok(stats) = cas.deduplicate_libraries(&root_path.join(constants::LIB_DIRS[0])) {
                        UserInterface::cas(&format!(
                            "CAS dedup: {} unique files, {} bytes saved",
                            stats.unique_files, stats.bytes_saved
                        ));
                    }
                    UserInterface::separator();

                    // profile validation after install
                    let profile_path = root_path.join("etc/mcx/profile.ini");
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

                    let new_bins = crate::core::binindex::detect_new_binaries(&args.root, &bin_before);
                    if !new_bins.is_empty() {
                        UserInterface::info(&format!("New binaries available: {}", new_bins.join(", ")));
                    }

                    let binindex = crate::core::binindex::BinaryIndex::new(args.root.clone(), Arc::clone(&ctx.db));
                    if let Ok(count) = binindex.rebuild() {
                        UserInterface::info(&format!("Binary index rebuilt: {} entries", count));
                    }

                    UserInterface::success("Installation committed.");
                    run_autoremove_scan(&ctx.db, &args.root);
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Remove { packages, components, purge_broken } => {
            UserInterface::info(&format!("Removing: {:?}", packages));

            if purge_broken {
                UserInterface::info("Scanning for broken files...");
                let scanner = crate::core::integrity::IntegrityScanner::new(&root_path, Arc::clone(&ctx.db));
                let report = scanner.verify_all();
                let mut broken_files: Vec<String> = Vec::new();
                for mf in &report.missing_files {
                    broken_files.push(mf.path.to_string_lossy().to_string());
                }
                for cf in &report.corrupted_files {
                    broken_files.push(cf.path.to_string_lossy().to_string());
                }
                if !broken_files.is_empty() {
                    UserInterface::info(&format!("Found {} broken files, purging...", broken_files.len()));
                    for f in &broken_files {
                        let abs = root_path.join(f);
                        if abs.exists() { let _ = fs::remove_file(&abs); }
                    }
                } else {
                    UserInterface::success("No broken files found.");
                }
            }

            let cmd = RemoveCommand::new(args.root.clone(), Arc::clone(&ctx.db))
                .with_plugin_mgr(Arc::clone(&ctx.plugin_mgr))
                .with_component_filter(components.as_deref().map(|c| {
                    let comps: Vec<String> = c.split(',').map(|s| s.trim().to_string()).collect();
                    crate::core::component::ComponentFilter { minimal: false, include_dev: false, include: comps, exclude: Vec::new() }
                }));
            match cmd.execute(&packages, &cgroup_mgr, &security_mon) {
                Ok(_) => {
                    record_lifecycle_remove(&mut ctx, &packages);
                    let git_cleanup = GitPackageManager::new(&root_path);
                    for pkg in &packages {
                        let _ = git_cleanup.purge(pkg);
                    }
                    let binindex = crate::core::binindex::BinaryIndex::new(args.root.clone(), Arc::clone(&ctx.db));
                    let _ = binindex.rebuild();
                    UserInterface::separator();
                    UserInterface::security(&format!("Tracking {} active packages", security_mon.active_count()));
                    UserInterface::success("Packages removed.");
                    run_autoremove_scan(&ctx.db, &args.root);
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Purge { packages } => {
            UserInterface::info(&format!("Purging: {:?}", packages));
            let cmd = RemoveCommand::new(args.root.clone(), Arc::clone(&ctx.db))
                .with_plugin_mgr(Arc::clone(&ctx.plugin_mgr));
            match cmd.execute(&packages, &cgroup_mgr, &security_mon) {
                Ok(_) => {
                    record_lifecycle_remove(&mut ctx, &packages);
                    let git_cleanup = GitPackageManager::new(&root_path);
                    for pkg in &packages {
                        let _ = git_cleanup.purge(pkg);
                    }
                    let purge_dirs = vec![
                        root_path.join(constants::PATH_ACTIVE),
                        root_path.join(constants::PATH_CACHE),
                        root_path.join(constants::PATH_TMP),
                    ];
                    for dir in &purge_dirs {
                        for pkg in &packages {
                            let pkg_dir = dir.join(pkg);
                            if pkg_dir.exists()
                                && let Err(e) = fs::remove_dir_all(&pkg_dir)
                            {
                                UserInterface::warning(&format!("Failed to purge {}: {}", pkg_dir.display(), e));
                            }
                        }
                    }
                    let binindex = crate::core::binindex::BinaryIndex::new(args.root.clone(), Arc::clone(&ctx.db));
                    let _ = binindex.rebuild();
                    UserInterface::success("Package fully purged (including caches).");
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
                    let rollback_mgr = crate::core::rollback::RollbackManager::new(&root_path);
                    let gen_root = root_path.join(constants::PATH_ACTIVE);
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
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db))
                    .with_plugin_mgr(Arc::clone(&ctx.plugin_mgr));
                match cmd.execute(&pkgs).await {
                    Ok(_) => {
                        record_lifecycle_install(&mut ctx, &pkgs);
                        UserInterface::success("Packages updated.");
                        run_autoremove_scan(&ctx.db, &args.root);
                    }
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
        Commands::Upgrade { packages, components, only } => {
            let pkgs_to_upgrade: Vec<String> = if let Some(pkgs) = packages {
                pkgs
            } else {
                ctx.db.get_all_installed_packages()
                    .unwrap_or_default().into_iter().map(|p| p.pkg_name).collect()
            };

            UserInterface::info(&format!("Upgrading: {:?}", pkgs_to_upgrade));

            let filter = if let Some(ref binary_name) = only {
                let mut resolved = ComponentFilter { minimal: false, include_dev: false, include: Vec::new(), exclude: Vec::new() };
                for pkg_name in &pkgs_to_upgrade {
                    if let Ok(meta) = ctx.db.get_package_manifest(pkg_name)
                        && let Some(comp) = meta.find_component_for_binary(binary_name) {
                            UserInterface::info(&format!("'{}' is in component '{}' of {}", binary_name, comp.name, pkg_name));
                            resolved.include.push(comp.name.clone());
                        }
                }
                resolved
            } else if let Some(ref comp_str) = components {
                let comps: Vec<String> = comp_str.split(',').map(|s| s.trim().to_string()).collect();
                ComponentFilter { minimal: false, include_dev: false, include: comps, exclude: Vec::new() }
            } else {
                ComponentFilter::none()
            };

            let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db))
                .with_plugin_mgr(Arc::clone(&ctx.plugin_mgr))
                .with_component_filter(filter);
            match cmd.execute(&pkgs_to_upgrade).await {
                Ok(_) => {
                    record_lifecycle_install(&mut ctx, &pkgs_to_upgrade);
                    let binindex = crate::core::binindex::BinaryIndex::new(args.root.clone(), Arc::clone(&ctx.db));
                    let _ = binindex.rebuild();
                    UserInterface::success("Upgrade complete.");
                    run_autoremove_scan(&ctx.db, &args.root);
                }
                Err(e) => { UserInterface::error(&format!("{}", e)); process::exit(1); }
            }
        }
        Commands::Query { package } => {
            if crate::core::wildcard::has_wildcard(&package) {
                let installed: Vec<String> = ctx.db.get_all_installed_packages()
                    .unwrap_or_default().iter().map(|p| p.pkg_name.clone()).collect();
                let matched = crate::core::wildcard::expand(&package, installed.iter().map(String::as_str));
                if matched.is_empty() {
                    UserInterface::error(&format!("No installed packages match '{}'", package));
                    process::exit(1);
                }
                for name in &matched {
                    run_query_command(&ctx.db, name);
                }
            } else {
                run_query_command(&ctx.db, &package);
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
            let scanner = crate::core::integrity::IntegrityScanner::new(&root_path, Arc::clone(&ctx.db));
            let report = scanner.verify_all();

            let mut items: Vec<String> = Vec::new();
            items.push(format!("{} packages checked", report.total_packages));

            let issue_count = report.missing_files.len() + report.corrupted_files.len()
                + report.broken_deps.len() + report.dangling_symlinks + report.errors.len();

            if issue_count == 0 {
                items.push("No broken dependencies".into());
                items.push("No missing files".into());
                items.push("No dangling symlinks".into());
                UserInterface::block("Verification summary", &items.iter().map(|s| s.as_str()).collect::<Vec<_>>());
                UserInterface::success(&format!("All {} packages intact.", report.total_packages));
            } else {
                for mf in &report.missing_files {
                    UserInterface::error(&format!("{}: missing {}", mf.pkg, mf.path.display()));
                }
                for cf in &report.corrupted_files {
                    UserInterface::error(&format!("{}: corrupted {} ({})", cf.pkg, cf.path.display(), cf.reason));
                }
                for bd in &report.broken_deps {
                    UserInterface::error(&format!("{}: missing dep {}", bd.pkg, bd.missing_dep));
                }
                for e in &report.errors {
                    UserInterface::error(e);
                }
                if report.dangling_symlinks > 0 {
                    UserInterface::warning(&format!("{} dangling symlink(s) found", report.dangling_symlinks));
                }
                UserInterface::error(&format!("{} issues found. Run mcx -f to repair.", issue_count));
            }
        }
        Commands::FixDeps => {
            ctx.plugin_mgr.fire_hook(PluginHook::PreFix, &PluginEvent {
                hook: "pre-fix".into(),
                package: None,
                root: root_path.to_string_lossy().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });

            let scanner = crate::core::integrity::IntegrityScanner::new(&root_path, Arc::clone(&ctx.db));
            let result = scanner.repair_all();

            let mut total = result.files_repaired + result.symlinks_cleaned;

            if result.files_repaired > 0 {
                UserInterface::success(&format!("Hard-link repair: {} files recreated from CAS", result.files_repaired));
            }
            if result.symlinks_cleaned > 0 {
                UserInterface::success(&format!("Cleaned {} dangling symlinks", result.symlinks_cleaned));
            }
            for e in &result.errors {
                UserInterface::error(e);
            }

            if !result.missing_deps.is_empty() {
                UserInterface::info(&format!("Installing {} missing dependencies...", result.missing_deps.len()));
                let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&ctx.db))
                    .with_plugin_mgr(Arc::clone(&ctx.plugin_mgr));
                if let Err(e) = cmd.execute(&result.missing_deps).await {
                    UserInterface::error(&format!("Dependency install failed: {e}"));
                } else {
                    total += result.missing_deps.len();
                }
            }

            ctx.plugin_mgr.fire_hook(PluginHook::PostFix, &PluginEvent {
                hook: "postfix".into(),
                package: None,
                root: root_path.to_string_lossy().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });

            if total > 0 {
                UserInterface::success(&format!("Repair complete: {} issues resolved.", total));
            } else {
                UserInterface::success("All packages intact. No repair needed.");
            }
            run_autoremove_scan(&ctx.db, &args.root);
        }
        Commands::Config { init } => {
            if init {
                let config_dir = root_path.join("etc/mcx");
                fs::create_dir_all(&config_dir).unwrap_or_else(|e| {
                    UserInterface::error(&format!("Failed to create config dir: {e}"));
                    process::exit(1);
                });

                let config_ini = config_dir.join("config.ini");
                if !config_ini.exists() {
                    fs::write(&config_ini, constants::DEFAULT_CONFIG_INI.as_bytes()).unwrap_or_else(|e| {
                        UserInterface::error(&format!("Failed to write config.ini: {e}"));
                        process::exit(1);
                    });
                }

                let repo_ini = config_dir.join("repo.ini");
                if !repo_ini.exists() {
                    fs::write(&repo_ini, constants::DEFAULT_REPO_INI.as_bytes()).unwrap_or_else(|e| {
                        UserInterface::error(&format!("Failed to write repo.ini: {e}"));
                        process::exit(1);
                    });
                }

                let profile_ini = config_dir.join("profile.ini");
                if !profile_ini.exists() {
                    let default_arch = crate::core::arch::host_architecture().to_string();
                    let profile_content = format!("[profile]\nversion = 1.0.0\narchitecture = {}\npackages = \n", default_arch);
                    fs::write(&profile_ini, profile_content).unwrap_or_else(|e| {
                        UserInterface::error(&format!("Failed to write profile.ini: {e}"));
                        process::exit(1);
                    });
                }

                UserInterface::success("Core configuration files generated.");
                UserInterface::render_list("Generated", &[
                    config_ini.to_string_lossy().to_string(),
                    repo_ini.to_string_lossy().to_string(),
                    profile_ini.to_string_lossy().to_string(),
                ]);
            } else {
                let editor = ConfigEditorCommand::new(&args.root, ConfigTarget::EngineConfig);
                if let Err(e) = editor.execute() {
                    UserInterface::error(&format!("Config editor error: {e}"));
                    process::exit(1);
                }
            }
        }

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
                UserInterface::info(&format!("Previewing rollback plan for transaction {}", tx_id));
                let history = crate::core::history::HistoryEngine::new(&root_path, Arc::clone(&ctx.db));
                match tx_id.parse::<u64>() {
                    Ok(id) => {
                        match history.compute_rollback_plan(id) {
                            Ok(plan) => {
                                if plan.is_empty() {
                                    UserInterface::info("Nothing to roll back.");
                                }
                                for (action, targets) in &plan {
                                    match action {
                                        crate::core::changelog::ActionKind::Installation => {
                                            UserInterface::info(&format!("Would reinstall: {:?}", targets));
                                        }
                                        crate::core::changelog::ActionKind::Removal => {
                                            UserInterface::info(&format!("Would remove: {:?}", targets));
                                        }
                                        _ => {}
                                    }
                                }
                                UserInterface::warning(
                                    "This is a preview only; automatic rollback execution is not supported. Reinstall/remove the listed packages manually.",
                                );
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
                name, url, checksum: None, enabled: true,
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
                        .map(|r| {
                            let status = if r.enabled { "enabled" } else { "disabled" };
                            format!("{} -> {} [{}]", r.name, r.url, status)
                        })
                        .collect::<Vec<_>>();
                    UserInterface::render_list("Repositories", &items);
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }

        Commands::RepoSync { name } => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.sync_single(&name).await {
                Ok(_) => {
                    // Rebuild the available index from every repository's
                    // cached index so stale entries disappear instead of
                    // accumulating across syncs.
                    let repos = mgr.load_repositories().unwrap_or_default();
                    if let Err(e) = (|| -> anyhow::Result<()> {
                        let mut tx = ctx.db.begin_transaction()?;
                        tx.clear_available_index()?;
                        for repo in &repos {
                            let index_path = mgr.get_local_index_path(&repo.name);
                            if index_path.exists() {
                                let path_str = index_path.to_str()
                                    .ok_or_else(|| anyhow::anyhow!("index path is not valid UTF-8"))?;
                                tx.update_repository_index(&repo.name, path_str)?;
                            }
                        }
                        tx.commit()?;
                        Ok(())
                    })() {
                        UserInterface::error(&format!("Failed to refresh package index: {e}"));
                        process::exit(1);
                    }
                    let git = GitPackageManager::new(Path::new(&args.root));
                    if let Err(e) = git.sync_registry_from_indexes(&PathBuf::from(&args.root).join(constants::PATH_SYNC)) {
                        UserInterface::warning(&format!("Failed to refresh git registry: {e}"));
                    }
                    UserInterface::success(&format!("Repository '{}' synced.", name));
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }

        Commands::RepoEnable { name } => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.set_enabled(&name, true) {
                Ok(_) => UserInterface::success(&format!("Repository '{}' enabled.", name)),
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }

        Commands::RepoDisable { name } => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.set_enabled(&name, false) {
                Ok(_) => UserInterface::success(&format!("Repository '{}' disabled.", name)),
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }

        Commands::RepoInfo { name } => {
            let mgr = crate::core::repo::RepositoryManager::new(&args.root);
            match mgr.info(&name) {
                Ok(repo) => {
                    let status = if repo.enabled { "enabled" } else { "disabled" };
                    UserInterface::render_key_values(&format!("Repository: {}", repo.name), &[
                        ("URL", repo.url.as_str()),
                        ("Status", status),
                        ("Checksum", repo.checksum.as_deref().unwrap_or("(none)")),
                    ]);
                    let index_path = mgr.get_local_index_path(&repo.name);
                    if index_path.exists() {
                        if let Ok(content) = fs::read_to_string(&index_path)
                            && let Ok(pkgs) = serde_json::from_str::<Vec<crate::core::database::PackageMetadata>>(&content) {
                                UserInterface::info(&format!("Cached index: {} packages", pkgs.len()));
                            }
                    } else {
                        UserInterface::warning("No cached index (run mcx --repo-sync first)");
                    }
                }
                Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
            }
        }

        Commands::SelfUpdate => {
            let output_path = PathBuf::from(constants::SELF_UPDATE_BINARY_PATH);

            let repo_mgr = crate::core::repo::RepositoryManager::new(&root_path);
            let repos = repo_mgr.load_repositories()
                .unwrap_or_else(|e| { UserInterface::error(&format!("Failed to load repos: {e}")); process::exit(1); });

            if repos.is_empty() {
                UserInterface::error("No repositories configured. Use --repo-add first.");
                process::exit(1);
            }

            // Root-owned staging area under the target root; never /tmp.
            let stage_dir = root_path.join(constants::PATH_TMP);

            let mut last_error = String::new();
            let mut updated = false;

            for repo in &repos {
                let binary_url = format!("{}/system/bin/mcx", repo.url.trim_end_matches('/'));
                UserInterface::self_update(&format!("Downloading from {}...", binary_url));

                // stage_verified_binary refuses payloads without a published
                // checksum and verifies them before anything is executed.
                match crate::core::update::SelfUpdateManager::stage_verified_binary(&binary_url, &stage_dir).await {
                    Ok(staged_path) => {
                        let ver_output = std::process::Command::new(&staged_path)
                            .arg("--version")
                            .output()
                            .map(|o| o.stdout)
                            .unwrap_or_default();
                        let ver = String::from_utf8_lossy(&ver_output);
                        UserInterface::info(&format!("Verified: {}", ver.trim()));

                        match crate::core::update::SelfUpdateManager::promote(&staged_path, &output_path) {
                            Ok(()) => {
                                updated = true;
                                break;
                            }
                            Err(e) => {
                                last_error = format!("{}: {}", repo.name, e);
                                UserInterface::warning(&format!("Promotion failed for {}: {}", repo.name, e));
                            }
                        }
                    }
                    Err(e) => {
                        last_error = format!("{}: {}", repo.name, e);
                        UserInterface::info(&format!("Skipping {}: {}", repo.name, e));
                        continue;
                    }
                }
            }

            if !updated {
                UserInterface::error(&format!("Self-update failed. Last error: {}", last_error));
                process::exit(1);
            }

            UserInterface::self_update(&format!("Self-update complete. New binary at {}", constants::SELF_UPDATE_BINARY_PATH));
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

        Commands::Cgroup { action } => {
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

        Commands::HookPlugin { action } => {
            match action {
                HookPluginAction::List => {
                    let plugins = ctx.plugin_mgr.list();
                    if plugins.is_empty() {
                        UserInterface::info("No external plugins installed.");
                        UserInterface::info(&format!("Configure plugins in <root>/{}", constants::PLUGIN_CONFIG_FILE));
                    } else {
                        let summary: Vec<String> = plugins.iter().map(|p| {
                            format!("{} ({})", p.name(), p.path().display())
                        }).collect();
                        UserInterface::render_list("External plugins", &summary);
                    }
                }
                HookPluginAction::Info { name } => {
                    match ctx.plugin_mgr.find(&name) {
                        Some(p) => {
                            UserInterface::block(&format!("Plugin: {}", p.name()), &[
                                &format!("Name: {}", p.name()),
                                "Language: python",
                                &format!("File: {}", p.path().display()),
                            ]);
                        }
                        None => { UserInterface::error(&format!("Plugin '{}' not found", name)); process::exit(1); }
                    }
                }
                HookPluginAction::Run { name, hook } => {
                    let hook_str = hook.as_deref().unwrap_or("post-install");
                    let hook_enum = PluginHook::from_str(hook_str).unwrap_or(PluginHook::PostInstall);
                    let event = PluginEvent {
                        hook: hook_enum.as_str().to_string(),
                        package: None,
                        root: root_path.to_string_lossy().to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    match ctx.plugin_mgr.run_plugin_once(&name, &event) {
                        Ok(result) => {
                            if result.success {
                                UserInterface::success(&format!("Plugin '{}' completed", name));
                                if let Some(msg) = result.message {
                                    println!("{}", msg);
                                }
                            } else {
                                UserInterface::error(&format!("Plugin '{}' failed: {}", name, result.message.as_deref().unwrap_or("")));
                            }
                        }
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                HookPluginAction::Reload => {
                    ctx.plugin_mgr.reload(&root_path);
                    UserInterface::success(&format!("{} plugins loaded", ctx.plugin_mgr.list().len()));
                }
                HookPluginAction::ReloadConfig => {
                    match ctx.plugin_mgr.reload_from_config(&root_path) {
                        Ok(new_count) => {
                            UserInterface::success(&format!("{} new plugin(s) loaded, {} total", new_count, ctx.plugin_mgr.list().len()));
                        }
                        Err(e) => { UserInterface::error(&format!("{e}")); process::exit(1); }
                    }
                }
                HookPluginAction::Add { source } => {
                    let plugins_dir = root_path.join(constants::PATH_PLUGINS);
                    let source_path = PathBuf::from(&source);
                    if source_path.is_dir() {
                        let name = source_path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unnamed");
                        let target = plugins_dir.join(name);
                        if target.exists() {
                            UserInterface::error(&format!("Plugin '{}' already exists at {:?}", name, target));
                            process::exit(1);
                        }
                        match copy_dir(&source_path, &target) {
                            Ok(_) => {
                                ctx.plugin_mgr.reload(&root_path);
                                UserInterface::success(&format!("Plugin '{}' added from {:?}", name, source_path));
                            }
                            Err(e) => { UserInterface::error(&format!("Failed to add plugin: {e}")); process::exit(1); }
                        }
                    } else {
                        UserInterface::error(&format!("Source path '{}' is not a directory", source));
                        process::exit(1);
                    }
                }
                HookPluginAction::Remove { name } => {
                    let plugins_dir = root_path.join(constants::PATH_PLUGINS).join(&name);
                    if !plugins_dir.exists() {
                        UserInterface::error(&format!("Plugin '{}' not found at {:?}", name, plugins_dir));
                        process::exit(1);
                    }
                    match std::fs::remove_dir_all(&plugins_dir) {
                        Ok(_) => {
                            ctx.plugin_mgr.reload(&root_path);
                            UserInterface::success(&format!("Plugin '{}' removed", name));
                        }
                        Err(e) => { UserInterface::error(&format!("Failed to remove plugin: {e}")); process::exit(1); }
                    }
                }
            }
        }
        Commands::Plugin(cmd) => {
            use cps::plugin::PluginManager;
            match cmd {
                PluginCommand::List => {
                    let plugins = PluginManager::list();
                    if plugins.is_empty() {
                        UserInterface::info("No Python plugins installed.");
                    } else {
                        for p in &plugins {
                            println!("\x1b[32m{}\x1b[0m", p.name);
                            println!("  Path:    {}", p.path);
                            if !p.aliases.is_empty() {
                                println!("  Aliases:");
                                for (alias, cmd_str) in &p.aliases {
                                    println!("    {}  →  {}", alias, cmd_str);
                                }
                            }
                            println!();
                        }
                    }
                }
                PluginCommand::Run(args) => {
                    let (entry, func) = match PluginManager::by_alias(&args.alias) {
                        Some(found) => found,
                        None => {
                            UserInterface::error(&format!("No plugin alias '{}' found", args.alias));
                            process::exit(1);
                        }
                    };
                    match PluginManager::run(&entry, &func, &args.args) {
                        Ok(output) => println!("{}", output),
                        Err(e) => UserInterface::error(&format!("Plugin '{}' failed: {}", entry.name, e)),
                    }
                }
                PluginCommand::Install(args) => {
                    let src = Path::new(&args.path);
                    if !src.exists() {
                        UserInterface::error(&format!("File not found: {}", args.path));
                        process::exit(1);
                    }
                    if !src.is_file() {
                        UserInterface::error(&format!("Not a file: {}", args.path));
                        process::exit(1);
                    }
                    let name = args.name.unwrap_or_else(|| {
                        src.file_stem().unwrap_or_default().to_string_lossy().to_string()
                    });
                    let plugins_dir = &root_path.join(constants::PATH_PLUGINS);
                    let _ = fs::create_dir_all(plugins_dir);
                    let dest = plugins_dir.join(src.file_name().unwrap_or_default());
                    if dest.exists() && !args.force {
                        UserInterface::error(&format!("Plugin '{}' already exists. Use --force", name));
                        process::exit(1);
                    }
                    if let Err(e) = fs::copy(src, &dest) {
                        UserInterface::error(&format!("Failed to copy plugin: {}", e));
                        process::exit(1);
                    }
                    let mut aliases: HashMap<String, String> = args.aliases.clone().into_iter().collect();
                    if let Some(alias) = args.alias {
                        aliases.insert(alias.clone(), format!("{} {{}}", dest.display()));
                    }
                    PluginManager::register(&name, &dest, &aliases);
                    UserInterface::success(&format!("Plugin '{}' installed", name));
                }
                PluginCommand::Remove(args) => {
                    let entry = match PluginManager::by_name(&args.name) {
                        Some(e) => e,
                        None => { UserInterface::error(&format!("Plugin '{}' not found", args.name)); process::exit(1); }
                    };
                    let _ = fs::remove_file(&entry.path);
                    PluginManager::unregister(&args.name);
                    UserInterface::success(&format!("Plugin '{}' removed", args.name));
                }
                PluginCommand::Info(args) => {
                    match PluginManager::by_name(&args.name) {
                        Some(p) => {
                            println!("\x1b[32m{}\x1b[0m", p.name);
                            println!("  Path:    {}", p.path);
                            if !p.aliases.is_empty() {
                                println!("  Aliases:");
                                for (alias, cmd) in &p.aliases {
                                    println!("    {}  →  {}", alias, cmd);
                                }
                            }
                        }
                        None => UserInterface::error(&format!("Plugin '{}' not found", args.name)),
                    }
                }
            }
        }
        Commands::Theme(cmd) => {
            use cps::theme::ThemeEngine;
            match cmd {
                ThemeCommand::List => {
                    let themes = ThemeEngine::list();
                    if themes.is_empty() {
                        UserInterface::info("No themes installed.");
                    } else {
                        for t in &themes {
                            println!("\x1b[32m{}\x1b[0m", t.name);
                            println!("  Path: {}", t.path);
                            if !t.description.is_empty() {
                                println!("  Desc: {}", t.description);
                            }
                            println!();
                        }
                    }
                }
                ThemeCommand::Apply(args) => {
                    let theme = match ThemeEngine::by_name(&args.name) {
                        Some(t) => t,
                        None => { UserInterface::error(&format!("Theme '{}' not found", args.name)); process::exit(1); }
                    };
                    match ThemeEngine::apply(&theme) {
                        Ok(output) => println!("{}", output),
                        Err(e) => UserInterface::error(&format!("Theme '{}' failed: {}", theme.name, e)),
                    }
                }
                ThemeCommand::Install(args) => {
                    let src = Path::new(&args.path);
                    if !src.exists() {
                        UserInterface::error(&format!("File not found: {}", args.path));
                        process::exit(1);
                    }
                    if !src.is_file() {
                        UserInterface::error(&format!("Not a file: {}", args.path));
                        process::exit(1);
                    }
                    let name = args.name.unwrap_or_else(|| {
                        src.file_stem().unwrap_or_default().to_string_lossy().to_string()
                    });
                    let themes_dir = &root_path.join("etc/mcx/themes");
                    let _ = fs::create_dir_all(themes_dir);
                    let dest = themes_dir.join(src.file_name().unwrap_or_default());
                    if dest.exists() && !args.force {
                        UserInterface::error(&format!("Theme '{}' already exists. Use --force", name));
                        process::exit(1);
                    }
                    if let Err(e) = fs::copy(src, &dest) {
                        UserInterface::error(&format!("Failed to copy theme: {}", e));
                        process::exit(1);
                    }
                    ThemeEngine::register(&name, &dest);
                    UserInterface::success(&format!("Theme '{}' installed", name));
                }
                ThemeCommand::Remove(args) => {
                    let entry = match ThemeEngine::by_name(&args.name) {
                        Some(e) => e,
                        None => { UserInterface::error(&format!("Theme '{}' not found", args.name)); process::exit(1); }
                    };
                    let _ = fs::remove_file(&entry.path);
                    ThemeEngine::unregister(&args.name);
                    UserInterface::success(&format!("Theme '{}' removed", args.name));
                }
                ThemeCommand::Info(args) => {
                    match ThemeEngine::by_name(&args.name) {
                        Some(t) => {
                            println!("\x1b[32m{}\x1b[0m", t.name);
                            println!("  Path: {}", t.path);
                        }
                        None => UserInterface::error(&format!("Theme '{}' not found", args.name)),
                    }
                }
            }
        }
        Commands::Tui(cmd) => {
            use cps::tui::TuiEngine;
            match cmd {
                TuiCommand::List => {
                    let tuis = TuiEngine::list();
                    if tuis.is_empty() {
                        UserInterface::info("No TUIs installed.");
                    } else {
                        for t in &tuis {
                            println!("\x1b[32m{}\x1b[0m", t.name);
                            println!("  Path: {}", t.path);
                            if !t.description.is_empty() {
                                println!("  Desc: {}", t.description);
                            }
                            println!();
                        }
                    }
                }
                TuiCommand::Apply(args) => {
                    let tui = match TuiEngine::by_name(&args.name) {
                        Some(t) => t,
                        None => { UserInterface::error(&format!("TUI '{}' not found", args.name)); process::exit(1); }
                    };
                    match TuiEngine::apply(&tui) {
                        Ok(output) => println!("{}", output),
                        Err(e) => UserInterface::error(&format!("TUI '{}' failed: {}", tui.name, e)),
                    }
                }
                TuiCommand::Install(args) => {
                    let src = Path::new(&args.path);
                    if !src.exists() {
                        UserInterface::error(&format!("File not found: {}", args.path));
                        process::exit(1);
                    }
                    if !src.is_file() {
                        UserInterface::error(&format!("Not a file: {}", args.path));
                        process::exit(1);
                    }
                    let name = args.name.unwrap_or_else(|| {
                        src.file_stem().unwrap_or_default().to_string_lossy().to_string()
                    });
                    let tuis_dir = &root_path.join("etc/mcx/tuis");
                    let _ = fs::create_dir_all(tuis_dir);
                    let dest = tuis_dir.join(src.file_name().unwrap_or_default());
                    if dest.exists() && !args.force {
                        UserInterface::error(&format!("TUI '{}' already exists. Use --force", name));
                        process::exit(1);
                    }
                    if let Err(e) = fs::copy(src, &dest) {
                        UserInterface::error(&format!("Failed to copy TUI: {}", e));
                        process::exit(1);
                    }
                    TuiEngine::register(&name, &dest);
                    UserInterface::success(&format!("TUI '{}' installed", name));
                }
                TuiCommand::Remove(args) => {
                    let entry = match TuiEngine::by_name(&args.name) {
                        Some(e) => e,
                        None => { UserInterface::error(&format!("TUI '{}' not found", args.name)); process::exit(1); }
                    };
                    let _ = fs::remove_file(&entry.path);
                    TuiEngine::unregister(&args.name);
                    UserInterface::success(&format!("TUI '{}' removed", args.name));
                }
                TuiCommand::Info(args) => {
                    match TuiEngine::by_name(&args.name) {
                        Some(t) => {
                            println!("\x1b[32m{}\x1b[0m", t.name);
                            println!("  Path: {}", t.path);
                        }
                        None => UserInterface::error(&format!("TUI '{}' not found", args.name)),
                    }
                }
            }
        }
        Commands::Service { args } => {
            let cmd = ServiceCommand::new(root_path.to_string_lossy().to_string(), Arc::clone(&ctx.db));
            if let Err(e) = cmd.execute(&args) {
                UserInterface::error(&format!("{e}"));
                process::exit(1);
            }
        }
        Commands::CommandNotFound { command } => {
            if let Err(e) = crate::core::binindex::command_not_found_handler(
                &args.root, Arc::clone(&ctx.db), &command,
            ) {
                UserInterface::error(&format!("{e}"));
                process::exit(1);
            }
        }
        Commands::BinIndex => {
            let binindex = crate::core::binindex::BinaryIndex::new(args.root.clone(), Arc::clone(&ctx.db));
            match binindex.rebuild() {
                Ok(count) => UserInterface::success(&format!("Binary index rebuilt: {} entries", count)),
                Err(e) => { UserInterface::error(&format!("Failed to rebuild binary index: {e}")); process::exit(1); }
            }
        }
        Commands::AutoRemove { apply } => {
            UserInterface::info("Analyzing unnecessary packages...");
            let analyzer = crate::core::autoremove::AutoRemoveAnalyzer::new(Arc::clone(&ctx.db), &args.root);
            match analyzer.analyze() {
                Ok(report) => {
                    analyzer.print_report(&report);
                    if apply && !report.orphaned_packages.is_empty() {
                        UserInterface::info(&format!("Removing {} orphaned package(s)...", report.orphaned_packages.len()));
                        match analyzer.execute_removal(&report) {
                            Ok(count) => {
                                let binindex = crate::core::binindex::BinaryIndex::new(args.root.clone(), Arc::clone(&ctx.db));
                                let _ = binindex.rebuild();
                                UserInterface::success(&format!("Auto-remove complete: {} package(s) removed.", count));
                            }
                            Err(e) => { UserInterface::error(&format!("Auto-remove failed: {e}")); process::exit(1); }
                        }
                    } else if !report.orphaned_packages.is_empty() {
                        UserInterface::info("Dry-run: use --apply to actually remove orphaned packages.");
                    }
                }
                Err(e) => { UserInterface::error(&format!("Analysis failed: {e}")); process::exit(1); }
            }
        }
        Commands::Mode { .. } => unreachable!("mode command is handled before engine context is built"),
    }
}

fn copy_dir(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    copy_dir_recursive(src, dst, src)
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf, base: &PathBuf) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(base).expect("path under base");
        let target = dst.join(relative);
        if path.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_dir_recursive(&path, dst, base)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// Expand `*`/`?` wildcard package arguments into their concrete matches
/// before dispatch. Non-wildcard arguments pass through untouched; wildcards
/// are matched against the most relevant name space for the command
/// (available packages for installs/updates, installed packages for
/// removal), so `mcx install libs*` installs every matching package instead
/// of requiring each name literally.
fn expand_wildcards(cmd: &mut Commands, db: &Database) -> anyhow::Result<()> {
    use crate::core::wildcard::{expand, has_wildcard};

    fn name_pool(db: &Database, available: bool) -> Vec<String> {
        let metas = if available {
            db.get_all_available_packages()
        } else {
            db.get_all_installed_packages()
        };
        let mut names: Vec<String> = metas
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.pkg_name)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn expand_targets(patterns: &[String], pool: &[String]) -> anyhow::Result<Vec<String>> {
        let mut result: Vec<String> = Vec::new();
        for pat in patterns {
            if !has_wildcard(pat) {
                if !result.iter().any(|r| r == pat) {
                    result.push(pat.clone());
                }
                continue;
            }
            let matched = expand(pat, pool.iter().map(String::as_str));
            if matched.is_empty() {
                return Err(anyhow::anyhow!("No packages match pattern '{}'", pat));
            }
            for name in matched {
                if !result.iter().any(|r| r == &name) {
                    result.push(name);
                }
            }
        }
        Ok(result)
    }

    let available_pool = name_pool(db, true);
    match cmd {
        Commands::Install { packages, .. } => {
            *packages = expand_targets(packages, &available_pool)?;
        }
        Commands::Remove { packages, .. } | Commands::Purge { packages } => {
            let installed_pool = name_pool(db, false);
            *packages = expand_targets(packages, &installed_pool)?;
        }
        Commands::Update { packages: Some(pkgs) } => {
            *pkgs = expand_targets(pkgs, &available_pool)?;
        }
        Commands::Update { packages: None } => {}
        Commands::Upgrade { packages: Some(pkgs), .. } => {
            *pkgs = expand_targets(pkgs, &available_pool)?;
        }
        Commands::Upgrade { packages: None, .. } => {}
        _ => {}
    }
    Ok(())
}

/// Render a single package's query output ("Not installed." on failure).
fn run_query_command(db: &Database, package: &str) {
    match db.get_package_manifest(package) {
        Ok(meta) => {
            let file_count = meta.files.len().to_string();
            let dep_count = meta.dependencies.len().to_string();

            let rdepends: Vec<String> = db.get_all_installed_packages()
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
                    let ver = db.get_package_manifest(&d.name)
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
                ("Architecture", &meta.architecture),
                ("Source", meta.source.as_str()),
                ("Files", &file_count),
                ("Dependencies", &dep_count),
                ("Reverse deps", &rdeps_str),
            ];
            UserInterface::render_key_values(&format!("Package: {}", meta.pkg_name), &pairs);

            let table_rows = vec![
                vec![meta.pkg_name.clone(), meta.version.clone(), meta.architecture.clone(), meta.license.clone()],
            ];
            UserInterface::table("Package summary", &["Name", "Version", "Architecture", "License"], &table_rows);

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
            if !meta.components.is_empty() {
                let comp_strings: Vec<String> = meta.components.iter()
                    .map(|c| format!("{} [{}] ({} files)", c.name, c.priority, c.files.len()))
                    .collect();
                UserInterface::render_list("Components", &comp_strings);
            }
            for svc in meta.all_services() {
                let mut svc_items: Vec<String> = Vec::new();
                svc_items.push(format!("Name: {}", svc.name));
                svc_items.push(format!("Exec: {}", svc.exec));
                svc_items.push(format!("Restart: {}", svc.restart));
                if !svc.requires.is_empty() {
                    svc_items.push(format!("Requires: {}", svc.requires));
                }
                UserInterface::render_list("Service", &svc_items);
            }
        }
        Err(_) => UserInterface::error("Not installed."),
    }
}

/// Commands that never mutate the target root: when a non-root user runs one
/// in system mode, mcx operates on the user root instead of escalating.
fn subcommand_is_read_only(cmd: &Commands) -> bool {
    matches!(
        cmd,
        Commands::Search { .. }
            | Commands::Query { .. }
            | Commands::History { .. }
            | Commands::Completion { .. }
            | Commands::RepoList
            | Commands::RepoInfo { .. }
    )
}

/// `mcx --mode user|system|status`. Toggling requires the root password on
/// every invocation and persists the user selection in the separate
/// `~/.mcx/etc/mcx/user.ini` (with a required `user` field), so the mode is
/// never mixed into the root-directory config.
fn handle_mode_command(action: &str, mode_flag: Option<bool>) {
    let cfg = crate::core::mode::read_mode_config();
    match action {
        "status" => {
            let effective = crate::core::mode::resolve_mode(mode_flag);
            let selection = crate::core::mode::read_user_selection();
            let user = selection
                .map(|s| s.user)
                .unwrap_or_else(crate::core::mode::current_user);
            UserInterface::info("Mode status:");
            UserInterface::block(
                "Operating mode",
                &[
                    &format!("Mode: {}", effective.as_str()),
                    &format!("User: {}", user),
                    &format!("User root: {}", cfg.effective_user_root().display()),
                    &format!("System root: {}", cfg.effective_system_root().display()),
                    &format!("Selection config: {}", crate::core::mode::user_selection_path().display()),
                    &format!("Root config: {}", crate::core::mode::user_config_path().display()),
                ],
            );
            UserInterface::info("Toggling: mcx --mode user | mcx --mode system");
        }
        target @ ("user" | "system") => {
            UserInterface::info(&format!(
                "Toggling mode to '{}' (root password required)...",
                target
            ));
            if !crate::core::sudo::authenticate_toggle() {
                process::exit(1);
            }
            let user = crate::core::mode::current_user();
            let sel = crate::core::mode::UserSelection {
                mode: crate::core::mode::Mode::parse(target).expect("clap restricts mode action"),
                user: user.clone(),
            };
            if let Err(e) = crate::core::mode::write_user_selection(&sel) {
                UserInterface::error(&format!("Failed to persist user selection: {e}"));
                process::exit(1);
            }
            let _ = crate::core::mode::ensure_mode_fields();
            let root = if target == "user" {
                let home = crate::core::mode::home_dir().join(crate::core::constants::USER_ROOT_DIR);
                home.display().to_string()
            } else {
                cfg.effective_system_root().display().to_string()
            };
            UserInterface::success(&format!(
                "Mode switched to '{}' for user '{}' (root: {}, selection config: {})",
                target,
                user,
                root,
                crate::core::mode::user_selection_path().display()
            ));
            UserInterface::info(&format!(
                "Next invocations of mcx will run in {} mode. Toggle again with: mcx --mode {}",
                target,
                if target == "user" { "system" } else { "user" }
            ));
        }
        _ => unreachable!("clap restricts mode action to user/system/status"),
    }
}

fn run_autoremove_scan(db: &Arc<Database>, root: &str) {
    let analyzer = crate::core::autoremove::AutoRemoveAnalyzer::new(Arc::clone(db), root);
    if let Ok(report) = analyzer.analyze() {
        if !report.orphaned_packages.is_empty() {
            UserInterface::separator();
            UserInterface::info(&format!("Auto-remove: {} orphaned package(s) can be removed", report.orphaned_packages.len()));
            for orphan in &report.orphaned_packages {
                UserInterface::info(&format!("  {} {} — {:?}", orphan.name, orphan.version, orphan.reason));
            }
            UserInterface::info("Run `mcx --autoremove --apply` to remove them.");
        }
        if !report.unnecessary_libs.is_empty() {
            UserInterface::info(&format!("Auto-remove: {} unnecessary library file(s) found", report.unnecessary_libs.len()));
        }
    }
}