//! HTTP API routes for OpenNodia.
//!
//! Public (no auth):
//! - `GET  /api/status` — auth setup status + node reachability
//! - `POST /api/setup` — first-time PIN setup (PIN only, no mnemonic)
//! - `POST /api/login` — PIN login, returns session token
//!
//! Protected (session token required):
//! - `POST /api/logout` — revoke session
//! - `POST /api/change-pin` — change PIN (requires current PIN)
//! - `GET  /api/node/status` — algod node status
//! - `GET  /api/accounts/:addr` — account information
//! - `GET  /api/wallets` — list registered wallets
//! - `POST /api/wallets/create` — create a new kmd wallet
//! - `POST /api/wallets/import` — import a wallet from mnemonic
//! - `POST /api/wallets/activate` — set active wallet
//! - `DELETE /api/wallets/:id` — remove a wallet from registry
//! - `GET  /api/wallets/active` — get active wallet

use axum::routing::{get, post};
use axum::Router;

pub use crate::api_error::ApiError;
use crate::middleware::require_auth;
use crate::state::AppState;

#[path = "routes/account.rs"]
mod account_routes;
#[path = "routes/analytics.rs"]
mod analytics_routes;
#[path = "routes/auth.rs"]
mod auth_routes;
pub(crate) use auth_routes::verify_pin;
#[path = "routes/events.rs"]
mod event_routes;
#[path = "routes/history.rs"]
mod history_routes;
#[path = "routes/market.rs"]
mod market_routes;
#[path = "routes/node.rs"]
mod node_routes;
#[path = "routes/wallets.rs"]
mod wallet_routes;

/// Build the full API router with auth middleware applied to protected routes.
pub fn api_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/api/status", get(auth_routes::api_status))
        .route("/api/setup", post(auth_routes::api_setup))
        .route("/api/login", post(auth_routes::api_login));

    let protected = Router::new()
        .route("/api/session", get(auth_routes::api_session))
        .route("/api/logout", post(auth_routes::api_logout))
        .route("/api/change-pin", post(auth_routes::api_change_pin))
        .merge(node_routes::node_routes())
        .merge(account_routes::account_routes())
        .merge(history_routes::history_routes())
        .merge(analytics_routes::analytics_routes())
        // ASA issuance
        .route(
            "/api/assets/create/prepare",
            post(crate::asa::prepare_asset_create_handler),
        )
        .route("/api/assets/create", post(crate::asa::create_asset_handler))
        .route(
            "/api/assets/config/prepare",
            post(crate::asa::prepare_asset_config_handler),
        )
        .route(
            "/api/assets/issued",
            get(crate::asa::list_issued_assets_handler),
        )
        // Wallet management
        .merge(wallet_routes::wallet_routes())
        .merge(market_routes::market_routes())
        .merge(event_routes::event_routes())
        // Transfer (send ALGO / ASA)
        .route(
            "/api/transfer/prepare",
            post(crate::transfer::prepare_transfer_handler),
        )
        .route(
            "/api/transfer/send",
            post(crate::transfer::send_transfer_handler),
        )
        .route(
            "/api/transfer/opt-in",
            post(crate::transfer::opt_in_handler),
        )
        .route(
            "/api/transfer/opt-in/prepare",
            post(crate::transfer::prepare_opt_in_handler),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    // DEX sub-router (also session-authenticated).
    let dex = crate::dex::dex_router().route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        require_auth,
    ));

    let community_dex = crate::community_dex::community_dex_router().route_layer(
        axum::middleware::from_fn_with_state(state.clone(), require_auth),
    );

    let lp = crate::lp::lp_router().route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        require_auth,
    ));

    let external_liquidity = crate::external_liquidity::external_liquidity_router().route_layer(
        axum::middleware::from_fn_with_state(state.clone(), require_auth),
    );

    let router = crate::router::router_api().route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        require_auth,
    ));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(dex)
        .merge(community_dex)
        .merge(lp)
        .merge(external_liquidity)
        .merge(router)
        .with_state(state)
}
