#

`▐▀` `-` `▀▀▀▀▀▀▀▀▌`

```shell
███╗   ███╗  ██████╗██╗    ██╗    ██████╗  █████╗  ██████╗██╗  ██╗ █████╗  ██████╗ ███████╗
████╗ ████║██╔════╝ ╚██╗  ██╔╝    ██╔══██╗██╔══██╗██╔════╝██║ ██╔╝██╔══██╗██╔════╝ ██╔════╝
██╔████╔██║██║        ╚███╔╝      ██████╔╝███████║██║     █████╔╝ ███████║██║  ███╗█████╗  
██║╚██╔╝██║██║      ██╔   ██╗     ██╔═══╝ ██╔══██║██║     ██╔═██╗ ██╔══██║██║   ██║██╔══╝  
██║ ╚═╝ ██║╚██████╗██╔╝    ██╗    ██║     ██║  ██║╚██████╗██║  ██╗██║  ██║╚██████╔╝███████╗
╚═╝     ╚═╝ ╚═════╝╚═╝     ╚═╝    ╚═╝     ╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝
                                                                                        
███╗   ███╗ █████╗ ███╗   ██╗ █████╗  ██████╗ ███████╗██████╗                           
████╗ ████║██╔══██╗████╗  ██║██╔══██╗██╔════╝ ██╔════╝██╔══██╗                          
██╔████╔██║███████║██╔██╗ ██║███████║██║  ███╗█████╗  ██████╔╝                          
██║╚██╔╝██║██╔══██║██║╚██╗██║██╔══██║██║   ██║██╔══╝  ██╔══██╗                          
██║ ╚═╝ ██║██║  ██║██║ ╚████║██║  ██║╚██████╔╝███████╗██║  ██║                          
╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝                          
```

`▀` `-` `▀▀▀▀▀▀`

<html><details><summary id="contents">Contents</summary>

- [[Commands]](#commands)
- [[Architecture]](#architecture)
  - [[High-level modules]](#high-level-modules)
  - [[Execution flow]](#execution-flow)
- [[Code structure]](#code-structure)
- [[Data & persistence]](#data--persistence)
- [[Development]](#development)
- [[Building]](#building)
- [[Credits]](#credits)
- [[License]](#license)

</details>
</html>

<details><summary id="commands">Commands</summary>

## Package Management

| Command | Aliases | Description |
| --------- | ---------- | ------------- |
| `install` | `i`, `in`, `add` | Install/Deploy packages into the target system |
| `add-local` | `a`, `local`, `package`, `xcs` | Install a local `.xcs` package file |
| `remove` | `r`, `rm`, `uninstall`, `delete` | Remove installed packages |
| `search` | `s`, `find`, `look` | Search available packages from repository indexes |
| `update` | `u`, `refresh`, `sync` | Sync repository indexes (optionally selected packages) |
| `upgrade` | `U`, `up`, `dist-upgrade` | Upgrade installed packages (optionally selected ones) |
| `query` | `q`, `info`, `show` | Show installed package metadata |
| `clean` | `c`, `wipe`, `clear` | Clear cache and temporary files |
| `verify` | `v`, `check`, `certify` | Validate package integrity |
| `fix` | `f`, `fix-deps`, `repair` | Fix dependency issues |
| `config` | `C`, `cfg`, `settings` | Manage MCX configuration |
| `history` | `H`, `log`, `record` | Show installation history (optional rollback id) |
| `build` | `b`, `make`, `create` | Rebuild system from a blueprint/config |

## Repository Management

| Command | Aliases | Description |
| --------- | ---------- | ------------- |
| `repo-add` | `ra` | Add a repository (`name`, `url`, optional `checksum`) |
| `repo-remove` | `rr` | Remove a repository by name |
| `repo-list` | `rl` | List configured repositories |

## Global flag

- `--root <PATH>`: MCX root directory (used to locate `var/lib/mcx/*`)

</details>

<details><summary id="architecture">Architecture</summary>

## High-level modules

MCX is organized into Rust modules:

- `commands/*`: CLI command implementations (e.g., `InstallCommand`, `SyncCommand`, `SystemCommand`)
- `core/*`: core engines/managers (database, dependency solving, transactions, repositories, workspace, etc.)
- `network/*`: remote operations (download + sync)
- `archive/*`: archive primitives (extract/hash/verify)
- `utils/*`: shared utilities (UI + configuration)

## Execution flow

1. `src/main.rs` parses CLI using `clap`
2. Opens the local registry/database:
   - `Database::open(root)` loads/stores ledger state in `var/lib/mcx/local.json`
3. Dispatches to a command:
   - `InstallCommand`, `RemoveCommand`, `AddLocalCommand`, `SyncCommand`, `SystemCommand`, etc.
4. Commands delegate to `core/*` for the heavy lifting:
   - dependency graph building + resolution (solver/graph)
   - manifest parsing
   - staging + safe placement
   - transactional updates to the ledger
5. State is updated by committing the `DbTransaction`.

</details>

<details><summary id="code-structure">Code structure</summary>

### Entry points

- `src/main.rs`
  - Defines `mcx` CLI (`Cli`, `Commands`)
  - Opens `Database` from `--root`
  - Executes command implementations from `commands::*`
  - Uses `utils::ui::UserInterface` for user-facing output

- `src/lib.rs`
  - Declares and re-exports modules (`core`, `network`, `archive`, `utils`, `commands`)
  - Re-exports key structs so they can be reused by tests or other crates

### `commands/*` (CLI-level behavior)

Command modules live in `src/commands/` and are composed in `src/commands/mod.rs`.

- `add.rs` — `AddLocalCommand` (install local package, then move staged artifact into active root)
- `install.rs` — `InstallCommand` (install/upgrade packages via solver + transaction)
- `remove.rs` — `RemoveCommand` (remove packages safely)
- `search.rs` — `SearchCommand` (search from available indexes)
- `sync.rs` — `SyncCommand` (sync repository indexes; repository sync can be parallelized)
- `system.rs` — `SystemCommand` (system rebuild/alignment from blueprint/config)
- `clean.rs` — `CleanCommand` (cache/temp cleanup)
- `configuration.rs` — `ConfigEditorCommand` (configuration editing)

### `core/*` (domain logic)

Core modules live in `src/core/` and are composed in `src/core/mod.rs`.

Important components include:

- `database.rs`
  - `Database` (persistent `LedgerState` + `begin_transaction()`)
  - `DbTransaction` (staging state + `commit()`)

- `repo.rs`
  - `RepositoryManager` (add/remove/list repositories and manage repository metadata)

- `manifest.rs`
  - `ManifestParser` (parse package manifests)

- `solver.rs`
  - `DependencySolver` (resolve dependencies and compute an operation plan)

- `graph.rs`
  - `DepGraph` (dependency graph representation)

- `transaction.rs`
  - `PackageTransaction` (transaction log; backup/staged file tracking)

- `history.rs`
  - `HistoryEngine` (record/inspect installation history)

- `cache.rs` / `changelog.rs` / `completion.rs` / `declarative.rs` / `workspace.rs` / `vendor.rs` / `sudo.rs`
  - supporting subsystems for caching, logging, validation, workspace operations, and privileged actions

### `network/*` (remote operations)

- `download.rs` — `Downloader` (fetch repository indexes and artifacts)
- `sync.rs` — `NetworkSyncEngine` (coordinate syncing)

### `archive/*` (artifact primitives)

- `extract.rs` — `Extractor`
- `hash.rs` — `HashVerifier`
- `verify.rs` — `ContentValidator`

### `utils/*` (shared utilities)

- `ui.rs` — `UserInterface` (status/success/error display helpers)
- `config.rs` — `ConfigManager` (read/update MCX configuration)

</details>

<details><summary id="data--persistence">Data & persistence</summary>

MCX persists state through `core::database::Database`.

## Ledger state

The ledger is represented by `LedgerState`:

- `installed: HashMap<String, PackageMetadata>`
- `available: HashMap<String, PackageMetadata>`
- `repositories: Vec<RepositoryInfo>`
- `virtual_provides: HashMap<String, String>` (virtual “provide” mapping)

## On-disk location

- `var/lib/mcx/local.json` under the `--root` directory

## Staging and commit model

The commit model is implemented using:

- `Database::begin_transaction()`
  - clones current ledger into `staging_state`
  - initializes a `PackageTransaction` log

- `DbTransaction::commit()`
  - commits the transaction log
  - serializes `staging_state` into `local.json` (truncate + write)
  - swaps the in-memory state behind the `Mutex`

This approach ensures installs/removals update the ledger in a consistent, logged manner.

</details>

<details><summary id="development">Development</summary>

## Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Check without building
cargo check
```

</details>

<details><summary id="credits">Credits</summary>

**`MCX`** is part of the **`Cudane` Linux** ecosystem, designed to work seamlessly with **`rLine`** build engine.

- **`Cudane`** - The Linux Distribution.
- **`rLine`** - Source-To-Archive Build Engine.
- **`MCX`** - Runtime Package Manager.

</details>

<details><summary id="license">License</summary>

The Unlicebse - see [[**`LICENSE`**](github.com/Cudane/MCX/LICENSE)] file for details.

</details>

`-` `▄▄▄▄▄▄▄▄▌`

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`MCX`** is the official package manager for **`Cudane`**, completely written as a **`Runtime Package Manager`** to achieve 100% compatibility with the **`rLine`** build engine.
  
`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`Version`:** **`2.7.6`**.
- **`Engine`:** **`rLine 0.2.0`**.
- **`Architecture`:** **`x86_64-unknown-linux-musl`** (**`x86_64-pc-linux-musl`**).
- **`Compression`:** **`Zstd Level 3 (.xcs)`**.

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`
