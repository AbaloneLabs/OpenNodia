//! Permanent PostgreSQL cache for registered wallet transaction history.
//!
//! The upstream Indexer tables are intentionally pruned. This module owns a
//! separate schema and only stores transactions involving addresses registered
//! in OpenNodia, so those records survive Indexer pruning.

use std::sync::Arc;

use opennodia_core::Network;
use opennodia_node::IndexerTransaction;
use serde::Serialize;
use tokio_postgres::{Client, NoTls};

const SCHEMA_SQL: &str = r#"
CREATE SCHEMA IF NOT EXISTS opennodia;

CREATE TABLE IF NOT EXISTS opennodia.wallet_transaction (
    network text NOT NULL,
    address text NOT NULL,
    transaction_key text NOT NULL,
    txid text NOT NULL,
    confirmed_round bigint NOT NULL,
    intra_round_offset bigint NOT NULL,
    round_time bigint NOT NULL,
    payload jsonb NOT NULL,
    source text NOT NULL,
    first_cached_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    last_cached_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (network, address, transaction_key)
);

CREATE INDEX IF NOT EXISTS wallet_transaction_address_round
    ON opennodia.wallet_transaction
        (network, address, confirmed_round DESC, intra_round_offset DESC);

CREATE TABLE IF NOT EXISTS opennodia.wallet_balance_snapshot (
    network text NOT NULL,
    address text NOT NULL,
    snapshot_month text NOT NULL,
    asset_id bigint NOT NULL,
    kind text NOT NULL,
    name text NOT NULL,
    unit text NOT NULL,
    decimals integer NOT NULL,
    amount numeric(78, 0) NOT NULL,
    source_round bigint NOT NULL,
    captured_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (network, address, snapshot_month, asset_id)
);

CREATE INDEX IF NOT EXISTS wallet_balance_snapshot_address_month
    ON opennodia.wallet_balance_snapshot
        (network, address, snapshot_month DESC, asset_id ASC);

CREATE TABLE IF NOT EXISTS opennodia.wallet_value_snapshot (
    network text NOT NULL,
    address text NOT NULL,
    snapshot_bucket text NOT NULL,
    source_round bigint NOT NULL,
    algo_amount numeric(78, 0) NOT NULL,
    algo_price_usd double precision NOT NULL,
    algo_value_usd double precision NOT NULL,
    asa_value_usd double precision NOT NULL,
    total_value_usd double precision NOT NULL,
    unpriced_asset_count integer NOT NULL,
    captured_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (network, address, snapshot_bucket)
);

CREATE INDEX IF NOT EXISTS wallet_value_snapshot_address_time
    ON opennodia.wallet_value_snapshot
        (network, address, captured_at DESC);

CREATE TABLE IF NOT EXISTS opennodia.wallet_history_sync (
    network text NOT NULL,
    address text NOT NULL,
    public_max_round bigint NOT NULL,
    next_token text,
    backfill_complete boolean NOT NULL DEFAULT false,
    local_synced_round bigint NOT NULL DEFAULT 0,
    local_window_min_round bigint,
    local_window_max_round bigint,
    local_next_token text,
    local_source text,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (network, address)
);

ALTER TABLE opennodia.wallet_history_sync
    ADD COLUMN IF NOT EXISTS local_synced_round bigint NOT NULL DEFAULT 0;
ALTER TABLE opennodia.wallet_history_sync
    ADD COLUMN IF NOT EXISTS local_window_min_round bigint;
ALTER TABLE opennodia.wallet_history_sync
    ADD COLUMN IF NOT EXISTS local_window_max_round bigint;
ALTER TABLE opennodia.wallet_history_sync
    ADD COLUMN IF NOT EXISTS local_next_token text;
ALTER TABLE opennodia.wallet_history_sync
    ADD COLUMN IF NOT EXISTS local_source text;
"#;

/// Source used to populate a cached transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistorySource {
    Local,
    Public,
}

impl HistorySource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Public => "public",
        }
    }
}

/// Persistent progress for the descending public-history backfill.
#[derive(Debug, Clone)]
pub struct BackfillState {
    pub public_max_round: u64,
    pub next_token: Option<String>,
    pub complete: bool,
    pub local_synced_round: u64,
    pub local_window_min_round: Option<u64>,
    pub local_window_max_round: Option<u64>,
    pub local_next_token: Option<String>,
    pub local_source: Option<String>,
}

/// Filter and pagination options for permanent wallet history reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalletHistoryQuery {
    pub limit: u32,
    pub offset: u32,
    pub min_round: Option<u64>,
    pub max_round: Option<u64>,
    pub from_time: Option<u64>,
    pub to_time: Option<u64>,
    pub tx_type: Option<String>,
    pub asset_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WalletTransactionPage {
    pub transactions: Vec<IndexerTransaction>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceSnapshotAsset {
    pub asset_id: u64,
    pub kind: String,
    pub name: String,
    pub unit: String,
    pub decimals: u32,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BalanceSnapshotRecord {
    pub snapshot_month: String,
    pub asset_id: u64,
    pub kind: String,
    pub name: String,
    pub unit: String,
    pub decimals: u32,
    pub amount: u64,
    pub source_round: u64,
    pub captured_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioValueSnapshotInput {
    pub snapshot_bucket: String,
    pub source_round: u64,
    pub algo_amount: u64,
    pub algo_price_usd: f64,
    pub algo_value_usd: f64,
    pub asa_value_usd: f64,
    pub total_value_usd: f64,
    pub unpriced_asset_count: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PortfolioValueSnapshotRecord {
    pub snapshot_bucket: String,
    pub source_round: u64,
    pub algo_amount: u64,
    pub algo_price_usd: f64,
    pub algo_value_usd: f64,
    pub asa_value_usd: f64,
    pub total_value_usd: f64,
    pub unpriced_asset_count: u32,
    pub captured_at: i64,
}

/// PostgreSQL wallet history store.
#[derive(Clone)]
pub struct WalletHistoryStore {
    database_url: Arc<str>,
    network: Arc<str>,
}

impl std::fmt::Debug for WalletHistoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalletHistoryStore")
            .field("network", &self.network)
            .finish_non_exhaustive()
    }
}

impl WalletHistoryStore {
    pub fn new(database_url: String, network: Network) -> Self {
        Self {
            database_url: Arc::from(database_url),
            network: Arc::from(network.to_string()),
        }
    }

    async fn connect(&self) -> anyhow::Result<Client> {
        let (client, connection) = tokio_postgres::connect(&self.database_url, NoTls)
            .await
            .map_err(|error| anyhow::anyhow!("connect wallet history database: {error}"))?;
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::warn!(%error, "wallet history database connection closed");
            }
        });
        Ok(client)
    }

    /// Create the OpenNodia-owned schema and indexes.
    pub async fn initialize(&self) -> anyhow::Result<()> {
        let client = self.connect().await?;
        client
            .batch_execute(SCHEMA_SQL)
            .await
            .map_err(|error| anyhow::anyhow!("initialize wallet history schema: {error}"))
    }

    /// Insert or refresh a page of transactions for one registered address.
    pub async fn upsert_transactions(
        &self,
        address: &str,
        source: HistorySource,
        transactions: &[IndexerTransaction],
    ) -> anyhow::Result<usize> {
        if transactions.is_empty() {
            return Ok(0);
        }

        let mut client = self.connect().await?;
        let db_tx = client
            .transaction()
            .await
            .map_err(|error| anyhow::anyhow!("start wallet history transaction: {error}"))?;
        let statement = db_tx
            .prepare(
                r#"
INSERT INTO opennodia.wallet_transaction (
    network,
    address,
    transaction_key,
    txid,
    confirmed_round,
    intra_round_offset,
    round_time,
    payload,
    source
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT (network, address, transaction_key) DO UPDATE SET
    txid = EXCLUDED.txid,
    confirmed_round = EXCLUDED.confirmed_round,
    intra_round_offset = EXCLUDED.intra_round_offset,
    round_time = EXCLUDED.round_time,
    payload = EXCLUDED.payload,
    source = CASE
        WHEN EXCLUDED.source = 'local' THEN EXCLUDED.source
        ELSE opennodia.wallet_transaction.source
    END,
    last_cached_at = clock_timestamp()
"#,
            )
            .await
            .map_err(|error| anyhow::anyhow!("prepare wallet history upsert: {error}"))?;

        for transaction in transactions {
            let confirmed_round = i64::try_from(transaction.round)
                .map_err(|_| anyhow::anyhow!("confirmed round exceeds PostgreSQL bigint"))?;
            let intra_round_offset = i64::try_from(transaction.intra_round_offset)
                .map_err(|_| anyhow::anyhow!("intra-round offset exceeds PostgreSQL bigint"))?;
            let round_time = i64::try_from(transaction.round_time)
                .map_err(|_| anyhow::anyhow!("round time exceeds PostgreSQL bigint"))?;
            let transaction_key = transaction_key(transaction);
            let payload = serde_json::to_value(transaction)?;

            db_tx
                .execute(
                    &statement,
                    &[
                        &self.network.as_ref(),
                        &address,
                        &transaction_key,
                        &transaction.id,
                        &confirmed_round,
                        &intra_round_offset,
                        &round_time,
                        &payload,
                        &source.as_str(),
                    ],
                )
                .await
                .map_err(|error| anyhow::anyhow!("upsert wallet transaction: {error}"))?;
        }

        db_tx
            .commit()
            .await
            .map_err(|error| anyhow::anyhow!("commit wallet history transaction: {error}"))?;
        Ok(transactions.len())
    }

    /// Return cached transactions in reverse chain order.
    pub async fn list_transactions(
        &self,
        address: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<IndexerTransaction>> {
        let client = self.connect().await?;
        let limit = i64::from(limit);
        let rows = client
            .query(
                r#"
SELECT payload
FROM opennodia.wallet_transaction
WHERE network = $1 AND address = $2
ORDER BY confirmed_round DESC, intra_round_offset DESC
LIMIT $3
"#,
                &[&self.network.as_ref(), &address, &limit],
            )
            .await
            .map_err(|error| anyhow::anyhow!("query wallet history: {error}"))?;

        rows.into_iter()
            .map(|row| {
                let payload: serde_json::Value = row.get(0);
                serde_json::from_value(payload)
                    .map_err(|error| anyhow::anyhow!("decode cached transaction: {error}"))
            })
            .collect()
    }

    /// Return cached transactions with filters and pagination.
    pub async fn list_transactions_filtered(
        &self,
        address: &str,
        query: &WalletHistoryQuery,
    ) -> anyhow::Result<WalletTransactionPage> {
        let client = self.connect().await?;
        let limit = i64::from(query.limit.clamp(1, 1_000));
        let offset = i64::from(query.offset);
        let min_round = optional_i64(query.min_round, "minimum round")?;
        let max_round = optional_i64(query.max_round, "maximum round")?;
        let from_time = optional_i64(query.from_time, "from timestamp")?;
        let to_time = optional_i64(query.to_time, "to timestamp")?;
        let asset_id = optional_i64(query.asset_id, "asset id")?;
        let tx_type = query.tx_type.as_deref();

        let count_row = client
            .query_one(
                r#"
SELECT count(*)
FROM opennodia.wallet_transaction
WHERE network = $1
  AND address = $2
  AND ($3::bigint IS NULL OR confirmed_round >= $3)
  AND ($4::bigint IS NULL OR confirmed_round <= $4)
  AND ($5::bigint IS NULL OR round_time >= $5)
  AND ($6::bigint IS NULL OR round_time <= $6)
  AND ($7::text IS NULL OR payload->>'tx-type' = $7)
  AND (
      $8::bigint IS NULL
      OR ($8 = 0 AND payload->>'tx-type' = 'pay')
      OR ((payload->'asset-transfer-transaction'->>'asset-id')::bigint = $8)
  )
"#,
                &[
                    &self.network.as_ref(),
                    &address,
                    &min_round,
                    &max_round,
                    &from_time,
                    &to_time,
                    &tx_type,
                    &asset_id,
                ],
            )
            .await
            .map_err(|error| anyhow::anyhow!("count wallet history: {error}"))?;
        let total = from_pg_round(count_row.get(0), "transaction count")?;

        let rows = client
            .query(
                r#"
SELECT payload
FROM opennodia.wallet_transaction
WHERE network = $1
  AND address = $2
  AND ($3::bigint IS NULL OR confirmed_round >= $3)
  AND ($4::bigint IS NULL OR confirmed_round <= $4)
  AND ($5::bigint IS NULL OR round_time >= $5)
  AND ($6::bigint IS NULL OR round_time <= $6)
  AND ($7::text IS NULL OR payload->>'tx-type' = $7)
  AND (
      $8::bigint IS NULL
      OR ($8 = 0 AND payload->>'tx-type' = 'pay')
      OR ((payload->'asset-transfer-transaction'->>'asset-id')::bigint = $8)
  )
ORDER BY confirmed_round DESC, intra_round_offset DESC
LIMIT $9 OFFSET $10
"#,
                &[
                    &self.network.as_ref(),
                    &address,
                    &min_round,
                    &max_round,
                    &from_time,
                    &to_time,
                    &tx_type,
                    &asset_id,
                    &limit,
                    &offset,
                ],
            )
            .await
            .map_err(|error| anyhow::anyhow!("query filtered wallet history: {error}"))?;

        let transactions = rows
            .into_iter()
            .map(|row| {
                let payload: serde_json::Value = row.get(0);
                serde_json::from_value(payload)
                    .map_err(|error| anyhow::anyhow!("decode cached transaction: {error}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(WalletTransactionPage {
            transactions,
            total,
            limit: query.limit.clamp(1, 1_000),
            offset: query.offset,
        })
    }

    /// Upsert the current month balance snapshot for one registered address.
    pub async fn upsert_balance_snapshot(
        &self,
        address: &str,
        snapshot_month: &str,
        source_round: u64,
        assets: &[BalanceSnapshotAsset],
    ) -> anyhow::Result<()> {
        let mut client = self.connect().await?;
        let db_tx = client
            .transaction()
            .await
            .map_err(|error| anyhow::anyhow!("start balance snapshot transaction: {error}"))?;
        let source_round = i64::try_from(source_round)
            .map_err(|_| anyhow::anyhow!("source round exceeds PostgreSQL bigint"))?;
        let statement = db_tx
            .prepare(
                r#"
INSERT INTO opennodia.wallet_balance_snapshot (
    network,
    address,
    snapshot_month,
    asset_id,
    kind,
    name,
    unit,
    decimals,
    amount,
    source_round
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::text::numeric, $10)
ON CONFLICT (network, address, snapshot_month, asset_id) DO UPDATE SET
    kind = EXCLUDED.kind,
    name = EXCLUDED.name,
    unit = EXCLUDED.unit,
    decimals = EXCLUDED.decimals,
    amount = EXCLUDED.amount,
    source_round = EXCLUDED.source_round,
    captured_at = clock_timestamp()
"#,
            )
            .await
            .map_err(|error| anyhow::anyhow!("prepare balance snapshot upsert: {error}"))?;

        for asset in assets {
            let asset_id = i64::try_from(asset.asset_id)
                .map_err(|_| anyhow::anyhow!("asset id exceeds PostgreSQL bigint"))?;
            let decimals = i32::try_from(asset.decimals)
                .map_err(|_| anyhow::anyhow!("asset decimals exceeds PostgreSQL integer"))?;
            let amount = asset.amount.to_string();
            db_tx
                .execute(
                    &statement,
                    &[
                        &self.network.as_ref(),
                        &address,
                        &snapshot_month,
                        &asset_id,
                        &asset.kind,
                        &asset.name,
                        &asset.unit,
                        &decimals,
                        &amount,
                        &source_round,
                    ],
                )
                .await
                .map_err(|error| anyhow::anyhow!("upsert balance snapshot: {error}"))?;
        }

        db_tx
            .commit()
            .await
            .map_err(|error| anyhow::anyhow!("commit balance snapshot transaction: {error}"))
    }

    /// Return persisted balance snapshots newest first.
    pub async fn list_balance_snapshots(
        &self,
        address: &str,
        months: u32,
    ) -> anyhow::Result<Vec<BalanceSnapshotRecord>> {
        let client = self.connect().await?;
        let months = i64::from(months.clamp(1, 120));
        let rows = client
            .query(
                r#"
WITH recent_months AS (
    SELECT DISTINCT snapshot_month
    FROM opennodia.wallet_balance_snapshot
    WHERE network = $1 AND address = $2
    ORDER BY snapshot_month DESC
    LIMIT $3
)
SELECT snapshot_month, asset_id, kind, name, unit, decimals,
       amount::text, source_round, extract(epoch from captured_at)::bigint
FROM opennodia.wallet_balance_snapshot
WHERE network = $1
  AND address = $2
  AND snapshot_month IN (SELECT snapshot_month FROM recent_months)
ORDER BY snapshot_month DESC, asset_id ASC
"#,
                &[&self.network.as_ref(), &address, &months],
            )
            .await
            .map_err(|error| anyhow::anyhow!("query balance snapshots: {error}"))?;

        rows.into_iter()
            .map(|row| {
                let amount_text: String = row.get(6);
                Ok(BalanceSnapshotRecord {
                    snapshot_month: row.get(0),
                    asset_id: sqlite_u64(row.get(1), "asset id")?,
                    kind: row.get(2),
                    name: row.get(3),
                    unit: row.get(4),
                    decimals: sqlite_u32(row.get(5), "decimals")?,
                    amount: amount_text
                        .parse::<u64>()
                        .map_err(|error| anyhow::anyhow!("decode snapshot amount: {error}"))?,
                    source_round: from_pg_round(row.get(7), "source round")?,
                    captured_at: row.get(8),
                })
            })
            .collect()
    }

    /// Upsert a portfolio value snapshot for one registered wallet address.
    pub async fn upsert_value_snapshot(
        &self,
        address: &str,
        snapshot: &PortfolioValueSnapshotInput,
    ) -> anyhow::Result<()> {
        let client = self.connect().await?;
        let source_round = i64::try_from(snapshot.source_round)
            .map_err(|_| anyhow::anyhow!("source round exceeds PostgreSQL bigint"))?;
        let algo_amount = snapshot.algo_amount.to_string();
        let unpriced_asset_count = i32::try_from(snapshot.unpriced_asset_count)
            .map_err(|_| anyhow::anyhow!("unpriced asset count exceeds PostgreSQL integer"))?;
        client
            .execute(
                r#"
INSERT INTO opennodia.wallet_value_snapshot (
    network,
    address,
    snapshot_bucket,
    source_round,
    algo_amount,
    algo_price_usd,
    algo_value_usd,
    asa_value_usd,
    total_value_usd,
    unpriced_asset_count
)
VALUES ($1, $2, $3, $4, $5::text::numeric, $6, $7, $8, $9, $10)
ON CONFLICT (network, address, snapshot_bucket) DO UPDATE SET
    source_round = EXCLUDED.source_round,
    algo_amount = EXCLUDED.algo_amount,
    algo_price_usd = EXCLUDED.algo_price_usd,
    algo_value_usd = EXCLUDED.algo_value_usd,
    asa_value_usd = EXCLUDED.asa_value_usd,
    total_value_usd = EXCLUDED.total_value_usd,
    unpriced_asset_count = EXCLUDED.unpriced_asset_count,
    captured_at = clock_timestamp()
"#,
                &[
                    &self.network.as_ref(),
                    &address,
                    &snapshot.snapshot_bucket,
                    &source_round,
                    &algo_amount,
                    &snapshot.algo_price_usd,
                    &snapshot.algo_value_usd,
                    &snapshot.asa_value_usd,
                    &snapshot.total_value_usd,
                    &unpriced_asset_count,
                ],
            )
            .await
            .map_err(|error| anyhow::anyhow!("upsert portfolio value snapshot: {error}"))?;
        Ok(())
    }

    /// Return portfolio value snapshots since a Unix timestamp.
    pub async fn list_value_snapshots(
        &self,
        address: &str,
        since_unix: u64,
        limit: u32,
    ) -> anyhow::Result<Vec<PortfolioValueSnapshotRecord>> {
        let client = self.connect().await?;
        let address = address.to_string();
        let since_unix = i64::try_from(since_unix)
            .map_err(|_| anyhow::anyhow!("snapshot start timestamp exceeds PostgreSQL bigint"))?;
        let since_unix = since_unix as f64;
        let limit = i64::from(limit.clamp(1, 2_000));
        let rows = client
            .query(
                r#"
SELECT snapshot_bucket,
       source_round,
       algo_amount::text,
       algo_price_usd,
       algo_value_usd,
       asa_value_usd,
       total_value_usd,
       unpriced_asset_count,
       extract(epoch from captured_at)::bigint
FROM opennodia.wallet_value_snapshot
WHERE network = $1
  AND address = $2
  AND captured_at >= to_timestamp($3::double precision)
ORDER BY captured_at ASC
LIMIT $4
"#,
                &[&self.network.as_ref(), &address, &since_unix, &limit],
            )
            .await
            .map_err(|error| anyhow::anyhow!("query portfolio value snapshots: {error}"))?;

        rows.into_iter()
            .map(|row| {
                let algo_amount_text: String = row.get(2);
                Ok(PortfolioValueSnapshotRecord {
                    snapshot_bucket: row.get(0),
                    source_round: from_pg_round(row.get(1), "source round")?,
                    algo_amount: algo_amount_text.parse::<u64>().map_err(|error| {
                        anyhow::anyhow!("decode portfolio snapshot ALGO amount: {error}")
                    })?,
                    algo_price_usd: row.get(3),
                    algo_value_usd: row.get(4),
                    asa_value_usd: row.get(5),
                    total_value_usd: row.get(6),
                    unpriced_asset_count: sqlite_u32(row.get(7), "unpriced asset count")?,
                    captured_at: row.get(8),
                })
            })
            .collect()
    }

    /// Remove cached history and sync state after a wallet is explicitly
    /// removed from the OpenNodia registry.
    pub async fn delete_address(&self, address: &str) -> anyhow::Result<()> {
        let mut client = self.connect().await?;
        let db_tx = client
            .transaction()
            .await
            .map_err(|error| anyhow::anyhow!("start wallet history deletion: {error}"))?;
        db_tx
            .execute(
                "DELETE FROM opennodia.wallet_transaction WHERE network = $1 AND address = $2",
                &[&self.network.as_ref(), &address],
            )
            .await
            .map_err(|error| anyhow::anyhow!("delete wallet transactions: {error}"))?;
        db_tx
            .execute(
                "DELETE FROM opennodia.wallet_history_sync WHERE network = $1 AND address = $2",
                &[&self.network.as_ref(), &address],
            )
            .await
            .map_err(|error| anyhow::anyhow!("delete wallet history sync state: {error}"))?;
        db_tx
            .execute(
                "DELETE FROM opennodia.wallet_balance_snapshot WHERE network = $1 AND address = $2",
                &[&self.network.as_ref(), &address],
            )
            .await
            .map_err(|error| anyhow::anyhow!("delete wallet balance snapshots: {error}"))?;
        db_tx
            .execute(
                "DELETE FROM opennodia.wallet_value_snapshot WHERE network = $1 AND address = $2",
                &[&self.network.as_ref(), &address],
            )
            .await
            .map_err(|error| anyhow::anyhow!("delete wallet value snapshots: {error}"))?;
        db_tx
            .commit()
            .await
            .map_err(|error| anyhow::anyhow!("commit wallet history deletion: {error}"))
    }

    /// Load or initialize public backfill progress for an address.
    pub async fn backfill_state(
        &self,
        address: &str,
        public_max_round: u64,
    ) -> anyhow::Result<BackfillState> {
        let max_round = i64::try_from(public_max_round)
            .map_err(|_| anyhow::anyhow!("public max round exceeds PostgreSQL bigint"))?;
        let client = self.connect().await?;
        let row = client
            .query_one(
                r#"
INSERT INTO opennodia.wallet_history_sync (
    network,
    address,
    public_max_round
)
VALUES ($1, $2, $3)
ON CONFLICT (network, address) DO UPDATE SET
    updated_at = opennodia.wallet_history_sync.updated_at
RETURNING
    public_max_round,
    next_token,
    backfill_complete,
    local_synced_round,
    local_window_min_round,
    local_window_max_round,
    local_next_token,
    local_source
"#,
                &[&self.network.as_ref(), &address, &max_round],
            )
            .await
            .map_err(|error| anyhow::anyhow!("load wallet backfill state: {error}"))?;

        let stored_max_round: i64 = row.get(0);
        Ok(BackfillState {
            public_max_round: u64::try_from(stored_max_round)
                .map_err(|_| anyhow::anyhow!("stored public max round is negative"))?,
            next_token: row.get(1),
            complete: row.get(2),
            local_synced_round: from_pg_round(row.get(3), "local synced round")?,
            local_window_min_round: optional_pg_round(row.get(4), "local window minimum round")?,
            local_window_max_round: optional_pg_round(row.get(5), "local window maximum round")?,
            local_next_token: row.get(6),
            local_source: row.get(7),
        })
    }

    /// Persist local recent-history pagination and its completed watermark.
    pub async fn save_local_sync_state(
        &self,
        address: &str,
        window_min_round: u64,
        window_max_round: u64,
        next_token: Option<&str>,
        source: HistorySource,
        complete: bool,
    ) -> anyhow::Result<()> {
        let min_round = i64::try_from(window_min_round)
            .map_err(|_| anyhow::anyhow!("local minimum round exceeds PostgreSQL bigint"))?;
        let max_round = i64::try_from(window_max_round)
            .map_err(|_| anyhow::anyhow!("local maximum round exceeds PostgreSQL bigint"))?;
        let client = self.connect().await?;

        if complete {
            client
                .execute(
                    r#"
UPDATE opennodia.wallet_history_sync
SET local_synced_round = GREATEST(local_synced_round, $3),
    local_window_min_round = NULL,
    local_window_max_round = NULL,
    local_next_token = NULL,
    local_source = NULL,
    updated_at = clock_timestamp()
WHERE network = $1 AND address = $2
"#,
                    &[&self.network.as_ref(), &address, &max_round],
                )
                .await
                .map_err(|error| anyhow::anyhow!("complete local wallet sync: {error}"))?;
        } else {
            client
                .execute(
                    r#"
UPDATE opennodia.wallet_history_sync
SET local_window_min_round = $3,
    local_window_max_round = $4,
    local_next_token = $5,
    local_source = $6,
    updated_at = clock_timestamp()
WHERE network = $1 AND address = $2
"#,
                    &[
                        &self.network.as_ref(),
                        &address,
                        &min_round,
                        &max_round,
                        &next_token,
                        &source.as_str(),
                    ],
                )
                .await
                .map_err(|error| anyhow::anyhow!("save local wallet sync state: {error}"))?;
        }
        Ok(())
    }

    /// Persist the continuation token for the next background sync cycle.
    pub async fn save_backfill_state(
        &self,
        address: &str,
        next_token: Option<&str>,
        complete: bool,
    ) -> anyhow::Result<()> {
        let client = self.connect().await?;
        client
            .execute(
                r#"
UPDATE opennodia.wallet_history_sync
SET next_token = $3,
    backfill_complete = $4,
    updated_at = clock_timestamp()
WHERE network = $1 AND address = $2
"#,
                &[&self.network.as_ref(), &address, &next_token, &complete],
            )
            .await
            .map_err(|error| anyhow::anyhow!("save wallet backfill state: {error}"))?;
        Ok(())
    }
}

fn transaction_key(transaction: &IndexerTransaction) -> String {
    if transaction.id.is_empty() {
        format!(
            "{}:{}:{}",
            transaction.round, transaction.intra_round_offset, transaction.tx_type
        )
    } else {
        transaction.id.clone()
    }
}

fn from_pg_round(value: i64, label: &str) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow::anyhow!("stored {label} is negative"))
}

fn optional_pg_round(value: Option<i64>, label: &str) -> anyhow::Result<Option<u64>> {
    value.map(|round| from_pg_round(round, label)).transpose()
}

fn optional_i64(value: Option<u64>, label: &str) -> anyhow::Result<Option<i64>> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| anyhow::anyhow!("{label} exceeds PostgreSQL bigint"))
        })
        .transpose()
}

fn sqlite_u64(value: i64, label: &str) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow::anyhow!("stored {label} is negative"))
}

fn sqlite_u32(value: i32, label: &str) -> anyhow::Result<u32> {
    u32::try_from(value).map_err(|_| anyhow::anyhow!("stored {label} is negative"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_id_is_preferred_as_cache_key() {
        let transaction = IndexerTransaction {
            id: "TXID".into(),
            round: 10,
            intra_round_offset: 2,
            tx_type: "pay".into(),
            ..serde_json::from_value(serde_json::json!({})).unwrap()
        };
        assert_eq!(transaction_key(&transaction), "TXID");
    }

    #[test]
    fn optional_i64_rejects_big_values() {
        assert_eq!(optional_i64(Some(42), "value").unwrap(), Some(42));
        assert!(optional_i64(Some(i64::MAX as u64 + 1), "value").is_err());
    }
}
