use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use opennodia_core::Round;
use opennodia_dex::DexOrderbookSource;
use opennodia_swap::matching::{route_order, OrderRequest, RoutingDecision};
use opennodia_swap::verify_escrow;

use crate::routes::verify_pin;
use crate::routes::ApiError;
use crate::session::Session;
use crate::state::AppState;
use crate::tx_flow::TxDescription;

use super::{
    bad_request, confirmed_trade, describe_tx, fetch_params, internal, parse_address, parse_side,
    prepare_create_orders_from_plans, reject_escrow_regulated_assets, reject_regulated_asset,
    require_canonical_escrow, require_dex, require_dex_write, require_wallet_ownership,
    service_unavailable, store_intent, submit_prepared_create_order, take_intent, ApiResult,
    DexIntentAction, FillPreview, PrepareRouteRequest, PrepareRouteResponse, RouteFill,
    RouteFillResult, SubmitRouteRequest, SubmitRouteResponse,
};

/// `POST /api/dex/prepare/route` — immediate-or-cancel (IOC) routing preview.
///
/// Matches the incoming order against the live orderbook and returns a fill
/// plan. Unlike a standing order, the unmatched remainder is *never* issued —
/// it is discarded per IOC semantics. Each matched escrow is submitted as an
/// independent atomic group. If no match is found, the response reports
/// `decision = "no_match"` with no intent stored.
pub async fn prepare_route(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<PrepareRouteRequest>,
) -> ApiResult<Json<PrepareRouteResponse>> {
    require_dex_write(&state)?;
    let db = require_dex(&state)?;

    let filler = parse_address(&req.filler)?;
    let side = parse_side(&req.side)?;
    validate_route_request(&req)?;
    if req.sell_asset_id != 0 {
        reject_regulated_asset(&state, req.sell_asset_id).await?;
    }
    if req.buy_asset_id != 0 {
        reject_regulated_asset(&state, req.buy_asset_id).await?;
    }

    // Build the matching-engine request. IOC always sets immediate_fill so the
    // engine scans the book; the remainder is dropped below regardless.
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "dex prepare route");
    let tx_params = fetch_params(algod).await?;
    let current_round = tx_params.first_valid;

    let order_request = OrderRequest {
        side,
        sell_asset: req.sell_asset_id,
        buy_asset: req.buy_asset_id,
        sell_amount: req.sell_amount,
        buy_amount: req.buy_amount,
        split_count: req.split_count.max(1),
        immediate_fill: true,
        owner: filler,
        expire_round: Round(current_round.as_u64().saturating_add(req.expire_rounds)),
    };

    let book = DexOrderbookSource::new(&db);
    let decision = route_order(&order_request, &book, filler, &tx_params)
        .map_err(|e| bad_request(format!("route order: {e}")))?;

    // IOC keeps the historical behavior and discards any remainder. When
    // `place_remaining` is requested, the matching engine's remainder plans
    // are converted into standing child escrows after the immediate fills.
    let (fills, fill_results, stats, discarded_remaining, remaining_plans, decision_label) =
        match decision {
            RoutingDecision::FillOnly {
                fills,
                fill_results,
                stats,
            } => (fills, fill_results, stats, 0u64, Vec::new(), "filled"),
            RoutingDecision::FillAndIssue {
                fills,
                fill_results,
                stats,
                remaining_plans,
                remaining_amount,
                ..
            } => {
                if req.place_remaining {
                    (
                        fills,
                        fill_results,
                        stats,
                        0u64,
                        remaining_plans,
                        "filled_and_placed",
                    )
                } else {
                    (
                        fills,
                        fill_results,
                        stats,
                        remaining_amount,
                        Vec::new(),
                        "partial",
                    )
                }
            }
            RoutingDecision::IssueOnly { plans } => {
                if req.place_remaining {
                    (
                        Vec::new(),
                        Vec::new(),
                        opennodia_swap::matching::FillStats::default(),
                        0u64,
                        plans,
                        "placed",
                    )
                } else {
                    // No match against the book — IOC discards everything.
                    return Ok(Json(PrepareRouteResponse {
                        intent_id: String::new(),
                        decision: "no_match".to_string(),
                        fills: Vec::new(),
                        txs: Vec::new(),
                        average_price: 0,
                        total_cost: 0,
                        total_received: 0,
                        remaining: order_request.sell_amount,
                        new_orders_needed: 0,
                        created_orders: Vec::new(),
                    }));
                }
            }
        };

    let create_orders =
        prepare_create_orders_from_plans(algod, &tx_params, remaining_plans).await?;

    if fills.is_empty() && create_orders.is_empty() {
        return Ok(Json(PrepareRouteResponse {
            intent_id: String::new(),
            decision: "no_match".to_string(),
            fills: Vec::new(),
            txs: Vec::new(),
            average_price: 0,
            total_cost: 0,
            total_received: 0,
            remaining: order_request.sell_amount,
            new_orders_needed: 0,
            created_orders: Vec::new(),
        }));
    }

    // Bound sequential submissions to the matching engine's per-route limit.
    if fill_results.len() > opennodia_swap::matching::MAX_FILLS_PER_GROUP {
        return Err(bad_request(format!(
            "route matched {} escrows; IOC submit is limited to {} sequential fills — reduce the order size",
            fill_results.len(),
            opennodia_swap::matching::MAX_FILLS_PER_GROUP
        )));
    }

    // Reconstruct each matched escrow and pair it with its fill result.
    // Each fill is an independent atomic group (the escrow TEAL uses absolute
    // `gtxn 0/1/2` indices), so they are submitted sequentially in submit_route.
    let mut route_fills = Vec::with_capacity(fills.len());
    for (candidate, result) in fills.iter().zip(fill_results.iter()) {
        let escrow = candidate
            .order
            .to_escrow()
            .map_err(|e| internal(format!("reconstruct matched escrow: {e}")))?;
        require_canonical_escrow(algod, &escrow).await?;
        route_fills.push(RouteFill {
            escrow,
            result: result.clone(),
        });
    }

    let txs: Vec<TxDescription> = route_fills
        .iter()
        .flat_map(|route_fill| {
            let escrow_label = route_fill.escrow.address.to_string();
            std::iter::once(describe_tx(
                &route_fill.result.filler_tx,
                &filler.to_string(),
            ))
            .chain(
                route_fill
                    .result
                    .escrow_txs
                    .iter()
                    .map(move |transaction| describe_tx(transaction, &escrow_label)),
            )
        })
        .collect();

    let intent_id = store_intent(
        &state,
        &session,
        &req.wallet_id,
        DexIntentAction::Route {
            fills: route_fills,
            filler,
            remaining: discarded_remaining,
            creates: create_orders.clone(),
        },
    )
    .await?;

    let fill_previews: Vec<FillPreview> = fills
        .iter()
        .map(|f| FillPreview {
            escrow_address: f.order.escrow_address.to_string(),
            amount: f.fill_amount,
            price_micro: f.price_micro,
        })
        .collect();
    let create_txs: Vec<TxDescription> = create_orders
        .iter()
        .flat_map(|order| {
            let escrow_label = order.escrow.address.to_string();
            order
                .result
                .owner_txs
                .iter()
                .map(|tx| describe_tx(tx, &filler.to_string()))
                .chain(
                    order
                        .result
                        .logicsig_txs
                        .iter()
                        .map(move |tx| describe_tx(tx, &escrow_label)),
                )
        })
        .collect();
    let new_orders_needed = create_orders.len() as u32;
    let mut txs = txs;
    txs.extend(create_txs);

    Ok(Json(PrepareRouteResponse {
        intent_id,
        decision: decision_label.to_string(),
        fills: fill_previews,
        txs,
        average_price: stats.average_price,
        total_cost: stats.total_cost,
        total_received: stats.total_received,
        remaining: discarded_remaining,
        new_orders_needed,
        created_orders: Vec::new(),
    }))
}

/// `POST /api/dex/submit/route` — sign and submit a routed IOC fill.
///
/// Signs and relays the matched fill group(s) stored by `prepare_route`.
/// Each fill is an independent atomic group (the escrow TEAL uses absolute
/// `gtxn 0/1/2` indices), so for multi-fill routes the groups are submitted
/// sequentially. The unmatched remainder was already discarded during prepare,
/// so this only ever submits the on-chain fills (no standing order creation).
pub async fn submit_route(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<SubmitRouteRequest>,
) -> ApiResult<Json<SubmitRouteResponse>> {
    require_dex_write(&state)?;
    let db = require_dex(&state)?;
    let action = take_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let DexIntentAction::Route {
        fills,
        filler,
        remaining: prepare_remaining,
        creates,
    } = action
    else {
        return Err(bad_request("DEX intent is not a route intent"));
    };
    let pin = verify_pin(&state, &req.pin).await?;
    require_wallet_ownership(&state, &req.wallet_id, &pin, filler).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, fill_count = fills.len(), "dex submit route");

    let mut total_cost: u64 = 0;
    let mut total_received: u64 = 0;
    let mut last_txid = String::new();
    let mut filled_count = 0usize;
    let fill_total = fills.len();
    let mut tx_ids = Vec::new();
    let mut fill_outcomes = Vec::new();
    let mut failed_amount = 0u64;
    let mut created_orders = Vec::new();
    let prepared_create_amount = creates.iter().fold(0u64, |total, order| {
        total.saturating_add(order.escrow.params.sell_amount)
    });

    for (index, rf) in fills.iter().enumerate() {
        let cost = rf
            .result
            .filler_tx
            .amount
            .or(rf.result.filler_tx.asset_amount)
            .unwrap_or(0);
        let attempt = async {
            reject_escrow_regulated_assets(&state, &rf.escrow)
                .await
                .map_err(|(_, error)| error.0.error)?;
            let current = algod
                .status()
                .await
                .map_err(|error| format!("node status: {error}"))?;
            let verification = verify_escrow(algod, None, &rf.escrow, current.last_round)
                .await
                .map_err(|error| format!("verify escrow: {error}"))?;
            if !verification.valid {
                return Err(format!(
                    "escrow state changed since prepare: {}",
                    verification.mismatch_reason
                ));
            }
            let filler_signed = state
                .stores
                .wallets
                .sign_transaction(
                    &req.wallet_id,
                    &pin,
                    &filler.to_string(),
                    &opennodia_swap::encode_transaction(&rf.result.filler_tx),
                )
                .await
                .map_err(|error| format!("sign route filler transaction: {error}"))?;
            let mut group_bytes = filler_signed;
            for transaction in &rf.result.escrow_txs {
                group_bytes.extend_from_slice(&opennodia_swap::sign_with_logicsig(
                    transaction.clone(),
                    rf.escrow.program.clone(),
                ));
            }
            let tx_id = opennodia_swap::submit_signed_tx(algod, &group_bytes)
                .await
                .map_err(|error| format!("submit route group: {error}"))?;
            let confirmed_round = opennodia_swap::wait_for_confirmation(algod, &tx_id, 20)
                .await
                .map_err(|error| format!("route confirmation: {error}"))?;
            let trade = confirmed_trade(
                &rf.escrow,
                filler,
                tx_id.clone(),
                confirmed_round,
                super::unix_timestamp(),
            )
            .map_err(|(_, error)| error.0.error)?;
            let record_error = db
                .record_fill(&rf.escrow.address, rf.escrow.params.sell_amount, &trade)
                .err()
                .map(|error| format!("record routed fill: {error}"));
            Ok::<_, String>((tx_id, confirmed_round, record_error))
        }
        .await;

        match attempt {
            Ok((tx_id, confirmed_round, record_error)) => {
                total_cost = total_cost.saturating_add(cost);
                total_received = total_received.saturating_add(rf.escrow.params.sell_amount);
                last_txid = tx_id.clone();
                tx_ids.push(tx_id.clone());
                filled_count += 1;
                fill_outcomes.push(RouteFillResult {
                    escrow_address: rf.escrow.address.to_string(),
                    status: if record_error.is_some() {
                        "confirmed_unrecorded".to_string()
                    } else {
                        "recorded".to_string()
                    },
                    tx_id: Some(tx_id),
                    confirmed_round: Some(confirmed_round),
                    amount: rf.escrow.params.sell_amount,
                    cost,
                    error: record_error,
                });
            }
            Err(error) if filled_count == 0 => {
                return Err(internal(error));
            }
            Err(error) => {
                failed_amount = fills[index..].iter().fold(0u64, |total, pending| {
                    total.saturating_add(
                        pending
                            .result
                            .filler_tx
                            .amount
                            .or(pending.result.filler_tx.asset_amount)
                            .unwrap_or(0),
                    )
                });
                fill_outcomes.push(RouteFillResult {
                    escrow_address: rf.escrow.address.to_string(),
                    status: "failed".to_string(),
                    tx_id: None,
                    confirmed_round: None,
                    amount: rf.escrow.params.sell_amount,
                    cost,
                    error: Some(error),
                });
                for pending in fills.iter().skip(index + 1) {
                    fill_outcomes.push(RouteFillResult {
                        escrow_address: pending.escrow.address.to_string(),
                        status: "not_attempted".to_string(),
                        tx_id: None,
                        confirmed_round: None,
                        amount: pending.escrow.params.sell_amount,
                        cost: pending
                            .result
                            .filler_tx
                            .amount
                            .or(pending.result.filler_tx.asset_amount)
                            .unwrap_or(0),
                        error: None,
                    });
                }
                break;
            }
        }
    }

    if filled_count == fill_total {
        for create in creates {
            let (txid, _confirmed, order) = submit_prepared_create_order(
                &state,
                &db,
                algod,
                &req.wallet_id,
                &pin,
                filler,
                create,
            )
            .await?;
            last_txid = txid.clone();
            tx_ids.push(txid);
            created_orders.push(order);
        }
    }

    Ok(Json(SubmitRouteResponse {
        tx_id: last_txid,
        tx_ids,
        outcome: if filled_count == fill_total {
            if fill_total == 0 && !created_orders.is_empty() {
                "placed".to_string()
            } else if created_orders.is_empty() {
                "filled".to_string()
            } else {
                "filled_and_placed".to_string()
            }
        } else {
            "partial".to_string()
        },
        total_cost,
        total_received,
        failed_amount,
        // IOC-discarded remainder (sell-asset units): the unmatched quantity
        // computed at prepare time that never landed on-chain.
        remaining: prepare_remaining
            .saturating_add(failed_amount)
            .saturating_add(if filled_count == fill_total {
                0
            } else {
                prepared_create_amount
            }),
        fills: fill_outcomes,
        created_orders,
    }))
}

/// Validate an IOC route request. Mirrors the create/fill guards.
fn validate_route_request(req: &PrepareRouteRequest) -> Result<(), (StatusCode, Json<ApiError>)> {
    if req.sell_asset_id == req.buy_asset_id {
        return Err(bad_request("sell and buy assets must differ"));
    }
    if req.sell_amount == 0 || req.buy_amount == 0 {
        return Err(bad_request("sell_amount and buy_amount must be > 0"));
    }
    if !(3..=1_000_000).contains(&req.expire_rounds) {
        return Err(bad_request("expire_rounds must be between 3 and 1_000_000"));
    }
    if req.split_count == 0 || req.split_count > opennodia_swap::matching::MAX_SPLITS {
        return Err(bad_request(format!(
            "split_count must be between 1 and {}",
            opennodia_swap::matching::MAX_SPLITS
        )));
    }
    Ok(())
}
