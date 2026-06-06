use std::env;
use std::process::{Command, exit};
use crate::utils::ui::UserInterface;

pub fn root_access() {
    if env::var("MCX_IGNORE_SUDO").is_ok() || env::var("USER").map(|u| u == "root").unwrap_or(false) {
        return;
    }

    let uid = unsafe { libc::getuid() };
    
    if uid != 0 {
        UserInterface::display_info("Using root access for this action...");
        
        let args: Vec<String> = env::args().skip(1).collect();

        let status = Command::new("sudo")
            .arg(env::current_exe().unwrap())
            .args(&args)
            .status()
            .expect("Failed to execute sudo");

        exit(status.code().unwrap_or(1));
    }
}