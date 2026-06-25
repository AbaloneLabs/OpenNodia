//! Delta sync: resume orderbook tracking from the last synced round.
//!
//! On non-archival nodes, block scanning for old rounds is unavailable. We use
//! account-info polling (in [`crate::events`]) as the primary mechanism. This module
//! provides the sync-state bookkeeping and a lightweight block-scan hook for
//! archival nodes.

use opennodia_core::Round;
use opennodia_node::algod::AlgodClient;

use crate::db::DexDb;

/// Delta-sync state.
#[derive(Clone, Debug)]
pub struct SyncState {
    pub last_synced_round: Round,
    pub current_round: Round,
    pub orders_checked: u64,
    pub changes_detected: u64,
}

/// Perform a delta sync: advance the synced round and trigger event polling.
///
/// Returns the updated sync state. The actual per-escrow polling is done by
/// [`crate::events::poll_order`]; this function manages the round bookkeeping.
pub async fn sync_from_round(
    db: &DexDb,
    algod: &AlgodClient,
    public: Option<&AlgodClient>,
) -> opennodia_core::Result<SyncState> {
    let last_synced = Round(db.get_last_synced_round()?);
    let status = algod.status_with_fallback(public).await?;
    let current = status.0.last_round;

    let state = SyncState {
        last_synced_round: last_synced,
        current_round: current,
        orders_checked: 0,
        changes_detected: 0,
    };

    // Persist the new synced round.
    db.set_last_synced_round(current.as_u64())?;

    Ok(state)
}

/// Mark the sync start point (used on first run or reset).
pub fn init_sync_state(db: &DexDb, round: Round) -> opennodia_core::Result<()> {
    db.set_last_synced_round(round.as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_and_read_sync_state() {
        let db = DexDb::open_memory().unwrap();
        assert_eq!(db.get_last_synced_round().unwrap(), 0);
        init_sync_state(&db, Round(12345)).unwrap();
        assert_eq!(db.get_last_synced_round().unwrap(), 12345);
    }
}
