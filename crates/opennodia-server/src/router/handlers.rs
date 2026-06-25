use axum::extract::{Extension, State};
use axum::Json;
use opennodia_core::Address;
use serde_json::{json, Value};

use crate::session::Session;
use crate::state::AppState;
use crate::tx_flow::{TxDescription, WalletTxGroup};

use super::dto::{
    PreparedNativeSplitLeg, RouterIntentAction, RouterPrepareRequest, RouterPrepareResponse,
    RouterQuoteRequest, RouterQuoteResponse, RouterSubmitDelegate, RouterSubmitRequest,
    RouterSubmitResponse, UnifiedRouteQuote,
};
use super::quotes::build_router_quote;
use super::{
    bad_request, internal, service_unavailable, store_router_intent, take_router_intent, ApiResult,
};

pub(super) async fn router_quote(
    State(state): State<AppState>,
    Json(req): Json<RouterQuoteRequest>,
) -> ApiResult<Json<RouterQuoteResponse>> {
    Ok(Json(build_router_quote(&state, req).await?))
}

pub(super) async fn router_prepare(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<RouterPrepareRequest>,
) -> ApiResult<Json<RouterPrepareResponse>> {
    let quote = build_router_quote(&state, req.quote.clone()).await?;
    if quote.quote_id != req.quote_id {
        return Err(bad_request(
            "quote_id no longer matches live route state; refresh the quote",
        ));
    }
    let selected = quote
        .candidates
        .iter()
        .find(|candidate| candidate.route_hash == req.route_hash)
        .cloned()
        .ok_or_else(|| bad_request("route_hash is not present in the current quote"))?;
    if !selected.executable || selected.remaining_input != 0 {
        return Err(bad_request(
            "selected route is not currently executable as a single venue",
        ));
    }

    let (delegate, tx_hash, txs, preview) = match selected.source_type.as_str() {
        "orderbook" => {
            let side = match selected.execution.as_str() {
                "native_orderbook_ioc_buy" => "buy",
                _ => "sell",
            };
            let resp = crate::dex::prepare_route(
                State(state.clone()),
                Extension(session.clone()),
                Json(crate::dex::PrepareRouteRequest {
                    wallet_id: req.wallet_id.clone(),
                    filler: req.trader.clone(),
                    side: side.into(),
                    sell_asset_id: selected.asset_in,
                    sell_amount: selected.amount_in,
                    buy_asset_id: selected.asset_out,
                    buy_amount: selected.minimum_out.max(1),
                    split_count: 1,
                    immediate_fill: true,
                    place_remaining: false,
                    expire_rounds: req.expire_rounds,
                }),
            )
            .await?
            .0;
            if resp.intent_id.is_empty() {
                return Err(bad_request(
                    "selected orderbook route no longer has an executable IOC fill",
                ));
            }
            let preview = serde_json::to_value(&resp)
                .map_err(|error| internal(format!("serialize orderbook preview: {error}")))?;
            (
                RouterSubmitDelegate::Orderbook {
                    intent_id: resp.intent_id,
                },
                None,
                resp.txs,
                preview,
            )
        }
        "native_pool" => {
            let app_id = selected
                .app_id
                .ok_or_else(|| bad_request("native route is missing app_id"))?;
            let fields = crate::lp::PoolSwapFields {
                trader: req.trader.clone(),
                app_id,
                asset_in: selected.asset_in,
                amount_in: selected.amount_in,
                slippage_bps: req.quote.slippage_bps,
                expire_rounds: req.expire_rounds,
            };
            let resp = crate::lp::prepare_pool_swap(
                Extension(session.clone()),
                State(state.clone()),
                Json(crate::lp::PoolSwapPrepareRequest {
                    wallet_id: req.wallet_id.clone(),
                    fields: fields.clone(),
                }),
            )
            .await?
            .0;
            let preview = serde_json::to_value(&resp.preview)
                .map_err(|error| internal(format!("serialize native preview: {error}")))?;
            (
                RouterSubmitDelegate::NativePool {
                    intent_id: resp.intent_id,
                    fields,
                },
                Some(resp.tx_hash),
                resp.txs,
                preview,
            )
        }
        "external_pool" => {
            let source = selected
                .source_id
                .strip_prefix("external_")
                .unwrap_or(&selected.source_id)
                .to_string();
            let pool_id = selected
                .pool_id
                .clone()
                .ok_or_else(|| bad_request("external route is missing pool_id"))?;
            let fields = crate::external_liquidity::ExternalSwapFields {
                source,
                pool_id,
                trader: req.trader.clone(),
                asset_in: selected.asset_in,
                amount_in: selected.amount_in,
                slippage_bps: req.quote.slippage_bps,
                expire_rounds: req.expire_rounds,
            };
            let resp = crate::external_liquidity::prepare_external_swap(
                Extension(session.clone()),
                State(state.clone()),
                Json(crate::external_liquidity::ExternalSwapPrepareRequest {
                    wallet_id: req.wallet_id.clone(),
                    fields: fields.clone(),
                }),
            )
            .await?
            .0;
            let preview = serde_json::to_value(&resp.preview)
                .map_err(|error| internal(format!("serialize external preview: {error}")))?;
            (
                RouterSubmitDelegate::ExternalPool {
                    intent_id: resp.intent_id,
                    fields,
                },
                Some(resp.tx_hash),
                resp.txs,
                preview,
            )
        }
        "split" => prepare_native_split(&state, &req, &selected).await?,
        _ => return Err(bad_request("unsupported route source type")),
    };

    let intent_id = store_router_intent(
        &state,
        &session,
        &req.wallet_id,
        RouterIntentAction::Delegated {
            source_type: selected.source_type.clone(),
            source_id: selected.source_id.clone(),
            quote_id: req.quote_id.clone(),
            route_hash: req.route_hash.clone(),
            submit: delegate,
        },
    )
    .await?;

    Ok(Json(RouterPrepareResponse {
        intent_id,
        quote_id: req.quote_id,
        route_hash: req.route_hash,
        source_type: selected.source_type.clone(),
        source_id: selected.source_id.clone(),
        tx_hash,
        txs,
        preview,
        selected,
    }))
}

pub(super) async fn router_submit(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<RouterSubmitRequest>,
) -> ApiResult<Json<RouterSubmitResponse>> {
    let intent = take_router_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let RouterIntentAction::Delegated {
        source_type,
        source_id,
        quote_id,
        route_hash,
        submit,
    } = intent;
    if quote_id != req.quote_id || route_hash != req.route_hash {
        return Err(bad_request(
            "router intent does not match submitted quote_id and route_hash",
        ));
    }

    let (tx_id, tx_ids, confirmed_round, outcome, result) = match submit {
        RouterSubmitDelegate::Orderbook { intent_id } => {
            let resp = crate::dex::submit_route(
                State(state.clone()),
                Extension(session.clone()),
                Json(crate::dex::SubmitRouteRequest {
                    wallet_id: req.wallet_id.clone(),
                    pin: req.pin.clone(),
                    intent_id,
                }),
            )
            .await?
            .0;
            let tx_ids = resp.tx_ids.clone();
            let tx_id = if resp.tx_id.is_empty() {
                None
            } else {
                Some(resp.tx_id.clone())
            };
            let confirmed_round = resp
                .fills
                .iter()
                .filter_map(|fill| fill.confirmed_round)
                .max();
            let result = serde_json::to_value(&resp)
                .map_err(|error| internal(format!("serialize orderbook submit: {error}")))?;
            (tx_id, tx_ids, confirmed_round, resp.outcome, result)
        }
        RouterSubmitDelegate::NativePool { intent_id, fields } => {
            let resp = crate::lp::swap_pool_exact_in(
                Extension(session.clone()),
                State(state.clone()),
                Json(crate::lp::PoolSwapSubmitRequest {
                    wallet_id: req.wallet_id.clone(),
                    pin: req.pin.clone(),
                    intent_id,
                    fields,
                }),
            )
            .await?
            .0;
            let result = serde_json::to_value(&resp)
                .map_err(|error| internal(format!("serialize native submit: {error}")))?;
            (
                Some(resp.txid.clone()),
                vec![resp.txid],
                Some(resp.confirmed_round),
                "filled".into(),
                result,
            )
        }
        RouterSubmitDelegate::ExternalPool { intent_id, fields } => {
            let resp = crate::external_liquidity::submit_external_swap(
                Extension(session.clone()),
                State(state.clone()),
                Json(crate::external_liquidity::ExternalSwapSubmitRequest {
                    wallet_id: req.wallet_id.clone(),
                    pin: req.pin.clone(),
                    intent_id,
                    fields,
                }),
            )
            .await?
            .0;
            let result = serde_json::to_value(&resp)
                .map_err(|error| internal(format!("serialize external submit: {error}")))?;
            (
                Some(resp.txid.clone()),
                vec![resp.txid],
                Some(resp.confirmed_round),
                "filled".into(),
                result,
            )
        }
        RouterSubmitDelegate::NativeSplit {
            group,
            tx_hash,
            legs,
        } => {
            if group.tx_hash() != tx_hash {
                return Err(bad_request(
                    "prepared native split group hash does not match the stored preview hash",
                ));
            }
            let (algod, source) = state
                .effective_write_client()
                .await
                .map_err(service_unavailable)?;
            tracing::debug!(source = ?source, legs = legs.len(), "router submit native split");
            for leg in &legs {
                let current = crate::lp::read_current_pool(algod, leg.app_id).await?;
                if !crate::lp::pool_execution_state_matches(&current, &leg.pool_before) {
                    return Err(bad_request(format!(
                        "native split pool {} changed since prepare; refresh the quote",
                        leg.app_id
                    )));
                }
            }
            let confirmed = group
                .sign_submit_and_confirm(
                    &state,
                    algod,
                    &req.wallet_id,
                    &req.pin,
                    20,
                    "router native split",
                )
                .await?;
            let result = json!({
                "txid": confirmed.txid,
                "confirmed_round": confirmed.confirmed_round,
                "atomic_group_size": group.txs().len(),
                "tx_hash": tx_hash,
                "legs": legs.iter().map(|leg| {
                    json!({
                        "app_id": leg.app_id,
                        "pool_id": leg.pool_id,
                        "amount_in": leg.quote.amount_in,
                        "amount_out": leg.quote.amount_out,
                        "minimum_out": leg.quote.minimum_out,
                        "fee_bps": leg.quote.fee_bps,
                        "source_round": leg.quote.source_round,
                    })
                }).collect::<Vec<_>>(),
            });
            (
                Some(confirmed.txid.clone()),
                vec![confirmed.txid],
                Some(confirmed.confirmed_round),
                "filled".into(),
                result,
            )
        }
    };

    Ok(Json(RouterSubmitResponse {
        quote_id,
        route_hash,
        source_type,
        source_id,
        tx_id,
        tx_ids,
        confirmed_round,
        outcome,
        result,
    }))
}

async fn prepare_native_split(
    state: &AppState,
    req: &RouterPrepareRequest,
    selected: &UnifiedRouteQuote,
) -> ApiResult<(
    RouterSubmitDelegate,
    Option<String>,
    Vec<TxDescription>,
    Value,
)> {
    if selected.split_legs.len() < 2 {
        return Err(bad_request("split route must contain at least two legs"));
    }
    let trader = req
        .trader
        .parse::<Address>()
        .map_err(|error| bad_request(format!("invalid trader address: {error}")))?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, legs = selected.split_legs.len(), "router prepare native split");

    let mut txs = Vec::new();
    let mut prepared_legs = Vec::new();
    let mut total_minimum_out = 0u64;
    let mut total_amount_out = 0u64;
    let mut earliest_deadline = u64::MAX;

    for leg in &selected.split_legs {
        if leg.source_type != "native_pool" {
            return Err(bad_request("native split route contains a non-native leg"));
        }
        let app_id = leg
            .app_id
            .ok_or_else(|| bad_request("native split leg is missing app_id"))?;
        let fields = crate::lp::PoolSwapFields {
            trader: req.trader.clone(),
            app_id,
            asset_in: leg.asset_in,
            amount_in: leg.amount_in,
            slippage_bps: req.quote.slippage_bps,
            expire_rounds: req.expire_rounds,
        };
        let (pool, quote, draft, draft_trader, deadline) =
            crate::lp::pool_swap_draft(algod, &fields).await?;
        if draft_trader != trader {
            return Err(internal("native split draft returned a different trader"));
        }
        if pool.key.contract_version < opennodia_amm::CONTRACT_VERSION_V3 {
            return Err(bad_request(format!(
                "native split requires composable AMM contract v3; pool {} is v{}",
                pool.app_id, pool.key.contract_version
            )));
        }
        if quote.asset_out != selected.asset_out
            || quote.amount_out != leg.amount_out
            || quote.minimum_out != leg.minimum_out
        {
            return Err(bad_request(
                "native split leg quote changed while preparing; refresh the quote",
            ));
        }
        for tx in &draft.txs {
            if tx.sender != trader {
                return Err(internal(
                    "native split can only compose transactions signed by the trader",
                ));
            }
        }
        total_minimum_out = total_minimum_out
            .checked_add(quote.minimum_out)
            .ok_or_else(|| bad_request("native split minimum output overflow"))?;
        total_amount_out = total_amount_out
            .checked_add(quote.amount_out)
            .ok_or_else(|| bad_request("native split output overflow"))?;
        earliest_deadline = earliest_deadline.min(deadline.as_u64());
        txs.extend(draft.txs);
        prepared_legs.push(PreparedNativeSplitLeg {
            app_id,
            pool_id: pool.key.id(),
            pool_before: pool,
            quote,
        });
    }

    if txs.len() > 16 {
        return Err(bad_request(format!(
            "native split route requires {} transactions, exceeding Algorand atomic group limit 16",
            txs.len()
        )));
    }
    if total_minimum_out != selected.minimum_out || total_amount_out != selected.amount_out {
        return Err(bad_request(
            "native split totals changed while preparing; refresh the quote",
        ));
    }

    opennodia_swap::assign_group_id(&mut txs);
    let group = WalletTxGroup::new(trader, txs)?;
    let tx_hash = group.tx_hash().to_string();
    let txs = group.descriptions();
    let preview = json!({
        "execution": selected.execution,
        "tx_hash": tx_hash,
        "atomic_group_size": txs.len(),
        "total_amount_in": selected.amount_in,
        "total_amount_out": selected.amount_out,
        "total_minimum_out": selected.minimum_out,
        "deadline_round": earliest_deadline,
        "network_fee_microalgo": selected.network_fee_microalgo,
        "legs": selected.split_legs,
        "note": "The router signs one atomic group. If any leg fails, no leg is submitted as a fallback."
    });

    Ok((
        RouterSubmitDelegate::NativeSplit {
            group,
            tx_hash: tx_hash.clone(),
            legs: prepared_legs,
        },
        Some(tx_hash),
        txs,
        preview,
    ))
}
