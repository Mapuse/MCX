##

```shell
███╗   ███╗  ██████╗ ██╗    ██╗
████╗ ████║██╔════╝  ╚██╗  ██╔╝
██╔████╔██║██║         ╚███╔╝
██║╚██╔╝██║██║       ██╔    ██╗
██║ ╚═╝ ██║╚██████╗ ██╔╝     ██╗
╚═╝     ╚═╝ ╚═════╝ ╚═╝      ╚═╝
```

##

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

The Package Manager of **`[Cudane]`**, built for a full lifecycle and heavy workflows, no one needs a full-featured Package Manager in the same much of needing a Package Manager that just *`Works`*, but also no one want to be restricted, so it has a full **`[Python]`** Plugins and Theming with **`[No Limits]`**, you can design a full system inside **`[MCX]`** as a plugin, or design a full **`[TUI]`** with literally **`[any]`** library, the only limit is the **`[Python]`** Language itself.

- **`[Version]`**: **`[7.0.0]`**

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`


<details><summary id="contents">Contents</summary>

## Table of Contents

- [**`[Commands]`**](#commands)
- [**`[Architecture]`**](#architecture)
  - [**`[Module graph]`**](#module-graph)
  - [**`[Module inventory]`**](#module-inventory)
  - [**`[Trait contracts]`**](#trait-contracts)
  - [**`[Execution flow]`**](#execution-flow)
  - [**`[Auto-calibration]`**](#auto-calibration)
- [**`[Code structure]`**](#code-structure)
  - [**`[Modules]`**](#modules)
  - [**`[Entry points]`**](#entry-points)
  - [**`[commands/ — CLI]`**](#commands--cli)
  - [**`[core/ — Domain logic]`**](#core--domain-logic)
  - [**`[core/config.rs — merged config.ini]`**](#coreconfigrs--merged-configini)
  - [**`[cps — Python subsystem (external crate)]`**](#cps--python-subsystem-external-crate)
  - [**`[event.rs — Event bus]`**](#eventrs--event-bus)
  - [**`[network/ — Remote operations]`**](#network--remote-operations)
  - [**`[archive/ — Artifact primitives]`**](#archive--artifact-primitives)
  - [**`[utils/ — Shared utilities]`**](#utils--shared-utilities)
- [**`[Data & persistence]`**](#data--persistence)
  - [**`[Package metadata]`**](#package-metadata)
  - [**`[On-disk layout]`**](#on-disk-layout)
  - [**`[INI-based configuration]`**](#ini-based-configuration)
  - [**`[Package format]`**](#package-format)
  - [**`[Staging and commit model]`**](#staging-and-commit-model)
  - [**`[Transaction log format]`**](#transaction-log-format)
- [**`[Feature subsystems]`**](#feature-subsystems)
  - [**`[Atomic package rollback]`**](#atomic-package-rollback)
  - [**`[Content-addressable library store]`**](#content-addressable-library-store)
  - [**`[Resource control via cgroups]`**](#resource-control-via-cgroups)
  - [**`[Self-update]`**](#self-update)
  - [**`[Workspace management]`**](#workspace-management)
  - [**`[Vendor (offline mirror)]`**](#vendor-offline-mirror)
  - [**`[Completion engine]`**](#completion-engine)
  - [**`[Network downloader]`**](#network-downloader)
  - [**`[Integrity scanner (async verify & repair)]`**](#integrity-scanner-async-verify--repair)
  - [**`[Binary index (`--binindex`)]`**](#binary-index---binindex)
  - [**`[Auto-remove (`--autoremove`)]`**](#auto-remove---autoremove)
  - [**`[Security monitor]`**](#security-monitor)
  - [**`[Component tiers]`**](#component-tiers)
- [**`[Development]`**](#development)
  - [**`[Building]`**](#building)
  - [**`[Testing]`**](#testing)
  - [**`[Linting]`**](#linting)
  - [**`[Auditing]`**](#auditing)
  - [**`[Debugging]`**](#debugging)
  - [**`[Profiling]`**](#profiling)
  - [**`[Continuous integration]`**](#continuous-integration)
- [**`[Plugin authoring & linking]`**](#plugin-authoring--linking)
- [**`[Configuration guide]`**](#configuration-guide)

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
| — | `--purge` | `full-remove` | `RemoveCommand` | `commands::remove` |
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
| — | `--autoremove` | `ar`, `autoclean` | `AutoRemoveAnalyzer` | `core::autoremove` |

### `-i` / `--install`

```
mcx -i <package>...
mcx --install <package>...
mcx in <package>...
```

Resolves the dependency graph for the target packages via `DependencySolver`, downloads missing `.xcs` archives into `var/cache/mcx/`, verifies SHA-256 checksums, extracts each package in parallel (≥4 CPUs + ≥1 GB RAM triggers `spawn_blocking` per-package), copies artifacts into both the active root and `var/lib/mcx/active/<pkg>/`, and commits the transaction to LMDB.

Installs proceed in topological dependency order. Downloads write to a sibling `<name>.xcs.part` file and are only promoted to the final archive once the full body has been received; a retried transfer sends a `Range: bytes=<resume_from>-` request so an interrupted download resumes from the last byte instead of restarting (the server responds with `206 Partial Content`, or `200`/`416` to fall back to a full re-download). If an older version of a package is already installed, `mcx install <pkg>` re-installs it to upgrade; an installed version that is already equal to or newer than the resolved target is a no-op. Every state transition is recorded in `var/lib/mcx/lifecycle.jsonl` by `LifecycleEngine`.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `packages` | `Vec<String>` positional | yes | Package names to install |
| `--root` | global `-PATH-` | no | MCX root (default `/`) |

| Flag | Type | Default | Description |
| ---- | ---- | ------- | ----------- |
| `--minimal` | `bool` | `false` | Install only required components |
| `--dev` | `bool` | `false` | Install all components including development |
| `--components` | `Option<String>` | — | Install specific components (comma-separated) |
| `--exclude` | `Option<String>` | — | Exclude specific components (comma-separated) |
| `--only` | `Option<String>` | — | Install only the specific binary/tool from the package |

Component selection flows through `resolve_and_commit()`: when `components`, `minimal`, or `dev` are set, `PackageEntity` filters the manifest's component tiers (`Required`/`Recommended`/`Optional`/`Development`) before extraction.

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

Performs a self-healing deep-purge removal. Traces the reverse dependency graph via `analysis()` to identify orphaned packages. Removes each target's active directory, all manifest-listed files, scours `etc/mcx/`, `var/lib/mcx/`, `var/tmp/mcx/`, `var/cache/mcx/` for package-keyed residue, cleans dangling symlinks, and commits the transaction.

| Input | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `packages` | `Vec<String>` positional | yes | Package names to remove |

| Flag | Type | Default | Description |
| ---- | ---- | ------- | ----------- |
| `--components` | `Option<String>` | — | Remove specific components only (comma-separated) |
| `--purge-broken` | `bool` | `false` | Purge broken files and caches |

### `--purge` / `full-remove`

```
mcx --purge <package>...
mcx full-remove <package>...
```

Unconditional removal of a package — skips the reverse-dependency/orphan analysis and purges the package, its manifest-listed files, and all associated residue without tracing dependents.

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

Without package arguments: triggers `SyncCommand`, which syncs all enabled repository indexes in parallel via `RepositoryManager::sync_all_parallel` and rebuilds the available-package index in a single transaction.

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

| Flag | Type | Default | Description |
| ---- | ---- | ------- | ----------- |
| `--components` | `Option<String>` | — | Upgrade only specific components (comma-separated) |
| `--only` | `Option<String>` | — | Upgrade only the specific binary/tool |

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

The blueprint is a JSON file describing the target system state. `mcx -b <path>` reads it, computes the diff against the current installed packages, and runs install/remove to converge. INI-style profiles (`version =` / `architecture =` / `packages =` keys) are also accepted; the format is sniffed automatically.

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
| `--cgroup` | `cg` | `CgroupController` | `core::cgroup` |
| `--repo-add` | `ra` | `RepositoryManager` | `core::repo` |
| `--repo-remove` | `rr` | `RepositoryManager` | `core::repo` |
| `--repo-list` | `rl` | `RepositoryManager` | `core::repo` |
| `--hook-plugin` | `hp` | `PluginManager` | `core::plugin` |
| `plugin` | — | `PluginManager` (pyo3 via `cps`) | `core::plugin` |
| `theme` | — | `ThemeEngine` (pyo3 via `cps`) | `cps::theme` |
| `tui` | — | `TuiEngine` (pyo3 via `cps`) | `cps::tui` |
| `--service` | `svc` | inline in `main.rs` | — |
| `--command-not-found` | `cnf` | `BinaryIndex` | `core::binindex` |
| `--binindex` | `bi` | `BinaryIndex` | `core::binindex` |
| `--autoremove` | `ar`, `autoclean` | `AutoRemoveAnalyzer` | `core::autoremove` |

### `self-update`

```
mcx --self-update
```

Iterates over every configured repository (`repo.ini`), constructs the URL `<repo-url>/system/bin/mcx`, fetches the published `<url>.sha256` digest, and stages the payload inside a root-owned `var/tmp/mcx/` directory (mode 0700, unique file name). The staged bytes are SHA-256-verified against the published digest before the executable bit is set; repositories that publish no checksum are refused. After a `--version` probe, the verified binary is promoted through a same-filesystem atomic rename onto `/system/bin/mcx`. Falls through to the next repository on failure; exits with an error if no repo succeeds.

### `--vendor`

```
mcx --vendor add <package> <source.xcs>
mcx --vendor remove <package>
mcx --vendor list
```

Manages an offline package mirror in `var/lib/mcx/vendor/` (archives named `<package>-<version>.xcs` or bare `<package>.xcs`). When installing, `mcx -i` resolves each package from the download cache first, then the vendor mirror, and only hits the network as a last resort — so fully vendored sets install with no network access.

### `--completion`

```
mcx --completion bash|zsh|fish
```

Generates shell-completion scripts for the specified shell and writes them to stdout. Supports Bash (`complete -F`), Zsh (`#compdef`), and Fish (`complete -c`) formats covering all commands, aliases, and flags.

### `--cgroup`

```
mcx --cgroup enforce <package> <max_memory_mb> <max_cpu_percent>
mcx --cgroup enforce-mem <package> <max_memory_mb>
mcx --cgroup enforce-cpu <package> <max_cpu_percent>
mcx --cgroup remove <package>
mcx --cgroup status
```

cgroup v2 resource enforcement. Writes memory and CPU quota limits to `/sys/fs/cgroup/mcx/<pkg>/memory.max` and `cpu.max`. Package names are sanitised for cgroup path safety. `status` checks whether cgroup v2 is available on the host.

### `--hook-plugin` (legacy external plugins)

```
mcx --hook-plugin list
mcx --hook-plugin info <name>
mcx --hook-plugin run <name> [hook]
mcx --hook-plugin add <source>
mcx --hook-plugin remove <name>
mcx --hook-plugin reload
mcx --hook-plugin reload-config
mcx hp list
```

Manages the legacy external Python plugin system. Plugins are single `.py` files loaded via `PluginManager` (subprocess execution with the event JSON injected as the global `MCX_EVENT`).

| Subcommand | Description |
| ---------- | ----------- |
| `list` | List all loaded plugins with name, path, and status |
| `info <name>` | Show plugin details (name, path, language) |
| `run <name> [hook]` | Execute a plugin once with an optional hook name |
| `add <source>` | Copy a plugin file into `var/lib/mcx/plugins/` and reload |
| `remove <name>` | Delete a plugin file and reload |
| `reload` | Re-scan `var/lib/mcx/plugins/` for changes |
| `reload-config` | Reload plugins from `etc/mcx/p.desc` (TOML config), skip already-loaded, create+wire new entries |

> The newer in-process Python subsystem is split into the `plugin`, `theme`, and `tui` subcommands below.

### `plugin` (Python aliases)

```
mcx plugin list
mcx plugin info <name>
mcx plugin run <alias> [args...]
mcx plugin install <path> [-n NAME] [-a ALIAS] [-A k=v] [-f]
mcx plugin remove <name> [-f]
```

The modern in-process Python plugin runner (`core::plugin::PluginManager`). The pyo3 runtime lives in the external `cps` crate (see below); Python modules are loaded in-process from the paths declared in the `[python]` section of `config.ini`; each module's hook functions are registered and invoked directly.

| Subcommand | Description |
| ---------- | ----------- |
| `list` | List all loaded Python plugins |
| `info <name>` | Show plugin details |
| `run <alias> [args...]` | Run a plugin by alias with optional extra arguments |
| `install <path> [-n NAME] [-a ALIAS] [-A k=v...] [-f]` | Install a plugin file and register aliases |
| `remove <name> [-f]` | Remove a plugin |

### `theme` (Python themes)

```
mcx theme list
mcx theme info <name>
mcx theme apply <name>
mcx theme install <path> [-n NAME] [-f]
mcx theme remove <name>
```

Theme management via `cps::theme::ThemeEngine`. Themes are Python modules that render the prompt; they load from the configured theme path (`var/lib/mcx/themes` or the `[python] theme` setting) and apply via `ThemeEngine::apply`.

Themes are also registered through a `t.desc` TOML descriptor file, loaded from the first existing, parseable file among `~/.config/mcx/t.desc`, `/etc/mcx/t.desc`, `./t.desc`, or `<cwd>/t.desc` (files are never merged; `path` values support `~` expansion). Each `[theme.<id>]` section has `name`, `path`, and optional `description`; `mcx theme list`, `mcx theme info`, and `mcx theme apply` read from this registry. `mcx theme apply` executes the file out-of-process via `python3 <path>`.

### `tui` (Python TUIs)

```
mcx tui list
mcx tui info <name>
mcx tui apply <name>
mcx tui install <path> [-n NAME] [-f]
mcx tui remove <name>
```

TUI management via `cps::tui::TuiEngine`. TUIs are full-screen Python applications launched out-of-process with `mcx tui apply` (executes the file via `python3 <path>`). TUIs can be registered in the same `t.desc` file under `[tui.<id>]` sections (`name`, `path`, optional `description`) and managed with `mcx tui list`, `mcx tui apply`, `mcx tui install`, and `mcx tui remove`. `mcx theme list` and `mcx tui list` print a `Desc:` line for entries that have a description.

### `--service`

```
mcx --service <args...>
mcx svc <args...>
```

Forwards a trailing list of arguments to the **`[Cesar]`** service manager (`/etc/cesar/services.d`). Useful for scripting service lifecycle alongside package operations.

### `--command-not-found`

```
mcx --command-not-found <command>
mcx cnf <command>
```

Consults the binary index (`core::binindex::command_not_found_handler`) to suggest which package provides the missing command. Wired into shell hooks via `assets/command-not-found.sh`.

### `--binindex`

```
mcx --binindex
mcx bi
```

Rebuilds the binary index (`var/lib/mcx/binindex.json`) mapping binaries to their owning package.

### `--autoremove`

```
mcx --autoremove           # dry-run
mcx --autoremove --apply   # actually remove
mcx ar --apply
```

Analyzes the installed set via `AutoRemoveAnalyzer` and reports orphaned packages and unnecessary libraries. Default is a dry run; `--apply` performs the removal.

## Repository management

| Command (long flag) | Aliases | Struct | Module |
| ------------------- | ------- | ------ | ------ |
| `--repo-add` | `ra` | `RepositoryManager` | `core::repo` |
| `--repo-remove` | `rr` | `RepositoryManager` | `core::repo` |
| `--repo-list` | `rl` | `RepositoryManager` | `core::repo` |
| `--repo-sync` | `rs` | `RepositoryManager` | `core::repo` |
| `--repo-enable` | `re` | `RepositoryManager` | `core::repo` |
| `--repo-disable` | `rd` | `RepositoryManager` | `core::repo` |
| `--repo-info` | `ri` | `RepositoryManager` | `core::repo` |

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

Enumerates all configured repositories from `etc/mcx/repo.ini` with name, URL, and enabled/disabled status.

### `--repo-sync`

```
mcx --repo-sync <name>
mcx rs <name>
```

Syncs a single repository by name. Downloads `index.<arch>.json` for the host architecture and updates the local database. Use this when you want to refresh a specific repo without syncing all.

### `--repo-enable`

```
mcx --repo-enable <name>
mcx re <name>
```

Enables a repository. Disabled repositories are skipped during `mcx -u` (sync) and `mcx --upgrade`.

### `--repo-disable`

```
mcx --repo-disable <name>
mcx rd <name>
```

Disables a repository. The entry remains in `repo.ini` but is skipped during sync and upgrade operations.

### `--repo-info`

```
mcx --repo-info <name>
mcx ri <name>
```

Displays detailed information about a repository: URL, enabled status, checksum, and cached index stats.

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
  ├─ core -> https://packages.cudane.org
  └─ plus -> https://mirror.internal/mcx
```

### Resolution order

When installing a package, each configured repository is queried in the order they appear in `repo.ini`. The first repository that provides the package is used.

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
┌───────────────┐       ┌──────────────────┐
│  src/main.rs  │ ────  | src/commands/    │  CLI dispatch & argument parsing
└───────────────┘       │ install, remove, │
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
                       │  ├─ cas.rs       │  Content-addressable library dedup
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
 │  sync.rs         │  │  hash.rs         │  │  UserInterface   │
 │  reqwest+rustls  │  │  verify.rs       │  └──────────────────┘
 │  reqwest+rustls  │  │  verify.rs       │
 └──────────────────┘  └──────────────────┘
```

## Multi-arch support

MCX supports building and deploying packages for both `amd64` (x86_64) and `arm64` (aarch64) architectures, as well as a `"native"` fallback.

### Architecture types

The `Architecture` enum (`src/core/arch.rs`) defines three variants:

| Variant | String value | Target triple | Matches on host |
| ------- | ------------ | ------------- | --------------- |
| `Amd64` | `"amd64"` / `"x86_64"` | `x86_64-unknown-linux-musl` | x86_64 hosts only |
| `Arm64` | `"arm64"` / `"aarch64"` | `aarch64-unknown-linux-musl` | aarch64 hosts only |
| `Native` | `"native"` | *host-dependent* | Any host (wildcard) |

### Host detection

On startup, `Architecture::host()` auto-detects the running architecture by reading `/proc/sys/kernel/arch` (Linux) and falling back to `uname -m`. The detected value is stored in the profiler's `SystemProfile.architecture` field and used throughout the engine.

### Package metadata

Each `PackageMetadata` record carries an `architecture` field (default: `"native"`). This field is:

- **Propagated from repository indexes.** When syncing repository data, packages whose architecture does not match the host are silently skipped.
- **Checked during install.** If a package specifies `"amd64"` but the host is `arm64`, the install is rejected with a clear error.
- **Displayed in query output.** `mcx -q <pkg>` shows the architecture alongside version and license.
- **Used by the dependency solver.** Only packages matching the host architecture are considered during dependency resolution.

### Repository index format

Repository indexes are architecture-specific. Each repository exposes one index per architecture at `index.<arch>.json`:

| Architecture | Index file |
| ------------ | ---------- |
| amd64 | `index.x86_64.json` |
| arm64 | `index.aarch64.json` |

When MCX syncs a repository, it automatically fetches the index matching the host architecture by appending `index.<arch>.json` to the repo base URL. For example, a repo configured with `url = https://packages.cudane.org` will fetch `https://packages.cudane.org/index.x86_64.json` on an amd64 host.

Each index entry carries an `"architecture"` field and a `"source"` URL rooted in an architecture-specific pool:

```json
{
  "pkg_name": "curl",
  "version": "8.0.0",
  "architecture": "amd64",
  "license": "MIT",
  "source": "https://packages.cudane.org/pool/x86_64/curl/curl-8.0.0.xcs",
  "checksum": { "kind": "sha256", "value": "abc…" },
  "dependencies": [],
  "files": ["usr/bin/curl", "usr/lib/libcurl.so.4"],
  "provides": [],
  "conflicts": []
}
```

The pool layout follows the pattern `pool/<arch>/<name>/<pkg>-<ver>.xcs`, keeping binaries for different architectures isolated while sharing the same repository root.

Omitting the `architecture` field or setting it to `"native"` makes the package available on any architecture.

### Cross-compilation & build pipeline

MCX supports building packages for multiple architectures in a single pipeline run via the `CUDANE_TARGETS` environment variable:

```shell
export CUDANE_TARGETS="x86_64-unknown-linux-musl,aarch64-unknown-linux-musl"
./pipeline.sh
```

For each target in `CUDANE_TARGETS`, the pipeline:

1. Creates `output/<arch>/` for built packages
2. Writes `index.<arch>.json` with arch-prefixed pool URLs
3. Sorts artifacts into `pool/<arch>/<name>/`
4. Runs validation, signing, and testing separately per architecture

The `DefaultBuilder` plugin respects the `CUDANE_TARGET` environment variable (singular, per-invocation) when compiling Rust packages. If unset, it uses the host architecture:

```shell
# Cross-compile for arm64 from an amd64 host
export CUDANE_TARGET=aarch64-unknown-linux-musl
mcx -b build_config.json
```

#### Target specifications

Each architecture is defined by a Rust target specification JSON file:

| File | Architecture | CPU | Env |
| ---- | ------------ | --- | --- |
| `x86_64-unknown-linux-musl.json` | amd64 | x86-64-v3 | musl |
| `aarch64-unknown-linux-musl.json` | arm64 | armv8-a | musl |

These files define the LLVM target, data layout, linker, and CPU features for `rustc`. The `target-family` field is required for `-Zbuild-std` compilation — without it, `libc` and other core crates will fail to find their platform-specific modules.

**`[x86_64-unknown-linux-musl.json]`** (amd64):
```json
{
  "arch": "x86_64",
  "cpu": "x86-64-v3",
  "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
  "env": "musl",
  "executables": true,
  "linker": "clang",
  "linker-flavor": "gnu-cc",
  "llvm-target": "x86_64-unknown-linux-musl",
  "max-atomic-width": 64,
  "os": "linux",
  "position-independent-executables": true,
  "crt-static-default": true,
  "crt-static-respected": true,
  "target-family": ["unix"],
  "target-pointer-width": 64,
  "vendor": "pc"
}
```

**`[aarch64-unknown-linux-musl.json]`** (arm64):
```json
{
  "arch": "aarch64",
  "cpu": "armv8-a",
  "data-layout": "e-m:e-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128",
  "env": "musl",
  "executables": true,
  "linker": "clang",
  "linker-flavor": "gnu-cc",
  "llvm-target": "aarch64-unknown-linux-musl",
  "max-atomic-width": 128,
  "os": "linux",
  "position-independent-executables": true,
  "crt-static-default": true,
  "crt-static-respected": true,
  "target-family": ["unix"],
  "target-pointer-width": 64,
  "vendor": "unknown"
}
```

To add a new architecture, create the target spec JSON and add a case entry in `pipeline.sh`.

### Builder architecture awareness

The `DefaultBuilder` in `core/plugin.rs` auto-detects the build target:

1. Checks `CUDANE_TARGET` environment variable for a target triple (e.g. `aarch64-unknown-linux-musl`)
2. Falls back to `CUDANE_RUST_TARGET` for the `cargo build --target` flag
3. If neither is set, uses the host architecture detected at runtime

This enables transparent cross-compilation from any supported host to any supported target.

### Profile declarations

In `profile.ini`, the `architecture` field accepts values parsed by the `Architecture` enum:

```ini
[profile]
version = 1.0.0
architecture = x86_64
packages = curl, openssl
```

Invalid architecture strings are caught at parse time with a descriptive error.

### Host profile probing

The profiler (`SystemProfile::probe()`) now includes an `architecture: Architecture` field alongside CPU, RAM, and OS information. This enables decision-engine heuristics that are aware of cross-architecture scenarios.

### Package entity

The `PackageEntity` struct (used for embedded `metadata.json` manifests) also carries an `architecture` field with the same semantics, defaulting to `"native"` when absent.

## Module inventory

| Module | Path | Responsibility | Public surface |
| ------ | ---- | -------------- | -------------- |
| `commands` | `src/commands/` | CLI command implementations — one file per command group. Each command struct implements `execute()` taking `EngineContext`. | `InstallCommand`, `RemoveCommand`, `SyncCommand`, `SearchCommand`, `AddLocalCommand`, `CleanCommand`, `ConfigEditorCommand`, `SystemCommand`, `BinIndexCommand`, `AutoremoveCommand`, `ServiceCommand`, `HookPluginCommand` |
| `core` | `src/core/` | Domain logic — architecture detection, persistence, solver, lifecycle, plugins, profiling, configuration, repositories, history, changelog, completion, declarative validation, self-update, vendor mirroring, workspace management, content-addressable store, cgroup control, generation-based rollback, security monitor, runtime isolation. | `Architecture` enum, config types, `Database`, `DependencySolver`, `LifecycleEngine`, `PluginManager`, `SystemProfile`, `HistoryEngine`, `RepositoryManager`, `PackageEntity`, `SelfUpdateManager`, `VendorManager`, `WorkspaceManager`, `ProfileValidator`, `CompletionEngine`, `CgroupController`, `RollbackManager`, `CasStore`, `SecurityMonitor`, `BinaryIndex`, `AutoRemoveAnalyzer` |
| `config` | `src/core/config.rs` | Owned INI-style `config.ini` / `repo.ini` configuration plus `[python]` section mapping. | `ConfigManager`, `MappedConfig`, `PythonConfig` |
| `cps` | external crate (`github.com/Mapuse/CPS`, pinned in `Cargo.toml`) | In-process Python subsystem via pyo3 — plugin runtime, theme engine, TUI launcher. | `PythonConfig`, `ThemeEngine`, `TuiEngine` |
| `network` | `src/network/` | Remote data operations — HTTP(S) download via `reqwest` + `rustls-tls`. | `Downloader` |
| `archive` | `src/archive/` | Artifact format handling — `.xcs` extraction with link-target validation, SHA-256 hashing. | `Extractor`, `HashVerifier` |
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
     b. ConfigManager::new(root) — read config.ini + repo.ini,
        auto-generate defaults if absent
     c. PluginManager::new(root) — load Python plugins from
        var/lib/mcx/plugins or p.desc config
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
  - IntegrityScanner::verify_all() — per-file integrity report
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
                                ├ ➔ config
                                ├ ➔ python
                                ├ ➔ event
                                └ ➔ utils
```

Every `commands::*` struct receives an `EngineContext` reference which gates access to all `core` subsystems. `core` depends on `network` (download during install/sync) and `archive` (extract/verify). `utils` is a leaf module used by both `commands` and `main`. `core::config` holds the merged `config.ini` (including `[general]`/`[python]`) consumed by `cps` (the external in-process pyo3 plugins/themes/TUIs crate), while `event` runs the socket-backed event bus.

## Entry points

- **`[src/main.rs]`**
  - `Cli` struct (clap `#[derive(Parser)]`) — defines `--root` global flag and the full `Commands` enum (package, repository, platform, and Python subcommands).
  - `EngineContext::new(root)` — constructs the shared environment holding `Database`, `ConfigManager`, `PluginManager`, `SystemProfile`, and lifecycle engine.
  - Match on `Commands` variant → dispatch to `command.execute(&engine)`.
  - Output via `UserInterface` methods.

- **`[src/lib.rs]`**
  - Declares modules: `commands`, `core`, `network`, `archive`, `utils`, `config`, `python`, `event`.
  - Re-exports all public types (`pub use commands::*`, `pub use core::*`, etc.) for integration tests and external consumers of the `mcx` crate.

## `commands/` — CLI-level behaviour

Every command struct implements `pub fn execute(&self, engine: &EngineContext) -> Result<()>`.

| File | Struct | Responsibility | Public API |
| ---- | ------ | -------------- | ---------- |
| `add.rs` | `AddLocalCommand` | Install local `.xcs` file | `execute()` |
| `install.rs` | `InstallCommand` | Full install/upgrade pipeline | `execute()`, `resolve_and_commit()` |
| `remove.rs` | `RemoveCommand` | Remove packages + deep-purge orphans | `execute()`, `analysis()` |
| `search.rs` | `SearchCommand` | Pattern-match available index | `execute()` |
| `sync.rs` | `SyncCommand` | Parallel repo index sync | `execute()` |
| `system.rs` | `SystemCommand` | Declarative rebuild from blueprint | `execute()`, `rebuild()` |
| `clean.rs` | `CleanCommand` | Purge cache + staging | `execute()` |
| `configuration.rs` | `ConfigEditorCommand` | TUI editor for config files | `execute()`, `open_editor()` |
| `repo.rs` | `RepoAdd`/`RepoRemove`/etc. | Repository CRUD (`--repo-*`) | `execute()` |
| `self_update.rs` | `SelfUpdateCommand` | Binary self-replacement | `execute()` |
| `vendor.rs` | `VendorCommand` | Offline vendor mirror | `execute()` |
| `completion.rs` | `CompletionCommand` | Shell completion generation | `execute()` |
| `cgroup.rs` | `CgroupCommand` | cgroup v2 resource enforcement | `execute()` |
| `hook_plugin.rs` | `HookPluginCommand` | Legacy external Python plugins | `execute()` |
| `plugin.rs` | `PluginCommand` | In-process Python plugin runner | `execute()` |
| `theme.rs` | `ThemeCommand` | Python theme management | `execute()` |
| `tui.rs` | `TuiCommand` | Python TUI management | `execute()` |
| `service.rs` | `ServiceCommand` | Forward args to Cesar service manager | `execute()` |
| `command_not_found.rs` | `CommandNotFoundCommand` | Binary-index lookup for missing commands | `execute()` |
| `binindex.rs` | `BinIndexCommand` | Rebuild binary index | `execute()` |
| `autoremove.rs` | `AutoRemoveCommand` | Orphaned-package analysis + removal | `execute()` |

## `core/` — Domain logic

| File | Exports | Role | Dependencies |
| ---- | ------- | ---- | ------------ |
| `arch.rs` | `Architecture` enum, `host_architecture()`, `package_matches_host()` | Multi-arch detection, validation, and compatibility checking. Defines `Amd64`, `Arm64`, and `Native` variants with host auto-detection via `/proc/sys/kernel/arch` or `uname -m`. | — |
| `config.rs` | `MappedConfig<'a>`, `ConfigManager`, `CalibratedParams` | Mmap INI parser with `PhantomData` lifetime tracking. `ConfigManager` embeds `config.ini` + `repo.ini`. | `memmap2` |
| `database.rs` | `Database`, `DbTransaction`, `PackageMetadata` | LMDB-backed package registry via `heed` + `bincode`. Three named databases: installed, available, virtual_provides. | `heed`, `bincode` |
| `repo.rs` | `RepositoryManager` | CRUD for `etc/mcx/repo.ini` (INI format). Synced indexes remain JSON on disk. | — |
| `manifest.rs` | `ManifestParser` | Deserialise `.xcs` package manifests. | — |
| `solver.rs` | `DependencySolver`, `ResolutionVerdict`, `UpgradePath` | Dependency graph resolution, delta-cost estimation, deadlock detection, cycle breaking. | `graph.rs` |
| `graph.rs` | `DepGraph` | DAG of package dependencies and conflicts. | — |
| `transaction.rs` | `PackageTransaction` | Transaction log for install/remove operations. | — |
| `history.rs` | `HistoryEngine` | Transaction history; rollback to ID. | `database.rs` |
| `changelog.rs` | `ChangelogManager` | Append-only changelog writer. | — |
| `completion.rs` | `CompletionEngine` | Shell-completion generation (bash/zsh/fish). | — |
| `declarative.rs` | `ProfileValidator` | Validate declarative system blueprints. | — |
| `lifecycle.rs` | `LifecycleEngine`, `PackageState`, `DependencyGraph`, `OrphanSet` | State machine: Unknown→Resolved→Staged→Installed→Active→MarkedForRemoval→Removed→Purged. Pre/post hooks, audit history. | `database.rs` |
| `package.rs` | `PackageEntity` | Unified package representation across all stages. | — |
| `plugin.rs` | `PluginSlot<T>`, `PythonPlugin`, `PluginManager`, `PluginHook`, `PluginEvent`, `PluginResult`, `PluginConfig` | Lock-free policy hot-swap via `RwLock<Arc<T>>`. Python-based external plugin system with `PluginManager`, TOML config (`p.desc`), AST-probed hook registration, stdin event delivery, and a hard execution timeout. | — |
| `constants.rs` | All centralized constants | Paths, URLs, thresholds, tool names, ELF format constants, UI widths, DB sizes, Python plugin template — every hardcoded value in one place. | `PATH_ACTIVE`, `PATH_CACHE`, `DEFAULT_NETWORK_TIMEOUT_SECS`, `PLUGIN_CONFIG_FILE`, and 100+ other constants |
| `profiler.rs` | `SystemProfile`, `DecisionEngine`, `AutoHealer`, `NetworkProber` | Host profiling, heuristic decisions, network latency probing. | — |
| `update.rs` | `SelfUpdateManager` | GitHub Releases check + binary self-replace. | `network::download` |
| `vendor.rs` | `VendorManager` | Offline mirror: recursive download + caching. | `network::download` |
| `workspace.rs` | `WorkspaceManager` | Multi-package workspace orchestration. | `solver.rs` |
| `binindex.rs` | `BinaryIndex` | Maps binaries to owning package (`binindex.json`); `command_not_found_handler()`. | — |
| `autoremove.rs` | `AutoRemoveAnalyzer`, `AutoRemoveReport`, `OrphanedPackage`, `OrphanReason` | Orphaned-package and unnecessary-library analysis (dry-run + `--apply`). | `database.rs` |
| `component.rs` | `Component`, `ComponentTier` (`Required`/`Recommended`/`Optional`/`Development`) | Component tier selection and filtering for install/upgrade. | — |
| `security.rs` | `SecurityMonitor` | Security scanning and audit. | — |
| `cgroup.rs` | `CgroupController` | cgroup v2 memory/CPU enforcement. | — |
| `integrity.rs` | `IntegrityScanner` | Async verify & repair of installed content. | `tokio` |
| `rollback.rs` | `RollbackManager` | Generation-based rollback. | `database.rs` |
| `cas.rs` | `CasStore` | Content-addressable library store. | — |
| `service.rs` | `ServiceManager` | Service lifecycle forwarding to Cesar. | — |

## `core/config.rs` — merged `config.ini` configuration

`ConfigManager` now owns the whole config surface in one INI file (`etc/mcx/config.ini`):

| Section | Exports | Role |
| ------- | ------- | ---- |
| `[general]` | `log_level`, `log_file`, `cache_dir`, `build_dir` | Global paths and logging (merged from the former `mcx.toml`). |
| `[engine]` | `thread_pool_mode`, `max_concurrent_downloads`, `zstd_level`, `io_parallelism` | Engine tuning consumed by `CalibratedParams::calibrate()`. |
| `[network]` | `fallback_repos`, `latency_threshold_ms`, `bandwidth_threshold_kbps`, `concurrent_downloads` | Network heuristics. |
| `[security]` | `verify_checksums`, `allow_unverified`, `restricted_mode`, `allowed_paths` | Security surface (merged from the former TOML schema). |
| `[cache]` | `enabled`, `limit_bytes`, `max_size_mb`, `prune_age_hours`, `ttl_hours` | Cache tuning (merged from the former TOML schema). |
| `[python]` | `enabled`, `theme`, `tui`, `plugins`, `fallback_on_error`, `venv_path`, `tui_mode` | Python subsystem config; parsed into `PythonConfig` by `ConfigManager::python()`. |

## `cps/` — Python subsystem (external crate)

The pyo3 runtime is no longer compiled into `mcx`; it lives in the external `cps` crate (`Cargo.toml` → `cps = { git = "https://github.com/Mapuse/CPS", rev = "<pinned>", default-features = false }`). Python support is opt-in: the default build excludes pyo3 and libpython entirely, and `cargo build --release --features python` links libpython dynamically. The engine is lazy — the interpreter only initialises when a plugin/theme/TUI is actually configured, and is finalised on exit. `mcx` re-exports `cps::PythonConfig` from `core::config` and drives the subsystem through it:

| Source | Exports | Role |
| ------ | ------- | ---- |
| `cps` crate | `PythonConfig` | `[python]` section parsing (plugins, theme, tui, enabled, fallback_on_error, venv_path, tui_mode). |
| `cps::plugin` | `PluginManager` (pyo3) | In-process Python plugin loading (`load_all`) and hook dispatch from `PythonConfig::plugins`. |
| `cps::theme` | `ThemeEngine` | Python theme load/render/prompt (`apply`, `register`, `unregister`, `list`). |
| `cps::tui` | `TuiEngine` | Python TUI launch (`run`). |
| `core::plugin` | `PluginSlot`, `PythonPlugin`, `PluginManager`, `PluginManifest`, `PluginHook`, `PluginEvent` | Hot-swappable isolation slot and the subprocess `--hook-plugin` runner, kept local to `mcx`. |

## `network/` — Remote operations

| File | Struct | Role | Dependencies |
| ---- | ------ | ---- | ------------ |
| `download.rs` | `Downloader` | Concurrent multi-package HTTP(S) downloader with configurable parallelism, automatic retry with exponential backoff, streaming SHA-256 verification, connect/read timeouts, and If-Range resume for interrupted transfers. Uses a semaphore-bounded worker pool. | `reqwest` + `rustls-tls` |

## `archive/` — Artifact primitives

| File | Struct | Role | Dependencies |
| ---- | ------ | ---- | ------------ |
| `extract.rs` | `Extractor` | Zstd → tar → filesystem tree decompression/unpacking. | — |
| `hash.rs` | `HashVerifier` | SHA-256 digest computation for files and streams. | — |

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
| `checksum` | `ChecksumData` | `{ type: String, value: String }` |
| `provides` | `Option<Vec<String>>` | Virtual package names provided by this package |
| `conflicts` | `Option<Vec<String>>` | Package names this package conflicts with |
| `architecture` | `String` | Target architecture (`"amd64"`, `"arm64"`, or `"native"`). Defaults to `"native"` for backward compatibility. When set to a specific arch, packages are only installed on matching hosts. `"native"` matches any host architecture. |

## On-disk layout

All paths are relative to the `--root` directory (default `/`).

```
etc/mcx/
├── config.ini          # Engine configuration (mmap-based, INI format)
├── repo.ini            # Repository definitions (INI format, CLI-managed)
└── profile.ini         # Declarative package profile (INI format)

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
│   ├── plugins/        # Python plugin modules (`.py`) loaded in-process
│   ├── binindex.json   # Binary→package index (built/queried by `--binindex`)
│   ├── history.jsonl   # Append-only transaction history (`-H`, rollback)
│   ├── lifecycle.jsonl # LifecycleEngine state machine journal (`transition` audit log)
│   ├── vendor/         # Offline mirror cache (`--vendor`)
│   └── sync/           # Cached repository indexes (`repo-sync`)
├── tmp/mcx/
│   └── stage/          # Staging area for in-flight package extractions
└── cache/mcx/          # Package cache (downloaded .xcs files)
```

Runtime sockets and generated state:

```
/etc/mcx/config.ini    # Engine + [general]/[python] merged INI config (also at ~/.config/mcx)
/etc/mcx/p.desc        # Legacy subprocess plugin descriptors (--hook-plugin)
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

## Transaction flow

```
  ┌──────────────────────────────────────────┐
  │  Database::begin_transaction()           │
  │  1. env.write_txn() → LMDB RwTxn         │
  │  2. PackageTransaction::new() → tx_log   │
  │  3. Return DbTransaction { txn, tx_log } │
  └──────────────────────┬───────────────────┘
                         │
                         ▼
  ┌──────────────────────────────────────────────────────────────────┐
  │  Command execution:                                              │
  │  Each operation reads/writes LMDB directly via the open RwTxn:   │
  │                                                                  │
  │  Install:                                                        │
  │   1. Download .xcs → var/cache/mcx/<pkg>-<ver>.xcs               │
  │   2. Extract → var/tmp/mcx/stage/<pkg>/                          │
  │   3. Copy → active root                                          │
  │   4. installed_db.put(txn, &pkg_name, &meta)                     │
  │   5. txn_log.record_install(pkg, version, files)                 │
  │                                                                  │
  │  Remove:                                                         │
  │   1. analysis() → orphan set                                     │
  │   2. Delete files listed in meta.files                           │
  │   3. scour_system_residue()                                      │
  │   4. Clean dangling symlinks                                     │
  │   5. installed_db.delete(txn, &pkg_name)                         │
  │   6. txn_log.record_remove(pkg)                                  │
  └──────────────────────┬───────────────────────────────────────────┘
                         │
                         ▼
  ┌──────────────────────────────────────┐
  │  DbTransaction::commit()             │
  │  1. txn_log.commit() → history.jsonl │
  │  2. txn.commit() → LMDB atomic flush │
  └──────────────────────────────────────┘
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

## Full upgrade lifecycle

A full upgrade (loading the entire new `.xcs` package) is more efficient and less resource-intensive than delta upgrades for the following reasons:

**Zstd + mmap throughput.** Unpacking a full package with Zstd and passing the data directly via mmap to the Content-Addressable Store (CAS) saturates the CPU cache line faster than binary merging algorithms.

**Atomic hard-linking.** Once the new package is unpacked into the CAS, the engine creates hard links to the new files and updates the OverlayFS. This is instantaneous (zero-copy) and leaves no corrupted temporary files.

**Dependency solver.** The `DependencySolver` resolves the topological order and conflict matrix before any download starts. No partial states.

**Plugin system.** Any advanced enhancement feature (such as P2P or delta) is activated via Rust trait-based plugins during build, or via external Python plugins loaded from `etc/mcx/p.desc`. The core remains lean: HTTPS download + full archive extraction + CAS dedup.

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
| `VendorManager::install` | `core::vendor` | `(&self, pkg: &str) -> Result<()>` |

Recursive dependency resolution, download, and caching into a vendored directory structure. When the vendor directory is present, `mcx -i` can operate entirely offline.

## Completion engine

`CompletionEngine` (in `core::completion.rs`) generates shell-completion scripts:

| Function | Module | Signature |
| -------- | ------ | --------- |
| `CompletionEngine::generate` | `core::completion` | `(&self, shell: Shell) -> Result<String>` |

Supports Bash, Zsh, and Fish. Generates completions for all commands, aliases, and flags. Output is written to the appropriate system completions directory or stdout.

## Network downloader (concurrent, retry, streaming, ETag)

`Downloader` (in `network::download.rs`) provides HTTP(S) package downloads with:

- **Concurrent multi-package**: configurable parallelism via a semaphore-bounded worker pool
- **Automatic retry**: configurable max retries with exponential backoff
- **Streaming integrity**: SHA-256 hashing verified mid-stream before finalising
- **ETag caching**: conditional `If-None-Match` headers skip redundant downloads; records ETag on success
- **Progress callbacks**: optional `ProgressFn` for per-chunk progress reporting

| Method | Signature |
| ------ | --------- |
| `Downloader::new` | `(parallelism: usize, max_retries: u32) -> Self` |
| `Downloader::download` | `(&self, url: &str, dest: &Path, progress: Option<ProgressFn>) -> Result<DownloadResult>` |
| `Downloader::download_many` | `(&self, items: &[DownloadItem]) -> Vec<DownloadOutcome>` |

Returns `DownloadResult` with bytes downloaded, checksum, and server ETag (used for If-Range resume).

## Repository index sync

Repository sync is handled by `RepositoryManager::sync_all_parallel` (in `core/repo.rs`): all enabled repository indexes download concurrently, each written atomically via a `.tmp` rename into place, with per-repository success/error reporting. After syncing, the available-package index is rebuilt from every cached index inside one LMDB transaction so stale entries disappear.
## Integrity scanner (verify & repair)

`IntegrityScanner` (in `core::integrity.rs`) verifies every installed package's file tree:

- **Per-file digests**: packages installed by this version record a SHA-256 digest per regular file (`file_hashes`); verification recomputes each digest and flags tampered content
- **Legacy manifests**: packages installed before per-file digests existed are checked for existence only (the archive-level checksum validates downloads, not placed content)
- **Dangling symlinks**: dangling links under the managed `usr/`, `etc/`, and `var/` trees are reported; `repair_all()` removes only links recorded in some installed package's manifest — unowned links are left untouched

| Method | Signature |
| ------ | --------- |
| `IntegrityScanner::new` | `(root: &Path, db: Arc&lt;Database&gt;) -> Self` |
| `IntegrityScanner::verify_all` | `(&self) -> IntegrityReport` |
| `IntegrityScanner::repair_all` | `(&self) -> RepairReport` |

The scanner runs in the `--verify` command path; `-f` (fix-deps) drives repair.

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

---

## External plugin system (Python-based, open)

MCX supports **external plugins** written in Python (or any language via subprocess). Plugins are standalone `.py` files that can contain ANY code — they are not restricted to specific hook functions. The only requirement is valid Python syntax.

### How it works

1. **Plugin files** live under `var/lib/mcx/plugins/` as single `.py` files.
2. **Plugin configuration** is optional via TOML at `etc/mcx/p.desc`.
3. **Loading**: Python syntax is validated via `python3 -c "compile(open(path).read(), path, 'exec')"`. If valid, the plugin is loaded.
4. **Execution**: Plugins are executed via `python3` subprocess with the event JSON available as a global variable `MCX_EVENT`.

### Plugin configuration (`etc/mcx/p.desc`)

MCX uses a TOML configuration file at `etc/mcx/p.desc` to define plugins. The plugin system is **completely open** — any Python code is accepted. The only validation is a syntax check (`python3 -c "compile(...)"`). There are no restrictions on what your plugin can do.

```toml
[plugin.1]
name = "My Plugin"
path = "/path/to/plugin1.py"
ls = "ls -la"
update = "mcx -u && mcx -U"
info = "mcx --info"
my-custom-command = "some-tool --flag && another-tool"

[plugin.2]
name = "Another Plugin"
path = "~/plugins/plugin2.py"
```

#### Config fields

| Field | Required | Description |
| ----- | -------- | ----------- |
| `name` | yes | Display name for the plugin. Does not need to match the Python file's content or filename. |
| `path` | yes | Path to the `.py` file. Supports absolute paths and `~` for home directory expansion. |
| `<alias>` | no | **Unlimited.** Any extra key is treated as an alias. The key is the alias name, the value is the shell command to execute. Supports `&&` for chaining. Any characters are allowed in the key and value: `-`, `/`, `@`, `#`, `$`, `%`, `^`, `&`, `*`, `(`, `)`, `[`, `]`, `{`, `}`, `'`, `"`, `;`, `:`, `\`, `|`, spaces, UTF-8, emoji, etc. |

#### How plugins are discovered

1. **Primary**: If `etc/mcx/p.desc` exists, MCX reads it and loads every `[plugin.*]` section. Each section becomes a plugin.
2. **Fallback**: If `p.desc` does not exist, MCX scans `var/lib/mcx/plugins/` for any `.py` files and loads them automatically (backward compatibility).

#### Alias system

Aliases are **unlimited per plugin** and have **no naming restrictions**. The key can be any string, and the value is a shell command executed via `sh -c`.

```toml
[plugin.tools]
name = "Toolbox"
path = "/opt/plugins/tools.py"

# Simple aliases
ls = "ls -la"
find = "find / -name"

# Chained commands (&&)
update-all = "mcx -u && mcx -U && mcx --fix-deps"

# Complex shell pipelines
deploy = "rsync -avz ./dist/ user@host:/app/ && ssh user@host 'systemctl restart app'"

# Commands with special characters
test = "cargo test && cargo clippy"
grep-logs = "grep -r 'ERROR' /var/log/ | head -20"
backup = "tar -czf /tmp/backup-$(date +%Y%m%d).tar.gz /etc/mcx/"

# Any characters work
weird-path = "/opt/my tool/bin/run.sh --config='path with spaces'"
json-tool = "python3 -c \"import json; print(json.dumps({'key': 'value'}))\""
```

Run any alias via:

```shell
mcx --hook-plugin run <plugin-name> <alias-name>
# Example:
mcx --hook-plugin run tools update-all
mcx --hook-plugin run tools deploy
```

### Creating a plugin from scratch

#### Step 1 — Write a Python file

Your plugin can be any valid Python code. There is no required structure, no base class, no imports you must use. The only thing MCX provides is a global variable `MCX_EVENT` containing JSON data about the current operation.

```python
# /opt/mcx/var/lib/mcx/plugins/hello.py
import json, os

# MCX_EVENT is injected as a global variable containing event JSON
event_str = os.environ.get("MCX_EVENT", "{}")
event = json.loads(event_str)
pkg = event.get("package", "unknown")
print(f"Hello from plugin! Package: {pkg}")
```

#### Step 2 — Add to config (optional)

```toml
# /etc/mcx/p.desc
[plugin.1]
name = "Hello Plugin"
path = "/opt/mcx/var/lib/mcx/plugins/hello.py"
hello = "echo 'Hello from alias!'"
multi = "echo step1 && echo step2 && echo step3"
```

Or place the `.py` file in `var/lib/mcx/plugins/` and it will be auto-discovered.

#### Step 3 — Test it

```shell
mcx --hook-plugin run hello
```

### Plugin event data

When MCX fires a hook, it passes a JSON event to your plugin. The event is available as the global variable `MCX_EVENT` (a Python dict after `json.loads()`).

#### Event fields

| Field | Type | Description |
| ----- | ---- | ----------- |
| `hook` | string | The hook name (e.g., `"pre-install"`, `"post-build"`) |
| `package` | string or null | Package name being operated on |
| `version` | string or null | Package version |
| `source` | string or null | Source URL or path |
| `build_type` | string or null | Build type (e.g., `"rust"`, `"make"`, `"custom"`) |
| `work_dir` | string or null | Working directory for the operation |
| `root_dir` | string or null | Target root directory |
| `output_path` | string or null | Output path for archives |
| `arch` | string or null | Target architecture |

#### MCX hooks

| Hook | When it fires |
| ---- | ------------- |
| `pre-install` | Before installing a package |
| `post-install` | After installing a package |
| `pre-remove` | Before removing a package |
| `post-remove` | After removing a package |
| `pre-upgrade` | Before upgrading a package |
| `post-upgrade` | After upgrading a package |
| `pre-verify` | Before integrity verification |
| `post-verify` | After integrity verification |
| `pre-fix` | Before dependency fix |
| `post-fix` | After dependency fix |

#### Reading the event in Python

```python
import json, os

event = json.loads(os.environ.get("MCX_EVENT", "{}"))

hook = event.get("hook", "")
package = event.get("package", "")
version = event.get("version", "")

if hook == "post-install":
    print(f"Installed {package} v{version}")

if hook == "pre-remove":
    print(f"About to remove {package}")
```

### Plugin output protocol

Plugins communicate results via **exit code**:

- Exit `0`: success
- Exit non-zero: failure (stderr is captured as the error message)

If your plugin prints JSON to stdout with `{"success": true, "message": "..."}`, MCX will parse it. Otherwise, stdout is treated as the message.

### Plugin aliases via CLI

```shell
mcx --hook-plugin list                     # list all loaded plugins with their aliases
mcx --hook-plugin info <name>              # show plugin details (name, path, aliases)
mcx --hook-plugin run <name> <alias>       # run a specific alias
mcx --hook-plugin add <path-to-plugin.py>  # copy a .py file into plugins dir
mcx --hook-plugin remove <name>            # delete a plugin
mcx --hook-plugin reload                   # re-scan plugins directory
mcx --hook-plugin reload-config            # reload from p.desc TOML config
```

### Plugin lifecycle

1. **Discovery**: MCX reads `etc/mcx/p.desc` (or scans `var/lib/mcx/plugins/`).
2. **Loading**: Python syntax is validated via `python3 -c "compile(open(path).read(), path, 'exec')"`. If invalid, the plugin is rejected with an error.
3. **Wiring**: Each plugin is registered for all hooks. Aliases are stored for CLI invocation.
4. **Execution**: When a hook fires, MCX runs `python3 -c "import json, sys; MCX_EVENT = json.loads('...'); exec(open('plugin.py').read())"`. The plugin has full access to the Python standard library and any installed packages.
5. **Hot-swap**: `reload` and `reload-config` allow loading new plugins without restarting MCX.

### Advanced plugin example

```python
# /opt/mcx/var/lib/mcx/plugins/advanced.py
import json, os, subprocess, datetime

event = json.loads(os.environ.get("MCX_EVENT", "{}"))
hook = event.get("hook", "")
pkg = event.get("package", "unknown")
version = event.get("version", "")

# Log all events to a file
with open("/var/log/mcx-plugins.log", "a") as f:
    f.write(f"[{datetime.datetime.now()}] {hook}: {pkg} v{version}\n")

# Custom behavior per hook
if hook == "post-install":
    # Run a custom script after every install
    subprocess.run(["/opt/scripts/post-install.sh", pkg], check=False)

elif hook == "pre-remove":
    # Backup config before removal
    config = f"/etc/{pkg}/config.conf"
    if os.path.exists(config):
        subprocess.run(["cp", config, f"/tmp/{pkg}.conf.bak"])

elif hook == "post-upgrade":
    # Notify admin after upgrade
    subprocess.run(["wall", f"Package {pkg} upgraded to v{version}"])

print(json.dumps({"success": True, "message": f"Hook {hook} executed for {pkg}"}))
```

### Architecture

The plugin system is built around three core traits for Rust-side plugins:

```
Fetcher        — fetch source artifacts from remote locations
Builder        — compile source code into deployable binaries
Packer         — compress/decompress .xcs package archives
```

Each plugin is registered as a `PluginSlot<T>` — a lock-free wrapper using `RwLock<Arc<T>>`. This enables **live hot-swap**: any reader gets an `Arc::clone()` with zero contention, and a writer can atomically replace the internal `Arc` while existing references continue operating on the old version.

</details>

<details><summary id="configuration-guide">Configuration guide</summary>

## Overview

MCX configuration is entirely file-based. Three INI files under `<root>/etc/mcx/` control every aspect of behaviour:

| File | Purpose | Reading mechanism | Writing mechanism |
| ---- | ------- | ----------------- | ----------------- |
| `config.ini` | Engine tuning (threads, network, cache, security) plus `[general]` and `[python]` | `MappedConfig` (mmap, zero-copy) | TUI editor `-C` or manual edit |
| `repo.ini` | Package repository definitions | `RepositoryManager.load_repositories()` (text parse) | CLI `--repo-add`/`--repo-remove`/`--repo-list` or manual edit |
| `profile.ini` | Declarative package manifest for drift detection | `ProfileValidator.load_profile()` (text parse) | Manual edit |

---

## Merged `config.ini`

The former `mcx.toml` TOML config has been merged into `config.ini` — one INI file now carries every setting (schema in `src/core/config.rs`). `[general]` and `[python]` are new; `[engine]`, `[network]`, `[cache]`, `[security]` gained keys from the old TOML schema (`io_parallelism`, `concurrent_downloads`, `enabled`, `max_size_mb`, `ttl_hours`, `restricted_mode`, `allowed_paths`).

```ini
[general]
log_level = info
log_file = /var/log/mcx.md
cache_dir = /var/cache/mcx
build_dir = /tmp/mcx/build

[engine]
thread_pool_mode = auto
max_concurrent_downloads = 8
zstd_level = 3
io_parallelism = 4

[network]
fallback_repos = enabled
latency_threshold_ms = 200
bandwidth_threshold_kbps = 5000
concurrent_downloads = 8

[security]
verify_checksums = true
allow_unverified = false
restricted_mode = false
allowed_paths = /system,/etc,/tmp,/var,/home

[cache]
enabled = true
limit_bytes = 5368709120
max_size_mb = 1024
prune_age_hours = 168
ttl_hours = 24

[python]
enabled = false
theme =
tui =
plugins =
fallback_on_error = true
venv_path =
tui_mode = false
```

The `[python]` section drives the in-process Python subsystem (provided by the external `cps` crate): `plugins` is a comma-separated list of `.py` module paths loaded by `PluginManager::load_all`, `theme`/`tui` select the active theme/TUI, and `enabled` gates the whole subsystem. It is parsed into `PythonConfig` via `ConfigManager::python()`.

---

## Configuring MCX from scratch

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
| `url` | yes | URL string | Base URL of the repository index. The index must be available at `<url>/index.<arch>.json`. |
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
│   │   ├── generations/   # Per-package rollback snapshots
│   │   │   └── <pkg>/
│   │   │       ├── 1/     # Generation N-1
│   │   │       └── 2/     # Generation N (current)
│   │   ├── sync/           # Synced repository index files
│   │   │   └── <repo>.json # Downloaded index (JSON array of PackageMetadata)
│   │   ├── vendor/         # Offline package mirror
│   │   └── history.jsonl   # Append-only transaction changelog
│   ├── lifecycle.jsonl     # LifecycleEngine state-machine audit log
│   ├── cache/mcx/          # Downloaded .xcs package archives
│   │   └── <pkg>-<ver>.xcs
│   └── tmp/mcx/
│       └── stage/          # In-flight extraction staging
└── /sys/fs/cgroup/mcx/     # cgroup v2 hierarchy (root only)
    └── <pkg>/
        ├── memory.max
        └── cpu.max
```

---

## Creating a package repository

A package repository is any HTTP(S) server that serves two things:

1. **`[index.<arch>.json]`** — an array of `PackageMetadata` objects describing every available package for a specific architecture (e.g., `index.x86_64.json`, `index.aarch64.json`).
2. **`[.xcs` archives**] — the actual package files, addressed by path.

### Repository directory structure (server-side)

```
<repo-root>/
├── index.x86_64.json        # Required: package index for amd64
├── index.aarch64.json       # Required: package index for arm64
└── pool/
    └── <pkg-name>/
        └── <pkg-name>-<version>.xcs   # Package archives
```

The `url` field in `repo.ini` points to `<repo-root>`.

### `index.<arch>.json` format

```json
[
  {
    "pkg_name": "zlib",
    "version": "1.3.1",
    "license": "Zlib",
    "source": "https://repo.example.com/pool/zlib/zlib-1.3.1.xcs",
    "checksum": {
      "type": "sha256",
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
      "type": "sha256",
      "value": "fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321"
    },
    "dependencies": [
      { "name": "zlib", "dep_type": "runtime" }
    ],
    "files": [],
    "provides": ["libpng16.so.16"],
    "conflicts": [...]
  }
]
```

| Field | Type | Required | Description |
| ----- | ---- | -------- | ----------- |
| `pkg_name` | string | yes | Canonical package name |
| `version` | string | yes | Semantic version |
| `license` | string | yes | SPDX identifier or custom |
| `source` | string | yes | Download URL for the `.xcs` archive |
| `checksum` | object | yes | `{ type: "sha256", value: "<hex>" }` |
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

### Generating `index.<arch>.json` automatically

```shell
# Assuming .xcs files are in pool/<pkg>/
cat << 'SCRIPT' > generate-index.sh
#!/bin/sh
ARCH=${1:-x86_64}
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
    "checksum": { "type": "sha256", "value": "$hash" },
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
./generate-index.sh x86_64 > index.x86_64.json
./generate-index.sh aarch64 > index.aarch64.json
```

### Requirements for the HTTP server

- Serve `index.<arch>.json` at `<url>/index.<arch>.json` for each supported architecture.
- Serve `.xcs` files at whatever path `source` specifies in the index.
- No special headers required; standard HTTPS with `curl`/`reqwest`-compatible responses.
- Optional: serve a checksum file for the index itself (`<url>/index.<arch>.json.sha256`) if you want `checksum` in `repo.ini` to work.

---

## Guide 3: Adding a repository

### Via CLI — `--repo-add`

```shell
mcx --repo-add <name> <url>
```

| Argument | Required | Description |
| -------- | -------- | ----------- |
| `name` | yes | Unique identifier for the repository (alphanumeric, hyphens allowed) |
| `url` | yes | Base URL of the repository (must serve `index.<arch>.json` at this path) |

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
  ├─ core -> https://packages.example.org
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
2. On next `mcx -u` (sync), `RepositoryManager.sync_all_parallel()` downloads `<url>/index.<arch>.json` to `var/lib/mcx/sync/<name>.json`.
3. The downloaded index is loaded into the `available` LMDB database via `DbTransaction.update_repository_index()`.
4. Packages from the new repo now appear in `mcx -s` (search) and are available for `mcx -i` (install).

### Sync flow in detail

```
mcx -u (no package args)
  └─ SyncCommand::execute()
       ├─ RepositoryManager::load_repositories()  ← reads repo.ini sections
       ├─ For each enabled repo (parallel):
       │    ├─ Downloader::package(url/index.<arch>.json, sync/<name>.json)
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

## API

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
| Debug | `cargo +nightly -Zjson-target-spec -Zbuild-std build --target x86_64-unknown-linux-musl.json` | — | Development iteration, fast compile |
| Release | `cargo +nightly -Zjson-target-spec -Zbuild-std build --release --target x86_64-unknown-linux-musl.json` | `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true` | Production binary, minimised size |
| Check | `cargo check` | — | Compile-only verification, no artifacts |
| Release with debug | `cargo +nightly -Zjson-target-spec -Zbuild-std build --profile release --target x86_64-unknown-linux-musl.json` | same as Release + debug symbols preserved | Profiling with `perf`, flamegraph |

```shell
# Compile-only verification (fastest)
cargo  +nightly -Zjson-target-spec -Zbuild-std check --target x86_64-unknown-linux-musl.json

# Debug build
cargo +nightly -Zjson-target-spec -Zbuild-std build --target x86_64-unknown-linux-musl.json

# Release build (optimised for size)
cargo +nightly -Zjson-target-spec -Zbuild-std build --release --target x86_64-unknown-linux-musl.json
```

### Feature flags

| Feature | Default | Enables |
| ------- | ------- | ------- |
| `python` | off | cps Python subsystem: plugins, themes and TUIs through an embedded interpreter |

The default build is fully native and thin — no `pyo3`, no `libpython` linked. The `cps` engine is lazy: even in a `python` build the interpreter only initialises when a plugin/theme/TUI is actually configured, and is finalised on exit.

```shell
# Thin default build (no Python)
cargo build --release

# With Python subsystem
cargo build --release --features python

# Compile-only verification of the opt-in path
cargo check --features python
```

## Installation

All build systems auto-detect `x86_64`/`aarch64` and select the correct musl target. Cross-compilation files are in `env.mk`, `toolchain.cmake`, and `cross.txt` (generated via `scripts/crossgen.sh`).

### Cargo (direct)

```shell
cargo build --release
# Binary: target/release/mcx
# Install:
install -Dm755 target/release/mcx /system/bin/mcx
```

### Make

```shell
make build                    # auto-detects arch, builds for host
make install                  # installs to /system/bin/mcx
make install DESTDIR=/mnt     # staged install
```

### Meson

```shell
./scripts/crossgen.sh                              # generate cross file for host arch
meson setup builddir --cross-file /path/to/cross.txt --prefix=/system
meson compile -C builddir
meson install -C builddir
```

### Ninja

```shell
ninja -f build.ninja                       # build
DESTDIR=/mnt ninja -f build.ninja install  # staged install
```

### CMake

```shell
cmake -B build -DCMAKE_TOOLCHAIN_FILE=toolchain.cmake -DCMAKE_INSTALL_PREFIX=/system
cmake --build build
cmake --install build
```

## Testing

```shell
# Run all tests (unit + integration)
cargo +nightly -Zjson-target-spec -Zbuild-std test --target x86_64-unknown-linux-musl.json

# Run with stdout/stderr visible
cargo +nightly -Zjson-target-spec -Zbuild-std test -- -nocapture --target x86_64-unknown-linux-musl.json

# Run a specific test by name
cargo +nightly -Zjson-target-spec -Zbuild-std test -- test_install_package --target x86_64-unknown-linux-musl.json

# Run integration tests only
cargo +nightly -Zjson-target-spec -Zbuild-std test --test integration --target x86_64-unknown-linux-musl.json

# Run with all features and release mode
cargo +nightly -Zjson-target-spec -Zbuild-std test --release --all-features --target x86_64-unknown-linux-musl.json
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

## Credits

**`[MCX]`** is part of the **`[Cudane]`** ecosystem.

- **`[Cudane]`** — The Distribution.
- **`[Cesar]`** — Init System (PID 1).

## License
**MIT License** ─ See [**`[LICENSE]`**](https://github.com/Mapuse/.github/blob/profile/LICENSE) for More Details.