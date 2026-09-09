use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Architecture {
    Amd64,
    Arm64,
    Native,
}

impl Architecture {
    pub fn host() -> Self {
        if let Ok(arch) = std::fs::read_to_string("/proc/sys/kernel/arch") {
            let arch = arch.trim();
            if arch == "x86_64" {
                return Architecture::Amd64;
            } else if arch == "aarch64" {
                return Architecture::Arm64;
            }
        }
        if let Ok(output) = std::process::Command::new("uname").arg("-m").output()
            && let Ok(val) = String::from_utf8(output.stdout)
        {
            let val = val.trim();
            if val == "x86_64" {
                return Architecture::Amd64;
            } else if val == "aarch64" {
                return Architecture::Arm64;
            }
        }
        Architecture::Amd64
    }

    pub fn all() -> Vec<Architecture> {
        vec![
            Architecture::Amd64,
            Architecture::Arm64,
            Architecture::Native,
        ]
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            Architecture::Amd64 => "x86_64",
            Architecture::Arm64 => "aarch64",
            Architecture::Native => "native",
        }
    }

    pub fn target_triple(&self) -> &'static str {
        match self {
            Architecture::Amd64 => "x86_64-unknown-linux-musl",
            Architecture::Arm64 => "aarch64-unknown-linux-musl",
            Architecture::Native => Architecture::host().target_triple(),
        }
    }

    pub fn rust_target(&self) -> &'static str {
        match self {
            Architecture::Amd64 => "x86_64-unknown-linux-musl",
            Architecture::Arm64 => "aarch64-unknown-linux-musl",
            Architecture::Native => Architecture::host().rust_target(),
        }
    }

    pub fn index_filename(&self) -> String {
        format!("index.{}.json", self.short_name())
    }

    pub fn pool_prefix(&self) -> String {
        format!("pool/{}/", self.short_name())
    }

    pub fn is_compatible_with(&self, host: &Architecture) -> bool {
        match self {
            Architecture::Native => true,
            _ => self == host,
        }
    }

    pub fn resolve(&self) -> Architecture {
        match self {
            Architecture::Native => Architecture::host(),
            other => other.clone(),
        }
    }
}

impl fmt::Display for Architecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Architecture::Amd64 => write!(f, "x86_64"),
            Architecture::Arm64 => write!(f, "aarch64"),
            Architecture::Native => write!(f, "native"),
        }
    }
}

impl FromStr for Architecture {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_lowercase().as_str() {
            "x86_64" | "amd64" | "x64" | "x86" => Ok(Architecture::Amd64),
            "aarch64" | "arm64" | "armv8" | "armv8l" => Ok(Architecture::Arm64),
            "native" => Ok(Architecture::Native),
            _ => Err(anyhow!(
                "Unsupported architecture: '{}'. Expected amd64/x86_64, arm64/aarch64, or native",
                s
            )),
        }
    }
}

pub fn host_architecture() -> Architecture {
    Architecture::host()
}

pub fn package_matches_host(pkg_arch: &str) -> bool {
    if pkg_arch.is_empty() || pkg_arch == "native" {
        return true;
    }
    let host = Architecture::host().short_name();
    pkg_arch == host
}
