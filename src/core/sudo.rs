use std::env;
use std::process::{Command, exit};
use crate::utils::ui::UserInterface;

/// Returns the effective command token: the first argument that is neither a
/// global option nor a global option's value.
fn command_token(args: &[String]) -> Option<(usize, &str)> {
    let mut iter = args.iter().enumerate();
    while let Some((idx, a)) = iter.next() {
        if a == "--root" {
            iter.next(); // consume the separate root value
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
        other => other,
    }
}

fn is_read_only_command() -> bool {
    let args: Vec<String> = env::args().collect();
    let Some((_, cmd)) = command_token(&args[1..]) else {
        return true;
    };
    let canonical = canonical_command(cmd);
    matches!(canonical,
        "search" |
        "query" |
        "history" |
        "completion" |
        "repo-list" |
        "repo-info" |
        "version"
    )
}

pub fn root_access() {
    if env::var("MCX_IGNORE_SUDO").is_ok() || env::var("USER").map(|u| u == "root").unwrap_or(false) {
        return;
    }

    // SAFETY: libc::getuid() takes no arguments, does not dereference any
    // pointers, and has no side effects; it is always safe to call.
    let uid = unsafe { libc::getuid() };

    if uid != 0 {
        if is_read_only_command() {
            return;
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

        if !new_args.iter().any(|a| a == "--root" || a.starts_with("--root=")) {
            new_args.push("--root".to_string());
            new_args.push("/".to_string());
        }

        let status = match Command::new("sudo")
            .arg(&exe)
            .args(&new_args)
            .status()
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: failed to execute sudo: {}", e);
                exit(1);
            }
        };

        exit(status.code().unwrap_or(1));
    }
}
