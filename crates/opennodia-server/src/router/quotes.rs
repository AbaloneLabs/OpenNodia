use std::collections::HashMap;

use axum::extract::State;
use axum::Json;

use crate::state::AppState;

use super::dto::{
    RouterQuoteRequest, RouterQuoteResponse, UnifiedRouteLegQuote, UnifiedRouteQuote,
};
use super::evidence::{quote_id, route_hash};
use super::selection::{compare_candidates, exclusion_reason, source_matches};
use super::{bad_request, ApiResult};

pub(super) async fn build_router_quote(
    state: &AppState,
    req: RouterQuoteRequest,
) -> ApiResult<RouterQuoteResponse> {
    if req.asset_in == req.asset_out {
        return Err(bad_request("asset_in and asset_out must differ"));
    }
    if req.amount_in == 0 {
        return Err(bad_request("amount_in must be greater than zero"));
    }

    let raw = crate::dex::route_candidates(
        State(state.clone()),
        Json(crate::dex::RouteCandidatesRequest {
            asset_in: req.asset_in,
            asset_out: req.asset_out,
            amount_in: req.amount_in,
            slippage_bps: req.slippage_bps,
            depth: req.depth,
        }),
    )
    .await?
    .0;

    let network = state.config.algod.network.to_string();
    let expires_after_round = raw.source_round.saturating_add(20);
    let mut candidates: Vec<UnifiedRouteQuote> = raw
        .candidates
        .into_iter()
        .map(|candidate| {
            normalize_candidate(
                &network,
                req.asset_in,
                req.asset_out,
                req.amount_in,
                expires_after_round,
                candidate,
            )
        })
        .collect();
    let split_candidates =
        native_split_candidates(state, &req, &network, expires_after_round, &candidates).await?;
    candidates.extend(split_candidates);

    let source_filter = req.source.as_deref().unwrap_or("best");
    let mut warnings = raw.warnings;
    let mut eligible: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.executable
                && candidate.remaining_input == 0
                && source_matches(source_filter, candidate)
        })
        .map(|(index, _)| index)
        .collect();

    eligible.sort_by(|left, right| compare_candidates(&candidates[*left], &candidates[*right]));
    let selected_index = eligible.first().copied();
    let second_index = eligible.get(1).copied();
    for (rank, index) in eligible.iter().enumerate() {
        let candidate = &mut candidates[*index];
        candidate.selection_rank = Some((rank + 1) as u32);
        candidate.selection_reason = Some(if rank == 0 {
            if source_filter == "best" || source_filter.trim().is_empty() {
                "best executable final output after LP/protocol fee quote; ALGO output accounts for network fee"
                    .into()
            } else {
                format!("best executable route matching pinned source '{source_filter}'")
            }
        } else {
            "lower final output than the selected route".into()
        });
    }
    for candidate in candidates.iter_mut() {
        if candidate.selection_reason.is_some() {
            continue;
        }
        candidate.selection_reason = exclusion_reason(source_filter, candidate);
    }
    if selected_index.is_none() {
        warnings.push(format!(
            "no executable single-venue route matched source '{source_filter}'"
        ));
    }

    let quote_id = quote_id(
        &network,
        &req,
        source_filter,
        candidates
            .get(selected_index.unwrap_or(0))
            .map(|candidate| candidate.route_hash.as_str())
            .unwrap_or("none"),
    );

    Ok(RouterQuoteResponse {
        quote_id,
        network,
        asset_in: req.asset_in,
        asset_out: req.asset_out,
        amount_in: req.amount_in,
        slippage_bps: req.slippage_bps,
        source_round: raw.source_round,
        expires_after_round,
        comparison_basis: "exact-input asset_out amount; ALGO output subtracts network fee for route selection while non-ALGO output reports network fee separately".into(),
        selected: selected_index.and_then(|index| candidates.get(index).cloned()),
        second_best: second_index.and_then(|index| candidates.get(index).cloned()),
        candidates,
        warnings,
    })
}

fn normalize_candidate(
    network: &str,
    asset_in: u64,
    asset_out: u64,
    amount_in: u64,
    expires_after_round: u64,
    candidate: crate::dex::RouteCandidateResponse,
) -> UnifiedRouteQuote {
    let (source_type, source_id) = match candidate.source.as_str() {
        "native_orderbook" => ("orderbook".to_string(), "orderbook".to_string()),
        "native_amm" => ("native_pool".to_string(), "native_pool".to_string()),
        source if source.starts_with("external_") => ("external_pool".to_string(), source.into()),
        other => ("external_router".to_string(), other.into()),
    };
    let canonical_id = match source_type.as_str() {
        "orderbook" => format!("orderbook:{network}:{asset_in}:{asset_out}"),
        "native_pool" => format!(
            "native_pool:{network}:{}",
            candidate
                .app_id
                .map(|id| id.to_string())
                .or_else(|| candidate.pool_id.clone())
                .unwrap_or_else(|| "unknown".into())
        ),
        "external_pool" => format!(
            "external_pool:{network}:{}:{}",
            source_id,
            candidate
                .pool_id
                .clone()
                .unwrap_or_else(|| "unknown".into())
        ),
        _ => format!("{}:{network}:{}", source_type, source_id),
    };
    let network_fee_microalgo = estimate_network_fee(&source_type, &candidate);
    let mut quote = UnifiedRouteQuote {
        route_hash: String::new(),
        source_type,
        source_id,
        source_label: candidate.source_label,
        execution: candidate.execution,
        canonical_id,
        pool_id: candidate.pool_id,
        app_id: candidate.app_id,
        app_address: candidate.app_address,
        asset_in,
        asset_out,
        amount_in,
        input_consumed: candidate.input_consumed,
        remaining_input: candidate.remaining_input,
        amount_out: candidate.amount_out,
        minimum_out: candidate.minimum_out,
        lp_fee_bps: candidate.fee_bps,
        lp_fee_amount: candidate.fee_amount_estimate,
        protocol_fee_bps: 0,
        protocol_fee_amount: 0,
        network_fee_microalgo,
        price_impact_bps: candidate.price_impact_bps,
        source_round: candidate.source_round,
        expires_after_round,
        executable: candidate.executable,
        virtual_orderbook: candidate.virtual_orderbook,
        split_legs: Vec::new(),
        selection_rank: None,
        selection_reason: None,
        note: candidate.note,
    };
    quote.route_hash = route_hash(network, &quote);
    quote
}

async fn native_split_candidates(
    state: &AppState,
    req: &RouterQuoteRequest,
    network: &str,
    expires_after_round: u64,
    base_candidates: &[UnifiedRouteQuote],
) -> ApiResult<Vec<UnifiedRouteQuote>> {
    if req.amount_in < 2 {
        return Ok(Vec::new());
    }
    let Some(base_best) = base_candidates
        .iter()
        .filter(|candidate| candidate.executable && candidate.remaining_input == 0)
        .max_by(|left, right| compare_candidates(left, right).reverse())
    else {
        return Ok(Vec::new());
    };

    let mut by_amount = HashMap::new();
    let mut amounts = Vec::new();
    for ratio_bps in [2_500u64, 5_000, 7_500] {
        let first = ((u128::from(req.amount_in) * u128::from(ratio_bps)) / 10_000u128)
            .clamp(1, u128::from(req.amount_in.saturating_sub(1))) as u64;
        let second = req.amount_in.saturating_sub(first);
        if first == 0 || second == 0 {
            continue;
        }
        amounts.push(first);
        amounts.push(second);
    }
    amounts.sort_unstable();
    amounts.dedup();

    for amount in amounts {
        let native = crate::lp::native_route_quote_candidates(
            state,
            req.asset_in,
            req.asset_out,
            amount,
            req.slippage_bps,
        )
        .await?;
        let candidates: Vec<_> = native
            .candidates
            .into_iter()
            .filter(|candidate| {
                candidate.pool.tradable
                    && candidate.pool.contract_version >= opennodia_amm::CONTRACT_VERSION_V3
            })
            .collect();
        by_amount.insert(amount, candidates);
    }

    let mut out = Vec::new();
    for ratio_bps in [2_500u64, 5_000, 7_500] {
        let first_amount = ((u128::from(req.amount_in) * u128::from(ratio_bps)) / 10_000u128)
            .clamp(1, u128::from(req.amount_in.saturating_sub(1)))
            as u64;
        let second_amount = req.amount_in.saturating_sub(first_amount);
        let (Some(first_candidates), Some(second_candidates)) =
            (by_amount.get(&first_amount), by_amount.get(&second_amount))
        else {
            continue;
        };

        for first in first_candidates {
            for second in second_candidates {
                if first.pool.app_id == second.pool.app_id {
                    continue;
                }
                let Some(amount_out) = first.quote.amount_out.checked_add(second.quote.amount_out)
                else {
                    continue;
                };
                let Some(minimum_out) = first
                    .quote
                    .minimum_out
                    .checked_add(second.quote.minimum_out)
                else {
                    continue;
                };
                let network_fee_microalgo = 6_000;
                if !split_materially_beats_base(amount_out, network_fee_microalgo, base_best) {
                    continue;
                }
                let Some(lp_fee_amount) = first
                    .quote
                    .fee_amount_estimate
                    .checked_add(second.quote.fee_amount_estimate)
                else {
                    continue;
                };
                let canonical_id = format!(
                    "native_split:{network}:{}:{}:{}:{}",
                    first.pool.app_id, first_amount, second.pool.app_id, second_amount
                );
                let split_legs = vec![
                    native_split_leg(network, req, first, first_amount),
                    native_split_leg(network, req, second, second_amount),
                ];
                let mut quote = UnifiedRouteQuote {
                    route_hash: String::new(),
                    source_type: "split".into(),
                    source_id: "native_split".into(),
                    source_label: "OpenNodia AMM split".into(),
                    execution: "atomic_native_split_swap".into(),
                    canonical_id,
                    pool_id: None,
                    app_id: None,
                    app_address: None,
                    asset_in: req.asset_in,
                    asset_out: req.asset_out,
                    amount_in: req.amount_in,
                    input_consumed: req.amount_in,
                    remaining_input: 0,
                    amount_out,
                    minimum_out,
                    lp_fee_bps: first.quote.fee_bps.max(second.quote.fee_bps),
                    lp_fee_amount,
                    protocol_fee_bps: 0,
                    protocol_fee_amount: 0,
                    network_fee_microalgo,
                    price_impact_bps: first
                        .quote
                        .price_impact_bps
                        .max(second.quote.price_impact_bps),
                    source_round: first.quote.source_round.max(second.quote.source_round),
                    expires_after_round,
                    executable: true,
                    virtual_orderbook: true,
                    split_legs,
                    selection_rank: None,
                    selection_reason: None,
                    note: "atomic split across composable native AMM v3 pools; no non-atomic fallback is used".into(),
                };
                quote.route_hash = route_hash(network, &quote);
                out.push(quote);
            }
        }
    }

    out.sort_by(compare_candidates);
    out.dedup_by(|left, right| left.route_hash == right.route_hash);
    Ok(out)
}

fn native_split_leg(
    network: &str,
    req: &RouterQuoteRequest,
    candidate: &crate::lp::NativeRouteQuoteCandidate,
    amount_in: u64,
) -> UnifiedRouteLegQuote {
    UnifiedRouteLegQuote {
        source_type: "native_pool".into(),
        source_id: "native_pool".into(),
        source_label: "OpenNodia AMM".into(),
        canonical_id: format!("native_pool:{network}:{}", candidate.pool.app_id),
        pool_id: Some(candidate.pool.pool_id.clone()),
        app_id: Some(candidate.pool.app_id),
        asset_in: req.asset_in,
        asset_out: req.asset_out,
        amount_in,
        amount_out: candidate.quote.amount_out,
        minimum_out: candidate.quote.minimum_out,
        lp_fee_bps: candidate.quote.fee_bps,
        lp_fee_amount: candidate.quote.fee_amount_estimate,
        network_fee_microalgo: 3_000,
        source_round: candidate.quote.source_round,
    }
}

pub(super) fn split_materially_beats_base(
    amount_out: u64,
    network_fee_microalgo: u64,
    base: &UnifiedRouteQuote,
) -> bool {
    let Some(improvement) = amount_out.checked_sub(base.amount_out) else {
        return false;
    };
    let fee_increase = network_fee_microalgo.saturating_sub(base.network_fee_microalgo);
    improvement > fee_increase
}

fn estimate_network_fee(source_type: &str, candidate: &crate::dex::RouteCandidateResponse) -> u64 {
    match source_type {
        "orderbook" => {
            let filled = if candidate.input_consumed > 0 { 1 } else { 0 };
            // Filler payment + escrow release for each IOC fill. Multi-fill
            // exact fee is shown at prepare time.
            2_000 * filled
        }
        "native_pool" => 3_000,
        "external_pool" => match candidate.source.as_str() {
            "external_tinyman" => 4_000,
            "external_pact" => 3_000,
            _ => 4_000,
        },
        _ => 0,
    }
}
