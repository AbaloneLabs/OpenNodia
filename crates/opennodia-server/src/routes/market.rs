use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::api_error::ApiError;
use crate::market::PriceHistoryPoint;
use crate::state::AppState;

use super::history_routes::{parse_portfolio_range, PortfolioQuery};

pub(super) fn market_routes() -> Router<AppState> {
    Router::new()
        .route("/api/market/price", get(market_price))
        .route("/api/market/algo/history", get(market_algo_history))
}

#[derive(Debug, Serialize)]
struct MarketPriceResponse {
    price_usd: f64,
    change_24h: f64,
    available: bool,
}

#[derive(Debug, Serialize)]
struct MarketHistoryResponse {
    range: String,
    points: Vec<PriceHistoryPoint>,
    available: bool,
}

/// `GET /api/market/price` — current ALGO/USD price.
async fn market_price(State(state): State<AppState>) -> Json<MarketPriceResponse> {
    match state.runtime.prices.get_algo_price().await {
        Some(quote) => Json(MarketPriceResponse {
            price_usd: quote.price_usd,
            change_24h: quote.change_24h,
            available: true,
        }),
        None => Json(MarketPriceResponse {
            price_usd: 0.0,
            change_24h: 0.0,
            available: false,
        }),
    }
}

/// `GET /api/market/algo/history?range=1m` — ALGO/USD history.
async fn market_algo_history(
    State(state): State<AppState>,
    Query(query): Query<PortfolioQuery>,
) -> Result<Json<MarketHistoryResponse>, (StatusCode, Json<ApiError>)> {
    let range = parse_portfolio_range(query.range.as_deref())?;
    let history = state.runtime.prices.get_algo_history(range.days).await;
    Ok(Json(MarketHistoryResponse {
        range: range.label.to_string(),
        points: history
            .as_ref()
            .map(|quote| quote.points.clone())
            .unwrap_or_default(),
        available: history.is_some(),
    }))
}
