//! Persistent local registry for OpenNodia native AMM pools.

use std::path::PathBuf;

use base64::Engine;
use opennodia_amm::{PoolKey, PoolState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct LpRegistry {
    path: PathBuf,
    entries: Vec<LpRegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpRegistryEntry {
    pub pool_id: String,
    pub app_id: u64,
    pub genesis_hash_b64: String,
    pub asset_0: u64,
    pub asset_1: u64,
    pub fee_bps: u16,
    pub curve_id: u16,
    pub contract_version: u16,
    pub lp_asset_id: u64,
    pub source_round: u64,
}

impl LpRegistry {
    pub fn empty(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
        }
    }

    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::empty(path));
        }
        let bytes = std::fs::read(&path)?;
        let entries = serde_json::from_slice::<Vec<LpRegistryEntry>>(&bytes)?;
        Ok(Self { path, entries })
    }

    pub fn upsert(&mut self, pool: &PoolState) -> anyhow::Result<()> {
        let entry = LpRegistryEntry::from_pool(pool);
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.pool_id == entry.pool_id)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
        self.save()
    }

    pub fn find_pool_key(&self, key: &PoolKey) -> Option<&LpRegistryEntry> {
        let pool_id = key.id();
        self.entries.iter().find(|entry| {
            entry.pool_id == pool_id && entry.genesis_hash_b64 == b64(&key.genesis_hash)
        })
    }

    pub fn entries(&self) -> Vec<LpRegistryEntry> {
        self.entries.clone()
    }

    pub fn entries_for_pair(
        &self,
        genesis_hash: [u8; 32],
        asset_a: u64,
        asset_b: u64,
    ) -> Vec<LpRegistryEntry> {
        let (asset_0, asset_1) = if asset_a < asset_b {
            (asset_a, asset_b)
        } else {
            (asset_b, asset_a)
        };
        let genesis_hash_b64 = b64(&genesis_hash);
        self.entries
            .iter()
            .filter(|entry| {
                entry.genesis_hash_b64 == genesis_hash_b64
                    && entry.asset_0 == asset_0
                    && entry.asset_1 == asset_1
            })
            .cloned()
            .collect()
    }

    fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.entries)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

impl LpRegistryEntry {
    fn from_pool(pool: &PoolState) -> Self {
        Self {
            pool_id: pool.key.id(),
            app_id: pool.app_id,
            genesis_hash_b64: b64(&pool.key.genesis_hash),
            asset_0: pool.key.asset_0,
            asset_1: pool.key.asset_1,
            fee_bps: pool.key.fee_bps,
            curve_id: pool.key.curve_id,
            contract_version: pool.key.contract_version,
            lp_asset_id: pool.lp_asset_id,
            source_round: pool.source_round,
        }
    }
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
