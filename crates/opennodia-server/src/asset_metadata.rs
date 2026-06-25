//! Local per-wallet asset metadata.
//!
//! This store contains user-owned labels only. It must never read from or
//! write to algod, indexer, public fallbacks, or any on-chain transaction.

use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;

const MAX_TAG_BYTES: usize = 64;
const MAX_MEMO_BYTES: usize = 1024;
const MAX_COLOR_LABEL_BYTES: usize = 24;

const COLOR_LABELS: &[&str] = &[
    "", "slate", "red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AssetUserMetadata {
    pub asset_id: u64,
    pub tag: String,
    pub memo: String,
    pub color_label: String,
    pub pinned: bool,
    pub updated_at: u64,
}

impl AssetUserMetadata {
    fn empty(asset_id: u64) -> Self {
        Self {
            asset_id,
            tag: String::new(),
            memo: String::new(),
            color_label: String::new(),
            pinned: false,
            updated_at: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetMetadataUpdate {
    pub(crate) tag: String,
    pub(crate) memo: String,
    pub(crate) color_label: String,
    pub(crate) pinned: bool,
}

impl AssetMetadataUpdate {
    fn normalize(self) -> anyhow::Result<Self> {
        let tag = normalize_single_line(self.tag, MAX_TAG_BYTES, "tag")?;
        let memo = normalize_memo(self.memo)?;
        let color_label =
            normalize_single_line(self.color_label, MAX_COLOR_LABEL_BYTES, "color label")?
                .to_ascii_lowercase();
        if !COLOR_LABELS.contains(&color_label.as_str()) {
            anyhow::bail!("unsupported color label: {color_label}");
        }
        Ok(Self {
            tag,
            memo,
            color_label,
            pinned: self.pinned,
        })
    }

    fn has_content(&self) -> bool {
        self.pinned || !self.tag.is_empty() || !self.memo.is_empty() || !self.color_label.is_empty()
    }
}

#[derive(Debug)]
pub(crate) struct AssetMetadataStore {
    conn: Mutex<Connection>,
}

impl AssetMetadataStore {
    pub(crate) fn open(path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> anyhow::Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS asset_metadata (
                network TEXT NOT NULL,
                wallet_address TEXT NOT NULL,
                asset_id INTEGER NOT NULL,
                tag TEXT NOT NULL DEFAULT '',
                memo TEXT NOT NULL DEFAULT '',
                color_label TEXT NOT NULL DEFAULT '',
                pinned INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (network, wallet_address, asset_id)
            );
            CREATE INDEX IF NOT EXISTS idx_asset_metadata_wallet
                ON asset_metadata(network, wallet_address, updated_at DESC);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn list(
        &self,
        network: &str,
        wallet_address: &str,
    ) -> anyhow::Result<Vec<AssetUserMetadata>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("asset metadata store lock poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT asset_id, tag, memo, color_label, pinned, updated_at
             FROM asset_metadata
             WHERE network = ?1 AND wallet_address = ?2
             ORDER BY pinned DESC, tag COLLATE NOCASE ASC, asset_id ASC",
        )?;
        let rows = stmt.query_map(params![network, wallet_address], |row| {
            Ok(AssetUserMetadata {
                asset_id: sqlite_u64(row.get(0)?, "asset id")?,
                tag: row.get(1)?,
                memo: row.get(2)?,
                color_label: row.get(3)?,
                pinned: row.get::<_, i64>(4)? != 0,
                updated_at: sqlite_u64(row.get(5)?, "updated at")?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub(crate) fn upsert(
        &self,
        network: &str,
        wallet_address: &str,
        asset_id: u64,
        update: AssetMetadataUpdate,
    ) -> anyhow::Result<AssetUserMetadata> {
        let update = update.normalize()?;
        if !update.has_content() {
            self.clear(network, wallet_address, asset_id)?;
            return Ok(AssetUserMetadata::empty(asset_id));
        }

        let updated_at = unix_timestamp();
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("asset metadata store lock poisoned"))?;
        conn.execute(
            "INSERT INTO asset_metadata (
                network, wallet_address, asset_id, tag, memo, color_label, pinned, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(network, wallet_address, asset_id) DO UPDATE SET
                tag = excluded.tag,
                memo = excluded.memo,
                color_label = excluded.color_label,
                pinned = excluded.pinned,
                updated_at = excluded.updated_at",
            params![
                network,
                wallet_address,
                sqlite_i64(asset_id, "asset id")?,
                &update.tag,
                &update.memo,
                &update.color_label,
                update.pinned as i64,
                sqlite_i64(updated_at, "updated at")?,
            ],
        )?;

        Ok(AssetUserMetadata {
            asset_id,
            tag: update.tag,
            memo: update.memo,
            color_label: update.color_label,
            pinned: update.pinned,
            updated_at,
        })
    }

    pub(crate) fn clear(
        &self,
        network: &str,
        wallet_address: &str,
        asset_id: u64,
    ) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("asset metadata store lock poisoned"))?;
        conn.execute(
            "DELETE FROM asset_metadata
             WHERE network = ?1 AND wallet_address = ?2 AND asset_id = ?3",
            params![network, wallet_address, sqlite_i64(asset_id, "asset id")?],
        )?;
        Ok(())
    }

    pub(crate) fn delete_address(&self, network: &str, wallet_address: &str) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("asset metadata store lock poisoned"))?;
        conn.execute(
            "DELETE FROM asset_metadata WHERE network = ?1 AND wallet_address = ?2",
            params![network, wallet_address],
        )?;
        Ok(())
    }
}

fn normalize_single_line(value: String, max_bytes: usize, field: &str) -> anyhow::Result<String> {
    let normalized = value.trim().to_string();
    if normalized.len() > max_bytes {
        anyhow::bail!("{field} exceeds {max_bytes} bytes");
    }
    if normalized.chars().any(char::is_control) {
        anyhow::bail!("{field} contains control characters");
    }
    Ok(normalized)
}

fn normalize_memo(value: String) -> anyhow::Result<String> {
    let normalized = value.trim().replace("\r\n", "\n").replace('\r', "\n");
    if normalized.len() > MAX_MEMO_BYTES {
        anyhow::bail!("memo exceeds {MAX_MEMO_BYTES} bytes");
    }
    if normalized
        .chars()
        .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
    {
        anyhow::bail!("memo contains unsupported control characters");
    }
    Ok(normalized)
}

fn sqlite_i64(value: u64, field: &str) -> anyhow::Result<i64> {
    i64::try_from(value).map_err(|_| anyhow::anyhow!("{field} exceeds SQLite integer range"))
}

fn sqlite_u64(value: i64, field: &str) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("stored {field} is negative"),
            )
            .into(),
        )
    })
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_store() -> AssetMetadataStore {
        AssetMetadataStore::from_connection(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn sample_update() -> AssetMetadataUpdate {
        AssetMetadataUpdate {
            tag: " Stablecoin ".into(),
            memo: " Treasury wallet\nverified by operator ".into(),
            color_label: "Blue".into(),
            pinned: true,
        }
    }

    #[test]
    fn upsert_lists_normalized_record() {
        let store = memory_store();
        let record = store
            .upsert("testnet", "ADDR", 42, sample_update())
            .unwrap();
        assert_eq!(record.asset_id, 42);
        assert_eq!(record.tag, "Stablecoin");
        assert_eq!(record.memo, "Treasury wallet\nverified by operator");
        assert_eq!(record.color_label, "blue");
        assert!(record.pinned);

        let records = store.list("testnet", "ADDR").unwrap();
        assert_eq!(records, vec![record]);
        assert!(store.list("mainnet", "ADDR").unwrap().is_empty());
    }

    #[test]
    fn empty_unpinned_update_deletes_record() {
        let store = memory_store();
        store
            .upsert("testnet", "ADDR", 42, sample_update())
            .unwrap();
        let record = store
            .upsert(
                "testnet",
                "ADDR",
                42,
                AssetMetadataUpdate {
                    tag: String::new(),
                    memo: String::new(),
                    color_label: String::new(),
                    pinned: false,
                },
            )
            .unwrap();
        assert_eq!(record, AssetUserMetadata::empty(42));
        assert!(store.list("testnet", "ADDR").unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_color_and_long_tag() {
        let store = memory_store();
        let invalid_color = store
            .upsert(
                "testnet",
                "ADDR",
                42,
                AssetMetadataUpdate {
                    color_label: "chartreuse".into(),
                    ..sample_update()
                },
            )
            .unwrap_err()
            .to_string();
        assert!(invalid_color.contains("unsupported color label"));

        let long_tag = store
            .upsert(
                "testnet",
                "ADDR",
                42,
                AssetMetadataUpdate {
                    tag: "x".repeat(MAX_TAG_BYTES + 1),
                    ..sample_update()
                },
            )
            .unwrap_err()
            .to_string();
        assert!(long_tag.contains("tag exceeds"));
    }

    #[test]
    fn file_backed_records_survive_reopen() {
        let path = std::env::temp_dir().join(format!(
            "opennodia-asset-metadata-{}-{}.sqlite",
            std::process::id(),
            unix_timestamp()
        ));
        {
            let store = AssetMetadataStore::open(&path).unwrap();
            store.upsert("testnet", "ADDR", 7, sample_update()).unwrap();
        }
        {
            let store = AssetMetadataStore::open(&path).unwrap();
            let records = store.list("testnet", "ADDR").unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].asset_id, 7);
            assert_eq!(records[0].tag, "Stablecoin");
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
    }
}
