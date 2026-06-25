//! Node synchronization progress tracker.
//!
//! Estimates how long the local algod node needs to finish catching up by:
//! 1. Sampling the local node's current round at intervals.
//! 2. Querying the public relay API for the network's latest round.
//! 3. Computing blocks/sec from the sample history and extrapolating ETA.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opennodia_core::Network;
use serde::Serialize;
use tokio::sync::Mutex;

/// Maximum number of round samples to retain for speed calculation.
const MAX_SAMPLES: usize = 20;
/// Minimum number of samples required before we can estimate speed.
const MIN_SAMPLES_FOR_ETA: usize = 2;
/// How long to keep samples before discarding them.
const SAMPLE_WINDOW: Duration = Duration::from_secs(600); // 10 minutes
/// TTL for the cached network (public API) round.
const NETWORK_ROUND_TTL: Duration = Duration::from_secs(120); // 2 minutes

/// A single (timestamp, local round) sample.
#[derive(Debug, Clone, Copy)]
struct RoundSample {
    at: Instant,
    round: u64,
}

/// Thread-safe sync progress tracker.
#[derive(Clone)]
pub struct SyncTracker {
    inner: Arc<Mutex<Inner>>,
    network: Network,
    http: reqwest::Client,
}

struct Inner {
    samples: VecDeque<RoundSample>,
    network_round: Option<u64>,
    network_round_fetched_at: Option<Instant>,
}

/// API response for `GET /api/node/sync-progress`.
#[derive(Debug, Clone, Serialize)]
pub struct SyncProgress {
    /// The local node's current round.
    pub local_round: u64,
    /// The network's latest round (from public relay), if known.
    pub network_round: Option<u64>,
    /// Whether the local node is fully synced.
    pub synced: bool,
    /// Sync progress as a percentage (0.0–100.0).
    pub progress_pct: Option<f64>,
    /// How many rounds the local node is behind.
    pub rounds_behind: Option<u64>,
    /// Estimated sync speed in blocks per second.
    pub blocks_per_sec: Option<f64>,
    /// Estimated seconds remaining to full sync.
    pub estimated_seconds_remaining: Option<u64>,
}

impl SyncTracker {
    /// Create a new tracker for the given network.
    pub fn new(network: Network) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("opennodia/0.1")
            .build()
            .expect("reqwest client build");

        Self {
            inner: Arc::new(Mutex::new(Inner {
                samples: VecDeque::with_capacity(MAX_SAMPLES),
                network_round: None,
                network_round_fetched_at: None,
            })),
            network,
            http,
        }
    }

    /// Record a new local round sample and refresh the network round if needed.
    /// Should be called on every status poll (e.g., every 5 seconds).
    pub async fn record(&self, local_round: u64, synced: bool) {
        let now = Instant::now();
        let mut inner = self.inner.lock().await;

        // Add the new sample.
        inner.samples.push_back(RoundSample {
            at: now,
            round: local_round,
        });

        // Prune old samples.
        while inner.samples.len() > MAX_SAMPLES {
            inner.samples.pop_front();
        }
        let cutoff = now - SAMPLE_WINDOW;
        while let Some(front) = inner.samples.front() {
            if front.at < cutoff {
                inner.samples.pop_front();
            } else {
                break;
            }
        }

        // Refresh network round if stale or missing (only for public networks).
        let need_fetch = self.network.public_api_url().is_some()
            && (inner.network_round.is_none()
                || inner
                    .network_round_fetched_at
                    .map(|t| now.duration_since(t) > NETWORK_ROUND_TTL)
                    .unwrap_or(true));

        // Release the lock before the async HTTP call.
        let network = self.network;
        let http = self.http.clone();
        drop(inner);

        if need_fetch {
            if let Some(url) = network.public_api_url() {
                match fetch_network_round(&http, url).await {
                    Ok(round) => {
                        let mut inner = self.inner.lock().await;
                        inner.network_round = Some(round);
                        inner.network_round_fetched_at = Some(Instant::now());
                        tracing::debug!(network_round = round, "fetched network round");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to fetch network round from public API");
                    }
                }
            }
        }

        // If synced, we don't need the network round to be precise.
        let _ = synced;
    }

    /// Compute the current sync progress.
    pub async fn progress(&self, local_round: u64, synced: bool) -> SyncProgress {
        let inner = self.inner.lock().await;

        let network_round = inner.network_round;

        // Calculate blocks/sec from samples.
        let blocks_per_sec = calc_speed(&inner.samples);

        let (progress_pct, rounds_behind, estimated_seconds_remaining) =
            if let Some(net_round) = network_round {
                if net_round <= local_round {
                    (Some(100.0), Some(0), Some(0))
                } else {
                    let behind = net_round - local_round;
                    let pct = (local_round as f64 / net_round as f64) * 100.0;
                    let eta = blocks_per_sec
                        .filter(|s| *s > 0.0)
                        .map(|s| (behind as f64 / s) as u64);
                    (Some(pct), Some(behind), eta)
                }
            } else {
                (None, None, None)
            };

        SyncProgress {
            local_round,
            network_round,
            synced,
            progress_pct,
            rounds_behind,
            blocks_per_sec,
            estimated_seconds_remaining,
        }
    }
}

/// Calculate blocks per second from the sample history.
fn calc_speed(samples: &VecDeque<RoundSample>) -> Option<f64> {
    if samples.len() < MIN_SAMPLES_FOR_ETA {
        return None;
    }
    let first = samples.front()?;
    let last = samples.back()?;
    let elapsed = last.at.duration_since(first.at).as_secs_f64();
    if elapsed < 1.0 {
        return None;
    }
    let rounds = last.round.saturating_sub(first.round);
    if rounds == 0 {
        return None;
    }
    Some(rounds as f64 / elapsed)
}

/// Fetch the latest round from a public relay API.
async fn fetch_network_round(http: &reqwest::Client, base_url: &str) -> anyhow::Result<u64> {
    let url = format!("{base_url}/v2/status");
    let resp: serde_json::Value = http
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("network status: {e}"))?
        .error_for_status()?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("network status decode: {e}"))?;
    let round = resp
        .get("last-round")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing last-round in network status"))?;
    Ok(round)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_speed_basic() {
        let mut samples = VecDeque::new();
        let now = Instant::now();
        samples.push_back(RoundSample {
            at: now,
            round: 100,
        });
        samples.push_back(RoundSample {
            at: now + Duration::from_secs(10),
            round: 200,
        });
        let speed = calc_speed(&samples).unwrap();
        assert!((speed - 10.0).abs() < 0.1); // 100 rounds / 10 sec = 10 blocks/sec
    }

    #[test]
    fn calc_speed_insufficient_samples() {
        let mut samples = VecDeque::new();
        samples.push_back(RoundSample {
            at: Instant::now(),
            round: 100,
        });
        assert!(calc_speed(&samples).is_none());
    }

    #[test]
    fn calc_speed_no_progress() {
        let mut samples = VecDeque::new();
        let now = Instant::now();
        samples.push_back(RoundSample {
            at: now,
            round: 100,
        });
        samples.push_back(RoundSample {
            at: now + Duration::from_secs(10),
            round: 100, // no progress
        });
        assert!(calc_speed(&samples).is_none());
    }
}

// ===========================================================================
// Indexer sync tracker
// ===========================================================================

/// Thread-safe indexer sync progress tracker.
///
/// Mirrors [`SyncTracker`] but tracks the indexer's indexing progress instead
/// of algod's catchup. The indexer reports its indexed round via `GET /health`,
/// which we compare against the network round to compute progress.
#[derive(Clone)]
pub struct IndexerSyncTracker {
    inner: Arc<Mutex<IndexerInner>>,
    http: reqwest::Client,
}

struct IndexerInner {
    samples: VecDeque<RoundSample>,
    network_round: Option<u64>,
    /// Cached local indexer health.
    health: Option<IndexerHealth>,
    /// The indexer base URL to poll (local if configured, else public).
    indexer_url: Option<String>,
    /// Public indexer URL used as the network-round reference.
    network_indexer_url: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct IndexerHealth {
    /// The latest round the indexer has processed.
    round: u64,
    /// Current network round when explicitly reported by the endpoint.
    current_round: Option<u64>,
}

/// API response for indexer sync progress.
#[derive(Debug, Clone, Serialize)]
pub struct IndexerSyncProgress {
    /// Whether the indexer is available (local or public).
    pub available: bool,
    /// The latest round the indexer has processed.
    pub indexed_round: Option<u64>,
    /// The network's latest round, if known.
    pub network_round: Option<u64>,
    /// Whether the indexer is fully caught up.
    pub synced: bool,
    /// Sync progress as a percentage (0.0–100.0).
    pub progress_pct: Option<f64>,
    /// How many rounds the indexer is behind.
    pub rounds_behind: Option<u64>,
    /// Estimated indexing speed in blocks per second.
    pub blocks_per_sec: Option<f64>,
    /// Estimated seconds remaining to full index.
    pub estimated_seconds_remaining: Option<u64>,
}

impl IndexerSyncTracker {
    /// Create a new indexer sync tracker.
    ///
    /// `has_local` / `has_public` determine which URL to poll. When a local
    /// indexer is configured it is preferred; otherwise the public indexer
    /// relay is used.
    pub fn new(network: Network, has_local: bool, has_public: bool) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("opennodia/0.1")
            .build()
            .expect("reqwest client build");

        let indexer_url = if has_local {
            // Local indexer default URL; the caller (AppState) overrides this
            // via `set_indexer_url` with the configured URL.
            Some("http://localhost:8980".to_string())
        } else if has_public {
            network.public_indexer_url().map(|s| s.to_string())
        } else {
            None
        };

        Self {
            inner: Arc::new(Mutex::new(IndexerInner {
                samples: VecDeque::with_capacity(MAX_SAMPLES),
                network_round: None,
                health: None,
                indexer_url,
                network_indexer_url: network.public_indexer_url().map(str::to_string),
            })),
            http,
        }
    }

    /// Override the indexer URL to poll (e.g. from config).
    ///
    /// This must be called before any `poll` / `progress` calls. Since the
    /// tracker is constructed synchronously in `AppState::from_config`, this
    /// setter is synchronous as well.
    pub fn set_indexer_url(&self, url: impl Into<String>) {
        // Use try_lock with a spin since this is called at construction time
        // before any async polling has started.
        if let Ok(mut inner) = self.inner.try_lock() {
            inner.indexer_url = Some(url.into());
        } else {
            tracing::warn!("indexer_sync_tracker lock busy during set_indexer_url");
        }
    }

    /// Poll the indexer health and record progress. Should be called
    /// periodically (e.g. every 10 seconds).
    pub async fn poll(&self) {
        let (url, network_indexer_url) = {
            let inner = self.inner.lock().await;
            match &inner.indexer_url {
                Some(u) => (u.clone(), inner.network_indexer_url.clone()),
                None => return, // no indexer configured
            }
        };

        // Fetch indexer health.
        match fetch_indexer_health(&self.http, &url).await {
            Ok(h) => {
                let now = Instant::now();
                let network_round = match h.current_round {
                    Some(round) => Some(round),
                    None if network_indexer_url.as_deref() == Some(url.as_str()) => Some(h.round),
                    None => {
                        if let Some(network_url) = network_indexer_url {
                            match fetch_indexer_health(&self.http, &network_url).await {
                                Ok(network_health) => Some(
                                    network_health.current_round.unwrap_or(network_health.round),
                                ),
                                Err(e) => {
                                    tracing::debug!(
                                        error = %e,
                                        "network indexer health poll failed"
                                    );
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    }
                };
                let mut inner = self.inner.lock().await;
                inner.health = Some(h);
                // Record a sample for speed calculation.
                inner.samples.push_back(RoundSample {
                    at: now,
                    round: h.round,
                });
                while inner.samples.len() > MAX_SAMPLES {
                    inner.samples.pop_front();
                }
                let cutoff = now - SAMPLE_WINDOW;
                while let Some(front) = inner.samples.front() {
                    if front.at < cutoff {
                        inner.samples.pop_front();
                    } else {
                        break;
                    }
                }
                inner.network_round = network_round;
            }
            Err(e) => {
                tracing::debug!(error = %e, "indexer health poll failed");
            }
        }
    }

    /// Whether the indexer (local or public) is fully synced.
    ///
    /// Used by `effective_search_client` to decide whether to prefer the
    /// local indexer or fall back to the public relay.
    pub async fn is_synced(&self) -> bool {
        let inner = self.inner.lock().await;
        match (inner.health, inner.network_round) {
            (Some(h), Some(network_round)) => network_round <= h.round,
            _ => false,
        }
    }

    /// Compute the current indexer sync progress.
    pub async fn progress(&self) -> IndexerSyncProgress {
        let inner = self.inner.lock().await;

        let available = inner.indexer_url.is_some();
        if !available {
            return IndexerSyncProgress {
                available: false,
                indexed_round: None,
                network_round: None,
                synced: false,
                progress_pct: None,
                rounds_behind: None,
                blocks_per_sec: None,
                estimated_seconds_remaining: None,
            };
        }

        let indexed_round = inner.health.map(|h| h.round);
        let network_round = inner.network_round;
        let blocks_per_sec = calc_speed(&inner.samples);

        let synced = match (indexed_round, network_round) {
            (Some(idx), Some(net)) => net <= idx,
            _ => false,
        };

        let (progress_pct, rounds_behind, estimated_seconds_remaining) =
            if let (Some(idx), Some(net)) = (indexed_round, network_round) {
                if net <= idx {
                    (Some(100.0), Some(0), Some(0))
                } else {
                    let behind = net - idx;
                    let pct = if net > 0 {
                        (idx as f64 / net as f64) * 100.0
                    } else {
                        0.0
                    };
                    let eta = blocks_per_sec
                        .filter(|s| *s > 0.0)
                        .map(|s| (behind as f64 / s) as u64);
                    (Some(pct), Some(behind), eta)
                }
            } else {
                (None, None, None)
            };

        IndexerSyncProgress {
            available,
            indexed_round,
            network_round,
            synced,
            progress_pct,
            rounds_behind,
            blocks_per_sec,
            estimated_seconds_remaining,
        }
    }
}

/// Fetch the indexer health (indexed round + current round) from `GET /health`.
async fn fetch_indexer_health(
    http: &reqwest::Client,
    base_url: &str,
) -> anyhow::Result<IndexerHealth> {
    let url = format!("{base_url}/health");
    let resp: serde_json::Value = http
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("indexer health: {e}"))?
        .error_for_status()?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("indexer health decode: {e}"))?;
    parse_indexer_health(&resp)
}

fn parse_indexer_health(resp: &serde_json::Value) -> anyhow::Result<IndexerHealth> {
    let round = resp
        .get("round")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing round in indexer health"))?;
    let current_round = resp
        .get("current-round")
        .and_then(|v| v.as_u64())
        .or_else(|| resp.get("currentRound").and_then(|v| v.as_u64()));
    Ok(IndexerHealth {
        round,
        current_round,
    })
}

#[cfg(test)]
mod indexer_health_tests {
    use super::*;

    #[test]
    fn missing_current_round_stays_unknown() {
        let health = parse_indexer_health(&serde_json::json!({"round": 42})).unwrap();
        assert_eq!(health.round, 42);
        assert_eq!(health.current_round, None);
    }

    #[test]
    fn parses_explicit_current_round() {
        let health =
            parse_indexer_health(&serde_json::json!({"round": 40, "current-round": 42})).unwrap();
        assert_eq!(health.round, 40);
        assert_eq!(health.current_round, Some(42));
    }
}
