//! SQLite schema and order/trade persistence.
//!
//! Uses WAL mode for concurrent read/write safety. Schema auto-creates on first
//! connection.

use std::sync::Mutex;

use opennodia_core::{Address, Round};
use opennodia_swap::{
    escrow_address, validate_params, EscrowKind, EscrowParams, OrderSide,
    MAX_LOGICSIG_PROGRAM_BYTES,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::types::{invert_price, CommunityMarket, EntryStatus, OrderEntry, Pair, PairStat, Trade};

/// Schema version for migrations.
pub const SCHEMA_VERSION: u32 = 5;

/// A thread-safe wrapper around a SQLite connection.
///
/// Uses a `Mutex` since `rusqlite::Connection` is not `Sync`. The server uses
/// a single writer thread; readers acquire the lock briefly.
pub struct DexDb {
    conn: Mutex<Connection>,
}

impl DexDb {
    /// Open (or create) the database at `path`. Use `:memory:` for tests.
    pub fn open(path: &str) -> opennodia_core::Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| opennodia_core::Error::Other(format!("dex db open: {e}")))?;
        Self::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database (for tests).
    pub fn open_memory() -> opennodia_core::Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| opennodia_core::Error::Other(format!("dex db memory: {e}")))?;
        Self::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Initialize schema and pragmas.
    fn init(conn: &Connection) -> opennodia_core::Result<()> {
        // WAL mode for concurrent readers + single writer.
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| opennodia_core::Error::Other(format!("pragma WAL: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| opennodia_core::Error::Other(format!("pragma FK: {e}")))?;

        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| opennodia_core::Error::Other(format!("dex db schema: {e}")))?;
        migrate_schema(conn)?;
        Ok(())
    }

    /// Register a new order after its deposit confirms.
    pub fn register_order(&self, entry: &OrderEntry) -> opennodia_core::Result<()> {
        validate_order_entry(entry)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            INSERT_ORDER_SQL,
            params![
                hex::encode(entry.escrow_addr.as_bytes()),
                entry.side.as_str(),
                sqlite_int(entry.sell_asset, "sell asset")?,
                sqlite_int(entry.sell_amount, "sell amount")?,
                sqlite_int(entry.buy_asset, "buy asset")?,
                sqlite_int(entry.buy_amount, "buy amount")?,
                sqlite_int(entry.price, "price")?,
                hex::encode(entry.owner.as_bytes()),
                sqlite_int(entry.created_round.as_u64(), "created round")?,
                sqlite_int(entry.expire_round.as_u64(), "expire round")?,
                entry.status.as_str(),
                sqlite_int(entry.filled_amount, "filled amount")?,
                i64::from(entry.split_index),
                entry.parent_id,
                hex::encode(&entry.program),
                serde_json::to_string(&entry.params)
                    .map_err(|e| opennodia_core::Error::Other(format!("params serialize: {e}")))?,
            ],
        )
        .map_err(|e| opennodia_core::Error::Other(format!("register_order: {e}")))?;
        Ok(())
    }

    /// Create or update an operator-authenticated community market.
    pub fn upsert_community_market(&self, market: &CommunityMarket) -> opennodia_core::Result<()> {
        validate_community_market(market)?;
        let conn = self.conn.lock().unwrap();
        let existing_updated_at = conn
            .query_row(
                "SELECT updated_at FROM community_markets WHERE id = ?1",
                params![&market.id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| {
                opennodia_core::Error::Other(format!("read market updated_at: {error}"))
            })?;
        if let Some(existing_updated_at) = existing_updated_at {
            let existing_updated_at = u64::try_from(existing_updated_at).map_err(|_| {
                opennodia_core::Error::Other(
                    "stored market updated_at must be non-negative".to_string(),
                )
            })?;
            if market.updated_at < existing_updated_at {
                return Err(opennodia_core::Error::Other(format!(
                    "stale community market update: {} is older than existing {}",
                    market.updated_at, existing_updated_at
                )));
            }
        }
        conn.execute(
            "INSERT INTO community_markets
                (id, operator, name, description, logo_url, asset_ids, pairs,
                 migration_notice, announcement_channel, signature, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                operator = excluded.operator,
                name = excluded.name,
                description = excluded.description,
                logo_url = excluded.logo_url,
                asset_ids = excluded.asset_ids,
                pairs = excluded.pairs,
                migration_notice = excluded.migration_notice,
                announcement_channel = excluded.announcement_channel,
                signature = excluded.signature,
                updated_at = excluded.updated_at",
            params![
                &market.id,
                hex::encode(market.operator.as_bytes()),
                &market.name,
                &market.description,
                &market.logo_url,
                serde_json::to_string(&market.asset_ids).map_err(|error| {
                    opennodia_core::Error::Other(format!("market asset serialize: {error}"))
                })?,
                serde_json::to_string(&market.pairs).map_err(|error| {
                    opennodia_core::Error::Other(format!("market pair serialize: {error}"))
                })?,
                &market.migration_notice,
                &market.announcement_channel,
                &market.signature,
                sqlite_int(market.updated_at, "market updated_at")?,
            ],
        )
        .map_err(|error| opennodia_core::Error::Other(format!("upsert market: {error}")))?;
        Ok(())
    }

    /// Load one community market by id.
    pub fn get_community_market(
        &self,
        id: &str,
    ) -> opennodia_core::Result<Option<CommunityMarket>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT * FROM community_markets WHERE id = ?1",
            params![id],
            row_to_community_market,
        )
        .optional()
        .map_err(|error| opennodia_core::Error::Other(format!("get market: {error}")))
    }

    /// List community markets, optionally filtered by operator or asset id.
    pub fn list_community_markets(
        &self,
        operator: Option<&Address>,
        asset_id: Option<u64>,
        limit: u32,
    ) -> opennodia_core::Result<Vec<CommunityMarket>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT * FROM community_markets ORDER BY updated_at DESC, id ASC")
            .map_err(|error| {
                opennodia_core::Error::Other(format!("list markets prepare: {error}"))
            })?;
        let rows = stmt
            .query_map([], row_to_community_market)
            .map_err(|error| opennodia_core::Error::Other(format!("list markets: {error}")))?;
        let operator_hex = operator.map(|addr| hex::encode(addr.as_bytes()));
        let mut out = Vec::new();
        for row in rows {
            let market =
                row.map_err(|error| opennodia_core::Error::Other(format!("market row: {error}")))?;
            if let Some(expected) = operator_hex.as_deref() {
                if hex::encode(market.operator.as_bytes()) != expected {
                    continue;
                }
            }
            if let Some(asset_id) = asset_id {
                if !market.asset_ids.contains(&asset_id)
                    && !market.pairs.iter().any(|pair| pair.contains(asset_id))
                {
                    continue;
                }
            }
            out.push(market);
            if out.len() >= limit as usize {
                break;
            }
        }
        Ok(out)
    }

    /// Update an order's status.
    pub fn update_order_status(
        &self,
        escrow_addr: &Address,
        status: EntryStatus,
    ) -> opennodia_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE orders SET status = ?1 WHERE escrow_addr = ?2",
            params![status.as_str(), hex::encode(escrow_addr.as_bytes())],
        )
        .map_err(|e| opennodia_core::Error::Other(format!("update_order_status: {e}")))?;
        Ok(())
    }

    /// Update exactly one order, failing when the escrow is not registered.
    pub fn update_order_status_checked(
        &self,
        escrow_addr: &Address,
        status: EntryStatus,
    ) -> opennodia_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn
            .execute(
                "UPDATE orders SET status = ?1 WHERE escrow_addr = ?2",
                params![status.as_str(), hex::encode(escrow_addr.as_bytes())],
            )
            .map_err(|e| {
                opennodia_core::Error::Other(format!("update_order_status_checked: {e}"))
            })?;
        if changed != 1 {
            return Err(opennodia_core::Error::Other(format!(
                "expected one order update, changed {changed}"
            )));
        }
        Ok(())
    }

    /// Mark an escrow as closed when the ledger proves closure but the
    /// corresponding fill or cancellation group is not yet available.
    pub fn mark_closed_unresolved(
        &self,
        escrow_addr: &Address,
        round: Round,
    ) -> opennodia_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn
            .execute(
                "UPDATE orders \
                 SET status = 'closed_unresolved', resolution_round = ?1 \
                 WHERE escrow_addr = ?2 \
                   AND (status IN ('active', 'expired', 'closed_unresolved') \
                        OR (status IN ('filled', 'cancelled') \
                            AND resolution_tx_id IS NULL))",
                params![
                    sqlite_int(round.as_u64(), "resolution round")?,
                    hex::encode(escrow_addr.as_bytes())
                ],
            )
            .map_err(|e| opennodia_core::Error::Other(format!("mark_closed_unresolved: {e}")))?;
        if changed != 1 {
            return Err(opennodia_core::Error::Other(format!(
                "expected one reconcilable order update, changed {changed}"
            )));
        }
        Ok(())
    }

    /// Update the filled amount for an order.
    pub fn update_filled_amount(
        &self,
        escrow_addr: &Address,
        filled_amount: u64,
    ) -> opennodia_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE orders SET filled_amount = ?1 WHERE escrow_addr = ?2",
            params![filled_amount as i64, hex::encode(escrow_addr.as_bytes())],
        )
        .map_err(|e| opennodia_core::Error::Other(format!("update_filled_amount: {e}")))?;
        Ok(())
    }

    /// Mark all orders past their expiry round as expired (still active).
    pub fn mark_expired(&self, current_round: Round) -> opennodia_core::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count = conn
            .execute(
                "UPDATE orders SET status = 'expired' \
                 WHERE status = 'active' AND expire_round < ?1",
                params![current_round.as_u64() as i64],
            )
            .map_err(|e| opennodia_core::Error::Other(format!("mark_expired: {e}")))?;
        Ok(count as u64)
    }

    /// Get a single order by escrow address.
    pub fn get_order(&self, escrow_addr: &Address) -> opennodia_core::Result<Option<OrderEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(SELECT_ORDER_SQL)
            .map_err(|e| opennodia_core::Error::Other(format!("get_order prepare: {e}")))?;
        let entry = stmt
            .query_row(
                params![hex::encode(escrow_addr.as_bytes())],
                row_to_order_entry,
            )
            .optional()
            .map_err(|e| opennodia_core::Error::Other(format!("get_order: {e}")))?;
        Ok(entry)
    }

    /// Get all active orders for a trading pair.
    pub fn get_active_orders_for_pair(
        &self,
        pair: Pair,
    ) -> opennodia_core::Result<Vec<OrderEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM orders \
                 WHERE status = 'active' \
                 AND ((sell_asset = ?1 AND buy_asset = ?2) \
                      OR (sell_asset = ?2 AND buy_asset = ?1)) \
                 ORDER BY price ASC",
            )
            .map_err(|e| opennodia_core::Error::Other(format!("pair orders prepare: {e}")))?;
        let rows = stmt
            .query_map(
                params![pair.asset_a as i64, pair.asset_b as i64],
                row_to_order_entry,
            )
            .map_err(|e| opennodia_core::Error::Other(format!("pair orders: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?);
        }
        Ok(out)
    }

    /// Get all orders for a given owner (any status).
    pub fn get_orders_for_owner(
        &self,
        owner: &Address,
        status: Option<EntryStatus>,
    ) -> opennodia_core::Result<Vec<OrderEntry>> {
        let conn = self.conn.lock().unwrap();
        let owner_hex = hex::encode(owner.as_bytes());
        let rows = if let Some(s) = status {
            let mut stmt = conn
                .prepare("SELECT * FROM orders WHERE owner = ?1 AND status = ?2 ORDER BY created_round DESC")
                .map_err(|e| opennodia_core::Error::Other(format!("owner orders prepare: {e}")))?;
            let mapped = stmt
                .query_map(params![owner_hex, s.as_str()], row_to_order_entry)
                .map_err(|e| opennodia_core::Error::Other(format!("owner orders: {e}")))?;
            let mut out = Vec::new();
            for r in mapped {
                out.push(r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?);
            }
            out
        } else {
            let mut stmt = conn
                .prepare("SELECT * FROM orders WHERE owner = ?1 ORDER BY created_round DESC")
                .map_err(|e| opennodia_core::Error::Other(format!("owner orders prepare: {e}")))?;
            let mapped = stmt
                .query_map(params![owner_hex], row_to_order_entry)
                .map_err(|e| opennodia_core::Error::Other(format!("owner orders: {e}")))?;
            let mut out = Vec::new();
            for r in mapped {
                out.push(r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?);
            }
            out
        };
        Ok(rows)
    }

    /// Get orders whose on-chain state may still need reconciliation.
    pub fn get_reconcilable_orders(&self) -> opennodia_core::Result<Vec<OrderEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM orders \
                 WHERE status IN ('active', 'expired', 'closed_unresolved') \
                    OR (status IN ('filled', 'cancelled') \
                        AND resolution_tx_id IS NULL) \
                 ORDER BY created_round ASC",
            )
            .map_err(|e| {
                opennodia_core::Error::Other(format!("reconcilable orders prepare: {e}"))
            })?;
        let rows = stmt
            .query_map([], row_to_order_entry)
            .map_err(|e| opennodia_core::Error::Other(format!("reconcilable orders: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?);
        }
        Ok(out)
    }

    /// Record a trade.
    pub fn record_trade(&self, trade: &Trade) -> opennodia_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        let base_asset = trade
            .base_asset
            .map(|asset| sqlite_int(asset, "trade base asset"))
            .transpose()?;
        conn.execute(
            INSERT_TRADE_SQL,
            params![
                trade.tx_id,
                sqlite_int(trade.pair.asset_a, "pair asset A")?,
                sqlite_int(trade.pair.asset_b, "pair asset B")?,
                trade.side.as_str(),
                sqlite_int(trade.price, "trade price")?,
                base_asset,
                sqlite_int(trade.amount, "trade amount")?,
                hex::encode(trade.buyer.as_bytes()),
                hex::encode(trade.seller.as_bytes()),
                sqlite_int(trade.round.as_u64(), "trade round")?,
                sqlite_int(trade.timestamp, "trade timestamp")?,
                Option::<String>::None,
            ],
        )
        .map_err(|e| opennodia_core::Error::Other(format!("record_trade: {e}")))?;
        Ok(())
    }

    /// Atomically mark an order filled and insert its confirmed trade.
    pub fn record_fill(
        &self,
        escrow_addr: &Address,
        filled_amount: u64,
        trade: &Trade,
    ) -> opennodia_core::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction()
            .map_err(|e| opennodia_core::Error::Other(format!("record_fill begin: {e}")))?;
        let escrow_hex = hex::encode(escrow_addr.as_bytes());
        let current: Option<(String, Option<String>)> = transaction
            .query_row(
                "SELECT status, resolution_tx_id FROM orders WHERE escrow_addr = ?1",
                params![&escrow_hex],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| opennodia_core::Error::Other(format!("record_fill lookup: {e}")))?;
        if let Some((status, resolution_tx_id)) = current.as_ref() {
            if status == "filled"
                && resolution_tx_id.is_some()
                && resolution_tx_id.as_deref() != Some(trade.tx_id.as_str())
            {
                return Err(opennodia_core::Error::Other(
                    "order is already filled by a different transaction".to_string(),
                ));
            }
        }
        if current.as_ref().is_some_and(|(status, resolution_tx_id)| {
            status == "filled" && resolution_tx_id.as_deref() == Some(trade.tx_id.as_str())
        }) {
            let existing: Option<String> = transaction
                .query_row(
                    "SELECT tx_id FROM trades WHERE tx_id = ?1",
                    params![&trade.tx_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| {
                    opennodia_core::Error::Other(format!("record_fill existing trade: {e}"))
                })?;
            if existing.is_some() {
                return Ok(());
            }
        }
        let changed = transaction
            .execute(
                "UPDATE orders \
                 SET status = 'filled', filled_amount = ?1, \
                     resolution_tx_id = ?2, resolution_round = ?3 \
                 WHERE escrow_addr = ?4 \
                   AND (status IN ('active', 'expired', 'closed_unresolved') \
                        OR (status = 'filled' \
                            AND (resolution_tx_id IS NULL OR resolution_tx_id = ?2)))",
                params![
                    i64::try_from(filled_amount).map_err(|_| {
                        opennodia_core::Error::Other(
                            "filled amount exceeds SQLite integer range".to_string(),
                        )
                    })?,
                    &trade.tx_id,
                    sqlite_int(trade.round.as_u64(), "resolution round")?,
                    &escrow_hex
                ],
            )
            .map_err(|e| opennodia_core::Error::Other(format!("record_fill update: {e}")))?;
        if changed != 1 {
            return Err(opennodia_core::Error::Other(format!(
                "expected one active order during fill, changed {changed}"
            )));
        }
        let base_asset = trade
            .base_asset
            .map(|asset| sqlite_int(asset, "trade base asset"))
            .transpose()?;
        transaction
            .execute(
                INSERT_TRADE_SQL,
                params![
                    trade.tx_id,
                    i64::try_from(trade.pair.asset_a).map_err(|_| {
                        opennodia_core::Error::Other(
                            "trade pair asset exceeds SQLite integer range".to_string(),
                        )
                    })?,
                    i64::try_from(trade.pair.asset_b).map_err(|_| {
                        opennodia_core::Error::Other(
                            "trade pair asset exceeds SQLite integer range".to_string(),
                        )
                    })?,
                    trade.side.as_str(),
                    i64::try_from(trade.price).map_err(|_| {
                        opennodia_core::Error::Other(
                            "trade price exceeds SQLite integer range".to_string(),
                        )
                    })?,
                    base_asset,
                    i64::try_from(trade.amount).map_err(|_| {
                        opennodia_core::Error::Other(
                            "trade amount exceeds SQLite integer range".to_string(),
                        )
                    })?,
                    hex::encode(trade.buyer.as_bytes()),
                    hex::encode(trade.seller.as_bytes()),
                    i64::try_from(trade.round.as_u64()).map_err(|_| {
                        opennodia_core::Error::Other(
                            "trade round exceeds SQLite integer range".to_string(),
                        )
                    })?,
                    i64::try_from(trade.timestamp).map_err(|_| {
                        opennodia_core::Error::Other(
                            "trade timestamp exceeds SQLite integer range".to_string(),
                        )
                    })?,
                    &escrow_hex,
                ],
            )
            .map_err(|e| opennodia_core::Error::Other(format!("record_fill trade: {e}")))?;
        transaction
            .commit()
            .map_err(|e| opennodia_core::Error::Other(format!("record_fill commit: {e}")))
    }

    /// Atomically mark a confirmed cancellation and retain its evidence.
    pub fn record_cancel(
        &self,
        escrow_addr: &Address,
        tx_id: &str,
        round: Round,
    ) -> opennodia_core::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn
            .transaction()
            .map_err(|e| opennodia_core::Error::Other(format!("record_cancel begin: {e}")))?;
        let escrow_hex = hex::encode(escrow_addr.as_bytes());
        let current: Option<(String, Option<String>)> = transaction
            .query_row(
                "SELECT status, resolution_tx_id FROM orders WHERE escrow_addr = ?1",
                params![&escrow_hex],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| opennodia_core::Error::Other(format!("record_cancel lookup: {e}")))?;
        if let Some((status, existing_tx_id)) = current.as_ref() {
            if status == "cancelled" && existing_tx_id.as_deref() == Some(tx_id) {
                return Ok(());
            }
        }
        let changed = transaction
            .execute(
                "UPDATE orders \
                 SET status = 'cancelled', resolution_tx_id = ?1, resolution_round = ?2 \
                 WHERE escrow_addr = ?3 \
                   AND (status IN ('active', 'expired', 'closed_unresolved') \
                        OR (status IN ('filled', 'cancelled') \
                            AND resolution_tx_id IS NULL))",
                params![
                    tx_id,
                    sqlite_int(round.as_u64(), "resolution round")?,
                    escrow_hex
                ],
            )
            .map_err(|e| opennodia_core::Error::Other(format!("record_cancel update: {e}")))?;
        if changed != 1 {
            return Err(opennodia_core::Error::Other(format!(
                "expected one reconcilable order during cancellation, changed {changed}"
            )));
        }
        transaction
            .commit()
            .map_err(|e| opennodia_core::Error::Other(format!("record_cancel commit: {e}")))
    }

    /// Get recent trades for a pair.
    pub fn get_recent_trades(&self, pair: Pair, limit: u32) -> opennodia_core::Result<Vec<Trade>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM trades \
                 WHERE pair_a = ?1 AND pair_b = ?2 \
                 ORDER BY round DESC LIMIT ?3",
            )
            .map_err(|e| opennodia_core::Error::Other(format!("trades prepare: {e}")))?;
        let rows = stmt
            .query_map(
                params![pair.asset_a as i64, pair.asset_b as i64, limit as i64],
                row_to_trade,
            )
            .map_err(|e| opennodia_core::Error::Other(format!("trades: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?);
        }
        Ok(out)
    }

    /// Get trades for a specific account (as buyer or seller).
    pub fn get_trades_for_account(
        &self,
        addr: &Address,
        limit: u32,
    ) -> opennodia_core::Result<Vec<Trade>> {
        let conn = self.conn.lock().unwrap();
        let addr_hex = hex::encode(addr.as_bytes());
        let mut stmt = conn
            .prepare(
                "SELECT * FROM trades WHERE buyer = ?1 OR seller = ?1 ORDER BY round DESC LIMIT ?2",
            )
            .map_err(|e| opennodia_core::Error::Other(format!("acct trades prepare: {e}")))?;
        let rows = stmt
            .query_map(params![addr_hex, limit as i64], row_to_trade)
            .map_err(|e| opennodia_core::Error::Other(format!("acct trades: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?);
        }
        Ok(out)
    }

    /// Get/set the last synced round.
    pub fn get_last_synced_round(&self) -> opennodia_core::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let round: Option<i64> = conn
            .query_row(
                "SELECT value FROM sync_state WHERE key = 'last_synced_round'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| opennodia_core::Error::Other(format!("sync round: {e}")))?;
        Ok(round.unwrap_or(0) as u64)
    }

    /// Set the last synced round.
    pub fn set_last_synced_round(&self, round: u64) -> opennodia_core::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_state (key, value) VALUES ('last_synced_round', ?1) \
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![round as i64],
        )
        .map_err(|e| opennodia_core::Error::Other(format!("set sync round: {e}")))?;
        Ok(())
    }

    /// Count active orders (for diagnostics).
    pub fn count_active_orders(&self) -> opennodia_core::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM orders WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| opennodia_core::Error::Other(format!("count: {e}")))?;
        Ok(count as u64)
    }

    /// Get the most recent trade price for a pair (micro-ratio), if any.
    ///
    /// Used to populate `last_price` on the orderbook snapshot so the UI can
    /// show a last-traded price instead of a perpetual dash.
    pub fn get_last_trade_price(
        &self,
        pair: Pair,
        view_base_asset: u64,
    ) -> opennodia_core::Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(i64, Option<i64>)> = conn
            .query_row(
                "SELECT price, base_asset FROM trades \
                 WHERE pair_a = ?1 AND pair_b = ?2 AND base_asset IS NOT NULL \
                 ORDER BY round DESC LIMIT 1",
                params![pair.asset_a as i64, pair.asset_b as i64],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| opennodia_core::Error::Other(format!("last trade price: {e}")))?;
        let Some((price, Some(base_asset))) = row else {
            return Ok(None);
        };
        let price = u64::try_from(price).map_err(|_| {
            opennodia_core::Error::Other("last trade price is negative".to_string())
        })?;
        let base_asset = u64::try_from(base_asset).map_err(|_| {
            opennodia_core::Error::Other("last trade base asset is negative".to_string())
        })?;
        Ok(Some(if base_asset == view_base_asset {
            price
        } else {
            invert_price(price)
        }))
    }

    /// Aggregate popularity statistics across all pairs.
    ///
    /// For each distinct pair (from active orders and recent trades) this
    /// computes: active order count, recent trade count, recent trade volume,
    /// and the last execution price. A composite `score` ranks the pairs so
    /// the UI can present a "popular pairs" sidebar.
    ///
    /// `recent_trade_round` bounds the trade window (inclusive lower bound);
    /// trades with `round >= recent_trade_round` are counted.
    pub fn get_pair_stats(
        &self,
        recent_trade_round: Round,
        limit: u32,
    ) -> opennodia_core::Result<Vec<PairStat>> {
        let conn = self.conn.lock().unwrap();

        // Collect active-order counts per canonical pair. A pair is canonical
        // (lower asset id first), but orders store sell/buy directly, so we
        // normalize with MIN/MAX in SQL.
        let mut active_map: std::collections::HashMap<(u64, u64), u64> =
            std::collections::HashMap::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT MIN(sell_asset, buy_asset) AS a, \
                            MAX(sell_asset, buy_asset) AS b, \
                            COUNT(*) AS cnt \
                     FROM orders WHERE status = 'active' \
                     GROUP BY a, b",
                )
                .map_err(|e| opennodia_core::Error::Other(format!("pair active prepare: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    let a: i64 = row.get(0)?;
                    let b: i64 = row.get(1)?;
                    let cnt: i64 = row.get(2)?;
                    Ok((a as u64, b as u64, cnt as u64))
                })
                .map_err(|e| opennodia_core::Error::Other(format!("pair active rows: {e}")))?;
            for r in rows {
                let (a, b, cnt) =
                    r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?;
                *active_map.entry((a, b)).or_insert(0) += cnt;
            }
        }

        // Collect recent trade stats per canonical pair.
        #[derive(Default, Clone, Copy)]
        struct TradeAgg {
            count: u64,
            volume: u64,
            last_price: Option<u64>,
            last_round: u64,
        }
        let mut trade_map: std::collections::HashMap<(u64, u64), TradeAgg> =
            std::collections::HashMap::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT pair_a, pair_b, COUNT(*) AS cnt, \
                            COALESCE(SUM(amount), 0) AS vol \
                     FROM trades WHERE round >= ?1 \
                     GROUP BY pair_a, pair_b",
                )
                .map_err(|e| opennodia_core::Error::Other(format!("pair trades prepare: {e}")))?;
            let rows = stmt
                .query_map(params![recent_trade_round.as_u64() as i64], |row| {
                    let a: i64 = row.get(0)?;
                    let b: i64 = row.get(1)?;
                    let cnt: i64 = row.get(2)?;
                    let vol: i64 = row.get(3)?;
                    Ok((a as u64, b as u64, cnt as u64, vol as u64))
                })
                .map_err(|e| opennodia_core::Error::Other(format!("pair trades rows: {e}")))?;
            for r in rows {
                let (a, b, cnt, vol) =
                    r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?;
                let pair = Pair::new(a, b);
                let agg = trade_map.entry((pair.asset_a, pair.asset_b)).or_default();
                agg.count = cnt;
                agg.volume = vol;
            }
        }

        // Last price per pair (most recent trade regardless of window).
        {
            let mut stmt = conn
                .prepare(
                    "SELECT pair_a, pair_b, price, base_asset, round \
                     FROM trades WHERE base_asset IS NOT NULL ORDER BY round DESC",
                )
                .map_err(|e| opennodia_core::Error::Other(format!("last price prepare: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    let a: i64 = row.get(0)?;
                    let b: i64 = row.get(1)?;
                    let price: i64 = row.get(2)?;
                    let base_asset: Option<i64> = row.get(3)?;
                    let round: i64 = row.get(4)?;
                    Ok((
                        a as u64,
                        b as u64,
                        price as u64,
                        base_asset.map(|asset| asset as u64),
                        round as u64,
                    ))
                })
                .map_err(|e| opennodia_core::Error::Other(format!("last price rows: {e}")))?;
            let mut seen: std::collections::HashSet<(u64, u64)> = std::collections::HashSet::new();
            for r in rows {
                let (a, b, price, base_asset, round) =
                    r.map_err(|e| opennodia_core::Error::Other(format!("row: {e}")))?;
                let pair = Pair::new(a, b);
                let key = (pair.asset_a, pair.asset_b);
                if seen.insert(key) {
                    let agg = trade_map.entry(key).or_default();
                    agg.last_price = base_asset.map(|base| {
                        if base == pair.asset_b {
                            price
                        } else {
                            invert_price(price)
                        }
                    });
                    agg.last_round = round;
                }
            }
        }

        // Merge active + trade pairs and compute scores.
        let mut all_pairs: std::collections::HashSet<(u64, u64)> = std::collections::HashSet::new();
        all_pairs.extend(active_map.keys());
        all_pairs.extend(trade_map.keys());

        // Score weights favor active orders and recent trade activity:
        //   active orders x1, recent trade count x2, volume (log-scaled) x1.5
        let mut stats: Vec<PairStat> = all_pairs
            .into_iter()
            .map(|(a, b)| {
                let active = *active_map.get(&(a, b)).unwrap_or(&0);
                let agg = trade_map.get(&(a, b)).copied().unwrap_or_default();
                // Log-scale volume so one huge trade doesn't dominate.
                let volume_score = if agg.volume == 0 {
                    0u64
                } else {
                    ((agg.volume as f64).ln().ceil()) as u64
                };
                let score = active
                    .saturating_add(agg.count.saturating_mul(2))
                    .saturating_add(volume_score.saturating_mul(3 / 2));
                PairStat {
                    pair: Pair::new(a, b),
                    active_orders: active,
                    recent_trade_count: agg.count,
                    recent_trade_volume: agg.volume,
                    last_price: agg.last_price,
                    score,
                }
            })
            .collect();

        // Sort by score descending, then by pair for determinism.
        stats.sort_by(|x, y| {
            y.score
                .cmp(&x.score)
                .then_with(|| y.pair.asset_b.cmp(&x.pair.asset_b))
        });
        stats.truncate(limit as usize);
        Ok(stats)
    }
}

// ----------------------------------------------------------------------------
// Row mapping
// ----------------------------------------------------------------------------

fn row_to_order_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<OrderEntry> {
    let escrow_hex: String = row.get("escrow_addr")?;
    let side_str: String = row.get("side")?;
    let sell_asset: i64 = row.get("sell_asset")?;
    let sell_amount: i64 = row.get("sell_amount")?;
    let buy_asset: i64 = row.get("buy_asset")?;
    let buy_amount: i64 = row.get("buy_amount")?;
    let price: i64 = row.get("price")?;
    let owner_hex: String = row.get("owner")?;
    let created_round: i64 = row.get("created_round")?;
    let expire_round: i64 = row.get("expire_round")?;
    let status_str: String = row.get("status")?;
    let filled_amount: i64 = row.get("filled_amount")?;
    let split_index: i64 = row.get("split_index")?;
    let parent_id: Option<String> = row.get("parent_id")?;
    let program_hex: String = row.get("program")?;
    let params_json: String = row.get("params")?;

    let escrow_addr = decode_address(&escrow_hex, "escrow address")?;
    let owner = decode_address(&owner_hex, "owner address")?;
    let side = match side_str.as_str() {
        "sell" => OrderSide::Sell,
        "buy" => OrderSide::Buy,
        _ => return Err(invalid_text(format!("invalid order side: {side_str}"))),
    };
    let status = EntryStatus::parse(&status_str)
        .ok_or_else(|| invalid_text(format!("invalid order status: {status_str}")))?;
    let sell_asset = nonnegative(sell_asset, "sell asset")?;
    let sell_amount = nonnegative(sell_amount, "sell amount")?;
    let buy_asset = nonnegative(buy_asset, "buy asset")?;
    let buy_amount = nonnegative(buy_amount, "buy amount")?;
    let price = nonnegative(price, "price")?;
    let created_round = nonnegative(created_round, "created round")?;
    let expire_round = nonnegative(expire_round, "expire round")?;
    let filled_amount = nonnegative(filled_amount, "filled amount")?;
    let split_index = u32::try_from(nonnegative(split_index, "split index")?)
        .map_err(|_| invalid_text("split index exceeds u32"))?;
    if filled_amount > sell_amount {
        return Err(invalid_text("filled amount exceeds sell amount"));
    }

    let program = hex::decode(&program_hex)
        .map_err(|error| invalid_text(format!("invalid program: {error}")))?;
    if program.is_empty() || program.len() > MAX_LOGICSIG_PROGRAM_BYTES {
        return Err(invalid_text(format!(
            "invalid LogicSig program length: {}",
            program.len()
        )));
    }
    if escrow_address(&program) != escrow_addr {
        return Err(invalid_text(
            "stored program does not derive the escrow address",
        ));
    }
    let params: EscrowParams = serde_json::from_str(&params_json)
        .map_err(|error| invalid_text(format!("invalid escrow params: {error}")))?;
    let kind = match side {
        OrderSide::Sell => EscrowKind::Sell,
        OrderSide::Buy => EscrowKind::Buy,
    };
    validate_params(kind, &params)
        .map_err(|error| invalid_text(format!("invalid escrow params: {error}")))?;
    if params.owner != owner
        || params.sell_asset != sell_asset
        || params.sell_amount != sell_amount
        || params.buy_asset != buy_asset
        || params.buy_amount != buy_amount
        || params.expire_round != expire_round
    {
        return Err(invalid_text(
            "stored escrow params do not match indexed order fields",
        ));
    }

    Ok(OrderEntry {
        escrow_addr,
        side,
        sell_asset,
        sell_amount,
        buy_asset,
        buy_amount,
        price,
        owner,
        created_round: Round(created_round),
        expire_round: Round(expire_round),
        status,
        filled_amount,
        split_index,
        parent_id,
        program,
        params,
    })
}

fn row_to_trade(row: &rusqlite::Row<'_>) -> rusqlite::Result<Trade> {
    let tx_id: String = row.get("tx_id")?;
    let pair_a: i64 = row.get("pair_a")?;
    let pair_b: i64 = row.get("pair_b")?;
    let side_str: String = row.get("side")?;
    let price: i64 = row.get("price")?;
    let base_asset: Option<i64> = row.get("base_asset")?;
    let amount: i64 = row.get("amount")?;
    let buyer_hex: String = row.get("buyer")?;
    let seller_hex: String = row.get("seller")?;
    let round: i64 = row.get("round")?;
    let timestamp: i64 = row.get("timestamp")?;

    let side = match side_str.as_str() {
        "sell" => OrderSide::Sell,
        "buy" => OrderSide::Buy,
        _ => return Err(invalid_text(format!("invalid trade side: {side_str}"))),
    };

    Ok(Trade {
        tx_id,
        pair: Pair::new(
            nonnegative(pair_a, "pair asset A")?,
            nonnegative(pair_b, "pair asset B")?,
        ),
        side,
        price: nonnegative(price, "trade price")?,
        base_asset: base_asset
            .map(|asset| nonnegative(asset, "trade base asset"))
            .transpose()?,
        amount: nonnegative(amount, "trade amount")?,
        buyer: decode_address(&buyer_hex, "buyer address")?,
        seller: decode_address(&seller_hex, "seller address")?,
        round: Round(nonnegative(round, "trade round")?),
        timestamp: nonnegative(timestamp, "trade timestamp")?,
    })
}

fn row_to_community_market(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommunityMarket> {
    let id: String = row.get("id")?;
    let operator_hex: String = row.get("operator")?;
    let name: String = row.get("name")?;
    let description: String = row.get("description")?;
    let logo_url: String = row.get("logo_url")?;
    let asset_ids_json: String = row.get("asset_ids")?;
    let pairs_json: String = row.get("pairs")?;
    let migration_notice: Option<String> = row.get("migration_notice")?;
    let announcement_channel: Option<String> = row.get("announcement_channel")?;
    let signature: String = row.get("signature")?;
    let updated_at: i64 = row.get("updated_at")?;
    let asset_ids: Vec<u64> = serde_json::from_str(&asset_ids_json)
        .map_err(|error| invalid_text(format!("invalid market asset list: {error}")))?;
    let pairs: Vec<Pair> = serde_json::from_str(&pairs_json)
        .map_err(|error| invalid_text(format!("invalid market pair list: {error}")))?;
    Ok(CommunityMarket {
        id,
        operator: decode_address(&operator_hex, "market operator")?,
        name,
        description,
        logo_url,
        asset_ids,
        pairs,
        migration_notice,
        announcement_channel,
        signature,
        updated_at: nonnegative(updated_at, "market updated_at")?,
    })
}

fn sqlite_int(value: u64, field: &str) -> opennodia_core::Result<i64> {
    i64::try_from(value)
        .map_err(|_| opennodia_core::Error::Other(format!("{field} exceeds SQLite integer range")))
}

fn validate_community_market(market: &CommunityMarket) -> opennodia_core::Result<()> {
    if market.id.is_empty()
        || market.id.len() > 64
        || !market.id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        return Err(opennodia_core::Error::Other(
            "community market id must be 1-64 chars of lowercase letters, digits, '-' or '_'"
                .to_string(),
        ));
    }
    if market.operator.is_zero() {
        return Err(opennodia_core::Error::Other(
            "community market operator must not be the zero address".to_string(),
        ));
    }
    if market.name.trim().is_empty() || market.name.len() > 120 {
        return Err(opennodia_core::Error::Other(
            "community market name must be 1-120 bytes".to_string(),
        ));
    }
    if market.description.len() > 2_000 {
        return Err(opennodia_core::Error::Other(
            "community market description must be at most 2000 bytes".to_string(),
        ));
    }
    if market.logo_url.len() > 500 {
        return Err(opennodia_core::Error::Other(
            "community market logo_url must be at most 500 bytes".to_string(),
        ));
    }
    if market.asset_ids.is_empty() {
        return Err(opennodia_core::Error::Other(
            "community market must list at least one official ASA".to_string(),
        ));
    }
    if market.asset_ids.contains(&0) {
        return Err(opennodia_core::Error::Other(
            "community market official asset ids must be ASAs; use ALGO only in pairs".to_string(),
        ));
    }
    if market.pairs.is_empty() {
        return Err(opennodia_core::Error::Other(
            "community market must list at least one official pair".to_string(),
        ));
    }
    if market
        .pairs
        .iter()
        .any(|pair| pair.asset_a == pair.asset_b || pair.asset_a == 0 && pair.asset_b == 0)
    {
        return Err(opennodia_core::Error::Other(
            "community market pair must contain two distinct assets".to_string(),
        ));
    }
    if !market.pairs.iter().all(|pair| {
        market
            .asset_ids
            .iter()
            .any(|asset_id| pair.contains(*asset_id))
    }) {
        return Err(opennodia_core::Error::Other(
            "each community market pair must include at least one official ASA".to_string(),
        ));
    }
    if market.signature.trim().is_empty() {
        return Err(opennodia_core::Error::Other(
            "community market signature must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn migrate_schema(conn: &Connection) -> opennodia_core::Result<()> {
    let has_base_asset = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(trades)")
            .map_err(|e| opennodia_core::Error::Other(format!("trade schema inspect: {e}")))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| opennodia_core::Error::Other(format!("trade schema columns: {e}")))?;
        let mut found = false;
        for column in columns {
            if column.map_err(|e| opennodia_core::Error::Other(format!("trade schema row: {e}")))?
                == "base_asset"
            {
                found = true;
                break;
            }
        }
        found
    };
    if !has_base_asset {
        conn.execute("ALTER TABLE trades ADD COLUMN base_asset INTEGER", [])
            .map_err(|e| opennodia_core::Error::Other(format!("trade schema migration: {e}")))?;
    }
    let order_columns = table_columns(conn, "orders")?;
    if !order_columns.contains("resolution_tx_id") {
        conn.execute("ALTER TABLE orders ADD COLUMN resolution_tx_id TEXT", [])
            .map_err(|e| {
                opennodia_core::Error::Other(format!("order tx id schema migration: {e}"))
            })?;
    }
    if !order_columns.contains("resolution_round") {
        conn.execute("ALTER TABLE orders ADD COLUMN resolution_round INTEGER", [])
            .map_err(|e| {
                opennodia_core::Error::Other(format!("order round schema migration: {e}"))
            })?;
    }
    let trade_columns = table_columns(conn, "trades")?;
    if !trade_columns.contains("escrow_addr") {
        conn.execute("ALTER TABLE trades ADD COLUMN escrow_addr TEXT", [])
            .map_err(|e| {
                opennodia_core::Error::Other(format!("trade escrow schema migration: {e}"))
            })?;
    }
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_trades_escrow \
         ON trades (escrow_addr) WHERE escrow_addr IS NOT NULL",
        [],
    )
    .map_err(|e| opennodia_core::Error::Other(format!("trade escrow index: {e}")))?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|e| opennodia_core::Error::Other(format!("schema version: {e}")))?;
    Ok(())
}

fn table_columns(
    conn: &Connection,
    table: &str,
) -> opennodia_core::Result<std::collections::HashSet<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| opennodia_core::Error::Other(format!("{table} schema inspect: {e}")))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| opennodia_core::Error::Other(format!("{table} schema columns: {e}")))?;
    let mut names = std::collections::HashSet::new();
    for column in columns {
        names.insert(
            column.map_err(|e| opennodia_core::Error::Other(format!("{table} schema row: {e}")))?,
        );
    }
    Ok(names)
}

fn validate_order_entry(entry: &OrderEntry) -> opennodia_core::Result<()> {
    if entry.status != EntryStatus::Active || entry.filled_amount != 0 {
        return Err(opennodia_core::Error::Other(
            "new order must be active and unfilled".to_string(),
        ));
    }
    if entry.program.is_empty() || entry.program.len() > MAX_LOGICSIG_PROGRAM_BYTES {
        return Err(opennodia_core::Error::Other(format!(
            "invalid LogicSig program length: {}",
            entry.program.len()
        )));
    }
    if escrow_address(&entry.program) != entry.escrow_addr {
        return Err(opennodia_core::Error::Other(
            "program does not derive the escrow address".to_string(),
        ));
    }
    let kind = match entry.side {
        OrderSide::Sell => EscrowKind::Sell,
        OrderSide::Buy => EscrowKind::Buy,
    };
    validate_params(kind, &entry.params)?;
    if entry.params.owner != entry.owner
        || entry.params.sell_asset != entry.sell_asset
        || entry.params.sell_amount != entry.sell_amount
        || entry.params.buy_asset != entry.buy_asset
        || entry.params.buy_amount != entry.buy_amount
        || entry.params.expire_round != entry.expire_round.as_u64()
    {
        return Err(opennodia_core::Error::Other(
            "escrow params do not match indexed order fields".to_string(),
        ));
    }
    let expected_price = crate::types::order_price(
        entry.side,
        entry.sell_asset,
        entry.sell_amount,
        entry.buy_asset,
        entry.buy_amount,
    )
    .ok_or_else(|| opennodia_core::Error::Other("invalid order price inputs".to_string()))?;
    if entry.price != expected_price {
        return Err(opennodia_core::Error::Other(format!(
            "order price mismatch: stored {}, expected {expected_price}",
            entry.price
        )));
    }
    Ok(())
}

fn nonnegative(value: i64, field: &str) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| invalid_text(format!("{field} is negative")))
}

fn decode_address(value: &str, field: &str) -> rusqlite::Result<Address> {
    let bytes =
        hex::decode(value).map_err(|error| invalid_text(format!("invalid {field}: {error}")))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| invalid_text(format!("{field} must contain exactly 32 bytes")))?;
    Ok(Address::from_bytes(bytes))
}

fn invalid_text(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.into(),
        )),
    )
}

// ----------------------------------------------------------------------------
// SQL
// ----------------------------------------------------------------------------

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS orders (
    escrow_addr   TEXT PRIMARY KEY,
    side          TEXT NOT NULL,
    sell_asset    INTEGER NOT NULL,
    sell_amount   INTEGER NOT NULL,
    buy_asset     INTEGER NOT NULL,
    buy_amount    INTEGER NOT NULL,
    price         INTEGER NOT NULL,
    owner         TEXT NOT NULL,
    created_round INTEGER NOT NULL,
    expire_round  INTEGER NOT NULL,
    status        TEXT NOT NULL DEFAULT 'active',
    filled_amount INTEGER NOT NULL DEFAULT 0,
    split_index   INTEGER NOT NULL DEFAULT 0,
    parent_id     TEXT,
    program       TEXT,
    params        TEXT,
    resolution_tx_id TEXT,
    resolution_round INTEGER
);
CREATE INDEX IF NOT EXISTS idx_orders_pair_status_price
    ON orders (sell_asset, buy_asset, status, price);
CREATE INDEX IF NOT EXISTS idx_orders_owner_status
    ON orders (owner, status);
CREATE INDEX IF NOT EXISTS idx_orders_status_expire
    ON orders (status, expire_round);

CREATE TABLE IF NOT EXISTS trades (
    tx_id     TEXT PRIMARY KEY,
    pair_a    INTEGER NOT NULL,
    pair_b    INTEGER NOT NULL,
    side      TEXT NOT NULL,
    price     INTEGER NOT NULL,
    base_asset INTEGER,
    amount    INTEGER NOT NULL,
    buyer     TEXT NOT NULL,
    seller    TEXT NOT NULL,
    round     INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    escrow_addr TEXT
);
CREATE INDEX IF NOT EXISTS idx_trades_pair_round
    ON trades (pair_a, pair_b, round);
CREATE INDEX IF NOT EXISTS idx_trades_party
    ON trades (buyer, seller, round);

CREATE TABLE IF NOT EXISTS sync_state (
    key   TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS community_markets (
    id                   TEXT PRIMARY KEY,
    operator             TEXT NOT NULL,
    name                 TEXT NOT NULL,
    description          TEXT NOT NULL,
    logo_url             TEXT NOT NULL,
    asset_ids            TEXT NOT NULL,
    pairs                TEXT NOT NULL,
    migration_notice     TEXT,
    announcement_channel TEXT,
    signature            TEXT NOT NULL,
    updated_at           INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_community_markets_operator
    ON community_markets (operator, updated_at);
";

const INSERT_ORDER_SQL: &str = "
INSERT OR REPLACE INTO orders
    (escrow_addr, side, sell_asset, sell_amount, buy_asset, buy_amount, price,
     owner, created_round, expire_round, status, filled_amount, split_index,
     parent_id, program, params)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
";

const SELECT_ORDER_SQL: &str = "SELECT * FROM orders WHERE escrow_addr = ?1";

const INSERT_TRADE_SQL: &str = "
INSERT OR REPLACE INTO trades
    (tx_id, pair_a, pair_b, side, price, base_asset, amount, buyer, seller, round, timestamp,
     escrow_addr)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
";

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PriceLevel;
    use opennodia_swap::EscrowKind;

    fn sample_entry(escrow_byte: u8, sell_amount: u64, buy_amount: u64) -> OrderEntry {
        let owner = Address::from_bytes([1u8; 32]);
        let params = EscrowParams::new(owner, 12345, sell_amount, 0, buy_amount, 100_000);
        let price = if sell_amount == 0 {
            0
        } else {
            ((buy_amount as u128) * 1_000_000 / sell_amount as u128) as u64
        };
        let program = vec![escrow_byte, 0x01, 0x02];
        OrderEntry {
            escrow_addr: escrow_address(&program),
            side: OrderSide::Sell,
            sell_asset: 12345,
            sell_amount,
            buy_asset: 0,
            buy_amount,
            price,
            owner,
            created_round: Round(1000),
            expire_round: Round(100_000),
            status: EntryStatus::Active,
            filled_amount: 0,
            split_index: 0,
            parent_id: None,
            program,
            params,
        }
    }

    #[test]
    fn open_memory_creates_schema() {
        let db = DexDb::open_memory().unwrap();
        assert_eq!(db.count_active_orders().unwrap(), 0);
        assert_eq!(db.get_last_synced_round().unwrap(), 0);
    }

    #[test]
    fn register_and_get_order() {
        let db = DexDb::open_memory().unwrap();
        let entry = sample_entry(0xAA, 1000, 2_000_000);
        db.register_order(&entry).unwrap();
        let got = db.get_order(&entry.escrow_addr).unwrap().unwrap();
        assert_eq!(got.escrow_addr, entry.escrow_addr);
        assert_eq!(got.sell_amount, 1000);
        assert_eq!(got.buy_amount, 2_000_000);
        assert_eq!(got.status, EntryStatus::Active);
    }

    #[test]
    fn community_market_roundtrip_and_search() {
        let db = DexDb::open_memory().unwrap();
        let operator = Address::from_bytes([9u8; 32]);
        let market = CommunityMarket {
            id: "qat-market".into(),
            operator,
            name: "QAT Market".into(),
            description: "Official QAT pairs".into(),
            logo_url: "https://example.com/qat.png".into(),
            asset_ids: vec![42, 99],
            pairs: vec![Pair::new(0, 42), Pair::new(42, 99)],
            migration_notice: Some("Use the v2 asset".into()),
            announcement_channel: Some("qat-announcements".into()),
            signature: "signed".into(),
            updated_at: 1234,
        };
        db.upsert_community_market(&market).unwrap();
        let loaded = db.get_community_market("qat-market").unwrap().unwrap();
        assert_eq!(loaded, market);
        assert_eq!(
            db.list_community_markets(Some(&operator), Some(42), 10)
                .unwrap(),
            vec![market]
        );
        assert!(db
            .list_community_markets(Some(&Address::from_bytes([8u8; 32])), None, 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn community_market_rejects_stale_signed_metadata() {
        let db = DexDb::open_memory().unwrap();
        let operator = Address::from_bytes([9u8; 32]);
        let mut market = CommunityMarket {
            id: "qat-market".into(),
            operator,
            name: "QAT Market".into(),
            description: "Official QAT pairs".into(),
            logo_url: "https://example.com/qat.png".into(),
            asset_ids: vec![42],
            pairs: vec![Pair::new(0, 42)],
            migration_notice: None,
            announcement_channel: None,
            signature: "signed".into(),
            updated_at: 200,
        };
        db.upsert_community_market(&market).unwrap();

        market.name = "Old QAT Market".into();
        market.updated_at = 100;
        assert!(db.upsert_community_market(&market).is_err());
        let loaded = db.get_community_market("qat-market").unwrap().unwrap();
        assert_eq!(loaded.name, "QAT Market");
        assert_eq!(loaded.updated_at, 200);
    }

    #[test]
    fn community_market_rejects_unofficial_pair() {
        let db = DexDb::open_memory().unwrap();
        let market = CommunityMarket {
            id: "bad-market".into(),
            operator: Address::from_bytes([9u8; 32]),
            name: "Bad Market".into(),
            description: String::new(),
            logo_url: String::new(),
            asset_ids: vec![42],
            pairs: vec![Pair::new(0, 99)],
            migration_notice: None,
            announcement_channel: None,
            signature: "signed".into(),
            updated_at: 1234,
        };
        assert!(db.upsert_community_market(&market).is_err());
    }

    #[test]
    fn update_status() {
        let db = DexDb::open_memory().unwrap();
        let entry = sample_entry(0xBB, 1000, 2_000_000);
        db.register_order(&entry).unwrap();
        db.update_order_status(&entry.escrow_addr, EntryStatus::Filled)
            .unwrap();
        let got = db.get_order(&entry.escrow_addr).unwrap().unwrap();
        assert_eq!(got.status, EntryStatus::Filled);
    }

    #[test]
    fn update_filled_amount() {
        let db = DexDb::open_memory().unwrap();
        let entry = sample_entry(0xCC, 1000, 2_000_000);
        db.register_order(&entry).unwrap();
        db.update_filled_amount(&entry.escrow_addr, 400).unwrap();
        let got = db.get_order(&entry.escrow_addr).unwrap().unwrap();
        assert_eq!(got.filled_amount, 400);
    }

    #[test]
    fn get_active_orders_for_pair() {
        let db = DexDb::open_memory().unwrap();
        db.register_order(&sample_entry(1, 100, 200_000)).unwrap();
        db.register_order(&sample_entry(2, 200, 300_000)).unwrap();
        db.register_order(&sample_entry(3, 300, 400_000)).unwrap();
        // Mark one as filled (should be excluded).
        let third = sample_entry(3, 300, 400_000);
        db.update_order_status(&third.escrow_addr, EntryStatus::Filled)
            .unwrap();

        let pair = Pair::new(0, 12345);
        let orders = db.get_active_orders_for_pair(pair).unwrap();
        assert_eq!(orders.len(), 2);
        // Sorted by price ascending.
        assert!(orders[0].price <= orders[1].price);
    }

    #[test]
    fn get_orders_for_owner() {
        let db = DexDb::open_memory().unwrap();
        db.register_order(&sample_entry(1, 100, 200_000)).unwrap();
        db.register_order(&sample_entry(2, 200, 300_000)).unwrap();

        let owner = Address::from_bytes([1u8; 32]);
        let orders = db.get_orders_for_owner(&owner, None).unwrap();
        assert_eq!(orders.len(), 2);

        let active = db
            .get_orders_for_owner(&owner, Some(EntryStatus::Active))
            .unwrap();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn record_and_get_trades() {
        let db = DexDb::open_memory().unwrap();
        let trade = Trade {
            tx_id: "abc123".into(),
            pair: Pair::new(0, 12345),
            side: OrderSide::Buy,
            price: 2_000_000,
            base_asset: Some(0),
            amount: 1000,
            buyer: Address::from_bytes([1u8; 32]),
            seller: Address::from_bytes([2u8; 32]),
            round: Round(50_000),
            timestamp: 1_700_000_000,
        };
        db.record_trade(&trade).unwrap();

        let recent = db.get_recent_trades(Pair::new(0, 12345), 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].tx_id, "abc123");
        assert_eq!(recent[0].amount, 1000);

        let acct = db
            .get_trades_for_account(&Address::from_bytes([1u8; 32]), 10)
            .unwrap();
        assert_eq!(acct.len(), 1);
    }

    #[test]
    fn record_fill_is_atomic_and_idempotent() {
        let db = DexDb::open_memory().unwrap();
        let entry = sample_entry(0xE1, 1_000, 2_000_000);
        db.register_order(&entry).unwrap();
        let trade = Trade {
            tx_id: "fill-tx".into(),
            pair: Pair::new(0, 12345),
            side: OrderSide::Sell,
            price: entry.price,
            base_asset: Some(12345),
            amount: 1_000,
            buyer: Address::from_bytes([2u8; 32]),
            seller: entry.owner,
            round: Round(50_000),
            timestamp: 1_700_000_000,
        };

        db.record_fill(&entry.escrow_addr, entry.sell_amount, &trade)
            .unwrap();
        db.record_fill(&entry.escrow_addr, entry.sell_amount, &trade)
            .unwrap();

        let stored = db.get_order(&entry.escrow_addr).unwrap().unwrap();
        assert_eq!(stored.status, EntryStatus::Filled);
        assert_eq!(stored.filled_amount, entry.sell_amount);
        let trades = db
            .get_recent_trades(Pair::new(entry.sell_asset, entry.buy_asset), 10)
            .unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].tx_id, trade.tx_id);
    }

    #[test]
    fn unresolved_order_can_be_resolved_as_cancelled() {
        let db = DexDb::open_memory().unwrap();
        let entry = sample_entry(0xE2, 1_000, 2_000_000);
        db.register_order(&entry).unwrap();
        db.mark_closed_unresolved(&entry.escrow_addr, Round(49_999))
            .unwrap();
        db.record_cancel(&entry.escrow_addr, "cancel-tx", Round(50_000))
            .unwrap();
        db.record_cancel(&entry.escrow_addr, "cancel-tx", Round(50_000))
            .unwrap();

        let stored = db.get_order(&entry.escrow_addr).unwrap().unwrap();
        assert_eq!(stored.status, EntryStatus::Cancelled);
        assert!(db.get_reconcilable_orders().unwrap().is_empty());
    }

    #[test]
    fn legacy_filled_order_is_reconciled_with_trade_evidence() {
        let db = DexDb::open_memory().unwrap();
        let entry = sample_entry(0xE3, 1_000, 2_000_000);
        db.register_order(&entry).unwrap();
        db.update_order_status(&entry.escrow_addr, EntryStatus::Filled)
            .unwrap();
        assert_eq!(db.get_reconcilable_orders().unwrap().len(), 1);

        let trade = Trade {
            tx_id: "legacy-fill-tx".into(),
            pair: Pair::new(0, 12345),
            side: OrderSide::Sell,
            price: entry.price,
            base_asset: Some(12345),
            amount: entry.sell_amount,
            buyer: Address::from_bytes([2u8; 32]),
            seller: entry.owner,
            round: Round(50_000),
            timestamp: 1_700_000_000,
        };
        db.record_fill(&entry.escrow_addr, entry.sell_amount, &trade)
            .unwrap();

        assert!(db.get_reconcilable_orders().unwrap().is_empty());
        assert_eq!(
            db.get_recent_trades(Pair::new(0, 12345), 10).unwrap().len(),
            1
        );
    }

    #[test]
    fn legacy_cancelled_order_is_reconciled_with_evidence() {
        let db = DexDb::open_memory().unwrap();
        let entry = sample_entry(0xE4, 1_000, 2_000_000);
        db.register_order(&entry).unwrap();
        db.update_order_status(&entry.escrow_addr, EntryStatus::Cancelled)
            .unwrap();
        assert_eq!(db.get_reconcilable_orders().unwrap().len(), 1);

        db.record_cancel(&entry.escrow_addr, "legacy-cancel-tx", Round(50_000))
            .unwrap();

        assert!(db.get_reconcilable_orders().unwrap().is_empty());
        assert_eq!(
            db.get_order(&entry.escrow_addr).unwrap().unwrap().status,
            EntryStatus::Cancelled
        );
    }

    #[test]
    fn mark_expired_updates_status() {
        let db = DexDb::open_memory().unwrap();
        let mut entry = sample_entry(0xDD, 1000, 2_000_000);
        entry.expire_round = Round(5_000);
        entry.params.expire_round = 5_000;
        db.register_order(&entry).unwrap();

        let count = db.mark_expired(Round(10_000)).unwrap();
        assert_eq!(count, 1);
        let got = db.get_order(&entry.escrow_addr).unwrap().unwrap();
        assert_eq!(got.status, EntryStatus::Expired);
    }

    #[test]
    fn sync_round_roundtrip() {
        let db = DexDb::open_memory().unwrap();
        assert_eq!(db.get_last_synced_round().unwrap(), 0);
        db.set_last_synced_round(42_000).unwrap();
        assert_eq!(db.get_last_synced_round().unwrap(), 42_000);
        db.set_last_synced_round(43_000).unwrap();
        assert_eq!(db.get_last_synced_round().unwrap(), 43_000);
    }

    #[test]
    fn entry_escrow_kind() {
        let e = sample_entry(1, 100, 200);
        assert_eq!(e.escrow_kind(), EscrowKind::Sell);
    }

    #[test]
    fn price_level_total() {
        let lvl = PriceLevel {
            price: 2_000_000,
            amount: 100,
            order_count: 3,
            total: 100,
        };
        assert_eq!(lvl.total, 100);
    }
}

#[cfg(test)]
mod pair_stats_tests {
    use super::*;
    use opennodia_core::Address;
    use opennodia_swap::OrderSide;

    fn make_trade(tx_id: &str, pair: Pair, price: u64, amount: u64, round: u64) -> Trade {
        Trade {
            tx_id: tx_id.into(),
            pair,
            side: OrderSide::Buy,
            price,
            base_asset: Some(pair.asset_a),
            amount,
            buyer: Address::from_bytes([1u8; 32]),
            seller: Address::from_bytes([2u8; 32]),
            round: Round(round),
            timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn last_trade_price_returns_most_recent() {
        let db = DexDb::open_memory().unwrap();
        let pair = Pair::new(0, 12345);
        db.record_trade(&make_trade("t1", pair, 2_000_000, 100, 1000))
            .unwrap();
        db.record_trade(&make_trade("t2", pair, 2_100_000, 50, 2000))
            .unwrap();
        let price = db.get_last_trade_price(pair, pair.asset_a).unwrap();
        assert_eq!(price, Some(2_100_000));
    }

    #[test]
    fn last_trade_price_none_when_no_trades() {
        let db = DexDb::open_memory().unwrap();
        let pair = Pair::new(0, 12345);
        assert_eq!(db.get_last_trade_price(pair, pair.asset_a).unwrap(), None);
    }

    #[test]
    fn pair_stats_ranks_by_trade_activity() {
        let db = DexDb::open_memory().unwrap();
        // Pair A: 1 trade.
        let pair_a = Pair::new(0, 12345);
        db.record_trade(&make_trade("a1", pair_a, 2_000_000, 100, 1000))
            .unwrap();
        // Pair B: 3 trades with higher volume.
        let pair_b = Pair::new(0, 67890);
        db.record_trade(&make_trade("b1", pair_b, 1_000_000, 10, 5000))
            .unwrap();
        db.record_trade(&make_trade("b2", pair_b, 1_100_000, 20, 5100))
            .unwrap();
        db.record_trade(&make_trade("b3", pair_b, 1_200_000, 30, 5200))
            .unwrap();

        let stats = db.get_pair_stats(Round(0), 10).unwrap();
        assert_eq!(stats.len(), 2);
        // Pair B has more trades and volume → higher score → ranks first.
        assert_eq!(stats[0].pair, pair_b);
        assert!(stats[0].score > stats[1].score);
        assert_eq!(stats[0].recent_trade_count, 3);
        assert_eq!(stats[0].last_price, Some(833_333));
        assert_eq!(stats[1].pair, pair_a);
        assert_eq!(stats[1].recent_trade_count, 1);
    }

    #[test]
    fn pair_stats_respects_recent_window() {
        let db = DexDb::open_memory().unwrap();
        let pair = Pair::new(0, 12345);
        // Old trade (round 100), outside the window.
        db.record_trade(&make_trade("old", pair, 2_000_000, 100, 100))
            .unwrap();
        // Recent trade (round 5000), inside the window.
        db.record_trade(&make_trade("new", pair, 2_500_000, 50, 5000))
            .unwrap();

        // Window starts at round 4000: only the round-5000 trade counts.
        let stats = db.get_pair_stats(Round(4000), 10).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].recent_trade_count, 1);
        // Last price is the most recent trade regardless of window.
        assert_eq!(stats[0].last_price, Some(400_000));
    }

    #[test]
    fn pair_stats_empty_when_no_activity() {
        let db = DexDb::open_memory().unwrap();
        let stats = db.get_pair_stats(Round(0), 10).unwrap();
        assert!(stats.is_empty());
    }

    #[test]
    fn pair_stats_limits_results() {
        let db = DexDb::open_memory().unwrap();
        // Create 3 distinct pairs via trades.
        for (i, asset) in [111u64, 222, 333].iter().enumerate() {
            let pair = Pair::new(0, *asset);
            db.record_trade(&make_trade(&format!("t{i}"), pair, 1_000_000, 10, 1000))
                .unwrap();
        }
        let stats = db.get_pair_stats(Round(0), 2).unwrap();
        assert_eq!(stats.len(), 2);
    }
}
