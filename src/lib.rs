pub mod archive;
pub mod commands;
pub mod core;
pub mod network;
pub mod utils;

pub use commands::add::AddLocalCommand;
pub use commands::clean::CleanCommand;
pub use commands::configuration::ConfigEditorCommand;
pub use commands::install::InstallCommand;
pub use commands::localsrc::LocalSourceCommand;
pub use commands::remove::RemoveCommand;
pub use commands::search::SearchCommand;
pub use commands::sync::SyncCommand;
pub use commands::system::SystemCommand;

pub use core::arch::{Architecture, host_architecture};
pub use core::autoremove::{
    AutoRemoveAnalyzer, AutoRemoveReport, OrphanReason, OrphanedPackage, UnnecessaryLib,
};
pub use core::cas::CasStore;
pub use core::cgroup::CgroupController;
pub use core::changelog::ChangelogManager;
pub use core::completion::CompletionEngine;
pub use core::config::{ConfigManager, MappedConfig};
pub use core::database::Database;
pub use core::declarative::ProfileValidator;
pub use core::graph::DepGraph;
pub use core::history::HistoryEngine;
pub use core::integrity::IntegrityScanner;
pub use core::lifecycle::{
    DependencyGraph, LifecycleEngine, LifecycleEntry, LifecycleEvent, LifecycleTransition,
    OrphanSet, PackageState,
};
pub use core::localsrc::{
    LocalSource, LocalSourceAction, LocalSourceManager, LocalSourceResult, SourceMode,
};
pub use core::manifest::ManifestParser;
pub use core::mode::{
    Mode, ModeConfig, UserSelection, current_user, ensure_mode_fields, normalize_root,
    read_mode_config, read_user_selection, read_user_selection_in, resolve_mode, resolve_root,
    user_selection_path, user_selection_path_in, write_user_selection, write_user_selection_in,
};
pub use core::package::PackageEntity;
pub use core::plugin::{
    PluginEvent, PluginHook, PluginManager, PluginManifest, PluginResult, PluginSlot, PythonPlugin,
};
pub use core::profiler::{
    CalibratedParams, DecisionEngine, DecisionMatrix, HeuristicVerdict, NetworkProber,
    NetworkProfile, SystemProfile,
};
pub use core::provenance::{PackageProvenance, auto_link_origin};
pub use core::repo::RepositoryManager;
pub use core::rollback::RollbackManager;
pub use core::security::SecurityMonitor;
pub use core::solver::{DependencySolver, ResolutionVerdict, UpgradePath};
pub use core::transaction::PackageTransaction;
pub use core::update::SelfUpdateManager;
pub use core::vendor::VendorManager;
pub use core::workspace::WorkspaceManager;
pub use cps::PythonConfig;

pub use network::download::Downloader;

pub use utils::ui::UserInterface;
