# MCX Package Manager

---

**MCX** is a Rust-based package manager for **Cudane Linux**. It handles package installation, removal, dependency resolution, system profile reconciliation, repository synchronization, and metadata management using a JSON-backed state layer, async networking, and a transaction-safe engine. MCX also implements **nine kernel-level runtime features** that consume and act on the metadata embedded in every package archive.

---

## Table of Contents

- [Table of Contents](#table-of-contents)
- [Architecture](#architecture)
- [Package State Layer](#package-state-layer)
- [Transaction Safety](#transaction-safety)
- [CLI Usage](#cli-usage)
  - [Package Management](#package-management)
  - [Local Package Handling](#local-package-handling)
  - [Repository Management](#repository-management)
  - [System Profile Build](#system-profile-build)
  - [History and Rollback](#history-and-rollback)
  - [Configuration and Utilities](#configuration-and-utilities)
- [Kernel-Level Runtime Features](#kernel-level-runtime-features)
  - [Lazy Mount](#lazy-mount)
  - [CAS Deduplication](#cas-deduplication)
  - [Atomic Rollback](#atomic-rollback)
  - [Delta Reconstruction](#delta-reconstruction)
  - [Memory Snapshot (Checkpoint/Restore)](#memory-snapshot-checkpointrestore)
  - [Cloud-Streamable Overlay](#cloud-streamable-overlay)
  - [Isolated State Overlay](#isolated-state-overlay)
  - [P2P Swarm Distribution](#p2p-swarm-distribution)
  - [Resource Throttle and Self-Healing Telemetry](#resource-throttle-and-self-healing-telemetry)
- [Metadata Feature Flag Reference](#metadata-feature-flag-reference)
- [Dependency Solving](#dependency-solving)
- [System Profile Format](#system-profile-format)
- [Build From Source](#build-from-source)
- [Notes](#notes)
- [Contributing](#contributing)
- [Credits](#credits)

---

## Architecture

MCX is organized into four main layers:

- **`src/main.rs`** — CLI entry point and subcommand routing (clap-based parser). All commands have both short flags (`-i`, `-r`, `-s`, etc.) and long flags (`--install`, `--remove`, `--search`, etc.) plus aliases for convenience.
- **`src/commands/`** — implementation of install, remove, search, update, upgrade, query, clean, verify, fix, config, history, build, and all feature commands.
- **`src/core/`** — package database, dependency solver, transaction history, changelog, declarative profile handling, repository manager, and **FeatureEngine** implementing all nine runtime capabilities.
- **`src/archive/`** — archive extraction, collision detection, hash verification.
- **`src/network/`** — HTTP downloader with sequential and ranged chunked downloads.
- **`src/utils/`** — terminal output helpers, configuration management.

---

## Package State Layer

MCX stores all registry and package state under the configured root path:

| Path | Purpose |
| ------ | --------- |
| `var/lib/mcx/local.json` | Installed and available package metadata ledger |
| `var/lib/mcx/history.jsonl` | Transaction history journal |
| `var/cache/mcx` | Downloaded package archives |
| `var/tmp/mcx/stage` | Extraction staging area |
| `var/lib/mcx/generations/` | Atomic rollback generation snapshots |
| `var/lib/mcx/active/` | Symlinks to the current generation of each package |
| `var/lib/mcx/cas/` | Content-addressable storage for shared library dedup |
| `var/lib/mcx/snapshots/` | Memory snapshots (checkpoint data) |
| `var/lib/mcx/stream/` | Cloud-stream mount helper scripts |
| `var/lib/mcx/swarm/` | P2P peer database and swarm hash registrations |

Each package metadata record contains: `pkg_name`, `version`, `license`, `source`, `checksum`, `dependencies`, `files`, `provides`, `conflicts`, and `features`.

---

## Transaction Safety

Install and remove actions are wrapped in **atomic transactions** that:

- Back up targeted files before overwriting them
- Record all staged file paths
- Track affected packages by name
- Commit JSON state only after every file operation succeeds
- **Roll back automatically** if a transaction is dropped without committing (via `Drop` trait)

This eliminates partial or inconsistent package installations.

---

## CLI Usage

### Package Management

| Action | Short Flag | Long Flag | Aliases | Example |
| -------- | ----------- | ----------- | --------- | --------- |
| Install | `-i` | `--install` | `in`, `add` | `mcx -i firefox vim` |
| Remove | `-r` | `--remove` | `rm`, `uninstall`, `delete` | `mcx -r firefox` |
| Search | `-s` | `--search` | `find`, `look` | `mcx -s browser` |
| Query | `-q` | `--query` | `info`, `show` | `mcx -q firefox` |
| Update | `-u` | `--update` | `refresh`, `sync` | `mcx -u` |
| Upgrade | `-U` | `--upgrade` | `up`, `dist-upgrade` | `mcx -U` |
| Clean | `-c` | `--clean` | `wipe`, `clear` | `mcx -c` |
| Verify | `-v` | `--verify` | `check`, `certify` | `mcx -v` |
| Fix deps | `-f` | `--fix` | `fix-deps`, `repair` | `mcx -f` |

### Local Package Handling

```shell
# Install a local .xcs archive
mcx -a /path/to/package.xcs

# When the metadata's features list includes lazy-mount / cas-deduplication /
# atomic-rollback, the installation automatically triggers those engines.
```

### Repository Management

| Action | Long Flag | Aliases | Example |
| -------- | ----------- | --------- | --------- |
| Add | `--repo-add` | `ra` | `mcx --repo-add myrepo https://repo.example.com/index.json` |
| Remove | `--repo-remove` | `rr` | `mcx --repo-remove myrepo` |
| List | `--repo-list` | `rl` | `mcx --repo-list` |

Repositories are synchronized in parallel via `mcx -u`, which downloads each repository's index concurrently and verifies checksums.

### System Profile Build

```shell
mcx -b --config /etc/cudane/system.json
```

Rebuilds the system to match a declarative JSON profile — installs missing packages, removes undeclared ones.

### History and Rollback

```shell
# View transaction history
mcx -h

# Roll back to a specific transaction
mcx -h --rollback <transaction_id>
```

### Configuration and Utilities

```shell
# Open the built-in TUI editor (nano-like)
mcx -C
# Ctrl+O: Save  |  Ctrl+X: Exit
```

---

## Kernel-Level Runtime Features

MCX reads the `features` / `optimization_features` array from every package's `metadata.json` and activates the corresponding runtime engines. The following nine features are supported:

### Lazy Mount

**Flag:** `"lazy-mount"`

When a package's metadata includes `"lazy-mount"`, MCX generates a **dinit service script** at `/etc/dinit.d/mount-<pkgname>.dinit`. This script performs the mount on demand (when the service is requested) rather than at boot time, reducing boot pressure.

```shell
# Manual generation
mcx -L mypackage /system/mypackage/data

# Remove the service
mcx --lazy-umount mypackage
```

**Engine:** `FeatureEngine::generate_lazy_mount_service()` writes a dinit script that runs `/bin/mount <mount_point>` on service start.

### CAS Deduplication

**Flag:** `"cas-deduplication"`

Content-addressable storage eliminates redundant shared library copies across packages. MCX scans `system/lib/` for `.so` files, computes their SHA-256 hash, and stores them in `var/lib/mcx/cas/{first-2-hex}/{full-hash}`. Duplicates are replaced with hard links to the canonical copy.

```shell
# Run dedup on a staging directory
mcx -D run /path/to/staging

# View statistics
mcx -D stats
```

**Engine:** `FeatureEngine::deduplicate_libraries()`

### Atomic Rollback

**Flag:** `"atomic-rollback"`

Every package installation creates a numbered **generation** snapshot under `var/lib/mcx/generations/<pkg_name>/`. A symlink at `var/lib/mcx/active/<pkg_name>` points to the currently active generation. Rollback is a simple symlink flip — no file copying required.

```shell
# Roll back to generation 2
mcx -R mypackage 2

# List all generations
mcx --generations mypackage

# Example output:
#   Gen 1
#   Gen 2 [active]
#   Gen 3
```

**Engine:** `FeatureEngine::enable_atomic_rollback()`, `rollback_to_generation()`

### Delta Reconstruction

**Flag:** `"delta-reconstruct"`

Apply a `.xcd` micro-diff file to an old `.xcs` package to produce a new package version locally, avoiding full network downloads.

```shell
mcx -d old-package.xcs delta.xcd new-package.xcs
```

**`.xcd` format:** Zstd-compressed tar containing `diff.meta` (JSON with `pkg_name`, `from_version`, `to_version`, `removed` paths) and `files/` (new/modified file overlays).

**Engine:** `FeatureEngine::reconstruct_delta()`

### Memory Snapshot (Checkpoint/Restore)

**Flag:** `"memory-snapshot"`

CRIU-inspired technology: MCX takes a snapshot of a running process's memory via `/proc/<pid>/mem` (or falls back to `/proc/<pid>/maps`), compresses it with Zstd, and stores it under `var/lib/mcx/snapshots/<pkg_name>/snap-<timestamp>.mem`.

```shell
# Checkpoint process with PID 1234 for package 'myapp'
mcx --checkpoint myapp 1234

# List snapshots
mcx --snapshots myapp
```

**Engine:** `FeatureEngine::checkpoint_process()`

### Cloud-Streamable Overlay

**Flag:** `"cloud-streamable"`

Packages can be mounted directly from a remote URL via HTTP range-requests using `squashfuse`. MCX generates a shell script at `var/lib/mcx/stream/<pkg_name>.sh` that can be called to lazily mount the remote SquashFS filesystem without waiting for a full download.

```shell
# Generate streaming mount script
mcx --stream-mount myapp https://repo.example.com/myapp.xcs /mnt/myapp

# Remove the script
mcx --stream-umount myapp
```

**Engine:** `FeatureEngine::generate_stream_mount_script()`

### Isolated State Overlay

**Flag:** `"isolated-state-overlay"`

MCX creates an **ephemeral OverlayFS** per package in `~/.mcx/overlays/<pkg_name>/`. The package's configuration files are isolated in a private capsule — uninstalling the package leaves zero traces. Two instances of the same package can run with completely different configurations simultaneously.

```shell
# Create isolated overlay
mcx --overlay-create myapp ~/.config/myapp

# Remove overlay (cleanup on uninstall)
mcx --overlay-remove myapp
```

**Engine:** `FeatureEngine::create_isolated_overlay()`

### P2P Swarm Distribution

**Flag:** `"p2p-swarm"`

MCX implements a lightweight **peer-to-peer exchange protocol** where packages are identified by a cryptographic swarm hash. Peers register their addresses and advertised hashes in a local database. When installing a package, MCX can locate and fetch blocks from nearby peers rather than a central server.

```shell
# Register a swarm hash for a package
mcx --swarm-hash myapp e3b0c44298fc1c149afbf4c8996fb924

# Query a swarm hash
mcx --swarm-get myapp

# List known peers
mcx --swarm-peers

# Add a peer
mcx --swarm-peer-add 192.168.1.50:9735 peer-abc123
```

**Engine:** `FeatureEngine::register_swarm_hash()`, `register_swarm_peer()`

### Resource Throttle and Self-Healing Telemetry

**Flag:** `"resource-throttle"`

MCX writes **cgroup resource limits** for each package at `/sys/fs/cgroup/mcx/<pkg_name>/`, enforcing memory ceilings (`memory.max`) and CPU quotas (`cpu.max`). If a process exceeds its limits, the kernel throttles it automatically. This prevents any single package from consuming all system resources.

```shell
# Set max 512 MB memory, 50% CPU for myapp
mcx --throttle-set myapp 512 50

# Remove limits
mcx --throttle-remove myapp
```

**Engine:** `FeatureEngine::enforce_resource_limits()`, `remove_resource_limits()`

---

## Metadata Feature Flag Reference

When a package is built, its `metadata.json` can contain an `optimization_features` array. The following flags activate the corresponding MCX runtime engines:

| Flag | Feature | Engine Method |
| ------ | --------- | -------------- |
| `"lazy-mount"` | On-demand dinit mount | `generate_lazy_mount_service()` |
| `"cas-deduplication"` | Library dedup via CAS | `deduplicate_libraries()` |
| `"atomic-rollback"` | Symlink-switchable generations | `enable_atomic_rollback()` |
| `"delta-reconstruct"` | Micro-diff package rebuild | `reconstruct_delta()` |
| `"memory-snapshot"` | Process checkpoint/restore | `checkpoint_process()` |
| `"cloud-streamable"` | Remote SquashFS streaming | `generate_stream_mount_script()` |
| `"isolated-state-overlay"` | Per-package config isolation | `create_isolated_overlay()` |
| `"p2p-swarm"` | Decentralized peer-to-peer | `register_swarm_hash()` |
| `"resource-throttle"` | Cgroup resource policing | `enforce_resource_limits()` |

---

## Dependency Solving

The dependency solver:

- Loads package manifests from the local database
- Resolves recursive package dependencies
- Supports virtual providers via `provides`
- Detects cyclic dependency loops
- Verifies conflict constraints before install planning
- Produces a topologically ordered install plan
- Resolves library providers by file and `provides` matching

---

## System Profile Format

MCX can reconcile the installed package set against a declarative JSON profile:

```json
{
  "version": "1.0",
  "architecture": "x86_64",
  "packages": ["foo", "bar", "baz"]
}
```

The `build` command (`-b`) installs missing packages and removes packages not declared in the profile.

---

## Build From Source

```shell
cargo build --release
./target/release/mcx install foo
./target/release/mcx --root /tmp/mcx-root install foo
```

---

## Notes

- MCX automatically creates a lock at `/var/lib/mcx/lock` when any process starts, preventing database corruption from concurrent access. The lock is automatically removed on Ctrl+C.
- All package extraction and `metadata.json` checks are performed in a temporary isolated environment at `var/tmp/mcx/stage/`.
- The implementation is written in **Rust** and uses **tokio** for async operations.
- The downloader supports both sequential downloads and ranged chunked downloads for files larger than 5 MB.
- The configuration editor module provides a **TUI editor** accessible via `mcx -C`.
- Package state is managed in **JSON** and persisted under the configured root.
- Repository indexes are synchronized in **parallel** — each repository's fetch and verification runs concurrently.
- The `--root` flag allows operating on an alternative root filesystem (useful for containers or cross-installs).

---

## Contributing

To extend MCX or add new features, start with these files:

- `src/main.rs` — CLI entry point and command routing
- `src/core/database.rs` — package metadata and ledger state
- `src/core/features.rs` — all nine runtime feature engines
- `src/core/repo.rs` — multi-repository management and parallel sync
- `src/core/solver.rs` — dependency resolution
- `src/archive/extract.rs` — archive extraction
- `src/network/download.rs` — HTTP downloader
- `src/commands/` — subcommand implementations

For bug reports or feature requests, open an issue in the repository.

---

## Credits

[**`Myden`**](https://github.com/md7u) - **`Cudane`** and **`MCX`** Founder. Made with 🤍 and **Rust**.
