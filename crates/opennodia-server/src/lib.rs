//! OpenNodia HTTP server, web UI serving, and PIN authentication.

pub mod api_error;
pub mod asa;
pub mod asa_history;
pub mod asset_metadata;
pub mod auth;
pub mod community_dex;
pub mod config;
pub mod dex;
pub mod dex_validation;
pub mod external_liquidity;
pub mod intent;
pub mod lp;
pub mod market;
pub mod middleware;
pub mod mnemonic;
pub mod participation;
pub mod router;
pub mod routes;
pub mod session;
pub mod state;
pub mod sync;
pub mod transfer;
pub mod tx_flow;
pub mod wallet;
pub mod wallet_history;
pub mod workers;

pub use config::Config;
pub use state::AppState;

use std::path::PathBuf;

use axum::Router;
use tower_http::trace::TraceLayer;

/// Build the axum application from app state.
pub fn build_app(state: AppState) -> Router {
    let api = routes::api_router(state.clone());

    // The bundled UI and API share one origin. Do not enable permissive CORS:
    // unauthenticated setup endpoints must not be callable by arbitrary sites.
    let app = Router::new()
        .merge(api)
        .layer(axum::middleware::from_fn(middleware::security_headers))
        .layer(TraceLayer::new_for_http());

    // Serve static frontend files if the directory exists.
    if let Some(web_dir) = &state.web_dir {
        serve_static(app, web_dir)
    } else {
        app
    }
}

/// Attach a static file service for the frontend SPA.
fn serve_static(app: Router, web_dir: &PathBuf) -> Router {
    use tower_http::services::ServeDir;
    let index = web_dir.join("index.html");
    tracing::info!(dir = %web_dir.display(), "serving web UI");
    app.fallback_service(ServeDir::new(web_dir).fallback(ServeDir::new(index)))
}

/// Run the server. This is the shared entrypoint used by `main.rs` and tests.
pub async fn run(config: Config, web_dir: Option<PathBuf>) -> anyhow::Result<()> {
    config.ensure_data_dir()?;
    let mut state = AppState::from_config(config.clone()).await?;
    state.web_dir = web_dir;

    let addr = state.config.socket_addr();

    dex_validation::run_startup_validation(&state).await;
    if state.stores.dex.is_some() {
        if let Err(error) = dex::reconcile_orders(&state).await {
            let checked_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            state.runtime.dex_validation.record_failed(
                checked_at,
                format!("initial DEX reconciliation failed: {error}"),
            );
            tracing::error!(%error, "initial DEX reconciliation failed");
        }
    }

    workers::spawn_background_workers(&state);

    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind {addr}: {e}"))?;
    tracing::info!(%addr, "OpenNodia server listening");
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("serve: {e}"))?;
    Ok(())
}
