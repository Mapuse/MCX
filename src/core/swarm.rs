use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SwarmPeer {
    pub address: String,
    pub peer_id: String,
    pub last_seen: u64,
    pub advertised_hashes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SwarmHashEntry {
    pub pkg_name: String,
    pub version: String,
    pub swarm_hash: String,
}

pub struct SwarmManager {
    swarm_dir: PathBuf,
}

impl SwarmManager {
    pub fn new(root: &Path) -> Self {
        Self {
            swarm_dir: root.join("var/lib/mcx/swarm"),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.swarm_dir)
            .context("Failed to allocate P2P swarm state directory")
    }

    pub fn register_swarm_hash(&self, pkg_name: &str, version: &str, swarm_hash: &str) -> Result<()> {
        let entry = SwarmHashEntry {
            pkg_name: pkg_name.to_string(),
            version: version.to_string(),
            swarm_hash: swarm_hash.to_string(),
        };
        let target = self.swarm_dir.join(format!("{}.json", pkg_name));
        let payload = serde_json::to_string_pretty(&entry)?;
        let tmp = target.with_extension("tmp");
        fs::write(&tmp, &payload)?;
        fs::rename(&tmp, &target)?;
        Ok(())
    }

    pub fn get_swarm_hash(&self, pkg_name: &str) -> Result<Option<String>> {
        let target = self.swarm_dir.join(format!("{}.json", pkg_name));
        if !target.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&target)?;
        let entry: SwarmHashEntry = serde_json::from_str(&content)?;
        Ok(Some(entry.swarm_hash))
    }

    pub fn register_swarm_peer(&self, peer: SwarmPeer) -> Result<()> {
        let peers_path = self.swarm_dir.join("peers.json");
        let mut peers: Vec<SwarmPeer> = if peers_path.exists() {
            let content = fs::read_to_string(&peers_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        if let Some(existing) = peers.iter_mut().find(|p| p.peer_id == peer.peer_id) {
            existing.last_seen = peer.last_seen;
            existing.address = peer.address;
            existing.advertised_hashes = peer.advertised_hashes;
        } else {
            peers.push(peer);
        }

        let payload = serde_json::to_string_pretty(&peers)?;
        let tmp = peers_path.with_extension("tmp");
        fs::write(&tmp, &payload)?;
        fs::rename(&tmp, &peers_path)?;
        Ok(())
    }

    pub fn list_swarm_peers(&self) -> Result<Vec<SwarmPeer>> {
        let peers_path = self.swarm_dir.join("peers.json");
        if !peers_path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&peers_path)?;
        let peers: Vec<SwarmPeer> = serde_json::from_str(&content)?;
        Ok(peers)
    }

    pub fn remove_swarm_entry(&self, pkg_name: &str) -> Result<()> {
        let target = self.swarm_dir.join(format!("{}.json", pkg_name));
        if target.exists() {
            fs::remove_file(&target)?;
        }
        Ok(())
    }
}
