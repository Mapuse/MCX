#

`▐▀` `-` `▀▀▀▀▀▀▀▀▌`

```shell
███╗   ███╗  ██████╗ ██╗    ██╗     ██████╗  █████╗  ██████╗██╗  ██╗ █████╗  ██████╗ ███████╗
████╗ ████║██╔════╝  ╚██╗  ██╔╝     ██╔══██╗██╔══██╗██╔════╝██║ ██╔╝██╔══██╗██╔════╝ ██╔════╝
██╔████╔██║██║         ╚███╔╝       ██████╔╝███████║██║     █████╔╝ ███████║██║  ███╗█████╗  
██║╚██╔╝██║██║       ██╔    ██╗     ██╔═══╝ ██╔══██║██║     ██╔═██╗ ██╔══██║██║   ██║██╔══╝  
██║ ╚═╝ ██║╚██████╗ ██╔╝     ██╗    ██║     ██║  ██║╚██████╗██║  ██╗██║  ██║╚██████╔╝███████╗
╚═╝     ╚═╝ ╚═════╝ ╚═╝      ╚═╝    ╚═╝     ╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝
                                                                                        
███╗   ███╗ █████╗ ███╗   ██╗ █████╗  ██████╗ ███████╗██████╗                           
████╗ ████║██╔══██╗████╗  ██║██╔══██╗██╔════╝ ██╔════╝██╔══██╗                          
██╔████╔██║███████║██╔██╗ ██║███████║██║  ███╗█████╗  ██████╔╝                          
██║╚██╔╝██║██╔══██║██║╚██╗██║██╔══██║██║   ██║██╔══╝  ██╔══██╗                          
██║ ╚═╝ ██║██║  ██║██║ ╚████║██║  ██║╚██████╔╝███████╗██║  ██║                          
╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝                          
```

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- The official package manager of **`Cudane Linux`**.

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

`▀` `-` `▀▀▀▀▀▀`

<details><summary id="contents">Contents</summary>

- [[Commands]](#commands)
- [[Architecture]](#architecture)
  - [[High-level modules]](#high-level-modules)
  - [[Execution flow]](#execution-flow)
  - [[Auto-calibration]](#auto-calibration)
- [[Code structure]](#code-structure)
  - [[commands/ — CLI-level behaviour]](#commands--cli-level-behaviour)
  - [[core/ — Domain logic]](#core--domain-logic)
  - [[network/ — Remote operations]](#network--remote-operations)
  - [[archive/ — Artifact primitives]](#archive--artifact-primitives)
  - [[utils/ — Shared utilities]](#utils--shared-utilities)
- [[Data & persistence]](#data--persistence)
  - [[Ledger state]](#ledger-state)
  - [[On-disk layout]](#on-disk-layout)
  - [[INI-based configuration]](#ini-based-configuration)
  - [[Package format]](#package-format)
  - [[Staging and commit model]](#staging-and-commit-model)
- [[Feature subsystems]](#feature-subsystems)
  - [[Atomic package rollback]](#atomic-package-rollback)
  - [[Content-addressable library store]](#content-addressable-library-store)
  - [[Delta upgrades]](#delta-upgrades)
  - [[Process snapshot / checkpoint]](#process-snapshot--checkpoint)
  - [[P2P swarm distribution]](#p2p-swarm-distribution)
  - [[Streaming mounts]](#streaming-mounts)
  - [[Isolated overlayfs]](#isolated-overlayfs)
  - [[Resource control via cgroups]](#resource-control-via-cgroups)
  - [[Self-update]](#self-update)
  - [[Workspace management]](#workspace-management)
  - [[Vendor (offline mirror)]](#vendor-offline-mirror)
  - [[Completion engine]](#completion-engine)
  - [[Declarative profile validation]](#declarative-profile-validation)
- [[Development]](#development)
  - [[Building]](#building)
  - [[Testing]](#testing)
  - [[Profiling]](#profiling)
- [[Plugin authoring & linking]](#plugin-authoring--linking)
- [[Configuration guide]](#configuration-guide)
- [[Credits]](#credits)
- [[License]](#license)

</details>

<details><summary id="commands">Commands</summary>

## Package management

| Command | Aliases | Description |
| --------- | ---------- | ------------- |
| `install` | `i`, `in`, `add` | Install or upgrade packages. Resolves dependencies via the solver, stages artifacts in `var/tmp/mcx/stage/`, records the transaction, and commits to the ledger. |
| `add-local` | `a`, `local`, `package`, `xcs` | Install a local `.xcs` package file without repository resolution. The artifact is extracted into the staging area, then moved into `var/lib/mcx/active/`. |
| `remove` | `r`, `rm`, `uninstall`, `delete` | Remove installed packages. Marks entries in the ledger, clears staged artifacts, and triggers garbage collection for orphaned dependencies. |
| `search` | `s`, `find`, `look` | Query the in-memory `available` index for packages matching a search pattern. Results are drawn from the last repository sync. |
| `update` | `u`, `refresh`, `sync` | With no arguments, synchronises all configured repository indexes in parallel. With a package list, performs an install of the latest available versions (equivalent to `install`). |
| `upgrade` | `U`, `up`, `dist-upgrade` | Without arguments, upgrades every currently installed package. With explicit package names, upgrades only those. Delegate to `InstallCommand` for resolution and commit. |
| `query` | `q`, `info`, `show` | Display metadata for an installed package: version, license, source URL, file count, and dependency count. Information is read from the persisted `LedgerState`. |
| `clean` | `c`, `wipe`, `clear` | Purge the cache directory (`var/cache/mcx/`) and the staging area (`var/tmp/mcx/stage/`), freeing disk space. |
| `verify` | `v`, `check`, `certify` | Perform a content-integrity check across installed packages (stub — returns success). |
| `fix` | `f`, `fix-deps`, `repair` | Attempt automatic dependency repair (stub — returns success). |
| `config` | `C`, `cfg`, `settings` | Open the TUI configuration editor, targeting `etc/mcx/config.ini` (engine parameters) or `etc/mcx/repo.ini` (repository definitions). |
| `history` | `H`, `log`, `record` | Display installation transaction history. Accepts `--rollback <id>` to revert to a prior transaction. |
| `build` | `b`, `make`, `create` | Rebuild or align the system from a declarative blueprint file (delegated to `SystemCommand::rebuild`). |

## Repository management

| Command | Aliases | Description |
| --------- | ---------- | ------------- |
| `repo-add` | `ra` | Add a repository by `name` and `url`. Optionally accepts a `checksum`. Persisted to `etc/mcx/repo.json` via `RepositoryManager`. |
| `repo-remove` | `rr` | Remove a repository by `name`. Clears the entry from the repository registry. |
| `repo-list` | `rl` | Enumerate configured repositories in `name -> url` format. |

## Global flags

| Flag | Description |
| ------ | ------------- |
| `--root <PATH>` | Override the MCX root directory (default: `/`). All state paths are relative to this root. |

</details>

<details><summary id="architecture">Architecture</summary>

## High-level modules

MCX is organised into six Rust crate-level module groups:

| Module | Path | Responsibility |
| -------- | ------ | ---------------- |
| `commands` | `src/commands/` | CLI command implementations — one file per command group, composed in `mod.rs`. Each command struct implements an `execute` method. |
| `core` | `src/core/` | Domain logic and persistence — database, dependency solver, manifest parsing, transactions, configuration, repositories, history, delta engine, feature engine, plugin registry, profiler, privilege escalation, self-update, vendor mirroring, workspace management. |
| `network` | `src/network/` | Remote data operations — HTTP download (via `reqwest` with `rustls-tls`), parallel repository index synchronisation. |
| `archive` | `src/archive/` | Artifact format handling — extraction of `.xcs` archives, SHA-256 hashing, content verification. |
| `utils` | `src/utils/` | Shared infrastructure — terminal UI helpers (`UserInterface`). |
| `main` / `lib` | `src/main.rs`, `src/lib.rs` | Entry-point and public API surface. `main.rs` parses CLI arguments and dispatches; `lib.rs` re-exports every public type for downstream consumers and integration tests. |

## Execution flow

```
  ┌─────────────────────────────────────────────────────────┐
  │  src/main.rs                                            │
  │  clap::Parser → Cli { root, Commands }                  │
  └───────────┬─────────────────────────────────────────────┘
              │
              ▼
  ┌─────────────────────────────────────────────────────────┐
  │  EngineContext::new(root)                               │
  │   • SystemProfile::probe() — CPU cores, RAM             │
  │   • ConfigManager — mmap config.ini + repo.ini           │
  │   • PluginRegistry — register Fetcher, Builder, Packer  │
  │   • CalibratedParams — thread pool, concurrency, zstd   │
  │   • Database::open(root) — var/lib/mcx/local.json       │
  └───────────┬─────────────────────────────────────────────┘
              │
              ▼    match command
  ┌─────────────────────────────────────────────────────────┐
  │  Command::execute(…)                                    │
  │   • InstallCommand → DependencySolver → DepGraph        │
  │                        → PackageTransaction → commit    │
  │   • RemoveCommand → DbTransaction → commit              │
  │   • SyncCommand → NetworkSyncEngine →                   │
  │                    parallel index download               │
  │   • …                                                   │
  └─────────────────────────────────────────────────────────┘
```

## Auto-calibration

On startup `EngineContext::new()` probes the host system:

- **CPU cores** via `num_cpus`; used for thread-pool sizing, concurrent download cap, and I/O parallelism.
- **Available RAM** via `/proc/meminfo` (parsed by `SystemProfile::probe()`); informs caching and swap decisions.
- **Thread pool mode** read from `config.ini [engine] thread_pool_mode`; accepts `auto` (CPU count), `max` (×2), `half` (÷2), `quad` (×4).
- **Concurrent downloads** from `config.ini [engine] max_concurrent_downloads` (default: `min(CPU, 8)`).
- **Network latency** probed against `https://packages.cudane.org` with a 5-second timeout via `NetworkProber::probe()`; results feed `DecisionEngine` for parallel-install heuristics.
- **Zstd compression level** read from `config.ini [engine] zstd_level` (default: `3`).

All parameters are materialised into a `CalibratedParams` struct and exposed to every subsystem.

</details>

<details><summary id="code-structure">Code structure</summary>

### Entry points

- `src/main.rs`
  - Defines the `Cli` struct (clap `#[derive(Parser)]`) and `Commands` enum with 14 variants.
  - Builds `EngineContext` — the shared environment holding `Database`, `ConfigManager`, `PluginRegistry`, and `SystemProfile`.
  - Matches each `Commands` variant to its command implementation.
  - Uses `UserInterface` for structured stdout output (`display_info`, `display_success`, `display_error`, `render_key_values`, `render_list`).

- `src/lib.rs`
  - Declares the six top-level modules.
  - Re-exports every public type from `commands::*`, `core::*`, `network::*`, `utils::*` so that integration tests and external consumers can `use mcx::*`.

### `commands/` — CLI-level behaviour

| File | Struct | Responsibility |
| ------ | -------- | ---------------- |
| `add.rs` | `AddLocalCommand` | Install a local `.xcs` file. Extracts to `var/tmp/mcx/stage/`, renames into `var/lib/mcx/active/`, updates the ledger. |
| `install.rs` | `InstallCommand` | Full install/upgrade pipeline: resolve deps, download, extract, stage, commit. Async via `tokio`. |
| `remove.rs` | `RemoveCommand` | Remove packages from the ledger and active directory. |
| `search.rs` | `SearchCommand` | Pattern-match against the `available` package index. |
| `sync.rs` | `SyncCommand` | Trigger parallel repository index synchronisation. |
| `system.rs` | `SystemCommand` | Declarative system rebuild from blueprint file. |
| `clean.rs` | `CleanCommand` | Purge cache and staging directories. |
| `configuration.rs` | `ConfigEditorCommand` | Full-screen TUI editor for `config.ini` / `repo.ini`. Supports cut, paste, save, and dirty-state tracking. |

### `core/` — Domain logic

| File | Exports | Role |
| ------ | --------- | ------ |
| `config.rs` | `MappedConfig<'a>`, `ConfigManager`, `CalibratedParams` | Mmap-based INI parser with lifetime-tracked zero-copy via `PhantomData`. `ConfigManager` embeds both `config.ini` (engine) and `repo.ini` (repository URLs), auto-generates defaults, and exposes `calibrate()` for adaptive parameter computation. |
| `database.rs` | `Database`, `DbTransaction`, `LedgerState`, `PackageMetadata`, `RepositoryInfo` | Persistence layer for the installation ledger. `Database` opens/reads/writes `local.json` behind a `Mutex<LedgerState>`. `DbTransaction` stages mutations and commits atomically. |
| `repo.rs` | `RepositoryManager` | CRUD over the repository registry (`etc/mcx/repo.json`). Handles JSON serialisation and file I/O. |
| `manifest.rs` | `ManifestParser` | Deserialise package manifests (format defined by `.xcs` metadata section). |
| `solver.rs` | `DependencySolver`, `ResolutionVerdict`, `UpgradePath` | Build a dependency graph, compute topological operation plan, predict upgrade paths with delta-cost estimation and stability indices, detect deadlocks, and break cycles. `solve_with_analysis()` returns enriched verdict with per-package upgrade analysis. |
| `graph.rs` | `DepGraph` | Directed-acyclic graph representation of package dependencies and conflicts. |
| `transaction.rs` | `PackageTransaction` | Transaction log for install/remove operations; tracks staged files and backup paths. |
| `history.rs` | `HistoryEngine` | Record and inspect installation history; supports rollback to a specific transaction ID. |
| `cache.rs` | `CacheManager` | Manage the on-disk package cache (`var/cache/mcx/`). Prunes by age and size limits. |
| `changelog.rs` | `ChangelogManager` | Append-only changelog for package events. |
| `completion.rs` | `CompletionEngine` | Shell-completion candidate generation (Bash / Zsh / Fish). |
| `declarative.rs` | `ProfileValidator` | Validate declarative system profiles (blueprints) for semantic correctness before applying. |
| `delta.rs` | `DeltaEngine` | Compute and apply binary deltas between package versions (`.xcd` delta format). |
| `lifecycle.rs` | `LifecycleEngine`, `PackageState`, `LifecycleTransition`, `DependencyGraph`, `OrphanSet` | Formal package state machine: Unknown → Resolved → Staged → Installed → Active → MarkedForRemoval → Removed → Purged. Pre/post hook chains, audit history, reachability analysis for safe orphan purging. |
| `package.rs` | `PackageEntity` | Unified representation of a package across all lifecycle stages (manifest, staging, installed, vendored). |
| `plugin.rs` | `PluginRegistry`, `PluginSlot<T>`, `Fetcher`, `Builder`, `Packer`, `CurlFetcher`, `DefaultBuilder`, `ZstdPacker` | Lock-free plugin system with `RwLock<Arc<T>>` hot-swap support. Traits for fetch, build, and pack operations. Registered plugins can be atomically swapped at runtime. |
| `profiler.rs` | `SystemProfile`, `DecisionEngine`, `DecisionMatrix`, `HeuristicVerdict`, `AutoHealer`, `NetworkProber` | Host profiling and adaptive decision-making. `SystemProfile` probes `/proc/cpuinfo`, `/proc/meminfo`. `DecisionEngine` evaluates thread-strategy heuristics. `AutoHealer` diagnoses common system misconfigurations. |
| `sudo.rs` | — | Privilege-escalation helpers for operations that require root (stub). |
| `update.rs` | `SelfUpdateManager` | Check GitHub Releases for newer MCX binary versions and perform self-replacement. |
| `vendor.rs` | `VendorManager` | Offline mirror (vendor) management — download and cache complete dependency trees for air-gapped environments. |
| `workspace.rs` | `WorkspaceManager` | Multi-package workspace operations: parallel builds, shared dependency resolution, output aggregation. |

### `network/` — Remote operations

| File | Struct | Role |
| ------ | -------- | ------ |
| `download.rs` | `Downloader` | HTTP(S) download with `reqwest` + `rustls-tls`. Supports streaming, retries, and integrity hashing. |
| `sync.rs` | `NetworkSyncEngine` | Orchestrate parallel synchronisation of all configured repository indexes. |

### `archive/` — Artifact primitives

| File | Struct | Role |
| ------ | -------- | ------ |
| `extract.rs` | `Extractor` | Decompress and unpack `.xcs` packages (Zstd → tar → filesystem tree). |
| `hash.rs` | `HashVerifier` | SHA-256 digest computation over files and streams. |
| `verify.rs` | `ContentValidator` | Cross-check extracted content against manifest checksums. |

### `utils/` — Shared utilities

| File | Struct | Role |
| ------ | -------- | ------ |
| `ui.rs` | `UserInterface` | Terminal output helpers: `display_info`, `display_success`, `display_error`, `render_key_values`, `render_list`. Wraps `println!` with coloured prefixes and structured formatting. |

</details>

<details><summary id="data--persistence">Data & persistence</summary>

## Ledger state

The central data structure is `LedgerState` (defined in `core::database`):

| Field | Type | Purpose |
| -------- | ------ | --------- |
| `installed` | `HashMap<String, PackageMetadata>` | Currently installed packages, keyed by package name. |
| `available` | `HashMap<String, PackageMetadata>` | Packages discovered from repository indexes, keyed by package name. |
| `repositories` | `Vec<RepositoryInfo>` | Active repository descriptors (`name`, `url`, optional `checksum`). |
| `virtual_provides` | `HashMap<String, String>` | Virtual-package to real-package mapping (e.g. `webserver → apache`). |

## On-disk layout

All paths are relative to the `--root` directory (default `/`).

```
etc/mcx/
├── config.ini          # Engine configuration (mmap-based, INI format)
├── repo.ini            # Repository URL configuration (mmap-based, INI format)
├── repo.json           # Repository registry (JSON, managed by RepositoryManager)

var/
├── lib/mcx/
│   ├── local.json      # LedgerState serialised as JSON
│   ├── active/         # Symlinks to current generation for each installed package
│   ├── generations/    # Per-package numbered snapshots for atomic rollback
│   │   └── <pkg>/
│   │       ├── 1/
│   │       ├── 2/
│   │       └── …
│   ├── cas/            # Content-addressable library store
│   │   └── <hex2>/
│   │       └── <sha256>
│   ├── snapshots/      # Process memory snapshots (Zstd-compressed)
│   │   └── <pkg>/
│   │       └── snap-<timestamp>.mem
│   ├── swarm/          # P2P distribution state
│   │   ├── <pkg>.json  # IPFS/IPLD swarm hash entries
│   │   └── peers.json  # Known P2P peers
│   └── stream/         # Cloud-stream mount scripts
│       └── <pkg>.sh
├── tmp/mcx/
│   └── stage/          # Staging area for in-flight package extractions
└── cache/mcx/          # Package cache (downloaded .xcs files)

~/.mcx/overlays/        # Per-package overlayfs mount points
└── <pkg>/
    ├── upper/
    ├── work/
    └── merged/
```

## INI-based configuration

`MappedConfig` uses `memmap2` for zero-copy INI parsing. Format:

```ini
[engine]
thread_pool_mode = auto
max_concurrent_downloads = 8
zstd_level = 3

[network]
fallback_repos = enabled
latency_threshold_ms = 200
bandwidth_threshold_kbps = 5000

[security]
verify_checksums = true
allow_unverified = false

[cache]
limit_bytes = 5368709120
prune_age_hours = 168
```

`ConfigManager` loads both `config.ini` (engine parameters) and `repo.ini` (repository URLs with priority/weight) as lifetime-tracked `MappedConfig<'static>` instances. Default files are written on first access. `calibrate()` reads the INI values and produces a `CalibratedParams` struct with adaptive thread-pool sizing, concurrency caps, and network thresholds.

## Package format

MCX packages use the `.xcs` extension:

| Component | Detail |
| ----------- | -------- |
| Container | tar archive |
| Compression | Zstandard (level configured in `config.ini`, default 3) |
| Archive command | `zstd --compress -3 --tar -o output.xcs input/` |
| Extract command | `zstd --decompress --tar -o output_dir input.xcs` |
| Internal structure | Plain directory tree with no wrapper metadata; metadata is stored in the ledger rather than inside the archive. |

## Staging and commit model

1. `Database::begin_transaction()`
   - Clones the current `LedgerState` into a mutable `staging_state`.
   - Initialises a fresh `PackageTransaction` log.

2. Command execution mutates `staging_state`:
   - Adds/removes entries in `installed` and `available`.
   - Extracts packages to `var/tmp/mcx/stage/`.
   - The `PackageTransaction` records every file operation.

3. `DbTransaction::commit()`
   - Writes the transaction log.
   - Serialises `staging_state` to `local.json` (truncate + atomic write).
   - Swaps the in-memory `LedgerState` behind the `Mutex`.

This two-phase approach ensures that every mutation is logged and that partial failures do not corrupt the persisted ledger.

</details>

<details><summary id="feature-subsystems">Feature subsystems</summary>

## Atomic package rollback

Generation-based rollback is implemented at the filesystem level:

- Before modification, the current file tree of a package is copied into `var/lib/mcx/generations/<pkg>/<N>/`.
- A symlink at `var/lib/mcx/active/<pkg>` points to the current generation.
- `rollback_to_generation(pkg, gen)` flips the symlink back to an older generation.
- `list_generations(pkg)` enumerates available snapshots.
- On install, `enable_atomic_rollback()` creates the generation snapshot and updates the active symlink.

## Content-addressable library store

The CAS (`deduplicate_libraries()`) eliminates duplicate shared libraries:

1. Scans `system/lib/` in staged packages for `.so` / `.so.*` files.
2. Computes SHA-256 hash of each file.
3. Stores each unique file in `var/lib/mcx/cas/<hex2>/<sha256>`.
4. Replaces duplicates with hard links to the CAS copy.
5. `cas_stats()` reports total unique files and bytes saved.

## Delta upgrades

`DeltaEngine` applies binary deltas (`.xcd` files) via `DeltaEngine::apply_delta()`:

1. Extracts the old `.xcs` package to a temp directory.
2. Extracts the `.xcd` delta archive.
3. Reads `diff.meta` — a JSON manifest listing removed files.
4. Overlays new/changed files from the delta.
5. Deletes files listed in `diff.meta.removed`.
6. Re-packs the result as a new `.xcs` at Zstd level 3.

Delta files are produced externally by comparing two package versions and encoding the difference in `.xcd` format (tar + Zstd with a `diff.meta` entry).

## Process snapshot / checkpoint

`checkpoint_process(pkg, pid)` captures the runtime state of a running process associated with a package:

1. Attempts to read `/proc/<pid>/mem` for a full memory dump.
2. Falls back to `/proc/<pid>/maps` if the mem file is inaccessible.
3. Compresses the output with Zstd and writes to `var/lib/mcx/snapshots/<pkg>/snap-<timestamp>.mem`.
4. `list_snapshots(pkg)` returns all available checkpoint files.

## P2P swarm distribution

MCX supports peer-to-peer package distribution via IPFS/IPLD swarm hashes:

- `register_swarm_hash(meta, swarm_hash)` persists a mapping from package name/version to a content hash in `var/lib/mcx/swarm/<pkg>.json`.
- `get_swarm_hash(pkg)` retrieves the stored hash.
- Peers are registered in `var/lib/mcx/swarm/peers.json` with `address`, `peer_id`, `last_seen`, and `advertised_hashes`.
- `register_swarm_peer(peer)` upserts a peer entry.
- `list_swarm_peers()` returns all known peers.

## Streaming mounts

For packages hosted on remote HTTP servers, `generate_stream_mount_script()` creates a shell script that uses `squashfuse` with HTTP range requests:

```sh
#!/bin/sh
# MCX cloud-stream mount for <pkg> v<ver>
URL="https://..."
MOUNT="/mnt/<pkg>"
CACHE="/var/cache/mcx/stream"
mkdir -p "$MOUNT" "$CACHE"
squashfuse "$URL" "$MOUNT" -o ro,allow_other,cache=cache_dir="$CACHE"
```

The script is written to `var/lib/mcx/stream/<pkg>.sh` with executable permissions. It falls back gracefully if `squashfuse` is not installed.

## Isolated overlayfs

`create_isolated_overlay(pkg, home_overlay_path)` creates a three-layer overlayfs mount for package-level filesystem isolation:

```
~/.mcx/overlays/<pkg>/
├── upper/          # Writable layer
├── work/           # Overlayfs work directory
└── merged/         # Merged view
```

A `mount-overlay.sh` script is generated that:
1. Creates the overlay mount with `mount -t overlay`.
2. Bind-mounts the merged view over the target path.
3. The overlay is cleaned up by `remove_isolated_overlay(pkg)`.

## Resource control via cgroups

`enforce_resource_limits(pkg, max_memory_mb, max_cpu_percent)` writes to cgroup v2 control files:

- **Memory limit**: `echo <bytes> > /sys/fs/cgroup/mcx/<pkg>/memory.max`
- **CPU quota**: `echo <quota> 100000 > /sys/fs/cgroup/mcx/<pkg>/cpu.max`

`remove_resource_limits(pkg)` removes the cgroup directory. Package names are sanitised for cgroup path safety (non-alphanumeric characters replaced with `_`).

## Self-update

`SelfUpdateManager` (in `core::update.rs`) checks the project's GitHub Releases page for newer MCX binary versions. It downloads, verifies, and replaces the running binary. Not yet wired into the CLI dispatch.

## Workspace management

`WorkspaceManager` (in `core::workspace.rs`) coordinates operations across multiple packages simultaneously:

- Parallel builds across workspace members.
- Shared dependency resolution to avoid redundant downloads.
- Aggregated output and error reporting.

## Vendor (offline mirror)

`VendorManager` (in `core::vendor.rs`) downloads complete dependency trees for air-gapped environments:

- Recursive dependency resolution and download.
- Manifests stored in a vendored directory structure.
- Enables `mcx install` without network access when the vendor directory is present.

## Completion engine

`CompletionEngine` (in `core::completion.rs`) generates shell-completion scripts:

- Supports Bash, Zsh, and Fish.
- Generates completions for all commands, aliases, and flags.
- Output is written to the appropriate system completions directory or stdout.

</details>

<details><summary id="plugin-authoring">Plugin authoring & linking</summary>

## Architecture

The plugin system is built around three core traits defined in `core/plugin.rs`:

```
Fetcher        — fetch source artifacts from remote locations
Builder        — compile source code into deployable binaries
Packer         — compress/decompress .xcs package archives
```

Each plugin is registered as a `PluginSlot<T>` — a lock-free wrapper using `RwLock<Arc<T>>`. This enables **live hot-swap**: any reader gets an `Arc::clone()` with zero contention, and a writer can atomically replace the internal `Arc` while existing references continue operating on the old version.

```
                    ┌──────────────────────┐
  Thread 1 (reader) │  slot.load() → Arc   │  ← lock-free, no wait
                    └──────────────────────┘
                    ┌──────────────────────┐
  Thread 2 (writer) │  slot.swap(new Arc)  │  ← drains writer lock,
                    │  returns old Arc     │     readers never block
                    └──────────────────────┘
```

## Creating a custom plugin

Implement the corresponding trait. Every plugin must also implement `Send + Sync` and provide a `name()` method for registry lookups.

### Custom fetcher

```rust
use std::sync::Arc;
use anyhow::Result;
use mcx::core::plugin::Fetcher;

pub struct MyFetcher;

impl Fetcher for MyFetcher {
    fn fetch(&self, source: &str, destination: &str) -> Result<()> {
        std::fs::create_dir_all(destination)?;
        // Custom fetch logic — wget, rsync, s3 cp, etc.
        let status = std::process::Command::new("wget")
            .arg("-q")
            .arg("-O")
            .arg("-")
            .arg(source)
            .stdout(std::process::Stdio::piped())
            .status()?;
        if !status.success() {
            anyhow::bail!("MyFetcher failed for: {}", source);
        }
        Ok(())
    }

    fn name(&self) -> &'static str { "my-fetcher" }
}
```

### Custom builder

```rust
use mcx::core::plugin::Builder;

pub struct CustomBuilder;

impl Builder for CustomBuilder {
    fn build(&self, build_cmd: &str, source_dir: &str,
             _dest_dir: &str, build_type: &str) -> Result<String> {
        // Custom build pipeline
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("({}) 2>&1", build_cmd))
            .current_dir(source_dir)
            .output()?;
        let log = String::from_utf8_lossy(&output.stdout).to_string();
        if !output.status.success() {
            anyhow::bail!("Custom build failed:\n{}", log);
        }
        Ok(log)
    }

    fn name(&self) -> &'static str { "custom-builder" }
}
```

### Custom packer

```rust
use mcx::core::plugin::Packer;

pub struct Lz4Packer;

impl Packer for Lz4Packer {
    fn pack(&self, source_dir: &str, output_path: &str,
            compression_level: i32) -> Result<()> {
        let _ = std::fs::remove_file(output_path);
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("tar -c -C '{}' . | lz4 -{} -o '{}'",
                         source_dir, compression_level, output_path))
            .status()?;
        if !status.success() {
            anyhow::bail!("Lz4Packer failed on: {}", output_path);
        }
        Ok(())
    }

    fn unpack(&self, archive_path: &str, dest_dir: &str) -> Result<Vec<String>> {
        std::fs::create_dir_all(dest_dir)?;
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("lz4 -d '{}' - | tar -x -C '{}'",
                         archive_path, dest_dir))
            .status()?;
        if !status.success() {
            anyhow::bail!("Lz4Packer unpack failed on: {}", archive_path);
        }
        // Walk dest_dir to collect file list
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(dest_dir).min_depth(1) {
            let entry = entry?;
            if entry.path().is_file() {
                files.push(entry.path().to_string_lossy().to_string());
            }
        }
        Ok(files)
    }

    fn name(&self) -> &'static str { "lz4" }
}
```

## Registering and linking plugins

During startup in `EngineContext::new()`, plugins are registered with the `PluginRegistry`:

```rust
use std::sync::Arc;
use mcx::core::plugin::{PluginRegistry, CurlFetcher, DefaultBuilder, ZstdPacker};

let mut registry = PluginRegistry::new();

// Register built-in plugins
registry.register_fetcher(Arc::new(CurlFetcher));
registry.register_builder(Arc::new(DefaultBuilder));
registry.register_packer(Arc::new(ZstdPacker));

// Register your custom plugin
registry.register_fetcher(Arc::new(MyFetcher));
registry.register_builder(Arc::new(CustomBuilder));
registry.register_packer(Arc::new(Lz4Packer));
```

## Resolving plugins at runtime

```rust
// Get a plugin by name (returns Arc, zero-copy)
let fetcher = registry.resolve_fetcher("my-fetcher")
    .expect("my-fetcher not registered");
fetcher.fetch("https://example.com/src", "/tmp/build");

// Get the first-registered (default) plugin
let default_packer = registry.default_packer()
    .expect("no packer registered");
default_packer.pack("/tmp/build", "output.xcs", 3);
```

## Live hot-swapping

Swap a plugin at runtime without restarting. Existing operations complete on the old Arc; new operations see the replacement instantly.

```rust
// Atomically replace the "curl" fetcher with a custom one
registry.swap_fetcher("curl", Arc::new(MyFetcher));

// Old CurlFetcher references still in-flight are safe
// Subsequent resolve_fetcher("curl") calls return MyFetcher
```

## Trait contract summary

| Trait | Method | Signature |
| ----- | ------ | --------- |
| `Fetcher` | `fetch` | `(&self, source: &str, destination: &str) -> Result<()>` |
| `Fetcher` | `name` | `(&self) -> &'static str` |
| `Builder` | `build` | `(&self, build_cmd: &str, source_dir: &str, dest_dir: &str, build_type: &str) -> Result<String>` |
| `Builder` | `name` | `(&self) -> &'static str` |
| `Packer` | `pack` | `(&self, source_dir: &str, output_path: &str, compression_level: i32) -> Result<()>` |
| `Packer` | `unpack` | `(&self, archive_path: &str, dest_dir: &str) -> Result<Vec<String>>` |
| `Packer` | `name` | `(&self) -> &'static str` |

All traits require `Send + Sync`.

</details>

<details><summary id="configuration-guide">Configuration guide</summary>

## Architecture

MCX uses a two-tier configuration system:

1. **mmap-based INI config** (`core/config.rs`) — `ConfigManager` holds two memory-mapped configs (`config.ini` for engine parameters, `repo.ini` for repository definitions). Values are parsed zero-copy directly from the mapped region with proper lifetime tracking via `PhantomData`.

2. **JSON repository registry** (`core/repo.rs`) — `RepositoryManager` manages a list of repository descriptors (`name`, `url`, optional `checksum`) persisted to `etc/mcx/repo.json`. This is the runtime registry used by `repo-add`/`repo-remove`/`repo-list` CLI commands.

## File locations

All paths are relative to `--root` (default: `/`).

| Path | Format | Purpose | Managed by |
| ------ | ------ | --------- | ---------- |
| `etc/mcx/config.ini` | INI | Engine parameters (thread pool, network, security, cache) | `ConfigManager` (on first access) |
| `etc/mcx/repo.ini` | INI | Repository URLs with priority/enabled flags | `ConfigManager` (on first access) |
| `etc/mcx/repo.json` | JSON | Repository registry (add/remove/list) | `RepositoryManager` |

## `config.ini` — engine parameters

Written automatically on first `ConfigManager::new()`. Default content:

```ini
[engine]
thread_pool_mode = auto
max_concurrent_downloads = 8
zstd_level = 3

[network]
fallback_repos = enabled
latency_threshold_ms = 200
bandwidth_threshold_kbps = 5000

[security]
verify_checksums = true
allow_unverified = false

[cache]
limit_bytes = 5368709120
prune_age_hours = 168
```

### Section: `[engine]`

| Key | Default | Values | Effect |
| --- | ------- | ------ | ------ |
| `thread_pool_mode` | `auto` | `auto`, `max`, `half`, `quad` | Sets thread pool size relative to CPU count. `auto=cpus`, `max=cpus*2`, `half=cpus/2`, `quad=cpus*4`. |
| `max_concurrent_downloads` | `8` | integer | Caps parallel HTTP downloads. Clamped to `min(cpus, value)`. |
| `zstd_level` | `3` | 1–19 | Compression level for `.xcs` package archives. Higher = smaller but slower. |

### Section: `[network]`

| Key | Default | Values | Effect |
| --- | ------- | ------ | ------ |
| `fallback_repos` | `enabled` | `enabled`, `disabled` | When enabled, if primary repo is unreachable, MCX falls back to secondary repos. |
| `latency_threshold_ms` | `200` | integer (ms) | If network latency exceeds this threshold, MCX adjusts concurrency downward. |
| `bandwidth_threshold_kbps` | `5000` | integer (kbps) | If measured bandwidth drops below this, MCX switches to serial downloads. |

### Section: `[security]`

| Key | Default | Values | Effect |
| --- | ------- | ------ | ------ |
| `verify_checksums` | `true` | `true`, `false` | When enabled, every downloaded package is verified against its SHA-256 checksum before extraction. |
| `allow_unverified` | `false` | `true`, `false` | When true, packages without checksums are still installed with a warning. Affects `fix-deps` and `verify` behaviour. |

### Section: `[cache]`

| Key | Default | Values | Effect |
| --- | ------- | ------ | ------ |
| `limit_bytes` | `5368709120` | integer (bytes, 5 GB default) | Maximum size of the package cache at `var/cache/mcx/`. `clean` and `CacheManager` use this for pruning. |
| `prune_age_hours` | `168` | integer (hours, 7 days) | Packages older than this age are candidates for automatic cache eviction. |

## `repo.ini` — repository configuration

Written automatically on first `ConfigManager::new()`. Default content:

```ini
[main]
url = https://packages.cudane.org
enabled = true
priority = 100

[community]
url = https://community.cudane.org
enabled = false
priority = 200
```

| Key | Required | Values | Effect |
| --- | -------- | ------ | ------ |
| `url` | yes | URL string | Base URL of the package repository index. |
| `enabled` | yes | `true`, `false` | Whether this repo is active during sync. Disabled repos are skipped. |
| `priority` | yes | integer | Lower number = higher priority. Used by the solver to select between packages available from multiple repos. |

### Editing `repo.ini`

Use the TUI editor:

```bash
# Open repo.ini in the built-in text editor
mcx config
```

Or edit directly:

```bash
# Manual edit
$EDITOR /etc/mcx/repo.ini
```

### Adding a new repository

```ini
[my-repo]
url = https://my-packages.example.com/mcx
enabled = true
priority = 50
```

Sections are parsed by `ConfigParser` — the section header `[name]` becomes the repository key. Keys must be unique (last-writer-wins per section).

## `repo.json` — repository registry (CLI-managed)

The CLI commands `repo-add`/`repo-remove`/`repo-list` operate on `etc/mcx/repo.json`:

```bash
# Add a repository
mcx repo-add my-repo https://my-packages.example.com/mcx

# Add a repository with a checksum
mcx repo-add my-repo https://my-packages.example.com/mcx --checksum sha256:abc123...

# List configured repositories
mcx repo-list

# Remove a repository
mcx repo-remove my-repo
```

Format of `repo.json`:

```json
[
  {
    "name": "my-repo",
    "url": "https://my-packages.example.com/mcx",
    "checksum": null
  }
]
```

## Reading configuration programmatically

```rust
use std::path::Path;
use mcx::core::config::ConfigManager;

let mgr = ConfigManager::new(Path::new("/"))?;

// Read from config.ini (zero-copy, mmap-backed)
let thread_mode = mgr.local().get("engine", "thread_pool_mode");  // Option<&str>
let max_dl = mgr.local().get_usize("engine", "max_concurrent_downloads"); // Option<usize>
let verify = mgr.local().get_bool("security", "verify_checksums"); // Option<bool>
let cache_limit = mgr.local().get_u64("cache", "limit_bytes");     // Option<u64>

// Read from repo.ini
let main_url = mgr.repo().get("main", "url");          // Option<&str>
let main_enabled = mgr.repo().get_bool("main", "enabled"); // Option<bool>

// CalibratedParams auto-computes thread pools from config values + CPU count
let params = mgr.calibrate();
println!("Thread pool: {}", params.thread_pool_size);
```

## Auto-calibration at startup

`calibrate()` reads `config.ini` and bakes a `CalibratedParams` struct:

| Field | Source | Fallback |
| ----- | ------ | -------- |
| `thread_pool_size` | `[engine] thread_pool_mode` evaluated against `num_cpus` | `num_cpus` |
| `concurrent_downloads` | `[engine] max_concurrent_downloads` | `min(cpus, 8)` |
| `zstd_level` | `[engine] zstd_level` | `3` |
| `io_parallelism` | `thread_pool_size.max(2)` | `cpus.max(2)` |
| `network_latency_adaptive` | `[network] fallback_repos` | `true` |
| `latency_threshold_ms` | `[network] latency_threshold_ms` | `200` |
| `bandwidth_threshold_kbps` | `[network] bandwidth_threshold_kbps` | `5000` |

## Complete directory tree

```
<root>/
└── etc/mcx/
    ├── config.ini         # Engine config (mmap, zero-copy)
    ├── repo.ini           # Repo definitions (mmap, zero-copy)
    └── repo.json          # Repo registry (JSON, CLI-managed)
```

</details>

<details><summary id="development">Development</summary>

## Building

```bash
# Development build (debug, unoptimised)
cargo build

# Release build (optimised for size: opt-level = "z", LTO, single codegen unit, abort on panic, stripped)
cargo build --release

# Check compilation without producing artefacts
cargo check
```

## Testing

```bash
# Run all unit and integration tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

## Profiling

```bash
# Build with debug symbols for profiling
cargo build --profile release

# Run with perf (Linux)
perf record ./target/release/mcx install <pkg>
perf report
```

</details>

<details><summary id="credits">Credits</summary>

**`MCX`** is part of the **`Cudane` Linux** ecosystem.

- **`Cudane`** — The Linux Distribution.
- **`MCX`** — Runtime Package Manager.

</details>

<details><summary id="license">License</summary>

The Unlicense — see [**`LICENSE`**](github.com/Cudane/MCX/LICENSE) file for details.

</details>

`-` `▄▄▄▄▄▄▄▄▌`

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`Version`:** **`2.7.8`**.
- **`Architecture`:** **`x86_64-unknown-linux-musl`** (**`x86_64-pc-linux-musl`**).
- **`Compression`:** **`Zstd Level 3 (.xcs)`**.

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`
