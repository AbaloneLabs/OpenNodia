use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use opennodia_node::DataSource;
use serde::Serialize;

use crate::state::AppState;

use super::account_routes::fetch_account;

pub(super) fn event_routes() -> Router<AppState> {
    Router::new().route("/api/events", get(event_stream))
}

#[derive(Debug, Serialize)]
struct RealtimeNodeEvent {
    last_round: u64,
    last_version: String,
    time_since_last_round: u64,
    catchup_time: u64,
    source: DataSource,
    sync_progress: crate::sync::SyncProgress,
}

#[derive(Debug, Serialize)]
struct RealtimeWalletBalanceEvent {
    wallet_id: String,
    name: String,
    address: String,
    round: u64,
    algo_amount: u64,
    asset_count: usize,
}

#[derive(Debug, Serialize)]
struct RealtimeErrorEvent {
    scope: String,
    error: String,
}

/// `GET /api/events` — authenticated reconnectable SSE stream.
async fn event_stream(State(state): State<AppState>) -> impl IntoResponse {
    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut wallet_fingerprints: HashMap<String, String> = HashMap::new();

        loop {
            interval.tick().await;

            match state.ledger.algod.status_with_fallback(state.ledger.public_algod.as_ref()).await {
                Ok((status, source)) => {
                    if source == DataSource::Local {
                        state
                            .runtime.sync_tracker
                            .record(status.last_round.0, status.is_caught_up())
                            .await;
                    }
                    let sync_progress = state
                        .runtime.sync_tracker
                        .progress(status.last_round.0, status.is_caught_up())
                        .await;
                    let payload = RealtimeNodeEvent {
                        last_round: status.last_round.0,
                        last_version: status.last_version,
                        time_since_last_round: status.time_since_last_round,
                        catchup_time: status.catchup_time,
                        source,
                        sync_progress,
                    };
                    yield sse_json_event("node", &payload);
                }
                Err(error) => {
                    yield sse_json_event("error", &RealtimeErrorEvent {
                        scope: "node".to_string(),
                        error: error.to_string(),
                    });
                }
            }

            let indexer_progress = state.runtime.indexer_sync_tracker.progress().await;
            yield sse_json_event("indexer", &indexer_progress);

            for wallet in state.stores.wallets.list_wallets().await {
                let address = wallet.first_address.clone();
                match fetch_account(&state, &address).await {
                    Ok((info, _source)) => {
                        let fingerprint = wallet_balance_fingerprint(&info);
                        if wallet_fingerprints.get(&wallet.id) != Some(&fingerprint) {
                            wallet_fingerprints.insert(wallet.id.clone(), fingerprint);
                            let payload = RealtimeWalletBalanceEvent {
                                wallet_id: wallet.id,
                                name: wallet.name,
                                address,
                                round: info.round,
                                algo_amount: info.amount,
                                asset_count: info.assets.len(),
                            };
                            yield sse_json_event("wallet_balance", &payload);
                        }
                    }
                    Err(error) => {
                        yield sse_json_event("error", &RealtimeErrorEvent {
                            scope: format!("wallet:{}", wallet.id),
                            error: error.to_string(),
                        });
                    }
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}

fn sse_json_event<T: Serialize>(event: &'static str, payload: &T) -> Result<Event, Infallible> {
    let data = serde_json::to_string(payload).unwrap_or_else(|error| {
        serde_json::json!({
            "scope": event,
            "error": format!("event serialization failed: {error}")
        })
        .to_string()
    });
    Ok(Event::default().event(event).data(data))
}

fn wallet_balance_fingerprint(info: &opennodia_node::AccountInfo) -> String {
    let mut holdings = info
        .assets
        .iter()
        .map(|holding| {
            format!(
                "{}:{}:{}",
                holding.asset_id, holding.amount, holding.is_frozen
            )
        })
        .collect::<Vec<_>>();
    holdings.sort();
    format!("{}|{}", info.amount, holdings.join("|"))
}
