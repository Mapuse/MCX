use crate::core::constants;
use crate::core::db::Database;
use crate::core::service::CesarService;
use crate::utils::ui::UserInterface;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::Path;
use std::sync::Arc;

pub struct ServiceCommand {
    root: String,
    db: Arc<Database>,
}

impl ServiceCommand {
    pub fn new(root: String, db: Arc<Database>) -> Self {
        Self { root, db }
    }

    pub fn execute(&self, args: &[String]) -> Result<()> {
        if args.is_empty() {
            return Err(anyhow!(
                "Usage: mcx service <list|status|enable|disable|start|stop|restart|info> [name]"
            ));
        }

        match args[0].as_str() {
            "list" => self.list_services(),
            "status" => {
                if args.len() < 2 {
                    return Err(anyhow!("Usage: mcx service status <name>"));
                }
                self.status_service(&args[1])
            }
            "enable" => {
                if args.len() < 2 {
                    return Err(anyhow!("Usage: mcx service enable <name>"));
                }
                self.enable_service(&args[1])
            }
            "disable" => {
                if args.len() < 2 {
                    return Err(anyhow!("Usage: mcx service disable <name>"));
                }
                self.disable_service(&args[1])
            }
            "start" => {
                if args.len() < 2 {
                    return Err(anyhow!("Usage: mcx service start <name>"));
                }
                self.start_service(&args[1])
            }
            "stop" => {
                if args.len() < 2 {
                    return Err(anyhow!("Usage: mcx service stop <name>"));
                }
                self.stop_service(&args[1])
            }
            "restart" => {
                if args.len() < 2 {
                    return Err(anyhow!("Usage: mcx service restart <name>"));
                }
                self.restart_service(&args[1])
            }
            "info" => {
                if args.len() < 2 {
                    return Err(anyhow!("Usage: mcx service info <name>"));
                }
                self.info_service(&args[1])
            }
            "translate" => {
                if args.len() < 3 {
                    return Err(anyhow!(
                        "Usage: mcx service translate <input.service> <output_dir>"
                    ));
                }
                self.translate(&args[1], &args[2])
            }
            _ => Err(anyhow!(
                "Unknown subcommand '{}'. Use list|status|enable|disable|start|stop|restart|info|translate",
                args[0]
            )),
        }
    }

    fn services_dir(&self) -> std::path::PathBuf {
        Path::new(&self.root).join(constants::CESAR_SERVICES_DIR)
    }

    fn installed_packages_with_service(&self) -> Vec<(String, CesarService)> {
        let mut result = Vec::new();
        if let Ok(packages) = self.db.get_all_installed_packages() {
            for pkg in packages {
                for svc in pkg.all_services() {
                    result.push((pkg.pkg_name.clone(), svc.clone()));
                }
            }
        }
        result
    }

    fn list_services(&self) -> Result<()> {
        let packages = self.installed_packages_with_service();
        if packages.is_empty() {
            UserInterface::info("No services registered");
            return Ok(());
        }
        UserInterface::info(&format!("{} registered service(s):", packages.len()));
        for (name, svc) in &packages {
            let enabled = self.is_service_enabled(&svc.name);
            let status = if enabled { "enabled" } else { "disabled" };
            println!("  {} ({}) - {} [{}]", name, svc.name, svc.exec, status);
        }
        Ok(())
    }

    fn status_service(&self, name: &str) -> Result<()> {
        let svc = self.find_service_by_name(name)?;
        let enabled = self.is_service_enabled(&svc.name);
        UserInterface::info(&format!("Service: {}", svc.name));
        println!("  Package:       {}", name);
        println!("  Exec:          {}", svc.exec);
        println!("  Restart:       {}", svc.restart);
        if !svc.requires.is_empty() {
            println!("  Requires:      {}", svc.requires);
        }
        if !svc.description.is_empty() {
            println!("  Description:   {}", svc.description);
        }
        println!("  Enabled:       {}", enabled);
        Ok(())
    }

    fn enable_service(&self, name: &str) -> Result<()> {
        let svc = self.find_service_by_name(name)?;
        let dir = self.services_dir();
        fs::create_dir_all(&dir)?;

        let ini_path = dir.join(format!("{}.ini", svc.name));
        fs::write(&ini_path, svc.to_ini())?;
        UserInterface::success(&format!("Service '{}' enabled", svc.name));
        Ok(())
    }

    fn disable_service(&self, name: &str) -> Result<()> {
        let svc = self.find_service_by_name(name)?;
        let ini_path = self.services_dir().join(format!("{}.ini", svc.name));
        if ini_path.exists() {
            fs::remove_file(&ini_path)?;
            UserInterface::success(&format!("Service '{}' disabled", svc.name));
        } else {
            UserInterface::warning(&format!("Service '{}' was not enabled", svc.name));
        }
        Ok(())
    }

    fn start_service(&self, name: &str) -> Result<()> {
        let svc = self.find_service_by_name(name)?;
        let cesar_bin = self.find_cesar_binary()?;
        let status = std::process::Command::new(&cesar_bin)
            .args(["start", &svc.name])
            .status()
            .map_err(|e| anyhow!("Failed to start service '{}': {}", svc.name, e))?;
        if !status.success() {
            return Err(anyhow!(
                "Failed to start service '{}': cesar exited {}",
                svc.name,
                status
            ));
        }
        UserInterface::success(&format!("Service '{}' started", svc.name));
        Ok(())
    }

    fn stop_service(&self, name: &str) -> Result<()> {
        let svc = self.find_service_by_name(name)?;
        let cesar_bin = self.find_cesar_binary()?;
        let status = std::process::Command::new(&cesar_bin)
            .args(["stop", &svc.name])
            .status()
            .map_err(|e| anyhow!("Failed to stop service '{}': {}", svc.name, e))?;
        if !status.success() {
            return Err(anyhow!(
                "Failed to stop service '{}': cesar exited {}",
                svc.name,
                status
            ));
        }
        UserInterface::success(&format!("Service '{}' stopped", svc.name));
        Ok(())
    }

    fn restart_service(&self, name: &str) -> Result<()> {
        let svc = self.find_service_by_name(name)?;
        let cesar_bin = self.find_cesar_binary()?;
        let status = std::process::Command::new(&cesar_bin)
            .args(["restart", &svc.name])
            .status()
            .map_err(|e| anyhow!("Failed to restart service '{}': {}", svc.name, e))?;
        if !status.success() {
            return Err(anyhow!(
                "Failed to restart service '{}': cesar exited {}",
                svc.name,
                status
            ));
        }
        UserInterface::success(&format!("Service '{}' restarted", svc.name));
        Ok(())
    }

    fn info_service(&self, name: &str) -> Result<()> {
        let svc = self.find_service_by_name(name)?;
        println!("{}", svc.to_ini());
        Ok(())
    }

    fn translate(&self, input: &str, output_dir: &str) -> Result<()> {
        let input_path = Path::new(input);
        let output_path = Path::new(output_dir);
        let result = crate::core::service::translate_service_file(input_path, output_path)?;
        UserInterface::success(&format!("Translated to {:?}", result));
        Ok(())
    }

    fn find_service_by_name(&self, pkg_name: &str) -> Result<CesarService> {
        crate::core::service::validate_service_name(pkg_name)?;
        let packages = self.installed_packages_with_service();
        for (name, svc) in &packages {
            if name == pkg_name || svc.name == pkg_name {
                return Ok(svc.clone());
            }
        }
        Err(anyhow!("No service found for package '{}'", pkg_name))
    }

    fn is_service_enabled(&self, service_name: &str) -> bool {
        let dir = self.services_dir();
        dir.join(format!("{}.ini", service_name)).exists()
    }

    fn find_cesar_binary(&self) -> Result<String> {
        for candidate in constants::CESAR_BINARY_CANDIDATES {
            if Path::new(candidate).exists() {
                return Ok(candidate.to_string());
            }
            // Bare names must actually resolve on PATH, not just exist in
            // the caller's working directory.
            if !candidate.contains('/') {
                let on_path = std::env::var_os("PATH")
                    .map(|paths| {
                        std::env::split_paths(&paths).any(|dir| dir.join(candidate).is_file())
                    })
                    .unwrap_or(false);
                if on_path {
                    return Ok(candidate.to_string());
                }
            }
        }
        Err(anyhow!("Cesar binary not found"))
    }

    pub fn register_service(&self, svc: &CesarService) -> Result<()> {
        let dir = self.services_dir();
        fs::create_dir_all(&dir)?;
        let ini_path = dir.join(format!("{}.ini", svc.name));
        fs::write(&ini_path, svc.to_ini())?;
        Ok(())
    }

    pub fn unregister_service(&self, service_name: &str) -> Result<()> {
        let ini_path = self.services_dir().join(format!("{}.ini", service_name));
        if ini_path.exists() {
            fs::remove_file(&ini_path)?;
        }
        Ok(())
    }
}
