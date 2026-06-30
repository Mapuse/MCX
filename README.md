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

- [Commands]
- [Architecture]
  - [Module dependency graph]
  - [Module inventory]
  - [Trait contracts]
  - [Execution flow]
  - [Auto-calibration]
- [Code structure]
  - [Modules]
  - [Entry points]
  - [commands/ — CLI-level behaviour]
  - [core/ — Domain logic]
  - [network/ — Remote operations]
  - [archive/ — Artifact primitives]
  - [utils/ — Shared utilities]
- [Data & persistence]
  - [Ledger state — JSON schema]
  - [On-disk layout]
  - [INI-based configuration]
  - [Package format]
  - [Staging and commit model]
  - [Transaction log format]
- [Feature subsystems]
  - [Atomic package rollback]
  - [Content-addressable library store]
  - [Delta upgrades]
  - [Process snapshot / checkpoint]
  - [P2P swarm distribution]
  - [Streaming mounts]
  - [Isolated overlayfs]
  - [Resource control via cgroups]
  - [Self-update]
  - [Workspace management]
  - [Vendor (offline mirror)]
  - [Completion engine]
- [Development]
  - [Building]
  - [Testing]
  - [Linting]
  - [Auditing]
  - [Debugging]
  - [Profiling]
  - [Continuous integration]
- [Plugin authoring & linking]
- [Configuration guide]
- [Credits]
- [License]

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
| `-C` | `--config` | `cfg`, `settings` | `ConfigEditorCommand` + `--init` | `commands::configuration` / inline in `main.rs` |
| `-H` | `--history` | `log`, `record` | inline in `main.rs` | — |
| `-b` | `--build` | `make`, `create` | `SystemCommand` | `commands::system` |

### `-i` / `--install`

```
mcx -i <package>...
mcx --install <package>...
mcx in <package>...
```

Resolves the dependency graph for the target packages via `DependencySolver`, downloads missing `.xcs` archives into `var/cache/mcx/`, verifies SHA-256 checksums, extracts each package in parallel (≥4 CPUs + ≥1 GB RAM triggers `spawn_blocking` per-package), copies artifacts into both the active root and `var/lib/mcx/active/<pkg>/`, and commits the transaction to LMDB.

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

Pattern-matches `query` against the `available` database in LMDB (populated by the last `update`/sync). Results are printed to stdout via `UserInterface`.

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

Without arguments: collects all currently installed package names from LMDB, then runs `InstallCommand` over the full set.

With arguments: runs `InstallCommand` on the specified subset.

### `-q` / `--query`

```
mcx -q <package>
mcx --query <package>
mcx info <package>
```

Queries `PackageMetadata` from LMDB and displays:

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
mcx -C --init
mcx --config --init
```

Opens the full-screen TUI editor (`ConfigEditorCommand` in `commands::configuration.rs`) when invoked with no sub-flag. The editor targets `etc/mcx/config.ini`.

With `--init`, generates default configuration files without opening the editor:

```
mcx -C --init
```

Creates the following files under `<root>/etc/mcx/`:

| File | Content |
| ---- | ------- |
| `config.ini` | Engine parameters (thread pool, network, security, cache) |
| `repo.ini` | Repository definitions (main + community) |
| `profile.ini` | Declarative package profile (INI format) |

Existing files are **not** overwritten — only missing files are created. This is useful when bootstrapping a new root or restoring defaults after a wipe.

Key bindings (editor mode):

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

1. **From the current system state** — dump installed packages into a JSON blueprint:
   ```shell
   mcx -q all | awk '{print $1}' | jq -R -s '{version: "1.0", architecture: "x86_64", packages: split("\n")[:-1]}' > blueprint.json
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
   mcx -b blueprint.json
   ```
   The engine will remove packages not in the list and install missing ones.

#### Validation rules

`ProfileValidator::load_profile()` enforces:
- `version` must be non-empty
- `architecture` must be non-empty
- No duplicate package names in the array
- No empty-string package entries

If validation fails, `mcx -b` exits with an error before any packages are touched.

#### Automatic profile drift detection

After every `install` and `remove` operation, if `etc/mcx/profile.ini` exists, MCX automatically computes `compile_profile_diff()` between the declared profile and the current installed state. If drift is detected (packages to install or remove), a message is printed with the counts. This runs in the background without blocking the operation.

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
| `--stream` | `str` | `StreamManager` (zstd+tar) | `core::stream` |
| `--repo-add` | `ra` | `RepositoryManager` | `core::repo` |
| `--repo-remove` | `rr` | `RepositoryManager` | `core::repo` |
| `--repo-list` | `rl` | `RepositoryManager` | `core::repo` |

### `self-update`

```
mcx --self-update
```

Iterates over every configured repository (`repo.ini`), constructs the URL `<repo-url>/system/bin/mcx`, downloads the pre-built binary, verifies it via `--version`, and performs an atomic rename over `/system/bin/mcx`. Falls through to the next repository on failure; exits with an error if no repo succeeds.

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

Generates executable shell scripts at `var/lib/mcx/stream/<pkg>.sh` that download an `.xcs` archive via `curl` and decompress with `zstd` + `tar`, matching the native package format.

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

Adds a repository entry to `etc/mcx/repo.ini` via `RepositoryManager::add_repository()`.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `name` | `String` positional | yes | Repository identifier |
| `url` | `String` positional | yes | Repository base URL |

### `--repo-remove`

```
mcx --repo-remove <name>
mcx rr <name>
```

Removes a repository entry from `etc/mcx/repo.ini` via `RepositoryManager::remove_repository()`.

### `--repo-list`

```
mcx --repo-list
mcx rl
```

Enumerates all configured repositories from `etc/mcx/repo.ini` in `name -> url` format.

## Repository management guide

A repository is a remote source of package metadata and `.xcs` archives. MCX supports multiple named repositories.

### Adding a repository

```shell
mcx --repo-add <name> <url>
```

Example:

```shell
mcx --repo-add cudane https://packages.cudane.org
```

This writes an entry to `etc/mcx/repo.ini`:

```ini
[cudane]
url = https://packages.cudane.org
enabled = true
priority = 100
```

### Removing a repository

```shell
mcx --repo-remove <name>
```

### Listing repositories

```shell
mcx --repo-list
```

Output:

```shell
  ┌── Configured repositories ─────────────────────────
  ├─ cudane -> https://packages.cudane.org
  └─ local   -> https://mirror.internal/mcx
```

### Resolution order

When installing a package, each configured repository is queried in the order they appear in `repo.ini`. The first repository that provides the package is used. If all repositories fail, the download pipeline falls through to swarm P2P and finally `git clone`.

### Configuring without CLI

Edit `etc/mcx/repo.ini` directly with any text editor. The file is managed through CLI commands (`--repo-add`, `--repo-remove`, `--repo-list`) but can also be written manually.

## Global flags

| Flag | Type | Default | Description |
| ---- | ---- | ------- | ----------- |
| `--root` | `PathBuf` | `/` (root) or `~/.mcx/` (non-root) | MCX root directory. All state paths (`etc/mcx/`, `var/lib/mcx/`, `var/cache/mcx/`, etc.) are resolved relative to this path. Auto-detected at startup. |

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
                       │  ├─ database.rs  │  DbTransaction (LMDB)
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
                       │  ├─ stream.rs    │  zstd+tar mount scripts
                       │  ├─ overlay.rs   │  Overlayfs per-package isolation
                       │  ├─ cgroup.rs    │  cgroup v2 resource control
                       │  ├─ security.rs  │  SecurityMonitor, PluginSlot runtime isolation
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
 │  pipeline.rs     │  │  hash.rs         │  │  UserInterface   │
 │  sync.rs         │  │  verify.rs       │  └──────────────────┘
 │  reqwest+rustls  │  │  verify.rs       │
 └──────────────────┘  └──────────────────┘
```

## Module inventory

| Module | Path | Responsibility | Public surface |
| ------ | ---- | -------------- | -------------- |
| `commands` | `src/commands/` | CLI command implementations — one file per command group. Each command struct implements `execute()` taking `EngineContext`. | `InstallCommand`, `RemoveCommand`, `SyncCommand`, `SearchCommand`, `AddLocalCommand`, `CleanCommand`, `ConfigEditorCommand`, `SystemCommand` |
| `core` | `src/core/` | Domain logic — persistence, solver, lifecycle, plugins, profiling, configuration, repositories, history, delta engine, changelog, completion, declarative validation, self-update, vendor mirroring, workspace management, content-addressable store, process snapshots, P2P swarm, streaming mounts, overlayfs isolation, cgroup control, generation-based rollback, security monitor, runtime isolation. | Config types, `Database`, `DependencySolver`, `LifecycleEngine`, `PluginRegistry`, `SystemProfile`, `HistoryEngine`, `RepositoryManager`, `CacheManager`, `DeltaEngine`, `PackageEntity`, `SelfUpdateManager`, `VendorManager`, `WorkspaceManager`, `ProfileValidator`, `CompletionEngine`, `SnapshotManager`, `SwarmManager`, `StreamManager`, `OverlayManager`, `CgroupController`, `RollbackManager`, `CasManager`, `SecurityMonitor` |
| `network` | `src/network/` | Remote data operations — HTTP download via `reqwest` + `rustls-tls`, parallel index sync, download pipeline with HTTPS/P2P/git fallback. | `Downloader`, `DownloadPipeline`, `NetworkSyncEngine` |
| `archive` | `src/archive/` | Artifact format handling — `.xcs` extraction, SHA-256 hashing, content verification. | `Extractor`, `HashVerifier`, `ContentValidator` |
| `utils` | `src/utils/` | Shared infrastructure — terminal output. | `UserInterface` |
| `main` / `lib` | `src/main.rs`, `src/lib.rs` | Entry point, CLI parsing, public re-exports. | `Cli`, `Commands`, `EngineContext` |

## Trait contracts

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
      d. Database::open(root) — open LMDB environment at
         var/lib/mcx/data/; create three named databases
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
    - env.write_txn() → LMDB write transaction
    - Initialise PackageTransaction log
  2. For each package in plan:
    a. lifecycle.transition(pkg, PackageState::Staged) → hook pre_execute
    b. Download → extract → copy to active root
    c. lifecycle.transition(pkg, PackageState::Installed) → hook post_install
    d. Write to LMDB via installed_db.put() inside the RwTxn
  3. DbTransaction::commit()
    a. PackageTransaction::commit() → append to history.jsonl
    b. RwTxn::commit() → LMDB atomic write (all-or-nothing)

  ╔════════════════════╗
  ║  VERIFY / CLEANUP  ║
  ╚════════════════════╝
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

## Modules

```
  main.rs ➔ lib.rs ➔ commands ➔ core ➔ network
                                │              ├ ➔ archive
                                │              └ ➔ utils
                                └ ➔ utils
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
| `database.rs` | `Database`, `DbTransaction`, `PackageMetadata` | LMDB-backed package registry via `heed` + `bincode`. Three named databases: installed, available, virtual_provides. | `heed`, `bincode` |
| `repo.rs` | `RepositoryManager` | CRUD for `etc/mcx/repo.ini` (INI format). Synced indexes remain JSON on disk. | — |
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
| `ui.rs` | `UserInterface` | Terminal output with colour prefixes and structured formatting. | `info()`, `success()`, `error()`, `render_key_values()`, `render_list()` |

</details>

<details><summary id="data--persistence">Data & persistence</summary>

## Package metadata — LMDB schema

All package metadata is stored in an LMDB database at `var/lib/mcx/data/` via the `heed` crate. Three named databases exist:

| Database | Codec | Content |
| -------- | ----- | ------- |
| `installed` | `Database<Str, SerdeBincode<PackageMetadata>>` | Currently installed packages, keyed by package name. Mutated during install/remove transactions via `DbTransaction`. |
| `available` | `Database<Str, SerdeBincode<PackageMetadata>>` | Packages discovered from repository indexes. Cleared and rebuilt on each sync. |
| `virtual_provides` | `Database<Str, SerdeBincode<String>>` | Virtual package name → real package name mapping. Populated from repository indexes. |

LMDB provides memory-mapped, zero-copy reads and full ACID transactions with single-writer serialisation.

### `PackageMetadata` fields

| Field | Type | Description |
| ----- | ---- | ----------- |
| `pkg_name` | `String` | Canonical package name |
| `version` | `String` | Semantic version string |
| `license` | `String` | SPDX license identifier |
| `source` | `String` | URL or path of the source artifact |
| `files` | `Vec<PathBuf>` | Relative paths of installed files |
| `dependencies` | `Vec<Dependency>` | Dependency specs (`name`, `dep_type`) |
| `checksum` | `ChecksumData` | `{ kind: String, value: String }` |
| `provides` | `Option<Vec<String>>` | Virtual package names provided by this package |
| `conflicts` | `Option<Vec<String>>` | Package names this package conflicts with |

## On-disk layout

All paths are relative to the `--root` directory (default `/`).

```
etc/mcx/
├── config.ini          # Engine configuration (mmap-based, INI format)
├── repo.ini            # Repository definitions (INI format, CLI-managed)
├── profile.ini         # Declarative package profile (INI format)

var/
├── lib/mcx/
│   ├── data/           # LMDB environment directory
│   │   ├── data.mdb    # Package metadata (installed, available, virtual_provides)
│   │   └── lock.mdb    # LMDB lock file
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
| Metadata location | Stored in LMDB `installed` database (`PackageMetadata.checksum`) — the archive itself has no embedded manifest |

## Delta format — `.xcd`

| Component | Detail |
| --------- | ------ |
| Container | tar archive |
| Compression | Zstandard level 3 |
| Contents | `diff.meta` (JSON) + new/changed files |
| `diff.meta` schema | `{ "removed": ["path1", "path2", …], "base_version": "1.2.12" }` |
| Apply | Extract old `.xcs` → overlay delta files → delete removed → re-pack |

## Transaction flow

```
  ┌─────────────────────────────────────────────────────┐
  │  Database::begin_transaction()                      │
  │  1. env.write_txn() → LMDB RwTxn                   │
  │  2. PackageTransaction::new() → tx_log              │
  │  3. Return DbTransaction { txn, tx_log }            │
  └──────────────────────┬──────────────────────────────┘
                         │
                         ▼
  ┌──────────────────────────────────────────────────────────────────┐
  │  Command execution:                                              │
  │  Each operation reads/writes LMDB directly via the open RwTxn:  │
  │                                                                  │
  │  Install:                                                        │
  │   1. Download .xcs → var/cache/mcx/<pkg>-<ver>.xcs               │
  │   2. Extract → var/tmp/mcx/stage/<pkg>/                          │
  │   3. Copy → active root                                          │
  │   4. installed_db.put(txn, &pkg_name, &meta)                     │
  │   5. txn_log.record_install(pkg, version, files)                 │
  │                                                                  │
  │  Remove:                                                         │
  │   1. deep_purge_analysis() → orphan set                          │
  │   2. Delete files listed in meta.files                           │
  │   3. scour_system_residue()                                      │
  │   4. Clean dangling symlinks                                     │
  │   5. installed_db.delete(txn, &pkg_name)                          │
  │   6. txn_log.record_remove(pkg)                                  │
  └──────────────────────┬───────────────────────────────────────────┘
                         │
                         ▼
  ┌─────────────────────────────────────────────┐
  │  DbTransaction::commit()                    │
  │  1. txn_log.commit() → history.jsonl        │
  │  2. txn.commit() → LMDB atomic flush        │
  └─────────────────────────────────────────────┘
```

LMDB transactions are fully ACID. A crash during step 1 leaves the LMDB state unchanged (RwTxn is aborted on drop). The changelog write happens before the LMDB commit, enabling crash recovery by comparing the changelog against the LMDB state.

## Transaction log format

Each transaction is appended to `var/lib/mcx/history.jsonl` as a single JSON line:

```json
{"transaction_id":1719000000,"timestamp":1719000000,"action":"Installation","targets":["zlib","libpng"]}
```

The changelog (`ChangelogManager`) uses JSONL — one record per line, append-only. This is the only remaining JSON persistence in the system.

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

## Download pipeline

`DownloadPipeline` (in `network::pipeline.rs`) provides a three-stage fallback chain for every package download:

1. **HTTPS (primary)** — `Downloader::download_package()` via `reqwest` with chunked parallel download for files > 5 MB.
2. **Swarm P2P (fallback)** — on HTTPS failure, queries `SwarmManager` for the package content hash, finds peers advertising it, and downloads from a peer via HTTP.
3. **Git clone (last resort)** — if both HTTPS and P2P fail, converts the URL to a repo URL and runs `git clone --depth 1`.

Wired into `InstallCommand` as the download backend. Created with optional `SwarmManager`; when `None`, P2P stage is skipped silently.

| Function | Module | Signature |
| -------- | ------ | --------- |
| `fetch` | `network::pipeline` | `(url: &str, pkg: &str, ver: &str, dest: &Path) -> Result<()>` |

## Streaming mounts

`generate_stream_mount_script(pkg, version, url)` writes an executable shell script to `var/lib/mcx/stream/<pkg>.sh` that downloads and extracts the `.xcs` archive using `curl` + `zstd` + `tar`:

```sh
#!/bin/sh
URL="https://packages.cudane.org/stream/<pkg>-<version>.xcs"
MOUNT="/mnt/<pkg>"
CACHE="/var/cache/mcx/stream"
mkdir -p "$MOUNT" "$CACHE"
ARCHIVE="$CACHE/<pkg>-<version>.xcs"
curl -sL "$URL" -o "$ARCHIVE"
zstd -d -c "$ARCHIVE" | tar -x -C "$MOUNT"
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

## Security monitor

`SecurityMonitor` (in `core::security.rs`) tracks all active packages and provides runtime isolation via lock-free `PluginSlot<T>`:

| Function | Module | Signature |
| -------- | ------ | --------- |
| `register_package` | `core::security` | `(pkg: &str)` |
| `unregister_package` | `core::security` | `(pkg: &str)` |
| `isolate_package` | `core::security` | `(pkg: &str) -> Result<()>` |
| `is_package_isolated` | `core::security` | `(pkg: &str) -> bool` |
| `swap_isolation_policy` | `core::security` | `(policy: Arc<dyn Fn(&str) -> bool>) -> Arc<dyn Fn(&str) -> bool>` |
| `active_count` | `core::security` | `() -> usize` |

Auto-registers every package on `install`, auto-unregisters on `remove`. The isolation policy can be hot-swapped at runtime — when a package is flagged, `isolate_package()` marks it for containment.

## Self-update

`SelfUpdateManager::binary()` downloads a pre-built binary from `<repo-url>/system/bin/mcx` using the `Downloader`, verifies it via `--version`, and copies it to the destination path. If verification fails the temp file is removed and the original binary is never touched.

Escalates via `sudo` automatically when invoked as non-root. The CLI `mcx --self-update` handler iterates through all configured repositories (from `repo.ini`) and tries each one in order until a download succeeds.

| Function | Module | Signature |
| -------- | ------ | --------- |
| `SelfUpdateManager::binary` | `core::update` | `(binary_url: &str, dest: &Path) -> impl Future<Output = Result<PathBuf>>` |

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

## Overview

MCX configuration is entirely file-based. Three INI files under `<root>/etc/mcx/` control every aspect of behaviour:

| File | Purpose | Reading mechanism | Writing mechanism |
| ---- | ------- | ----------------- | ----------------- |
| `config.ini` | Engine tuning (threads, network, cache, security) | `MappedConfig` (mmap, zero-copy) | TUI editor `-C` or manual edit |
| `repo.ini` | Package repository definitions | `RepositoryManager.load_repositories()` (text parse) | CLI `--repo-add`/`--repo-remove`/`--repo-list` or manual edit |
| `profile.ini` | Declarative package manifest for drift detection | `ProfileValidator.load_profile()` (text parse) | Manual edit |

---

## Guide 1: Configuring MCX from scratch

### Step 1 — Generate defaults

```shell
mcx -C --init
```

or equivalently:

```shell
mcx --config --init
```

This creates the entire configuration directory and all three default files. Existing files are **never overwritten** — only missing files are created.

### Step 2 — Verify the directory tree

```
<root>/etc/mcx/
├── config.ini          # engine, network, security, cache sections
├── repo.ini            # main + community repositories
└── profile.ini         # empty declarative profile
```

The root is auto-detected:

| User | Root | Example |
| ---- | ---- | ------- |
| root (UID 0) | `/` | `/etc/mcx/config.ini` |
| non-root | `~/.mcx/` | `~/.mcx/etc/mcx/config.ini` |

Override with `--root`:

```shell
# Custom root
mcx -C --init --root /opt/mcx
```

### Step 3 — Understand each file

#### `config.ini` — engine parameters

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

| Section | Key | Default | Values | Effect |
| ------- | --- | ------- | ------ | ------ |
| `[engine]` | `thread_pool_mode` | `auto` | `auto`, `max`, `half`, `quad` | Thread pool = `auto=cpus`, `max=cpus*2`, `half=cpus/2`, `quad=cpus*4` |
| `[engine]` | `max_concurrent_downloads` | `8` | integer | Cap on parallel HTTP downloads, clamped to `min(cpus, val)` |
| `[engine]` | `zstd_level` | `3` | 1–19 | `.xcs` compression level |
| `[network]` | `fallback_repos` | `enabled` | `enabled`, `disabled` | Fall through to secondary repos on primary failure |
| `[network]` | `latency_threshold_ms` | `200` | integer (ms) | Concurrency drops if latency exceeds this |
| `[network]` | `bandwidth_threshold_kbps` | `5000` | integer (kbps) | Switches to serial downloads below this |
| `[security]` | `verify_checksums` | `true` | `true`, `false` | SHA-256 verification before extraction |
| `[security]` | `allow_unverified` | `false` | `true`, `false` | Install packages without checksums (with warning) |
| `[cache]` | `limit_bytes` | `5368709120` | integer (bytes) | Max size of `var/cache/mcx/` (5 GB default) |
| `[cache]` | `prune_age_hours` | `168` | integer (hours) | Cache eviction age threshold (7 days) |

#### `repo.ini` — repository definitions

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

Each `[section]` is one repository. Keys inside a section:

| Key | Required | Values | Effect |
| --- | -------- | ------ | ------ |
| `url` | yes | URL string | Base URL of the repository index. The index must be available at `<url>/index.json`. |
| `enabled` | yes | `true`, `false` | If `false`, the repo is skipped during `mcx -u` (sync). |
| `priority` | yes | integer | Lower number = higher priority. Used by `DependencySolver` when the same package version is available from multiple repos. |
| `checksum` | no | hex string | Optional SHA-256 of the index file. If set, every sync verifies the downloaded index against this hash. |

Comments (`#` or `;` to end of line) are allowed anywhere.

#### `profile.ini` — declarative package profile

```ini
[profile]
version = 1.0.0
architecture = x86_64
packages = nginx, openssl, curl
```

| Key | Required | Values | Effect |
| --- | -------- | ------ | ------ |
| `version` | yes | string | Schema version. Must be non-empty. |
| `architecture` | yes | string | Target CPU architecture. Must be non-empty. |
| `packages` | no | comma-separated list | Package names the system should have installed. No duplicates, no empty entries. |

When this file exists, MCX runs `ProfileValidator::compile_profile_diff()` automatically after every `install` and `remove` operation. If the current installed set diverges from the declared set, a drift report is printed:

```
Profile drift: 2 to install, 1 to remove
```

This is purely informational — it does not block the operation or auto-correct.

### Step 4 — Complete full state directory tree

After using MCX (installing packages, syncing repos), the full tree is:

```
<root>/
├── etc/mcx/
│   ├── config.ini         # Engine parameters (mmap)
│   ├── repo.ini           # Repository definitions (INI)
│   └── profile.ini        # Declarative profile (optional)
├── var/
│   ├── lib/mcx/
│   │   ├── data/          # LMDB environment
│   │   │   ├── data.mdb   # installed + available + virtual metadata
│   │   │   └── lock.mdb   # LMDB lock
│   │   ├── active/        # Active package dir symlinks
│   │   │   └── <pkg>/     # One directory per installed package
│   │   ├── cas/           # Content-addressable library store
│   │   │   └── <hex2>/    # First 2 hex chars of SHA-256
│   │   │       └── <sha256>  # Hard-linked unique .so
│   │   ├── deltas/        # Binary delta archives
│   │   │   └── <pkg>-<old>-<new>.xcd
│   │   ├── generations/   # Per-package rollback snapshots
│   │   │   └── <pkg>/
│   │   │       ├── 1/     # Generation N-1
│   │   │       └── 2/     # Generation N (current)
│   │   ├── snapshots/     # Process memory checkpoints
│   │   │   └── <pkg>/
│   │   │       └── snap-<timestamp>.mem
│   │   ├── stream/        # Cloud-stream mount scripts
│   │   │   └── <pkg>.sh
│   │   ├── swarm/         # P2P distribution state
│   │   │   ├── <pkg>.json  # IPFS/IPLD content hashes
│   │   │   └── peers.json  # Known swarm peers
│   │   ├── sync/           # Synced repository index files
│   │   │   └── <repo>.json # Downloaded index (JSON array of PackageMetadata)
│   │   ├── vendor/         # Offline package mirror
│   │   └── history.jsonl   # Append-only transaction changelog
│   ├── cache/mcx/          # Downloaded .xcs package archives
│   │   └── <pkg>-<ver>.xcs
│   └── tmp/mcx/
│       └── stage/          # In-flight extraction staging
├── ~/.mcx/overlays/        # Per-package overlayfs (upper/work/merged)
│   └── <pkg>/
│       ├── upper/
│       ├── work/
│       └── merged/
└── /sys/fs/cgroup/mcx/     # cgroup v2 hierarchy (root only)
    └── <pkg>/
        ├── memory.max
        └── cpu.max
```

---

## Guide 2: Creating a package repository

A package repository is any HTTP(S) server that serves two things:

1. **`index.json`** — an array of `PackageMetadata` objects describing every available package.
2. **`.xcs` archives** — the actual package files, addressed by path.

### Repository directory structure (server-side)

```
<repo-root>/
├── index.json               # Required: package index
└── pool/
    └── <pkg-name>/
        └── <pkg-name>-<version>.xcs   # Package archives
```

The `url` field in `repo.ini` points to `<repo-root>`.

### `index.json` format

```json
[
  {
    "pkg_name": "zlib",
    "version": "1.3.1",
    "license": "Zlib",
    "source": "https://repo.example.com/pool/zlib/zlib-1.3.1.xcs",
    "checksum": {
      "kind": "sha256",
      "value": "a1b2c3d4e5f67890abcdef1234567890abcdef1234567890abcdef1234567890"
    },
    "dependencies": [
      { "name": "glibc", "dep_type": "runtime" }
    ],
    "files": [],
    "provides": ["libz.so.1"],
    "conflicts": []
  },
  {
    "pkg_name": "libpng",
    "version": "1.6.40",
    "license": "libpng-2.0",
    "source": "https://repo.example.com/pool/libpng/libpng-1.6.40.xcs",
    "checksum": {
      "kind": "sha256",
      "value": "fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321"
    },
    "dependencies": [
      { "name": "zlib", "dep_type": "runtime" }
    ],
    "files": [],
    "provides": ["libpng16.so.16"],
    "conflicts": []
  }
]
```

| Field | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `pkg_name` | string | yes | Canonical package name |
| `version` | string | yes | Semantic version |
| `license` | string | yes | SPDX identifier or custom |
| `source` | string | yes | Download URL for the `.xcs` archive |
| `checksum` | object | yes | `{ kind: "sha256", value: "<hex>" }` |
| `dependencies` | array | yes | List of `{ name, dep_type }` objects. `dep_type` is typically `"runtime"`, `"build"`, or `"library"`. |
| `files` | array | yes | Populated after installation; empty in the index is fine |
| `provides` | array | no | Virtual package names this package provides (e.g., `libz.so.1`) |
| `conflicts` | array | no | Package names this package conflicts with |

### Creating a `.xcs` package archive

```shell
# Create a package directory with the files to distribute
mkdir -p my-pkg/usr/bin
cp my-binary my-pkg/usr/bin/

# Pack into .xcs (zstd-compressed tar)
cd my-pkg
zstd --compress -3 --tar -o my-pkg-1.0.0.xcs .
```

### Generating `index.json` automatically

```shell
# Assuming .xcs files are in pool/<pkg>/
cat << 'SCRIPT' > generate-index.sh
#!/bin/sh
echo "["
first=true
for xcs in pool/*/*.xcs; do
  $first || echo ","
  first=false
  pkg=$(basename "$xcs" | sed 's/-[0-9].*//')
  ver=$(basename "$xcs" | sed 's/.*-\([0-9].*\)\.xcs/\1/')
  hash=$(sha256sum "$xcs" | cut -d' ' -f1)
  cat << JSON
  {
    "pkg_name": "$pkg",
    "version": "$ver",
    "license": "Unknown",
    "source": "https://repo.example.com/pool/$pkg/$pkg-$ver.xcs",
    "checksum": { "kind": "sha256", "value": "$hash" },
    "dependencies": [],
    "files": [],
    "provides": [],
    "conflicts": []
  }
JSON
done
echo "]"
SCRIPT
chmod +x generate-index.sh
./generate-index.sh > index.json
```

### Requirements for the HTTP server

- Serve `index.json` at `<url>/index.json`.
- Serve `.xcs` files at whatever path `source` specifies in the index.
- No special headers required; standard HTTPS with `curl`/`reqwest`-compatible responses.
- Optional: serve a checksum file for the index itself (`<url>/index.json.sha256`) if you want `checksum` in `repo.ini` to work.

---

## Guide 3: Adding a repository

### Via CLI — `--repo-add`

```shell
mcx --repo-add <name> <url>
```

| Argument | Required | Description |
| -------- | -------- | ----------- |
| `name` | yes | Unique identifier for the repository (alphanumeric, hyphens allowed) |
| `url` | yes | Base URL of the repository (must serve `index.json` at this path) |

Examples:

```shell
# Add the official Cudane repository
mcx --repo-add cudane https://packages.cudane.org

# Add a custom internal mirror
mcx --repo-add internal https://mirror.internal.example.com/mcx

# Add an experimental repository
mcx --repo-add edge https://edge.packages.example.com
```

Duplicate names are rejected:

```
Error: Repository 'cudane' already exists
```

### Via CLI — `--repo-list`

```shell
mcx --repo-list
```

Output:

```
  ┌── Configured repositories ─────────────────────────
  ├─ cudane -> https://packages.cudane.org
  ├─ internal -> https://mirror.internal.example.com/mcx
  └─ edge -> https://edge.packages.example.com
```

### Via CLI — `--repo-remove`

```shell
mcx --repo-remove <name>
```

Example:

```shell
mcx --repo-remove edge
```

Non-existent names are rejected:

```
Error: Repository 'edge' not found
```

### Via manual edit — `repo.ini`

Edit `<root>/etc/mcx/repo.ini` directly:

```shell
# Using the built-in TUI editor
mcx -C
```

Or with any text editor:

```shell
$EDITOR /etc/mcx/repo.ini
```

Append a new section:

```ini
[my-repo]
url = https://my-repo.example.com/mcx
enabled = true
priority = 50
```

| Field | Required | Description |
| ----- | -------- | ----------- |
| `[name]` | yes | Section header becomes the repository name |
| `url` | yes | Repository base URL |
| `enabled` | yes | `true` to include during sync, `false` to skip |
| `priority` | yes | Lower = higher priority |
| `checksum` | no | SHA-256 of the index file for verification |

### What happens when you add a repo

1. `RepositoryManager.add_repository()` appends the entry to `repo.ini`.
2. On next `mcx -u` (sync), `RepositoryManager.sync_all_parallel()` downloads `<url>/index.json` to `var/lib/mcx/sync/<name>.json`.
3. The downloaded index is loaded into the `available` LMDB database via `DbTransaction.update_repository_index()`.
4. Packages from the new repo now appear in `mcx -s` (search) and are available for `mcx -i` (install).

### Sync flow in detail

```
mcx -u (no package args)
  └─ SyncCommand::execute()
       ├─ RepositoryManager::load_repositories()  ← reads repo.ini sections
       ├─ For each enabled repo (parallel):
       │    ├─ Downloader::download_package(url/index.json, sync/<name>.json)
       │    └─ HashVerifier::verify_integrity()    ← if checksum is set in repo.ini
       ├─ Database::begin_transaction()
       └─ For each downloaded index:
            └─ tx.update_repository_index()        ← inserts into LMDB available db
```

---

## Guide 4: Editing configuration

### Built-in TUI editor

The `-C` (`--config`) command opens a full-screen terminal editor:

```shell
mcx -C
```

Key bindings:

| Key | Action |
| --- | ------ |
| `Ctrl+X` | Exit (prompts if unsaved changes) |
| `Ctrl+O` / `Ctrl+S` | Save file |
| `Ctrl+K` | Cut current line |
| `Ctrl+U` | Paste cut buffer |
| Arrow keys | Navigate |
| `PageUp` / `PageDown` | Scroll |
| `Home` / `End` | Line start/end |
| `Backspace` / `Delete` | Character deletion |
| `Enter` | Split line |

The editor targets `etc/mcx/config.ini` by default.

### Direct file editing

Any of the three config files can be edited directly with any text editor:

```shell
# Edit engine parameters
$EDITOR /etc/mcx/config.ini

# Edit repository definitions
$EDITOR /etc/mcx/repo.ini

# Edit declarative profile
$EDITOR /etc/mcx/profile.ini
```

Changes take effect on the next MCX command. The `MappedConfig` (used for `config.ini` and `repo.ini` by `ConfigManager`) is a snapshot at startup — restart MCX to pick up changes. The `RepositoryManager` and `ProfileValidator` read their files fresh on every invocation.

### Validation rules

When editing manually, observe these rules:

| File | Rule | Consequence |
| ---- | ---- | ----------- |
| `config.ini` | Unknown sections/keys are silently ignored | No error, but the value has no effect |
| `config.ini` | Missing section → default values used | Engine falls back to baked defaults |
| `repo.ini` | Duplicate section names → last-writer-wins in `MappedConfig`, error in `RepositoryManager` | CLI `--repo-add` rejects duplicates; manual edit overwrites silently |
| `repo.ini` | Missing `url` key → section is skipped | Repo is not registered |
| `repo.ini` | Invalid `url` → sync fails at download time | Error during `mcx -u` |
| `profile.ini` | Empty `version` or `architecture` → load rejects | Drift detection is skipped |
| `profile.ini` | Duplicate packages → load rejects | Drift detection is skipped |
| All INI | Invalid INI syntax (unclosed `[section`, no `=`) → parse halts | Affected file becomes unreadable |

---

## Programmatic API

```rust
use std::path::Path;
use mcx::core::config::ConfigManager;
use mcx::core::repo::RepositoryManager;
use mcx::core::declarative::ProfileValidator;

// ── Reading config.ini (zero-copy, mmap-backed) ──
let mgr = ConfigManager::new(Path::new("/"))?;

let thread_mode = mgr.local().get("engine", "thread_pool_mode");
let max_dl = mgr.local().get_usize("engine", "max_concurrent_downloads");
let verify = mgr.local().get_bool("security", "verify_checksums");
let cache_limit = mgr.local().get_u64("cache", "limit_bytes");

// ── Reading repo.ini programmatically ──
let main_url = mgr.repo().get("main", "url");
let main_enabled = mgr.repo().get_bool("main", "enabled");

// ── CalibratedParams auto-computes thread pools ──
let params = mgr.calibrate();
println!("Thread pool: {} cores", params.thread_pool_size);
println!("Concurrent downloads: {}", params.concurrent_downloads);

// ── Managing repositories ──
let repo_mgr = RepositoryManager::new(Path::new("/"));
for repo in repo_mgr.load_repositories()? {
    println!("{} -> {}", repo.name, repo.url);
}

// ── Reading the declarative profile ──
let profile = ProfileValidator::load_profile("/etc/mcx/profile.ini")?;
println!("Target packages: {:?}", profile.packages);
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

## Linting

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
