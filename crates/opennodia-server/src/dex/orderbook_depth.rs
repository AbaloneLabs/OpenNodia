//! Synthetic orderbook depth built from executable route candidates.

use std::collections::HashSet;

use axum::extract::State;
use axum::Json;
use opennodia_dex::types::OrderBookSnapshot;

use crate::state::AppState;

use super::{
    api_error, assign_synthetic_totals, default_slippage_bps, mul_div_floor, route_candidates,
    RouteCandidateResponse, RouteCandidatesRequest, SyntheticPriceLevelResponse,
};

pub(super) async fn synthetic_orderbook_depth(
    state: &AppState,
    snapshot: &OrderBookSnapshot,
    view_base_asset: u64,
    depth: u32,
) -> (
    Vec<SyntheticPriceLevelResponse>,
    Vec<SyntheticPriceLevelResponse>,
    Vec<String>,
) {
    let view_quote_asset = snapshot.pair.other(view_base_asset);
    let sample_amounts = synthetic_sample_amounts(snapshot, view_base_asset, depth);
    let mut warnings = Vec::new();

    let (mut asks, ask_warnings) = synthetic_side_depth(
        state,
        view_base_asset,
        view_quote_asset,
        &sample_amounts,
        true,
        depth,
    )
    .await;
    warnings.extend(ask_warnings);

    let (mut bids, bid_warnings) = synthetic_side_depth(
        state,
        view_quote_asset,
        view_base_asset,
        &sample_amounts,
        false,
        depth,
    )
    .await;
    warnings.extend(bid_warnings);

    asks.sort_by(|left, right| {
        left.price
            .cmp(&right.price)
            .then_with(|| left.source.cmp(&right.source))
    });
    bids.sort_by(|left, right| {
        right
            .price
            .cmp(&left.price)
            .then_with(|| left.source.cmp(&right.source))
    });
    assign_synthetic_totals(&mut asks);
    assign_synthetic_totals(&mut bids);
    asks.truncate(depth as usize);
    bids.truncate(depth as usize);
    (bids, asks, warnings)
}

fn synthetic_sample_amounts(
    snapshot: &OrderBookSnapshot,
    view_base_asset: u64,
    depth: u32,
) -> Vec<u64> {
    let mut amounts: Vec<u64> = snapshot
        .asks
        .iter()
        .chain(snapshot.bids.iter())
        .filter_map(|level| (level.amount > 0).then_some(level.amount))
        .collect();
    if amounts.is_empty() {
        amounts.extend(if view_base_asset == 0 {
            [1_000_000, 10_000_000, 100_000_000]
        } else {
            [1, 10, 100]
        });
    }
    amounts.sort_unstable();
    amounts.dedup();
    amounts.truncate(depth.clamp(1, 4) as usize);
    amounts
}

async fn synthetic_side_depth(
    state: &AppState,
    asset_in: u64,
    asset_out: u64,
    sample_amounts: &[u64],
    ask_side: bool,
    depth: u32,
) -> (Vec<SyntheticPriceLevelResponse>, Vec<String>) {
    let mut levels = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();

    for amount_in in sample_amounts.iter().copied() {
        let response = route_candidates(
            State(state.clone()),
            Json(RouteCandidatesRequest {
                asset_in,
                asset_out,
                amount_in,
                slippage_bps: default_slippage_bps(),
                depth,
            }),
        )
        .await;
        let response = match response {
            Ok(Json(response)) => response,
            Err(error) => {
                warnings.push(format!(
                    "synthetic depth unavailable for {asset_in}->{asset_out} amount {amount_in}: {}",
                    api_error(&error)
                ));
                continue;
            }
        };
        warnings.extend(response.warnings);
        for candidate in response.candidates {
            if !candidate.virtual_orderbook
                || !candidate.executable
                || candidate.remaining_input != 0
            {
                continue;
            }
            let key = format!(
                "{}:{}:{}",
                candidate.source,
                candidate.pool_id.as_deref().unwrap_or(""),
                amount_in
            );
            if !seen.insert(key) {
                continue;
            }
            if let Some(level) = synthetic_level_from_candidate(candidate, ask_side) {
                levels.push(level);
            }
        }
    }

    (levels, warnings)
}

pub(super) fn synthetic_level_from_candidate(
    candidate: RouteCandidateResponse,
    ask_side: bool,
) -> Option<SyntheticPriceLevelResponse> {
    let (amount, price) = if ask_side {
        (
            candidate.input_consumed,
            mul_div_floor(candidate.amount_out, 1_000_000, candidate.input_consumed),
        )
    } else {
        (
            candidate.amount_out,
            mul_div_floor(candidate.input_consumed, 1_000_000, candidate.amount_out),
        )
    };
    if amount == 0 || price == 0 {
        return None;
    }
    Some(SyntheticPriceLevelResponse {
        price,
        amount,
        total: 0,
        source: candidate.source,
        source_label: candidate.source_label,
        pool_id: candidate.pool_id,
        app_id: candidate.app_id,
        fee_bps: candidate.fee_bps,
        fee_amount_estimate: candidate.fee_amount_estimate,
        price_impact_bps: candidate.price_impact_bps,
        executable: candidate.executable,
        source_round: candidate.source_round,
        note: candidate.note,
    })
}
