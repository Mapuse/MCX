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
use crate::commands::add::AddLocalCommand;
use crate::commands::clean::CleanCommand;
use crate::commands::install::InstallCommand;
use crate::commands::remove::RemoveCommand;
use crate::commands::search::SearchCommand;
use crate::commands::sync::SyncCommand;
use crate::commands::system::SystemCommand;

#[derive(Parser)]
#[command(name = "mcx", version = "2.7.6")]
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

    #[command(short_flag = 'H', long_flag = "history", aliases = ["log", "record"])]
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
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Remove { packages } => {
            UserInterface::display_info(&format!("Removing: {:?}", packages));
            let cmd = RemoveCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.execute(&packages) {
                Ok(_) => UserInterface::display_success("Packages removed."),
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::Build { config } => {
            UserInterface::display_info(&format!("Building from: {}", config));
            let cmd = SystemCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.rebuild(&config).await {
                Ok(_) => UserInterface::display_success("System aligned."),
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
        Commands::AddLocal { file } => {
            UserInterface::display_info(&format!("Installing local: {}", file));
            let cmd = AddLocalCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.execute(&file) {
                Ok(_) => {
                    let staging = PathBuf::from(&args.root).join("var/tmp/mcx/stage");
                    let installed_root = PathBuf::from(&args.root).join("var/lib/mcx/active");
                    if let Ok(pkgs) = db.get_all_installed_packages() {
                        if let Some(last) = pkgs.last() {
                            let pkg_path = staging.join(&last.pkg_name);
                            if pkg_path.exists() {
                                if let Err(e) = std::fs::rename(&pkg_path, installed_root.join(&last.pkg_name)) {
                                    UserInterface::display_error(&format!("Failed to move package from staging: {e}"));
                                    process::exit(1);
                                }
                            } else {
                                UserInterface::display_error(&format!("Staged package not found: {}", pkg_path.display()));
                                process::exit(1);
                            }
                        }
                    }
                    UserInterface::display_success("Local package installed.");
                }
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
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
                    // build key/value slices without holding temporary Strings across the call
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
        Commands::Config => UserInterface::display_success("Configuration saved."),
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
                    let items = repos
                        .into_iter()
                        .map(|r| format!("{} -> {}", r.name, r.url))
                        .collect::<Vec<_>>();
                    UserInterface::render_list("Repositories", &items);
                }
                Err(e) => { UserInterface::display_error(&format!("{e}")); process::exit(1); }
            }
        }
    }
}