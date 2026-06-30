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

- **`The package manager of Cudane Linux`**.

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

`▀` `-` `▀▀▀▀▀▀`

<details><summary id="contents">Contents</summary>

- [[Commands]](#commands)
- [[Architecture]](#architecture)
  - [[Module dependency graph]](#module-dependency-graph)
  - [[Module inventory]](#module-inventory)
  - [[Trait contracts — core domain]](#trait-contracts--core-domain)
  - [[Execution flow — phases]](#execution-flow--phases)
  - [[Auto-calibration]](#auto-calibration)
- [[Code structure]](#code-structure)
  - [[Module relationships]](#module-relationships)
  - [[Entry points]](#entry-points)
  - [[commands/ — CLI-level behaviour]](#commands--cli-level-behaviour)
  - [[core/ — Domain logic]](#core--domain-logic)
  - [[network/ — Remote operations]](#network--remote-operations)
  - [[archive/ — Artifact primitives]](#archive--artifact-primitives)
  - [[utils/ — Shared utilities]](#utils--shared-utilities)
- [[Data & persistence]](#data--persistence)
  - [[Ledger state — JSON schema]](#ledger-state--json-schema)
  - [[On-disk layout]](#on-disk-layout)
  - [[INI-based configuration]](#ini-based-configuration)
  - [[Package format]](#package-format)
  - [[Staging and commit model]](#staging-and-commit-model)
  - [[Transaction log format]](#transaction-log-format)
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
- [[Development]](#development)
  - [[Building]](#building)
  - [[Testing]](#testing)
  - [[Linting and static analysis]](#linting-and-static-analysis)
  - [[Auditing]](#auditing)
  - [[Debugging]](#debugging)
  - [[Profiling]](#profiling)
  - [[Continuous integration]](#continuous-integration)
- [[Plugin authoring & linking]](#plugin-authoring--linking)
- [[Configuration guide]](#configuration-guide)
- [[Credits]](#credits)
- [[License]](#license)

</details>

<details><summary id="commands">Commands</summary>

## Dispatch

CLI parsing is handled by `clap` derive macros in `src/main.rs`. The `Cli` struct defines the `--root` global flag; the `Commands` enum defines every subcommand with its arguments, aliases, and short-flag mappings. Each variant dispatches to a dedicated command struct in `src/commands/`.

## Package management

| Short | Long | Aliases | Struct | Module |
| ----- | ---- | ------- | ------ | ------ |
| `-i` | `--install` | `in`, `add` | `InstallCommand` | `commands::install` |
| `-a` | `--add` | `local`, `package`, `xcs` | `AddLocalCommand` | `commands::add` |
| `-r` | `--remove` | `rm`, `uninstall`, `delete` | `RemoveCommand` | `commands::remove` |
| `-s` | `--search` | `find`, `look` | `SearchCommand` | `commands::search` |
| `-u` | `--update` | `refresh`, `sync` | `SyncCommand` / `InstallCommand` | `commands::sync` / `commands::install` |
| `-U` | `--upgrade` | `up`, `dist-upgrade` | `InstallCommand` | `commands::install` |
| `-q` | `--query` | `info`, `show` | inline in `main.rs` | — |
| `-c` | `--clean` | `wipe`, `clear` | `CleanCommand` | `commands::clean` |
| `-V` | `--verify` | `check`, `certify` | inline in `main.rs` | — |
| `-f` | `--fix` | `fix-deps`, `repair` | inline in `main.rs` | — |
| `-C` | `--config` | `cfg`, `settings` | `ConfigEditorCommand` | `commands::configuration` |
| `-H` | `--history` | `log`, `record` | inline in `main.rs` | — |
| `-b` | `--build` | `make`, `create` | `SystemCommand` | `commands::system` |

### `-i` / `--install`

```
mcx -i <package>...
mcx --install <package>...
mcx in <package>...
```

Resolves the dependency graph for the target packages via `DependencySolver`, downloads missing `.xcs` archives into `var/cache/mcx/`, verifies SHA-256 checksums, extracts each package in parallel (≥4 CPUs + ≥1 GB RAM triggers `spawn_blocking` per-package), copies artifacts into both the active root and `var/lib/mcx/active/<pkg>/`, and commits the transaction to `local.json`.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `packages` | `Vec<String>` positional | yes | Package names to install |
| `--root` | global `-PATH-` | no | MCX root (default `/`) |

### `-a` / `--add`

```
mcx -a <file.xcs>
mcx --add <file.xcs>
mcx local <file.xcs>
```

Installs a local `.xcs` package file directly — no dependency resolution, no repository lookup. Extracts the archive to `var/tmp/mcx/stage/`, renames the staging directory into `var/lib/mcx/active/`, and updates the ledger.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `file` | `String` positional | yes | Path to `.xcs` file |

### `-r` / `--remove`

```
mcx -r <package>...
mcx --remove <package>...
mcx rm <package>...
```

Performs a self-healing deep-purge removal. Traces the reverse dependency graph via `deep_purge_analysis()` to identify orphaned packages. Removes each target's active directory, all manifest-listed files, scours `etc/mcx/`, `var/lib/mcx/`, `var/tmp/mcx/`, `var/cache/mcx/` for package-keyed residue, cleans dangling symlinks, and commits the transaction.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `packages` | `Vec<String>` positional | yes | Package names to remove |

### `-s` / `--search`

```
mcx -s <query>
mcx --search <query>
mcx find <query>
```

Pattern-matches `query` against the `available` index in the current `LedgerState` (populated by the last `update`/sync). Results are printed to stdout via `UserInterface`.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `query` | `String` positional | yes | Search pattern |

### `-u` / `--update`

```
mcx -u                         # sync all repo indexes
mcx -u <package>...            # install latest versions
mcx --update <package>...
mcx refresh <package>...
```

Without package arguments: triggers `SyncCommand` which calls `NetworkSyncEngine` to download all configured repository indexes in parallel.

With package arguments: delegates to `InstallCommand`, resolving and installing the specified packages.

### `-U` / `--upgrade`

```
mcx -U                         # upgrade all installed
mcx -U <package>...            # upgrade specific packages
mcx --upgrade <package>...
mcx up <package>...
```

Without arguments: collects all currently installed package names from the ledger, then runs `InstallCommand` over the full set.

With arguments: runs `InstallCommand` on the specified subset.

### `-q` / `--query`

```
mcx -q <package>
mcx --query <package>
mcx info <package>
```

Queries `PackageMetadata` from the ledger and displays:

| Output | Content |
| ------ | ------- |
| Key-value table | Package, Version, License, Source, file count, dependency count, reverse-dependency count |
| Dependency tree | Each dependency: `name version (type)` — resolved real-time from ledger |
| Required by | List of installed packages that declare this package as a dependency |
| Installed files | Full paths of every file claimed by the manifest |

### `-c` / `--clean`

```
mcx -c
mcx --clean
mcx wipe
```

Calls `CleanCommand::execute(true, true)` to purge both the cache directory (`var/cache/mcx/`) and staging area (`var/tmp/mcx/stage/`).

### `-V` / `--verify`

```
mcx -V
mcx --verify
```

Runs six integrity checks across the entire system:

| Check | What it does |
| ----- | ------------ |
| File existence | Every path in every package manifest must exist on disk |
| Active directory | Every installed package must have a `var/lib/mcx/active/<pkg>/` directory |
| Dependency integrity | Every dependency declared by an installed package must itself be installed |
| Dangling symlinks | Recurses `usr/`, `etc/`, `var/` under root counting symlinks whose target is missing |

If all checks pass: reports "All N packages intact. No broken deps, no missing files, no dangling symlinks."
If any check fails: lists every issue and advises `mcx -f` to repair.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| (none) | — | — | Operates on all installed packages |

### `-f` / `--fix`

```
mcx -f
mcx --fix
```

Scans all installed packages for two kinds of breakage and repairs them:

| Check | Action |
| ----- | ------ |
| Missing files | Any package whose manifest-listed files are not present on disk is reinstalled via `InstallCommand` |
| Missing dependencies | Any dependency declared by an installed package that is not itself installed is resolved and installed |

If nothing is broken, reports "All packages intact. No repair needed."

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| (none) | — | — | Operates on all installed packages |

### `-C` / `--config`

```
mcx -C
mcx --config
```

Opens the full-screen TUI editor (`ConfigEditorCommand` in `commands::configuration.rs`). The editor targets `etc/mcx/config.ini`.

Key bindings:

| Key | Action |
| --- | ------ |
| Ctrl+X | Close editor (prompts if dirty) |
| Ctrl+O / Ctrl+S | Save file |
| Ctrl+K | Cut current line |
| Ctrl+U | Paste cut buffer |
| Arrow keys | Navigate |
| PageUp/Down | Scroll |
| Home/End | Line start/end |
| Backspace/Delete | Character deletion |
| Enter | Split line |

### `-H` / `--history`

```
mcx -H
mcx -H --rollback <id>
mcx -H --prune <keep>
mcx -H --current-gen <package>
mcx --history
```

Without flags: prints installation transaction history from `HistoryEngine`.

`--rollback <id>`: computes and displays reverse operations to revert to transaction `id`.

`--prune <keep>`: deletes old generation snapshots for all installed packages, keeping the most recent `keep`.

`--current-gen <package>`: displays the active generation ID for a package.

### `-b` / `--build`

```
mcx -b <config>
mcx --build <config>
```

Calls `SystemCommand::rebuild(&config)` to rebuild or align the system from a declarative blueprint file. `WorkspaceManager` creates build/stage directories before execution and cleans them on completion.

### Blueprint file format

The blueprint is a JSON file describing the target system state. `mcx -b <path>` reads it, computes the diff against the current installed packages, and runs install/remove to converge.

```json
{
  "version": "1.0",
  "architecture": "x86_64",
  "packages": [
    "zlib",
    "libpng",
    "libjpeg-turbo",
    "freetype",
    "fontconfig",
    "harfbuzz"
  ]
}
```

| Field | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `version` | `String` | yes | Blueprint schema version — must be non-empty |
| `architecture` | `String` | yes | Target CPU architecture — validated by `ProfileValidator` |
| `packages` | `Array<String>` | yes | Declared package names — no duplicates, no empty entries |

#### Creating a blueprint

1. **From the current system state** — dump installed packages into a JSON file:
   ```shell
   mcx -q all | awk '{print $1}' | jq -R -s '{version: "1.0", architecture: "x86_64", packages: split("\n")[:-1]}' > profile.json
   ```
2. **Hand-edit** — remove packages you no longer want, add packages you need:
   ```json
   {
     "version": "1.0",
     "architecture": "x86_64",
     "packages": [
       "zlib",
       "libpng",
       "libjpeg-turbo"
     ]
   }
   ```
3. **Converge** — apply the blueprint:
   ```shell
   mcx -b profile.json
   ```
   The engine will remove packages not in the list and install missing ones.

#### Validation rules

`ProfileValidator::load_profile()` enforces:
- `version` must be non-empty
- `architecture` must be non-empty
- No duplicate package names in the array
- No empty-string package entries

If validation fails, `mcx -b` exits with an error before any packages are touched.

## Platform commands

| Command (long flag) | Aliases | Struct | Module |
| ------------------- | ------- | ------ | ------ |
| `--self-update` | `update-self` | `SelfUpdateManager` | `core::update` |
| `--vendor` | `vnd` | `VendorManager` | `core::vendor` |
| `--completion` | `comp` | `CompletionEngine` | `core::completion` |
| `--snapshot` | `snap` | `SnapshotManager` | `core::snapshot` |
| `--swarm` | `p2p` | `SwarmManager` | `core::swarm` |
| `--overlay` | `ovl` | `OverlayManager` | `core::overlay` |
| `--cgroup` | `cg` | `CgroupController` | `core::cgroup` |
| `--stream` | `str` | `StreamManager` | `core::stream` |
| `--repo-add` | `ra` | `RepositoryManager` | `core::repo` |
| `--repo-remove` | `rr` | `RepositoryManager` | `core::repo` |
| `--repo-list` | `rl` | `RepositoryManager` | `core::repo` |

### `self-update`

```
mcx --self-update
```

Clones `https://codeberg.org/Cudane/MCX` into a temporary directory, runs `cargo build --release --target x86_64-unknown-linux-musl`, and copies the resulting binary to `/system/bin/mcx`. Every invocation performs the full lifecycle — clone, compile, install.

### `--vendor`

```
mcx --vendor add <package> <source.xcs>
mcx --vendor remove <package>
mcx --vendor list
```

Manages an offline package mirror in `var/lib/mcx/vendor/`. When vendored packages are present, `mcx -i` can operate without network access by sourcing from the vendor store.

### `--completion`

```
mcx --completion bash|zsh|fish
```

Generates shell-completion scripts for the specified shell and writes them to stdout. Supports Bash (`complete -F`), Zsh (`#compdef`), and Fish (`complete -c`) formats covering all commands, aliases, and flags.

### `--snapshot`

```
mcx --snapshot take <package> <pid>
mcx --snapshot list <package>
mcx --snapshot restore <package> <snapshot_path> <pid>
mcx --snapshot remove <package>
```

Process memory checkpoint facility. `take` reads `/proc/<pid>/mem` (falls back to `/proc/<pid>/maps`), compresses with Zstd, and writes to `var/lib/mcx/snapshots/<pkg>/snap-<timestamp>.mem`. `restore` writes the decompressed snapshot back to `/proc/<pid>/mem`. `remove` purges all snapshots for a package.

### `--swarm`

```
mcx --swarm register-hash <package> <version> <hash>
mcx --swarm get-hash <package>
mcx --swarm remove-hash <package>
mcx --swarm register-peer <address> <peer_id>
mcx --swarm list-peers
```

Peer-to-peer package distribution via IPFS/IPLD content hashes. Hashes are persisted in `var/lib/mcx/swarm/<pkg>.json`; peer registry in `var/lib/mcx/swarm/peers.json`.

### `--overlay`

```
mcx --overlay create <package> <lower_root>
mcx --overlay remove <package>
mcx --overlay list
```

Per-package overlayfs isolation. `create` builds a three-layer mount (`upper/`, `work/`, `merged/`) at `~/.mcx/overlays/<pkg>/` and generates a `mount-overlay.sh` script. `remove` unmounts and purges the overlay directory.

### `--cgroup`

```
mcx --cgroup enforce <package> <max_memory_mb> <max_cpu_percent>
mcx --cgroup enforce-mem <package> <max_memory_mb>
mcx --cgroup enforce-cpu <package> <max_cpu_percent>
mcx --cgroup remove <package>
mcx --cgroup status
```

cgroup v2 resource enforcement. Writes memory and CPU quota limits to `/sys/fs/cgroup/mcx/<pkg>/memory.max` and `cpu.max`. Package names are sanitised for cgroup path safety. `status` checks whether cgroup v2 is available on the host.

### `--stream`

```
mcx --stream generate <package> <version> <url>
mcx --stream remove <package>
mcx --stream list
```

Generates executable shell scripts at `var/lib/mcx/stream/<pkg>.sh` that mount remote squashfs images via `squashfuse` with HTTP range requests. Falls back to `wget` + `tar` if `squashfuse` is absent.

## Repository management

| Command (long flag) | Aliases | Struct | Module |
| ------------------- | ------- | ------ | ------ |
| `--repo-add` | `ra` | `RepositoryManager` | `core::repo` |
| `--repo-remove` | `rr` | `RepositoryManager` | `core::repo` |
| `--repo-list` | `rl` | `RepositoryManager` | `core::repo` |

### `--repo-add`

```
mcx --repo-add <name> <url>
mcx ra <name> <url>
```

Adds a repository entry to `etc/mcx/repo.json` via `RepositoryManager::add_repository()`.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `name` | `String` positional | yes | Repository identifier |
| `url` | `String` positional | yes | Repository base URL |

### `--repo-remove`

```
mcx --repo-remove <name>
mcx rr <name>
```

Removes a repository entry from `etc/mcx/repo.json` via `RepositoryManager::remove_repository()`.

### `--repo-list`

```
mcx --repo-list
mcx rl
```

Enumerates all configured repositories from `etc/mcx/repo.json` in `name -> url` format.

## Global flags

| Flag | Type | Default | Description |
| ---- | ---- | ------- | ----------- |
| `--root` | `PathBuf` | `/` | MCX root directory. All state paths (`etc/mcx/`, `var/lib/mcx/`, `var/cache/mcx/`, etc.) are resolved relative to this path. |

</details>

<details><summary id="architecture">Architecture</summary>

## Module dependency graph

```
┌───────────────┐      ┌──────────────────┐
│  src/main.rs  │─────▶│ src/commands/    │  CLI dispatch & argument parsing
└───────────────┘      │ install, remove, │
                       │ search, sync, …  │
                       └────────┬─────────┘
                                │
                                ▼
                       ┌──────────────────┐
                       │  src/core/       │  Domain logic & persistence
                       │  ├─ config.rs    │  mmap INI, lifetime-tracked MappedConfig
                       │  ├─ database.rs  │  LedgerState, DbTransaction
                       │  ├─ solver.rs    │  DependencySolver, UpgradePath
                       │  ├─ lifecycle.rs │  PackageState machine, LifecycleEngine
                       │  ├─ plugin.rs    │  PluginSlot<T>, Fetcher/Builder/Packer
                       │  ├─ profiler.rs  │  SystemProfile, DecisionEngine
                       │  ├─ history.rs   │  HistoryEngine, rollback
                       │  ├─ repo.rs      │  RepositoryManager
                       │  ├─ manifest.rs  │  ManifestParser
                       │  ├─ graph.rs     │  DepGraph
                       │  ├─ transaction  │  PackageTransaction
                       │  ├─ cache.rs     │  CacheManager
                       │  ├─ delta.rs     │  DeltaEngine
                       │  ├─ cas.rs       │  Content-addressable library dedup
                       │  ├─ snapshot.rs  │  Process memory checkpoint
                       │  ├─ swarm.rs     │  P2P hash registry
                       │  ├─ stream.rs    │  Squashfuse mount scripts
                       │  ├─ overlay.rs   │  Overlayfs per-package isolation
                       │  ├─ cgroup.rs    │  cgroup v2 resource control
                       │  ├─ rollback.rs  │  Generation-based atomic rollback
                       │  ├─ update.rs    │  Self-update binary replacement
                       │  ├─ vendor.rs    │  Offline package mirroring
                       │  ├─ completion.rs│  Shell completion generation
                       │  ├─ workspace.rs │  Build/stage space orchestration
                       │  └───────────────┘
                       └────────┬───────────┘
                                │
           ┌────────────────────┼────────────────────┐
           ▼                    ▼                    ▼
 ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
 │  src/network/    │  │  src/archive/    │  │  src/utils/      │
 │  download.rs     │  │  extract.rs      │  │  ui.rs           │
 │  sync.rs         │  │  hash.rs         │  │  UserInterface   │
 │  reqwest+rustls  │  │  verify.rs       │  └──────────────────┘
 └──────────────────┘  └──────────────────┘
```

## Module inventory

| Module | Path | Responsibility | Public surface |
| ------ | ---- | -------------- | -------------- |
| `commands` | `src/commands/` | CLI command implementations — one file per command group. Each command struct implements `execute()` taking `EngineContext`. | `InstallCommand`, `RemoveCommand`, `SyncCommand`, `SearchCommand`, `AddLocalCommand`, `CleanCommand`, `ConfigEditorCommand`, `SystemCommand` |
| `core` | `src/core/` | Domain logic — persistence, solver, lifecycle, plugins, profiling, configuration, repositories, history, delta engine, changelog, completion, declarative validation, privilege escalation, self-update, vendor mirroring, workspace management, content-addressable store, process snapshots, P2P swarm, streaming mounts, overlayfs isolation, cgroup control, generation-based rollback. | Config types, `Database`, `DependencySolver`, `LifecycleEngine`, `PluginRegistry`, `SystemProfile`, `HistoryEngine`, `RepositoryManager`, `CacheManager`, `DeltaEngine`, `PackageEntity`, `SelfUpdateManager`, `VendorManager`, `WorkspaceManager`, `ProfileValidator`, `CompletionEngine`, `SnapshotManager`, `SwarmManager`, `StreamManager`, `OverlayManager`, `CgroupController`, `RollbackManager`, `CasManager` |
| `network` | `src/network/` | Remote data operations — HTTP download via `reqwest` + `rustls-tls`, parallel index sync. | `Downloader`, `NetworkSyncEngine` |
| `archive` | `src/archive/` | Artifact format handling — `.xcs` extraction, SHA-256 hashing, content verification. | `Extractor`, `HashVerifier`, `ContentValidator` |
| `utils` | `src/utils/` | Shared infrastructure — terminal output. | `UserInterface` |
| `main` / `lib` | `src/main.rs`, `src/lib.rs` | Entry point, CLI parsing, public re-exports. | `Cli`, `Commands`, `EngineContext` |

## Trait contracts — core domain

| Trait | Module | Method | Signature |
| ----- | ------ | ------ | --------- |
| `Fetcher` | `core::plugin` | `fetch` | `(&self, source: &str, destination: &str) -> Result<()>` |
| `Fetcher` | `core::plugin` | `name` | `(&self) -> &'static str` |
| `Builder` | `core::plugin` | `build` | `(&self, cmd: &str, src: &str, dest: &str, typ: &str) -> Result<String>` |
| `Builder` | `core::plugin` | `name` | `(&self) -> &'static str` |
| `Packer` | `core::plugin` | `pack` | `(&self, src: &str, out: &str, level: i32) -> Result<()>` |
| `Packer` | `core::plugin` | `unpack` | `(&self, path: &str, dest: &str) -> Result<Vec<String>>` |
| `Packer` | `core::plugin` | `name` | `(&self) -> &'static str` |
| `PluginSlot<T>` | `core::plugin` | `load` | `(&self) -> Arc<T>` |
| `PluginSlot<T>` | `core::plugin` | `swap` | `(&self, new: Arc<T>) -> Arc<T>` |
| `DependencySolver` | `core::solver` | `solve` | `(&self, deps: &[String]) -> ResolutionVerdict` |
| `DependencySolver` | `core::solver` | `compute_upgrade_path` | `(&self, from: &Metadata, to: &Metadata) -> UpgradePath` |
| `DependencySolver` | `core::solver` | `solve_with_analysis` | `(&self, deps: &[String]) -> ResolutionVerdict` |
| `LifecycleEngine` | `core::lifecycle` | `transition` | `(&self, pkg: &str, target: PackageState) -> Result<LifecycleTransition>` |
| `LifecycleEngine` | `core::lifecycle` | `can_transition_to` | `(&self, current: PackageState, target: PackageState) -> bool` |
| `LifecycleEngine` | `core::lifecycle` | `audit_log` | `(&self, pkg: &str) -> Vec<AuditEntry>` |
| `LifecycleEngine` | `core::lifecycle` | `find_orphans` | `(&self, graph: &DependencyGraph) -> OrphanSet` |

## Execution flow

```
  ╔══════════════════════════════════════════════╗
  ║  BOOTSTRAP (main.rs → EngineContext::new())  ║
  ╚══════════════════════════════════════════════╝
  1. clap::Parser::parse() → Cli { root, Commands::Install(…) }
  2. EngineContext::new(root):
     a. SystemProfile::probe() — read /proc/cpuinfo, /proc/meminfo
     b. ConfigManager::new(root) — mmap config.ini + repo.ini,
        auto-generate defaults if absent, calibrate() → CalibratedParams
     c. PluginRegistry::new() — register CurlFetcher, DefaultBuilder,
        ZstdPacker as built-in plugins
     d. Database::open(root) — deserialise var/lib/mcx/local.json
        into Mutex<LedgerState>; create empty state if absent
     e. LifecycleEngine::new() — load transition rules, hook chains
     f. DecisionEngine::new() — initialise heuristic matrix
     g. NetworkProber::probe() — ICMP/HTTP latency test (5 s timeout)
     h. CalibratedParams baked from config values + host probe

  ╔══════════════════════════════════╗
  ║  RESOLVE (per-command dispatch)  ║
  ╚══════════════════════════════════╝
  match command {
      Commands::Install(pkgs) => {
          solver.solve_with_analysis(&pkgs)
            → ResolutionVerdict { plan, dep_graph, missing, conflicts, upgrades }
      }
      Commands::Remove(pkgs) => {
          lifecycle.find_orphans(&dep_graph) → OrphanSet
          solver.reverse_deps(&pkgs) → affected list
      }
      Commands::Update(None) => {
          sync_engine.sync_all()  // parallel repo index download
      }
      …
  }

  ╔════════════════════════════════╗
  ║  EXECUTE (transaction commit)  ║
  ╚════════════════════════════════╝
  1. Database::begin_transaction() → DbTransaction
     - Clone LedgerState into staging_state
     - Initialise PackageTransaction log
  2. For each package in plan:
     a. lifecycle.transition(pkg, PackageState::Staged) → hook pre_execute
     b. Download → extract → copy to active root
     c. lifecycle.transition(pkg, PackageState::Installed) → hook post_install
     d. Record in PackageTransaction
  3. DbTransaction::commit() → flush JSON, swap Mutex

  ╔═════════════════════╗
  ║  VERIFY / CLEANUP   ║
  ╚═════════════════════╝
  - ContentValidator::validate(manifest, root) → Result
  - CacheManager::prune() — evict old .xcs files
  - AutoHealer::diagnose() — check for common misconfigurations
```

## Auto-calibration

`EngineContext::new()` probes the host system and materialises a `CalibratedParams` struct:

| Parameter | Source | Probe mechanism | Fallback |
| --------- | ------ | --------------- | -------- |
| CPU cores | `/sys/devices/system/cpu/online` | `num_cpus::get()` | 4 |
| Available RAM | `/proc/meminfo MemAvailable` | `SystemProfile::probe()` | 2048 MB |
| Thread pool size | `[engine] thread_pool_mode` × CPU | `CalibratedParams` evaluation | `num_cpus` |
| Concurrent downloads | `[engine] max_concurrent_downloads` | `min(config, num_cpus)` | `min(cpus, 8)` |
| Zstd compression | `[engine] zstd_level` | direct parse | 3 |
| Network latency | `https://packages.cudane.org` | `NetworkProber::probe()` (5 s timeout) | 200 ms |
| Bandwidth | measured during first download | `DecisionEngine` heuristic | 5000 kbps |

</details>

<details><summary id="code-structure">Code structure</summary>

## Module relationships

```
  main.rs ────→ lib.rs ────→ commands ────→ core ────→ network
                                │              ├────→ archive
                                │              └────→ utils
                                └───→ utils
```

Every `commands::*` struct receives an `EngineContext` reference which gates access to all `core` subsystems. `core` depends on `network` (download during install/sync) and `archive` (extract/verify). `utils` is a leaf module used by both `commands` and `main`.

## Entry points

- **`src/main.rs`**
  - `Cli` struct (clap `#[derive(Parser)]`) — defines `--root` global flag and 14 `Commands` enum variants.
  - `EngineContext::new(root)` — constructs the shared environment holding `Database`, `ConfigManager` (mmap, lifetime-tracked), `PluginRegistry`, `SystemProfile`, `DecisionEngine`, `NetworkProber`.
  - Match on `Commands` variant → dispatch to `command.execute(&engine)`.
  - Output via `UserInterface` methods.

- **`src/lib.rs`**
  - Declares modules: `commands`, `core`, `network`, `archive`, `utils`.
  - Re-exports all public types (`pub use commands::*`, `pub use core::*`, etc.) for integration tests and external consumers of the `mcx` crate.

## `commands/` — CLI-level behaviour

Every command struct implements `pub fn execute(&self, engine: &EngineContext) -> Result<()>`.

| File | Struct | Responsibility | Public API |
| ---- | ------ | -------------- | ---------- |
| `add.rs` | `AddLocalCommand` | Install local `.xcs` file | `execute()` |
| `install.rs` | `InstallCommand` | Full install/upgrade pipeline | `execute()`, `resolve_and_commit()` |
| `remove.rs` | `RemoveCommand` | Remove packages + deep-purge orphans | `execute()`, `deep_purge_analysis()` |
| `search.rs` | `SearchCommand` | Pattern-match available index | `execute()` |
| `sync.rs` | `SyncCommand` | Parallel repo index sync | `execute()` |
| `system.rs` | `SystemCommand` | Declarative rebuild from blueprint | `execute()`, `rebuild()` |
| `clean.rs` | `CleanCommand` | Purge cache + staging | `execute()` |
| `configuration.rs` | `ConfigEditorCommand` | TUI editor for config files | `execute()`, `open_editor()` |

## `core/` — Domain logic

| File | Exports | Role | Dependencies |
| ---- | ------- | ---- | ------------ |
| `config.rs` | `MappedConfig<'a>`, `ConfigManager`, `CalibratedParams` | Mmap INI parser with `PhantomData` lifetime tracking. `ConfigManager` embeds `config.ini` + `repo.ini`. | `memmap2` |
| `database.rs` | `Database`, `DbTransaction`, `LedgerState`, `PackageMetadata` | JSON-backed installation ledger behind `Mutex<LedgerState>`. | `serde_json` |
| `repo.rs` | `RepositoryManager` | CRUD for `etc/mcx/repo.json`. | `serde_json` |
| `manifest.rs` | `ManifestParser` | Deserialise `.xcs` package manifests. | — |
| `solver.rs` | `DependencySolver`, `ResolutionVerdict`, `UpgradePath` | Dependency graph resolution, delta-cost estimation, deadlock detection, cycle breaking. | `graph.rs` |
| `graph.rs` | `DepGraph` | DAG of package dependencies and conflicts. | — |
| `transaction.rs` | `PackageTransaction` | Transaction log for install/remove operations. | — |
| `history.rs` | `HistoryEngine` | Transaction history; rollback to ID. | `database.rs` |
| `cache.rs` | `CacheManager` | On-disk `.xcs` cache; age/size pruning. | — |
| `changelog.rs` | `ChangelogManager` | Append-only changelog writer. | — |
| `completion.rs` | `CompletionEngine` | Shell-completion generation (bash/zsh/fish). | — |
| `declarative.rs` | `ProfileValidator` | Validate declarative system blueprints. | — |
| `delta.rs` | `DeltaEngine` | Binary delta apply (`.xcd` format). | `archive::extract` |
| `lifecycle.rs` | `LifecycleEngine`, `PackageState`, `DependencyGraph`, `OrphanSet` | State machine: Unknown→Resolved→Staged→Installed→Active→MarkedForRemoval→Removed→Purged. Pre/post hooks, audit history. | `database.rs` |
| `package.rs` | `PackageEntity` | Unified package representation across all stages. | — |
| `plugin.rs` | `PluginRegistry`, `PluginSlot<T>`, `Fetcher`, `Builder`, `Packer`, `CurlFetcher`, `DefaultBuilder`, `ZstdPacker` | Lock-free plugin hot-swap via `RwLock<Arc<T>>`. | — |
| `profiler.rs` | `SystemProfile`, `DecisionEngine`, `AutoHealer`, `NetworkProber` | Host profiling, heuristic decisions, network latency probing. | — |
| `sudo.rs` | — | Privilege escalation (stub). | — |
| `update.rs` | `SelfUpdateManager` | GitHub Releases check + binary self-replace. | `network::download` |
| `vendor.rs` | `VendorManager` | Offline mirror: recursive download + caching. | `network::download` |
| `workspace.rs` | `WorkspaceManager` | Multi-package workspace orchestration. | `solver.rs` |

## `network/` — Remote operations

| File | Struct | Role | Dependencies |
| ---- | ------ | ---- | ------------ |
| `download.rs` | `Downloader` | HTTP(S) streaming download with retries and SHA-256 integrity hashing. | `reqwest` + `rustls-tls` |
| `sync.rs` | `NetworkSyncEngine` | Parallel sync of all repository indexes. | `download.rs`, `repo.rs` |

## `archive/` — Artifact primitives

| File | Struct | Role | Dependencies |
| ---- | ------ | ---- | ------------ |
| `extract.rs` | `Extractor` | Zstd → tar → filesystem tree decompression/unpacking. | — |
| `hash.rs` | `HashVerifier` | SHA-256 digest computation for files and streams. | — |
| `verify.rs` | `ContentValidator` | Cross-check extracted content against manifest checksums. | `hash.rs`, `manifest.rs` |

## `utils/` — Shared utilities

| File | Struct | Role | Public methods |
| ---- | ------ | ---- | -------------- |
| `ui.rs` | `UserInterface` | Terminal output with colour prefixes and structured formatting. | `display_info()`, `display_success()`, `display_error()`, `render_key_values()`, `render_list()` |

</details>

<details><summary id="data--persistence">Data & persistence</summary>

## Ledger state — JSON schema

The central data structure is `LedgerState` (defined in `core::database`), serialised to `var/lib/mcx/local.json`:

```json
{
  "installed": {
    "<package_name>": {
      "pkg_name": "zlib",
      "version": "1.2.13",
      "license": "Zlib",
      "source": "https://packages.cudane.org/zlib/1.2.13/xcs",
      "files": ["/usr/lib/libz.so.1.2.13", "/usr/include/zlib.h", "..."],
      "dependencies": ["glibc>=2.35"],
      "checksum": "sha256:a1b2c3d4e5f6..."
    }
  },
  "available": {
    "<package_name>": {
      "pkg_name": "zlib",
      "version": "1.3.0",
      "license": "Zlib",
      "source": "https://packages.cudane.org/zlib/1.3.0/xcs",
      "files": [],
      "dependencies": ["glibc>=2.35"],
      "checksum": "sha256:f6e5d4c3b2a1..."
    }
  },
  "repositories": [
    {
      "name": "main",
      "url": "https://packages.cudane.org",
      "checksum": null
    }
  ],
  "virtual_provides": {
    "webserver": "apache",
    "mailserver": "postfix"
  }
}
```

| Field | Type | Mutability | Purpose |
| ----- | ---- | ---------- | ------- |
| `installed` | `HashMap<String, PackageMetadata>` | read/write | Currently installed packages, keyed by package name. Mutated during install/remove transactions. |
| `available` | `HashMap<String, PackageMetadata>` | read/write | Packages discovered from repository indexes. Cleared and rebuilt on each sync. |
| `repositories` | `Vec<RepositoryInfo>` | read/write | Active repository descriptors. Mutated by `repo-add`/`repo-remove`. |
| `virtual_provides` | `HashMap<String, String>` | read-only | Virtual-package to real-package mapping. Populated from repository indexes. |

### `PackageMetadata` fields

| Field | Type | Description |
| ----- | ---- | ----------- |
| `pkg_name` | `String` | Canonical package name |
| `version` | `String` | Semantic version string |
| `license` | `String` | SPDX license identifier |
| `source` | `String` | URL or path of the source artifact |
| `files` | `Vec<String>` | Absolute paths of installed files |
| `dependencies` | `Vec<String>` | Dependency specs (`name`, `name>=ver`, `name==ver`, `name<ver`) |
| `checksum` | `Option<String>` | SHA-256 hex digest (prefixed `sha256:`) |

## On-disk layout

All paths are relative to the `--root` directory (default `/`).

```
etc/mcx/
├── config.ini          # Engine configuration (mmap-based, INI format)
├── repo.ini            # Repository URL configuration (mmap-based, INI format)
├── repo.json           # Repository registry (JSON, CLI-managed)

var/
├── lib/mcx/
│   ├── local.json      # LedgerState serialised as JSON
│   ├── active/         # Symlinks to current generation for each installed package
│   │   └── <pkg> → ../generations/<pkg>/<N>/
│   ├── generations/    # Per-package numbered snapshots for atomic rollback
│   │   └── <pkg>/
│   │       ├── 1/      # Snapshot N-1
│   │       ├── 2/      # Snapshot N (current)
│   │       └── …
│   ├── cas/            # Content-addressable library store
│   │   └── <hex2>/     # First two hex chars of SHA-256
│   │       └── <sha256>  # Hard-linked unique .so file
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
    ├── upper/          # Writable layer
    ├── work/           # Overlayfs work directory
    └── merged/         # Merged view
```

## INI-based configuration

`MappedConfig` (defined in `core/config.rs`) memory-maps INI files via `memmap2` and parses them zero-copy. The lifetime of the returned `&str` slices is bound to the mapping via `PhantomData<&'a ()>` — the borrow checker prevents accessing config values after the mapping is dropped.

```rust
pub struct MappedConfig<'a> {
    map: Mmap,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> MappedConfig<'a> {
    pub fn get(&self, section: &str, key: &str) -> Option<&'a str> { … }
    pub fn get_usize(&self, section: &str, key: &str) -> Option<usize> { … }
    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> { … }
    pub fn get_u64(&self, section: &str, key: &str) -> Option<u64> { … }
}
```

`ConfigManager` wraps two `MappedConfig` instances:

| File | Contents | Parser |
| ---- | -------- | ------ |
| `etc/mcx/config.ini` | Engine parameters (thread pool, network, security, cache) | `MappedConfig<'static>` via `ConfigManager::local()` |
| `etc/mcx/repo.ini` | Repository definitions (url, enabled, priority per section) | `MappedConfig<'static>` via `ConfigManager::repo()` |

Default files are written on first `ConfigManager::new()` if absent.

## Package format — `.xcs`

| Component | Detail |
| --------- | ------ |
| Container | tar archive |
| Compression | Zstandard (level from `config.ini [engine] zstd_level`, default 3) |
| Compression command | `zstd --compress -3 --tar -o output.xcs input/` |
| Decompression command | `zstd --decompress --tar -o output_dir input.xcs` |
| Internal structure | Plain directory tree with no wrapper metadata |
| Metadata location | Stored in `LedgerState.installed.<pkg>.checksum` — the archive itself has no embedded manifest |

## `.xcd` delta format

| Component | Detail |
| --------- | ------ |
| Container | tar archive |
| Compression | Zstandard level 3 |
| Contents | `diff.meta` (JSON) + new/changed files |
| `diff.meta` schema | `{ "removed": ["path1", "path2", …], "base_version": "1.2.12" }` |
| Apply | Extract old `.xcs` → overlay delta files → delete removed → re-pack |

## Staging and commit model — transaction flow

```
  ┌─────────────────────────────────────────────────────┐
  │  Database::begin_transaction()                      │
  │  1. let staging_state = self.state.lock().clone()   │
  │  2. let txn_log = PackageTransaction::new()         │
  │  3. Return DbTransaction { staging_state, txn_log } │
  └──────────────────────┬──────────────────────────────┘
                         │
                         ▼
  ┌──────────────────────────────────────────────────────────────────┐
  │  Command execution:                                              │
  │  Each operation mutates staging_state and appends to txn_log:    │
  │                                                                  │
  │  Install:                                                        │
  │   1. Download .xcs → var/cache/mcx/<pkg>-<ver>.xcs               │
  │   2. Extract → var/tmp/mcx/stage/<pkg>/                          │
  │   3. Copy → active root (<--root>/usr/lib/... etc.)              │
  │   4. staging_state.installed.insert(pkg, meta)                   │
  │   5. txn_log.record_install(pkg, version, files)                 │
  │                                                                  │
  │  Remove:                                                         │
  │   1. deep_purge_analysis() → orphan set                          │
  │   2. Delete files listed in meta.files                           │
  │   3. scour_system_residue() → etc, lib, tmp, cache               │
  │   4. Clean dangling symlinks                                     │
  │   5. staging_state.installed.remove(pkg)                         │
  │   6. txn_log.record_remove(pkg)                                  │
  └──────────────────────┬───────────────────────────────────────────┘
                         │
                         ▼
  ┌─────────────────────────────────────────────────────────────┐
  │  DbTransaction::commit()                                    │
  │  1. Write txn_log to var/lib/mcx/transactions/<id>.json     │
  │  2. Serialise staging_state to JSON string                  │
  │  3. Atomic write: write to .tmp, then rename to local.json  │
  │  4. Swap state: *self.state.lock() = staging_state          │
  └─────────────────────────────────────────────────────────────┘
```

The `.tmp` → `local.json` rename is atomic on Linux (same filesystem, `rename()` syscall). A crash during step 2 or 3 leaves the previous `local.json` intact. The transaction log is written before the state file, enabling crash recovery by replaying `transactions/`.

## Transaction log format

Each transaction is serialised to `var/lib/mcx/transactions/<unix_timestamp>-<uuid>.json`:

```json
{
  "id": "1719000000-abc123",
  "timestamp": 1719000000,
  "operations": [
    {
      "type": "install",
      "package": "zlib",
      "version": "1.2.13",
      "files_affected": 42
    }
  ]
}
```

</details>

<details><summary id="feature-subsystems">Feature subsystems</summary>

## Atomic package rollback

Generation-based rollback operates entirely at the filesystem level with no database overhead:

```
var/lib/mcx/
├── active/<pkg> → ../generations/<pkg>/<N>/      # Symlink to current gen
└── generations/<pkg>/
    ├── 1/     # Snapshot N-1
    ├── 2/     # Snapshot N (current)
    └── …
```

On install/upgrade, the current file tree is hard-linked into a new generation directory before modification. The `active/<pkg>` symlink is atomically updated to point to the latest generation.

Note: rollback to a prior generation is a single symlink swap — O(1), no data copy, no database write.

| Function | Module | Signature |
| -------- | ------ | --------- |
| `enable_atomic_rollback` | `core::rollback` | `(pkg: &str, source_dir: &Path) -> Result<GenerationId>` |
| `rollback_to_generation` | `core::rollback` | `(pkg: &str, gen: GenerationId) -> Result<()>` |
| `list_generations` | `core::rollback` | `(pkg: &str) -> Result<Vec<GenerationId>>` |
| `current_generation` | `core::rollback` | `(pkg: &str) -> Result<Option<GenerationId>>` |
| `prune_generations` | `core::rollback` | `(pkg: &str, keep: usize) -> Result<usize>` |

## Content-addressable library store

Deduplicates shared libraries across package boundaries:

1. Scan `root/usr/lib/` in the staging tree for `.so` / `.so.*` files.
2. Compute SHA-256 of each file content.
3. Store first unique copy in `var/lib/mcx/cas/<hex2>/<sha256>`.
4. Replace all subsequent identical files with hard links to `cas/` path.
5. `cas_stats()` reports unique count and bytes saved.

| Function | Module | Signature |
| -------- | ------ | --------- |
| `deduplicate_libraries` | `core::cas` | `(pkg: &str, root: &Path) -> Result<CasSummary>` |
| `cas_stats` | `core::cas` | `(root: &Path) -> Result<CasStats>` |

## Delta upgrades

`DeltaEngine` applies `.xcd` delta archives:

1. Extract old `.xcs` to a temp directory.
2. Extract `.xcd` delta archive.
3. Parse `diff.meta`: `{ "removed": [...], "base_version": "..." }`.
4. Overlay new/changed files onto the old tree.
5. Delete files listed in `diff.meta.removed`.
6. Re-pack the result as a new `.xcs` (Zstd level 3).

| Function | Module | Signature |
| -------- | ------ | --------- |
| `DeltaEngine::apply_delta` | `core::delta` | `(&self, old_xcs: &Path, delta: &Path, output: &Path) -> Result<()>` |

## Process snapshot / checkpoint

Captures runtime process memory for a given package:

1. Resolve `/proc/<pid>/mem` — if accessible, dump full virtual memory.
2. Fallback to `/proc/<pid>/maps` — address-space layout only.
3. Zstd-compress the dump to `var/lib/mcx/snapshots/<pkg>/snap-<timestamp>.mem`.

| Function | Module | Signature |
| -------- | ------ | --------- |
| `checkpoint_process` | `core::snapshot` | `(pkg: &str, pid: u32) -> Result<PathBuf>` |
| `list_snapshots` | `core::snapshot` | `(pkg: &str) -> Result<Vec<PathBuf>>` |
| `restore_snapshot` | `core::snapshot` | `(snapshot: &Path, target_pid: u32) -> Result<()>` |
| `remove_snapshots` | `core::snapshot` | `(pkg: &str) -> Result<()>` |

## P2P swarm distribution

Peer-to-peer package distribution using IPFS/IPLD content hashes:

- `register_swarm_hash(pkg_ver, hash)` — persist `<pkg>.json` in `var/lib/mcx/swarm/`.
- `get_swarm_hash(pkg)` — retrieve the content hash.
- Peers tracked in `peers.json`: `{ address, peer_id, last_seen, advertised_hashes }`.

| Function | Module | Signature |
| -------- | ------ | --------- |
| `register_swarm_hash` | `core::swarm` | `(pkg: &str, version: &str, hash: &str) -> Result<()>` |
| `get_swarm_hash` | `core::swarm` | `(pkg: &str) -> Result<Option<String>>` |
| `register_swarm_peer` | `core::swarm` | `(peer: SwarmPeer) -> Result<()>` |
| `list_swarm_peers` | `core::swarm` | `() -> Result<Vec<SwarmPeer>>` |
| `remove_swarm_entry` | `core::swarm` | `(pkg: &str) -> Result<()>` |

## Streaming mounts

`generate_stream_mount_script(pkg, version, url)` writes an executable shell script to `var/lib/mcx/stream/<pkg>.sh`:

```sh
#!/bin/sh
URL="https://packages.cudane.org/stream/<pkg>.squashfs"
MOUNT="/mnt/<pkg>"
CACHE="/var/cache/mcx/stream"
mkdir -p "$MOUNT" "$CACHE"
squashfuse "$URL" "$MOUNT" -o ro,allow_other,cache=cache_dir="$CACHE"
```

| Function | Module | Signature |
| -------- | ------ | --------- |
| `generate_stream_mount_script` | `core::stream` | `(pkg: &str, version: &str, url: &str) -> Result<PathBuf>` |
| `remove_stream_script` | `core::stream` | `(pkg: &str) -> Result<()>` |
| `list_stream_scripts` | `core::stream` | `(&self) -> Result<Vec<PathBuf>>` |

## Isolated overlayfs

Per-package overlayfs isolation via three-layer mount:

| Function | Module | Signature |
| -------- | ------ | --------- |
| `create_isolated_overlay` | `core::overlay` | `(pkg: &str, lower_root: &Path) -> Result<PathBuf>` |
| `remove_isolated_overlay` | `core::overlay` | `(pkg: &str) -> Result<()>` |
| `list_overlays` | `core::overlay` | `(&self) -> Result<Vec<PathBuf>>` |

Generated helper script `mount-overlay.sh` at `~/.mcx/overlays/<pkg>/`:

1. `mount -t overlay overlay -o lowerdir=<root>,upperdir=<upper>,workdir=<work> <merged>`
2. Bind-mount `<merged>` over the target path.

## Resource control via cgroups

cgroup v2 resource enforcement:

| Function | Module | Signature |
| -------- | ------ | --------- |
| `enforce_resource_limits` | `core::cgroup` | `(pkg: &str, max_memory_mb: u64, max_cpu_percent: u8) -> Result<()>` |
| `enforce_memory_limit` | `core::cgroup` | `(pkg: &str, max_memory_mb: u64) -> Result<()>` |
| `enforce_cpu_limit` | `core::cgroup` | `(pkg: &str, max_cpu_percent: u8) -> Result<()>` |
| `remove_resource_limits` | `core::cgroup` | `(pkg: &str) -> Result<()>` |
| `is_cgroup_v2_available` | `core::cgroup` | `(&self) -> bool` |

Implementation writes to `/sys/fs/cgroup/mcx/<pkg>/`:
- `memory.max` — bytes
- `cpu.max` — `<quota> 100000`

Package names are sanitised (non-alphanumeric → `_`) for cgroup path safety.

## Self-update

`SelfUpdateManager` (in `core::update.rs`) checks GitHub Releases for a newer binary, downloads it, verifies the checksum, and replaces the running executable:

| Function | Module | Signature |
| -------- | ------ | --------- |
| `SelfUpdateManager::check` | `core::update` | `(&self) -> Result<Option<Release>>` |
| `SelfUpdateManager::update` | `core::update` | `(&self, release: &Release) -> Result<()>` |

Wired into CLI as `mcx --self-update`.

## Workspace management

`WorkspaceManager` (in `core::workspace.rs`) coordinates multi-package operations:

- Parallel builds across workspace members.
- Shared dependency resolution to avoid redundant downloads.
- Aggregated output and error reporting.

| Function | Module | Signature |
| -------- | ------ | --------- |
| `WorkspaceManager::execute_all` | `core::workspace` | `(&self, pkgs: &[&str], cmd: &str) -> Result<()>` |
| `WorkspaceManager::resolve_shared` | `core::workspace` | `(&self, pkgs: &[&str]) -> Result<SharedDeps>` |

## Vendor (offline mirror)

`VendorManager` (in `core::vendor.rs`) downloads complete dependency trees for air-gapped environments:

| Function | Module | Signature |
| -------- | ------ | --------- |
| `VendorManager::vendor` | `core::vendor` | `(&self, pkg: &str) -> Result<()>` |
| `VendorManager::install_from_vendor` | `core::vendor` | `(&self, pkg: &str) -> Result<()>` |

Recursive dependency resolution, download, and caching into a vendored directory structure. When the vendor directory is present, `mcx -i` can operate entirely offline.

## Completion engine

`CompletionEngine` (in `core::completion.rs`) generates shell-completion scripts:

| Function | Module | Signature |
| -------- | ------ | --------- |
| `CompletionEngine::generate` | `core::completion` | `(&self, shell: Shell) -> Result<String>` |

Supports Bash, Zsh, and Fish. Generates completions for all commands, aliases, and flags. Output is written to the appropriate system completions directory or stdout.

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

2. **JSON repository registry** (`core/repo.rs`) — `RepositoryManager` manages a list of repository descriptors (`name`, `url`, optional `checksum`) persisted to `etc/mcx/repo.json`. This is the runtime registry used by `--repo-add`/`--repo-remove`/`--repo-list` CLI commands.

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

### `[engine]`

| Key | Default | Values | Effect |
| --- | ------- | ------ | ------ |
| `thread_pool_mode` | `auto` | `auto`, `max`, `half`, `quad` | Sets thread pool size relative to CPU count. `auto=cpus`, `max=cpus*2`, `half=cpus/2`, `quad=cpus*4`. |
| `max_concurrent_downloads` | `8` | integer | Caps parallel HTTP downloads. Clamped to `min(cpus, value)`. |
| `zstd_level` | `3` | 1–19 | Compression level for `.xcs` package archives. Higher = smaller but slower. |

### `[network]`

| Key | Default | Values | Effect |
| --- | ------- | ------ | ------ |
| `fallback_repos` | `enabled` | `enabled`, `disabled` | When enabled, if primary repo is unreachable, MCX falls back to secondary repos. |
| `latency_threshold_ms` | `200` | integer (ms) | If network latency exceeds this threshold, MCX adjusts concurrency downward. |
| `bandwidth_threshold_kbps` | `5000` | integer (kbps) | If measured bandwidth drops below this, MCX switches to serial downloads. |

### `[security]`

| Key | Default | Values | Effect |
| --- | ------- | ------ | ------ |
| `verify_checksums` | `true` | `true`, `false` | When enabled, every downloaded package is verified against its SHA-256 checksum before extraction. |
| `allow_unverified` | `false` | `true`, `false` | When true, packages without checksums are still installed with a warning. Affects `fix-deps` and `verify` behaviour. |

### `[cache]`

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

```shell
# Open repo.ini in the built-in text editor
mcx -C
```

Or edit directly:

```shell
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

The CLI commands `--repo-add`/`--repo-remove`/`--repo-list` operate on `etc/mcx/repo.json`:

```shell
# Add a repository
mcx --repo-add my-repo https://my-packages.example.com/mcx

# Add a repository with a checksum
mcx --repo-add my-repo https://my-packages.example.com/mcx --checksum sha256:abc123...

# List configured repositories
mcx --repo-list

# Remove a repository
mcx --repo-remove my-repo
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

| Profile | Command | Flags | Use case |
| ------- | ------- | ----- | -------- |
| Debug | `cargo build` | — | Development iteration, fast compile |
| Release | `cargo build --release` | `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true` | Production binary, minimised size |
| Check | `cargo check` | — | Compile-only verification, no artifacts |
| Release with debug | `cargo build --profile release` | same as Release + debug symbols preserved | Profiling with `perf`, flamegraph |

```shell
# Compile-only verification (fastest)
cargo check

# Debug build
cargo build

# Release build (optimised for size)
cargo build --release
```

## Testing

```shell
# Run all tests (unit + integration)
cargo test

# Run with stdout/stderr visible
cargo test -- --nocapture

# Run a specific test by name
cargo test -- test_install_package

# Run integration tests only
cargo test --test integration

# Run with all features and release mode
cargo test --release --all-features
```

Integration tests are located in `tests/integration.rs`. They exercise full command pipelines against a temporary directory root, verifying ledger state transitions, file system layout, and error paths.

## Linting and static analysis

```shell
# Clippy (lint checks)
cargo clippy -- -D warnings

# Format check
cargo fmt --check

# Format in place
cargo fmt
```

## Auditing

```shell
# Check for security advisories in dependencies
cargo audit
```

## Debugging

```shell
# Build with debug assertions enabled in release
cargo build --profile release-debug  # requires Cargo.toml profile

# Run with RUST_LOG for tracing
RUST_LOG=debug mcx -i zlib

# Run with backtrace on panic
RUST_BACKTRACE=1 mcx -i zlib

# Run under strace for syscall tracing
strace -f -o /tmp/mcx.strace ./target/release/mcx -i zlib

# Memory profiling with valgrind
valgrind --tool=massif ./target/release/mcx -i zlib
ms_print massif.out.* | less
```

## Profiling

```shell
# perf profiling (Linux)
perf record --call-graph dwarf ./target/release/mcx -i zlib
perf report

# Generate flamegraph
perf script | inferno-collapse-perf > stacks.folded
inferno-flamegraph stacks.folded > flamegraph.svg

# CPU sampling with perf stat
perf stat -e cycles,instructions,cache-misses,faults ./target/release/mcx -i zlib

# Heap profiling with dhat (requires `dhat` feature)
# Run with DHAT_VALIDATE=1 and parse dhat-heap.json
```

## Continuous integration

```yaml
# Expected CI pipeline (GitHub Actions)
steps:
  - name: Checkout
    run: git checkout ${{ github.ref }}

  - name: Build
    run: cargo build --release

  - name: Test
    run: cargo test --release

  - name: Lint
    run: cargo clippy -- -D warnings

  - name: Format
    run: cargo fmt --check

  - name: Audit
    run: cargo audit
```

## Cargo.toml release profile

```toml
[profile.release]
opt-level = "z"        # Optimise for size
lto = true              # Link-time optimisation
codegen-units = 1       # Single compilation unit for maximum optimisation
panic = "abort"         # No unwind tables
strip = true            # Strip symbols
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

- **`Version`:** **`3.0.0`**.
- **`Architecture`:** **`x86_64-unknown-linux-musl`** (**`x86_64-pc-linux-musl`**).
- **`Compression`:** **`Zstd Level 3 (.xcs)`**.

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`
