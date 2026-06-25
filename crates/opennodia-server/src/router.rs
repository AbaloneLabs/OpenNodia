//! Unified orderbook and AMM routing API.
//!
//! The router compares live orderbook, native AMM, and external AMM quotes
//! using one exact-input model. Execution supports single-venue routes and
//! atomic native AMM split routes when every leg is contract-composable.

use std::time::Duration;

use crate::api_error::{
    bad_request, internal, not_found, service_unavailable, ApiErrorResponse, ApiResult,
};
use axum::routing::post;
use axum::Router as AxumRouter;

use crate::intent::IntentStoreError;
use crate::session::Session;
use crate::state::AppState;

mod dto;
mod evidence;
mod handlers;
mod quotes;
mod selection;

pub(crate) use dto::RouterIntentAction;
pub use dto::{
    RouterPrepareRequest, RouterPrepareResponse, RouterQuoteRequest, RouterQuoteResponse,
    RouterSubmitRequest, RouterSubmitResponse, UnifiedRouteLegQuote, UnifiedRouteQuote,
};

/// Build the unified router API. Mounted under session auth by `routes.rs`.
pub fn router_api() -> AxumRouter<AppState> {
    AxumRouter::new()
        .route("/api/router/quote", post(handlers::router_quote))
        .route("/api/router/prepare", post(handlers::router_prepare))
        .route("/api/router/submit", post(handlers::router_submit))
}

pub(super) async fn store_router_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    action: RouterIntentAction,
) -> ApiResult<String> {
    if !state.stores.wallets.contains_wallet(wallet_id).await {
        return Err(not_found(format!("wallet not found: {wallet_id}")));
    }
    let ttl = Duration::from_secs(state.config.dex.intent_ttl_secs.max(30));
    state
        .intents
        .router
        .store(&session.sid, wallet_id, ttl, action)
        .await
        .map_err(intent_error)
}

pub(super) async fn take_router_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent_id: &str,
) -> ApiResult<RouterIntentAction> {
    state
        .intents
        .router
        .take(&session.sid, wallet_id, intent_id)
        .await
        .map_err(intent_error)
}

fn intent_error(error: IntentStoreError) -> ApiErrorResponse {
    crate::api_error::intent_store_error(error, "router")
}

#[cfg(test)]
mod tests {
    use super::dto::{RouterQuoteRequest, UnifiedRouteLegQuote, UnifiedRouteQuote};
    use super::evidence::{quote_id, route_hash};
    use super::quotes::split_materially_beats_base;
    use super::selection::{amount_out_after_network_fee, compare_candidates, exclusion_reason};

    fn quote(source_type: &str, amount_out: u64, fee: u64) -> UnifiedRouteQuote {
        UnifiedRouteQuote {
            route_hash: String::new(),
            source_type: source_type.into(),
            source_id: source_type.into(),
            source_label: source_type.into(),
            execution: "test".into(),
            canonical_id: source_type.into(),
            pool_id: None,
            app_id: None,
            app_address: None,
            asset_in: 0,
            asset_out: 1,
            amount_in: 100,
            input_consumed: 100,
            remaining_input: 0,
            amount_out,
            minimum_out: amount_out,
            lp_fee_bps: 0,
            lp_fee_amount: 0,
            protocol_fee_bps: 0,
            protocol_fee_amount: 0,
            network_fee_microalgo: fee,
            price_impact_bps: 0,
            source_round: 1,
            expires_after_round: 21,
            executable: true,
            virtual_orderbook: false,
            split_legs: Vec::new(),
            selection_rank: None,
            selection_reason: None,
            note: String::new(),
        }
    }

    #[test]
    fn compare_prefers_final_output_then_lower_fee() {
        assert_eq!(
            compare_candidates(&quote("a", 101, 5_000), &quote("b", 100, 1_000)),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_candidates(&quote("a", 100, 1_000), &quote("b", 100, 5_000)),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn compare_accounts_for_network_fee_when_output_is_algo() {
        let mut higher_surface = quote("external_pool", 10_000, 4_000);
        higher_surface.asset_out = 0;
        let mut lower_surface = quote("orderbook", 9_000, 1_000);
        lower_surface.asset_out = 0;

        assert_eq!(amount_out_after_network_fee(&higher_surface), 6_000);
        assert_eq!(amount_out_after_network_fee(&lower_surface), 8_000);
        assert_eq!(
            compare_candidates(&lower_surface, &higher_surface),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn compare_keeps_surface_output_for_non_algo_assets() {
        let higher_surface = quote("external_pool", 10_000, 4_000);
        let lower_surface = quote("orderbook", 9_000, 1_000);

        assert_eq!(
            compare_candidates(&higher_surface, &lower_surface),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn route_hash_ignores_observation_round_only() {
        let first = quote("native_pool", 10_000, 3_000);
        let mut second = first.clone();
        second.source_round = first.source_round + 1;

        assert_eq!(
            route_hash("testnet", &first),
            route_hash("testnet", &second)
        );

        second.amount_out += 1;
        assert_ne!(
            route_hash("testnet", &first),
            route_hash("testnet", &second)
        );
    }

    #[test]
    fn quote_id_ignores_observation_round_only() {
        let req = RouterQuoteRequest {
            asset_in: 0,
            asset_out: 1,
            amount_in: 100,
            slippage_bps: 50,
            depth: 20,
            source: Some("best".into()),
        };
        let mut selected_quote = quote("orderbook", 10_000, 2_000);
        let selected = route_hash("testnet", &selected_quote);
        selected_quote.source_round += 1;
        let selected_next_round = route_hash("testnet", &selected_quote);

        assert_eq!(
            quote_id("testnet", &req, "best", &selected),
            quote_id("testnet", &req, "best", &selected_next_round)
        );

        let changed = route_hash("testnet", &quote("orderbook", 10_001, 2_000));
        assert_ne!(
            quote_id("testnet", &req, "best", &selected),
            quote_id("testnet", &req, "best", &changed)
        );
    }

    #[test]
    fn exclusion_reason_explains_ineligible_routes() {
        let mut quote_only = quote("native_pool", 100, 1_000);
        quote_only.executable = false;
        assert_eq!(
            exclusion_reason("best", &quote_only).unwrap(),
            "excluded because the source is quote-only or unsupported for swap submit"
        );

        let mut partial = quote("orderbook", 100, 1_000);
        partial.remaining_input = 7;
        partial.asset_in = 42;
        assert_eq!(
            exclusion_reason("best", &partial).unwrap(),
            "excluded because 7 units of input asset 42 would remain unfilled"
        );

        assert_eq!(
            exclusion_reason("external_pact", &quote("native_pool", 100, 1_000)).unwrap(),
            "excluded by source filter 'external_pact'"
        );
    }

    #[test]
    fn split_must_beat_extra_network_fee() {
        let base = quote("native_pool", 10_000, 3_000);
        assert!(!split_materially_beats_base(10_002, 6_000, &base));
        assert!(split_materially_beats_base(13_001, 6_000, &base));
    }

    #[test]
    fn route_hash_includes_split_legs() {
        let mut first = quote("split", 20_000, 6_000);
        first.source_type = "split".into();
        first.source_id = "native_split".into();
        first.split_legs.push(UnifiedRouteLegQuote {
            source_type: "native_pool".into(),
            source_id: "native_pool".into(),
            source_label: "OpenNodia AMM".into(),
            canonical_id: "native_pool:test:1".into(),
            pool_id: Some("pool-a".into()),
            app_id: Some(1),
            asset_in: 0,
            asset_out: 1,
            amount_in: 40,
            amount_out: 10_000,
            minimum_out: 9_950,
            lp_fee_bps: 30,
            lp_fee_amount: 12,
            network_fee_microalgo: 3_000,
            source_round: 7,
        });
        let mut second = first.clone();
        second.split_legs[0].amount_in = 41;
        assert_ne!(
            route_hash("testnet", &first),
            route_hash("testnet", &second)
        );
    }
}
