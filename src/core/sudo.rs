use std::env;
use std::process::{Command, exit};
use crate::utils::ui::UserInterface;

fn is_read_only_command() -> bool {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        return true;
    }
    let cmd = args[2].trim_start_matches('-');
    matches!(cmd,
        "search" | "find" | "look" |
        "query" | "info" | "show" |
        "history" | "log" | "record" |
        "completion" | "comp" |
        "repo-list" | "rl" |
        "repo-info" | "ri" |
        "version"
    )
}

pub fn root_access() {
    if env::var("MCX_IGNORE_SUDO").is_ok() || env::var("USER").map(|u| u == "root").unwrap_or(false) {
        return;
    }

    let uid = unsafe { libc::getuid() };

    if uid != 0 {
        if is_read_only_command() {
            return;
        }

        UserInterface::info("Using root access for this action...");

        let exe = env::current_exe().unwrap();
        let all_args: Vec<String> = env::args().collect();
        let mut new_args: Vec<String> = all_args[1..].to_vec();

        if !new_args.iter().any(|a| a == "--root" || a.starts_with("--root=")) {
            new_args.push("--root".to_string());
            new_args.push("/".to_string());
        }

        let status = Command::new("sudo")
            .arg(&exe)
            .args(&new_args)
            .status()
            .expect("Failed to execute sudo");

        exit(status.code().unwrap_or(1));
    }
}
