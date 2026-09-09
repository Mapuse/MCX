use super::constants;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct MappedConfigEntry {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct MappedConfigSection {
    pub key: String,
    pub entries: Vec<MappedConfigEntry>,
}

/// Owned INI-style configuration. The file content is read into memory once;
/// an empty or whitespace-only file is valid and yields no sections.
#[derive(Clone, Debug, Default)]
pub struct MappedConfig {
    generation: usize,
    sections: Vec<MappedConfigSection>,
}

impl MappedConfig {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        let generation = next_generation();
        let mut parser = ConfigParser {
            input: &content,
            pos: 0,
        };
        Ok(Self {
            generation,
            sections: parser.parse_all(),
        })
    }

    pub fn generation(&self) -> usize {
        self.generation
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|s| s.key == section)
            .and_then(|s| s.entries.iter().find(|e| e.key == key))
            .map(|e| e.value.as_str())
    }

    pub fn get_section(&self, section: &str) -> Option<&[MappedConfigEntry]> {
        self.sections
            .iter()
            .find(|s| s.key == section)
            .map(|s| s.entries.as_slice())
    }

    pub fn sections(&self) -> &[MappedConfigSection] {
        &self.sections
    }

    pub fn get_or<'a>(&'a self, section: &str, key: &str, default: &'a str) -> &'a str {
        self.get(section, key).unwrap_or(default)
    }

    pub fn get_u64(&self, section: &str, key: &str) -> Option<u64> {
        self.get(section, key).and_then(|s| s.parse().ok())
    }

    pub fn get_usize(&self, section: &str, key: &str) -> Option<usize> {
        self.get(section, key).and_then(|s| s.parse().ok())
    }

    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        self.get(section, key).map(|s| {
            s.eq_ignore_ascii_case("true")
                || s == "1"
                || s.eq_ignore_ascii_case("yes")
                || s.eq_ignore_ascii_case("enabled")
                || s.eq_ignore_ascii_case("on")
        })
    }
}

static CONFIG_GENERATION: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn next_generation() -> usize {
    CONFIG_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

struct ConfigParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> ConfigParser<'a> {
    fn parse_all(&mut self) -> Vec<MappedConfigSection> {
        let mut sections = Vec::new();
        loop {
            self.skip_whitespace_and_newlines();
            if self.pos >= self.input.len() {
                break;
            }
            if self.peek() == Some('[') {
                if let Some(section) = self.parse_section() {
                    sections.push(section);
                }
            } else if self.peek() == Some('#') || self.peek() == Some(';') {
                self.skip_line();
            } else {
                break;
            }
        }
        sections
    }

    fn parse_section(&mut self) -> Option<MappedConfigSection> {
        if self.peek() != Some('[') {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == ']' {
                break;
            }
            self.pos += 1;
        }
        if self.pos > start && self.pos < self.input.len() {
            let key = self.input[start..self.pos].trim().to_string();
            self.pos += 1; // consume ']'
            MappedConfigSection {
                key,
                entries: self.parse_entries_until_next_section(),
            }
            .into()
        } else {
            None
        }
    }

    fn parse_entries_until_next_section(&mut self) -> Vec<MappedConfigEntry> {
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace_only();
            match self.peek() {
                None => break,
                Some('[') => break,
                Some('#') | Some(';') => {
                    self.skip_line();
                }
                _ => {
                    if let Some(entry) = self.parse_entry() {
                        entries.push(entry);
                    }
                }
            }
        }
        entries
    }

    fn parse_entry(&mut self) -> Option<MappedConfigEntry> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '=' || c == '\n' {
                break;
            }
            self.pos += 1;
        }
        if self.peek() != Some('=') {
            // Not a valid entry; skip the malformed line.
            self.skip_line();
            return None;
        }
        let key = self.input[start..self.pos].trim().to_string();
        self.pos += 1; // consume '='
        let value_start = self.pos;
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.pos += 1;
        }
        let value = self.input[value_start..self.pos].trim().to_string();
        Some(MappedConfigEntry { key, value })
    }

    fn skip_whitespace_and_newlines(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn skip_whitespace_only(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == '\n' {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
}

pub struct ConfigManager {
    local_config: MappedConfig,
    repo_config: MappedConfig,
}

impl ConfigManager {
    pub fn new(root: &Path) -> Result<Self> {
        let local_path = root.join("etc/mcx/config.ini");
        let repo_path = root.join("etc/mcx/repo.ini");

        fs::create_dir_all(local_path.parent().expect("local config path has parent"))
            .context("Failed to create config directory")?;
        fs::create_dir_all(repo_path.parent().expect("repo config path has parent"))
            .context("Failed to create repo config directory")?;

        if !local_path.exists() {
            Self::write_default_local(&local_path)?;
        }
        if !repo_path.exists() {
            Self::write_default_repo(&repo_path)?;
        }

        let local_config = MappedConfig::from_file(&local_path)?;
        let repo_config = MappedConfig::from_file(&repo_path)?;

        Ok(Self {
            local_config,
            repo_config,
        })
    }

    pub fn local(&self) -> &MappedConfig {
        &self.local_config
    }

    pub fn repo(&self) -> &MappedConfig {
        &self.repo_config
    }

    pub fn python(&self) -> cps::PythonConfig {
        let cfg = &self.local_config;
        cps::PythonConfig {
            enabled: cfg.get_bool("python", "enabled").unwrap_or(false),
            theme: cfg.get("python", "theme").unwrap_or("").to_string(),
            tui: cfg.get("python", "tui").unwrap_or("").to_string(),
            plugins: cfg
                .get("python", "plugins")
                .map(|s| {
                    s.split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            fallback_on_error: cfg.get_bool("python", "fallback_on_error").unwrap_or(true),
            venv_path: cfg.get("python", "venv_path").unwrap_or("").to_string(),
            tui_mode: cfg.get_bool("python", "tui_mode").unwrap_or(false),
        }
    }

    fn write_default_local(path: &Path) -> Result<()> {
        fs::write(path, constants::DEFAULT_CONFIG_INI.as_bytes())?;
        Ok(())
    }

    fn write_default_repo(path: &Path) -> Result<()> {
        fs::write(path, constants::DEFAULT_REPO_INI.as_bytes())?;
        Ok(())
    }
}
