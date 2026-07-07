pub mod archive;
pub mod commands;
pub mod core;
pub mod network;
pub mod utils;

pub use commands::add::AddLocalCommand;
pub use commands::clean::CleanCommand;
pub use commands::configuration::ConfigEditorCommand;
pub use commands::install::InstallCommand;
pub use commands::remove::RemoveCommand;
pub use commands::search::SearchCommand;
pub use commands::sync::SyncCommand;
pub use commands::system::SystemCommand;

pub use core::arch::{Architecture, host_architecture};
pub use core::cache::CacheManager;
pub use core::cas::CasStore;
pub use core::cgroup::CgroupController;
pub use core::changelog::ChangelogManager;
pub use core::completion::CompletionEngine;
pub use core::config::{ConfigManager, MappedConfig, CalibratedParams};
pub use core::database::Database;
pub use core::declarative::ProfileValidator;
pub use core::graph::DepGraph;
pub use core::history::HistoryEngine;
pub use core::integrity::IntegrityScanner;
pub use core::lifecycle::{LifecycleEngine, LifecycleEntry, LifecycleTransition, LifecycleEvent, PackageState, DependencyGraph, OrphanSet};
pub use core::manifest::ManifestParser;
pub use core::package::PackageEntity;
pub use core::plugin::{PluginRegistry, PluginSlot, PluginManager, ExternalPlugin, PluginManifest, PluginHook, PluginEvent, PluginResult, Fetcher, Builder, Packer, CurlFetcher, DefaultBuilder, ZstdPacker};
pub use core::profiler::{SystemProfile, DecisionEngine, DecisionMatrix, HeuristicVerdict, AutoHealer, NetworkProber};
pub use core::repo::RepositoryManager;
pub use core::rollback::RollbackManager;
pub use core::security::SecurityMonitor;
pub use core::update::SelfUpdateManager;
pub use core::solver::{DependencySolver, ResolutionVerdict, UpgradePath, compute_depsig, scan_package_directory, build_dependency_graph};
pub use core::transaction::PackageTransaction;
pub use core::vendor::VendorManager;
pub use core::workspace::WorkspaceManager;

pub use network::download::Downloader;
pub use network::sync::NetworkSyncEngine;

pub use utils::ui::UserInterface;
