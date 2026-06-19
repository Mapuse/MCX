#

`▐▀` `-` `▀▀▀▀▀▀▀▀▌`

```shell
███╗   ███╗  ██████╗██╗    ██╗    ██████╗  █████╗  ██████╗██╗  ██╗ █████╗  ██████╗ ███████╗
████╗ ████║██╔════╝ ╚██╗  ██╔╝     ██╔══██╗██╔══██╗██╔════╝██║ ██╔╝██╔══██╗██╔════╝ ██╔════╝
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

`▀` `-` `▀▀▀▀▀▀▀▀`

<details><summary id="contents">Contents</summary>
  
- [[Architecture]](#arch)
- [[1. Metadata Engine]](#1-metadata-engine)
- [[2. Atomical Layer]](#2-atomical-layer)
- [[3. CAS & OverlayFS Layer]](#3-cas--overlayfs-layer)
- [[4. Lazy-Mount & Dinit Integrator]](#4-lazy-mount--dinit-integrator)
- [[5. Automated SandBox Enforcer]](#5-automated-sandbox-enforcer)
- [[6. Delta Reconstructor Engine]](#6-delta-reconstructor-engine)
- [[7. Update Manager]](#7-update-manager)
- [[Commands]](#commands-list)
  - [[Package Management]](#package-management)
  - [[Repository Management]](#repository-management)
  - [[Runtime Features]](#runtime-features)
- [[Integration]](#integration)
  - [[Directory Structure]](#directory-structure)
- [[Development]](#development)
  - [[Project Structure]](#project-structure)
  - [[Building]](#building)
- [[Compatibility]](#compatibility)
  - [[rLine 0.2.0+ Features Supported]](#rline-020-features-supported)
- [[Credits]](#credits)
- [[License]](#license)

</details>

<details><summary id="arch">Architexture</summary>

Unlike traditional package managers that simply "unzip and transfer files," MCX treats `.xcs` packages as **isolated and protected live file systems**. MCX never decompresses packages on disk; instead, it orchestrates their lifecycle through six core engines:

---

## 1. Metadata Engine

**Location:** `src/core/metadata.rs`

Reads `metadata.json` directly from SquashFS packages without extraction using `unsquashfs -cat`.

### Features

- **Zero-extraction metadata reading** - No need to extract entire package to read metadata
- **Composite package analysis** - Recursive analysis of `pkg_type: "plain"/"bundle"` packages
- **Dependency signature verification** - Cryptographic integrity checking via `depsig`

### Usage

```rust
use mcx::core::metadata::{RLineMetadata, MetadataIngestionEngine};

let meta = RLineMetadata::read_from_xcs(Path::new("package.xcs"))?;
println!("Package: {} v{}", meta.pkg_name, meta.version);
println!("Prefix: {}", meta.prefix);
println!("Depsig: {:?}", meta.depsig);
```

### rLine Metadata Structure

The `RLineMetadata` struct matches rLine 0.2.0+ output exactly:

| Field | Type | Description |
| ------- | ------ | ------------- |
| `pkg_name` | String | Package identifier |
| `version` | String | Package version |
| `source` | String | Source URL or path |
| `license` | String | Detected license (MIT, GPL, etc.) |
| `build_type` | String | Build system (rust, make, meson, custom) |
| `build_date` | String | ISO 8601 UTC timestamp |
| `checksum` | String | SHA-256 of package contents |
| `pkg_type` | String | "plain" or "bundle" |
| `components` | Option<`Vec<RLineMetadata>`> | Nested sub-packages |
| `services` | Option<`Vec<String>`> | Dinit service filenames |
| `profile` | ``Vec<String>`` | SandBox capabilities |
| `features` | `Vec<String>` | Optimization features |
| `externals` | Option<`Vec<String>`> | External library dependencies |
| `bundled` | Option<`Vec<String>`> | Bundled library dependencies |
| `depsig` | Option<`String`> | SHA-256 dependency signature |
| `prefix` | `String` | Installation prefix path |

> [!TIP]
> The `prefix` field is crucial - it tells MCX where binaries and libraries are located within the capsule (e.g., "system" means `/system/bin` and `/system/lib`)

-

> [!NOTE]
> Meta packages (`pkg_type: "bundle"`) contain nested `.xcs` files in their `components` array, enabling composite package distribution

---

## 2. Atomical Layer

**Location:** `src/core/atomic.rs`

Provides zero-downtime updates with instant rollback via symlink switching.

### Atomical Architecture

```shell
/system/storage/
├── packages/          # All versions stored here
│   ├── hello-0.2.0-a1b2c3.xcs
│   └── hello-0.1.0-d4e5f6.xcs
└── active/            # Symlinks to active versions
    └── hello -> ../../packages/hello-0.2.0-a1b2c3.xcs
```

### Atomical Workflow

1. **Install**: New `.xcs` copied to `/system/storage/packages/` with unique name containing version and checksum
2. **Symlink Switch**: Active symlink flipped to point to new capsule (atomic operation)
3. **Old Version Preserved**: Previous version remains on disk for instant rollback
4. **Rollback**: Symlink flipped back to old capsule in <1ms

### Aromical Features

- **Zero-downtime updates** - No service interruption during package updates
- **Instant rollback** - <1ms rollback by symlink flip only
- **Generation tracking** - Unlimited history via generation numbers
- **Atomic operations** - No partial states possible

### Atomical Usage

```rust
use mcx::core::atomic::AtomicInstaller;

let installer = AtomicInstaller::new("/");
installer.install(Path::new("hello-0.2.0.xcs"), &meta)?;

// Rollback to previous generation
installer.rollback("hello", 1)?;

// List all generations
let gens = installer.list_generations("hello")?;
println!("Generations: {:?}", gens);
```

> [!WARNING]
> The atomic installer requires `/system/storage/packages/` and `/system/storage/active/` directories to be on the same filesystem for atomic symlink operations

---

## 3. CAS & OverlayFS Layer

**Location:** `src/core/overlay.rs`

Content-Addressable Storage for deduplication with OverlayFS integration.

### CAS Structure

```shell
system/storage/
├── shared_libs/       # CAS repository
│   ├── ab/
│   │   └── abc123...   # libfoo.so (stored once)
│   └── cd/
│       └── cdef456...  # libbar.so (stored once)
└── cas/               # Hard-link index
```

### OverlayFS Architecture

When a package runs, MCX creates an OverlayFS mount:

- **LowerDir**: Read-only SquashFS capsule (the package itself)
- **UpperDir**: tmpfs for temporary writes (disappears on unmount)
- **WorkDir**: OverlayFS work directory

### CAS Deduplication Workflow

1. **Hash Calculation**: Compute SHA-256 of each `.so` file
2. **CAS Lookup**: Check if hash exists in `/system/storage/shared_libs/`
3. **Hard Link**: If exists, replace with hard link (saves space)
4. **Store**: If new, copy to CAS and hard-link back

### CAS Usage

```rust
use mcx::core::overlay::CasOverlayEngine;

let engine = CasOverlayEngine::new("/");
let saved = engine.deduplicate_package(Path::new("/var/tmp/stage"))?;
println!("Saved {} bytes via CAS", saved);

// Create overlay mount for execution
let mount = engine.create_overlay_mount(
    Path::new("package.xcs"),
    Path::new("/mnt/app"),
    "system"
)?;
```

### CAS Features

- **Space efficiency** - Libraries stored once, hard-linked across packages
- **OverlayFS integration** - Combines read-only capsule + shared libs + tmpfs writes
- **Automatic deduplication** - Happens during package installation
- **Scalable savings** - More packages = more space saved

> [!TIP]
> CAS deduplication is most effective when multiple packages share common libraries like `libc.so`, `libm.so`, etc.

-

> [!NOTE]
> The OverlayFS mount is automatically cleaned up when the `OverlayMount` struct is dropped (RAII pattern)

---

## 4. Lazy-Mount & Dinit Integrator

**Location:** `src/core/mount.rs`

Zero RAM consumption for idle programs via on-demand mounting.

### Lazy-Mount Structure

```shell
/etc/dinit.d/
├── mcx-mount-hello.dinit      # Main mount service
└── hello.service.socket        # Socket activation

/system/bin/
├── mcx-mount-runner           # Mount + exec + unmount
└── mcx-mount                  # Fast SquashFS mount
```

### Lazy-Mount Workflow

1. **Dormant State**: Capsule remains closed on disk (zero RAM)
2. **Trigger**: dinit wakes MCX on service request or socket activation
3. **Mount**: MCX mounts SquashFS capsule (microseconds)
4. **Execute**: Program runs normally
5. **Unmount**: Automatic unmount when program exits

### Lazy-Mount Usage

```rust
use mcx::core::mount::LazyMountEngine;

let engine = LazyMountEngine::new("/");
engine.generate_mount_service(&meta, "/system/hello")?;
engine.generate_mount_runner()?;
```

### Lazy-Mount Features

- **Zero RAM for idle programs** - packages consume no memory when not running
- **Socket activation** - Instant response via dinit socket activation
- **Microsecond mounting** - SquashFS mounts in microseconds
- **Automatic cleanup** - Unmounts when program exits

> [!WARNING]
> Lazy-mount requires dinit init system. The capsule must remain accessible at its path for mounting to work.

-

> [!TIP]
> Use lazy-mount for services that are rarely used but need to be available on-demand (e.g., print services, backup daemons)

---

## 5. Automated SandBox Enforcer

**Location:** `src/core/sandbox.rs`

Military-grade isolation using Linux namespaces and cgroups.

### Isolation

The sandbox reads `profile` from rLine metadata and applies:

- **CLONE_NEWNET**: Network isolation (if "network" not in profile)
- **CLONE_NEWNS**: Filesystem isolation via pivot_root
- **Display blocking**: If "wayland" not in profile
- **cgroups**: Memory, CPU, and process limits

### SandBox Profile Example

```json
{
  "profile": [[
    "isolated-rootfs",
    "wayland",
    "network"
  ]]
}
```

### SB Manager Usage

```rust
use mcx::core::sandbox::SandboxEnforcer;

let enforcer = SandboxEnforcer::new("/");
let process = enforcer.execute_sandboxed(
    Path::new("package.xcs"),
    &meta,
    "/system/bin/app",
    &[]
)?;
```

### SB Manager Features

- **Automatic namespace configuration** - Reads profile and applies appropriate namespaces
- **cgroup-based resource policing** - Memory, CPU, and process limits
- **Root filesystem isolation** - Program sees only its capsule contents
- **No heavy tools** - No Docker or Flatpak required

> [!WARNING]
> SandBox execution requires root privileges for namespace creation. The `unshare` command must be available.

-

> [!NOTE]
> The sandbox automatically creates isolated rootfs by mounting the capsule's prefix directory as the program's root filesystem

---

## 6. Delta Reconstructor Engine

**Location:** `src/core/delta.rs`

Bandwidth-saving delta updates using `.xcs` patch files.

### Process

```shell
1. Old package: hello-1.0.xcs (50MB)
2. Delta patch: hello-1.0-to-1.1.xcs (2MB)
3. Reconstruct: hello-1.1.xcs (50MB) - generated locally
```

### Delta Workflow

1. **Extract old package**: Unpack old `.xcs` to staging directory
2. **Read delta metadata**: Parse `delta.meta` from `.xcs` file
3. **Apply patches**: Merge changed blocks from delta
4. **Handle removals**: Delete files marked as removed
5. **Repackage**: Create new `.xcd` with mksquashfs

### Delta Usage

```rust
use mcx::core::delta::DeltaReconstructor;

let reconstructor = DeltaReconstructor::new("/");
reconstructor.reconstruct(
    Path::new("hello-1.0.xcs"),
    Path::new("hello-1.0-to-1.1.xcd"),
    Path::new("hello-1.1.xcd")
)?;
```

### Delta Features

- **Bandwidth savings** - Download only changed blocks (typically 95% smaller)
- **Local reconstruction** - Uses CPU/RAM, no extra internet needed
- **Cryptographic verification** - Delta files include checksums
- **Fast updates** - Reconstruction takes ~50ms vs downloading 50MB

> [!TIP]
> Delta updates are ideal for frequent small updates. The `.xcs` packages are generated by rLine during repository indexing.

-

> [!NOTE]
> The delta reconstructor supports both SquashFS and tar+zstd package formats for maximum compatibility

---

## 7. Update Manager

**Location:** `src/core/update.rs`

Comprehensive update system supporting multiple source types.

### Update Sources

The `UpdateManager` supports three update source types:

1. **Repository**: Sync repo and install all updates
2. **Local**: Install from local `.xcs` file
3. **Delta**: Reconstruct from delta patch and install

### Update Manager Usage

```rust
use mcx::core::update::UpdateManager;

let manager = UpdateManager::new(
    "3.0.0",
    PathBuf::from("/"),
    Arc::new(db)
)?;

// Check for updates
if let Some(release) = manager.check_for_updates("https://updates.example.com/meta.json").await? {
    println!("New version available: {}", release.version);
}

// Update from repository
manager.update_from_source(&UpdateSource {
    source_type: "repository".to_string(),
    url: Some("https://repo.example.com".to_string()),
    local_path: None,
    package_name: None,
}).await?;

// Update from local file
manager.update_from_source(&UpdateSource {
    source_type: "local".to_string(),
    url: None,
    local_path: Some("/path/to/package.xcs".to_string()),
    package_name: None,
}).await?;

// Delta update
manager.update_from_source(&UpdateSource {
    source_type: "delta".to_string(),
    url: Some("https://updates.example.com/hello-1.0-to-1.1.xcd".to_string()),
    local_path: Some("/tmp/hello-1.1.xcd".to_string()),
    package_name: Some("hello".to_string()),
}).await?;

// Upgrade all packages
manager.upgrade_all().await?;

// Sync repositories
manager.sync_repositories().await?;
```

### Update Manager Features

- **Multiple source types** - Repository, local, or delta updates
- **Automatic fallback** - Tries delta first, falls back to full download
- **Version checking** - Compares versions before updating
- **Integrated operations** - Calls sync, install, and add commands internally

> [!TIP]
> Use `update_from_source()` with a `RepositoryInfo` source type for automated system updates

-

> [!WARNING]
> Delta updates require the old package to exist in `/system/storage/packages/`

</details>

<details><summary id="commands">Commands</summary>

### Package Management

| Command | Aliases | Description |
| --------- | --------- | ------------- |
| `install` | `i`, `in`, `add` | Deploy packages into target system |
| `add-local` | `a`, `local`, `package`, `xcs` | Install local .xcs package file |
| `remove` | `r`, `rm`, `uninstall`, `delete` | Remove installed packages |
| `search` | `s`, `find`, `look` | Search available packages |
| `update` | `u`, `refresh`, `sync` | Sync repository indexes |
| `upgrade` | `U`, `up`, `dist-upgrade` | Upgrade all installed packages |
| `query` | `q`, `info`, `show` | Show package information |
| `clean` | `c`, `wipe`, `clear` | Clear cache and temporary files |
| `verify` | `v`, `check`, `certify` | Verify package integrity |
| `fix-deps` | `f`, `fix-deps`, `repair` | Fix dependency issues |
| `config` | `C`, `cfg`, `settings` | Manage MCX configuration |
| `history` | `h`, `log`, `record` | Show installation history |
| `build` | `b`, `make`, `create` | Build system from blueprint |

### Repository Management

| Command | Aliases | Description |
| --------- | --------- | ------------- |
| `repo-add` | `ra` | Add a repository |
| `repo-remove` | `rr` | Remove a repository |
| `repo-list` | `rl` | List configured repositories |

### Runtime Features

| Command | Aliases | Description |
| --------- | --------- | ------------- |
| `lazy-mount` | `L`, `mount` | Enable lazy mounting for package |
| `lazy-umount` | `N`, `unmount` | Disable lazy mounting |
| `dedup` | `D`, `cas`, `overlay` | Run CAS deduplication |
| `rollback` | `R`, `rb` | Rollback to previous generation |
| `generations` | `gens` | List package generations |
| `delta` | `d`, `reconstruct`, `xcd` | Reconstruct package from delta |
| `checkpoint` | `snap` | Create process snapshot |
| `snapshots` | `snaps` | List process snapshots |
| `stream-mount` | `sm` | Mount remote package stream |
| `stream-umount` | `sum` | Unmount stream |
| `overlay-create` | `oc` | Create isolated overlay |
| `overlay-remove` | `or` | Remove isolated overlay |
| `swarm-hash` | `sh` | Register swarm hash |
| `swarm-get` | `sg` | Get swarm hash |
| `swarm-peers` | `sp` | List swarm peers |
| `swarm-peer-add` | `spa` | Add swarm peer |
| `throttle-set` | `ts` | Set resource limits |
| `throttle-remove` | `tr` | Remove resource limits |

</details>

<details><summary id="integration">Integration</summary>

### Directory Structure

```shell
/ (system root)
├── system/
│   └── storage/
│       ├── packages/          # All package versions
│       ├── active/            # Symlinks to active versions
│       ├── shared_libs/       # CAS repository
│       └── mcx/
│           └── metadata.db    # Hidden system database
├── etc/
│   └── dinit.d/              # Lazy-mount services
├── system/
│   └── bin/
│       ├── mcx               # Package manager
│       ├── mcx-mount         # Fast SquashFS mount
│       └── mcx-mount-runner  # Mount + exec + unmount
└── var/
    └── lib/
        └── mcx/
            ├── generations/   # Generation history
            ├── snapshots/     # Memory snapshots
            └── swarm/         # P2P swarm data
```

---

## Development

### Project Structure

```shell
MCX/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library root
│   ├── core/
│   │   ├── metadata.rs      # rLine metadata ingestion
│   │   ├── atomic.rs # Atomic installation layer
│   │   ├── overlay.rs   # CAS + OverlayFS
│   │   ├── mount.rs    # dinit integration
│   │   ├── sandbox.rs       # Namespace/cgroup sandbox
│   │   ├── delta.rs         # Delta reconstruction
│   │   ├── update.rs        # Update manager
│   │   ├── database.rs      # Legacy database
│   │   ├── features.rs      # Feature engine
│   │   └── ...              # Other core modules
│   ├── commands/            # CLI commands
│   ├── network/             # Download/sync
│   ├── archive/             # Extraction/verification
│   └── utils/               # UI utilities
├── Cargo.toml
└── README.md
```

### Building

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

<details><summary id="comptibility">Compatibility</summary>

### rLine 0.2.0+ Features Supported

- **`Metadata Ingestion`** - Reads all rLine metadata fields.
- **`Atomical Rollback`** - Symlink-based instant rollback.
- **`CAS Deduplication`** - Content-addressable shared libraries.
- **`Lazy-Mount`** - Zero RAM for idle programs.
- **`SandBox Profile`** - Automated namespace/cgroup isolation.
- **`Delta Reconstruct`** - Bandwidth-saving updates.
- **`Bundle Packages`** - Composite package support.
- **`Dependency Signature`** - Cryptographic integrity.
- **`Update Manager`** - Multi-source update system.

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

##

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`MCX`** is the official package manager for **`Cudane`**, completely written as a **`Runtime Package Manager`** to achieve 100% compatibility with the **`rLine`** build engine.
  
`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`Version`:** **`2.7.5`**.
- **`Engine`:** **`rLine 0.2.0`**.
- **`Architecture`:** **`x86_64-unknown-linux-musl`** (**`x86_64-pc-linux-musl`**).
- **`Isolation`:** **`Linux NameSpaces`** **+** **`CGroups`**.
- **`Compression`:** **`SquashFS`** **+** **`Zstd`**.
- **`Updates`:** **`Delta Reconstruction`** **+** **`Multi-Source`**.
- **`Deduplication`:** **`Content-Addressable Storage`** **(`CAS`)**.

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`
