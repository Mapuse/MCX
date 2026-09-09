use super::mode::Mode;
use crate::utils::ui::UserInterface;
use std::env;
use std::path::Path;
use std::process::{Command, exit};

/// Returns the effective command token: the first argument that is neither a
/// global option nor a global option's value.
fn command_token(args: &[String]) -> Option<(usize, &str)> {
    let mut iter = args.iter().enumerate();
    while let Some((idx, a)) = iter.next() {
        if a == "--root" || a == "--for-user" {
            iter.next(); // consume the separate value
        } else if !a.starts_with('-') {
            return Some((idx, a.as_str()));
        }
    }
    None
}

/// Maps every short flag and alias to its canonical long-form subcommand
/// name (mirrors the definitions in main.rs) so the elevated child always
/// receives an unambiguous command.
fn canonical_command(token: &str) -> &str {
    match token.trim_start_matches('-') {
        "i" | "in" => "install",
        "a" | "local" | "package" | "xcs" => "add",
        "r" | "rm" | "uninstall" | "delete" => "remove",
        "full-remove" => "purge",
        "s" | "find" | "look" => "search",
        "u" | "refresh" | "sync" => "update",
        "U" | "up" | "dist-upgrade" => "upgrade",
        "q" | "info" | "show" => "query",
        "c" | "wipe" | "clear" => "clean",
        "V" | "check" | "certify" => "verify",
        "f" | "repair" => "fix",
        "C" | "cfg" | "settings" => "config",
        "H" | "log" | "record" => "history",
        "b" | "make" | "create" => "build",
        "ra" => "repo-add",
        "rr" => "repo-remove",
        "rl" => "repo-list",
        "rs" => "repo-sync",
        "re" => "repo-enable",
        "rd" => "repo-disable",
        "ri" => "repo-info",
        "update-self" => "self-update",
        "vnd" => "vendor",
        "comp" => "completion",
        "cg" => "cgroup",
        "mt" | "toggle" | "mode-toggle" => "mode",
        other => other,
    }
}

fn is_read_only_command() -> bool {
    let args: Vec<String> = env::args().collect();
    let Some((_, cmd)) = command_token(&args[1..]) else {
        return true;
    };
    let canonical = canonical_command(cmd);
    matches!(
        canonical,
        "search" | "query" | "history" | "completion" | "repo-list" | "repo-info" | "version"
    )
}

pub fn root_access(root: &Path, mode: &Mode, target_user: Option<&str>) {
    if env::var("MCX_IGNORE_SUDO").is_ok() || env::var("USER").map(|u| u == "root").unwrap_or(false)
    {
        return;
    }

    // The mode command self-manages: toggling validates the root password
    // itself; status queries never need elevation.
    if is_mode_command() {
        return;
    }

    // SAFETY: libc::getuid() takes no arguments, does not dereference any
    // pointers, and has no side effects; it is always safe to call.
    let uid = unsafe { libc::getuid() };

    if uid != 0 {
        if mode.is_user() {
            // User mode operates entirely on the user's own root inside their
            // home; elevation is never required or permitted.
            return;
        }
        let elevating = target_user.is_some() || !is_read_only_command();
        if !elevating {
            return;
        }

        // Operating on another user's environment always demands the root
        // password, even for read-only commands.
        if target_user.is_some() && !authenticate_root() {
            eprintln!(
                "Error: root password required to operate on another user; authentication failed."
            );
            exit(1);
        }

        UserInterface::info("Using root access for this action...");

        let exe = match env::current_exe() {
            Ok(e) => e,
            Err(_) => {
                eprintln!("Error: cannot determine current executable path");
                exit(1);
            }
        };
        let all_args: Vec<String> = env::args().collect();
        let mut new_args: Vec<String> = all_args[1..].to_vec();

        // Normalize the command spelling to its long form so the elevated
        // child receives an unambiguous subcommand.
        if let Some((idx, cmd)) = command_token(&new_args) {
            new_args[idx] = canonical_command(cmd).to_string();
        }

        // An explicit target user re-resolves its own root as root in the
        // child; otherwise the resolved root is passed down explicitly.
        if target_user.is_none()
            && !new_args
                .iter()
                .any(|a| a == "--root" || a.starts_with("--root="))
        {
            new_args.push("--root".to_string());
            new_args.push(root.to_string_lossy().to_string());
        }

        let status = match Command::new("sudo").arg(&exe).args(&new_args).status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: failed to execute sudo: {}", e);
                exit(1);
            }
        };

        exit(status.code().unwrap_or(1));
    }
}

/// Demand the root password right now: cached `sudo` credentials are flushed
/// first so `sudo -v` always re-prompts. Returns `true` when authenticated
/// (or already running as root / `MCX_IGNORE_SUDO` is set). Pure: callers
/// decide what to print.
pub fn authenticate_root() -> bool {
    if env::var("MCX_IGNORE_SUDO").is_ok() || env::var("USER").map(|u| u == "root").unwrap_or(false)
    {
        return true;
    }

    // SAFETY: libc::getuid() is side-effect free and always safe to call.
    let uid = unsafe { libc::getuid() };
    if uid == 0 {
        return true;
    }

    // Flush any cached sudo credentials so every toggle re-prompts.
    let _ = Command::new("sudo").arg("-k").status();

    match Command::new("sudo").arg("-v").status() {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Validate that the current invocation is permitted to toggle the operating
/// mode. The root password is demanded on every toggle.
pub fn authenticate_toggle() -> bool {
    if authenticate_root() {
        return true;
    }
    UserInterface::error("Root password required to toggle mode; authentication failed.");
    false
}

fn is_mode_command() -> bool {
    let args: Vec<String> = env::args().collect();
    let Some((_, cmd)) = command_token(&args[1..]) else {
        return false;
    };
    canonical_command(cmd) == "mode"
}
