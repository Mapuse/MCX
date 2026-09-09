use crate::core::plugin::PluginSlot;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

pub enum PackageThreat {
    SuspiciousFiles,
    UnexpectedNetworkAccess,
    ResourceAbuse,
}

pub struct PackageGuard {
    pub pkg_name: String,
    pub isolated: bool,
}

pub struct SecurityMonitor {
    active_packages: RwLock<HashMap<String, PackageGuard>>,
    isolation_slot: PluginSlot<dyn Fn(&str) -> bool + Send + Sync>,
}

impl Default for SecurityMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityMonitor {
    pub fn new() -> Self {
        Self {
            active_packages: RwLock::new(HashMap::new()),
            isolation_slot: PluginSlot::new(Arc::new(|_pkg: &str| -> bool { true })),
        }
    }

    pub fn register_package(&self, pkg_name: &str) {
        if let Ok(mut map) = self.active_packages.write() {
            map.entry(pkg_name.to_string()).or_insert(PackageGuard {
                pkg_name: pkg_name.to_string(),
                isolated: false,
            });
        }
    }

    pub fn unregister_package(&self, pkg_name: &str) {
        if let Ok(mut map) = self.active_packages.write() {
            map.remove(pkg_name);
        }
    }

    pub fn isolate_package(&self, pkg_name: &str) -> Result<()> {
        if let Ok(mut map) = self.active_packages.write()
            && let Some(guard) = map.get_mut(pkg_name)
        {
            guard.isolated = true;
        }
        Ok(())
    }

    pub fn is_package_isolated(&self, pkg_name: &str) -> bool {
        self.active_packages
            .read()
            .map(|map| map.get(pkg_name).map(|g| g.isolated).unwrap_or(false))
            .unwrap_or(false)
    }

    pub fn swap_isolation_policy(
        &self,
        policy: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    ) -> Arc<dyn Fn(&str) -> bool + Send + Sync> {
        self.isolation_slot.swap(policy)
    }

    pub fn check_package(&self, pkg_name: &str) -> bool {
        self.isolation_slot.load()(pkg_name)
    }

    pub fn active_count(&self) -> usize {
        self.active_packages
            .read()
            .map(|map| map.len())
            .unwrap_or(0)
    }
}
