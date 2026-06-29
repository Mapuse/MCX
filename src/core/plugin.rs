use std::sync::Arc;
use std::sync::RwLock;
use anyhow::Result;

pub trait Fetcher: Send + Sync {
    fn fetch(&self, source: &str, destination: &str) -> Result<()>;
    fn name(&self) -> &'static str;
}

pub trait Builder: Send + Sync {
    fn build(&self, build_cmd: &str, source_dir: &str, dest_dir: &str, build_type: &str) -> Result<String>;
    fn name(&self) -> &'static str;
}

pub trait Packer: Send + Sync {
    fn pack(&self, source_dir: &str, output_path: &str, compression_level: i32) -> Result<()>;
    fn unpack(&self, archive_path: &str, dest_dir: &str) -> Result<Vec<String>>;
    fn name(&self) -> &'static str;
}

pub struct PluginSlot<T: ?Sized + Send + Sync> {
    inner: RwLock<Arc<T>>,
}

impl<T: ?Sized + Send + Sync> PluginSlot<T> {
    pub fn new(plugin: Arc<T>) -> Self {
        Self { inner: RwLock::new(plugin) }
    }

    pub fn load(&self) -> Arc<T> {
        self.inner.read().unwrap().clone()
    }

    pub fn swap(&self, new_plugin: Arc<T>) -> Arc<T> {
        let mut guard = self.inner.write().unwrap();
        let old = (*guard).clone();
        *guard = new_plugin;
        old
    }
}

pub struct PluginRegistry {
    fetchers: Vec<PluginSlot<dyn Fetcher>>,
    builders: Vec<PluginSlot<dyn Builder>>,
    packers: Vec<PluginSlot<dyn Packer>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { fetchers: Vec::new(), builders: Vec::new(), packers: Vec::new() }
    }

    pub fn register_fetcher(&mut self, f: Arc<dyn Fetcher>) {
        self.fetchers.push(PluginSlot::new(f));
    }

    pub fn register_builder(&mut self, b: Arc<dyn Builder>) {
        self.builders.push(PluginSlot::new(b));
    }

    pub fn register_packer(&mut self, p: Arc<dyn Packer>) {
        self.packers.push(PluginSlot::new(p));
    }

    pub fn resolve_fetcher(&self, name: &str) -> Option<Arc<dyn Fetcher>> {
        self.fetchers.iter().find(|s| s.load().name() == name).map(|s| s.load())
    }

    pub fn resolve_builder(&self, name: &str) -> Option<Arc<dyn Builder>> {
        self.builders.iter().find(|s| s.load().name() == name).map(|s| s.load())
    }

    pub fn resolve_packer(&self, name: &str) -> Option<Arc<dyn Packer>> {
        self.packers.iter().find(|s| s.load().name() == name).map(|s| s.load())
    }

    pub fn swap_fetcher(&self, name: &str, new: Arc<dyn Fetcher>) -> bool {
        for slot in &self.fetchers {
            if slot.load().name() == name {
                slot.swap(new);
                return true;
            }
        }
        false
    }

    pub fn swap_builder(&self, name: &str, new: Arc<dyn Builder>) -> bool {
        for slot in &self.builders {
            if slot.load().name() == name {
                slot.swap(new);
                return true;
            }
        }
        false
    }

    pub fn swap_packer(&self, name: &str, new: Arc<dyn Packer>) -> bool {
        for slot in &self.packers {
            if slot.load().name() == name {
                slot.swap(new);
                return true;
            }
        }
        false
    }

    pub fn default_fetcher(&self) -> Option<Arc<dyn Fetcher>> {
        self.fetchers.first().map(|s| s.load())
    }

    pub fn default_builder(&self) -> Option<Arc<dyn Builder>> {
        self.builders.first().map(|s| s.load())
    }

    pub fn default_packer(&self) -> Option<Arc<dyn Packer>> {
        self.packers.first().map(|s| s.load())
    }

    pub fn fetcher_count(&self) -> usize { self.fetchers.len() }
    pub fn builder_count(&self) -> usize { self.builders.len() }
    pub fn packer_count(&self) -> usize { self.packers.len() }
}

pub struct CurlFetcher;

impl Fetcher for CurlFetcher {
    fn fetch(&self, source: &str, destination: &str) -> Result<()> {
        std::fs::create_dir_all(destination)?;
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("curl -fSL -o /dev/stdout '{}' | tar -xz --strip-components=1 -C '{}'", source, destination))
            .status()?;
        if !status.success() {
            let status2 = std::process::Command::new("git")
                .arg("clone")
                .arg("--depth")
                .arg("1")
                .arg(source)
                .arg(destination)
                .status()?;
            if !status2.success() {
                anyhow::bail!("Failed to fetch source: {}", source);
            }
        }
        Ok(())
    }
    fn name(&self) -> &'static str { "curl" }
}

pub struct DefaultBuilder;

impl Builder for DefaultBuilder {
    fn build(&self, build_cmd: &str, source_dir: &str, _dest_dir: &str, build_type: &str) -> Result<String> {
        if build_cmd.trim().eq_ignore_ascii_case("none")
            || build_cmd.trim().eq_ignore_ascii_case("skip")
            || build_cmd.trim().eq_ignore_ascii_case("nothing") {
            return Ok(String::new());
        }
        if build_cmd.is_empty() && build_type == "rust" {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg("RUSTFLAGS=\"-C linker=clang -C link-arg=-target -C link-arg=x86_64-pc-linux-musl -C link-arg=--sysroot=/system -C target-feature=+crt-static\" cargo build --target x86_64-unknown-linux-musl --release 2>&1")
                .current_dir(source_dir)
                .output()?;
            let log = String::from_utf8_lossy(&output.stdout).to_string();
            if !output.status.success() { anyhow::bail!("Build failed:\n{}", log); }
            return Ok(log);
        }
        if !build_cmd.is_empty() {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("({}) 2>&1", build_cmd))
                .current_dir(source_dir)
                .output()?;
            let log = String::from_utf8_lossy(&output.stdout).to_string();
            if !output.status.success() { anyhow::bail!("Build failed:\n{}", log); }
            return Ok(log);
        }
        Ok(String::new())
    }
    fn name(&self) -> &'static str { "default" }
}

pub struct ZstdPacker;

impl Packer for ZstdPacker {
    fn pack(&self, source_dir: &str, output_path: &str, compression_level: i32) -> Result<()> {
        let _ = std::fs::remove_file(output_path);
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("tar -c -C '{}' . | zstd -{} -o '{}'", source_dir, compression_level, output_path))
            .status()?;
        if !status.success() {
            anyhow::bail!("Failed to pack archive: {}", output_path);
        }
        Ok(())
    }
    fn unpack(&self, archive_path: &str, dest_dir: &str) -> Result<Vec<String>> {
        std::fs::create_dir_all(dest_dir)?;
        let file = std::fs::File::open(archive_path)?;
        let decoder = zstd::stream::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        let mut files = Vec::new();
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                anyhow::bail!("Path traversal detected: {:?}", path);
            }
            let dest = std::path::Path::new(dest_dir).join(&path);
            if let Some(parent) = dest.parent() { std::fs::create_dir_all(parent)?; }
            entry.unpack(&dest)?;
            files.push(path.to_string_lossy().to_string());
        }
        Ok(files)
    }
    fn name(&self) -> &'static str { "zstd" }
}
