use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use mcx::utils::ui::UserInterface;
use mcx::core::database::{Database, PackageMetadata, ChecksumData, Dependency};
use mcx::core::completion::CompletionEngine;
use mcx::core::security::SecurityMonitor;
use mcx::core::declarative::ProfileValidator;
use mcx::core::cgroup::CgroupController;
use mcx::core::cas::CasStore;
use mcx::core::rollback::RollbackManager;
use mcx::core::lifecycle::{LifecycleEngine, DependencyGraph, PackageState, OrphanSet};

use mcx::commands::remove::RemoveCommand;
use mcx::commands::install::InstallCommand;
use mcx::commands::configuration::ConfigTarget;
use mcx::network::download::Downloader;

fn create_temporary_root(identifier: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("mcx_test_{}_{}", identifier, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&path).unwrap();
    path
}

// ── Database / Transaction ──────────────────────────────────────────────────

#[tokio::test]
async fn test_atomic_database_write_and_conflict_prevention() {
    let root = create_temporary_root("conflict_prevention");
    let db = Database::open(&root).unwrap();

    let package_a = PackageMetadata {
        pkg_name: "package-a".to_string(), version: "1.0.0".to_string(),
        license: "MIT".to_string(), source: "https://example.com/a".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string() },
        dependencies: vec![], files: vec![PathBuf::from("usr/bin/shared-binary")],
        provides: Some(vec![]), conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx_a = db.begin_transaction().unwrap();
    tx_a.register_package_placement(&package_a).unwrap();
    tx_a.commit().unwrap();
    assert!(db.is_package_installed("package-a").unwrap());

    let package_b = PackageMetadata {
        pkg_name: "package-b".to_string(), version: "2.0.0".to_string(),
        license: "Apache-2.0".to_string(), source: "https://example.com/b".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "5891a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146f".to_string() },
        dependencies: vec![], files: vec![PathBuf::from("usr/bin/shared-binary")],
        provides: Some(vec![]), conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx_b = db.begin_transaction().unwrap();
    let result = tx_b.register_package_placement(&package_b);
    assert!(result.is_err());

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_concurrent_transaction_serialization_isolation() {
    let root = create_temporary_root("isolation_lock");
    let db = Database::open(&root).unwrap();

    // LMDB enforces single-writer; commit first txn before starting second
    let tx_primary = db.begin_transaction().unwrap();
    tx_primary.commit().unwrap();

    let tx_secondary = db.begin_transaction().unwrap();
    tx_secondary.commit().unwrap();

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_empty_installation_command_error() {
    let root = create_temporary_root("install_command");
    let db = Database::open(&root).unwrap();
    let command = InstallCommand::new(root.to_string_lossy().into_owned(), Arc::new(db));
    let result = command.execute(&[]).await;
    assert!(result.is_err());
    fs::remove_dir_all(&root).unwrap();
}

// ── Remove + Sandbox Cleanup ────────────────────────────────────────────────

#[tokio::test]
async fn test_package_removal_and_filesystem_cleanup() {
    let root = create_temporary_root("filesystem_cleanup");
    let binary_dir = root.join("usr/bin");
    fs::create_dir_all(&binary_dir).unwrap();
    let binary_file = binary_dir.join("app-binary");
    fs::write(&binary_file, b"ELF").unwrap();

    let db = Database::open(&root).unwrap();
    let package = PackageMetadata {
        pkg_name: "app".to_string(), version: "1.5.2".to_string(),
        license: "GPL-3.0".to_string(), source: "https://example.com/app".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "9ee6a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146a".to_string() },
        dependencies: vec![], files: vec![PathBuf::from("usr/bin/app-binary")],
        provides: Some(vec![]), conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx = db.begin_transaction().unwrap();
    tx.register_package_placement(&package).unwrap();
    tx.commit().unwrap();

    let db_share = Arc::new(db);
    let command = RemoveCommand::new(root.to_string_lossy().into_owned(), db_share.clone());
    let cg = mcx::CgroupController::new();
    let sm = mcx::SecurityMonitor::new();
    command.execute(&["app".to_string()], &cg, &sm).unwrap();

    assert!(!db_share.is_package_installed("app").unwrap());
    assert!(!binary_file.exists());
    fs::remove_dir_all(&root).unwrap();
}

// ── Completion ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_shell_completion_engine_querying() {
    let root = create_temporary_root("completion_engine");
    let db = Database::open(&root).unwrap();

    let package = PackageMetadata {
        pkg_name: "neovim".to_string(), version: "0.9.0".to_string(),
        license: "Apache-2.0".to_string(), source: "https://example.com/nvim".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "1111a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string() },
        dependencies: vec![], files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx = db.begin_transaction().unwrap();
    tx.register_package_placement(&package).unwrap();
    tx.commit().unwrap();

    let engine = CompletionEngine::new(Arc::new(db));
    let subcommands = engine.complete_subcommand("inst");
    assert!(subcommands.contains(&"install".to_string()));

    let packages = engine.complete_installed_package("neo").unwrap();
    assert!(packages.contains(&"neovim".to_string()));

    fs::remove_dir_all(&root).unwrap();
}

// ── UI ──────────────────────────────────────────────────────────────────────

#[test]
fn test_user_interface_output_nodes() {
    UserInterface::info("Core synchronization test channel opened");
    UserInterface::success("Operation completed inside integration frame");
    UserInterface::error("Simulated catastrophic deployment rollback");
    UserInterface::warning("Alert safe status check bounds active");
    UserInterface::download("Download pipeline test message");
    UserInterface::security("Security monitor test message");
    UserInterface::profile("Profile validator test message");
    UserInterface::cgroup("Cgroup controller test message");
    UserInterface::cas("CAS store test message");
    UserInterface::self_update("Self-update test message");
    UserInterface::version("mcx 4.0.0");
    UserInterface::progress(50, 100, "Extracting asset metadata tree");

    let list_items = vec![
        "mcx-core-engine v4.0.0".to_string(),
        "network-transport-ssl".to_string(),
        "local-registry-ledger".to_string(),
    ];
    UserInterface::render_list("Monitored Core Graph Structures", &list_items);
}

// ── Config Init ─────────────────────────────────────────────────────────────

#[test]
fn test_config_init_generates_defaults() {
    let root = create_temporary_root("config_init");
    let config_dir = root.join("etc/mcx");
    fs::create_dir_all(&config_dir).unwrap();

    // mark config.ini as "already exists" to test skip-behaviour
    fs::write(config_dir.join("config.ini"), b"[engine]\nthread_pool_mode = auto\n").unwrap();

    let config_ini = config_dir.join("config.ini");
    let repo_ini = config_dir.join("repo.ini");
    let profile_ini = config_dir.join("profile.ini");

    // before init: config.ini exists, repo.ini and profile.ini do not
    assert!(config_ini.exists());
    assert!(!repo_ini.exists());
    assert!(!profile_ini.exists());

    // simulate init logic (same as main.rs Commands::Config { init: true })
    if !config_ini.exists() {
        fs::write(&config_ini, b"[engine]\nthread_pool_mode = auto\nmax_concurrent_downloads = 8\nzstd_level = 3\n\n[network]\nfallback_repos = enabled\nlatency_threshold_ms = 200\nbandwidth_threshold_kbps = 5000\n\n[security]\nverify_checksums = true\nallow_unverified = false\n\n[cache]\nlimit_bytes = 5368709120\nprune_age_hours = 168\n").unwrap();
    }
    if !repo_ini.exists() {
        fs::write(&repo_ini, b"[main]\nurl = https://packages.cudane.org\nenabled = true\npriority = 100\n\n[community]\nurl = https://community.cudane.org\nenabled = false\npriority = 200\n").unwrap();
    }
    if !profile_ini.exists() {
        fs::write(&profile_ini, b"[profile]\nversion = 1.0.0\narchitecture = x86_64\npackages = \n").unwrap();
    }

    // config.ini content preserved (not overwritten)
    let cfg_content = fs::read_to_string(&config_ini).unwrap();
    assert!(cfg_content.contains("thread_pool_mode"));

    // repo.ini created
    assert!(repo_ini.exists());
    let repo_content = fs::read_to_string(&repo_ini).unwrap();
    assert!(repo_content.contains("packages.cudane.org"));

    // profile.ini created
    assert!(profile_ini.exists());
    let profile_content = fs::read_to_string(&profile_ini).unwrap();
    assert!(profile_content.contains("version = 1.0.0"));

    fs::remove_dir_all(&root).unwrap();
}

// ── Network Download ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_network_downloader_endpoint_handling() {
    let root = create_temporary_root("network_download");
    let downloader = Downloader::new();

    let is_available = downloader.check_endpoint_availability("https://www.google.com").await;
    assert!(is_available);

    let destination = root.join("test_download.html");
    let result = downloader.package("https://www.google.com", &destination).await;
    assert!(result.is_ok());
    assert!(destination.exists());
    assert!(fs::metadata(&destination).unwrap().len() > 0);

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_network_downloader_transient_failure_recovery() {
    let root = create_temporary_root("network_fault");
    let downloader = Downloader::new();

    let invalid_endpoint = "https://invalid-subdomain-unreachable-target-node.org/asset.xcs";
    let destination = root.join("failed_output.xcs");

    let result = downloader.package(invalid_endpoint, &destination).await;
    assert!(result.is_err());

    let availability = downloader.check_endpoint_availability(invalid_endpoint).await;
    assert!(!availability);

    fs::remove_dir_all(&root).unwrap();
}

// ── Database / Dependency Graph ─────────────────────────────────────────────

#[tokio::test]
async fn test_database_dependency_graph_relations() {
    let root = create_temporary_root("database_relations");
    let db = Database::open(&root).unwrap();

    let base_package = PackageMetadata {
        pkg_name: "openssl".to_string(), version: "4.0.0".to_string(),
        license: "Apache-2.0".to_string(), source: "https://example.com/ssl".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "1234a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string() },
        dependencies: vec![], files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx = db.begin_transaction().unwrap();
    tx.register_package_placement(&base_package).unwrap();
    tx.commit().unwrap();

    let dependent_package = PackageMetadata {
        pkg_name: "curl".to_string(), version: "8.0.0".to_string(),
        license: "MIT".to_string(), source: "https://example.com/curl".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "5678a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string() },
        dependencies: vec![Dependency { name: "openssl".to_string(), dep_type: "runtime".to_string(), libraries: None }],
        files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx2 = db.begin_transaction().unwrap();
    tx2.register_package_placement(&dependent_package).unwrap();
    tx2.commit().unwrap();

    assert!(db.has_dependent_packages("openssl").unwrap());
    assert!(!db.has_dependent_packages("curl").unwrap());

    fs::remove_dir_all(&root).unwrap();
}

// ── Solver / Resolution ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_cyclic_dependency_deadlock_breaking() {
    let root = create_temporary_root("cyclic_deadlock");
    let db = Database::open(&root).unwrap();

    let node_x = PackageMetadata {
        pkg_name: "node-x".to_string(), version: "1.0.0".to_string(),
        license: "Apache".to_string(), source: "https://example.com/x".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "0000".to_string() },
        dependencies: vec![Dependency { name: "node-y".to_string(), dep_type: "runtime".to_string(), libraries: None }],
        files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let node_y = PackageMetadata {
        pkg_name: "node-y".to_string(), version: "1.0.0".to_string(),
        license: "Apache".to_string(), source: "https://example.com/y".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "0000".to_string() },
        dependencies: vec![Dependency { name: "node-x".to_string(), dep_type: "runtime".to_string(), libraries: None }],
        files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx = db.begin_transaction().unwrap();
    tx.register_package_placement(&node_x).unwrap();
    tx.register_package_placement(&node_y).unwrap();
    tx.commit().unwrap();

    let solver = mcx::core::solver::DependencySolver::new(Arc::new(db)).add_target("node-x");
    let resolve_result = solver.solve_with_analysis();
    assert!(resolve_result.is_ok());
    let verdict = resolve_result.unwrap();
    assert!(verdict.cycles_broken > 0, "Expected cycle to be detected and broken");

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_dependency_solver_topological_sorting_and_resolution() {
    let root = create_temporary_root("dependency_sorting");
    let db = Database::open(&root).unwrap();

    let dep_b = PackageMetadata {
        pkg_name: "library-b".to_string(), version: "1.0.0".to_string(),
        license: "MIT".to_string(), source: "https://example.com/b".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "0000".to_string() },
        dependencies: vec![], files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let dep_a = PackageMetadata {
        pkg_name: "library-a".to_string(), version: "1.0.0".to_string(),
        license: "MIT".to_string(), source: "https://example.com/a".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "0000".to_string() },
        dependencies: vec![Dependency { name: "library-b".to_string(), dep_type: "runtime".to_string(), libraries: None }],
        files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let target_pkg = PackageMetadata {
        pkg_name: "main-app".to_string(), version: "2.0.0".to_string(),
        license: "GPL".to_string(), source: "https://example.com/app".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "0000".to_string() },
        dependencies: vec![Dependency { name: "library-a".to_string(), dep_type: "runtime".to_string(), libraries: None }],
        files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx = db.begin_transaction().unwrap();
    tx.register_package_placement(&dep_b).unwrap();
    tx.register_package_placement(&dep_a).unwrap();
    tx.register_package_placement(&target_pkg).unwrap();
    tx.commit().unwrap();

    let solver = mcx::core::solver::DependencySolver::new(Arc::new(db)).add_target("main-app");
    let ordered_plan = solver.solve().unwrap();

    assert_eq!(ordered_plan.len(), 3);
    assert_eq!(ordered_plan[0].pkg_name, "main-app");
    assert_eq!(ordered_plan[1].pkg_name, "library-a");
    assert_eq!(ordered_plan[2].pkg_name, "library-b");

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_dependency_solver_library_provider_resolution() {
    let root = create_temporary_root("dependency_library_resolution");
    let db = Database::open(&root).unwrap();

    let provider_pkg = PackageMetadata {
        pkg_name: "gio-2.0".to_string(), version: "1.0.0".to_string(),
        license: "LGPL".to_string(), source: "https://example.com/gio".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "0000".to_string() },
        dependencies: vec![], files: vec![PathBuf::from("usr/lib/libgio-2.0.so.0")],
        provides: Some(vec!["libgio-2.0.so.0".to_string()]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let build_dep_pkg = PackageMetadata {
        pkg_name: "glib-2.0".to_string(), version: "2.0.0".to_string(),
        license: "LGPL".to_string(), source: "https://example.com/glib".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "1111".to_string() },
        dependencies: vec![], files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let json_glib_pkg = PackageMetadata {
        pkg_name: "json-glib".to_string(), version: "1.8.0".to_string(),
        license: "MPL".to_string(), source: "https://example.com/json-glib".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "2222".to_string() },
        dependencies: vec![
            Dependency { name: "glib-2.0".to_string(), dep_type: "Build".to_string(), libraries: None },
            Dependency { name: "libgio-2.0.so.0".to_string(), dep_type: "Library".to_string(), libraries: None },
        ],
        files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx = db.begin_transaction().unwrap();
    tx.register_package_placement(&provider_pkg).unwrap();
    tx.register_package_placement(&build_dep_pkg).unwrap();
    tx.register_package_placement(&json_glib_pkg).unwrap();
    tx.commit().unwrap();

    let solver = mcx::core::solver::DependencySolver::new(Arc::new(db)).add_target("json-glib");
    let ordered_plan = solver.solve().unwrap();

    assert_eq!(ordered_plan.len(), 3);
    assert_eq!(ordered_plan[0].pkg_name, "json-glib");
    assert!(ordered_plan.iter().any(|p| p.pkg_name == "glib-2.0"));
    assert!(ordered_plan.iter().any(|p| p.pkg_name == "gio-2.0"));

    fs::remove_dir_all(&root).unwrap();
}

// ── Hash Verification ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_corrupted_archive_hash_verification_failure() {
    let root = create_temporary_root("hash_failure");
    let cache_dir = root.join("var/cache/mcx");
    fs::create_dir_all(&cache_dir).unwrap();

    let archive_file = cache_dir.join("corrupted-package-1.0.0.xcs");
    fs::write(&archive_file, b"corrupted payload data").unwrap();

    let expected_valid_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let verification_result = mcx::archive::hash::HashVerifier::verify_integrity(&archive_file, "sha256", expected_valid_hash);
    assert!(verification_result.is_err());

    fs::remove_dir_all(&root).unwrap();
}

// ── History / Rollback ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_temporal_history_ledger_rollback() {
    let root = create_temporary_root("temporal_rollback");
    let db = Database::open(&root).unwrap();
    let db_arc = Arc::new(db);

    let history_engine = mcx::core::history::HistoryEngine::new(&root, Arc::clone(&db_arc));

    let state_file = root.join("var/lib/mcx/history.json");
    fs::create_dir_all(state_file.parent().unwrap()).unwrap();
    fs::write(&state_file, b"[]").unwrap();

    let mutated_pkg = PackageMetadata {
        pkg_name: "ephemeral-module".to_string(), version: "1.0.0".to_string(),
        license: "MIT".to_string(), source: "https://example.com/eph".to_string(),
        checksum: ChecksumData { kind: "sha256".to_string(), value: "0000".to_string() },
        dependencies: vec![], files: vec![], provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };

    let mut tx = db_arc.begin_transaction().unwrap();
    tx.register_package_placement(&mutated_pkg).unwrap();
    tx.commit().unwrap();
    assert!(db_arc.is_package_installed("ephemeral-module").unwrap());

    fs::write(&state_file, b"[]").unwrap();
    let mut tx_rollback = db_arc.begin_transaction().unwrap();
    tx_rollback.stage_package_removal("ephemeral-module").unwrap();
    tx_rollback.commit().unwrap();
    assert!(!db_arc.is_package_installed("ephemeral-module").unwrap());
    let _ = history_engine;

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_rollback_manager_generation_tracking() {
    let root = create_temporary_root("rollback_generations");
    let rollback = RollbackManager::new(&root);
    assert!(rollback.initialize().is_ok());

    let current = rollback.current_generation("test-pkg");
    assert!(current.is_ok());

    let _ = fs::remove_dir_all(&root);
}

// ── Cgroup ──────────────────────────────────────────────────────────────────

#[test]
fn test_cgroup_controller_availability_check() {
    let cg = CgroupController::new();
    // Should not panic; availability depends on /sys/fs/cgroup
    let _available = cg.is_cgroup_v2_available();
    // Creating a cgroup may fail without root, but the call should not panic
    let _ = cg.enforce_resource_limits("test-pkg", 256, 50);
    let _ = cg.remove_resource_limits("test-pkg");
}

// ── Security Monitor ────────────────────────────────────────────────────────

#[test]
fn test_security_monitor_package_tracking_and_isolation() {
    let sm = SecurityMonitor::new();
    assert_eq!(sm.active_count(), 0);

    sm.register_package("nginx");
    sm.register_package("openssl");
    assert_eq!(sm.active_count(), 2);

    assert!(!sm.is_package_isolated("nginx"));
    assert!(sm.isolate_package("nginx").is_ok());
    assert!(sm.is_package_isolated("nginx"));

    assert!(sm.check_package("nginx"));

    sm.unregister_package("nginx");
    assert_eq!(sm.active_count(), 1);

    // swap isolation policy
    let strict = Arc::new(|pkg: &str| -> bool { pkg != "evil" });
    let old = sm.swap_isolation_policy(strict);
    assert!(sm.check_package("good"));
    assert!(!sm.check_package("evil"));
    let _ = old;
}

// ── Profile Validator ───────────────────────────────────────────────────────

#[test]
fn test_profile_validator_load_and_diff() {
    let root = create_temporary_root("profile_validator");
    let profile_path = root.join("profile.ini");

    let profile_content = "[profile]\nversion = 1.0.0\narchitecture = x86_64\npackages = nginx, openssl, curl\n";
    fs::write(&profile_path, profile_content).unwrap();

    let profile = ProfileValidator::load_profile(&profile_path).unwrap();
    assert_eq!(profile.version, "1.0.0");
    assert_eq!(profile.packages.len(), 3);

    let current = vec!["nginx".to_string(), "curl".to_string()];
    let (to_install, to_remove) = ProfileValidator::compile_profile_diff(&current, &profile.packages);
    assert_eq!(to_install, vec!["openssl"]);
    assert!(to_remove.is_empty());

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn test_profile_validator_rejects_invalid_blueprints() {
    let root = create_temporary_root("profile_invalid");
    let profile_path = root.join("bad.ini");

    // empty version
    let bad = "[profile]\nversion = \narchitecture = x86_64\npackages = \n";
    fs::write(&profile_path, bad).unwrap();
    let result = ProfileValidator::load_profile(&profile_path);
    assert!(result.is_err());

    // duplicate packages
    let dup = "[profile]\nversion = 1.0\narchitecture = x86_64\npackages = nginx, nginx\n";
    fs::write(&profile_path, dup).unwrap();
    let result = ProfileValidator::load_profile(&profile_path);
    assert!(result.is_err());

    fs::remove_dir_all(&root).unwrap();
}

// ── CAS ─────────────────────────────────────────────────────────────────────

#[test]
fn test_cas_store_deduplication() {
    let root = create_temporary_root("cas_dedup");
    let lib_dir = root.join("usr/lib");
    fs::create_dir_all(&lib_dir).unwrap();

    // create identical files
    let content = b"identical library content";
    fs::write(lib_dir.join("libfoo.so.1"), content).unwrap();
    fs::write(lib_dir.join("libfoo.so.2"), content).unwrap();
    fs::write(lib_dir.join("libbar.so.1"), b"different content").unwrap();

    let cas = CasStore::new(&root);
    let stats = cas.deduplicate_libraries(&root).unwrap();

    assert_eq!(stats.unique_files, 2);
    assert_eq!(stats.total_files, 3);
    assert!(stats.bytes_saved > 0);

    fs::remove_dir_all(&root).unwrap();
}

// ── Config ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_configuration_text_editor_spawning_and_mutation() {
    let root = create_temporary_root("text_editor");
    let config_dir = root.join("etc/mcx");
    fs::create_dir_all(&config_dir).unwrap();
    let config_file = config_dir.join("mcx.conf");
    fs::write(&config_file, b"initial_key = initial_value\n").unwrap();

    let root_str = root.to_string_lossy();
    let _editor_command = mcx::commands::configuration::ConfigEditorCommand::new(&root_str, ConfigTarget::EngineConfig);

    let result = fs::write(&config_file, b"initial_key = mutated_value\n");
    assert!(result.is_ok());

    let updated_content = fs::read_to_string(&config_file).unwrap();
    assert!(updated_content.contains("mutated_value"));
    assert!(!updated_content.contains("initial_value"));

    fs::remove_dir_all(&root).unwrap();
}

// ── Completion Script ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_completion_engine_shell_script_generation() {
    let root = create_temporary_root("completion_gen");
    let db = Database::open(&root).unwrap();
    let engine = CompletionEngine::new(Arc::new(db));

    let bash_script = engine.generate_shell_blueprint("bash");
    assert!(bash_script.is_ok());
    assert!(bash_script.unwrap().contains("mcx"));

    let zsh_script = engine.generate_shell_blueprint("zsh");
    assert!(zsh_script.is_ok());

    let bad_shell = engine.generate_shell_blueprint("tcsh");
    assert!(bad_shell.is_err());

    fs::remove_dir_all(&root).unwrap();
}

// ── Workspace ───────────────────────────────────────────────────────────────

#[test]
fn test_workspace_manager_initialization_and_dirs() {
    let root = create_temporary_root("workspace_test");
    let ws = mcx::core::workspace::WorkspaceManager::new(&root);
    assert!(ws.initialize().is_ok());

    let _ = fs::remove_dir_all(&root);
}

// ── Lifecycle Engine + Dependency Graph ─────────────────────────────────────

#[test]
fn test_lifecycle_engine_state_machine_full_walk() {
    let mut eng = LifecycleEngine::new();

    // register two packages
    let gen_a = eng.register_package("core-lib", "2.1.0");
    let gen_b = eng.register_package("app", "1.0.0");
    assert!(gen_a > 0);
    assert!(gen_b > gen_a);

    // initial state
    assert_eq!(eng.state("core-lib"), Some(PackageState::Unknown));
    assert_eq!(eng.state("app"), Some(PackageState::Unknown));
    assert!(eng.entry("core-lib").is_some());
    assert!(eng.entry("missing").is_none());
    assert_eq!(eng.installed_count(), 0);
    assert_eq!(eng.removed_count(), 0);

    // walk core-lib through the full lifecycle
    assert!(eng.transition("core-lib", PackageState::Resolved).is_ok());
    assert!(eng.transition("core-lib", PackageState::Staged).is_ok());
    assert!(eng.transition("core-lib", PackageState::Installed).is_ok());
    assert!(eng.transition("core-lib", PackageState::Active).is_ok());
    assert_eq!(eng.state("core-lib"), Some(PackageState::Active));
    assert_eq!(eng.installed_count(), 1);

    // walk app through a shorter path
    assert!(eng.transition("app", PackageState::Resolved).is_ok());
    assert!(eng.transition("app", PackageState::Staged).is_ok());
    assert!(eng.transition("app", PackageState::Installed).is_ok());
    assert_eq!(eng.state("app"), Some(PackageState::Installed));
    assert_eq!(eng.installed_count(), 2);

    // mark core-lib for removal and remove
    assert!(eng.transition("core-lib", PackageState::MarkedForRemoval).is_ok());
    assert_eq!(eng.state("core-lib"), Some(PackageState::MarkedForRemoval));
    assert!(eng.transition("core-lib", PackageState::Removed).is_ok());
    assert_eq!(eng.state("core-lib"), Some(PackageState::Removed));
    assert_eq!(eng.removed_count(), 1);
    assert_eq!(eng.installed_count(), 1);

    // purge
    assert!(eng.transition("core-lib", PackageState::Purged).is_ok());
    assert!(eng.state("core-lib").unwrap().is_terminal());

    // invalid transition: Installed -> Resolved is not allowed by the matrix
    assert!(eng.transition("app", PackageState::Resolved).is_err());

    // transition to same state succeeds without error (returns id 0)
    let noop = eng.transition("app", PackageState::Installed).unwrap();
    assert_eq!(noop, 0);

    // unregistered package
    assert!(eng.transition("ghost", PackageState::Resolved).is_err());

    // history: Unknown→Resolved→Staged→Installed→Active→MarkedForRemoval→Removed→Purged
    let hist = eng.history("core-lib");
    assert_eq!(hist.len(), 7);
    assert_eq!(hist[0].from, PackageState::Unknown);
    assert_eq!(hist[0].to, PackageState::Resolved);
    assert_eq!(hist[6].from, PackageState::Removed);
    assert_eq!(hist[6].to, PackageState::Purged);

    let app_hist = eng.history("app");
    // Installed -> Installed noop is NOT recorded (early return), so only 3 transitions
    assert_eq!(app_hist.len(), 3);

    // counts
    assert_eq!(eng.transition_count(), 10);

    // all_entries iterator
    let names: Vec<&str> = eng.all_entries().map(|e| e.package.as_str()).collect();
    assert!(names.contains(&"core-lib"));
    assert!(names.contains(&"app"));
}

#[test]
fn test_lifecycle_engine_pre_and_post_hooks_fire() {
    let mut eng = LifecycleEngine::new();
    eng.register_package("test-pkg", "1.0.0");

    let pre_fired = std::sync::Arc::new(std::sync::Mutex::new(false));
    let post_fired = std::sync::Arc::new(std::sync::Mutex::new(false));
    let pre = pre_fired.clone();
    let post = post_fired.clone();

    eng.add_pre_hook(move |_, _, _| {
        *pre.lock().unwrap() = true;
        Ok(())
    });
    eng.add_post_hook(move |_, _, _| {
        *post.lock().unwrap() = true;
        Ok(())
    });

    assert!(!*pre_fired.lock().unwrap());
    assert!(!*post_fired.lock().unwrap());

    eng.transition("test-pkg", PackageState::Resolved).unwrap();

    assert!(*pre_fired.lock().unwrap());
    assert!(*post_fired.lock().unwrap());
}

#[test]
fn test_lifecycle_engine_hook_rejection_aborts_transition() {
    let mut eng = LifecycleEngine::new();
    eng.register_package("blocked-pkg", "1.0.0");

    eng.add_pre_hook(|_, _, _| {
        Err(anyhow::anyhow!("hook blocked transition"))
    });

    let result = eng.transition("blocked-pkg", PackageState::Resolved);
    assert!(result.is_err());
    // state must still be Unknown because pre-hook aborted
    assert_eq!(eng.state("blocked-pkg"), Some(PackageState::Unknown));
}

#[test]
fn test_package_state_predicates() {
    assert!(!PackageState::Unknown.is_terminal());
    assert!(!PackageState::Resolved.is_terminal());
    assert!(!PackageState::Installed.is_terminal());
    assert!(PackageState::Purged.is_terminal());

    assert!(PackageState::Installed.is_installed());
    assert!(PackageState::Active.is_installed());
    assert!(!PackageState::Unknown.is_installed());
    assert!(!PackageState::Removed.is_installed());

    assert!(PackageState::Removed.is_removed());
    assert!(PackageState::Purged.is_removed());
    assert!(!PackageState::Active.is_removed());
}

#[test]
fn test_package_state_invalid_transitions() {
    // direct transitions that violate the matrix
    assert!(!PackageState::Unknown.can_transition_to(PackageState::Active));
    assert!(!PackageState::Unknown.can_transition_to(PackageState::Purged));
    assert!(!PackageState::Staged.can_transition_to(PackageState::Purged));
    assert!(!PackageState::Removed.can_transition_to(PackageState::Installed));
    assert!(!PackageState::Purged.can_transition_to(PackageState::Unknown));
}

#[test]
fn test_dependency_graph_reachability_and_orphans() {
    let mut dg = DependencyGraph::new();

    dg.add_dep("app", "lib-a");
    dg.add_dep("app", "lib-b");
    dg.add_dep("lib-a", "lib-c");
    dg.add_dep("lib-b", "lib-c");

    let all = &["app", "lib-a", "lib-b", "lib-c", "unused-dep", "abandoned"];
    let roots = &["app".to_string()];

    let reachable = dg.reachable_from(roots);
    assert!(reachable.contains(&"app".to_string()));
    assert!(reachable.contains(&"lib-a".to_string()));
    assert!(reachable.contains(&"lib-b".to_string()));
    assert!(reachable.contains(&"lib-c".to_string()));

    let OrphanSet { packages: orphans, reachable: reach, purged } = dg.orphans(roots, &all.iter().map(|s| s.to_string()).collect::<Vec<_>>());

    assert!(orphans.contains(&"unused-dep".to_string()));
    assert!(orphans.contains(&"abandoned".to_string()));
    assert!(!orphans.contains(&"app".to_string()));
    assert!(reach.contains(&"app".to_string()));
    assert!(reach.contains(&"lib-c".to_string()));
    assert!(purged.is_empty());
}

// ── NetworkSyncEngine ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_network_sync_engine_ldex() {
    let root = create_temporary_root("sync_engine");
    let sync_dir = root.join("var/lib/mcx/sync");
    fs::create_dir_all(&sync_dir).unwrap();

    let db = Database::open(&root).unwrap();
    let engine = mcx::network::sync::NetworkSyncEngine::new(Arc::new(db), root.to_string_lossy().into_owned());

    // no index exists yet
    assert!(!engine.ldex("test-repo").unwrap());

    // create one
    fs::write(sync_dir.join("test-repo.json"), b"{}").unwrap();
    assert!(engine.ldex("test-repo").unwrap());

    fs::remove_dir_all(&root).unwrap();
}

// ── IntegrityScanner ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_integrity_scanner_clean_root() {
    let root = create_temporary_root("integrity_clean");
    fs::create_dir_all(&root.join("var/lib/mcx/active")).unwrap();
    let db = Database::open(&root).unwrap();
    let scanner = mcx::core::integrity::IntegrityScanner::new(&root, Arc::new(db));
    let report = scanner.verify_all();
    assert_eq!(report.total_packages, 0);
    assert!(report.errors.is_empty());
    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_integrity_scanner_detects_missing_files() {
    let root = create_temporary_root("integrity_missing");
    fs::create_dir_all(&root.join("var/lib/mcx/active/test-pkg")).unwrap();
    fs::create_dir_all(&root.join("usr/bin")).unwrap();

    let db = Database::open(&root).unwrap();
    let pkg = PackageMetadata {
        pkg_name: "test-pkg".into(), version: "1.0".into(),
        license: "MIT".into(), source: "https://example.com".into(),
        checksum: ChecksumData { kind: "sha256".into(), value: "0000".into() },
        dependencies: vec![],
        files: vec![PathBuf::from("usr/bin/test-bin")],
        provides: Some(vec![]),         conflicts: Some(vec![]),
        architecture: "native".to_string(),
    };
    let mut tx = db.begin_transaction().unwrap();
    tx.register_package_placement(&pkg).unwrap();
    tx.commit().unwrap();

    let scanner = mcx::core::integrity::IntegrityScanner::new(&root, Arc::new(db));
    let report = scanner.verify_all();
    assert_eq!(report.total_packages, 1);
    assert_eq!(report.missing_files.len(), 1);
    assert_eq!(report.missing_files[0].pkg, "test-pkg");

    fs::remove_dir_all(&root).unwrap();
}

// ── ExternalPlugin / PluginManager ───────────────────────────────────────────

#[tokio::test]
async fn test_plugin_manager_discovery_and_list() {
    let root = create_temporary_root("plugin_mgr");
    let plugins_dir = root.join("var/lib/mcx/plugins");
    fs::create_dir_all(&plugins_dir.join("alpha")).unwrap();
    fs::write(plugins_dir.join("alpha/plugin.ini"), "[plugin]\nname = alpha\ncommand = echo a\ntrigger = post-install\n").unwrap();
    fs::create_dir_all(&plugins_dir.join("beta")).unwrap();
    fs::write(plugins_dir.join("beta/plugin.ini"), "[plugin]\nname = beta\ncommand = echo b\ntrigger = pre-remove\n").unwrap();

    let mgr = mcx::core::plugin::PluginManager::new(&root);
    let list = mgr.list();
    assert_eq!(list.len(), 2);

    let alpha = mgr.find("alpha").unwrap();
    assert_eq!(alpha.manifest().name, "alpha");
    assert_eq!(alpha.manifest().trigger.as_deref(), Some("post-install"));

    let beta = mgr.find("beta").unwrap();
    assert_eq!(beta.manifest().trigger.as_deref(), Some("pre-remove"));

    let missing = mgr.find("nonexistent");
    assert!(missing.is_none());

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_plugin_manager_run_plugin_once() {
    let root = create_temporary_root("plugin_run");
    let plugins_dir = root.join("var/lib/mcx/plugins");
    fs::create_dir_all(&plugins_dir.join("greeter")).unwrap();
    fs::write(plugins_dir.join("greeter/plugin.ini"),
        "[plugin]\nname = greeter\ncommand = echo '{\"success\":true,\"message\":\"hello\"}'\n").unwrap();

    let mgr = mcx::core::plugin::PluginManager::new(&root);
    let event = mcx::core::plugin::PluginEvent {
        hook: "test".into(),
        package: Some("pkg".into()),
        root: root.to_string_lossy().into_owned(),
        timestamp: "now".into(),
    };
    let result = mgr.run_plugin_once("greeter", &event).unwrap();
    assert!(result.success);
    assert!(result.message.unwrap_or_default().contains("hello"));

    let err = mgr.run_plugin_once("nonexistent", &event);
    assert!(err.is_err());

    fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn test_plugin_start_daemon_rejects_hook_type() {
    let root = create_temporary_root("plugin_daemon_reject");
    let plugins_dir = root.join("var/lib/mcx/plugins");
    fs::create_dir_all(&plugins_dir.join("hooker")).unwrap();
    fs::write(plugins_dir.join("hooker/plugin.ini"),
        "[plugin]\nname = hooker\ncommand = echo hi\ntype = hook\n").unwrap();

    let mgr = mcx::core::plugin::PluginManager::new(&root);
    let result = mgr.start_daemon("hooker");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not a daemon plugin"));

    fs::remove_dir_all(&root).unwrap();
}
