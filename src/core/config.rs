use std::borrow::Cow;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use anyhow::{Result, Context};
use memmap2::Mmap;

static CONFIG_GENERATION: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug)]
pub struct MappedConfigEntry<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Clone, Debug)]
pub struct MappedConfigSection<'a> {
    pub key: &'a str,
    pub entries: Vec<MappedConfigEntry<'a>>,
}

pub struct MappedConfig<'a> {
    #[allow(dead_code)]
    mmap: Arc<Mmap>,
    generation: usize,
    sections: Vec<MappedConfigSection<'a>>,
    _phantom: PhantomData<&'a ()>,
}

unsafe impl<'a> Send for MappedConfig<'a> {}
unsafe impl<'a> Sync for MappedConfig<'a> {}

impl<'a> MappedConfig<'a> {
    pub fn from_file(path: &Path) -> Result<Self> {
        let file = fs::File::open(path)
            .with_context(|| format!("Failed to open config file for mmap: {:?}", path))?;
        let mmap = unsafe { Mmap::map(&file) }
            .context("Failed to memory-map config file")?;
        let raw = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(mmap.as_ptr(), mmap.len())) };
        let generation = CONFIG_GENERATION.fetch_add(1, Ordering::Relaxed);
        let mut parser = ConfigParser { input: raw, pos: 0 };
        let sections = parser.parse_all();
        Ok(Self { mmap: Arc::new(mmap), generation, sections, _phantom: PhantomData })
    }

    pub fn generation(&self) -> usize { self.generation }

    pub fn get(&self, section: &str, key: &str) -> Option<&'a str> {
        self.sections.iter()
            .find(|s| s.key == section)
            .and_then(|s| s.entries.iter().find(|e| e.key == key))
            .map(|e| e.value)
    }

    pub fn get_section(&self, section: &str) -> Option<&[MappedConfigEntry<'a>]> {
        self.sections.iter()
            .find(|s| s.key == section)
            .map(|s| s.entries.as_slice())
    }

    pub fn sections(&self) -> &[MappedConfigSection<'a>] {
        &self.sections
    }

    pub fn get_or<'b>(&self, section: &str, key: &str, default: &'b str) -> Cow<'a, str> {
        self.get(section, key)
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(default.to_string()))
    }

    pub fn get_u64(&self, section: &str, key: &str) -> Option<u64> {
        self.get(section, key).and_then(|s| s.parse().ok())
    }

    pub fn get_usize(&self, section: &str, key: &str) -> Option<usize> {
        self.get(section, key).and_then(|s| s.parse().ok())
    }

    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        self.get(section, key).map(|s| {
            s.eq_ignore_ascii_case("true") || s == "1" || s.eq_ignore_ascii_case("yes")
        })
    }
}

struct ConfigParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> ConfigParser<'a> {
    fn parse_all(&mut self) -> Vec<MappedConfigSection<'a>> {
        let mut sections = Vec::new();
        loop {
            self.skip_whitespace_and_newlines();
            if self.pos >= self.input.len() { break; }
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

    fn parse_section(&mut self) -> Option<MappedConfigSection<'a>> {
        if self.peek() != Some('[') { return None; }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos] != b']' {
            self.pos += 1;
        }
        if self.pos >= self.input.len() { return None; }
        let key = self.input[start..self.pos].trim();
        self.pos += 1;
        let entries = self.parse_entries_until_next_section();
        Some(MappedConfigSection { key, entries })
    }

    fn parse_entries_until_next_section(&mut self) -> Vec<MappedConfigEntry<'a>> {
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace_and_newlines();
            if self.pos >= self.input.len() { break; }
            let c = self.peek().unwrap();
            if c == '[' { break; }
            if c == '#' || c == ';' { self.skip_line(); continue; }
            if let Some(entry) = self.parse_entry() {
                entries.push(entry);
            }
        }
        entries
    }

    fn parse_entry(&mut self) -> Option<MappedConfigEntry<'a>> {
        let start = self.pos;
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos] != b'=' {
            if self.input.as_bytes()[self.pos] == b'\n' { return None; }
            self.pos += 1;
        }
        if self.pos >= self.input.len() { return None; }
        let key = self.input[start..self.pos].trim_end();
        self.pos += 1;
        let val_start = self.pos;
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos] != b'\n' {
            self.pos += 1;
        }
        let value = self.input[val_start..self.pos].trim();
        self.pos += 1;
        if key.is_empty() { return None; }
        Some(MappedConfigEntry { key, value })
    }

    fn skip_whitespace_and_newlines(&mut self) {
        while self.pos < self.input.len() {
            let b = self.input.as_bytes()[self.pos];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                self.pos += 1;
            } else { break; }
        }
    }

    fn skip_line(&mut self) {
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos] != b'\n' {
            self.pos += 1;
        }
        if self.pos < self.input.len() { self.pos += 1; }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
}

#[allow(dead_code)]
pub struct ConfigManager {
    local_path: PathBuf,
    repo_path: PathBuf,
    local_config: MappedConfig<'static>,
    repo_config: MappedConfig<'static>,
}

impl ConfigManager {
    pub fn new(root: &Path) -> Result<Self> {
        let local_path = root.join("etc/mcx/config.ini");
        let repo_path = root.join("etc/mcx/repo.ini");

        fs::create_dir_all(local_path.parent().unwrap())
            .context("Failed to create config directory")?;
        fs::create_dir_all(repo_path.parent().unwrap())
            .context("Failed to create repo config directory")?;

        if !local_path.exists() {
            Self::write_default_local(&local_path)?;
        }
        if !repo_path.exists() {
            Self::write_default_repo(&repo_path)?;
        }

        let local_config = MappedConfig::from_file(&local_path)?;
        let repo_config = MappedConfig::from_file(&repo_path)?;

        Ok(Self { local_path, repo_path, local_config, repo_config })
    }

    pub fn local(&self) -> &MappedConfig<'static> {
        &self.local_config
    }

    pub fn repo(&self) -> &MappedConfig<'static> {
        &self.repo_config
    }

    pub fn calibrate(&self) -> CalibratedParams {
        let cpus = num_cpus::get();
        let cfg = &self.local_config;
        let thread_mode = cfg.get("engine", "thread_pool_mode").unwrap_or("auto");
        let thread_pool_size = match thread_mode {
            "max" => cpus * 2,
            "half" => (cpus / 2).max(1),
            "quad" => cpus * 4,
            _ => cpus,
        };
        CalibratedParams {
            thread_pool_size,
            concurrent_downloads: cfg.get_usize("engine", "max_concurrent_downloads").unwrap_or(cpus.min(8)),
            zstd_level: cfg.get_u64("engine", "zstd_level").unwrap_or(3) as i32,
            io_parallelism: thread_pool_size.max(2),
            network_latency_adaptive: cfg.get_bool("network", "fallback_repos").unwrap_or(true),
            latency_threshold_ms: cfg.get_u64("network", "latency_threshold_ms").unwrap_or(200),
            bandwidth_threshold_kbps: cfg.get_u64("network", "bandwidth_threshold_kbps").unwrap_or(5000),
        }
    }

    fn write_default_local(path: &Path) -> Result<()> {
        fs::write(path, b"[engine]\nthread_pool_mode = auto\nmax_concurrent_downloads = 8\nzstd_level = 3\n\n[network]\nfallback_repos = enabled\nlatency_threshold_ms = 200\nbandwidth_threshold_kbps = 5000\n\n[security]\nverify_checksums = true\nallow_unverified = false\n\n[cache]\nlimit_bytes = 5368709120\nprune_age_hours = 168\n")?;
        Ok(())
    }

    fn write_default_repo(path: &Path) -> Result<()> {
        fs::write(path, b"[main]\nurl = https://packages.cudane.org\nenabled = true\npriority = 100\n\n[community]\nurl = https://community.cudane.org\nenabled = false\npriority = 200\n")?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CalibratedParams {
    pub thread_pool_size: usize,
    pub concurrent_downloads: usize,
    pub zstd_level: i32,
    pub io_parallelism: usize,
    pub network_latency_adaptive: bool,
    pub latency_threshold_ms: u64,
    pub bandwidth_threshold_kbps: u64,
}

impl Default for CalibratedParams {
    fn default() -> Self {
        let cpus = num_cpus::get();
        Self {
            thread_pool_size: cpus,
            concurrent_downloads: cpus.min(8),
            zstd_level: 3,
            io_parallelism: cpus.max(2),
            network_latency_adaptive: true,
            latency_threshold_ms: 200,
            bandwidth_threshold_kbps: 5000,
        }
    }
}
