//! Shared application state shared across all handlers.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opennodia_node::{
    AlgodClient, DataSource, IndexerClient, IndexerTransaction, KmdClient, NodeStatus,
};
use tokio::sync::Mutex;

use crate::asa::AssetCreateIntent;
use crate::asa_history::AsaIssueStore;
use crate::asset_metadata::AssetMetadataStore;
use crate::auth::PinStore;
use crate::config::Config;
use crate::dex::DexIntentAction;
use crate::dex_validation::DexValidationRuntime;
use crate::external_liquidity::ExternalLiquidityIntentAction;
use crate::intent::IntentStore;
use crate::lp::LpIntentAction;
use crate::market::PriceCache;
use crate::router::RouterIntentAction;
use crate::session::SessionStore;
use crate::sync::{IndexerSyncTracker, SyncTracker};
use crate::wallet::WalletManager;
use crate::wallet_history::{
    BalanceSnapshotAsset, BalanceSnapshotRecord, HistorySource, PortfolioValueSnapshotInput,
    PortfolioValueSnapshotRecord, WalletHistoryQuery, WalletHistoryStore, WalletTransactionPage,
};

const AUTHORITATIVE_ROUND_LAG: u64 = 20;

/// A thread-safe handle to the local DEX orderbook (SQLite-backed).
///
/// `None` when the DEX database could not be opened (the DEX endpoints will
/// return 503 in that case). Wrapped in `Arc` so it can be cheaply cloned
/// into every handler.
pub type DexStore = Arc<opennodia_dex::DexDb>;

/// Ledger and node clients used by read/write paths.
#[derive(Clone)]
pub(crate) struct LedgerClients {
    pub(crate) algod: AlgodClient,
    pub(crate) read_algod: Option<AlgodClient>,
    pub(crate) public_algod: Option<AlgodClient>,
    pub(crate) indexer: Option<IndexerClient>,
    pub(crate) public_indexer: Option<IndexerClient>,
    pub(crate) local_indexer_state_complete: bool,
    pub(crate) kmd: KmdClient,
}

/// Persistent stores and registries opened at startup.
#[derive(Clone)]
pub(crate) struct PersistentStores {
    pub(crate) wallet_history: Option<WalletHistoryStore>,
    pub(crate) wallets: WalletManager,
    pub(crate) dex: Option<DexStore>,
    pub(crate) asa_issues: Option<Arc<AsaIssueStore>>,
    pub(crate) asset_metadata: Option<Arc<AssetMetadataStore>>,
    pub(crate) lp_registry: Arc<Mutex<crate::lp::LpRegistry>>,
}

/// One-time, session-bound transaction intent stores.
#[derive(Clone)]
pub(crate) struct IntentStores {
    pub(crate) dex: IntentStore<DexIntentAction>,
    pub(crate) asset_create: IntentStore<AssetCreateIntent>,
    pub(crate) lp: IntentStore<LpIntentAction>,
    pub(crate) external_liquidity: IntentStore<ExternalLiquidityIntentAction>,
    pub(crate) router: IntentStore<RouterIntentAction>,
}

/// Runtime-only state that is rebuilt on each server start.
#[derive(Clone)]
pub(crate) struct RuntimeState {
    pub(crate) sessions: SessionStore,
    pub(crate) pin: Arc<Mutex<Option<PinStore>>>,
    pub(crate) pin_attempts: Arc<Mutex<PinAttemptTracker>>,
    pub(crate) pin_path: PathBuf,
    pub(crate) prices: PriceCache,
    pub(crate) sync_tracker: SyncTracker,
    pub(crate) indexer_sync_tracker: IndexerSyncTracker,
    pub(crate) participation_tracker: crate::participation::ParticipationTracker,
    pub(crate) dex_validation: DexValidationRuntime,
    pub(crate) dex_reconcile_lock: Arc<Mutex<()>>,
}

/// Short-lived caches used to reduce repeated public fallback calls.
#[derive(Clone)]
pub(crate) struct AppCaches {
    pub(crate) account_info: Arc<
        Mutex<std::collections::HashMap<String, (std::time::Instant, opennodia_node::AccountInfo)>>,
    >,
    pub(crate) asset_params: Arc<
        Mutex<std::collections::HashMap<u64, (std::time::Instant, opennodia_node::AssetParams)>>,
    >,
}

#[derive(Debug, Default)]
pub(crate) struct PinAttemptTracker {
    failed_attempts: u32,
    locked_until: Option<Instant>,
}

impl PinAttemptTracker {
    pub(crate) fn locked_remaining(&mut self) -> Option<Duration> {
        let until = self.locked_until?;
        let now = Instant::now();
        if until <= now {
            self.locked_until = None;
            self.failed_attempts = 0;
            return None;
        }
        Some(until.duration_since(now))
    }

    pub(crate) fn record_success(&mut self) {
        self.failed_attempts = 0;
        self.locked_until = None;
    }

    pub(crate) fn record_failure(
        &mut self,
        max_attempts: u32,
        lockout: Duration,
    ) -> Option<Duration> {
        if max_attempts == 0 || lockout.is_zero() {
            return None;
        }
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        if self.failed_attempts < max_attempts {
            return None;
        }
        self.failed_attempts = 0;
        self.locked_until = Some(Instant::now() + lockout);
        Some(lockout)
    }
}

/// Application state accessible to all request handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub(crate) ledger: LedgerClients,
    pub(crate) stores: PersistentStores,
    pub(crate) intents: IntentStores,
    pub(crate) runtime: RuntimeState,
    pub(crate) caches: AppCaches,
    /// Path to the frontend dist directory (if bundled).
    pub web_dir: Option<PathBuf>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("network", &self.config.algod.network)
            .field("server_bind", &self.config.server.bind)
            .field("server_port", &self.config.server.port)
            .field("algod_url", &self.ledger.algod.base_url())
            .field(
                "read_algod_url",
                &self.ledger.read_algod.as_ref().map(AlgodClient::base_url),
            )
            .field("kmd_url", &self.ledger.kmd.base_url())
            .field("indexer_url", &self.config.indexer.url)
            .field(
                "wallet_history_enabled",
                &self.stores.wallet_history.is_some(),
            )
            .field("pin_set", &self.runtime.pin.try_lock().is_ok())
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Build app state from configuration.
    pub async fn from_config(config: Config) -> anyhow::Result<Self> {
        if config.indexer.history_retention_rounds < 1_000 {
            anyhow::bail!("indexer history retention must be at least 1000 rounds");
        }
        let pin_path = config.pin_path();
        let pin = PinStore::load(&pin_path).map_err(|e| anyhow::anyhow!("load pin store: {e}"))?;
        let algod_token = config.algod.effective_token()?;
        let algod = AlgodClient::new(&config.algod.url, &algod_token);
        let read_algod = match config
            .algod
            .read_url
            .as_ref()
            .filter(|url| !url.trim().is_empty())
        {
            Some(url) => {
                let read_token = config.algod.effective_read_token()?;
                tracing::info!(read_url = %url, "read-only algod client configured");
                Some(AlgodClient::new(url, &read_token))
            }
            None => None,
        };
        let kmd_token = config.kmd.effective_token();
        tracing::info!(kmd_url = %config.kmd.url, "kmd client configured");
        let kmd = KmdClient::new(&config.kmd.url, &kmd_token);
        let wallets = WalletManager::new(kmd.clone(), &config.data_dir)?;
        let prices = PriceCache::new(std::time::Duration::from_secs(60));
        let sync_tracker = SyncTracker::new(config.algod.network);
        let participation_tracker = crate::participation::ParticipationTracker::new(algod.clone());

        // Build the public relay client for fallback, if enabled and available.
        let public_algod = if config.algod.use_public_fallback {
            config.algod.network.public_api_url().map(|url| {
                tracing::info!(public_url = url, "public API fallback enabled");
                AlgodClient::new(url, "")
            })
        } else {
            None
        };

        // Build the indexer clients. The local indexer is always part of
        // the Docker stack; the public indexer relay serves as an automatic
        // fallback while the local indexer is bootstrapping or unreachable.
        tracing::info!(indexer_url = %config.indexer.url, "local indexer configured");
        let indexer = Some(IndexerClient::new(
            &config.indexer.url,
            &config.indexer.token,
        ));
        let public_indexer = if config.indexer.use_public_fallback {
            config.algod.network.public_indexer_url().map(|url| {
                tracing::info!(public_indexer_url = url, "public indexer fallback enabled");
                IndexerClient::new(url, "")
            })
        } else {
            None
        };
        let indexer_sync_tracker = IndexerSyncTracker::new(
            config.algod.network,
            indexer.is_some(),
            public_indexer.is_some(),
        );
        // If a local indexer is configured, poll its actual URL (not the default).
        if indexer.is_some() {
            indexer_sync_tracker.set_indexer_url(&config.indexer.url);
        } else if let Some(ref pub_idx) = public_indexer {
            indexer_sync_tracker.set_indexer_url(pub_idx.base_url());
        }

        let sessions = SessionStore::new(std::time::Duration::from_secs(
            config.server.session_ttl_secs,
        ));

        let wallet_history = if config.wallet_history.enabled {
            let database_url = config
                .wallet_history
                .effective_database_url()
                .map_err(|error| anyhow::anyhow!("resolve wallet history database: {error}"))?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "wallet history is enabled but no database credentials are configured"
                    )
                })?;
            let store = WalletHistoryStore::new(database_url, config.algod.network);
            store.initialize().await?;
            tracing::info!("wallet-only PostgreSQL transaction cache initialized");
            Some(store)
        } else {
            None
        };

        // Open the DEX orderbook database (best-effort: DEX endpoints return
        // 503 if this is None).
        let dex_path = config.data_dir.join("dex.sqlite");
        let dex = match opennodia_dex::DexDb::open(dex_path.to_str().unwrap_or("dex.sqlite")) {
            Ok(db) => {
                tracing::info!(path = %dex_path.display(), "DEX orderbook database opened");
                Some(Arc::new(db))
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to open DEX orderbook database; DEX disabled");
                None
            }
        };
        let local_indexer_state_complete = config.indexer.local_state_complete;
        let dex_validation = DexValidationRuntime::new(config.dex.write_enabled);
        let lp_registry_path = config.data_dir.join("lp-pools.json");
        let lp_registry = match crate::lp::LpRegistry::load(lp_registry_path.clone()) {
            Ok(registry) => registry,
            Err(error) => {
                tracing::warn!(
                    path = %lp_registry_path.display(),
                    %error,
                    "failed to load LP pool registry; starting with an empty registry"
                );
                crate::lp::LpRegistry::empty(lp_registry_path)
            }
        };
        let asa_issues_path = config.data_dir.join("asa-issues.sqlite");
        let asa_issues = match AsaIssueStore::open(&asa_issues_path) {
            Ok(store) => {
                tracing::info!(path = %asa_issues_path.display(), "ASA issuance history database opened");
                Some(Arc::new(store))
            }
            Err(error) => {
                tracing::warn!(
                    path = %asa_issues_path.display(),
                    %error,
                    "failed to open ASA issuance history database"
                );
                None
            }
        };
        let asset_metadata_path = config.data_dir.join("asset-metadata.sqlite");
        let asset_metadata = match AssetMetadataStore::open(&asset_metadata_path) {
            Ok(store) => {
                tracing::info!(path = %asset_metadata_path.display(), "asset metadata database opened");
                Some(Arc::new(store))
            }
            Err(error) => {
                tracing::warn!(
                    path = %asset_metadata_path.display(),
                    %error,
                    "failed to open asset metadata database"
                );
                None
            }
        };

        Ok(Self {
            config: Arc::new(config),
            ledger: LedgerClients {
                algod,
                read_algod,
                public_algod,
                indexer,
                public_indexer,
                local_indexer_state_complete,
                kmd,
            },
            stores: PersistentStores {
                wallet_history,
                wallets,
                dex,
                asa_issues,
                asset_metadata,
                lp_registry: Arc::new(Mutex::new(lp_registry)),
            },
            intents: IntentStores {
                dex: IntentStore::new(4_096),
                asset_create: IntentStore::new(4_096),
                lp: IntentStore::new(4_096),
                external_liquidity: IntentStore::new(4_096),
                router: IntentStore::new(4_096),
            },
            runtime: RuntimeState {
                sessions,
                pin: Arc::new(Mutex::new(pin)),
                pin_attempts: Arc::new(Mutex::new(PinAttemptTracker::default())),
                pin_path,
                prices,
                sync_tracker,
                indexer_sync_tracker,
                participation_tracker,
                dex_validation,
                dex_reconcile_lock: Arc::new(Mutex::new(())),
            },
            caches: AppCaches {
                account_info: Arc::new(Mutex::new(std::collections::HashMap::new())),
                asset_params: Arc::new(Mutex::new(std::collections::HashMap::new())),
            },
            web_dir: None,
        })
    }

    /// Whether initial setup (PIN creation) has been completed.
    pub async fn is_setup(&self) -> bool {
        self.runtime.pin.lock().await.is_some()
    }

    /// Whether the local algod node is actively catching up.
    ///
    /// Returns `true` if the local node reports `catchup_time > 0` or is
    /// unreachable. When the node is unreachable we cannot determine its
    /// sync status, so we conservatively treat it as syncing.
    pub async fn is_local_syncing(&self) -> bool {
        match self.ledger.algod.status().await {
            Ok(s) => !s.is_caught_up(),
            Err(_) => true,
        }
    }

    /// Select the algod client that write-path operations (transaction
    /// params, balance validation, submission, confirmation) should use.
    ///
    /// When the local node is fully synced, all writes go through the local
    /// node (Local-First principle). When the local node is still catching
    /// up, writes are routed through the public relay so that validation and
    /// submission use the same authoritative ledger view.
    ///
    /// Returns the effective client and the data source it represents. If the
    /// local node is syncing but no public relay is configured, returns an
    /// error message so the caller can surface a clear 503 instead of letting
    /// the user hit confusing `overspend` / stale-ledger errors.
    pub async fn effective_write_client(&self) -> Result<(&AlgodClient, DataSource), String> {
        self.authoritative_ledger()
            .await
            .map(|(client, _, source)| (client, source))
    }

    /// Select a client for read-only current ledger state.
    ///
    /// When configured, the read-only local algod is preferred if it is near
    /// the public network tip. This lets Docker deployments use the follower
    /// node for discovery and quote reads while keeping all writes on the
    /// participation/public authoritative path.
    pub async fn current_read_ledger(
        &self,
    ) -> Result<(&AlgodClient, NodeStatus, DataSource), String> {
        let Some(read_client) = self.ledger.read_algod.as_ref() else {
            return self.authoritative_ledger().await;
        };

        match read_client.status().await {
            Ok(read_status) => {
                if let Some(public_client) = self.ledger.public_algod.as_ref() {
                    match public_client.status().await {
                        Ok(public_status)
                            if local_read_is_current(&read_status, &public_status) =>
                        {
                            return Ok((read_client, read_status, DataSource::Local));
                        }
                        Ok(public_status) => {
                            tracing::debug!(
                                read_round = read_status.last_round.as_u64(),
                                public_round = public_status.last_round.as_u64(),
                                "read-only algod is behind public tip; falling back"
                            );
                        }
                        Err(error) if read_status.is_caught_up() => {
                            tracing::warn!(
                                %error,
                                "public algod status unavailable; using caught-up read-only algod"
                            );
                            return Ok((read_client, read_status, DataSource::Local));
                        }
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                "public algod status unavailable and read-only algod is not caught up"
                            );
                        }
                    }
                } else if read_status.is_caught_up() {
                    return Ok((read_client, read_status, DataSource::Local));
                }
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    "read-only algod status unavailable; falling back to authoritative ledger"
                );
            }
        }

        self.authoritative_ledger().await
    }

    /// Select a ledger client and round that represent the current network
    /// state. Local algod remains preferred when it is caught up and close to
    /// the public network tip.
    pub async fn authoritative_ledger(
        &self,
    ) -> Result<(&AlgodClient, NodeStatus, DataSource), String> {
        let local = self.ledger.algod.status().await;
        let public = match self.ledger.public_algod.as_ref() {
            Some(client) => Some((client, client.status().await)),
            None => None,
        };

        match (local, public) {
            (Ok(local_status), Some((public_client, Ok(public_status)))) => {
                let local_is_current = local_is_authoritative(&local_status, &public_status);
                if local_is_current {
                    Ok((&self.ledger.algod, local_status, DataSource::Local))
                } else {
                    tracing::debug!(
                        local_round = local_status.last_round.as_u64(),
                        public_round = public_status.last_round.as_u64(),
                        "using public algod as authoritative ledger source"
                    );
                    Ok((public_client, public_status, DataSource::Public))
                }
            }
            (Ok(local_status), Some((_, Err(public_error)))) => {
                if local_status.is_caught_up() {
                    tracing::warn!(
                        %public_error,
                        "public algod status unavailable; using caught-up local node"
                    );
                    Ok((&self.ledger.algod, local_status, DataSource::Local))
                } else {
                    Err(format!(
                        "local node is still syncing and public algod is unavailable: {public_error}"
                    ))
                }
            }
            (Ok(local_status), None) if local_status.is_caught_up() => {
                Ok((&self.ledger.algod, local_status, DataSource::Local))
            }
            (Ok(_), None) => Err(
                "Local node is still syncing and no public relay is configured. \
                 Please wait for synchronization to complete before using current-ledger operations."
                    .to_string(),
            ),
            (Err(local_error), Some((public_client, Ok(public_status)))) => {
                tracing::warn!(%local_error, "local algod unavailable; using public relay");
                Ok((public_client, public_status, DataSource::Public))
            }
            (Err(local_error), Some((_, Err(public_error)))) => Err(format!(
                "local algod unavailable ({local_error}); public algod unavailable ({public_error})"
            )),
            (Err(local_error), None) => Err(format!("local algod unavailable: {local_error}")),
        }
    }

    /// Select the indexer client that search/read operations should use.
    ///
    /// Priority: local indexer (if synced) → public indexer relay → local
    /// indexer (best-effort, even if still bootstrapping).
    ///
    /// The local indexer is always configured (it is part of the Docker
    /// stack). When it is still bootstrapping, we prefer the public relay so
    /// that search returns complete results. If no public relay is available,
    /// we fall back to the local indexer anyway (partial results are better
    /// than none).
    pub async fn effective_search_client(&self) -> Option<(&IndexerClient, DataSource)> {
        // Check if the local indexer is synced via the sync tracker.
        let local_synced = self.runtime.indexer_sync_tracker.is_synced().await;

        if self.ledger.local_indexer_state_complete && local_synced {
            if let Some(local) = self.ledger.indexer.as_ref() {
                return Some((local, DataSource::Local));
            }
        }

        // Local indexer not synced (or not configured) — try public relay.
        if let Some(public) = self.ledger.public_indexer.as_ref() {
            return Some((public, DataSource::Public));
        }

        // No public relay — use local indexer anyway (best-effort).
        self.ledger
            .indexer
            .as_ref()
            .map(|local| (local, DataSource::Local))
    }

    /// Whether any indexer (local or public) is available for search.
    pub fn has_indexer(&self) -> bool {
        self.ledger.indexer.is_some() || self.ledger.public_indexer.is_some()
    }

    /// Whether the local recent-history Indexer is close enough to the public
    /// network tip to serve current transaction queries.
    pub async fn local_history_ready(&self) -> bool {
        let Some(local) = self.ledger.indexer.as_ref() else {
            return false;
        };

        let Ok(local_health) = local.health().await else {
            return false;
        };
        if self.runtime.indexer_sync_tracker.is_synced().await {
            return true;
        }
        let Some(public) = self.ledger.public_indexer.as_ref() else {
            return true;
        };
        let Ok(public_health) = public.health().await else {
            return false;
        };
        local_health.round.saturating_add(20) >= public_health.round
    }

    /// Refresh the local recent page and advance the public historical
    /// backfill for one registered wallet address.
    pub async fn sync_wallet_history_address(&self, address: &str) -> anyhow::Result<()> {
        let Some(store) = self.stores.wallet_history.as_ref() else {
            return Ok(());
        };
        if !self
            .stores
            .wallets
            .contains_registered_address(address)
            .await
        {
            anyhow::bail!("address is not registered with OpenNodia");
        }

        let page_size = self.config.wallet_history.page_size.clamp(1, 1_000);
        let retention = self.config.indexer.history_retention_rounds.max(1);
        let pages_per_sync = self.config.wallet_history.pages_per_sync.max(1);
        let local = if self.local_history_ready().await {
            self.ledger.indexer.as_ref()
        } else {
            None
        };
        let local_health = if let Some(local) = local {
            match local.health().await {
                Ok(health) => Some(health),
                Err(error) => {
                    tracing::warn!(%address, %error, "local Indexer became unavailable");
                    None
                }
            }
        } else {
            None
        };
        let local = if local_health.is_some() { local } else { None };

        let public = self.ledger.public_indexer.as_ref();
        let (mut recent_client, mut recent_source, mut recent_tip) =
            if let (Some(local), Some(health)) = (local, local_health) {
                (local, HistorySource::Local, health.round)
            } else {
                let Some(public) = public else {
                    return Ok(());
                };
                let health = public.health().await?;
                (public, HistorySource::Public, health.round)
            };

        let network_tip = recent_tip;
        let public_max_round = network_tip.saturating_sub(retention);
        let mut backfill = store.backfill_state(address, public_max_round).await?;

        if backfill.local_next_token.is_some()
            && backfill.local_source.as_deref() == Some(HistorySource::Public.as_str())
            && recent_source == HistorySource::Local
        {
            if let (Some(public), Some(window_max)) = (public, backfill.local_window_max_round) {
                recent_client = public;
                recent_source = HistorySource::Public;
                recent_tip = window_max;
            }
        }

        {
            let retention_floor = recent_tip.saturating_sub(retention.saturating_sub(1));
            let continuation_matches_source =
                backfill.local_source.as_deref() == Some(recent_source.as_str());
            let (window_min, window_max, mut next_token) =
                if let (Some(min), Some(max), Some(token), true) = (
                    backfill.local_window_min_round,
                    backfill.local_window_max_round,
                    backfill.local_next_token.clone(),
                    continuation_matches_source,
                ) {
                    (min, max, Some(token))
                } else {
                    let next_unsynced = backfill.local_synced_round.saturating_add(1);
                    let window_min = if backfill.local_synced_round == 0
                        || recent_source == HistorySource::Local
                    {
                        next_unsynced.max(retention_floor)
                    } else {
                        next_unsynced
                    };
                    (window_min, recent_tip, None)
                };

            if window_min <= window_max {
                for _ in 0..pages_per_sync {
                    let page = recent_client
                        .account_transactions_page(
                            address,
                            page_size,
                            Some(window_min),
                            Some(window_max),
                            next_token.as_deref(),
                        )
                        .await?;
                    store
                        .upsert_transactions(address, recent_source, &page.transactions)
                        .await?;

                    let complete = page.next_token.is_none();
                    store
                        .save_local_sync_state(
                            address,
                            window_min,
                            window_max,
                            page.next_token.as_deref(),
                            recent_source,
                            complete,
                        )
                        .await?;
                    next_token = page.next_token;
                    if complete {
                        break;
                    }
                }
            }
        }

        let Some(public) = public else {
            return Ok(());
        };
        if backfill.complete {
            return Ok(());
        }
        if backfill.public_max_round == 0 {
            store.save_backfill_state(address, None, true).await?;
            return Ok(());
        }

        for _ in 0..pages_per_sync {
            let page = public
                .account_transactions_page(
                    address,
                    page_size,
                    None,
                    Some(backfill.public_max_round),
                    backfill.next_token.as_deref(),
                )
                .await?;
            store
                .upsert_transactions(address, HistorySource::Public, &page.transactions)
                .await?;

            let complete = page.next_token.is_none();
            store
                .save_backfill_state(address, page.next_token.as_deref(), complete)
                .await?;
            backfill.next_token = page.next_token;
            backfill.complete = complete;
            if complete {
                break;
            }
        }

        Ok(())
    }

    /// Synchronize all registered addresses without requiring wallet secrets.
    pub async fn sync_registered_wallet_histories(&self) {
        for address in self.stores.wallets.tracked_addresses().await {
            if let Err(error) = self.sync_wallet_history_address(&address).await {
                tracing::warn!(%address, %error, "wallet history synchronization failed");
            }
        }
    }

    /// Return recent-to-oldest history by combining local Indexer data,
    /// permanent wallet cache, and the public fallback.
    pub async fn account_transaction_history(
        &self,
        address: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<IndexerTransaction>> {
        let page = self
            .account_transaction_history_page(
                address,
                WalletHistoryQuery {
                    limit: limit.clamp(1, 100),
                    ..WalletHistoryQuery::default()
                },
            )
            .await?;
        Ok(page.transactions)
    }

    /// Return filtered wallet history with pagination.
    pub async fn account_transaction_history_page(
        &self,
        address: &str,
        mut query: WalletHistoryQuery,
    ) -> anyhow::Result<WalletTransactionPage> {
        query.limit = query.limit.clamp(1, 1_000);

        if let Some(store) = self.stores.wallet_history.as_ref() {
            if !self
                .stores
                .wallets
                .contains_registered_address(address)
                .await
            {
                return self
                    .uncached_account_transaction_history_page(address, &query)
                    .await;
            }

            if let Err(error) = self.sync_wallet_history_address(address).await {
                tracing::warn!(
                    %address,
                    %error,
                    "wallet history sync failed before filtered query; using existing cache"
                );
            }

            return store.list_transactions_filtered(address, &query).await;
        }

        let Some((indexer, _)) = self.effective_search_client().await else {
            anyhow::bail!("Indexer is not configured");
        };
        let fetch_limit = query.limit.saturating_add(query.offset).clamp(1, 1_000);
        let transactions = indexer.account_transactions(address, fetch_limit).await?;
        Ok(page_from_uncached(transactions, &query))
    }

    /// Return history for an address that is not registered with OpenNodia.
    ///
    /// Recent rounds come from the local bounded Indexer. Older rounds come
    /// from the public Indexer and are returned without entering the permanent
    /// wallet cache.
    async fn uncached_account_transaction_history(
        &self,
        address: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<IndexerTransaction>> {
        let mut transactions = Vec::with_capacity(limit as usize);
        let mut public_max_round = None;
        let mut local_covers_genesis = false;

        if self.local_history_ready().await {
            if let Some(local) = self.ledger.indexer.as_ref() {
                match local.health().await {
                    Ok(health) => {
                        let retention_floor = health.round.saturating_sub(
                            self.config
                                .indexer
                                .history_retention_rounds
                                .max(1)
                                .saturating_sub(1),
                        );
                        match local
                            .account_transactions_page(
                                address,
                                limit,
                                Some(retention_floor),
                                None,
                                None,
                            )
                            .await
                        {
                            Ok(page) => {
                                transactions = page.transactions;
                                public_max_round = retention_floor.checked_sub(1);
                                local_covers_genesis = retention_floor == 0;
                            }
                            Err(error) => {
                                tracing::warn!(
                                    %address,
                                    %error,
                                    "local uncached history query failed; using public Indexer"
                                );
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            %address,
                            %error,
                            "local Indexer health failed for uncached history"
                        );
                    }
                }
            }
        }

        if transactions.len() >= limit as usize {
            transactions.truncate(limit as usize);
            return Ok(transactions);
        }
        if local_covers_genesis {
            return Ok(transactions);
        }

        if let Some(public) = self.ledger.public_indexer.as_ref() {
            let remaining = limit.saturating_sub(transactions.len() as u32);
            if remaining > 0 {
                match public
                    .account_transactions_page(address, remaining, None, public_max_round, None)
                    .await
                {
                    Ok(page) => transactions.extend(page.transactions),
                    Err(error) if !transactions.is_empty() => {
                        tracing::warn!(
                            %address,
                            %error,
                            "public historical query failed; returning recent local history"
                        );
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            return Ok(transactions);
        }

        if transactions.is_empty() {
            anyhow::bail!("no Indexer is available for transaction history");
        }
        Ok(transactions)
    }

    async fn uncached_account_transaction_history_page(
        &self,
        address: &str,
        query: &WalletHistoryQuery,
    ) -> anyhow::Result<WalletTransactionPage> {
        let fetch_limit = query.limit.saturating_add(query.offset).clamp(1, 1_000);
        let transactions = self
            .uncached_account_transaction_history(address, fetch_limit)
            .await?;
        Ok(page_from_uncached(transactions, query))
    }

    /// Persist the current account asset list as the latest snapshot for the
    /// current UTC month.
    pub async fn record_balance_snapshot(
        &self,
        address: &str,
        source_round: u64,
        assets: &[BalanceSnapshotAsset],
    ) -> anyhow::Result<()> {
        let Some(store) = self.stores.wallet_history.as_ref() else {
            return Ok(());
        };
        if !self
            .stores
            .wallets
            .contains_registered_address(address)
            .await
        {
            return Ok(());
        }
        store
            .upsert_balance_snapshot(address, &current_snapshot_month(), source_round, assets)
            .await
    }

    /// Return persisted monthly balance snapshots for a registered address.
    pub async fn balance_snapshots(
        &self,
        address: &str,
        months: u32,
    ) -> anyhow::Result<Vec<BalanceSnapshotRecord>> {
        let Some(store) = self.stores.wallet_history.as_ref() else {
            anyhow::bail!("wallet history database is not configured");
        };
        if !self
            .stores
            .wallets
            .contains_registered_address(address)
            .await
        {
            anyhow::bail!("address is not registered with OpenNodia");
        }
        store.list_balance_snapshots(address, months).await
    }

    /// Persist the latest portfolio valuation for a registered address.
    pub async fn record_portfolio_value_snapshot(
        &self,
        address: &str,
        snapshot: PortfolioValueSnapshotInput,
    ) -> anyhow::Result<()> {
        let Some(store) = self.stores.wallet_history.as_ref() else {
            return Ok(());
        };
        if !self
            .stores
            .wallets
            .contains_registered_address(address)
            .await
        {
            return Ok(());
        }
        store.upsert_value_snapshot(address, &snapshot).await
    }

    /// Return persisted portfolio value snapshots for a registered address.
    pub async fn portfolio_value_snapshots(
        &self,
        address: &str,
        since_unix: u64,
        limit: u32,
    ) -> anyhow::Result<Vec<PortfolioValueSnapshotRecord>> {
        let Some(store) = self.stores.wallet_history.as_ref() else {
            anyhow::bail!("wallet history database is not configured");
        };
        if !self
            .stores
            .wallets
            .contains_registered_address(address)
            .await
        {
            anyhow::bail!("address is not registered with OpenNodia");
        }
        store.list_value_snapshots(address, since_unix, limit).await
    }
}

fn page_from_uncached(
    transactions: Vec<IndexerTransaction>,
    query: &WalletHistoryQuery,
) -> WalletTransactionPage {
    let filtered: Vec<_> = transactions
        .into_iter()
        .filter(|transaction| transaction_matches_query(transaction, query))
        .collect();
    let total = filtered.len() as u64;
    let offset = query.offset as usize;
    let limit = query.limit.clamp(1, 1_000) as usize;
    let transactions = filtered.into_iter().skip(offset).take(limit).collect();
    WalletTransactionPage {
        transactions,
        total,
        limit: query.limit.clamp(1, 1_000),
        offset: query.offset,
    }
}

fn transaction_matches_query(transaction: &IndexerTransaction, query: &WalletHistoryQuery) -> bool {
    if let Some(min_round) = query.min_round {
        if transaction.round < min_round {
            return false;
        }
    }
    if let Some(max_round) = query.max_round {
        if transaction.round > max_round {
            return false;
        }
    }
    if let Some(from_time) = query.from_time {
        if transaction.round_time < from_time {
            return false;
        }
    }
    if let Some(to_time) = query.to_time {
        if transaction.round_time > to_time {
            return false;
        }
    }
    if let Some(tx_type) = query.tx_type.as_deref() {
        if transaction.tx_type != tx_type {
            return false;
        }
    }
    if let Some(asset_id) = query.asset_id {
        if asset_id == 0 {
            return transaction.tx_type == "pay";
        }
        return transaction
            .asset_transfer
            .as_ref()
            .map(|transfer| transfer.asset_id == asset_id)
            .unwrap_or(false);
    }
    true
}

fn current_snapshot_month() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!("{:04}-{:02}", now.year(), u8::from(now.month()))
}

pub(crate) fn current_snapshot_hour() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:00:00Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour()
    )
}

fn local_is_authoritative(local: &NodeStatus, public: &NodeStatus) -> bool {
    local.is_caught_up()
        && local
            .last_round
            .as_u64()
            .saturating_add(AUTHORITATIVE_ROUND_LAG)
            >= public.last_round.as_u64()
}

fn local_read_is_current(local: &NodeStatus, public: &NodeStatus) -> bool {
    local
        .last_round
        .as_u64()
        .saturating_add(AUTHORITATIVE_ROUND_LAG)
        >= public.last_round.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(round: u64, catchup_time: u64) -> NodeStatus {
        NodeStatus {
            last_round: opennodia_core::Round(round),
            last_version: String::new(),
            time_since_last_round: 0,
            catchup_time,
        }
    }

    #[test]
    fn authoritative_selection_rejects_syncing_or_stale_local_node() {
        assert!(!local_is_authoritative(
            &status(1_000, 1),
            &status(1_000, 0)
        ));
        assert!(!local_is_authoritative(&status(970, 0), &status(1_000, 0)));
        assert!(local_is_authoritative(&status(980, 0), &status(1_000, 0)));
    }

    #[test]
    fn read_selection_allows_near_tip_follow_node() {
        assert!(local_read_is_current(
            &status(990, 5_000),
            &status(1_000, 0)
        ));
        assert!(!local_read_is_current(&status(970, 0), &status(1_000, 0)));
    }

    #[test]
    fn pin_attempt_tracker_locks_and_resets() {
        let mut tracker = PinAttemptTracker::default();
        assert!(tracker.record_failure(2, Duration::from_secs(60)).is_none());
        assert!(tracker.record_failure(2, Duration::from_secs(60)).is_some());
        assert!(tracker.locked_remaining().is_some());
        tracker.record_success();
        assert!(tracker.locked_remaining().is_none());
    }
}
