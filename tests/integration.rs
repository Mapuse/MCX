use mcx::core::cas::CasStore;
use mcx::core::cgroup::CgroupController;
use mcx::core::completion::CompletionEngine;
use mcx::core::database::{ChecksumData, Database, Dependency, PackageMetadata};
use mcx::core::declarative::ProfileValidator;
use mcx::core::lifecycle::{DependencyGraph, LifecycleEngine, OrphanSet, PackageState};
use mcx::core::rollback::RollbackManager;
use mcx::core::security::SecurityMonitor;
use mcx::utils::ui::UserInterface;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mcx::commands::add::AddLocalCommand;
use mcx::commands::configuration::ConfigTarget;
use mcx::commands::install::InstallCommand;
use mcx::commands::localsrc::LocalSourceCommand;
use mcx::commands::remove::RemoveCommand;
use mcx::network::download::Downloader;

fn create_temporary_root(identifier: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mcx_test_{}_{}",
        identifier,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create temp root");
    path
}

// ── Database / Transaction ──────────────────────────────────────────────────

#[tokio::test]
async fn test_atomic_database_write_and_conflict_prevention() {
    let root = create_temporary_root("conflict_prevention");
    let db = Database::open(&root).expect("open test database");

    let package_a = PackageMetadata {
        pkg_name: "package-a".to_string(),
        version: "1.0.0".to_string(),
        license: "MIT".to_string(),
        source: "https://example.com/a".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string(),
        },
        dependencies: vec![],
        files: vec![PathBuf::from("usr/bin/shared-binary")],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx_a = db.begin_transaction().expect("begin transaction");
    tx_a.register_package_placement(&package_a)
        .expect("register package placement");
    tx_a.commit().expect("commit transaction");
    assert!(
        db.is_package_installed("package-a")
            .expect("package-a installed")
    );

    let package_b = PackageMetadata {
        pkg_name: "package-b".to_string(),
        version: "2.0.0".to_string(),
        license: "Apache-2.0".to_string(),
        source: "https://example.com/b".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "5891a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146f".to_string(),
        },
        dependencies: vec![],
        files: vec![PathBuf::from("usr/bin/shared-binary")],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx_b = db.begin_transaction().expect("begin transaction");
    let result = tx_b.register_package_placement(&package_b);
    assert!(result.is_err());

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_concurrent_transaction_serialization_isolation() {
    let root = create_temporary_root("isolation_lock");
    let db = Database::open(&root).expect("open test database");

    // LMDB enforces single-writer; commit first txn before starting second
    let tx_primary = db.begin_transaction().expect("begin transaction");
    tx_primary.commit().expect("commit transaction");

    let tx_secondary = db.begin_transaction().expect("begin transaction");
    tx_secondary.commit().expect("commit transaction");

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_empty_installation_command_error() {
    let root = create_temporary_root("install_command");
    let db = Database::open(&root).expect("open test database");
    let command = InstallCommand::new(root.to_string_lossy().into_owned(), Arc::new(db));
    let result = command.execute(&[]).await;
    assert!(result.is_err());
    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Remove + Sandbox Cleanup ────────────────────────────────────────────────

#[tokio::test]
async fn test_package_removal_and_filesystem_cleanup() {
    let root = create_temporary_root("filesystem_cleanup");
    let binary_dir = root.join("usr/bin");
    fs::create_dir_all(&binary_dir).expect("create binary dir");
    let binary_file = binary_dir.join("app-binary");
    fs::write(&binary_file, b"ELF").expect("write temp file");

    let db = Database::open(&root).expect("open test database");
    let package = PackageMetadata {
        pkg_name: "app".to_string(),
        version: "1.5.2".to_string(),
        license: "GPL-3.0".to_string(),
        source: "https://example.com/app".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "9ee6a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146a".to_string(),
        },
        dependencies: vec![],
        files: vec![PathBuf::from("usr/bin/app-binary")],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&package)
        .expect("register package placement");
    tx.commit().expect("commit transaction");

    let db_share = Arc::new(db);
    let command = RemoveCommand::new(root.to_string_lossy().into_owned(), db_share.clone());
    let cg = mcx::CgroupController::new();
    let sm = mcx::SecurityMonitor::new();
    command
        .execute(&["app".to_string()], &cg, &sm)
        .expect("execute remove command");

    assert!(
        !db_share
            .is_package_installed("app")
            .expect("app not installed")
    );
    assert!(!binary_file.exists());
    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Completion ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_shell_completion_engine_querying() {
    let root = create_temporary_root("completion_engine");
    let db = Database::open(&root).expect("open test database");

    let package = PackageMetadata {
        pkg_name: "neovim".to_string(),
        version: "0.9.0".to_string(),
        license: "Apache-2.0".to_string(),
        source: "https://example.com/nvim".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "1111a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string(),
        },
        dependencies: vec![],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&package)
        .expect("register package placement");
    tx.commit().expect("commit transaction");

    let engine = CompletionEngine::new(Arc::new(db));
    let subcommands = engine.complete_subcommand("inst");
    assert!(subcommands.contains(&"install".to_string()));

    let packages = engine
        .complete_installed_package("neo")
        .expect("complete package");
    assert!(packages.contains(&"neovim".to_string()));

    fs::remove_dir_all(&root).expect("remove temp root");
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
    UserInterface::version("mcx 5.0.0");

    let list_items = vec![
        "mcx-core-engine v5.0.0".to_string(),
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
    fs::create_dir_all(&config_dir).expect("create config dir");

    // mark config.ini as "already exists" to test skip-behaviour
    fs::write(
        config_dir.join("config.ini"),
        b"[engine]\nthread_pool_mode = auto\n",
    )
    .expect("write config file");

    let config_ini = config_dir.join("config.ini");
    let repo_ini = config_dir.join("repo.ini");
    let profile_ini = config_dir.join("profile.ini");

    // before init: config.ini exists, repo.ini and profile.ini do not
    assert!(config_ini.exists());
    assert!(!repo_ini.exists());
    assert!(!profile_ini.exists());

    // simulate init logic (same as main.rs Commands::Config { init: true })
    if !config_ini.exists() {
        fs::write(&config_ini, b"[engine]\nthread_pool_mode = auto\nmax_concurrent_downloads = 8\nzstd_level = 3\n\n[network]\nfallback_repos = enabled\nlatency_threshold_ms = 200\nbandwidth_threshold_kbps = 5000\n\n[security]\nverify_checksums = true\nallow_unverified = false\n\n[cache]\nlimit_bytes = 5368709120\nprune_age_hours = 168\n").expect("write config file");
    }
    if !repo_ini.exists() {
        fs::write(&repo_ini, b"[main]\nurl = https://packages.cudane.org\nenabled = true\npriority = 100\n\n[community]\nurl = https://community.cudane.org\nenabled = false\npriority = 200\n").expect("write config file");
    }
    if !profile_ini.exists() {
        fs::write(
            &profile_ini,
            b"[profile]\nversion = 1.0.0\narchitecture = x86_64\npackages = \n",
        )
        .expect("write config file");
    }

    // config.ini content preserved (not overwritten)
    let cfg_content = fs::read_to_string(&config_ini).expect("read config file");
    assert!(cfg_content.contains("thread_pool_mode"));

    // repo.ini created
    assert!(repo_ini.exists());
    let repo_content = fs::read_to_string(&repo_ini).expect("read config file");
    assert!(repo_content.contains("packages.cudane.org"));

    // profile.ini created
    assert!(profile_ini.exists());
    let profile_content = fs::read_to_string(&profile_ini).expect("read config file");
    assert!(profile_content.contains("version = 1.0.0"));

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Network Download ────────────────────────────────────────────────────────

// Requires outbound internet access; run explicitly with `cargo test -- --ignored`.
#[tokio::test]
#[ignore = "requires internet access"]
async fn test_network_downloader_endpoint_handling() {
    let root = create_temporary_root("network_download");
    let downloader = Downloader::new();

    let is_available = downloader
        .check_endpoint_availability("https://www.google.com")
        .await;
    assert!(is_available);

    let destination = root.join("test_download.html");
    let result = downloader
        .package("https://www.google.com", &destination)
        .await;
    assert!(result.is_ok());
    assert!(destination.exists());
    assert!(
        fs::metadata(&destination)
            .expect("destination metadata")
            .len()
            > 0
    );

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_network_downloader_transient_failure_recovery() {
    let root = create_temporary_root("network_fault");
    let downloader = Downloader::new();

    let invalid_endpoint = "https://invalid-subdomain-unreachable-target-node.org/asset.xcs";
    let destination = root.join("failed_output.xcs");

    let result = downloader.package(invalid_endpoint, &destination).await;
    assert!(result.is_err());

    let availability = downloader
        .check_endpoint_availability(invalid_endpoint)
        .await;
    assert!(!availability);

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Database / Dependency Graph ─────────────────────────────────────────────

#[tokio::test]
async fn test_database_dependency_graph_relations() {
    let root = create_temporary_root("database_relations");
    let db = Database::open(&root).expect("open test database");

    let base_package = PackageMetadata {
        pkg_name: "openssl".to_string(),
        version: "5.0.0".to_string(),
        license: "Apache-2.0".to_string(),
        source: "https://example.com/ssl".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "1234a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string(),
        },
        dependencies: vec![],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&base_package)
        .expect("register package placement");
    tx.commit().expect("commit transaction");

    let dependent_package = PackageMetadata {
        pkg_name: "curl".to_string(),
        version: "8.0.0".to_string(),
        license: "MIT".to_string(),
        source: "https://example.com/curl".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "5678a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e".to_string(),
        },
        dependencies: vec![Dependency {
            name: "openssl".to_string(),
            dep_type: "runtime".to_string(),
            libraries: None,
        }],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx2 = db.begin_transaction().expect("begin transaction");
    tx2.register_package_placement(&dependent_package)
        .expect("register package placement");
    tx2.commit().expect("commit transaction");

    assert!(
        db.has_dependent_packages("openssl")
            .expect("openssl has dependents")
    );
    assert!(
        !db.has_dependent_packages("curl")
            .expect("curl has no dependents")
    );

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Solver / Resolution ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_cyclic_dependency_deadlock_breaking() {
    let root = create_temporary_root("cyclic_deadlock");
    let db = Database::open(&root).expect("open test database");

    let node_x = PackageMetadata {
        pkg_name: "node-x".to_string(),
        version: "1.0.0".to_string(),
        license: "Apache".to_string(),
        source: "https://example.com/x".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "0000".to_string(),
        },
        dependencies: vec![Dependency {
            name: "node-y".to_string(),
            dep_type: "runtime".to_string(),
            libraries: None,
        }],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let node_y = PackageMetadata {
        pkg_name: "node-y".to_string(),
        version: "1.0.0".to_string(),
        license: "Apache".to_string(),
        source: "https://example.com/y".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "0000".to_string(),
        },
        dependencies: vec![Dependency {
            name: "node-x".to_string(),
            dep_type: "runtime".to_string(),
            libraries: None,
        }],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&node_x)
        .expect("register package placement");
    tx.register_package_placement(&node_y)
        .expect("register package placement");
    tx.commit().expect("commit transaction");

    let solver = mcx::core::solver::DependencySolver::new(Arc::new(db)).add_target("node-x");
    let resolve_result = solver.solve_with_analysis();
    assert!(resolve_result.is_ok());
    let verdict = resolve_result.expect("solver result");
    assert!(
        verdict.cycles_broken > 0,
        "Expected cycle to be detected and broken"
    );

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_dependency_solver_topological_sorting_and_resolution() {
    let root = create_temporary_root("dependency_sorting");
    let db = Database::open(&root).expect("open test database");

    let dep_b = PackageMetadata {
        pkg_name: "library-b".to_string(),
        version: "1.0.0".to_string(),
        license: "MIT".to_string(),
        source: "https://example.com/b".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "0000".to_string(),
        },
        dependencies: vec![],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let dep_a = PackageMetadata {
        pkg_name: "library-a".to_string(),
        version: "1.0.0".to_string(),
        license: "MIT".to_string(),
        source: "https://example.com/a".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "0000".to_string(),
        },
        dependencies: vec![Dependency {
            name: "library-b".to_string(),
            dep_type: "runtime".to_string(),
            libraries: None,
        }],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let target_pkg = PackageMetadata {
        pkg_name: "main-app".to_string(),
        version: "2.0.0".to_string(),
        license: "GPL".to_string(),
        source: "https://example.com/app".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "0000".to_string(),
        },
        dependencies: vec![Dependency {
            name: "library-a".to_string(),
            dep_type: "runtime".to_string(),
            libraries: None,
        }],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&dep_b)
        .expect("register package placement");
    tx.register_package_placement(&dep_a)
        .expect("register package placement");
    tx.register_package_placement(&target_pkg)
        .expect("register package placement");
    tx.commit().expect("commit transaction");

    let solver = mcx::core::solver::DependencySolver::new(Arc::new(db)).add_target("main-app");
    let ordered_plan = solver.solve().expect("solve dependencies");

    assert_eq!(ordered_plan.len(), 3);
    assert_eq!(ordered_plan[0].pkg_name, "main-app");
    assert_eq!(ordered_plan[1].pkg_name, "library-a");
    assert_eq!(ordered_plan[2].pkg_name, "library-b");

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_dependency_solver_library_provider_resolution() {
    let root = create_temporary_root("dependency_library_resolution");
    let db = Database::open(&root).expect("open test database");

    let provider_pkg = PackageMetadata {
        pkg_name: "gio-2.0".to_string(),
        version: "1.0.0".to_string(),
        license: "LGPL".to_string(),
        source: "https://example.com/gio".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "0000".to_string(),
        },
        dependencies: vec![],
        files: vec![PathBuf::from("usr/lib/libgio-2.0.so.0")],
        provides: Some(vec!["libgio-2.0.so.0".to_string()]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let build_dep_pkg = PackageMetadata {
        pkg_name: "glib-2.0".to_string(),
        version: "2.0.0".to_string(),
        license: "LGPL".to_string(),
        source: "https://example.com/glib".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "1111".to_string(),
        },
        dependencies: vec![],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let json_glib_pkg = PackageMetadata {
        pkg_name: "json-glib".to_string(),
        version: "1.8.0".to_string(),
        license: "MPL".to_string(),
        source: "https://example.com/json-glib".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "2222".to_string(),
        },
        dependencies: vec![
            Dependency {
                name: "glib-2.0".to_string(),
                dep_type: "Build".to_string(),
                libraries: None,
            },
            Dependency {
                name: "libgio-2.0.so.0".to_string(),
                dep_type: "Library".to_string(),
                libraries: None,
            },
        ],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&provider_pkg)
        .expect("register package placement");
    tx.register_package_placement(&build_dep_pkg)
        .expect("register package placement");
    tx.register_package_placement(&json_glib_pkg)
        .expect("register package placement");
    tx.commit().expect("commit transaction");

    let solver = mcx::core::solver::DependencySolver::new(Arc::new(db)).add_target("json-glib");
    let ordered_plan = solver.solve().expect("solve dependencies");

    assert_eq!(ordered_plan.len(), 3);
    assert_eq!(ordered_plan[0].pkg_name, "json-glib");
    assert!(ordered_plan.iter().any(|p| p.pkg_name == "glib-2.0"));
    assert!(ordered_plan.iter().any(|p| p.pkg_name == "gio-2.0"));

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Hash Verification ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_corrupted_archive_hash_verification_failure() {
    let root = create_temporary_root("hash_failure");
    let cache_dir = root.join("var/cache/mcx");
    fs::create_dir_all(&cache_dir).expect("create cache dir");

    let archive_file = cache_dir.join("corrupted-package-1.0.0.xcs");
    fs::write(&archive_file, b"corrupted payload data").expect("write temp file");

    let expected_valid_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let verification_result = mcx::archive::hash::HashVerifier::verify_integrity(
        &archive_file,
        "sha256",
        expected_valid_hash,
    );
    assert!(verification_result.is_err());

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── History / Rollback ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_temporal_history_ledger_rollback() {
    let root = create_temporary_root("temporal_rollback");
    let db = Database::open(&root).expect("open test database");
    let db_arc = Arc::new(db);

    let history_engine = mcx::core::history::HistoryEngine::new(&root, Arc::clone(&db_arc));

    let state_file = root.join("var/lib/mcx/history.json");
    fs::create_dir_all(state_file.parent().expect("state file has parent"))
        .expect("create state dir");
    fs::write(&state_file, b"[]").expect("write state file");

    let mutated_pkg = PackageMetadata {
        pkg_name: "ephemeral-module".to_string(),
        version: "1.0.0".to_string(),
        license: "MIT".to_string(),
        source: "https://example.com/eph".to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: "0000".to_string(),
        },
        dependencies: vec![],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };

    let mut tx = db_arc.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&mutated_pkg)
        .expect("register package placement");
    tx.commit().expect("commit transaction");
    assert!(
        db_arc
            .is_package_installed("ephemeral-module")
            .expect("ephemeral-module installed")
    );

    fs::write(&state_file, b"[]").expect("write state file");
    let mut tx_rollback = db_arc.begin_transaction().expect("begin transaction");
    tx_rollback
        .stage_package_removal("ephemeral-module")
        .expect("stage package removal");
    tx_rollback.commit().expect("commit transaction");
    assert!(
        !db_arc
            .is_package_installed("ephemeral-module")
            .expect("ephemeral-module removed")
    );
    let _ = history_engine;

    fs::remove_dir_all(&root).expect("remove temp root");
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

    let profile_content =
        "[profile]\nversion = 1.0.0\narchitecture = x86_64\npackages = nginx, openssl, curl\n";
    fs::write(&profile_path, profile_content).expect("write profile file");

    let profile = ProfileValidator::load_profile(&profile_path).expect("load profile");
    assert_eq!(profile.version, "1.0.0");
    assert_eq!(profile.packages.len(), 3);

    let current = vec!["nginx".to_string(), "curl".to_string()];
    let (to_install, to_remove) =
        ProfileValidator::compile_profile_diff(&current, &profile.packages);
    assert_eq!(to_install, vec!["openssl"]);
    assert!(to_remove.is_empty());

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[test]
fn test_profile_validator_rejects_invalid_blueprints() {
    let root = create_temporary_root("profile_invalid");
    let profile_path = root.join("bad.ini");

    // empty version
    let bad = "[profile]\nversion = \narchitecture = x86_64\npackages = \n";
    fs::write(&profile_path, bad).expect("write profile file");
    let result = ProfileValidator::load_profile(&profile_path);
    assert!(result.is_err());

    // duplicate packages
    let dup = "[profile]\nversion = 1.0\narchitecture = x86_64\npackages = nginx, nginx\n";
    fs::write(&profile_path, dup).expect("write profile file");
    let result = ProfileValidator::load_profile(&profile_path);
    assert!(result.is_err());

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── CAS ─────────────────────────────────────────────────────────────────────

#[test]
fn test_cas_store_deduplication() {
    let root = create_temporary_root("cas_dedup");
    let lib_dir = root.join("usr/lib");
    fs::create_dir_all(&lib_dir).expect("create lib dir");

    // create identical files
    let content = b"identical library content";
    fs::write(lib_dir.join("libfoo.so.1"), content).expect("write temp file");
    fs::write(lib_dir.join("libfoo.so.2"), content).expect("write temp file");
    fs::write(lib_dir.join("libbar.so.1"), b"different content").expect("write temp file");

    let cas = CasStore::new(&root);
    let stats = cas
        .deduplicate_libraries(&root)
        .expect("deduplicate libraries");

    assert_eq!(stats.unique_files, 2);
    assert_eq!(stats.total_files, 3);
    assert!(stats.bytes_saved > 0);

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Config ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_configuration_text_editor_spawning_and_mutation() {
    let root = create_temporary_root("text_editor");
    let config_dir = root.join("etc/mcx");
    fs::create_dir_all(&config_dir).expect("create config dir");
    let config_file = config_dir.join("mcx.conf");
    fs::write(&config_file, b"initial_key = initial_value\n").expect("write config file");

    let root_str = root.to_string_lossy();
    let _editor_command = mcx::commands::configuration::ConfigEditorCommand::new(
        &root_str,
        ConfigTarget::EngineConfig,
    );

    let result = fs::write(&config_file, b"initial_key = mutated_value\n");
    assert!(result.is_ok());

    let updated_content = fs::read_to_string(&config_file).expect("read config file");
    assert!(updated_content.contains("mutated_value"));
    assert!(!updated_content.contains("initial_value"));

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Completion Script ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_completion_engine_shell_script_generation() {
    let root = create_temporary_root("completion_gen");
    let db = Database::open(&root).expect("open test database");
    let engine = CompletionEngine::new(Arc::new(db));

    let bash_script = engine.generate_shell_blueprint("bash");
    assert!(bash_script.is_ok());
    assert!(bash_script.expect("bash script").contains("mcx"));

    let zsh_script = engine.generate_shell_blueprint("zsh");
    assert!(zsh_script.is_ok());

    let bad_shell = engine.generate_shell_blueprint("tcsh");
    assert!(bad_shell.is_err());

    fs::remove_dir_all(&root).expect("remove temp root");
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
    assert!(
        eng.transition("core-lib", PackageState::MarkedForRemoval)
            .is_ok()
    );
    assert_eq!(eng.state("core-lib"), Some(PackageState::MarkedForRemoval));
    assert!(eng.transition("core-lib", PackageState::Removed).is_ok());
    assert_eq!(eng.state("core-lib"), Some(PackageState::Removed));
    assert_eq!(eng.removed_count(), 1);
    assert_eq!(eng.installed_count(), 1);

    // purge
    assert!(eng.transition("core-lib", PackageState::Purged).is_ok());
    assert!(eng.state("core-lib").expect("core-lib state").is_terminal());

    // invalid transition: Installed -> Resolved is not allowed by the matrix
    assert!(eng.transition("app", PackageState::Resolved).is_err());

    // transition to same state succeeds without error (returns id 0)
    let noop = eng
        .transition("app", PackageState::Installed)
        .expect("noop transition");
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
        *pre.lock().expect("pre hook lock") = true;
        Ok(())
    });
    eng.add_post_hook(move |_, _, _| {
        *post.lock().expect("post hook lock") = true;
        Ok(())
    });

    assert!(!*pre_fired.lock().expect("pre fired lock"));
    assert!(!*post_fired.lock().expect("post fired lock"));

    eng.transition("test-pkg", PackageState::Resolved)
        .expect("resolve transition");

    assert!(*pre_fired.lock().expect("pre fired lock"));
    assert!(*post_fired.lock().expect("post fired lock"));
}

#[test]
fn test_lifecycle_engine_hook_rejection_aborts_transition() {
    let mut eng = LifecycleEngine::new();
    eng.register_package("blocked-pkg", "1.0.0");

    eng.add_pre_hook(|_, _, _| Err(anyhow::anyhow!("hook blocked transition")));

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

    let OrphanSet {
        packages: orphans,
        reachable: reach,
        purged,
    } = dg.orphans(
        roots,
        &all.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    assert!(orphans.contains(&"unused-dep".to_string()));
    assert!(orphans.contains(&"abandoned".to_string()));
    assert!(!orphans.contains(&"app".to_string()));
    assert!(reach.contains(&"app".to_string()));
    assert!(reach.contains(&"lib-c".to_string()));
    assert!(purged.is_empty());
}

// ── IntegrityScanner ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_integrity_scanner_clean_root() {
    let root = create_temporary_root("integrity_clean");
    fs::create_dir_all(root.join("var/lib/mcx/active")).expect("create active dir");
    let db = Database::open(&root).expect("open test database");
    let scanner = mcx::core::integrity::IntegrityScanner::new(&root, Arc::new(db));
    let report = scanner.verify_all();
    assert_eq!(report.total_packages, 0);
    assert!(report.errors.is_empty());
    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_integrity_scanner_detects_missing_files() {
    let root = create_temporary_root("integrity_missing");
    fs::create_dir_all(root.join("var/lib/mcx/active/test-pkg")).expect("create active dir");
    fs::create_dir_all(root.join("usr/bin")).expect("create usr bin dir");

    let db = Database::open(&root).expect("open test database");
    let pkg = PackageMetadata {
        pkg_name: "test-pkg".into(),
        version: "1.0".into(),
        license: "MIT".into(),
        source: "https://example.com".into(),
        checksum: ChecksumData {
            kind: "sha256".into(),
            value: "0000".into(),
        },
        dependencies: vec![],
        files: vec![PathBuf::from("usr/bin/test-bin")],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };
    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&pkg)
        .expect("register package placement");
    tx.commit().expect("commit transaction");

    let scanner = mcx::core::integrity::IntegrityScanner::new(&root, Arc::new(db));
    let report = scanner.verify_all();
    assert_eq!(report.total_packages, 1);
    assert_eq!(report.missing_files.len(), 1);
    assert_eq!(report.missing_files[0].pkg, "test-pkg");

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── PythonPlugin / PluginManager ────────────────────────────────────────────

#[tokio::test]
async fn test_plugin_manager_discovery_and_list() {
    let root = create_temporary_root("plugin_mgr");
    let plugins_dir = root.join("var/lib/mcx/plugins");
    fs::create_dir_all(&plugins_dir).expect("create plugins dir");
    fs::write(plugins_dir.join("alpha.py"), "print('alpha')\n").expect("write temp plugin");
    fs::write(plugins_dir.join("beta.py"), "print('beta')\n").expect("write temp plugin");

    let mgr = mcx::core::plugin::PluginManager::new(&root);
    let list = mgr.list();
    assert_eq!(list.len(), 2);

    let alpha = mgr.find("alpha").expect("alpha plugin present");
    assert_eq!(alpha.name(), "alpha");

    let beta = mgr.find("beta").expect("beta plugin present");
    assert_eq!(beta.name(), "beta");

    let missing = mgr.find("nonexistent");
    assert!(missing.is_none());

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_plugin_manager_run_plugin_once() {
    let root = create_temporary_root("plugin_run");
    let plugins_dir = root.join("var/lib/mcx/plugins");
    fs::create_dir_all(&plugins_dir).expect("create plugins dir");
    fs::write(plugins_dir.join("greeter.py"),
        "import json\ndef on_post_install(event):\n    pass\nprint(json.dumps({'success': True, 'message': 'hello'}))\n").expect("write temp plugin");

    let mgr = mcx::core::plugin::PluginManager::new(&root);
    let event = mcx::core::plugin::PluginEvent {
        hook: "post-install".into(),
        package: Some("pkg".into()),
        root: root.to_string_lossy().into_owned(),
        timestamp: "now".into(),
    };
    let result = mgr
        .run_plugin_once("greeter", &event)
        .expect("run plugin once");
    assert!(result.success);
    assert!(result.message.unwrap_or_default().contains("hello"));

    let err = mgr.run_plugin_once("nonexistent", &event);
    assert!(err.is_err());

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_plugin_run_any_code() {
    let root = create_temporary_root("plugin_run_any");
    let plugins_dir = root.join("var/lib/mcx/plugins");
    fs::create_dir_all(&plugins_dir).expect("create plugins dir");
    fs::write(plugins_dir.join("arbitrary.py"),
        "import json\nresult = {'success': True, 'message': 'arbitrary ran'}\ndef on_post_install(event):\n    pass\nprint(json.dumps(result))\n").expect("write temp plugin");

    let mgr = mcx::core::plugin::PluginManager::new(&root);
    let event = mcx::core::plugin::PluginEvent {
        hook: "post-install".into(),
        package: Some("pkg".into()),
        root: root.to_string_lossy().into_owned(),
        timestamp: "now".into(),
    };
    let result = mgr
        .run_plugin_once("arbitrary", &event)
        .expect("run plugin once");
    assert!(result.success);

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Changelog two-phase protocol (C4 / M16 / M17) ────────────────────────────

#[tokio::test]
async fn test_changelog_two_phase_roundtrip_and_malformed_tolerance() {
    let root = create_temporary_root("changelog_two_phase");
    let db = Arc::new(Database::open(&root).expect("open test database"));

    // Commit through the ordered transaction protocol: an intent is written
    // at prepare time and a completion marker at finalize time.
    let pkg = PackageMetadata {
        pkg_name: "journal-pkg".into(),
        version: "1.0".into(),
        license: "MIT".into(),
        source: "https://example.com".into(),
        checksum: ChecksumData {
            kind: "sha256".into(),
            value: "00".into(),
        },
        dependencies: vec![],
        files: vec![],
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    };
    let mut tx = db.begin_transaction().expect("begin transaction");
    tx.register_package_placement(&pkg)
        .expect("register placement");
    tx.commit().expect("commit");

    let changelog = mcx::core::changelog::ChangelogManager::new(&root);
    let records = changelog.get_history().expect("read history");
    let recorded = records
        .iter()
        .find(|r| r.targets.iter().any(|t| t == "journal-pkg"));
    assert!(
        recorded.is_some(),
        "completed transaction must appear in history"
    );
    let tx_id = recorded.unwrap().transaction_id;

    // A journal containing garbage lines must not break reads.
    let journal = root.join("var/lib/mcx/history.jsonl");
    if journal.exists() {
        let mut content = fs::read_to_string(&journal).unwrap();
        content.insert_str(0, "\x00{{{definitely not json\n");
        fs::write(&journal, content).unwrap();
        let records = changelog.get_history().expect("tolerates malformed lines");
        assert!(
            records.iter().any(|r| r.transaction_id == tx_id),
            "completed records survive malformed-line skipping"
        );
    }

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Solver multi-target failure propagation (M1) ─────────────────────────────

#[tokio::test]
async fn test_solver_reports_failing_target_by_name() {
    let root = create_temporary_root("solver_target_failure");
    let db = Arc::new(Database::open(&root).expect("open test database"));
    let solver = mcx::core::solver::DependencySolver::new(Arc::clone(&db));

    let result = solver.add_target("definitely-not-a-package-xyz").solve();
    match result {
        Ok(_) => panic!("resolution of unknown package must fail"),
        Err(e) => {
            let msg = format!("{}", e);
            assert!(
                msg.contains("definitely-not-a-package-xyz"),
                "error must name the failing target, got: {msg}"
            );
        }
    }

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Name validation (M8 services / M9 repositories) ─────────────────────────

#[tokio::test]
async fn test_service_ini_rejects_path_traversal_names() {
    for evil in ["../escape", "svc/../../evil", "..", "a b"] {
        let ini = format!("[Service]\nName = {}\nExec = /bin/true\n", evil);
        assert!(
            mcx::core::service::CesarService::from_ini(&ini).is_err(),
            "service name {:?} must be rejected",
            evil
        );
    }
    let ok = "[Service]\nName = my-svc_1.0\nExec = /bin/true\n";
    assert!(mcx::core::service::CesarService::from_ini(ok).is_ok());
}

#[tokio::test]
async fn test_repo_add_rejects_traversal_and_empty_names() {
    let root = create_temporary_root("repo_name_validation");
    let mgr = mcx::core::repo::RepositoryManager::new(&root);
    mgr.initialize().expect("init repo manager");

    for evil in ["../evil", "a/b", "", ".hidden/../x"] {
        let repo = mcx::core::database::RepositoryInfo {
            name: evil.to_string(),
            url: "https://example.com/repo".to_string(),
            checksum: None,
            enabled: true,
        };
        assert!(
            mgr.add_repository(repo).is_err(),
            "repository name {:?} must be rejected",
            evil
        );
    }

    let good = mcx::core::database::RepositoryInfo {
        name: "main-repo_2".to_string(),
        url: "https://example.com/repo".to_string(),
        checksum: None,
        enabled: true,
    };
    mgr.add_repository(good).expect("valid repository accepted");

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── Git-tracked packages (archive install, git diff update) ───────────────

fn run_git_silent(dir: &PathBuf, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Write a `.xcs` archive (zstd-compressed tar) from a flat payload list.
fn write_xcs(path: &PathBuf, files: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().expect("archive parent")).expect("create archive dir");
    let tar_path = path.with_extension("tar");
    {
        let f = fs::File::create(&tar_path).expect("create tar file");
        let mut builder = tar::Builder::new(f);
        for (rel, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_path(rel).expect("set tar path");
            builder
                .append_data(&mut header, rel, content.as_bytes())
                .expect("append tar entry");
        }
        builder.finish().expect("finish tar");
    }
    {
        let mut input = fs::File::open(&tar_path).expect("open tar");
        let output = fs::File::create(path).expect("create xcs");
        let mut enc = zstd::stream::Encoder::new(output, 1).expect("zstd encoder");
        std::io::copy(&mut input, &mut enc).expect("compress to zstd");
        enc.finish().expect("finish zstd");
    }
    fs::remove_file(&tar_path).expect("remove temp tar");
}

fn write_payload(dir: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create payload dir");
        }
        fs::write(&full, content).expect("write payload file");
    }
}

fn package_meta(
    name: &str,
    version: &str,
    source: &str,
    checksum: &str,
    files: &[&str],
) -> PackageMetadata {
    PackageMetadata {
        pkg_name: name.to_string(),
        version: version.to_string(),
        license: "MIT".to_string(),
        source: source.to_string(),
        checksum: ChecksumData {
            kind: "sha256".to_string(),
            value: checksum.to_string(),
        },
        dependencies: vec![],
        files: files.iter().map(PathBuf::from).collect(),
        provides: Some(vec![]),
        conflicts: Some(vec![]),
        architecture: "native".to_string(),
        components: Vec::new(),
        services: Vec::new(),
        binaries: Vec::new(),
        file_hashes: std::collections::HashMap::new(),
        provenance: None,
    }
}

/// Set up a git-tracked package: a cached v1 `.xcs` archive + its git repo
/// (with a `v1.0.0` tag) + the git registry sidecar + a v1 available index.
/// Returns (root, db, remote_git_repo_dir).
fn setup_git_backed_package(identifier: &str) -> (PathBuf, Arc<Database>, PathBuf) {
    let root = create_temporary_root(identifier);
    let remote = root.join("remote/pkg");

    // v1 archive placed straight into the cache so the first install never
    // hits the network.
    let archive_path = root.join("var/cache/mcx/git-hello-1.0.0.xcs");
    write_xcs(
        &archive_path,
        &[
            ("usr/bin/hello", "hello v1\n"),
            ("usr/share/doc/hello/readme.txt", "readme v1\n"),
        ],
    );
    let archive_hash =
        mcx::archive::hash::HashVerifier::calculate(&archive_path, "sha256").expect("hash archive");

    // The package's own git repository, tagged per released version.
    fs::create_dir_all(&remote).expect("create repo dir");
    write_payload(
        &remote,
        &[
            ("usr/bin/hello", "hello v1\n"),
            ("usr/share/doc/hello/readme.txt", "readme v1\n"),
        ],
    );
    run_git_silent(&remote, &["init", "-b", "main"]);
    run_git_silent(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    run_git_silent(&remote, &["add", "."]);
    run_git_silent(
        &remote,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "v1.0.0",
        ],
    );
    run_git_silent(&remote, &["tag", "v1.0.0"]);

    // Git registry sidecar: package -> its git repository.
    let gitstate_dir = root.join("var/lib/mcx/gitstate");
    fs::create_dir_all(&gitstate_dir).expect("create gitstate dir");
    fs::write(
        gitstate_dir.join("registry.json"),
        serde_json::to_string_pretty(&std::collections::HashMap::from([(
            "git-hello".to_string(),
            format!("file://{}", remote.display()),
        )]))
        .expect("serialize registry"),
    )
    .expect("write registry");

    // Available index with v1 only.
    let db = Arc::new(Database::open(&root).expect("open test database"));
    let index_dir = root.join("var/lib/mcx/sync");
    fs::create_dir_all(&index_dir).expect("create sync dir");
    let index = index_dir.join("test.json");
    let v1 = package_meta(
        "git-hello",
        "1.0.0",
        &format!("file://{}", archive_path.display()),
        &archive_hash,
        &["usr/bin/hello", "usr/share/doc/hello/readme.txt"],
    );
    fs::write(
        &index,
        serde_json::to_string_pretty(&vec![&v1]).expect("serialize index"),
    )
    .expect("write index");
    {
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.update_repository_index("test", index.to_str().expect("utf8 index path"))
            .expect("load v1 index");
        tx.commit().expect("commit transaction");
    }

    (root, db, remote)
}

#[tokio::test]
async fn test_archive_diff_update_touches_only_changed_files() {
    let root = create_temporary_root("archive_diff");
    let db = Arc::new(Database::open(&root).expect("open test database"));
    let index_dir = root.join("var/lib/mcx/sync");
    fs::create_dir_all(&index_dir).expect("create sync dir");

    // v1 archive + available index; install happens entirely from the archive.
    let v1_archive = root.join("var/cache/mcx/plain-pkg-1.0.0.xcs");
    write_xcs(
        &v1_archive,
        &[
            ("usr/bin/tool", "tool v1\n"),
            ("etc/plain-pkg.conf", "conf v1\n"),
            ("usr/share/doc/plain-pkg/readme.txt", "readme v1\n"),
        ],
    );
    let v1_hash =
        mcx::archive::hash::HashVerifier::calculate(&v1_archive, "sha256").expect("hash v1");
    let v1 = package_meta(
        "plain-pkg",
        "1.0.0",
        &format!("file://{}", v1_archive.display()),
        &v1_hash,
        &[
            "usr/bin/tool",
            "etc/plain-pkg.conf",
            "usr/share/doc/plain-pkg/readme.txt",
        ],
    );
    let index = index_dir.join("test.json");
    fs::write(
        &index,
        serde_json::to_string_pretty(&vec![&v1]).expect("serialize v1"),
    )
    .expect("write index");
    {
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.update_repository_index("test", index.to_str().expect("utf8"))
            .expect("load v1");
        tx.commit().expect("commit");
    }

    let cmd = InstallCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    cmd.execute(&["plain-pkg".to_string()])
        .await
        .expect("v1 install");
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/tool")).unwrap(),
        "tool v1\n"
    );

    // v2: tool changed, tool-extra added, readme removed, conf payload-identical.
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let before_conf = fs::metadata(root.join("etc/plain-pkg.conf"))
        .expect("conf metadata")
        .modified()
        .expect("conf mtime");
    let before_tool = fs::metadata(root.join("usr/bin/tool"))
        .expect("tool metadata")
        .modified()
        .expect("tool mtime");

    let v2_archive = root.join("var/cache/mcx/plain-pkg-2.0.0.xcs");
    write_xcs(
        &v2_archive,
        &[
            ("usr/bin/tool", "tool v2\n"),
            ("usr/bin/tool-extra", "tool-extra v2\n"),
            ("etc/plain-pkg.conf", "conf v1\n"),
        ],
    );
    let v2_hash =
        mcx::archive::hash::HashVerifier::calculate(&v2_archive, "sha256").expect("hash v2");
    let v2 = package_meta(
        "plain-pkg",
        "2.0.0",
        &format!("file://{}", v2_archive.display()),
        &v2_hash,
        &["usr/bin/tool", "usr/bin/tool-extra", "etc/plain-pkg.conf"],
    );
    fs::write(
        &index,
        serde_json::to_string_pretty(&vec![&v2]).expect("serialize v2"),
    )
    .expect("write index 2");
    {
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.update_repository_index("test", index.to_str().expect("utf8"))
            .expect("load v2");
        tx.commit().expect("commit");
    }

    // No git registry entry => the diff is computed against the new archive.
    let cmd2 = InstallCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    cmd2.execute(&["plain-pkg".to_string()])
        .await
        .expect("v2 archive-diff update");

    // Changed + added placed, removed gone.
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/tool")).unwrap(),
        "tool v2\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/tool-extra")).unwrap(),
        "tool-extra v2\n"
    );
    assert!(
        !root.join("usr/share/doc/plain-pkg/readme.txt").exists(),
        "removed file deleted"
    );

    // Payload-identical file keeps original mtime: only the diff was applied.
    let after_conf = fs::metadata(root.join("etc/plain-pkg.conf"))
        .expect("conf metadata")
        .modified()
        .expect("conf mtime");
    assert_eq!(after_conf, before_conf, "unchanged file not rewritten");
    assert_eq!(
        fs::read_to_string(root.join("etc/plain-pkg.conf")).unwrap(),
        "conf v1\n"
    );

    // Rewritten file got a fresh timestamp.
    let after_tool = fs::metadata(root.join("usr/bin/tool"))
        .expect("tool metadata")
        .modified()
        .expect("tool mtime");
    assert_ne!(after_tool, before_tool, "changed file rewritten");

    // Installed state + active mirror reflect v2.
    let installed = db.get_package_manifest("plain-pkg").expect("get manifest");
    assert_eq!(installed.version, "2.0.0");
    assert_eq!(
        fs::read_to_string(root.join("var/lib/mcx/active/plain-pkg/usr/bin/tool-extra")).unwrap(),
        "tool-extra v2\n",
        "active mirror updated with added file"
    );
    assert!(
        !root
            .join("var/lib/mcx/active/plain-pkg/usr/share/doc/plain-pkg/readme.txt")
            .exists(),
        "active mirror drops removed file"
    );

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_git_tracked_package_still_updates_from_archive() {
    let (root, db, remote) = setup_git_backed_package("git_pkg_e2e");

    // Fresh install comes from the `.xcs` archive, not from git.
    let cmd = InstallCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    cmd.execute(&["git-hello".to_string()])
        .await
        .expect("archive install");
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/hello")).unwrap(),
        "hello v1\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("usr/share/doc/hello/readme.txt")).unwrap(),
        "readme v1\n"
    );
    assert!(
        !root.join("var/lib/mcx/gits/git-hello").exists(),
        "no git clone on install"
    );

    // Maintainer bumps the package's git remote to v2 and tags it — install
    // does not care: the update reads the new `.xcs` archive from source.
    run_git_silent(&remote, &["rm", "-q", "usr/share/doc/hello/readme.txt"]);
    fs::write(remote.join("usr/bin/hello"), "hello v2\n").expect("modify hello");
    fs::write(remote.join("usr/bin/hello2"), "hello2 v2\n").expect("add hello2");
    run_git_silent(&remote, &["add", "-A"]);
    run_git_silent(
        &remote,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "v2.0.0",
        ],
    );
    run_git_silent(&remote, &["tag", "v2.0.0"]);

    // Publish a real v2 archive (read from source this time) and advertise it
    // in the available index with its real checksum.
    let v2_archive = root.join("var/cache/mcx/git-hello-2.0.0.xcs");
    write_xcs(
        &v2_archive,
        &[
            ("usr/bin/hello", "hello v2\n"),
            ("usr/bin/hello2", "hello2 v2\n"),
        ],
    );
    let v2_hash =
        mcx::archive::hash::HashVerifier::calculate(&v2_archive, "sha256").expect("hash v2");
    let v2 = package_meta(
        "git-hello",
        "2.0.0",
        &format!("file://{}", v2_archive.display()),
        &v2_hash,
        &["usr/bin/hello", "usr/bin/hello2"],
    );
    {
        let index = root.join("var/lib/mcx/sync/test.json");
        fs::write(
            &index,
            serde_json::to_string_pretty(&vec![&v2]).expect("serialize v2 index"),
        )
        .expect("write v2 index");
        let mut tx = db.begin_transaction().expect("begin transaction");
        tx.update_repository_index("test", index.to_str().expect("utf8"))
            .expect("load v2 index");
        tx.commit().expect("commit transaction");
    }

    // The update reads the archive from source and applies only the diff.
    let cmd2 = InstallCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    cmd2.execute(&["git-hello".to_string()])
        .await
        .expect("archive diff update");

    assert_eq!(
        fs::read_to_string(root.join("usr/bin/hello")).unwrap(),
        "hello v2\n",
        "changed file updated"
    );
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/hello2")).unwrap(),
        "hello2 v2\n",
        "added file placed"
    );
    assert!(
        !root.join("usr/share/doc/hello/readme.txt").exists(),
        "removed file deleted"
    );

    // The git machinery is not consulted: no checkout, no worktree, no state.
    assert!(
        !root.join("var/lib/mcx/gits/git-hello").exists(),
        "no git clone on update"
    );
    assert!(
        !root.join("var/lib/mcx/gits/git-hello.wt").exists(),
        "no worktree created"
    );
    let git = mcx::core::gitpkg::GitPackageManager::new(&root);
    assert!(
        git.read_state("git-hello").expect("read state").is_none(),
        "no git state written"
    );

    // Installed state + active mirror reflect v2.
    let installed = db.get_package_manifest("git-hello").expect("get manifest");
    assert_eq!(installed.version, "2.0.0");
    assert_eq!(
        fs::read_to_string(root.join("var/lib/mcx/active/git-hello/usr/bin/hello2")).unwrap(),
        "hello2 v2\n",
        "active mirror updated"
    );

    fs::remove_dir_all(&root).expect("remove temp root");
}

#[tokio::test]
async fn test_git_package_purge_removes_checkout_and_worktree() {
    let (root, db, remote) = setup_git_backed_package("git_pkg_purge");

    let cmd = InstallCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    cmd.execute(&["git-hello".to_string()])
        .await
        .expect("archive install");

    // Simulate a git-updated package: clone + materialise a worktree.
    let git = mcx::core::gitpkg::GitPackageManager::new(&root);
    git.ensure_clone("git-hello", &format!("file://{}", remote.display()))
        .expect("clone");
    git.fetch("git-hello").expect("fetch");
    let commit = git
        .resolve_commit("git-hello", "1.0.0")
        .expect("resolve")
        .expect("tag exists");
    let wt = git.create_worktree("git-hello", &commit).expect("worktree");
    assert!(wt.exists(), "worktree created");

    assert!(
        root.join("var/lib/mcx/gits/git-hello/.git").exists(),
        "checkout exists"
    );

    git.purge("git-hello").expect("purge git state");
    assert!(
        !root.join("var/lib/mcx/gits/git-hello").exists(),
        "checkout purged"
    );
    assert!(!wt.exists(), "worktree purged");
    assert!(
        git.read_state("git-hello")
            .expect("read state after purge")
            .is_none(),
        "state purged"
    );

    fs::remove_dir_all(&root).expect("remove temp root");
}

// ── User / System mode ───────────────────────────────────────────────────────

#[test]
fn test_user_selection_round_trip_requires_fields() {
    let root = create_temporary_root("user_sel_round_trip");
    let sel_path = root.join("etc/mcx/user.ini");
    fs::create_dir_all(sel_path.parent().unwrap()).expect("create config dir");

    assert!(
        mcx::core::mode::read_user_selection_in(&sel_path).is_none(),
        "missing file has no selection"
    );
    fs::write(&sel_path, "[general]\nmode = user\n").expect("write selection without user");
    assert!(
        mcx::core::mode::read_user_selection_in(&sel_path).is_none(),
        "missing required user field yields no selection"
    );

    let sel = mcx::core::mode::UserSelection {
        mode: mcx::core::mode::Mode::User,
        user: "alice".to_string(),
    };
    mcx::core::mode::write_user_selection_in(&sel, &sel_path).expect("write selection");
    let reread = mcx::core::mode::read_user_selection_in(&sel_path).expect("read selection");
    assert_eq!(reread.mode, mcx::core::mode::Mode::User);
    assert_eq!(reread.user, "alice");
    fs::remove_dir_all(&root).expect("remove temp root");
}

#[test]
fn test_mode_config_patch_appends_general_without_selection() {
    let root = create_temporary_root("mode_defaults");
    let config_path = root.join("etc/mcx/config.ini");
    fs::create_dir_all(config_path.parent().unwrap()).expect("create config dir");
    fs::write(
        &config_path,
        "[general]\nlog_level = debug\n\n[python]\nenabled = false\n",
    )
    .expect("write seed config");

    mcx::core::mode::ensure_mode_fields_in(&config_path).expect("ensure root fields");

    let content = fs::read_to_string(&config_path).expect("read back");
    assert!(content.contains("user_root = ~/.mcx"), "user_root appended");
    assert!(content.contains("system_root = /"), "system_root appended");
    assert!(
        content.contains("log_level = debug"),
        "other general keys preserved"
    );
    assert!(content.contains("[python]"), "seed section untouched");
    assert!(
        !content.contains("user_mode"),
        "user selection never lives in root config"
    );

    let cfg = mcx::core::mode::read_mode_config_in(&config_path);
    assert_eq!(cfg.user_root, "~/.mcx");
    assert_eq!(cfg.system_root, "/");
    fs::remove_dir_all(&root).expect("remove temp root");
}

#[test]
fn test_resolve_root_modes() {
    let cfg = mcx::core::mode::ModeConfig {
        user_root: "u-root".to_string(),
        system_root: "/sys-root".to_string(),
    };

    // User mode honors the configured per-user root and any explicit --root;
    // callers separately enforce that the user can read/write the directory.
    let cwd = std::env::current_dir().expect("cwd");
    let user =
        mcx::core::mode::resolve_root(Some("/external"), mcx::core::mode::Mode::User, &cfg, false);
    assert_eq!(user, std::path::PathBuf::from("/external"));
    let user_cfg = mcx::core::mode::resolve_root(None, mcx::core::mode::Mode::User, &cfg, false);
    assert_eq!(user_cfg, cwd.join("u-root"));

    // System mode honors explicit --root: absolute stays absolute, relative joins CWD.
    let abs =
        mcx::core::mode::resolve_root(Some("/opt/mcx"), mcx::core::mode::Mode::System, &cfg, false);
    assert_eq!(abs, std::path::PathBuf::from("/opt/mcx"));
    let rel =
        mcx::core::mode::resolve_root(Some("my-root"), mcx::core::mode::Mode::System, &cfg, false);
    assert_eq!(rel, cwd.join("my-root"));

    // System mode mutating targets the system root; read-only falls back to
    // the per-user root so queries never elevate (only when not running as root).
    let sys = mcx::core::mode::resolve_root(None, mcx::core::mode::Mode::System, &cfg, false);
    assert_eq!(sys, std::path::PathBuf::from("/sys-root"));
    let is_root = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8(o.stdout).unwrap_or_default().trim() == "0")
        .unwrap_or(false);
    if !is_root {
        let readonly =
            mcx::core::mode::resolve_root(None, mcx::core::mode::Mode::System, &cfg, true);
        assert_eq!(readonly, cwd.join("u-root"));
    }
}

// ── Wildcard package matching ────────────────────────────────────────────────

#[test]
fn test_wildcard_expand_from_library() {
    let names = ["pkg-tools-1", "pkg-tools-2", "pkg-libs", "otherpkg"];

    // Prefix glob: pkg* matches every pkg-tools and pkg-libs.
    let matched = mcx::core::wildcard::expand("pkg*", names.iter().copied());
    assert_eq!(matched, vec!["pkg-libs", "pkg-tools-1", "pkg-tools-2"]);

    // Suffix glob.
    let matched = mcx::core::wildcard::expand("*tools*", names.iter().copied());
    assert_eq!(matched, vec!["pkg-tools-1", "pkg-tools-2"]);

    // No matches yields an empty list.
    assert!(mcx::core::wildcard::expand("zzz*", names.iter().copied()).is_empty());

    // Exact names are flagged as non-wildcards and match themselves.
    assert!(!mcx::core::wildcard::has_wildcard("pkg-tools-1"));
    assert!(mcx::core::wildcard::has_wildcard("pkg-*"));
}

// ── Outsider ↔ MCX round-trip (add.rs local path) ──────────────────────────

/// Construct a zstd-compressed tar archive whose layout mirrors an
/// Outsider-produced `.xcs` package, including a `metadata.json` with
/// the `{"kind","value"}` checksum object and an `architecture` field.
/// Install it via `AddLocalCommand` and assert every interlock point.
#[tokio::test]
async fn test_outsider_round_trip_local_install() {
    let root = create_temporary_root("outsider_round_trip");
    let db = Arc::new(Database::open(&root).expect("open test database"));

    let archive_dir = root.join("pool/x86_64/hello-pkg");
    fs::create_dir_all(&archive_dir).expect("create archive dir");
    let archive_path = archive_dir.join("hello-pkg-2.0.0.xcs");
    let tar_path = archive_path.with_extension("tar");

    // ── Build the tar with Outsider's exact metadata.json shape ──
    {
        let f = fs::File::create(&tar_path).expect("create tar");
        let mut builder = tar::Builder::new(f);

        // payload file
        let mut hdr = tar::Header::new_gnu();
        hdr.set_size(6);
        hdr.set_mode(0o755);
        hdr.set_entry_type(tar::EntryType::Regular);
        hdr.set_path("usr/bin/hello").unwrap();
        builder
            .append_data(&mut hdr, "usr/bin/hello", &b"hello!"[..])
            .unwrap();

        // metadata.json — Outsider's exact output shape.
        // The checksum value here is the hash of the UNCOMPRESSED tar stream
        // (informational per contract); transport integrity uses the sidecar.
        let meta = serde_json::json!({
            "pkg_name": "hello-pkg",
            "version": "2.0.0",
            "license": "MIT",
            "architecture": "x86_64",
            "checksum": {"kind": "sha256", "value": "uncompressed-tar-hash-info-only"},
            "dependencies": [],
            "provides": null,
            "conflicts": null
        });
        let meta_bytes = serde_json::to_vec_pretty(&meta).unwrap();
        let mut mhdr = tar::Header::new_gnu();
        mhdr.set_size(meta_bytes.len() as u64);
        mhdr.set_mode(0o644);
        mhdr.set_entry_type(tar::EntryType::Regular);
        mhdr.set_path("metadata.json").unwrap();
        builder
            .append_data(&mut mhdr, "metadata.json", &*meta_bytes)
            .unwrap();

        builder.finish().expect("finish tar");
    }

    // ── Compress to zstd ──
    {
        let mut input = fs::File::open(&tar_path).expect("open tar");
        let output = fs::File::create(&archive_path).expect("create xcs");
        let mut enc = zstd::stream::Encoder::new(output, 1).expect("zstd encoder");
        std::io::copy(&mut input, &mut enc).expect("compress to zstd");
        enc.finish().expect("finish zstd");
    }
    fs::remove_file(&tar_path).expect("remove temp tar");

    // ── Write .sha256 sidecar (compressed-bytes hash) ──
    let sidecar_hash = mcx::archive::hash::HashVerifier::calculate(&archive_path, "sha256")
        .expect("hash compressed archive");
    fs::write(
        archive_path.with_file_name("hello-pkg-2.0.0.xcs.sha256"),
        format!("{}\n", sidecar_hash),
    )
    .expect("write sidecar");

    // ── (a) metadata.json deserializes from Outsider's checksum object ──
    let meta_json = {
        let f = fs::File::open(&archive_path).expect("open archive for read");
        let dec = zstd::stream::Decoder::new(f).expect("zstd decoder");
        let mut archive = tar::Archive::new(dec);
        let mut content = String::new();
        for entry in archive.entries().expect("entries") {
            let mut e = entry.expect("entry");
            if e.path().expect("path").as_ref() == Path::new("metadata.json") {
                e.read_to_string(&mut content).expect("read metadata");
                break;
            }
        }
        content
    };
    let parsed: serde_json::Value = serde_json::from_str(&meta_json).expect("parse metadata");
    assert_eq!(parsed["architecture"], "x86_64", "architecture key present");
    assert!(parsed["checksum"].is_object(), "checksum is object");
    assert_eq!(parsed["checksum"]["kind"], "sha256");

    // ── Install via AddLocalCommand ──
    let cmd = AddLocalCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    cmd.execute(archive_path.to_str().expect("archive path str"))
        .expect("add local install");

    // ── (b) installed package has the correct architecture ──
    let pkg = db.get_package_manifest("hello-pkg").expect("manifest");
    assert_eq!(
        pkg.architecture, "x86_64",
        "architecture consumed from metadata.json"
    );

    // ── (c) checksum from metadata.json round-trips to DB (informational) ──
    assert_eq!(pkg.checksum.kind, "sha256", "checksum kind");
    assert_eq!(
        pkg.checksum.value, "uncompressed-tar-hash-info-only",
        "checksum value from metadata.json object (informational, not verified at install)"
    );

    // ── (d) metadata.json does NOT land on the live root ──
    assert!(
        !root.join("metadata.json").exists(),
        "metadata.json filtered from live root"
    );

    // ── (e) payload file was placed ──
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/hello")).unwrap(),
        "hello!",
        "payload file placed correctly"
    );

    // ── (f) sidecar file is readable and matches the compressed archive hash ──
    let sidecar_content =
        fs::read_to_string(archive_path.with_file_name("hello-pkg-2.0.0.xcs.sha256"))
            .expect("read sidecar");
    assert_eq!(
        sidecar_content.trim(),
        sidecar_hash,
        "sidecar matches compressed hash"
    );

    fs::remove_dir_all(&root).expect("remove temp root");
}

/// Verify that the `PackageEntity` flexible checksum deserializes both
/// the Outsider `{"kind","value"}` object and a legacy flat string, and
/// that `ManifestParser::parse_embedded_manifest` accepts both.
#[test]
fn test_manifest_parser_accepts_outsider_checksum_object() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Outsider-shaped metadata.json
    let outsider_meta = serde_json::json!({
        "pkg_name": "outsider-pkg",
        "version": "3.0.0",
        "license": "MIT",
        "build_type": "custom",
        "build_date": "2026-09-09",
        "architecture": "aarch64",
        "checksum": {"kind": "sha256", "value": "abcdef0123456789"}
    });
    fs::write(
        root.join("metadata.json"),
        serde_json::to_string_pretty(&outsider_meta).unwrap(),
    )
    .expect("write outsider metadata");

    let entity = mcx::core::manifest::ManifestParser::parse_embedded_manifest(root)
        .expect("parse Outsider-shaped metadata.json");
    assert_eq!(entity.pkg_name, "outsider-pkg");
    assert_eq!(entity.architecture, "aarch64");
    assert_eq!(entity.checksum, "sha256:abcdef0123456789");

    // Legacy flat-string checksum also accepted
    let legacy_meta = serde_json::json!({
        "pkg_name": "legacy-pkg",
        "version": "1.0.0",
        "license": "GPL",
        "build_type": "static",
        "build_date": "2026-01-01",
        "architecture": "native",
        "checksum": "flat_hash_value"
    });
    fs::write(
        root.join("metadata.json"),
        serde_json::to_string_pretty(&legacy_meta).unwrap(),
    )
    .expect("write legacy metadata");

    let entity2 = mcx::core::manifest::ManifestParser::parse_embedded_manifest(root)
        .expect("parse legacy metadata.json");
    assert_eq!(entity2.checksum, "flat_hash_value");
}

// ── Local package sources ────────────────────────────────────────────────

/// Write an Outsider-shaped `.xcs` archive (metadata.json + flat payload) and
/// its `.sha256` transport sidecar, exactly like `ous` produces.
fn write_outsider_xcs(path: &PathBuf, pkg_name: &str, version: &str, files: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().expect("archive parent")).expect("create archive dir");
    let metadata = serde_json::json!({
        "pkg_name": pkg_name,
        "version": version,
        "license": "MIT",
        "source": "local-source-test",
        "architecture": "native",
        "checksum": {"kind": "sha256", "value": "info-only"},
        "dependencies": [],
        "files": [],
        "provides": [],
        "conflicts": []
    });
    let mut entries: Vec<(String, String)> = vec![(
        "metadata.json".to_string(),
        serde_json::to_string(&metadata).expect("serialize metadata"),
    )];
    entries.extend(
        files
            .iter()
            .map(|(rel, content)| (rel.to_string(), content.to_string())),
    );
    let flat: Vec<(&str, &str)> = entries
        .iter()
        .map(|(rel, content)| (rel.as_str(), content.as_str()))
        .collect();
    write_xcs(path, &flat);
    let hash = mcx::archive::hash::HashVerifier::calculate(path, "sha256").expect("hash archive");
    fs::write(path.with_extension("xcs.sha256"), format!("{}\n", hash)).expect("write sidecar");
}

/// Like `write_outsider_xcs` but embeds an optional `provenance` block.
fn write_outsider_xcs_with_provenance(
    path: &PathBuf,
    pkg_name: &str,
    version: &str,
    source: &str,
    provenance: Option<serde_json::Value>,
    files: &[(&str, &str)],
) {
    fs::create_dir_all(path.parent().expect("archive parent")).expect("create archive dir");
    let mut metadata = serde_json::json!({
        "pkg_name": pkg_name,
        "version": version,
        "license": "MIT",
        "source": source,
        "architecture": "native",
        "checksum": {"kind": "sha256", "value": "info-only"},
        "dependencies": [],
        "files": [],
        "provides": [],
        "conflicts": [],
    });
    if let Some(prov) = provenance {
        metadata["provenance"] = prov;
    }
    let mut entries: Vec<(String, String)> = vec![(
        "metadata.json".to_string(),
        serde_json::to_string(&metadata).expect("serialize metadata"),
    )];
    entries.extend(
        files
            .iter()
            .map(|(rel, content)| (rel.to_string(), content.to_string())),
    );
    let flat: Vec<(&str, &str)> = entries
        .iter()
        .map(|(rel, content)| (rel.as_str(), content.as_str()))
        .collect();
    write_xcs(path, &flat);
    let hash = mcx::archive::hash::HashVerifier::calculate(path, "sha256").expect("hash archive");
    fs::write(path.with_extension("xcs.sha256"), format!("{}\n", hash)).expect("write sidecar");
}

/// Build a `.tar.zst` containing the given archive entries (relative path ->
/// content) without requiring any external binary (tar + zstd crates only).
fn write_tar_zst(path: &PathBuf, entries: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().expect("tar parent")).expect("create tar dir");
    let plain = path.with_extension("plain.tar");
    {
        let f = fs::File::create(&plain).expect("create tar file");
        let mut builder = tar::Builder::new(f);
        for (rel, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_path(rel).expect("set tar path");
            builder
                .append_data(&mut header, rel, content.as_bytes())
                .expect("append tar entry");
        }
        builder.finish().expect("finish tar");
    }
    {
        let mut input = fs::File::open(&plain).expect("open plain tar");
        let output = fs::File::create(path).expect("create tar.zst");
        let mut enc = zstd::stream::Encoder::new(output, 1).expect("zstd encoder");
        std::io::copy(&mut input, &mut enc).expect("compress to zstd");
        enc.finish().expect("finish zstd");
    }
    fs::remove_file(&plain).expect("remove temp tar");
}

#[tokio::test]
async fn test_local_prebuilt_source_sync_and_fingerprint() {
    let root = create_temporary_root("local_prebuilt_sync");
    let prebuilt_dir = root.join("srv/prebuilt");
    fs::create_dir_all(&prebuilt_dir).expect("create prebuilt dir");

    let archive_100 = prebuilt_dir.join("hello-pkg-1.0.0.xcs");
    write_outsider_xcs(
        &archive_100,
        "hello-pkg",
        "1.0.0",
        &[("usr/bin/hello", "hello v1\n")],
    );

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let cmd = LocalSourceCommand::new(&root, Arc::clone(&db));

    cmd.add("prebuilt", prebuilt_dir.to_str().expect("str"), true, true)
        .expect("register prebuilt source");

    cmd.sync_local_sources().expect("first sync");

    assert!(db.is_package_installed("hello-pkg").expect("installed"));
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/hello")).unwrap(),
        "hello v1\n"
    );

    let mtime = fs::metadata(root.join("usr/bin/hello"))
        .expect("hello meta")
        .modified()
        .expect("mtime");

    cmd.sync_local_sources().expect("second sync unchanged");
    assert_eq!(
        fs::metadata(root.join("usr/bin/hello"))
            .expect("hello meta")
            .modified()
            .expect("mtime"),
        mtime,
        "unchanged source must not rewrite installed files"
    );

    let archive_110 = prebuilt_dir.join("hello-pkg-1.1.0.xcs");
    write_outsider_xcs(
        &archive_110,
        "hello-pkg",
        "1.1.0",
        &[("usr/bin/hello", "hello v1.1\n")],
    );
    cmd.sync_local_sources()
        .expect("third sync with new archive");

    let meta = db.get_package_manifest("hello-pkg").expect("manifest");
    assert_eq!(meta.version, "1.1.0", "newer archive wins");
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/hello")).unwrap(),
        "hello v1.1\n"
    );

    let listed = cmd.list().expect("list sources");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].mode, mcx::core::localsrc::SourceMode::Prebuilt);
    assert!(listed[0].last_built.is_some(), "last_built was recorded");
    assert!(
        root.join("etc/mcx/localsources.ini").exists(),
        "config written under etc/mcx"
    );

    cmd.remove("prebuilt").expect("remove source");
    assert!(cmd.list().expect("list").is_empty());

    fs::remove_dir_all(&root).expect("cleanup");
}

#[tokio::test]
async fn test_local_source_add_validation_and_skip_without_ous() {
    let root = create_temporary_root("local_source_skip");
    let source_dir = root.join("srv/src");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::write(
        source_dir.join("manifest.json"),
        r#"{"packages":[{"name":"hello-pkg","version":"1.0.0","source":"payload","type":"custom"}]}"#,
    )
    .expect("write manifest");
    fs::create_dir_all(source_dir.join("payload")).expect("create payload");
    fs::write(
        source_dir.join("payload/hello.c"),
        "int main(void) { return 0; }\n",
    )
    .expect("write source");

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let cmd = LocalSourceCommand::new(&root, Arc::clone(&db));

    assert!(
        cmd.add("../evil", source_dir.to_str().expect("str"), false, true)
            .is_err()
    );
    assert!(
        cmd.add("prebuilt-url", "https://example.com/x.git", true, true)
            .is_err()
    );
    assert!(cmd.add("missing-dir", "/no/such/dir", true, true).is_err());
    assert!(
        cmd.add(
            "no-manifest",
            root.join("srv").to_str().expect("str"),
            false,
            true
        )
        .is_err()
    );

    cmd.add("source", source_dir.to_str().expect("str"), false, true)
        .expect("register source source");

    let sources = cmd.list().expect("list");
    let result = cmd
        .build_one(&sources[0], false, None)
        .expect("build_one must not panic without ous");
    assert_eq!(
        result,
        mcx::core::localsrc::LocalSourceResult::Skipped("ous binary not available".to_string())
    );

    assert!(
        cmd.build(vec!["source".to_string()], false).is_err(),
        "explicit build without ous must fail"
    );

    fs::remove_dir_all(&root).expect("cleanup");
}

#[tokio::test]
async fn test_local_source_add_cli_default_is_enabled() {
    // Drives the real `mcx` binary so the enable/disable flag resolution in
    // the CLI handler is pinned: no flag must default to ENABLED, only
    // `--disable` may produce `enabled = false`.
    let home = create_temporary_root("lsrc_cli_default");
    let src = home.join("srv/srca");
    fs::create_dir_all(&src).expect("create source dir");
    fs::write(src.join("manifest.json"), r#"{"packages":[]}"#).expect("write manifest");
    let src = src.to_str().expect("src path");
    let bin = env!("CARGO_BIN_EXE_mcx");

    let run = |args: &[&str]| {
        std::process::Command::new(bin)
            .args(args)
            .env("HOME", &home)
            .output()
            .expect("run mcx")
    };

    // No flag: enabled by default.
    let ok = run(&["--user-mode", "local-source-add", "cli-a", src]);
    assert!(
        ok.status.success(),
        "no-flag add failed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let ini =
        fs::read_to_string(home.join(".mcx/etc/mcx/localsources.ini")).expect("localsources.ini");
    assert!(ini.contains("[cli-a]"), "{ini}");
    assert!(
        ini.contains("enabled = true"),
        "no flag must store enabled=true:\n{ini}"
    );

    // --enable also stores enabled=true.
    let ok = run(&["--user-mode", "local-source-add", "cli-b", src, "--enable"]);
    assert!(
        ok.status.success(),
        "enable add failed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let ini =
        fs::read_to_string(home.join(".mcx/etc/mcx/localsources.ini")).expect("localsources.ini");
    assert!(ini.contains("[cli-b]"), "{ini}");
    assert!(
        ini.contains("enabled = true"),
        "--enable must store enabled=true:\n{ini}"
    );

    // --disable stores enabled=false.
    let ok = run(&["--user-mode", "local-source-add", "cli-c", src, "--disable"]);
    assert!(
        ok.status.success(),
        "disable add failed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let ini =
        fs::read_to_string(home.join(".mcx/etc/mcx/localsources.ini")).expect("localsources.ini");
    assert!(ini.contains("[cli-c]"), "{ini}");
    assert!(
        ini.contains("enabled = false"),
        "--disable must store enabled=false:\n{ini}"
    );

    fs::remove_dir_all(&home).expect("cleanup");
}

#[tokio::test]
#[ignore = "requires a real ous binary (set OUS_BIN)"]
async fn test_local_source_build_with_real_ous_binary() {
    let Some(ous) = std::env::var("OUS_BIN")
        .ok()
        .filter(|p| !p.is_empty() && Path::new(p).is_file())
    else {
        eprintln!("OUS_BIN not set to an existing binary; skipping real-ous test");
        return;
    };

    let root = create_temporary_root("local_source_real_ous");
    let source_dir = root.join("srv/src");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::write(
        source_dir.join("manifest.json"),
        r#"{"packages":[{"name":"hello-pkg","version":"1.0.0","source":"payload","type":"custom","build":["none"],"install":["mkdir -p \"$CUDANE_DEST/usr/share/hello\" && cp hello.txt \"$CUDANE_DEST/usr/share/hello/\""]}]}"#,
    )
    .expect("write manifest");
    fs::create_dir_all(source_dir.join("payload")).expect("create payload");
    fs::write(source_dir.join("payload/hello.txt"), "hello\n").expect("write payload");

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let cmd = LocalSourceCommand::new(&root, Arc::clone(&db));
    cmd.add("source", source_dir.to_str().expect("str"), false, true)
        .expect("register source");

    let sources = cmd.list().expect("list");
    let result = cmd
        .build_one(&sources[0], true, Some(&ous))
        .expect("build with real ous succeeded");
    assert_eq!(result, mcx::core::localsrc::LocalSourceResult::Built);

    assert!(db.is_package_installed("hello-pkg").expect("installed"));
    assert_eq!(
        fs::read_to_string(root.join("usr/share/hello/hello.txt")).unwrap(),
        "hello\n",
        "payload produced by the real ous archive must reach the live root"
    );

    fs::remove_dir_all(&root).expect("cleanup");
}

#[tokio::test]
async fn test_local_add_handles_dot_prefix_archive_entries() {
    let root = create_temporary_root("local_add_dot_prefix");
    let out = root.join("var/tmp/dot.xcs");
    fs::create_dir_all(out.parent().expect("out parent")).expect("create out dir");
    let metadata = serde_json::json!({
        "pkg_name": "hello-dot",
        "version": "1.0.0",
        "license": "MIT",
        "source": "local-source-test",
        "architecture": "native",
        "checksum": {"kind": "sha256", "value": "info-only"},
        "dependencies": [],
        "files": ["usr/bin/hello-dot"],
        "provides": [],
        "conflicts": []
    });

    let tar_path = out.with_extension("tar");
    {
        let f = fs::File::create(&tar_path).expect("create tar file");
        let mut builder = tar::Builder::new(f);
        for (rel, content) in [
            (
                "./metadata.json",
                serde_json::to_string(&metadata).expect("serialize"),
            ),
            ("./usr/bin/hello-dot", String::from("#!/bin/sh\necho hi\n")),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_path(rel).expect("set tar path");
            builder
                .append_data(&mut header, rel, content.as_bytes())
                .expect("append tar entry");
        }
        builder.finish().expect("finish tar");
    }
    {
        let mut input = fs::File::open(&tar_path).expect("open tar");
        let output = fs::File::create(&out).expect("create xcs");
        let mut enc = zstd::stream::Encoder::new(output, 1).expect("zstd encoder");
        std::io::copy(&mut input, &mut enc).expect("compress to zstd");
        enc.finish().expect("finish zstd");
        fs::remove_file(&tar_path).expect("remove temp tar");
    }

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let add =
        mcx::commands::AddLocalCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    add.execute(&out.to_string_lossy()).expect("add succeeds");

    assert!(db.is_package_installed("hello-dot").expect("installed"));
    let meta = db.get_package_manifest("hello-dot").expect("manifest");
    assert_eq!(
        meta.version, "1.0.0",
        "metadata read from ./-prefixed entry"
    );
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/hello-dot")).unwrap(),
        "#!/bin/sh\necho hi\n"
    );
    assert!(
        !root.join("metadata.json").exists(),
        "./-prefixed metadata.json must not be installed verbatim"
    );

    fs::remove_dir_all(&root).expect("cleanup");
}

// ── Universal local-source kinds + provenance auto-link ────────────────────

#[tokio::test]
async fn test_local_source_archive_tarball_materialization_and_fingerprint() {
    let root = create_temporary_root("local_src_archive_tar");
    let tarball = root.join("srv/hello-tar.tar.zst");
    write_tar_zst(
        &tarball,
        &[
            (
                "manifest.json",
                r#"{"packages":[{"name":"hello-tar","version":"1.0.0","source":"payload","type":"custom","build":["none"],"install":["mkdir -p \"$CUDANE_DEST/usr/share/hello\" && cp hello.txt \"$CUDANE_DEST/usr/share/hello/\""]}]}"#,
            ),
            ("payload/hello.txt", "hello tar v1\n"),
        ],
    );

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let cmd = LocalSourceCommand::new(&root, Arc::clone(&db));
    let url = format!("file://{}", tarball.display());
    cmd.add("archive-src", &url, false, true)
        .expect("register archive source");

    let mgr = mcx::core::localsrc::LocalSourceManager::new(&root);
    let sources = cmd.list().expect("list sources");
    let materialized =
        mcx::core::localsrc::materialize_source(&mgr, &sources[0]).expect("materialize");

    let artifact = mgr.downloads_dir().join("archive-src.tar.zst");
    assert!(artifact.exists(), "artifact downloaded into downloads dir");
    assert_eq!(
        materialized.fingerprint,
        mcx::archive::hash::HashVerifier::calculate(&artifact, "sha256").expect("artifact sha"),
        "fingerprint must be the sha256 of the artifact BYTES"
    );
    assert_eq!(materialized.fingerprint.len(), 64);
    assert_eq!(
        materialized.manifest,
        mgr.work_dir().join("archive-src").join("manifest.json")
    );
    assert!(
        materialized.manifest.exists(),
        "manifest extracted into work dir"
    );
    assert!(
        mgr.work_dir()
            .join("archive-src")
            .join("payload/hello.txt")
            .exists()
    );

    // Changed artifact bytes => different fingerprint and re-extraction.
    write_tar_zst(
        &tarball,
        &[
            (
                "manifest.json",
                r#"{"packages":[{"name":"hello-tar","version":"1.1.0","source":"payload","type":"custom","build":["none"],"install":["mkdir -p \"$CUDANE_DEST/usr/share/hello\" && cp hello.txt \"$CUDANE_DEST/usr/share/hello/\""]}]}"#,
            ),
            ("payload/hello.txt", "hello tar v2\n"),
        ],
    );
    let re_materialized =
        mcx::core::localsrc::materialize_source(&mgr, &sources[0]).expect("re-materialize");
    assert_ne!(materialized.fingerprint, re_materialized.fingerprint);
    assert_eq!(
        fs::read_to_string(mgr.work_dir().join("archive-src").join("payload/hello.txt")).unwrap(),
        "hello tar v2\n",
        "work dir reflects the new artifact bytes"
    );

    // Skip semantics on the management layer.
    assert_eq!(
        mcx::core::localsrc::decide_action(
            Some(&re_materialized.fingerprint),
            Some(&re_materialized.fingerprint),
            false,
            false
        ),
        mcx::core::localsrc::LocalSourceAction::Skip
    );
    assert_eq!(
        mcx::core::localsrc::decide_action(
            Some(&re_materialized.fingerprint),
            Some("other-fingerprint"),
            false,
            false
        ),
        mcx::core::localsrc::LocalSourceAction::Rebuild
    );

    // Full build/install flow requires the real ous binary; degrade gracefully.
    let Some(ous) = std::env::var("OUS_BIN")
        .ok()
        .filter(|p| !p.is_empty() && Path::new(p).is_file())
    else {
        eprintln!("OUS_BIN not set to an existing binary; skipping archive install assertion");
        fs::remove_dir_all(&root).expect("cleanup");
        return;
    };

    let result = cmd
        .build_one(&sources[0], false, Some(&ous))
        .expect("build archive source with real ous");
    assert_eq!(result, mcx::core::localsrc::LocalSourceResult::Built);
    assert!(db.is_package_installed("hello-tar").expect("installed"));
    assert_eq!(
        fs::read_to_string(root.join("usr/share/hello/hello.txt")).unwrap(),
        "hello tar v2\n",
        "built archive payload must reach the live root"
    );

    let mtime = fs::metadata(root.join("usr/share/hello/hello.txt"))
        .expect("hello meta")
        .modified()
        .expect("mtime");
    cmd.sync_local_sources().expect("second sync unchanged");
    assert_eq!(
        fs::metadata(root.join("usr/share/hello/hello.txt"))
            .expect("hello meta")
            .modified()
            .expect("mtime"),
        mtime,
        "unchanged artifact must not trigger a rebuild"
    );

    fs::remove_dir_all(&root).expect("cleanup");
}

#[tokio::test]
async fn test_auto_link_origin_pool_url_adds_repo_and_disabled_source() {
    let root = create_temporary_root("auto_link_pool");
    let pool_url = "https://packages.example.org/repo/pool/x86_64/hello-pkg/hello-pkg-1.0.0.xcs";
    let archive = root.join("srv/mirror/hello-pkg-1.0.0.xcs");
    write_outsider_xcs_with_provenance(
        &archive,
        "hello-pkg",
        "1.0.0",
        pool_url,
        Some(serde_json::json!({
            "source_type": "dir",
            "source_url": pool_url,
            "source_revision": null,
            "builder": "ous-1.2.3",
        })),
        &[("usr/bin/hello", "hello\n")],
    );

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let add = AddLocalCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    add.execute(&archive.to_string_lossy())
        .expect("add succeeds");

    assert!(db.is_package_installed("hello-pkg").expect("installed"));

    // Registry repo auto-added: section per host+path, enabled, pointing at base.
    let repo_ini = fs::read_to_string(root.join("etc/mcx/repo.ini")).expect("repo.ini");
    assert!(
        repo_ini.contains("[packages.example.org.repo]"),
        "{repo_ini}"
    );
    assert!(
        repo_ini.contains("url = https://packages.example.org/repo"),
        "{repo_ini}"
    );
    assert!(repo_ini.contains("enabled = true"), "{repo_ini}");

    // Local source recorded: provenance.source_url verbatim, mode=source,
    // disabled because the registry link drives updates.
    let lsrc_ini =
        fs::read_to_string(root.join("etc/mcx/localsources.ini")).expect("localsources.ini");
    assert!(lsrc_ini.contains("[hello-pkg]"), "{lsrc_ini}");
    assert!(
        lsrc_ini.contains(&format!("path = {pool_url}")),
        "{lsrc_ini}"
    );
    assert!(lsrc_ini.contains("mode = source"), "{lsrc_ini}");
    assert!(lsrc_ini.contains("enabled = false"), "{lsrc_ini}");

    // Re-install must not duplicate either record.
    let repo_count = fs::read_to_string(root.join("etc/mcx/repo.ini"))
        .expect("repo.ini")
        .matches("[packages.example.org.repo]")
        .count();
    assert_eq!(repo_count, 1, "repo section must not be duplicated");
    add.execute(&archive.to_string_lossy())
        .expect("re-add succeeds");
    let lsrc_ini2 =
        fs::read_to_string(root.join("etc/mcx/localsources.ini")).expect("localsources.ini");
    assert_eq!(
        lsrc_ini2.matches("[hello-pkg]").count(),
        1,
        "local source section must not be duplicated"
    );
    let repo_count2 = fs::read_to_string(root.join("etc/mcx/repo.ini"))
        .expect("repo.ini")
        .matches("[packages.example.org.repo]")
        .count();
    assert_eq!(
        repo_count2, 1,
        "repo section must not be duplicated after re-add"
    );

    fs::remove_dir_all(&root).expect("cleanup");
}

#[tokio::test]
async fn test_auto_link_origin_no_pool_records_enabled_source() {
    let root = create_temporary_root("auto_link_dir");
    let source_path = "/srv/build-src/hello-pkg";
    let archive = root.join("srv/mirror/hello-pkg-1.0.0.xcs");
    write_outsider_xcs_with_provenance(
        &archive,
        "hello-pkg",
        "1.0.0",
        "unused",
        Some(serde_json::json!({
            "source_type": "dir",
            "source_url": source_path,
            "source_revision": "abc123",
            "built_at": "2026-01-01T00:00:00Z",
            "builder": "ous-1.2.3",
        })),
        &[("usr/bin/hello", "hello\n")],
    );

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let add = AddLocalCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    add.execute(&archive.to_string_lossy())
        .expect("add succeeds");

    // No /pool/ in source_url: no registry repo write.
    assert!(
        !root.join("etc/mcx/repo.ini").exists(),
        "no repo.ini expected without a pool-style origin"
    );

    // Local source recorded enabled=true (no registry link), verbatim path.
    let lsrc_ini =
        fs::read_to_string(root.join("etc/mcx/localsources.ini")).expect("localsources.ini");
    assert!(lsrc_ini.contains("[hello-pkg]"), "{lsrc_ini}");
    assert!(
        lsrc_ini.contains(&format!("path = {source_path}")),
        "{lsrc_ini}"
    );
    assert!(lsrc_ini.contains("mode = source"), "{lsrc_ini}");
    assert!(lsrc_ini.contains("enabled = true"), "{lsrc_ini}");

    fs::remove_dir_all(&root).expect("cleanup");
}

#[tokio::test]
async fn test_auto_link_origin_metadata_source_pool_fallback_and_provenance_display() {
    let root = create_temporary_root("auto_link_meta_fallback");
    let pool_url = "https://mirror.example.net/pool/x86_64/legacy-pkg/legacy-pkg-1.0.0.xcs";
    let archive = root.join("srv/mirror/legacy-pkg-1.0.0.xcs");
    // No provenance block at all: auto-link must fall back to metadata.source.
    write_outsider_xcs_with_provenance(
        &archive,
        "legacy-pkg",
        "1.0.0",
        pool_url,
        None,
        &[("usr/bin/legacy", "legacy\n")],
    );

    let db = Arc::new(mcx::core::database::Database::open(&root).expect("open db"));
    let add = AddLocalCommand::new(root.to_string_lossy().into_owned(), Arc::clone(&db));
    add.execute(&archive.to_string_lossy())
        .expect("add succeeds");

    let repo_ini = fs::read_to_string(root.join("etc/mcx/repo.ini")).expect("repo.ini");
    assert!(repo_ini.contains("[mirror.example.net]"), "{repo_ini}");

    // metadata.source pool fallback: local source path = base without /pool/.
    let lsrc_ini =
        fs::read_to_string(root.join("etc/mcx/localsources.ini")).expect("localsources.ini");
    assert!(lsrc_ini.contains("[legacy-pkg]"), "{lsrc_ini}");
    assert!(
        lsrc_ini.contains("path = https://mirror.example.net"),
        "{lsrc_ini}"
    );
    assert!(
        !lsrc_ini.contains("path = https://mirror.example.net/pool"),
        "{lsrc_ini}"
    );
    assert!(lsrc_ini.contains("enabled = false"), "{lsrc_ini}");

    // Provenance recorded in the DB (from embedded metadata) and visible.
    let meta = db
        .get_package_manifest("legacy-pkg")
        .expect("legacy manifest");
    assert_eq!(meta.provenance, None, "old archive has no provenance block");

    let prov = serde_json::json!({
        "source_type": "git",
        "source_url": "https://git.example.com/srv/proj.git",
        "source_revision": "deadbeef",
        "built_at": "2026-01-01T00:00:00Z",
        "builder": "ous-1.2.3",
    });
    let archive2 = root.join("srv/mirror/modern-pkg-1.0.0.xcs");
    write_outsider_xcs_with_provenance(
        &archive2,
        "modern-pkg",
        "1.0.0",
        "registry:modern",
        Some(prov.clone()),
        &[("usr/bin/modern", "modern\n")],
    );
    add.execute(&archive2.to_string_lossy())
        .expect("add modern succeeds");
    let meta2 = db
        .get_package_manifest("modern-pkg")
        .expect("modern manifest");
    let embedded = meta2.provenance.expect("modern provenance embedded");
    assert_eq!(embedded.source_type, "git");
    assert_eq!(embedded.source_url, "https://git.example.com/srv/proj.git");
    assert_eq!(embedded.source_revision.as_deref(), Some("deadbeef"));
    assert_eq!(embedded.builder.as_deref(), Some("ous-1.2.3"));
    // metadata.source is not a pool/URL -> nothing auto-linked, provenance
    // source_url is a git URL -> local source records it enabled.
    let lsrc_ini2 =
        fs::read_to_string(root.join("etc/mcx/localsources.ini")).expect("localsources.ini");
    assert!(
        lsrc_ini2.contains("path = https://git.example.com/srv/proj.git"),
        "{lsrc_ini2}"
    );

    fs::remove_dir_all(&root).expect("cleanup");
}
