use std::collections::HashSet;

use opennodia_amm::quote_exact_in;
use opennodia_node::ApplicationInfo;

use crate::state::AppState;

use super::{
    api_error, bad_request, compile_native_pool_programs, decode_application_pool,
    ensure_pool_is_tradable, genesis_hash, pool_response, read_pool, record_pool_state,
    service_unavailable, ApiResult, NativeRouteQuoteCandidate, NativeRouteQuoteCandidates,
};

pub(crate) async fn native_route_quote_candidates(
    state: &AppState,
    asset_in: u64,
    asset_out: u64,
    amount_in: u64,
    slippage_bps: u16,
) -> ApiResult<NativeRouteQuoteCandidates> {
    if asset_in == asset_out {
        return Err(bad_request("asset_in and asset_out must differ"));
    }
    if amount_in == 0 {
        return Err(bad_request("amount_in must be greater than zero"));
    }

    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let round = status.last_round.as_u64();
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let asset_a = asset_in.min(asset_out);
    let asset_b = asset_in.max(asset_out);
    let mut warnings = Vec::new();
    let mut pool_states = Vec::new();

    let registry_entries = {
        let registry = state.stores.lp_registry.lock().await;
        registry.entries_for_pair(genesis_hash, asset_a, asset_b)
    };
    let mut seen_app_ids = HashSet::new();
    for entry in registry_entries {
        match read_pool(algod, entry.app_id, genesis_hash, round, &expected_programs).await {
            Ok(pool) => {
                if pool.key.contains(asset_a) && pool.key.contains(asset_b) {
                    seen_app_ids.insert(pool.app_id);
                    pool_states.push(pool);
                }
            }
            Err(error) => warnings.push(format!(
                "native pool {} unavailable: {}",
                entry.app_id,
                api_error(&error)
            )),
        }
    }

    if let Some(indexer) = state
        .ledger
        .indexer
        .as_ref()
        .or(state.ledger.public_indexer.as_ref())
    {
        let discover_asset = match (asset_a, asset_b) {
            (0, 0) => unreachable!("equal assets rejected"),
            (0, other) | (other, 0) => other,
            (left, right) => left.min(right),
        };

        match indexer.applications_by_asset(discover_asset).await {
            Ok(applications) => {
                for app in applications {
                    if seen_app_ids.contains(&app.id) {
                        continue;
                    }
                    let mut app_info = ApplicationInfo {
                        id: app.id,
                        params: opennodia_node::ApplicationParams {
                            creator: app.params.creator,
                            approval_program: app.params.approval_program,
                            clear_state_program: app.params.clear_state_program,
                            global_state: app.params.global_state,
                            global_state_schema: None,
                            local_state_schema: None,
                            extra_program_pages: 0,
                        },
                    };
                    if app_info.params.global_state.is_empty()
                        || app_info.params.approval_program.is_empty()
                        || app_info.params.clear_state_program.is_empty()
                    {
                        app_info = match algod.application_info(app_info.id).await {
                            Ok(info) => info,
                            Err(error) => {
                                tracing::debug!(
                                    app_id = app_info.id,
                                    %error,
                                    "skipping native AMM route candidate that could not be loaded from algod"
                                );
                                continue;
                            }
                        };
                    }
                    let Ok(pool) =
                        decode_application_pool(&app_info, genesis_hash, round, &expected_programs)
                    else {
                        continue;
                    };
                    if pool.key.contains(asset_a) && pool.key.contains(asset_b) {
                        seen_app_ids.insert(pool.app_id);
                        record_pool_state(state, &pool).await;
                        pool_states.push(pool);
                    }
                }
            }
            Err(error) => warnings.push(format!("native pool discovery failed: {error}")),
        }
    } else {
        warnings
            .push("native pool indexer discovery unavailable; using local registry only".into());
    }

    let mut candidates = Vec::new();
    for pool in pool_states {
        if let Err(error) = ensure_pool_is_tradable(&pool, "route quote") {
            warnings.push(format!(
                "native pool {} skipped: {}",
                pool.app_id,
                api_error(&error)
            ));
            continue;
        }
        match quote_exact_in(&pool, asset_in, amount_in, slippage_bps) {
            Ok(quote) if quote.asset_out == asset_out => {
                candidates.push(NativeRouteQuoteCandidate {
                    pool: pool_response(&pool),
                    quote,
                });
            }
            Ok(_) => warnings.push(format!(
                "native pool {} returned an unexpected output asset",
                pool.app_id
            )),
            Err(error) => {
                warnings.push(format!("native pool {} quote failed: {error}", pool.app_id))
            }
        }
    }

    Ok(NativeRouteQuoteCandidates {
        candidates,
        source,
        source_round: round,
        warnings,
    })
}
