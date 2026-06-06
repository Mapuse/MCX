# MCX

MCX is a Rust-based package manager engine for Cudane Linux. It manages package installation, removal, dependency resolution, system profile reconciliation, and repository metadata using a lightweight JSON-backed state layer and async networking.

## What MCX Is

MCX is intended to provide a neutral package lifecycle runtime for a Linux distribution by:

- resolving package dependencies with a topological solver
- downloading package archives over HTTP
- verifying archive integrity before installation
- staging and extracting payloads safely
- persisting package state and history in JSON
- reconciling a declared system profile with the installed package set

## Project Structure

Key folders and files:

- `src/main.rs` — CLI entrypoint and subcommand routing
- `src/commands/` — implementation of install, remove, search, update, upgrade, query, clean, verify, fix, config, history, and build commands
- `src/core/` — package database, dependency solver, transaction history, changelog, and declarative profile handling
- `src/archive/` — archive extraction and collision detection
- `src/network/` — HTTP downloader with sequential and ranged chunked downloads
- `src/utils/ui.rs` — terminal output helpers and progress rendering
- `Cargo.toml` — build metadata, runtime dependencies, and release optimizations

## How MCX Works

### Package State

MCX stores registry and package state under the configured root path:

- `var/lib/mcx/local.json` — installed and available package metadata
- `var/lib/mcx/history.jsonl` — transaction history journal
- `var/cache/mcx` — downloaded package archives
- `var/tmp/mcx/stage` — extraction staging area

Package metadata includes package name, version, license, source URL, checksum data, dependency list, file manifest, provides, and conflicts.

### Dependency Solving

The dependency solver:

- loads package manifests from the local database
- resolves recursive package dependencies
- supports virtual providers via `provides`
- detects cyclic dependency loops
- verifies conflict constraints before install planning
- produces a topologically ordered install plan

### Transactions and Safety

Install and remove actions are wrapped in transactions that:

- back up files before overwriting them
- record staged file paths
- track affected packages
- commit JSON state only after successful completion
- rollback automatically if a transaction is dropped without committing

This design reduces the risk of partial or inconsistent package installations.

## CLI Usage

The `mcx` binary supports a subcommand-based interface. Use `--root <path>` to change the root filesystem base (default: `/`).

### Install packages

```bash
mcx install <package>...
```

Aliases: `in`, `add`

### Add a local package file

```bash
mcx add-local <file>
```

Aliases: `local`, `package`, `xcs`

### Remove packages

```bash
mcx remove <package>...
```

Aliases: `rm`, `uninstall`, `delete`

### Search available packages

```bash
mcx search <query>
```

Aliases: `find`, `look`

### Update local manifest registry

```bash
mcx update
```

Aliases: `refresh`, `sync`

### Upgrade the installed set

```bash
mcx upgrade
```

Aliases: `up`, `dist-upgrade`

### Query package status

```bash
mcx query <package>
```

Aliases: `info`, `show`

### Clean cache and history

```bash
mcx clean
```

Aliases: `wipe`, `clear`

### Verify installation state

```bash
mcx verify
```

Aliases: `check`, `certify`

### Fix dependency graph issues

```bash
mcx fix
```

Aliases: `fix-deps`, `repair`

### Inspect configuration

```bash
mcx config
```

Aliases: `cfg`, `settings`

### History and rollback

```bash
mcx history [--rollback <transaction_id>]
```

Aliases: `log`, `record`

### Rebuild from system profile

```bash
mcx build <profile.json>
```

Aliases: `make`, `create`

## System Profile Format

MCX can reconcile the installed package set against a declarative JSON profile.

Example profile:

```json
{
  "version": "1.0",
  "architecture": "x86_64",
  "packages": ["foo", "bar", "baz"]
}
```

The `build` command installs missing packages and removes packages not declared in the profile.

## Build and Run

Build the project with Cargo:

```bash
cargo build --release
```

Run the compiled binary:

```bash
./target/release/mcx install foo
```

Use a custom root path:

```bash
./target/release/mcx --root /tmp/mcx-root install foo
```

## Notes

- The implementation is written in Rust and uses `tokio` for async operations.
- Package state is managed in JSON and persisted under the configured root.
- The downloader supports both sequential downloads and ranged chunked downloads for larger files.
- The configuration editor module provides a TUI editor implementation, though `mcx config` currently reports configuration state.
- Some CLI commands currently act as workflow scaffolding or simulated progress UI while the core install/remove logic is fully implemented.

## Contributing

To extend MCX or add new features, start with these files:

- `src/main.rs`
- `src/commands/*.rs`
- `src/core/database.rs`
- `src/core/solver.rs`
- `src/network/download.rs`
- `src/archive/extract.rs`
- `src/utils/ui.rs`

For bug reports or feature requests, open an issue in the repository.
