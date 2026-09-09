use super::config::MappedConfig;
use super::constants;
use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Operating mode. `User` mode acts on the selected user's own root and
/// never elevates; `System` mode targets the system root and escalates via
/// sudo for mutating commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    User,
    System,
}

impl Mode {
    pub fn is_user(self) -> bool {
        matches!(self, Mode::User)
    }

    pub fn is_system(self) -> bool {
        matches!(self, Mode::System)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::User => "user",
            Mode::System => "system",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim() {
            "user" => Some(Mode::User),
            "system" => Some(Mode::System),
            _ => None,
        }
    }
}

/// Which user mcx operates for, persisted in its own separate config
/// (`~/.mcx/etc/mcx/user.ini`). Both `mode` and `user` are required fields;
/// the `user` field records the OS user who switched, and a selection is only
/// honored while that same OS user runs mcx. Keeping this file distinct from
/// the root-directory config means the selection can never silently target a
/// directory outside the user.
#[derive(Clone, Debug)]
pub struct UserSelection {
    pub mode: Mode,
    pub user: String,
}

/// Root-directory configuration, stored in the user home config
/// (`~/.mcx/etc/mcx/config.ini`) `[general]` section. Paths are kept as
/// authored (possibly relative or `~`-prefixed) and normalized at use time.
#[derive(Clone, Debug)]
pub struct ModeConfig {
    pub user_root: String,
    pub system_root: String,
}

impl Default for ModeConfig {
    fn default() -> Self {
        Self {
            user_root: constants::DEFAULT_USER_ROOT.to_string(),
            system_root: constants::DEFAULT_ROOT.to_string(),
        }
    }
}

impl ModeConfig {
    /// The normalized per-user root directory.
    pub fn effective_user_root(&self) -> PathBuf {
        normalize_root(Path::new(&self.user_root))
    }

    /// The normalized system root directory.
    pub fn effective_system_root(&self) -> PathBuf {
        normalize_root(Path::new(&self.system_root))
    }
}

/// Home directory for the current user, falling back to mcx's own `HOME`
/// substitute when the variable is unset.
pub fn home_dir() -> PathBuf {
    env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(constants::DEFAULT_HOME))
}

/// Current OS user name, from `USER` then `LOGNAME`.
pub fn current_user() -> String {
    env::var("USER")
        .or_else(|_| env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Path of the root-directory config (`config.ini`) inside the user root
/// (`~/.mcx/etc/mcx/config.ini`).
pub fn user_config_path() -> PathBuf {
    user_config_path_in(&home_dir())
}

/// Root-directory config path anchored at an explicit home (used by tests).
pub fn user_config_path_in(home: &Path) -> PathBuf {
    home.join(constants::USER_ROOT_DIR)
        .join(constants::PATH_ETC_MCX)
        .join("config.ini")
}

/// Path of the separate user-selection config
/// (`~/.mcx/etc/mcx/user.ini`). This file is distinct from the
/// root-directory config so the mode/user selection stays required and
/// independent of any configured root.
pub fn user_selection_path() -> PathBuf {
    user_selection_path_in(&home_dir())
}

/// User-selection config path anchored at an explicit home (used by tests).
pub fn user_selection_path_in(home: &Path) -> PathBuf {
    home.join(constants::USER_ROOT_DIR)
        .join(constants::PATH_ETC_MCX)
        .join("user.ini")
}

fn is_root_process() -> bool {
    std::process::Command::new(constants::TOOL_ID)
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false)
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if !s.starts_with('~') {
        return path.to_path_buf();
    }
    let rest = if s == "~" {
        ""
    } else if let Some(stripped) = s.strip_prefix("~/") {
        stripped
    } else {
        &s[1..]
    };
    let mut home = home_dir();
    if !rest.is_empty() {
        home.push(rest);
    }
    home
}

/// Normalize a configured root directory: expand `~` and resolve relative
/// paths against the current working directory so every command behaves the
/// same regardless of where it is launched from.
pub fn normalize_root(path: &Path) -> PathBuf {
    let expanded = expand_tilde(path);
    if expanded.is_absolute() {
        expanded
    } else {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        cwd.join(expanded)
    }
}

/// The fixed per-user root: the selected user's home plus `.mcx`.
fn default_user_root() -> PathBuf {
    normalize_root(Path::new(constants::DEFAULT_USER_ROOT))
}

fn inside_home(path: &Path) -> bool {
    let home = home_dir();
    path == home || path.starts_with(&home)
}

/// Whether `path` lies inside the current process's home directory. User mode
/// is 100% separated from the system and every other user, so its root must
/// satisfy this.
pub fn is_within_home(path: &Path) -> bool {
    inside_home(path)
}

/// Home directory of the given OS user, from the passwd database (`getpwnam`).
/// Returns `None` for unknown users.
pub fn home_dir_for_user(name: &str) -> Option<PathBuf> {
    let c_name = std::ffi::CString::new(name).ok()?;
    // SAFETY: getpwnam() read-only; the returned pointer is owned by libc and
    // only dereferenced here, before any other passwd call could invalidate it.
    let pw = unsafe { libc::getpwnam(c_name.as_ptr()) };
    if pw.is_null() {
        return None;
    }
    // SAFETY: pw is non-null and pw_dir is a NUL-terminated string valid for
    // the lifetime of this read.
    let dir = unsafe { std::ffi::CStr::from_ptr((*pw).pw_dir) };
    let s = dir.to_string_lossy();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s.into_owned()))
    }
}

/// The canonical per-user root for `--for-user`: the selected user's home plus
/// `.mcx`, exactly the directory that same user's own user mode targets.
pub fn target_user_root(name: &str) -> Option<PathBuf> {
    home_dir_for_user(name).map(|home| home.join(constants::USER_ROOT_DIR))
}

/// Resolve the effective mode with CLI flag precedence over environment
/// variables over the persisted user-selection config over the default.
pub fn resolve_mode(flag: Option<bool>) -> Mode {
    if let Some(user) = flag {
        return if user { Mode::User } else { Mode::System };
    }
    if env::var("MCX_USER_MODE").is_ok() {
        return Mode::User;
    }
    if env::var("MCX_SYSTEM_MODE").is_ok() {
        return Mode::System;
    }
    read_user_selection()
        .map(|sel| sel.mode)
        .unwrap_or(Mode::System)
}

/// Choose the operational root directory for this invocation.
///
/// User mode honors `--root` (absolute, relative, or `~`-prefixed) and
/// otherwise uses the configured per-user root; callers additionally enforce
/// [`check_directory_access`] so a user-mode directory a user cannot read and
/// write is refused. System mode honors `--root` and the system root;
/// non-root read-only commands in system mode fall back to the per-user root
/// so queries never require elevation.
pub fn resolve_root(
    explicit_root: Option<&str>,
    mode: Mode,
    cfg: &ModeConfig,
    read_only: bool,
) -> PathBuf {
    let user_root = cfg.effective_user_root();
    if mode.is_user() {
        return explicit_root
            .map(Path::new)
            .map(normalize_root)
            .unwrap_or(user_root);
    }
    if let Some(root) = explicit_root {
        return normalize_root(Path::new(root));
    }
    let system_root = cfg.effective_system_root();
    if is_root_process() || !read_only {
        return system_root;
    }
    if inside_home(&user_root) {
        user_root
    } else {
        default_user_root()
    }
}

/// Verify the current process can traverse to, read, and write the directory
/// at `path` — the same permission `ls -la` exposes for a user. Used to
/// refuse a user-mode root the switching user has no read/write access to.
///
/// The final component may not exist yet (mcx creates its root on first
/// use); in that case the nearest existing ancestor is checked, so a path can
/// never be selected in a location the user cannot create files in.
pub fn check_directory_access(path: &Path) -> Result<()> {
    let mut candidate = path.to_path_buf();
    let existing = loop {
        if candidate.as_os_str().is_empty() || candidate == Path::new("/") {
            break candidate;
        }
        match fs::metadata(&candidate) {
            Ok(md) => {
                if !md.is_dir() {
                    anyhow::bail!("{} is not a directory", candidate.display());
                }
                break candidate;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match candidate.parent() {
                Some(parent) => candidate = parent.to_path_buf(),
                None => break candidate,
            },
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "Cannot inspect {} while checking directory access",
                        candidate.display()
                    )
                });
            }
        }
    };

    let c_path = std::ffi::CString::new(existing.as_os_str().as_bytes())
        .with_context(|| format!("Unrepresentable path {}", existing.display()))?;
    // SAFETY: access(2) takes a NUL-terminated C string we own and never
    // touches process state beyond the requested permission bits.
    let rc = unsafe { libc::access(c_path.as_ptr(), libc::R_OK | libc::W_OK | libc::X_OK) };
    if rc != 0 {
        anyhow::bail!(
            "{} has no read/write access to {} ({})",
            current_user(),
            path.display(),
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

// ── User selection config (user.ini) ───────────────────────────────────────

/// Load the persisted user selection from `~/.mcx/etc/mcx/user.ini`, but only
/// if it belongs to the current OS user. A selection written by another user
/// is ignored, so switching between users stays isolated: each user inherits
/// user mode only if they switched to it themselves; everyone else falls back
/// to the default (system mode). A missing file or missing fields also
/// returns `None` so the caller never guesses an external target.
pub fn read_user_selection() -> Option<UserSelection> {
    owned_for_user(
        read_user_selection_in(&user_selection_path()),
        &current_user(),
    )
}

/// Whether `sel` was authored by the given OS user. Kept separate so the
/// ownership rule can be tested without touching process-global `HOME`/`USER`.
pub fn selection_owned_by(sel: &UserSelection, user: &str) -> bool {
    sel.user == user
}

/// Filter a raw selection by the OS user it is bound to (`None` when missing
/// or authored by someone else).
fn owned_for_user(sel: Option<UserSelection>, user: &str) -> Option<UserSelection> {
    match sel {
        Some(sel) if selection_owned_by(&sel, user) => Some(sel),
        _ => None,
    }
}

/// User-selection read anchored at an explicit path (used by tests).
pub fn read_user_selection_in(path: &Path) -> Option<UserSelection> {
    let parsed = MappedConfig::from_file(path).ok()?;
    let get = |key: &str| {
        parsed
            .get("general", key)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let mode = get("mode").and_then(Mode::parse)?;
    let user = get("user")?.to_string();
    Some(UserSelection { mode, user })
}

/// Persist the user selection into `~/.mcx/etc/mcx/user.ini`, preserving
/// every other section and entry in the file.
pub fn write_user_selection(sel: &UserSelection) -> Result<()> {
    write_user_selection_in(sel, &user_selection_path())
}

/// User-selection write anchored at an explicit path (used by tests).
pub fn write_user_selection_in(sel: &UserSelection, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create user config directory {:?}", parent))?;
    }
    let content = if path.exists() {
        let original = fs::read_to_string(path).unwrap_or_default();
        patch_general(
            &original,
            &[("mode", sel.mode.as_str()), ("user", sel.user.as_str())],
        )
    } else {
        format!(
            "[general]\nmode = {}\nuser = {}\n",
            sel.mode.as_str(),
            sel.user
        )
    };
    fs::write(path, content).with_context(|| format!("Failed to write user config {:?}", path))
}

// ── Root-directory config (config.ini) ─────────────────────────────────────

/// Load the root-directory settings from the user's home config, applying
/// defaults for anything missing.
pub fn read_mode_config() -> ModeConfig {
    read_mode_config_in(&user_config_path())
}

/// Root-directory config read anchored at an explicit path (used by tests).
pub fn read_mode_config_in(path: &Path) -> ModeConfig {
    let mut cfg = ModeConfig::default();
    if let Ok(parsed) = MappedConfig::from_file(path) {
        if let Some(root) = parsed
            .get("general", "user_root")
            .map(str::trim)
            .filter(|r| !r.is_empty())
        {
            cfg.user_root = root.to_string();
        }
        if let Some(root) = parsed
            .get("general", "system_root")
            .map(str::trim)
            .filter(|r| !r.is_empty())
        {
            cfg.system_root = root.to_string();
        }
    }
    cfg
}

/// Persist the root-directory settings into the user's home config,
/// preserving every other section and entry in the file.
pub fn write_mode_config(cfg: &ModeConfig) -> Result<()> {
    write_mode_config_in(cfg, &user_config_path())
}

/// Root-directory config write anchored at an explicit path (used by tests).
pub fn write_mode_config_in(cfg: &ModeConfig, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create mode config directory {:?}", parent))?;
    }
    let content = if path.exists() {
        let original = fs::read_to_string(path).unwrap_or_default();
        patch_general(
            &original,
            &[
                ("user_root", cfg.user_root.as_str()),
                ("system_root", cfg.system_root.as_str()),
            ],
        )
    } else {
        format!(
            "[general]\nsystem_root = {}\nuser_root = {}\n",
            cfg.system_root, cfg.user_root
        )
    };
    fs::write(path, content).with_context(|| format!("Failed to write mode config {:?}", path))
}

/// Ensure the root-directory config exposes `user_root` and `system_root`.
/// Fields are added when missing and left untouched when present, so an
/// existing config (or one written by an older mcx) gains the fields without
/// losing anything.
pub fn ensure_mode_fields() -> Result<()> {
    ensure_mode_fields_in(&user_config_path())
}

/// Ensure root-directory fields exist in an explicit config path.
pub fn ensure_mode_fields_in(path: &Path) -> Result<()> {
    if path.exists() && general_has_keys(path, &ROOT_KEYS) {
        return Ok(());
    }
    let cfg = read_mode_config_in(path);
    write_mode_config_in(&cfg, path)
}

const ROOT_KEYS: [&str; 2] = ["user_root", "system_root"];

/// Whether every key is present (case-insensitively) in the `[general]`
/// section of the config at `path`.
fn general_has_keys(path: &Path, keys: &[&str]) -> bool {
    let Ok(parsed) = MappedConfig::from_file(path) else {
        return false;
    };
    let Some(section) = parsed
        .sections()
        .iter()
        .find(|s| s.key.eq_ignore_ascii_case("general"))
    else {
        return false;
    };
    let mut found = vec![false; keys.len()];
    for entry in &section.entries {
        for (i, key) in keys.iter().enumerate() {
            if entry.key.eq_ignore_ascii_case(key) {
                found[i] = true;
            }
        }
    }
    found.iter().all(|f| *f)
}

/// Rewrite the `[general]` section of an INI document, replacing the given
/// keys while leaving every other section and line untouched. Missing keys
/// are appended into `[general]`; a missing section is appended at the end.
fn patch_general(original: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut pending: Vec<(&str, String)> = pairs.iter().map(|(k, v)| (*k, v.to_string())).collect();

    let mut in_general = false;
    let mut saw_general = false;

    let flush_pending = |out: &mut String, pending: &mut Vec<(&str, String)>| {
        for (key, value) in pending.drain(..) {
            out.push_str(&format!("{} = {}\n", key, value));
        }
    };

    for line in original.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_general {
                flush_pending(&mut out, &mut pending);
                in_general = false;
            }
            let name = trimmed[1..trimmed.len() - 1].trim();
            if name.eq_ignore_ascii_case("general") {
                saw_general = true;
                in_general = true;
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if in_general && let Some(eq) = line.find('=') {
            let key = line[..eq].trim();
            if let Some((_, value)) = pending.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
                let value = value.clone();
                pending.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
                out.push_str(&format!("{} = {}\n", key, value));
                continue;
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    if in_general {
        flush_pending(&mut out, &mut pending);
    }
    if !saw_general {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("[general]\n");
        flush_pending(&mut out, &mut pending);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(dir: &str) -> ModeConfig {
        ModeConfig {
            user_root: format!("{}/u", dir),
            system_root: format!("{}/s", dir),
        }
    }

    #[test]
    fn test_normalize_root_absolute_passes_through() {
        assert_eq!(
            normalize_root(Path::new("/opt/mcx")),
            PathBuf::from("/opt/mcx")
        );
    }

    #[test]
    fn test_normalize_root_relative_joins_cwd() {
        let cwd = env::current_dir().unwrap();
        assert_eq!(normalize_root(Path::new("rel/root")), cwd.join("rel/root"));
    }

    #[test]
    fn test_normalize_root_dot_is_cwd() {
        let cwd = env::current_dir().unwrap();
        assert_eq!(normalize_root(Path::new(".")), cwd);
    }

    #[test]
    fn test_expand_tilde_uses_home() {
        let home = home_dir();
        assert_eq!(expand_tilde(Path::new("~")), home);
        assert_eq!(expand_tilde(Path::new("~/mcx")), home.join("mcx"));
        assert_eq!(expand_tilde(Path::new("~/a/b")), home.join("a/b"));
    }

    #[test]
    fn test_resolve_mode_flag_precedence() {
        assert_eq!(resolve_mode(Some(true)), Mode::User);
        assert_eq!(resolve_mode(Some(false)), Mode::System);
    }

    #[test]
    fn test_user_selection_round_trip_requires_fields() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("etc/mcx/user.ini");
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        assert!(
            read_user_selection_in(&path).is_none(),
            "missing file has no selection"
        );

        fs::write(&path, "[general]\nmode = user\n").unwrap();
        assert!(
            read_user_selection_in(&path).is_none(),
            "missing required user field"
        );

        fs::write(&path, "[general]\nmode = user\nuser = alice\n").unwrap();
        let sel = read_user_selection_in(&path).unwrap();
        assert_eq!(sel.mode, Mode::User);
        assert_eq!(sel.user, "alice");
    }

    #[test]
    fn test_write_user_selection_preserves_other_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("etc/mcx/user.ini");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[general]\nnote = keep\n").unwrap();
        write_user_selection_in(
            &UserSelection {
                mode: Mode::System,
                user: "root".to_string(),
            },
            &path,
        )
        .unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("mode = system"));
        assert!(content.contains("user = root"));
        assert!(content.contains("note = keep"));
    }

    #[test]
    fn test_patch_general_replaces_and_preserves() {
        let original =
            "[general]\nlog_level = info\nuser_root = /old\n\n[python]\nenabled = false\n";
        let patched = patch_general(
            original,
            &[("user_root", "/new/u"), ("system_root", "/new/s")],
        );
        assert!(patched.contains("user_root = /new/u"));
        assert!(patched.contains("system_root = /new/s"));
        assert!(patched.contains("log_level = info"));
        assert!(patched.contains("[python]"));
        assert!(patched.contains("enabled = false"));
        assert!(!patched.contains("/old"));
    }

    #[test]
    fn test_patch_general_appends_section_when_missing() {
        let original = "[python]\nenabled = false\n";
        let patched = patch_general(original, &[("user_root", "/u"), ("system_root", "/s")]);
        assert!(patched.contains("[general]"));
        assert!(patched.contains("user_root = /u"));
        assert!(patched.contains("[python]"));
    }

    #[test]
    fn test_ensure_mode_fields_creates_config_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("etc/mcx/config.ini");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        ensure_mode_fields_in(&path).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("user_root = ~/.mcx"));
        assert!(content.contains("system_root = /"));
    }

    #[test]
    fn test_ensure_mode_fields_adds_missing_keys() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.ini");
        fs::write(
            &path,
            "[general]\nlog_level = info\n\n[python]\nenabled = true\n",
        )
        .unwrap();
        ensure_mode_fields_in(&path).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("user_root = ~/.mcx"));
        assert!(content.contains("system_root = /"));
        assert!(content.contains("log_level = info"));
        assert!(content.contains("enabled = true"));
    }

    #[test]
    fn test_ensure_mode_fields_is_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.ini");
        ensure_mode_fields_in(&path).unwrap();
        let first = fs::read_to_string(&path).unwrap();
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        ensure_mode_fields_in(&path).unwrap();
        let second = fs::read_to_string(&path).unwrap();
        assert_eq!(first, second);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), mtime);
    }

    #[test]
    fn test_ensure_mode_fields_preserves_custom_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.ini");
        fs::write(&path, "[general]\nuser_root = ~/.mcx/dev\n").unwrap();
        ensure_mode_fields_in(&path).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("user_root = ~/.mcx/dev"));
        assert!(content.contains("system_root = /"));
    }

    #[test]
    fn test_resolve_root_user_mode_uses_configured_user_root() {
        let cfg = config("/tmp/u");
        let root = resolve_root(None, Mode::User, &cfg, false);
        assert_eq!(root, std::path::PathBuf::from("/tmp/u/u"));
    }

    #[test]
    fn test_resolve_root_user_mode_honors_explicit_root() {
        let cfg = config("/tmp");
        let root = resolve_root(Some("/external/root"), Mode::User, &cfg, false);
        assert_eq!(root, std::path::PathBuf::from("/external/root"));
    }

    #[test]
    fn test_resolve_root_system_mode_targets_system_root() {
        let cfg = config("/tmp/sys");
        let sys = resolve_root(None, Mode::System, &cfg, false);
        assert_eq!(sys, std::path::PathBuf::from("/tmp/sys/s"));
    }

    #[test]
    fn test_selection_ignored_when_owned_by_another_user() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("etc/mcx/user.ini");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[general]\nmode = user\nuser = alice\n").unwrap();

        let raw = read_user_selection_in(&path);
        assert!(raw.is_some());
        assert!(
            owned_for_user(raw.clone(), "alice").is_some(),
            "the user who switched inherits user mode"
        );
        assert!(
            owned_for_user(raw, "bob").is_none(),
            "other users stay on the default system mode"
        );
    }

    #[test]
    fn test_check_directory_access_allows_writable_temp_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        check_directory_access(tmp.path()).expect("writable temp dir is accessible");
    }

    #[test]
    fn test_check_directory_access_allows_missing_leaf_under_writable_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let missing = tmp.path().join("not-yet-created");
        check_directory_access(&missing).expect("missing leaf under writable parent is accessible");
    }

    #[test]
    fn test_check_directory_access_rejects_regular_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("file");
        fs::write(&file, b"x").unwrap();
        let err = check_directory_access(&file).unwrap_err();
        assert!(err.to_string().contains("not a directory"));
    }

    #[cfg(unix)]
    #[test]
    fn test_check_directory_access_rejects_readonly_dir_when_not_root() {
        use std::os::unix::fs::PermissionsExt;
        let is_root = std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8(o.stdout).unwrap_or_default().trim() == "0")
            .unwrap_or(false);
        if is_root {
            return; // root can always write, whatever the mode bits say
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let ro = tmp.path().join("ro");
        fs::create_dir(&ro).unwrap();
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
        let err = check_directory_access(&ro).unwrap_err();
        assert!(err.to_string().contains("no read/write access"));
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn test_is_within_home_matches_home_boundary() {
        let home = home_dir();
        assert!(is_within_home(&home), "the home itself qualifies");
        assert!(
            is_within_home(&home.join(".mcx")),
            "subdirs of home qualify"
        );
        assert!(
            !is_within_home(Path::new("/tmp")),
            "system dirs do not qualify"
        );
        assert!(
            !is_within_home(Path::new("/")),
            "the system root never qualifies"
        );
    }

    #[test]
    fn test_home_dir_for_user_uses_passwd_database() {
        assert!(
            home_dir_for_user("this-user-cannot-exist-mcx").is_none(),
            "unknown user has no home"
        );
        let home = home_dir_for_user("root").expect("root exists in the passwd database");
        assert_eq!(home, PathBuf::from("/root"));
        assert_eq!(target_user_root("root"), Some(PathBuf::from("/root/.mcx")));
    }

    #[test]
    fn test_read_mode_config_requires_defaults() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.ini");
        fs::write(&path, "[general]\nuser_root = ~/.mcx/custom\n").unwrap();
        let cfg = read_mode_config_in(&path);
        assert_eq!(cfg.user_root, "~/.mcx/custom");
        assert_eq!(cfg.system_root, "/");
    }
}
