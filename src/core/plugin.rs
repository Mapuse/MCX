use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::RwLock;
use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use super::constants;


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

/// Runs a command to completion with a hard timeout. Input is written to the
/// child's stdin; stdout/stderr are drained concurrently so large outputs
/// cannot deadlock the pipe buffers. On expiry the child is killed.
fn run_with_timeout(cmd: &mut Command, input: &[u8], timeout: std::time::Duration) -> Result<std::process::Output> {
    use std::io::{Read, Write};
    use std::time::Instant;

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn {:?}", cmd.get_program()))?;
    let mut stdin = child.stdin.take().context("child has no stdin")?;
    let mut stdout = child.stdout.take().context("child has no stdout")?;
    let mut stderr = child.stderr.take().context("child has no stderr")?;

    std::thread::scope(|scope| -> Result<std::process::Output> {
        scope.spawn(move || {
            let _ = stdin.write_all(input);
            // stdin dropped here, closing the pipe
        });
        let out_thread = scope.spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            buf
        });
        let err_thread = scope.spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf);
            buf
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait()? {
                Some(status) => break status,
                None => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        anyhow::bail!("Plugin execution timed out after {}s", timeout.as_secs());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }
        };

        let stdout_buf = out_thread.join().unwrap_or_default();
        let stderr_buf = err_thread.join().unwrap_or_default();
        Ok(std::process::Output { status, stdout: stdout_buf, stderr: stderr_buf })
    })
}

#[derive(Debug, Clone)]
pub struct PythonPlugin {
    name: String,
    pub path: PathBuf,
    pub aliases: HashMap<String, String>,
    /// Hooks this plugin actually implements (probed via AST at load time).
    hooks: std::collections::HashSet<PluginHook>,
}

impl PythonPlugin {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!("Plugin file not found: {:?}", path));
        }

        // Syntax check + hook discovery in one interpreter run: parse the
        // file's AST (no code execution) and list defined function names.
        // Paths are passed as argv — never interpolated into script text.
        let probe_script = concat!(
            "import ast, sys\n",
            "src = open(sys.argv[1], encoding='utf-8').read()\n",
            "tree = ast.parse(src)\n",
            "names = sorted({n.name for n in ast.walk(tree) if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))})\n",
            "print('\\n'.join(names))\n",
        );
        let mut probe_cmd = Command::new(constants::TOOL_PYTHON3);
        probe_cmd.arg("-c").arg(probe_script).arg(path);
        let output = run_with_timeout(
            &mut probe_cmd,
            b"",
            std::time::Duration::from_secs(constants::DEFAULT_PLUGIN_TIMEOUT_SECS),
        )
        .with_context(|| format!("Failed to run python3 for syntax check of {:?}", path))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Python syntax error in {:?}: {}", path, stderr.trim());
        }
        let defined: std::collections::HashSet<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        let hooks = ALL_HOOKS
            .iter()
            .filter(|h| defined.contains(h.function_name()))
            .copied()
            .collect();

        let file_stem = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(Self {
            name: file_stem,
            path: path.to_path_buf(),
            aliases: HashMap::new(),
            hooks,
        })
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn name(&self) -> &str { &self.name }
    pub fn path(&self) -> &Path { &self.path }

    /// The set of hooks this plugin implements.
    pub fn hooks(&self) -> &std::collections::HashSet<PluginHook> { &self.hooks }

    /// True when the plugin defines the handler for `hook`.
    pub fn supports_hook(&self, hook: PluginHook) -> bool {
        self.hooks.contains(&hook)
    }

    pub fn run(&self, event_json: &str) -> Result<PluginResult> {
        // The runner reads the event from stdin and takes all paths via argv;
        // nothing user-controlled is ever interpolated into script text.
        const RUNNER: &str = concat!(
            "import json, sys\n",
            "MCX_EVENT = json.load(sys.stdin)\n",
            "sys.path.insert(0, sys.argv[1])\n",
            "exec(compile(open(sys.argv[2], encoding='utf-8').read(), sys.argv[2], 'exec'))\n",
        );
        let plugin_dir = self.path.parent().unwrap_or(Path::new(".")).to_string_lossy().to_string();

        let mut cmd = Command::new(constants::TOOL_PYTHON3);
        cmd.arg("-c")
            .arg(RUNNER)
            .arg(&*plugin_dir)
            .arg(&self.path)
            .current_dir(self.path.parent().unwrap_or(Path::new(".")));
        let output = run_with_timeout(
            &mut cmd,
            event_json.as_bytes(),
            std::time::Duration::from_secs(constants::DEFAULT_PLUGIN_TIMEOUT_SECS),
        )?;

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
        if let Some(hook) = PluginHook::from_str(&event.hook)
            && !self.supports_hook(hook)
        {
            anyhow::bail!(
                "Plugin '{}' does not implement {} (no on_{}_ function)",
                self.name,
                hook.as_str(),
                hook.as_str().replace('-', "_")
            );
        }
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
                    for hook in plugin.hooks() {
                        hook_map.entry(*hook).or_default().push(idx);
                    }
                    loaded.push(plugin);
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
                        for hook in plugin.hooks() {
                            hook_map.entry(*hook).or_default().push(idx);
                        }
                        loaded.push(plugin);
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
                    for hook in plugin.hooks() {
                        inner.hook_map.entry(*hook).or_default().push(idx);
                    }
                    inner.plugins.push(plugin);
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

    fn python3_available() -> bool {
        Command::new(constants::TOOL_PYTHON3)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    // ── run_with_timeout ─────────────────────────────────────────────────────

    #[test]
    fn test_run_with_timeout_kills_hung_child() {
        if !python3_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let mut cmd = Command::new(constants::TOOL_PYTHON3);
        cmd.arg("-c").arg("import time; time.sleep(60)");
        let started = std::time::Instant::now();
        let result = run_with_timeout(&mut cmd, b"", std::time::Duration::from_millis(300));
        assert!(result.is_err(), "hung child must be killed and reported");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    // ── PythonPlugin::load (AST probe) ──────────────────────────────────────

    #[test]
    fn test_plugin_load_probes_implemented_hooks_only() {
        if !python3_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_probe_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let py = dir.join("probe.py");
        fs::write(
            &py,
            "def on_pre_install(event):\n    return None\n\ndef helper():\n    pass\n",
        )
        .unwrap();

        let plugin = PythonPlugin::load(&py).expect("load plugin");
        assert!(plugin.supports_hook(PluginHook::PreInstall));
        assert!(!plugin.supports_hook(PluginHook::PostInstall), "unimplemented hooks must not be registered");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_plugin_load_rejects_syntax_errors() {
        if !python3_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_syntax_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let py = dir.join("broken.py");
        fs::write(&py, "def broken(:\n    pass\n").unwrap();
        assert!(PythonPlugin::load(&py).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    // ── PythonPlugin::run (stdin event) ─────────────────────────────────────

    #[test]
    fn test_plugin_run_receives_event_via_stdin() {
        if !python3_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_run_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let py = dir.join("echo.py");
        fs::write(
            &py,
            concat!(
                "import json\n",
                "print(json.dumps({'success': True, 'message': MCX_EVENT['package']}))\n",
            ),
        )
        .unwrap();
        let plugin = PythonPlugin::load(&py).expect("load plugin");

        let result = plugin
            .run(r#"{"package": "pkg-with-'quote'", "hook": "post-install", "root": "/", "timestamp": "t"}"#)
            .expect("run plugin");
        assert!(result.success);
        assert_eq!(result.message.as_deref(), Some("pkg-with-'quote'"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_run_hook_refuses_unimplemented_hook() {
        if !python3_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let dir = std::env::temp_dir().join(format!("mcx_test_plugin_refuse_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let py = dir.join("noop.py");
        fs::write(&py, "x = 1\n").unwrap();
        let plugin = PythonPlugin::load(&py).expect("load plugin");
        let event = PluginEvent {
            hook: "pre-install".to_string(),
            package: None,
            root: "/".to_string(),
            timestamp: "t".to_string(),
        };
        let err = plugin.run_hook(&event).expect_err("missing handler must be refused");
        assert!(err.to_string().contains("does not implement"));
        let _ = fs::remove_dir_all(&dir);
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
