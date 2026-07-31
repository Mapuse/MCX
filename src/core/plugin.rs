use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::RwLock;
use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use crate::core::arch::Architecture;
use super::constants;

// ── Builtin plugin traits ─────────────────────────────────────────────────

fn shell_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('\'');
    for c in s.chars() {
        if c == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(c);
        }
    }
    escaped.push('\'');
    escaped
}

pub trait Fetcher: Send + Sync {
    fn fetch(&self, source: &str, destination: &str) -> Result<()>;
    fn name(&self) -> &'static str;
}

pub trait Builder: Send + Sync {
    fn build(&self, build_cmd: &str, source_dir: &str, dest_dir: &str, build_type: &str) -> Result<String>;
    fn name(&self) -> &'static str;
}

pub trait Packer: Send + Sync {
    fn pack(&self, source_dir: &str, output_path: &str, compression_level: i32) -> Result<()>;
    fn unpack(&self, archive_path: &str, dest_dir: &str) -> Result<Vec<String>>;
    fn name(&self) -> &'static str;
}

// ── PluginSlot: hot-swappable wrapper ──────────────────────────────────────

pub struct PluginSlot<T: ?Sized + Send + Sync> {
    inner: RwLock<Arc<T>>,
}

impl<T: ?Sized + Send + Sync> PluginSlot<T> {
    pub fn new(plugin: Arc<T>) -> Self {
        Self { inner: RwLock::new(plugin) }
    }

    pub fn load(&self) -> Arc<T> {
        self.inner.read().expect("plugin inner read").clone()
    }

    pub fn swap(&self, new_plugin: Arc<T>) -> Arc<T> {
        let mut guard = self.inner.write().expect("plugin inner write");
        let old = (*guard).clone();
        *guard = new_plugin;
        old
    }
}

// ── Builtin registry ───────────────────────────────────────────────────────

pub struct PluginRegistry {
    fetchers: Vec<PluginSlot<dyn Fetcher>>,
    builders: Vec<PluginSlot<dyn Builder>>,
    packers: Vec<PluginSlot<dyn Packer>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { fetchers: Vec::new(), builders: Vec::new(), packers: Vec::new() }
    }

    pub fn register_fetcher(&mut self, f: Arc<dyn Fetcher>) {
        self.fetchers.push(PluginSlot::new(f));
    }

    pub fn register_builder(&mut self, b: Arc<dyn Builder>) {
        self.builders.push(PluginSlot::new(b));
    }

    pub fn register_packer(&mut self, p: Arc<dyn Packer>) {
        self.packers.push(PluginSlot::new(p));
    }

    pub fn resolve_fetcher(&self, name: &str) -> Option<Arc<dyn Fetcher>> {
        self.fetchers.iter().find(|s| s.load().name() == name).map(|s| s.load())
    }

    pub fn resolve_builder(&self, name: &str) -> Option<Arc<dyn Builder>> {
        self.builders.iter().find(|s| s.load().name() == name).map(|s| s.load())
    }

    pub fn resolve_packer(&self, name: &str) -> Option<Arc<dyn Packer>> {
        self.packers.iter().find(|s| s.load().name() == name).map(|s| s.load())
    }

    pub fn swap_fetcher(&self, name: &str, new: Arc<dyn Fetcher>) -> bool {
        for slot in &self.fetchers {
            if slot.load().name() == name { slot.swap(new); return true; }
        }
        false
    }

    pub fn swap_builder(&self, name: &str, new: Arc<dyn Builder>) -> bool {
        for slot in &self.builders {
            if slot.load().name() == name { slot.swap(new); return true; }
        }
        false
    }

    pub fn swap_packer(&self, name: &str, new: Arc<dyn Packer>) -> bool {
        for slot in &self.packers {
            if slot.load().name() == name { slot.swap(new); return true; }
        }
        false
    }

    pub fn default_fetcher(&self) -> Option<Arc<dyn Fetcher>> { self.fetchers.first().map(|s| s.load()) }
    pub fn default_builder(&self) -> Option<Arc<dyn Builder>> { self.builders.first().map(|s| s.load()) }
    pub fn default_packer(&self) -> Option<Arc<dyn Packer>> { self.packers.first().map(|s| s.load()) }

    pub fn fetcher_count(&self) -> usize { self.fetchers.len() }
    pub fn builder_count(&self) -> usize { self.builders.len() }
    pub fn packer_count(&self) -> usize { self.packers.len() }
}

// Builtin implementations

pub struct CurlFetcher;
impl Fetcher for CurlFetcher {
    fn fetch(&self, source: &str, destination: &str) -> Result<()> {
        fs::create_dir_all(destination)?;
        let status = Command::new(constants::TOOL_CURL)
            .arg("-fSL")
            .arg("-o")
            .arg("/dev/stdout")
            .arg(source)
            .stdout(Stdio::piped())
            .status()
            .or_else(|_| {
                Command::new(constants::TOOL_SH)
                    .arg("-c")
                    .arg(format!("curl -fSL -o /dev/stdout -- {}", shell_escape(source)))
                    .status()
            })?;
        if !status.success() {
            let status2 = Command::new(constants::TOOL_GIT).arg("clone").arg("--depth").arg("1").arg(source).arg(destination).status()?;
            if !status2.success() { anyhow::bail!("Failed to fetch source: {}", source); }
        }
        Ok(())
    }
    fn name(&self) -> &'static str { "curl" }
}

pub struct DefaultBuilder;
impl Builder for DefaultBuilder {
    fn build(&self, build_cmd: &str, source_dir: &str, _dest_dir: &str, build_type: &str) -> Result<String> {
        if constants::BUILD_SKIP_KEYWORDS.contains(&build_cmd.trim()) {
            return Ok(String::new());
        }
        if build_cmd.is_empty() && build_type == "rust" {
            let target = std::env::var(constants::CARGO_BUILD_TARGET_ENV)
                .unwrap_or_else(|_| Architecture::host().target_triple().to_string());
            let rust_target = std::env::var(constants::CARGO_RUST_TARGET_ENV)
                .unwrap_or_else(|_| target.clone());
            let output = Command::new(constants::TOOL_SH)
                .arg("-c")
                .arg(format!(
                    "RUSTFLAGS=\"-C linker=clang -C link-arg=-target -C link-arg={} \
                     -C link-arg=--sysroot={} -C target-feature=+crt-static\" \
                     cargo build --target {} --release 2>&1",
                    shell_escape(&target), constants::CARGO_SYSROOT, shell_escape(&rust_target)
                ))
                .current_dir(source_dir).output()?;
            let log = String::from_utf8_lossy(&output.stdout).to_string();
            if !output.status.success() { anyhow::bail!("Build failed:\n{}", log); }
            return Ok(log);
        }
        if !build_cmd.is_empty() {
            let env_target = std::env::var(constants::CARGO_BUILD_TARGET_ENV).unwrap_or_default();
            let cmd = if env_target.is_empty() {
                format!("({}) 2>&1", build_cmd)
            } else {
                format!("CUDANE_TARGET={} ({}) 2>&1", shell_escape(&env_target), build_cmd)
            };
            let output = Command::new(constants::TOOL_SH).arg("-c").arg(&cmd).current_dir(source_dir).output()?;
            let log = String::from_utf8_lossy(&output.stdout).to_string();
            if !output.status.success() { anyhow::bail!("Build failed:\n{}", log); }
            return Ok(log);
        }
        Ok(String::new())
    }
    fn name(&self) -> &'static str { "default" }
}

pub struct ZstdPacker;
impl Packer for ZstdPacker {
    fn pack(&self, source_dir: &str, output_path: &str, compression_level: i32) -> Result<()> {
        let _ = fs::remove_file(output_path);
        let status = Command::new(constants::TOOL_TAR)
            .arg("-c")
            .arg("-C").arg(source_dir)
            .arg(".")
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|child| {
                Command::new(constants::TOOL_ZSTD)
                    .arg(format!("-{}", compression_level))
                    .arg("-o").arg(output_path)
                    .stdin(child.stdout.expect("child stdout"))
                    .status()
            })?;
        if !status.success() { anyhow::bail!("Failed to pack archive: {}", output_path); }
        Ok(())
    }
    fn unpack(&self, archive_path: &str, dest_dir: &str) -> Result<Vec<String>> {
        fs::create_dir_all(dest_dir)?;
        let file = fs::File::open(archive_path)?;
        let decoder = zstd::stream::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        let mut files = Vec::new();
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                anyhow::bail!("Path traversal detected: {:?}", path);
            }
            let dest = std::path::Path::new(dest_dir).join(&path);
            if let Some(parent) = dest.parent() { fs::create_dir_all(parent)?; }
            entry.unpack(&dest)?;
            files.push(path.to_string_lossy().to_string());
        }
        Ok(files)
    }
    fn name(&self) -> &'static str { "zstd" }
}

// ── External plugin system (Python) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginHook {
    PreInstall,
    PostInstall,
    PreRemove,
    PostRemove,
    PreVerify,
    PostVerify,
    PreFix,
    PostFix,
}

impl std::str::FromStr for PluginHook {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().replace('-', "_").as_str() {
            "pre_install" => Self::PreInstall,
            "post_install" => Self::PostInstall,
            "pre_remove" => Self::PreRemove,
            "post_remove" => Self::PostRemove,
            "pre_verify" => Self::PreVerify,
            "post_verify" => Self::PostVerify,
            "pre_fix" => Self::PreFix,
            "post_fix" => Self::PostFix,
            _ => return Err(()),
        })
    }
}

impl PluginHook {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreInstall => "pre-install",
            Self::PostInstall => "post-install",
            Self::PreRemove => "pre-remove",
            Self::PostRemove => "post-remove",
            Self::PreVerify => "pre-verify",
            Self::PostVerify => "post-verify",
            Self::PreFix => "pre-fix",
            Self::PostFix => "post-fix",
        }
    }

    pub fn function_name(&self) -> &'static str {
        match self {
            Self::PreInstall => "on_pre_install",
            Self::PostInstall => "on_post_install",
            Self::PreRemove => "on_pre_remove",
            Self::PostRemove => "on_post_remove",
            Self::PreVerify => "on_pre_verify",
            Self::PostVerify => "on_post_verify",
            Self::PreFix => "on_pre_fix",
            Self::PostFix => "on_post_fix",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEvent {
    pub hook: String,
    pub package: Option<String>,
    pub root: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResult {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
}

// ── TOML plugin configuration ─────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfig {
    #[serde(rename = "plugin")]
    pub plugins: HashMap<String, PluginEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginEntry {
    pub name: String,
    pub path: String,
    #[serde(flatten)]
    pub aliases: HashMap<String, String>,
}

// ── PythonPlugin ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PythonPlugin {
    name: String,
    pub path: PathBuf,
    pub aliases: HashMap<String, String>,
}

impl PythonPlugin {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!("Plugin file not found: {:?}", path));
        }
        let path_str = path.to_string_lossy();
        let check_script = format!(
            "compile(open('{}').read(), '{}', 'exec')",
            path_str.replace('\'', "\\'"),
            path_str.replace('\'', "\\'"),
        );
        let output = Command::new(constants::TOOL_PYTHON3)
            .arg("-c")
            .arg(&check_script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("Failed to run python3 for syntax check of {:?}", path))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Python syntax error in {:?}: {}", path, stderr.trim());
        }
        let file_stem = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(Self {
            name: file_stem,
            path: path.to_path_buf(),
            aliases: HashMap::new(),
        })
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn name(&self) -> &str { &self.name }
    pub fn path(&self) -> &Path { &self.path }

    pub fn run(&self, event_json: &str) -> Result<PluginResult> {
        let path_str = self.path.to_string_lossy();
        let escaped_event = event_json.replace('\'', "\\'");
        let script = format!(
            r#"import json, sys; MCX_EVENT = json.loads('{}'); exec(open('{}').read())"#,
            escaped_event,
            path_str.replace('\'', "\\'"),
        );
        let output = Command::new(constants::TOOL_PYTHON3)
            .arg("-c")
            .arg(&script)
            .current_dir(self.path.parent().unwrap_or(Path::new(".")))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            let result: PluginResult = serde_json::from_str(&stdout).unwrap_or(PluginResult {
                success: true,
                message: Some(stdout),
                data: None,
            });
            Ok(result)
        } else {
            Ok(PluginResult {
                success: false,
                message: Some(if stderr.is_empty() { stdout } else { stderr }),
                data: None,
            })
        }
    }

    pub fn run_hook(&self, event: &PluginEvent) -> Result<PluginResult> {
        let event_dict = serde_json::to_string(&serde_json::json!({
            "hook": event.hook,
            "package": event.package,
            "root": event.root,
            "timestamp": event.timestamp,
        })).unwrap_or_default();
        self.run(&event_dict)
    }
}

// ── PluginManager ─────────────────────────────────────────────────────────────

pub struct PluginManager {
    inner: RwLock<PluginManagerInner>,
}

struct PluginManagerInner {
    plugins: Vec<PythonPlugin>,
    loaded_paths: HashMap<PathBuf, usize>,
    hook_map: HashMap<PluginHook, Vec<usize>>,
}

impl PluginManager {
    pub fn new(root: &Path) -> Self {
        let mgr = Self {
            inner: RwLock::new(PluginManagerInner {
                plugins: Vec::new(),
                loaded_paths: HashMap::new(),
                hook_map: HashMap::new(),
            }),
        };
        let _ = mgr.load_all(root);
        mgr
    }

    fn load_all(&self, root: &Path) -> Result<()> {
        let config_path = root.join(constants::PLUGIN_CONFIG_FILE);
        if config_path.exists() {
            self.load_config(root)
        } else {
            self.scan_plugins_dir(root)
        }
    }

    fn load_config(&self, root: &Path) -> Result<()> {
        let config_path = root.join(constants::PLUGIN_CONFIG_FILE);
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read plugin config: {:?}", config_path))?;
        let config: PluginConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse plugin config: {:?}", config_path))?;
        let mut loaded = Vec::new();
        let mut loaded_paths = HashMap::new();
        let mut hook_map: HashMap<PluginHook, Vec<usize>> = HashMap::new();
        for entry in config.plugins.values() {
            let path = Self::resolve_path(&entry.path);
            match PythonPlugin::load(&path) {
                Ok(plugin) => {
                    let mut plugin = plugin.with_name(entry.name.clone());
                    plugin.aliases = entry.aliases.clone();
                    let idx = loaded.len();
                    loaded_paths.insert(plugin.path.clone(), idx);
                    loaded.push(plugin);
                    for hook in ALL_HOOKS.iter() {
                        hook_map.entry(*hook).or_default().push(idx);
                    }
                }
                Err(e) => {
                    eprintln!("Warning: failed to load plugin '{}' from {:?}: {}", entry.name, path, e);
                }
            }
        }
        let mut inner = self.inner.write().expect("plugin inner write");
        inner.plugins = loaded;
        inner.loaded_paths = loaded_paths;
        inner.hook_map = hook_map;
        Ok(())
    }

    fn scan_plugins_dir(&self, root: &Path) -> Result<()> {
        let plugins_dir = root.join(constants::PATH_PLUGINS);
        if !plugins_dir.exists() {
            fs::create_dir_all(&plugins_dir)?;
            return Ok(());
        }
        let mut loaded = Vec::new();
        let mut loaded_paths = HashMap::new();
        let mut hook_map: HashMap<PluginHook, Vec<usize>> = HashMap::new();

        for entry in fs::read_dir(&plugins_dir)? {
            let entry = entry?;
            let path = entry.path();
            let plugin_path = if path.is_dir() {
                let init = path.join("__init__.py");
                if init.exists() { Some(init) } else { None }
            } else if path.extension().map(|e| e == "py").unwrap_or(false) {
                Some(path.clone())
            } else {
                None
            };
            if let Some(py_path) = plugin_path {
                match PythonPlugin::load(&py_path) {
                    Ok(plugin) => {
                        let idx = loaded.len();
                        loaded_paths.insert(plugin.path.clone(), idx);
                        loaded.push(plugin);
                        for hook in ALL_HOOKS.iter() {
                            hook_map.entry(*hook).or_default().push(idx);
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to load plugin from {:?}: {}", path, e);
                    }
                }
            }
        }
        let mut inner = self.inner.write().expect("plugin inner write");
        inner.plugins = loaded;
        inner.loaded_paths = loaded_paths;
        inner.hook_map = hook_map;
        Ok(())
    }

    fn resolve_path(path_str: &str) -> PathBuf {
        if path_str.starts_with("~/")
            && let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join(&path_str[2..]);
            }
        PathBuf::from(path_str)
    }

    pub fn reload(&self, root: &Path) {
        let _ = self.load_all(root);
    }

    pub fn reload_from_config(&self, root: &Path) -> Result<usize> {
        let config_path = root.join(constants::PLUGIN_CONFIG_FILE);
        if !config_path.exists() {
            anyhow::bail!("Plugin config not found: {:?}", config_path);
        }
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read plugin config: {:?}", config_path))?;
        let config: PluginConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse plugin config: {:?}", config_path))?;
        let mut new_count = 0;
        let mut inner = self.inner.write().expect("plugin inner write");
        for entry in config.plugins.values() {
            let path = Self::resolve_path(&entry.path);
            if inner.loaded_paths.contains_key(&path) {
                continue;
            }
            match PythonPlugin::load(&path) {
                Ok(plugin) => {
                    let mut plugin = plugin.with_name(entry.name.clone());
                    plugin.aliases = entry.aliases.clone();
                    let idx = inner.plugins.len();
                    inner.loaded_paths.insert(plugin.path.clone(), idx);
                    inner.plugins.push(plugin);
                    for hook in ALL_HOOKS.iter() {
                        inner.hook_map.entry(*hook).or_default().push(idx);
                    }
                    new_count += 1;
                }
                Err(e) => {
                    eprintln!("Warning: failed to load plugin '{}' from {:?}: {}", entry.name, path, e);
                }
            }
        }
        Ok(new_count)
    }

    pub fn list(&self) -> Vec<PythonPlugin> {
        let inner = self.inner.read().expect("plugin inner read");
        inner.plugins.clone()
    }

    pub fn find(&self, name: &str) -> Option<PythonPlugin> {
        let inner = self.inner.read().expect("plugin inner read");
        inner.plugins.iter().find(|p| p.name() == name).cloned()
    }

    pub fn fire_hook(&self, hook: PluginHook, event: &PluginEvent) {
        let indices: Vec<usize>;
        {
            let inner = self.inner.read().expect("plugin inner read");
            indices = inner.hook_map.get(&hook).cloned().unwrap_or_default();
        }
        for idx in indices {
            if let Ok(inner) = self.inner.read()
                && let Some(plugin) = inner.plugins.get(idx) {
                    match plugin.run_hook(event) {
                        Ok(result) => {
                            if !result.success {
                                eprintln!("Plugin '{}' failed: {}", plugin.name(),
                                    result.message.as_deref().unwrap_or("unknown error"));
                            }
                        }
                        Err(e) => {
                            eprintln!("Plugin '{}' error: {}", plugin.name(), e);
                        }
                    }
                }
        }
    }

    pub fn run_plugin_once(&self, name: &str, event: &PluginEvent) -> Result<PluginResult> {
        let plugin = self.find(name).ok_or_else(|| anyhow!("Plugin '{}' not found", name))?;
        plugin.run_hook(event)
    }
}

const ALL_HOOKS: &[PluginHook] = &[
    PluginHook::PreInstall,
    PluginHook::PostInstall,
    PluginHook::PreRemove,
    PluginHook::PostRemove,
    PluginHook::PreVerify,
    PluginHook::PostVerify,
    PluginHook::PreFix,
    PluginHook::PostFix,
];

#[cfg(test)]
mod tests {
    use super::*;

    // ── PluginHook ───────────────────────────────────────────────────────────

    #[test]
    fn test_plugin_hook_roundtrip() {
        let hooks = [
            ("pre-install", PluginHook::PreInstall),
            ("post-install", PluginHook::PostInstall),
            ("pre-remove", PluginHook::PreRemove),
            ("post-remove", PluginHook::PostRemove),
            ("pre-verify", PluginHook::PreVerify),
            ("post-verify", PluginHook::PostVerify),
            ("pre-fix", PluginHook::PreFix),
            ("post-fix", PluginHook::PostFix),
        ];
        for (s, expected) in &hooks {
            assert_eq!(PluginHook::from_str(s), Some(*expected));
            assert_eq!(expected.as_str(), *s);
        }
    }

    #[test]
    fn test_plugin_hook_unknown_returns_none() {
        assert_eq!(PluginHook::from_str("pre-build"), None);
        assert_eq!(PluginHook::from_str(""), None);
        assert_eq!(PluginHook::from_str("install"), None);
    }

    #[test]
    fn test_plugin_hook_normalizes_dashes_and_underscores() {
        assert_eq!(PluginHook::from_str("pre_install"), Some(PluginHook::PreInstall));
        assert_eq!(PluginHook::from_str("post_install"), Some(PluginHook::PostInstall));
    }

    #[test]
    fn test_plugin_hook_function_names() {
        assert_eq!(PluginHook::PreInstall.function_name(), "on_pre_install");
        assert_eq!(PluginHook::PostInstall.function_name(), "on_post_install");
        assert_eq!(PluginHook::PreRemove.function_name(), "on_pre_remove");
        assert_eq!(PluginHook::PostFix.function_name(), "on_post_fix");
    }

    // ── PythonPlugin::load ───────────────────────────────────────────────────

    #[test]
    fn test_python_plugin_load_valid_syntax() {
        let dir = std::env::temp_dir().join(format!("mcx_test_pyplugin_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("ok.py"), "x = 1\nprint(x)\n").expect("write temp plugin");
        let plugin = PythonPlugin::load(&dir.join("ok.py")).expect("load test plugin");
        assert_eq!(plugin.name(), "ok");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_python_plugin_load_invalid_syntax() {
        let dir = std::env::temp_dir().join(format!("mcx_test_pyplugin_bad_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("bad.py"), "def foo(\n").expect("write temp plugin");
        let err = PythonPlugin::load(&dir.join("bad.py")).expect_err("load invalid plugin");
        assert!(err.to_string().contains("syntax error"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_python_plugin_load_nonexistent() {
        let err = PythonPlugin::load(Path::new("/nonexistent/plugin.py")).expect_err("load missing plugin");
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_python_plugin_load_arbitrary_code() {
        let dir = std::env::temp_dir().join(format!("mcx_test_pyplugin_any_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("anything.py"), r#"
import os, sys, json
class MyTool:
    def run(self):
        return "hello"
"#).expect("write temp plugin");
        let plugin = PythonPlugin::load(&dir.join("anything.py")).expect("load test plugin");
        assert_eq!(plugin.name(), "anything");
        let _ = fs::remove_dir_all(&dir);
    }

    // ── PythonPlugin::run ────────────────────────────────────────────────────

    #[test]
    fn test_python_plugin_run_executes_code() {
        let dir = std::env::temp_dir().join(format!("mcx_test_pyrun_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("runner.py"), "print('plugin-output')\n").expect("write temp plugin");
        let plugin = PythonPlugin::load(&dir.join("runner.py")).expect("load test plugin");
        let result = plugin.run("{}").expect("run test plugin");
        assert!(result.success);
        assert!(result.message.as_deref().expect("plugin message present").contains("plugin-output"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_python_plugin_run_receives_event() {
        let dir = std::env::temp_dir().join(format!("mcx_test_pyrun_evt_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("evt.py"), r#"
import json
print(json.dumps({"hook_received": MCX_EVENT.get("hook", "none")}))
"#).expect("write temp plugin");
        let plugin = PythonPlugin::load(&dir.join("evt.py")).expect("load test plugin");
        let event_json = r#"{"hook": "post-install", "package": null, "root": "/test", "timestamp": "2026-01-01T00:00:00Z"}"#;
        let result = plugin.run(event_json).expect("run test plugin");
        assert!(result.success);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_python_plugin_run_syntax_error() {
        let dir = std::env::temp_dir().join(format!("mcx_test_pyrun_err_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("err.py"), "raise ValueError('intentional')\n").expect("write temp plugin");
        let plugin = PythonPlugin::load(&dir.join("err.py")).expect("load test plugin");
        let result = plugin.run("{}").expect("run test plugin");
        assert!(!result.success);
        assert!(result.message.as_deref().expect("plugin message present").contains("intentional"));
        let _ = fs::remove_dir_all(&dir);
    }

    // ── PluginConfig TOML parsing ────────────────────────────────────────────

    #[test]
    fn test_plugin_config_parse() {
        let toml_str = r#"
[plugin.1]
name = "Plugin One"
path = "/tmp/plugin1.py"

[plugin.2]
name = "Plugin Two"
path = "~/plugins/plugin2.py"
"#;
        let config: PluginConfig = toml::from_str(toml_str).expect("parse plugin config");
        assert_eq!(config.plugins.len(), 2);
        let p1 = config.plugins.get("1").expect("plugin 1 present");
        assert_eq!(p1.name, "Plugin One");
        assert_eq!(p1.path, "/tmp/plugin1.py");
        let p2 = config.plugins.get("2").expect("plugin 2 present");
        assert_eq!(p2.name, "Plugin Two");
        assert_eq!(p2.path, "~/plugins/plugin2.py");
    }

    #[test]
    fn test_plugin_config_single_plugin() {
        let toml_str = r#"
[plugin.mine]
name = "My Plugin"
path = "/opt/plugins/mine.py"
"#;
        let config: PluginConfig = toml::from_str(toml_str).expect("parse plugin config");
        assert_eq!(config.plugins.len(), 1);
        let p = config.plugins.get("mine").expect("plugin mine present");
        assert_eq!(p.name, "My Plugin");
    }

    #[test]
    fn test_resolve_path_home() {
        let home = std::env::var("HOME").unwrap_or_default();
        let resolved = PluginManager::resolve_path("~/test.py");
        assert_eq!(resolved, PathBuf::from(home).join("test.py"));
    }

    #[test]
    fn test_resolve_path_absolute() {
        let resolved = PluginManager::resolve_path("/opt/plugins/test.py");
        assert_eq!(resolved, PathBuf::from("/opt/plugins/test.py"));
    }

    // ── PluginManager with config ────────────────────────────────────────────

    #[test]
    fn test_plugin_manager_load_from_config() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_cfg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let config_dir = root.join("etc/mcx");
        fs::create_dir_all(&config_dir).expect("create config dir");

        let plugin_dir = root.join("var/lib/mcx/plugins_test");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        fs::write(plugin_dir.join("p1.py"), "print('hello')\n").expect("write temp plugin");
        fs::write(plugin_dir.join("p2.py"), "print('world')\n").expect("write temp plugin");

        let p1_path = plugin_dir.join("p1.py").to_string_lossy().to_string();
        let p2_path = plugin_dir.join("p2.py").to_string_lossy().to_string();
        let toml_content = format!(
            "[plugin.a]\nname = \"Plugin A\"\npath = \"{}\"\nls = \"ls -la\"\nupdate = \"mcx -u && mcx -U\"\n\n[plugin.b]\nname = \"Plugin B\"\npath = \"{}\"\n",
            p1_path, p2_path
        );
        fs::write(config_dir.join("p.desc"), &toml_content).expect("write plugin config");

        let mgr = PluginManager::new(&root);
        mgr.reload_from_config(&root).expect("reload plugin config");
        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.name() == "Plugin A"));
        assert!(list.iter().any(|p| p.name() == "Plugin B"));
        let plugin_a = list.iter().find(|p| p.name() == "Plugin A").expect("plugin A present");
        assert_eq!(plugin_a.aliases.get("ls").expect("ls alias present"), "ls -la");
        assert_eq!(plugin_a.aliases.get("update").expect("update alias present"), "mcx -u && mcx -U");
        assert!(!plugin_a.aliases.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_plugin_manager_reload_from_config() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_rld_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let config_dir = root.join("etc/mcx");
        fs::create_dir_all(&config_dir).expect("create config dir");

        let plugin_dir = root.join("var/lib/mcx/plugins_test");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        fs::write(plugin_dir.join("a.py"), "print('a')\n").expect("write temp plugin");

        let a_path = plugin_dir.join("a.py").to_string_lossy().to_string();
        fs::write(config_dir.join("p.desc"),
            format!("[plugin.a]\nname = \"A\"\npath = \"{}\"\n", a_path)).expect("write plugin config");

        let mgr = PluginManager::new(&root);
        assert_eq!(mgr.list().len(), 1);

        fs::write(plugin_dir.join("b.py"), "print('b')\n").expect("write temp plugin");
        let b_path = plugin_dir.join("b.py").to_string_lossy().to_string();
        fs::write(config_dir.join("p.desc"),
            format!(
                "[plugin.a]\nname = \"A\"\npath = \"{}\"\n\n[plugin.b]\nname = \"B\"\npath = \"{}\"\n",
                a_path, b_path
            )).expect("write plugin config");

        let new_count = mgr.reload_from_config(&root).expect("reload plugin config");
        assert_eq!(new_count, 1);
        assert_eq!(mgr.list().len(), 2);

        let _ = fs::remove_dir_all(&root);
    }

    // ── PluginManager backward compat (scan dir) ─────────────────────────────

    #[test]
    fn test_plugin_manager_scan_plugins_dir() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_scan_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let plugins_dir = root.join(constants::PATH_PLUGINS);
        fs::create_dir_all(&plugins_dir).expect("create plugins dir");
        fs::write(plugins_dir.join("p1.py"), "print('p1')\n").expect("write temp plugin");
        fs::write(plugins_dir.join("p2.py"), "print('p2')\n").expect("write temp plugin");

        let mgr = PluginManager::new(&root);
        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.name() == "p1"));
        assert!(list.iter().any(|p| p.name() == "p2"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_plugin_manager_fire_hook() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_hook_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let plugins_dir = root.join(constants::PATH_PLUGINS);
        fs::create_dir_all(&plugins_dir).expect("create plugins dir");
        fs::write(plugins_dir.join("h.py"), "print('hooked')\n").expect("write temp plugin");

        let mgr = PluginManager::new(&root);
        let event = PluginEvent {
            hook: "post-install".into(),
            package: Some("curl".into()),
            root: root.to_string_lossy().into_owned(),
            timestamp: "now".into(),
        };
        mgr.fire_hook(PluginHook::PostInstall, &event);
        mgr.fire_hook(PluginHook::PreRemove, &event);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_plugin_manager_find() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_find_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let plugins_dir = root.join(constants::PATH_PLUGINS);
        fs::create_dir_all(&plugins_dir).expect("create plugins dir");
        fs::write(plugins_dir.join("findme.py"), "print('found')\n").expect("write temp plugin");

        let mgr = PluginManager::new(&root);
        assert!(mgr.find("findme").is_some());
        assert!(mgr.find("nonexistent").is_none());

        let _ = fs::remove_dir_all(&root);
    }
}
