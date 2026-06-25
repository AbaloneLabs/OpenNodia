//! Background worker bootstrap.

use std::time::Duration;

use crate::state::AppState;

/// Start periodic background workers for sync, participation, wallet history,
/// and DEX reconciliation.
pub(crate) fn spawn_background_workers(state: &AppState) {
    if state.has_indexer() {
        let tracker = state.runtime.indexer_sync_tracker.clone();
        tokio::spawn(async move {
            tracing::info!("indexer sync poller started (10s interval)");
            loop {
                tracker.poll().await;
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
    }

    {
        let tracker = state.runtime.participation_tracker.clone();
        tokio::spawn(async move {
            tracker.run().await;
        });
    }

    if state.stores.wallet_history.is_some() {
        let history_state = state.clone();
        let interval = Duration::from_secs(state.config.wallet_history.sync_interval_secs.max(10));
        tokio::spawn(async move {
            tracing::info!(
                interval_secs = interval.as_secs(),
                "wallet history synchronizer started"
            );
            loop {
                history_state.sync_registered_wallet_histories().await;
                tokio::time::sleep(interval).await;
            }
        });
    }

    if state.stores.dex.is_some() {
        let dex_state = state.clone();
        let interval = Duration::from_secs(state.config.dex.reconcile_interval_secs.max(10));
        tokio::spawn(async move {
            tracing::info!(
                interval_secs = interval.as_secs(),
                "DEX reconciliation worker started"
            );
            loop {
                tokio::time::sleep(interval).await;
                if let Err(error) = crate::dex::reconcile_orders(&dex_state).await {
                    tracing::error!(%error, "periodic DEX reconciliation failed");
                }
            }
        });
    }
}
