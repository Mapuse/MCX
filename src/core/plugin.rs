use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::RwLock;
use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use crate::core::arch::Architecture;

// ── Builtin plugin traits ─────────────────────────────────────────────────

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
        self.inner.read().unwrap().clone()
    }

    pub fn swap(&self, new_plugin: Arc<T>) -> Arc<T> {
        let mut guard = self.inner.write().unwrap();
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
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("curl -fSL -o /dev/stdout '{}' | tar -xz --strip-components=1 -C '{}'", source, destination))
            .status()?;
        if !status.success() {
            let status2 = Command::new("git").arg("clone").arg("--depth").arg("1").arg(source).arg(destination).status()?;
            if !status2.success() { anyhow::bail!("Failed to fetch source: {}", source); }
        }
        Ok(())
    }
    fn name(&self) -> &'static str { "curl" }
}

pub struct DefaultBuilder;
impl Builder for DefaultBuilder {
    fn build(&self, build_cmd: &str, source_dir: &str, _dest_dir: &str, build_type: &str) -> Result<String> {
        if build_cmd.trim().eq_ignore_ascii_case("none") || build_cmd.trim().eq_ignore_ascii_case("skip") || build_cmd.trim().eq_ignore_ascii_case("nothing") {
            return Ok(String::new());
        }
        if build_cmd.is_empty() && build_type == "rust" {
            let target = std::env::var("CUDANE_TARGET")
                .unwrap_or_else(|_| Architecture::host().target_triple().to_string());
            let rust_target = std::env::var("CUDANE_RUST_TARGET")
                .unwrap_or_else(|_| target.clone());
            let rustflags = format!(
                "RUSTFLAGS=\"-C linker=clang -C link-arg=-target -C link-arg={} \
                 -C link-arg=--sysroot=/system -C target-feature=+crt-static\" \
                 cargo build --target {} --release 2>&1",
                target, rust_target
            );
            let output = Command::new("sh")
                .arg("-c")
                .arg(&rustflags)
                .current_dir(source_dir).output()?;
            let log = String::from_utf8_lossy(&output.stdout).to_string();
            if !output.status.success() { anyhow::bail!("Build failed:\n{}", log); }
            return Ok(log);
        }
        if !build_cmd.is_empty() {
            let env_target = std::env::var("CUDANE_TARGET").unwrap_or_default();
            let cmd = if env_target.is_empty() {
                format!("({}) 2>&1", build_cmd)
            } else {
                format!("CUDANE_TARGET={} ({}) 2>&1", env_target, build_cmd)
            };
            let output = Command::new("sh").arg("-c").arg(&cmd).current_dir(source_dir).output()?;
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
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("tar -c -C '{}' . | zstd -{} -o '{}'", source_dir, compression_level, output_path))
            .status()?;
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

// ── External plugin system ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub language: String,
    #[serde(rename = "type")]
    pub plugin_type: String,
    pub command: String,
    pub trigger: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub timeout_secs: Option<u64>,
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

impl PluginHook {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "pre_install" => Some(Self::PreInstall),
            "post_install" => Some(Self::PostInstall),
            "pre_remove" => Some(Self::PreRemove),
            "post_remove" => Some(Self::PostRemove),
            "pre_verify" => Some(Self::PreVerify),
            "post_verify" => Some(Self::PostVerify),
            "pre_fix" => Some(Self::PreFix),
            "post_fix" => Some(Self::PostFix),
            _ => None,
        }
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

#[derive(Debug, Clone)]
pub struct ExternalPlugin {
    manifest: PluginManifest,
    dir: PathBuf,
}

impl ExternalPlugin {
    pub fn load(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join("plugin.ini");
        if !manifest_path.exists() {
            return Err(anyhow!("No plugin.ini in {:?}", dir));
        }
        let content = fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {:?}", manifest_path))?;
        let manifest = parse_plugin_ini(&content)
            .with_context(|| format!("Failed to parse plugin manifest in {:?}", dir))?;
        Ok(Self { manifest, dir: dir.to_path_buf() })
    }

    pub fn manifest(&self) -> &PluginManifest { &self.manifest }

    pub fn run_hook(&self, event: &PluginEvent) -> Result<PluginResult> {
        let _timeout = self.manifest.timeout_secs.unwrap_or(30);
        let expanded = self.expand_command(event);
        let output = Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
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

    fn expand_command(&self, event: &PluginEvent) -> String {
        let cmd = &self.manifest.command;
        let event_json = serde_json::to_string(event).unwrap_or_default();
        cmd.replace("${event}", &event_json)
            .replace("${root}", &event.root)
            .replace("${package}", event.package.as_deref().unwrap_or(""))
            .replace("${hook}", &event.hook)
            .replace("${dir}", self.dir.to_string_lossy().as_ref())
    }
}

fn parse_plugin_ini(content: &str) -> Result<PluginManifest> {
    let mut name = String::new();
    let mut version = String::new();
    let mut description = String::new();
    let mut language = String::new();
    let mut plugin_type = String::new();
    let mut command = String::new();
    let mut trigger = None;
    let mut author = None;
    let mut homepage = None;
    let mut timeout_secs = None;

    let mut in_plugin = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_plugin = line.eq_ignore_ascii_case("[plugin]");
            continue;
        }
        if !in_plugin { continue; }
        if let Some((key, value)) = line.split_once('=') {
            let k = key.trim().to_lowercase();
            let v = value.trim().to_string();
            match k.as_str() {
                "name" => name = v,
                "version" => version = v,
                "description" => description = v,
                "language" => language = v,
                "type" => plugin_type = v,
                "command" => command = v,
                "trigger" => trigger = Some(v),
                "author" => author = Some(v),
                "homepage" => homepage = Some(v),
                "timeout_secs" => timeout_secs = v.parse::<u64>().ok(),
                _ => {}
            }
        }
    }

    if name.is_empty() || command.is_empty() {
        return Err(anyhow!("plugin.ini must have 'name' and 'command' fields"));
    }

    Ok(PluginManifest {
        name, version, description, language, plugin_type, command,
        trigger, author, homepage, timeout_secs,
    })
}

// ── PluginManager ─────────────────────────────────────────────────────────────

pub struct PluginManager {
    inner: RwLock<PluginManagerInner>,
}

struct PluginManagerInner {
    plugins: Vec<ExternalPlugin>,
    hook_map: HashMap<PluginHook, Vec<usize>>,
}

impl PluginManager {
    pub fn new(root: &Path) -> Self {
        let mgr = Self { inner: RwLock::new(PluginManagerInner { plugins: Vec::new(), hook_map: HashMap::new() }) };
        let _ = mgr.load_all(root);
        mgr
    }

    fn load_all(&self, root: &Path) -> Result<()> {
        let plugins_dir = root.join("var/lib/mcx/plugins");
        if !plugins_dir.exists() {
            fs::create_dir_all(&plugins_dir)?;
            return Ok(());
        }
        let mut loaded = Vec::new();
        let mut hook_map: HashMap<PluginHook, Vec<usize>> = HashMap::new();
        for entry in fs::read_dir(&plugins_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                match ExternalPlugin::load(&path) {
                    Ok(plugin) => {
                        let idx = loaded.len();
                        if let Some(ref trigger_str) = plugin.manifest().trigger {
                            if let Some(hook) = PluginHook::from_str(trigger_str) {
                                hook_map.entry(hook).or_default().push(idx);
                            }
                        }
                        loaded.push(plugin);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to load plugin from {:?}: {}", path, e);
                    }
                }
            }
        }
        let mut inner = self.inner.write().unwrap();
        inner.plugins = loaded;
        inner.hook_map = hook_map;
        Ok(())
    }

    pub fn reload(&self, root: &Path) {
        let _ = self.load_all(root);
    }

    pub fn list(&self) -> Vec<ExternalPlugin> {
        let inner = self.inner.read().unwrap();
        inner.plugins.clone()
    }

    pub fn find(&self, name: &str) -> Option<ExternalPlugin> {
        let inner = self.inner.read().unwrap();
        inner.plugins.iter().find(|p| p.manifest().name == name).cloned()
    }

    pub fn fire_hook(&self, hook: PluginHook, event: &PluginEvent) {
        let indices: Vec<usize>;
        {
            let inner = self.inner.read().unwrap();
            indices = inner.hook_map.get(&hook).cloned().unwrap_or_default();
        }
        for idx in indices {
            if let Ok(inner) = self.inner.read() {
                if let Some(plugin) = inner.plugins.get(idx) {
                    match plugin.run_hook(event) {
                        Ok(result) => {
                            if !result.success {
                                eprintln!("Plugin '{}' failed: {}", plugin.manifest().name,
                                    result.message.as_deref().unwrap_or("unknown error"));
                            }
                        }
                        Err(e) => {
                            eprintln!("Plugin '{}' error: {}", plugin.manifest().name, e);
                        }
                    }
                }
            }
        }
    }

    pub fn run_plugin_once(&self, name: &str, event: &PluginEvent) -> Result<PluginResult> {
        let plugin = self.find(name).ok_or_else(|| anyhow!("Plugin '{}' not found", name))?;
        plugin.run_hook(event)
    }

    pub fn daemon_plugins(&self) -> Vec<ExternalPlugin> {
        let inner = self.inner.read().unwrap();
        inner.plugins.iter().filter(|p| p.manifest().plugin_type == "daemon").cloned().collect()
    }

    pub fn start_daemon(&self, name: &str) -> Result<()> {
        let plugin = self.find(name).ok_or_else(|| anyhow!("Plugin '{}' not found", name))?;
        if plugin.manifest().plugin_type != "daemon" {
            return Err(anyhow!("Plugin '{}' is not a daemon plugin", name));
        }
        let expanded = plugin.expand_command(&PluginEvent {
            hook: "daemon-start".into(),
            package: None,
            root: String::new(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .current_dir(&plugin.dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let _ = child.stdout.take();
        let _ = child.stderr.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_plugin_ini ─────────────────────────────────────────────────────

    #[test]
    fn test_parse_minimal_plugin_ini() {
        let content = "[plugin]\nname = test-pkg\ncommand = echo hello\n";
        let m = parse_plugin_ini(content).unwrap();
        assert_eq!(m.name, "test-pkg");
        assert_eq!(m.command, "echo hello");
        assert_eq!(m.version, "");
        assert_eq!(m.language, "");
        assert!(m.trigger.is_none());
    }

    #[test]
    fn test_parse_full_plugin_ini() {
        let content = "\
[plugin]
name = my-plugin
version = 2.1.0
description = Test plugin
language = python
type = daemon
command = python3 ${dir}/main.py
trigger = post-install
author = Alice
homepage = https://example.com
timeout_secs = 60
";
        let m = parse_plugin_ini(content).unwrap();
        assert_eq!(m.name, "my-plugin");
        assert_eq!(m.version, "2.1.0");
        assert_eq!(m.description, "Test plugin");
        assert_eq!(m.language, "python");
        assert_eq!(m.plugin_type, "daemon");
        assert_eq!(m.command, "python3 ${dir}/main.py");
        assert_eq!(m.trigger.as_deref(), Some("post-install"));
        assert_eq!(m.author.as_deref(), Some("Alice"));
        assert_eq!(m.homepage.as_deref(), Some("https://example.com"));
        assert_eq!(m.timeout_secs, Some(60));
    }

    #[test]
    fn test_parse_plugin_ini_missing_required_fields() {
        let content = "[plugin]\nname = \ncommand = \n";
        let err = parse_plugin_ini(content).unwrap_err();
        assert!(err.to_string().contains("plugin.ini must have 'name' and 'command' fields"));
    }

    #[test]
    fn test_parse_plugin_ini_no_section() {
        let content = "name = orphan\ncommand = run\n";
        let err = parse_plugin_ini(content).unwrap_err();
        assert!(err.to_string().contains("plugin.ini must have 'name' and 'command' fields"));
    }

    #[test]
    fn test_parse_plugin_ini_case_insensitive_section() {
        let content = "[PLUGIN]\nNAME = caps-test\nCOMMAND = echo ok\n";
        let m = parse_plugin_ini(content).unwrap();
        assert_eq!(m.name, "caps-test");
        assert_eq!(m.command, "echo ok");
    }

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

    // ── ExternalPlugin::load ─────────────────────────────────────────────────

    #[test]
    fn test_external_plugin_load_success() {
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_load_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.ini"), "[plugin]\nname = unit-test\ncommand = echo ok\n").unwrap();

        let plugin = ExternalPlugin::load(&dir).unwrap();
        assert_eq!(plugin.manifest().name, "unit-test");
        assert_eq!(plugin.manifest().command, "echo ok");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_external_plugin_load_missing_ini() {
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_noini_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let err = ExternalPlugin::load(&dir).unwrap_err();
        assert!(err.to_string().contains("No plugin.ini"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_external_plugin_expand_command() {
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_expand_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.ini"), "[plugin]\nname = expand-test\ncommand = ${root} ${hook} ${package} ${event} ${dir}\n").unwrap();

        let plugin = ExternalPlugin::load(&dir).unwrap();
        let event = PluginEvent {
            hook: "test-hook".into(),
            package: Some("test-pkg".into()),
            root: "/test-root".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
        };
        let expanded = plugin.expand_command(&event);
        assert!(expanded.contains("/test-root"));
        assert!(expanded.contains("test-hook"));
        assert!(expanded.contains("test-pkg"));
        assert!(expanded.contains(&dir.to_string_lossy().to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_external_plugin_run_success() {
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_run_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.ini"), "[plugin]\nname = runner\ncommand = echo '{\"success\":true,\"message\":\"done\"}'\n").unwrap();
        let plugin = ExternalPlugin::load(&dir).unwrap();
        let event = PluginEvent {
            hook: "test".into(),
            package: None,
            root: String::new(),
            timestamp: String::new(),
        };
        let result = plugin.run_hook(&event).unwrap();
        assert!(result.success);
        assert_eq!(result.message.as_deref(), Some("done"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_external_plugin_run_failure_via_exit_code() {
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_fail_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.ini"), "[plugin]\nname = failer\ncommand = exit 1\n").unwrap();
        let plugin = ExternalPlugin::load(&dir).unwrap();
        let event = PluginEvent {
            hook: "test".into(),
            package: None,
            root: String::new(),
            timestamp: String::new(),
        };
        let result = plugin.run_hook(&event).unwrap();
        assert!(!result.success);
        let _ = fs::remove_dir_all(&dir);
    }

    // ── PluginManager ────────────────────────────────────────────────────────

    #[test]
    fn test_plugin_manager_load_and_list() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let plugins_dir = root.join("var/lib/mcx/plugins");
        fs::create_dir_all(&plugins_dir.join("p1")).unwrap();
        fs::write(plugins_dir.join("p1/plugin.ini"), "[plugin]\nname = p1\ncommand = echo 1\ntrigger = post-install\n").unwrap();
        fs::create_dir_all(&plugins_dir.join("p2")).unwrap();
        fs::write(plugins_dir.join("p2/plugin.ini"), "[plugin]\nname = p2\ncommand = echo 2\ntrigger = post-install\n").unwrap();

        let mgr = PluginManager::new(&root);
        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.manifest().name == "p1"));
        assert!(list.iter().any(|p| p.manifest().name == "p2"));

        let found = mgr.find("p1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().manifest().name, "p1");

        let missing = mgr.find("nonexistent");
        assert!(missing.is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_plugin_manager_reload() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_reload_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let plugins_dir = root.join("var/lib/mcx/plugins");
        fs::create_dir_all(&plugins_dir.join("a")).unwrap();
        fs::write(plugins_dir.join("a/plugin.ini"), "[plugin]\nname = a\ncommand = echo a\ntrigger = post-install\n").unwrap();

        let mgr = PluginManager::new(&root);
        assert_eq!(mgr.list().len(), 1);

        // Add a new plugin and reload
        fs::create_dir_all(&plugins_dir.join("b")).unwrap();
        fs::write(plugins_dir.join("b/plugin.ini"), "[plugin]\nname = b\ncommand = echo b\ntrigger = post-install\n").unwrap();
        mgr.reload(&root);
        assert_eq!(mgr.list().len(), 2);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_plugin_manager_fire_hook() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_hook_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let plugins_dir = root.join("var/lib/mcx/plugins");
        fs::create_dir_all(&plugins_dir.join("h")).unwrap();
        fs::write(plugins_dir.join("h/plugin.ini"), "[plugin]\nname = hooker\ncommand = echo ok\ntrigger = post-install\n").unwrap();

        let mgr = PluginManager::new(&root);
        let event = PluginEvent {
            hook: "post-install".into(),
            package: Some("curl".into()),
            root: root.to_string_lossy().into_owned(),
            timestamp: "now".into(),
        };
        // Should not panic; hooker is bound to post-install
        mgr.fire_hook(PluginHook::PostInstall, &event);
        // Unbound hook should be a no-op
        mgr.fire_hook(PluginHook::PreRemove, &event);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_plugin_manager_daemon_plugins() {
        let root = std::env::temp_dir().join(format!("mcx_test_mgr_daemon_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let plugins_dir = root.join("var/lib/mcx/plugins");
        fs::create_dir_all(&plugins_dir.join("hook-p")).unwrap();
        fs::write(plugins_dir.join("hook-p/plugin.ini"), "[plugin]\nname = hp\ncommand = echo hp\ntype = hook\n").unwrap();
        fs::create_dir_all(&plugins_dir.join("daemon-p")).unwrap();
        fs::write(plugins_dir.join("daemon-p/plugin.ini"), "[plugin]\nname = dp\ncommand = sleep 100\ntype = daemon\n").unwrap();

        let mgr = PluginManager::new(&root);
        let daemons = mgr.daemon_plugins();
        assert_eq!(daemons.len(), 1);
        assert_eq!(daemons[0].manifest().name, "dp");

        let _ = fs::remove_dir_all(&root);
    }
}
