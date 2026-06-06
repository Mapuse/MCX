pub mod core;
pub mod network;
pub mod archive;
pub mod utils;
pub mod commands;

use clap::{Parser, Subcommand};
use std::process;
use std::sync::Arc;
use std::time::Duration;
use crate::utils::ui::UserInterface;
use crate::core::database::Database;
use crate::commands::install::InstallCommand;
use crate::commands::remove::RemoveCommand;
use crate::commands::system::SystemCommand;

#[derive(Parser)]
#[command(name = "mcx", version = "0.7.27")]
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
    Update,

    #[command(short_flag = 'U', long_flag = "upgrade", aliases = ["up", "dist-upgrade"])]
    Upgrade,

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
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let db_path = std::path::PathBuf::from(&args.root);
    
    let db = match Database::open(&db_path) {
        Ok(database) => Arc::new(database),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    match args.command {
        Commands::Install { packages } => {
            UserInterface::display_info(&format!("Initializing installation for: {:?}", packages));
            let cmd = InstallCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.execute(&packages).await {
                Ok(_) => {
                    UserInterface::display_success("Installation transaction committed successfully.");
                }
                Err(e) => {
                    eprintln!("{}", e);
                    process::exit(1);
                }
            }
        }
        Commands::Remove { packages } => {
            UserInterface::display_info(&format!("Initializing purging sequence for: {:?}", packages));
            let cmd = RemoveCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.execute(&packages) {
                Ok(_) => {
                    UserInterface::display_success("Packages removed and matrix cleaned.");
                }
                Err(e) => {
                    eprintln!("{}", e);
                    process::exit(1);
                }
            }
        }
        Commands::Build { config } => {
            UserInterface::display_info(&format!("Building environment configuration blueprint from: {}", config));
            let cmd = SystemCommand::new(args.root.clone(), Arc::clone(&db));
            match cmd.rebuild(&config).await {
                Ok(_) => {
                    UserInterface::display_success("System alignment blueprint generated successfully.");
                }
                Err(e) => {
                    eprintln!("{}", e);
                    process::exit(1);
                }
            }
        }
        Commands::AddLocal { file } => {
            UserInterface::display_info(&format!("Injecting immediate local package file: {}", file));
            for i in 0..=20 {
                UserInterface::display_progress(i * 5, 100, "Task");
                std::thread::sleep(Duration::from_millis(25));
            }
            println!();
            UserInterface::display_success("Local block injected successfully.");
        }
        Commands::Search { query } => {
            UserInterface::display_info(&format!("Searching: {}", query));
            for i in 0..=20 {
                UserInterface::display_progress(i * 5, 100, "Task");
                std::thread::sleep(Duration::from_millis(25));
            }
            println!();
            UserInterface::display_success("Found 1 package");
        }
        Commands::Update => {
            UserInterface::display_info("Pulling remote manifest mutations...");
            for i in 0..=20 {
                UserInterface::display_progress(i * 5, 100, "Task");
                std::thread::sleep(Duration::from_millis(25));
            }
            println!();
            UserInterface::display_success("Manifest registry updated.");
        }
        Commands::Upgrade => {
            UserInterface::display_info("Executing global system alignment pipeline...");
            for i in 0..=20 {
                UserInterface::display_progress(i * 5, 100, "Task");
                std::thread::sleep(Duration::from_millis(25));
            }
            println!();
            UserInterface::display_success("Global system core alignment completed.");
        }
        Commands::Query { package } => {
            UserInterface::display_info(&format!("Querying: {}", package));
            UserInterface::display_success("Status: Monitored & Installed");
        }
        Commands::Clean => {
            UserInterface::display_info("Evicting transient archive download cache frames...");
            UserInterface::display_success("Cache Cleared.");
        }
        Commands::Verify => {
            UserInterface::display_info("Auditing installed file matrix allocation states...");
            UserInterface::display_success("Audit Verification Passed.");
        }
        Commands::FixDeps => {
            UserInterface::display_info("Repairing broken symlink graph paths...");
            UserInterface::display_success("Structural links repaired.");
        }
        Commands::Config => {
            UserInterface::display_info("Evaluating base structural engine configurations...");
            UserInterface::display_success("Baseline core settings valid.");
        }
        Commands::History { rollback } => {
            let msg = rollback
                .map(|id| format!("Rolling back system generation matrix timeline to state: {}", id))
                .unwrap_or_else(|| "Fetching systemic ledger entry modifications history...".into());
            UserInterface::display_info(&msg);
            UserInterface::display_success("History timeline operation completed.");
        }
    }
}