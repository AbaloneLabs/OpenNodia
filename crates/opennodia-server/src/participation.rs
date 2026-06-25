//! Node participation and block info tracker.
//!
//! Runs as a background task that periodically:
//! 1. Fetches recent block headers (round, timestamp, txn count, proposer).
//! 2. Identifies the local node's participation address(es) via `GET /v2/participation`.
//! 3. Counts how many of the cached blocks were proposed by the local node.
//!
//! The tracker caches the latest 30 blocks so API handlers can serve them
//! instantly.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use opennodia_node::{AlgodClient, BlockHeader};
use serde::Serialize;
use tokio::sync::Mutex;

/// How often the background poller refreshes data.
const POLL_INTERVAL: Duration = Duration::from_secs(10);
/// How many recent blocks to cache and expose via the API.
const CACHE_SIZE: usize = 30;

/// Thread-safe participation / block-info tracker.
#[derive(Clone)]
pub struct ParticipationTracker {
    inner: Arc<Mutex<Inner>>,
    algod: AlgodClient,
}

struct Inner {
    /// Cached recent block headers, newest-first (index 0 = latest).
    recent_blocks: Vec<BlockHeader>,
    /// Participation addresses registered on the local node.
    participation_addresses: HashSet<String>,
    /// Blocks proposed by the local node within the cached window.
    blocks_proposed: u64,
}

/// API response for `GET /api/node/block-info`.
#[derive(Debug, Clone, Serialize)]
pub struct BlockInfo {
    /// Block round.
    pub round: u64,
    /// Block timestamp (Unix seconds).
    pub timestamp: i64,
    /// Number of transactions in this block.
    pub txn_count: u64,
    /// Block proposer address (base32), empty if not available.
    pub proposer: String,
    /// Total fees collected in this block (microAlgos).
    pub fees_collected: u64,
    /// Proposer payout for this block (microAlgos).
    pub proposer_payout: u64,
}

/// API response for `GET /api/node/participation-stats`.
#[derive(Debug, Clone, Serialize)]
pub struct ParticipationStats {
    /// Whether the local node has participation keys registered.
    pub participating: bool,
    /// Participation addresses (base32).
    pub addresses: Vec<String>,
    /// Blocks proposed by the local node within the cached window.
    pub blocks_proposed: u64,
    /// Total blocks scanned in the window.
    pub blocks_scanned: u64,
}

impl ParticipationTracker {
    /// Create a new tracker bound to the local algod client.
    pub fn new(algod: AlgodClient) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                recent_blocks: Vec::new(),
                participation_addresses: HashSet::new(),
                blocks_proposed: 0,
            })),
            algod,
        }
    }

    /// Background poll loop. Call via `tokio::spawn`.
    pub async fn run(&self) {
        tracing::info!(
            "participation tracker started ({}s interval)",
            POLL_INTERVAL.as_secs()
        );
        loop {
            if let Err(e) = self.poll_once().await {
                tracing::debug!(error = %e, "participation tracker poll failed");
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn poll_once(&self) -> anyhow::Result<()> {
        // Refresh participation keys.
        let part_keys = self.algod.participation_keys().await.unwrap_or_default();
        let part_addresses: HashSet<String> = part_keys
            .iter()
            .map(|k| k.key.parent.clone())
            .filter(|a| !a.is_empty())
            .collect();

        // Determine the range of blocks to fetch.
        let status = self.algod.status().await?;
        let current_round = status.last_round.as_u64();

        let last_cached_round = {
            let inner = self.inner.lock().await;
            inner.recent_blocks.first().map(|b| b.round)
        };

        // Fetch new blocks since the last cached round (or the latest
        // CACHE_SIZE blocks on first run).
        let start_round = match last_cached_round {
            Some(last) if last >= current_round => current_round, // nothing new
            Some(last) => last + 1,
            None => current_round.saturating_sub((CACHE_SIZE as u64) - 1),
        };

        let mut new_blocks: Vec<BlockHeader> = Vec::new();
        if start_round <= current_round {
            for r in (start_round..=current_round).rev() {
                match self.algod.block_header(r).await {
                    Ok(hdr) => new_blocks.push(hdr),
                    Err(_) => break,
                }
                // Yield periodically to avoid blocking the runtime.
                if new_blocks.len().is_multiple_of(10) {
                    tokio::task::yield_now().await;
                }
            }
        }

        // Merge new blocks into the cache (newest-first).
        let mut inner = self.inner.lock().await;
        if !new_blocks.is_empty() {
            // The first new block may overlap with the cache head; skip
            // rounds we already have.
            let existing_top = inner.recent_blocks.first().map(|b| b.round);
            let filtered: Vec<BlockHeader> = new_blocks
                .into_iter()
                .filter(|b| Some(b.round) > existing_top || existing_top.is_none())
                .collect();
            // Insert newest-first: extend with the older cached blocks.
            let mut merged = filtered;
            merged.append(&mut inner.recent_blocks);
            merged.truncate(CACHE_SIZE);
            inner.recent_blocks = merged;
        }

        // Recompute participation stats from the cached window.
        inner.participation_addresses = part_addresses;
        let addresses = inner.participation_addresses.clone();
        inner.blocks_proposed = inner
            .recent_blocks
            .iter()
            .filter(|b| !b.proposer.is_empty() && addresses.contains(&b.proposer))
            .count() as u64;

        Ok(())
    }

    /// Get the cached recent blocks as API DTOs (newest-first).
    pub async fn block_info(&self) -> Vec<BlockInfo> {
        let inner = self.inner.lock().await;
        inner
            .recent_blocks
            .iter()
            .enumerate()
            .map(|(i, block)| {
                // txn_count = this block's cumulative counter minus the
                // previous (older) block's counter. The older block is at
                // index i+1 in our newest-first list.
                let prev_txn_counter = inner.recent_blocks.get(i + 1).map(|b| b.txn_counter);
                BlockInfo {
                    round: block.round,
                    timestamp: block.timestamp,
                    txn_count: block.txn_count(prev_txn_counter),
                    proposer: block.proposer.clone(),
                    fees_collected: block.fees_collected,
                    proposer_payout: block.proposer_payout,
                }
            })
            .collect()
    }

    /// Get participation statistics.
    pub async fn participation_stats(&self) -> ParticipationStats {
        let inner = self.inner.lock().await;
        ParticipationStats {
            participating: !inner.participation_addresses.is_empty(),
            addresses: inner.participation_addresses.iter().cloned().collect(),
            blocks_proposed: inner.blocks_proposed,
            blocks_scanned: inner.recent_blocks.len() as u64,
        }
    }
}
