use crate::commands::add::AddLocalCommand;
use crate::core::database::Database;
use crate::core::localsrc::{
    LocalSource, LocalSourceAction, LocalSourceManager, LocalSourceResult, SourceKind, SourceMode,
    build_ous_args, check_download_tools, classify_source_url, compute_prebuilt_fingerprint,
    decide_action, find_ous_binary, materialize_source, parse_xcs_filename, predict_output_names,
    scan_xcs_dir, source_manifest, validate_source_name, versions_outdated,
};
use crate::utils::ui::UserInterface;
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct LocalSourceCommand {
    root: PathBuf,
    db: Arc<Database>,
}

impl LocalSourceCommand {
    pub fn new(root: &Path, db: Arc<Database>) -> Self {
        Self {
            root: root.to_path_buf(),
            db,
        }
    }

    fn manager(&self) -> LocalSourceManager {
        LocalSourceManager::new(&self.root)
    }

    pub fn add(&self, name: &str, path_or_url: &str, prebuilt: bool, enabled: bool) -> Result<()> {
        validate_source_name(name)?;
        let path = path_or_url.trim();
        if path.is_empty() {
            return Err(anyhow!("Source path/URL must not be empty"));
        }
        let class = classify_source_url(path);
        let mode = if prebuilt {
            SourceMode::Prebuilt
        } else {
            SourceMode::Source
        };
        if prebuilt {
            if class != SourceKind::Dir && class != SourceKind::FileDir {
                return Err(anyhow!(
                    "Prebuilt local sources must be a local directory of .xcs archives, not a URL"
                ));
            }
            let dir = if class == SourceKind::FileDir {
                PathBuf::from(crate::core::localsrc::strip_file_scheme(path))
            } else {
                PathBuf::from(path)
            };
            if !dir.is_dir() {
                return Err(anyhow!("Prebuilt source directory not found: '{}'", path));
            }
            if scan_xcs_dir(&dir)?.is_empty() {
                UserInterface::warning(&format!(
                    "No .xcs archives found in '{}'; the source will skip until archives appear",
                    path
                ));
            }
        } else {
            match class {
                SourceKind::Dir | SourceKind::FileDir => {
                    let local = if class == SourceKind::FileDir {
                        crate::core::localsrc::strip_file_scheme(path)
                    } else {
                        path.to_string()
                    };
                    if !source_manifest(&local).exists() {
                        return Err(anyhow!("Source path has no manifest.json: '{}'", local));
                    }
                }
                SourceKind::Git | SourceKind::Archive | SourceKind::Http | SourceKind::JsonUrl => {
                    // Remote/git/download sources validate at build/pull time.
                }
            }
        }

        self.manager().initialize()?;
        self.manager().add_source(LocalSource {
            name: name.to_string(),
            path: path.to_string(),
            mode,
            enabled,
            last_built: None,
        })
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        validate_source_name(name)?;
        self.manager().initialize()?;
        self.manager().remove_source(name)
    }

    pub fn list(&self) -> Result<Vec<LocalSource>> {
        self.manager().initialize()?;
        self.manager().load_sources()
    }

    pub fn build(&self, names: Vec<String>, force: bool) -> Result<()> {
        self.manager().initialize()?;
        let all = self.manager().load_sources()?;
        let targets: Vec<LocalSource> = if names.is_empty() {
            all.into_iter().filter(|s| s.enabled).collect()
        } else {
            let mut selected = Vec::new();
            for name in &names {
                let source = all
                    .iter()
                    .find(|s| s.name == *name)
                    .ok_or_else(|| anyhow!("Local source '{}' not found", name))?;
                selected.push(source.clone());
            }
            selected
        };
        if targets.is_empty() {
            return Err(anyhow!("No enabled local sources to build"));
        }

        let ous_bin = find_ous_binary();
        if targets.iter().any(|s| s.mode == SourceMode::Source) && ous_bin.is_none() {
            return Err(anyhow!(
                "ous binary not found (set OUS_BIN or install ous on PATH); cannot build source-mode local sources"
            ));
        }

        let mut failures = 0usize;
        for source in &targets {
            match self.build_one(source, force, ous_bin.as_deref()) {
                Ok(LocalSourceResult::Skipped(reason)) => {
                    UserInterface::info(&format!("Local source '{}': {}", source.name, reason));
                }
                Ok(LocalSourceResult::Built) => {
                    UserInterface::success(&format!(
                        "Local source '{}': built and installed.",
                        source.name
                    ));
                }
                Ok(LocalSourceResult::InstalledPrebuilt) => {
                    UserInterface::success(&format!(
                        "Local source '{}': prebuilt archives installed.",
                        source.name
                    ));
                }
                Err(e) => {
                    UserInterface::error(&format!("Local source '{}' failed: {}", source.name, e));
                    failures += 1;
                }
            }
        }
        if failures > 0 {
            return Err(anyhow!("{} local source(s) failed", failures));
        }
        Ok(())
    }

    pub fn sync_local_sources(&self) -> Result<()> {
        self.manager().initialize()?;
        let sources: Vec<LocalSource> = self
            .manager()
            .load_sources()?
            .into_iter()
            .filter(|s| s.enabled)
            .collect();
        if sources.is_empty() {
            return Ok(());
        }

        let needs_ous = sources.iter().any(|s| s.mode == SourceMode::Source);
        let ous_bin = find_ous_binary();
        if needs_ous && ous_bin.is_none() {
            UserInterface::warning(
                "ous binary not found (set OUS_BIN or install ous on PATH); skipping build of source packages",
            );
        }

        for source in &sources {
            match self.build_one(source, false, ous_bin.as_deref()) {
                Ok(LocalSourceResult::Skipped(reason)) => {
                    UserInterface::info(&format!("Local source '{}': {}", source.name, reason));
                }
                Ok(_) => {}
                Err(e) => {
                    UserInterface::warning(&format!(
                        "Local source '{}' failed: {}",
                        source.name, e
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn build_one(
        &self,
        source: &LocalSource,
        force: bool,
        ous_bin: Option<&str>,
    ) -> Result<LocalSourceResult> {
        self.manager().initialize()?;
        match source.mode {
            SourceMode::Prebuilt => self.build_prebuilt(source, force),
            SourceMode::Source => self.build_from_source(source, force, ous_bin),
        }
    }

    fn build_prebuilt(&self, source: &LocalSource, force: bool) -> Result<LocalSourceResult> {
        let dir = PathBuf::from(&source.path);
        if !dir.is_dir() {
            return Err(anyhow!(
                "Prebuilt source directory not found: '{}'",
                source.path
            ));
        }
        let archives = scan_xcs_dir(&dir)?;
        if archives.is_empty() {
            return Ok(LocalSourceResult::Skipped(
                "no .xcs archives found".to_string(),
            ));
        }

        let fingerprint = compute_prebuilt_fingerprint(&dir)?;
        let mut packages: Vec<(String, String)> = Vec::new();
        for archive in &archives {
            if let Some(file_name) = archive.file_name().and_then(|n| n.to_str())
                && let Some((name, version)) = parse_xcs_filename(file_name)
            {
                packages.push((name, version));
            }
        }
        let outdated = versions_outdated(&self.db, &packages);

        match decide_action(
            Some(&fingerprint),
            source.last_built.as_deref(),
            force,
            outdated,
        ) {
            LocalSourceAction::Skip => Ok(LocalSourceResult::Skipped(
                "unchanged, no install needed".to_string(),
            )),
            LocalSourceAction::Rebuild => {
                let add = AddLocalCommand::new(
                    self.root.to_string_lossy().into_owned(),
                    Arc::clone(&self.db),
                );
                for archive in &archives {
                    add.execute(&archive.to_string_lossy())?;
                }
                self.manager()
                    .update_last_built(&source.name, &fingerprint)?;
                Ok(LocalSourceResult::InstalledPrebuilt)
            }
        }
    }

    fn build_from_source(
        &self,
        source: &LocalSource,
        force: bool,
        ous_bin: Option<&str>,
    ) -> Result<LocalSourceResult> {
        let ous = match ous_bin {
            Some(ous) => ous.to_string(),
            None => {
                UserInterface::warning(&format!(
                    "ous binary not found; skipping build of '{}'",
                    source.name
                ));
                return Ok(LocalSourceResult::Skipped(
                    "ous binary not available".to_string(),
                ));
            }
        };

        if let Some(reason) = check_download_tools(&source.path) {
            UserInterface::warning(&format!("Local source '{}': {}", source.name, reason));
            return Ok(LocalSourceResult::Skipped(
                "download tool missing".to_string(),
            ));
        }

        let materialized = materialize_source(&self.manager(), source)?;
        let manifest = materialized.manifest;
        let fingerprint = materialized.fingerprint;

        let output_names = predict_output_names(&manifest)?;
        let packages: Vec<(String, String)> = output_names
            .iter()
            .filter_map(|name| parse_xcs_filename(name))
            .collect();
        let outdated = versions_outdated(&self.db, &packages);

        match decide_action(
            Some(&fingerprint),
            source.last_built.as_deref(),
            force,
            outdated,
        ) {
            LocalSourceAction::Skip => Ok(LocalSourceResult::Skipped(
                "unchanged, no rebuild needed".to_string(),
            )),
            LocalSourceAction::Rebuild => {
                let out_dir = self.manager().out_dir().join(&source.name);
                if out_dir.exists() {
                    fs::remove_dir_all(&out_dir)?;
                }
                fs::create_dir_all(&out_dir)?;

                let manifest_parent = manifest
                    .parent()
                    .ok_or_else(|| anyhow!("Manifest has no parent directory"))?;
                let args = build_ous_args(&manifest.to_string_lossy(), &out_dir.to_string_lossy());
                let status = std::process::Command::new(&ous)
                    .args(&args)
                    .current_dir(manifest_parent)
                    .env_remove("OUS_UPLOAD_URL")
                    .env_remove("OUS_UPLOAD_TOKEN")
                    .env_remove("OUS_UPLOAD_INDEX")
                    .status()
                    .with_context(|| format!("failed to run ous ({})", ous))?;
                if !status.success() {
                    return Err(anyhow!(
                        "ous build failed for '{}' (exit status {})",
                        source.name,
                        status
                    ));
                }

                let mut expected_names = output_names;
                if expected_names.is_empty() {
                    expected_names = scan_xcs_dir(&out_dir)?
                        .into_iter()
                        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
                        .collect();
                }

                let add = AddLocalCommand::new(
                    self.root.to_string_lossy().into_owned(),
                    Arc::clone(&self.db),
                );
                let mut installed = 0usize;
                for name in &expected_names {
                    let candidate = out_dir.join(name);
                    if !candidate.exists() {
                        return Err(anyhow!("ous did not produce expected archive '{}'", name));
                    }
                    add.execute(&candidate.to_string_lossy())?;
                    installed += 1;
                }
                if installed == 0 {
                    return Err(anyhow!("ous build produced no installable archives"));
                }
                self.manager()
                    .update_last_built(&source.name, &fingerprint)?;
                Ok(LocalSourceResult::Built)
            }
        }
    }
}
