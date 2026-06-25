use crate::state::AppState;

pub async fn reconcile_orders(state: &AppState) -> anyhow::Result<opennodia_dex::SweepResult> {
    let _guard = state.runtime.dex_reconcile_lock.lock().await;
    let Some(db) = state.stores.dex.as_ref() else {
        anyhow::bail!("DEX orderbook database unavailable");
    };
    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(anyhow::Error::msg)?;
    let indexer = state
        .effective_search_client()
        .await
        .map(|(client, _)| client);
    let result =
        opennodia_dex::sweep_active_orders(db, algod, None, indexer, status.last_round).await?;
    tracing::info!(
        source = ?source,
        round = status.last_round.as_u64(),
        checked = result.checked,
        changes = result.total_changes(),
        errors = result.errors.len(),
        "DEX reconciliation sweep completed"
    );
    if !result.errors.is_empty() {
        anyhow::bail!(
            "DEX reconciliation completed with {} order errors",
            result.errors.len()
        );
    }
    Ok(result)
}
