//! Local ASA issuance history.
//!
//! The store records only public issuance metadata and policy snapshots. It
//! must never persist PINs, mnemonics, KMD tokens, or signed transaction bytes.

use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone)]
pub(crate) struct AsaIssueInsert {
    pub(crate) network: String,
    pub(crate) wallet_id: String,
    pub(crate) asset_id: u64,
    pub(crate) creator: String,
    pub(crate) txid: String,
    pub(crate) confirmed_round: u64,
    pub(crate) policy_grade: String,
    pub(crate) dex_eligible: bool,
    pub(crate) lp_eligible: bool,
    pub(crate) control_capable: bool,
    pub(crate) default_frozen: bool,
    pub(crate) manager: Option<String>,
    pub(crate) reserve: Option<String>,
    pub(crate) freeze: Option<String>,
    pub(crate) clawback: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AsaIssueRecord {
    pub network: String,
    pub wallet_id: String,
    pub asset_id: u64,
    pub creator: String,
    pub txid: String,
    pub confirmed_round: u64,
    pub policy_grade: String,
    pub dex_eligible: bool,
    pub lp_eligible: bool,
    pub control_capable: bool,
    pub default_frozen: bool,
    pub manager: Option<String>,
    pub reserve: Option<String>,
    pub freeze: Option<String>,
    pub clawback: Option<String>,
    pub created_at: u64,
}

#[derive(Debug)]
pub(crate) struct AsaIssueStore {
    conn: Mutex<Connection>,
}

impl AsaIssueStore {
    pub(crate) fn open(path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS asa_issues (
                network TEXT NOT NULL,
                wallet_id TEXT NOT NULL,
                asset_id INTEGER NOT NULL,
                creator TEXT NOT NULL,
                txid TEXT NOT NULL,
                confirmed_round INTEGER NOT NULL,
                policy_grade TEXT NOT NULL,
                dex_eligible INTEGER NOT NULL,
                lp_eligible INTEGER NOT NULL,
                control_capable INTEGER NOT NULL,
                default_frozen INTEGER NOT NULL,
                manager TEXT,
                reserve TEXT,
                freeze TEXT,
                clawback TEXT,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (network, asset_id)
            );
            CREATE INDEX IF NOT EXISTS idx_asa_issues_wallet_network
                ON asa_issues(wallet_id, network, confirmed_round DESC);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn record(&self, insert: &AsaIssueInsert) -> anyhow::Result<()> {
        let created_at = unix_timestamp();
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("ASA issue store lock poisoned"))?;
        conn.execute(
            "INSERT OR REPLACE INTO asa_issues (
                network, wallet_id, asset_id, creator, txid, confirmed_round,
                policy_grade, dex_eligible, lp_eligible, control_capable,
                default_frozen, manager, reserve, freeze, clawback, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                insert.network,
                insert.wallet_id,
                sqlite_int(insert.asset_id, "asset id")?,
                insert.creator,
                insert.txid,
                sqlite_int(insert.confirmed_round, "confirmed round")?,
                insert.policy_grade,
                insert.dex_eligible as i64,
                insert.lp_eligible as i64,
                insert.control_capable as i64,
                insert.default_frozen as i64,
                insert.manager,
                insert.reserve,
                insert.freeze,
                insert.clawback,
                sqlite_int(created_at, "created at")?,
            ],
        )?;
        Ok(())
    }

    pub(crate) fn list(
        &self,
        network: &str,
        wallet_id: &str,
    ) -> anyhow::Result<Vec<AsaIssueRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("ASA issue store lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT network, wallet_id, asset_id, creator, txid, confirmed_round,
                    policy_grade, dex_eligible, lp_eligible, control_capable,
                    default_frozen, manager, reserve, freeze, clawback, created_at
             FROM asa_issues
             WHERE network = ?1 AND wallet_id = ?2
             ORDER BY confirmed_round DESC, asset_id DESC",
        )?;
        let rows = stmt.query_map(params![network, wallet_id], |row| {
            Ok(AsaIssueRecord {
                network: row.get(0)?,
                wallet_id: row.get(1)?,
                asset_id: row.get::<_, i64>(2)? as u64,
                creator: row.get(3)?,
                txid: row.get(4)?,
                confirmed_round: row.get::<_, i64>(5)? as u64,
                policy_grade: row.get(6)?,
                dex_eligible: row.get::<_, i64>(7)? != 0,
                lp_eligible: row.get::<_, i64>(8)? != 0,
                control_capable: row.get::<_, i64>(9)? != 0,
                default_frozen: row.get::<_, i64>(10)? != 0,
                manager: row.get(11)?,
                reserve: row.get(12)?,
                freeze: row.get(13)?,
                clawback: row.get(14)?,
                created_at: row.get::<_, i64>(15)? as u64,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}

fn sqlite_int(value: u64, field: &str) -> anyhow::Result<i64> {
    i64::try_from(value).map_err(|_| anyhow::anyhow!("{field} exceeds SQLite integer range"))
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
