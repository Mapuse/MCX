pub const APP_NAME: &str = "mcx";
pub const APP_VERSION: &str = "7.0.0";
pub const DEFAULT_HOME: &str = "/tmp/.mcx";
pub const DEFAULT_ROOT: &str = "/";

// ── URLs ────────────────────────────────────────────────────────────────
pub const MAIN_REPO_URL: &str = "https://packages.cudane.org";
pub const COMMUNITY_REPO_URL: &str = "https://community.cudane.org";
pub const MAIN_REPO_PRIORITY: u32 = 100;
pub const COMMUNITY_REPO_PRIORITY: u32 = 200;

// ── Configuration defaults ──────────────────────────────────────────────
pub const DEFAULT_THREAD_POOL_MODE: &str = "auto";
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;
pub const HIGH_ZSTD_LEVEL: i32 = 5;
pub const DEFAULT_MAX_CONCURRENT_DOWNLOADS: usize = 8;
pub const DEFAULT_LATENCY_THRESHOLD_MS: u64 = 200;
pub const DEFAULT_BANDWIDTH_THRESHOLD_KBPS: u64 = 5000;
pub const DEFAULT_VERIFY_CHECKSUMS: bool = true;
pub const DEFAULT_ALLOW_UNVERIFIED: bool = false;
pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 5_368_709_120; // 5 GiB
pub const DEFAULT_CACHE_PRUNE_HOURS: u64 = 168; // 7 days

// ── Network ─────────────────────────────────────────────────────────────
pub const PROBE_SAMPLES: usize = 5;
pub const PROBE_TIMEOUT_SECS: u64 = 5;
pub const DEFAULT_FALLBACK_LATENCY: f64 = 999.9;
pub const DEFAULT_BANDWIDTH_KBPS: u64 = 100;
pub const FALLBACK_LATENCY_THRESHOLD: f64 = 5000.0;
pub const POOL_IDLE_TIMEOUT_SECS: u64 = 30;
pub const TCP_KEEPALIVE_SECS: u64 = 15;
pub const POOL_MAX_IDLE_PER_HOST: usize = 32;
pub const DEFAULT_MAX_CONCURRENT_CHUNKS: usize = 16;
pub const DEFAULT_MAX_CONCURRENT_PACKAGES: usize = 8;
pub const DEFAULT_MAX_RETRIES: u32 = 3;
pub const DEFAULT_BASE_DELAY_MS: u64 = 200;
pub const CHUNKED_DOWNLOAD_THRESHOLD: u64 = 5 * 1024 * 1024; // 5 MiB
pub const MIN_CHUNK_SIZE: u64 = 1024 * 1024; // 1 MiB

// ── System profiler thresholds ──────────────────────────────────────────
pub const CPU_THRESHOLD_HIGH: usize = 16;
pub const CPU_THRESHOLD_MEDIUM: usize = 8;
pub const CPU_THRESHOLD_LOW: usize = 4;
pub const RAM_THRESHOLD_LOW_MB: u64 = 512;
pub const RAM_THRESHOLD_MEDIUM_MB: u64 = 1024;
pub const RAM_THRESHOLD_HIGH_MB: u64 = 2048;
pub const THREAD_SCALE_FACTOR: f64 = 1.5;
pub const THREAD_SCALE_MAX: usize = 4;
pub const CONCURRENCY_SCALE_DOWN: usize = 2;
pub const DECISION_CONFIDENCE_HIGH: f64 = 0.95;
pub const DECISION_CONFIDENCE_MEDIUM: f64 = 0.85;
pub const DECISION_CONFIDENCE_LOW: f64 = 0.75;
pub const LATENCY_RATIO_PARALLEL: f64 = 2.0;
pub const LATENCY_RATIO_SEQUENTIAL: f64 = 1.0;
pub const LATENCY_SPIKE_CRITICAL: f64 = 3.0;
pub const LATENCY_SPIKE_WARNING: f64 = 1.5;
pub const JITTER_THRESHOLD_MS: f64 = 50.0;

// ── Repeated path components ─────────────────────────────────────────────
pub const PATH_ACTIVE: &str = "var/lib/mcx/active";
pub const PATH_CACHE: &str = "var/cache/mcx";
pub const PATH_TMP: &str = "var/tmp/mcx";
pub const PATH_STAGE: &str = "var/tmp/mcx/stage";
pub const PATH_PLUGINS: &str = "var/lib/mcx/plugins";
pub const PATH_DATA: &str = "var/lib/mcx/data";
pub const PATH_CAS: &str = "var/lib/mcx/cas";
pub const PATH_BININDEX: &str = "var/lib/mcx/binindex.json";
pub const PATH_GENERATIONS: &str = "var/lib/mcx/generations";
pub const PATH_HISTORY: &str = "var/lib/mcx/history.jsonl";
pub const PATH_VENDOR: &str = "var/lib/mcx/vendor";
pub const PATH_SYNC: &str = "var/lib/mcx/sync";
pub const PATH_BUILD: &str = "var/mcx/build";
pub const PATH_MCX_STAGE: &str = "var/mcx/stage";
pub const PATH_LOG_HISTORY: &str = "var/log/mcx/history";
pub const PATH_LIB_MCX: &str = "var/lib/mcx";
pub const PATH_ETC_MCX: &str = "etc/mcx";
pub const PATH_CONFIG_INI: &str = "etc/mcx/config.ini";
pub const PATH_REPO_INI: &str = "etc/mcx/repo.ini";
pub const PATH_PROFILE_INI: &str = "etc/mcx/profile.ini";

// ── Cgroup ──────────────────────────────────────────────────────────────
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup/mcx";
pub const CGROUP_PERIOD_US: u64 = 100_000;
pub const CGROUP_CPU_QUOTA_FACTOR: u64 = 1_000;
pub const DEFAULT_CGROUP_MAX_MEMORY_MB: u64 = 512;
pub const DEFAULT_CGROUP_MAX_CPU_PERCENT: u8 = 80;

// ── CAS (Content-Addressable Store) ────────────────────────────────────
pub const CAS_HASH_BUFFER_SIZE: usize = 65_536; // 64 KiB
pub const INTEGRITY_HASH_BUFFER_SIZE: usize = 65_536; // 64 KiB
pub const TRANSACTION_HASH_BUFFER_SIZE: usize = 65_536; // 64 KiB
pub const HASH_BUFFER_SIZE: usize = 8 * 1024 * 1024; // 8 MiB

// ── Database ────────────────────────────────────────────────────────────
pub const DB_MAP_SIZE: usize = 10 * 1024 * 1024; // 10 MiB
pub const DB_MAX_DBS: u32 = 4;

// ── Plugin system ───────────────────────────────────────────────────────
pub const DEFAULT_PLUGIN_TIMEOUT_SECS: u64 = 30;
pub const PYTHON_PLUGIN_MODULE_VAR: &str = "plugin.py";
pub const PLUGIN_CONFIG_FILE: &str = "etc/mcx/p.desc";

// ── Binaries ────────────────────────────────────────────────────────────
pub const SELF_UPDATE_BINARY_PATH: &str = "/system/bin/mcx";
pub const SELF_UPDATE_OLD_NAME: &str = "mcx.old";
pub const SELF_UPDATE_NEW_EXT: &str = "mcx.new";
pub const SELF_UPDATE_PERMISSIONS: u32 = 0o755;

// ── Tool names ──────────────────────────────────────────────────────────
pub const TOOL_CURL: &str = "curl";
pub const TOOL_GIT: &str = "git";
pub const TOOL_SH: &str = "sh";
pub const TOOL_TAR: &str = "tar";
pub const TOOL_ZSTD: &str = "zstd";
pub const TOOL_ID: &str = "id";
pub const TOOL_PYTHON3: &str = "python3";

// ── Cesar paths ─────────────────────────────────────────────────────────
pub const CESAR_SERVICES_DIR: &str = "etc/cesar/services.d";
pub const CESAR_BINARY_CANDIDATES: &[&str] = &["/usr/bin/cesar", "/sbin/cesar", "cesar"];

// ── Shared library directories for CAS deduplication ────────────────────
pub const LIB_DIRS: &[&str] = &["usr/lib", "lib", "usr/lib64", "lib64"];

// ── Binary scan directories ─────────────────────────────────────────────
pub const BINARY_SCAN_DIRS: &[&str] = &["usr/bin", "bin", "usr/sbin", "sbin", "usr/local/bin"];

// ── Library path prefixes for ELF string cleanup ────────────────────────
pub const LIB_PATH_PREFIXES: &[&str] = &["/system/lib/", "/usr/lib/", "/lib/"];

// ── Build environment ───────────────────────────────────────────────────
pub const CARGO_BUILD_TARGET_ENV: &str = "CUDANE_TARGET";
pub const CARGO_RUST_TARGET_ENV: &str = "CUDANE_RUST_TARGET";
pub const CARGO_SYSROOT: &str = "/system";
pub const RUSTFLAGS_TEMPLATE: &str =
    "-C linker=clang -C link-arg=-target -C link-arg={target} \
     -C link-arg=--sysroot={sysroot} -C target-feature=+crt-static";

// ── Build skip keywords ─────────────────────────────────────────────────
pub const BUILD_SKIP_KEYWORDS: &[&str] = &["none", "skip", "nothing"];

// ── ELF format constants (binary format, not configurable) ──────────────
pub const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46]; // \x7fELF
pub const ELFCLASS64: u8 = 2;
pub const ELF_MIN_HEADER_SIZE: usize = 64;
pub const ELF_PT_DYNAMIC: u32 = 2;
pub const ELF_PT_LOAD: u32 = 1;
pub const ELF_DT_STRTAB: u64 = 5;
pub const ELF_DT_STRSZ: u64 = 10;
pub const ELF_DT_NEEDED: u64 = 1;
pub const ELF_DT_NULL: u64 = 0;
pub const ELF_DYN_ENTRY_SIZE: usize = 16;
pub const ELF64_PHOFF_RANGE: std::ops::Range<usize> = 32..40;
pub const ELF64_PHENTSIZE_RANGE: std::ops::Range<usize> = 54..56;
pub const ELF64_PHNUM_RANGE: std::ops::Range<usize> = 56..58;

// ── UI defaults ─────────────────────────────────────────────────────────
pub const UI_PROGRESS_BAR_WIDTH: usize = 18;
pub const UI_TABLE_WIDTH: usize = 50;
pub const UI_BLOCK_WIDTH: usize = 60;

// ── Default INI content ─────────────────────────────────────────────────
pub const DEFAULT_CONFIG_INI: &str = "\
[general]\n\
log_level = info\n\
log_file = /var/log/mcx.md\n\
cache_dir = /var/cache/mcx\n\
build_dir = /tmp/mcx/build\n\
\n\
[engine]\n\
thread_pool_mode = auto\n\
max_concurrent_downloads = 8\n\
zstd_level = 3\n\
io_parallelism = 4\n\
\n\
[network]\n\
fallback_repos = enabled\n\
latency_threshold_ms = 200\n\
bandwidth_threshold_kbps = 5000\n\
concurrent_downloads = 8\n\
\n\
[security]\n\
verify_checksums = true\n\
allow_unverified = false\n\
restricted_mode = false\n\
allowed_paths = /system,/etc,/tmp,/var,/home\n\
\n\
[cache]\n\
enabled = true\n\
limit_bytes = 5368709120\n\
max_size_mb = 1024\n\
prune_age_hours = 168\n\
ttl_hours = 24\n\
\n\
[python]\n\
enabled = false\n\
theme = \n\
tui = \n\
plugins = \n\
fallback_on_error = true\n\
venv_path = \n\
tui_mode = false\n";

pub const DEFAULT_REPO_INI: &str = "\
[main]\n\
url = https://packages.cudane.org\n\
enabled = true\n\
priority = 100\n\
\n\
[community]\n\
url = https://community.cudane.org\n\
enabled = false\n\
priority = 200\n";

// ── Default Python plugin template ──────────────────────────────────────
pub const DEFAULT_PYTHON_PLUGIN: &str = r#"PLUGIN_NAME = "my-plugin"
PLUGIN_VERSION = "1.0.0"
PLUGIN_DESCRIPTION = "A custom MCX plugin"
PLUGIN_TYPE = "hook"
PLUGIN_HOOKS = ["post-install"]
PLUGIN_TIMEOUT = 30
PLUGIN_AUTHOR = ""
PLUGIN_HOMEPAGE = ""

def on_pre_install(event):
    return {"success": True, "message": "pre-install hook executed"}

def on_post_install(event):
    return {"success": True, "message": "post-install hook executed"}

def on_pre_remove(event):
    return {"success": True, "message": "pre-remove hook executed"}

def on_post_remove(event):
    return {"success": True, "message": "post-remove hook executed"}

def on_pre_verify(event):
    return {"success": True, "message": "pre-verify hook executed"}

def on_post_verify(event):
    return {"success": True, "message": "post-verify hook executed"}

def on_pre_fix(event):
    return {"success": True, "message": "pre-fix hook executed"}

def on_post_fix(event):
    return {"success": True, "message": "post-fix hook executed"}
"#;
