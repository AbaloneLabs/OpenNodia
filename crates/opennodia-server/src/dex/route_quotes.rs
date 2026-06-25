use axum::extract::State;
use axum::Json;
use opennodia_amm::apply_slippage_floor;
use opennodia_core::{Address, Round};
use opennodia_dex::DexOrderbookSource;
use opennodia_swap::matching::{match_order, OrderRequest};
use opennodia_swap::OrderSide;

use crate::state::AppState;

use super::{
    api_error, bad_request, default_expire_rounds, mul_div_floor, require_dex, ApiResult,
    RouteCandidateResponse, RouteCandidatesRequest, RouteCandidatesResponse,
};

#[derive(Clone, Copy)]
pub(super) struct OrderbookRouteContext {
    pub(super) asset_in: u64,
    pub(super) asset_out: u64,
    pub(super) amount_in: u64,
    pub(super) slippage_bps: u16,
    pub(super) depth: u32,
    pub(super) source_round: u64,
    pub(super) writes_enabled: bool,
}

pub(super) fn orderbook_route_candidate(
    book: &DexOrderbookSource<'_>,
    side: OrderSide,
    context: OrderbookRouteContext,
) -> ApiResult<Option<RouteCandidateResponse>> {
    let request = OrderRequest {
        side,
        sell_asset: context.asset_in,
        buy_asset: context.asset_out,
        sell_amount: context.amount_in,
        buy_amount: 1,
        split_count: 1,
        immediate_fill: true,
        owner: Address::from_bytes([0u8; 32]),
        expire_round: Round(context.source_round.saturating_add(default_expire_rounds())),
    };
    let matched = match_order(&request, book);
    if matched.immediate_fills.is_empty() {
        return Ok(None);
    }

    let depth = context.depth.max(1) as usize;
    let fills: Vec<_> = matched.immediate_fills.iter().take(depth).collect();
    let input_consumed = fills.iter().fold(0u64, |total, fill| {
        total.saturating_add(fill.order.buy_amount)
    });
    let amount_out = fills
        .iter()
        .fold(0u64, |total, fill| total.saturating_add(fill.fill_amount));
    if amount_out == 0 || input_consumed == 0 {
        return Ok(None);
    }
    let minimum_out = apply_slippage_floor(amount_out, context.slippage_bps)
        .map_err(|error| bad_request(format!("orderbook slippage calculation failed: {error}")))?;
    let remaining_input = context.amount_in.saturating_sub(input_consumed);
    let best_price = fills.first().map(|fill| fill.price_micro).unwrap_or(0);
    let average_price = mul_div_floor(input_consumed, 1_000_000, amount_out);
    let price_impact_bps = orderbook_price_impact_bps(side, best_price, average_price);
    let side_label = match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    let note = if remaining_input == 0 {
        format!("native orderbook candidate from matching-engine {side_label} IOC against active escrows")
    } else {
        format!(
            "native orderbook partial {side_label} IOC candidate; {remaining_input} units of asset {} would remain unfilled",
            context.asset_in
        )
    };

    Ok(Some(RouteCandidateResponse {
        source: "native_orderbook".into(),
        source_label: "OpenNodia Orderbook".into(),
        execution: format!("native_orderbook_ioc_{side_label}"),
        pool_id: None,
        app_id: None,
        app_address: None,
        input_consumed,
        remaining_input,
        amount_out,
        minimum_out,
        fee_bps: 0,
        fee_amount_estimate: 0,
        price_impact_bps,
        source_round: context.source_round,
        executable: context.writes_enabled,
        virtual_orderbook: false,
        note,
    }))
}

fn orderbook_price_impact_bps(side: OrderSide, best_price: u64, average_price: u64) -> u64 {
    if best_price == 0 || average_price == 0 {
        return 0;
    }
    match side {
        OrderSide::Buy => {
            if average_price <= best_price {
                0
            } else {
                mul_div_floor(average_price - best_price, 10_000, best_price)
            }
        }
        OrderSide::Sell => {
            if average_price >= best_price {
                0
            } else {
                mul_div_floor(best_price - average_price, 10_000, best_price)
            }
        }
    }
}

pub(crate) async fn route_candidates(
    State(state): State<AppState>,
    Json(req): Json<RouteCandidatesRequest>,
) -> ApiResult<Json<RouteCandidatesResponse>> {
    if req.asset_in == req.asset_out {
        return Err(bad_request("asset_in and asset_out must differ"));
    }
    if req.amount_in == 0 {
        return Err(bad_request("amount_in must be greater than zero"));
    }

    let (_, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(super::service_unavailable)?;
    let mut source_round = status.last_round.as_u64();
    let mut warnings = Vec::new();
    let mut candidates = Vec::new();

    match require_dex(&state) {
        Ok(db) => {
            let book = DexOrderbookSource::new(&db);
            let context = OrderbookRouteContext {
                asset_in: req.asset_in,
                asset_out: req.asset_out,
                amount_in: req.amount_in,
                slippage_bps: req.slippage_bps,
                depth: req.depth,
                source_round: status.last_round.as_u64(),
                writes_enabled: state.runtime.dex_validation.snapshot().allows_writes(),
            };
            for side in [OrderSide::Buy, OrderSide::Sell] {
                if let Some(candidate) = orderbook_route_candidate(&book, side, context)? {
                    candidates.push(candidate);
                }
            }
        }
        Err(error) => warnings.push(format!(
            "native orderbook unavailable: {}",
            api_error(&error)
        )),
    }

    match crate::lp::native_route_quote_candidates(
        &state,
        req.asset_in,
        req.asset_out,
        req.amount_in,
        req.slippage_bps,
    )
    .await
    {
        Ok(native) => {
            source_round = source_round.max(native.source_round);
            if native.source != source {
                warnings.push(format!(
                    "native AMM source {:?} differs from route ledger source {:?}",
                    native.source, source
                ));
            }
            warnings.extend(native.warnings);
            candidates.extend(native.candidates.into_iter().map(|candidate| {
                RouteCandidateResponse {
                    source: "native_amm".into(),
                    source_label: "OpenNodia AMM".into(),
                    execution: "native_amm_swap".into(),
                    pool_id: Some(candidate.pool.pool_id),
                    app_id: Some(candidate.pool.app_id),
                    app_address: Some(candidate.pool.app_address),
                    input_consumed: candidate.quote.amount_in,
                    remaining_input: 0,
                    amount_out: candidate.quote.amount_out,
                    minimum_out: candidate.quote.minimum_out,
                    fee_bps: candidate.quote.fee_bps,
                    fee_amount_estimate: candidate.quote.fee_amount_estimate,
                    price_impact_bps: candidate.quote.price_impact_bps,
                    source_round: candidate.quote.source_round,
                    executable: candidate.pool.tradable,
                    virtual_orderbook: true,
                    note: "native AMM quote; not a real orderbook level".into(),
                }
            }));
        }
        Err(error) => warnings.push(format!("native AMM unavailable: {}", api_error(&error))),
    }

    match crate::external_liquidity::external_route_quote_candidates(
        &state,
        req.asset_in,
        req.asset_out,
        req.amount_in,
        req.slippage_bps,
    )
    .await
    {
        Ok(external) => {
            source_round = source_round.max(external.source_round);
            if external.source != source {
                warnings.push(format!(
                    "external AMM source {:?} differs from route ledger source {:?}",
                    external.source, source
                ));
            }
            warnings.extend(external.warnings);
            candidates.extend(external.candidates.into_iter().map(|candidate| {
                let source = candidate.pool.source.clone();
                let note = if candidate.pool.folks_backed {
                    "Folks-backed Pact pool quote-only candidate; liquidity is still executed through the underlying Pact pool and is not counted as an additional AMM source"
                        .into()
                } else if candidate.pool.swap_supported {
                    "external AMM quote; submit path revalidates pool state before signing"
                        .into()
                } else {
                    "external AMM quote-only candidate; not executable from OpenNodia".into()
                };
                RouteCandidateResponse {
                    source: format!("external_{source}"),
                    source_label: candidate.pool.source.clone(),
                    execution: "external_amm_swap".into(),
                    pool_id: Some(candidate.pool.pool_id),
                    app_id: Some(candidate.pool.app_id),
                    app_address: Some(candidate.pool.app_address),
                    input_consumed: candidate.quote.amount_in,
                    remaining_input: 0,
                    amount_out: candidate.quote.amount_out,
                    minimum_out: candidate.quote.minimum_out,
                    fee_bps: candidate.quote.fee_bps,
                    fee_amount_estimate: candidate.quote.fee_amount_estimate,
                    price_impact_bps: candidate.quote.price_impact_bps,
                    source_round: candidate.quote.source_round,
                    executable: candidate.pool.swap_supported,
                    virtual_orderbook: true,
                    note,
                }
            }));
        }
        Err(error) => warnings.push(format!("external AMM unavailable: {}", api_error(&error))),
    }

    candidates.sort_by(|left, right| {
        right
            .amount_out
            .cmp(&left.amount_out)
            .then_with(|| left.source.cmp(&right.source))
    });

    Ok(Json(RouteCandidatesResponse {
        asset_in: req.asset_in,
        asset_out: req.asset_out,
        amount_in: req.amount_in,
        candidates,
        warnings,
        source_round,
        source,
    }))
}
