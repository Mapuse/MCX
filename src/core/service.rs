use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context, anyhow};

/// Service names become file names (`{name}.ini`) and process arguments;
/// keep them strictly limited to safe characters.
pub fn validate_service_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 255
        && !name.chars().all(|c| c == '.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "Invalid service name {:?}: allowed characters are [a-zA-Z0-9._-]",
            name
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CesarService {
    pub name: String,
    pub exec: String,
    #[serde(default)]
    pub requires: String,    #[serde(default = "default_restart")]
    pub restart: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub working_directory: String,
    #[serde(default)]
    pub socket: Option<String>,
}

fn default_restart() -> String {
    "on-failure".to_string()
}

impl CesarService {
    pub fn to_ini(&self) -> String {
        let mut ini = String::from("[Service]\n");
        ini.push_str(&format!("Name = {}\n", self.name));
        ini.push_str(&format!("Exec = {}\n", self.exec));
        if !self.requires.is_empty() {
            ini.push_str(&format!("Requires = {}\n", self.requires));
        }
        ini.push_str(&format!("Restart = {}\n", self.restart));
        if !self.description.is_empty() {
            ini.push_str(&format!("Description = {}\n", self.description));
        }
        if let Some(ref socket) = self.socket {
            ini.push_str(&format!("Socket = {}\n", socket));
        }
        for (key, value) in &self.environment {
            ini.push_str(&format!("Environment = {}={}\n", key, value));
        }
        if !self.working_directory.is_empty() {
            ini.push_str(&format!("WorkingDirectory = {}\n", self.working_directory));
        }
        ini
    }

    pub fn from_ini(content: &str) -> Result<Self> {
        let mut name = String::new();
        let mut exec = String::new();
        let mut requires = String::new();
        let mut restart = "on-failure".to_string();
        let mut description = String::new();
        let mut environment = HashMap::new();
        let mut working_directory = String::new();
        let mut socket = None;

        let mut in_service = false;
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_service = line.eq_ignore_ascii_case("[service]");
                continue;
            }
            if !in_service { continue; }
            if let Some((key, value)) = line.split_once('=') {
                let k = key.trim().to_lowercase();
                let v = value.trim().to_string();
                match k.as_str() {
                    "name" => name = v,
                    "exec" => exec = v,
                    "requires" => requires = v,
                    "restart" => restart = v,
                    "description" => description = v,
                    "socket" => socket = Some(v),
                    "workingdirectory" => working_directory = v,
                    "environment" => {
                        if let Some((ek, ev)) = v.split_once('=') {
                            environment.insert(ek.trim().to_string(), ev.trim().to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        if name.is_empty() || exec.is_empty() {
            return Err(anyhow!("Service INI must have 'Name' and 'Exec' fields"));
        }
        validate_service_name(&name)?;

        Ok(CesarService { name, exec, requires, restart, description, environment, working_directory, socket })
    }

    pub fn from_systemd_unit(content: &str) -> Result<Self> {
        systemd_to_cesar(content)
    }
}

fn systemd_to_cesar(content: &str) -> Result<CesarService> {
    let mut exec = String::new();
    let mut description = String::new();
    let mut requires = String::new();
    let mut _after = String::new();
    let mut restart = "on-failure".to_string();
    let mut environment = HashMap::new();
    let mut working_directory = String::new();
    let mut wants = Vec::new();
    let mut wanted_by = Vec::new();

    let mut current_section = String::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len()-1].to_lowercase();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let k = key.trim().to_lowercase();
            let v = value.trim();
            match (current_section.as_str(), k.as_str()) {
                ("unit", "description") => description = v.to_string(),
                ("unit", "requires") => requires = v.to_string(),
                ("unit", "wants") => wants.push(v.to_string()),
                ("unit", "after") => _after = v.to_string(),
                ("service", "execstart") => {
                    if exec.is_empty() { exec = v.to_string(); }
                }
                ("service", "execstartpre") => {}
                ("service", "execstartpost") => {}
                ("service", "execstop") => {}
                ("service", "execstoppost") => {}
                ("service", "restart") => {
                    restart = match v {
                        "always" => "always".to_string(),
                        "on-failure" | "on-abnormal" | "on-abort" => "on-failure".to_string(),
                        "no" | "on-success" => "never".to_string(),
                        _ => "on-failure".to_string(),
                    };
                }
                ("service", "environment") => {
                    if let Some((ek, ev)) = v.split_once('=') {
                        environment.insert(ek.trim().to_string(), ev.trim().to_string());
                    }
                }
                ("service", "workingdirectory") => working_directory = v.to_string(),
                ("install", "wantedby") => wanted_by.push(v.to_string()),
                _ => {}
            }
        }
    }

    if exec.is_empty() {
        return Err(anyhow!("systemd unit has no ExecStart= directive"));
    }

    let mut all_requires = Vec::new();
    if !requires.is_empty() {
        all_requires.extend(requires.split_whitespace().map(|s| s.trim_end_matches('.').to_string()));
    }
    for w in &wants {
        let cleaned = w.trim_end_matches('.');
        if !all_requires.contains(&cleaned.to_string()) {
            all_requires.push(cleaned.to_string());
        }
    }

    let name = description.split_whitespace().next()
        .unwrap_or("unknown")
        .to_string()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();

    Ok(CesarService {
        name,
        exec,
        requires: all_requires.join(" "),
        restart,
        description,
        environment,
        working_directory,
        socket: None,
    })
}

pub fn translate_service_file(input_path: &Path, output_dir: &Path) -> Result<PathBuf> {
    let content = fs::read_to_string(input_path)
        .with_context(|| format!("Failed to read service file: {:?}", input_path))?;

    let service = if is_systemd_format(&content) {
        CesarService::from_systemd_unit(&content)?
    } else {
        CesarService::from_ini(&content)?
    };

    fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create output directory: {:?}", output_dir))?;

    let output_path = output_dir.join(format!("{}.ini", service.name));
    fs::write(&output_path, service.to_ini())
        .with_context(|| format!("Failed to write Cesar service: {:?}", output_path))?;

    Ok(output_path)
}

fn is_systemd_format(content: &str) -> bool {
    let mut has_service_section = false;
    let mut has_exec_start = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("[Service]") {
            has_service_section = true;
        }
        if has_service_section && trimmed.to_lowercase().starts_with("execstart=") {
            has_exec_start = true;
            break;
        }
    }
    has_service_section && has_exec_start
}

pub fn batch_translate_services(
    input_dir: &Path,
    output_dir: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut results = Vec::new();

    if !input_dir.exists() {
        return Ok(results);
    }

    for entry in fs::read_dir(input_dir)
        .with_context(|| format!("Failed to read directory: {:?}", input_dir))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "service" || ext == "ini" {
                match translate_service_file(&path, output_dir) {
                    Ok(out_path) => results.push((path, out_path)),
                    Err(e) => {
                        eprintln!("Warning: failed to translate {:?}: {}", path, e);
                    }
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_systemd_to_cesar() {
        let systemd = "\
[Unit]
Description=Network daemon
Requires=network.target
After=network.target

[Service]
ExecStart=/usr/sbin/dhclient
Restart=on-failure
Environment=INTERFACE=eth0

[Install]
WantedBy=multi-user.target
";

        let cesar = CesarService::from_systemd_unit(systemd).expect("parse systemd unit");
        assert_eq!(cesar.exec, "/usr/sbin/dhclient");
        assert_eq!(cesar.restart, "on-failure");
        assert_eq!(cesar.environment.get("INTERFACE").expect("INTERFACE set"), "eth0");
        assert!(cesar.requires.contains("network"));
    }

    #[test]
    fn test_cesar_ini_roundtrip() {
        let service = CesarService {
            name: "test-svc".to_string(),
            exec: "/bin/test".to_string(),
            requires: "dep-a dep-b".to_string(),
            restart: "always".to_string(),
            description: "Test service".to_string(),
            environment: HashMap::new(),
            working_directory: String::new(),
            socket: None,
        };

        let ini = service.to_ini();
        let parsed = CesarService::from_ini(&ini).expect("parse ini service");
        assert_eq!(parsed.name, "test-svc");
        assert_eq!(parsed.exec, "/bin/test");
        assert_eq!(parsed.restart, "always");
    }

    #[test]
    fn test_is_systemd_format() {
        let systemd = "[Service]\nExecStart=/bin/test\n";
        assert!(is_systemd_format(systemd));

        let cesar = "[Service]\nName = test\nExec = /bin/test\n";
        assert!(!is_systemd_format(cesar));
    }

    #[test]
    fn test_systemd_restart_mapping() {
        let cases = vec![
            ("always", "always"),
            ("on-failure", "on-failure"),
            ("on-abnormal", "on-failure"),
            ("no", "never"),
            ("on-success", "never"),
        ];
        for (input, expected) in cases {
            let systemd = format!("[Service]\nExecStart=/bin/test\nRestart={}\n", input);
            let cesar = CesarService::from_systemd_unit(&systemd).expect("parse systemd unit");
            assert_eq!(cesar.restart, expected, "Failed for Restart={}", input);
        }
    }
}
