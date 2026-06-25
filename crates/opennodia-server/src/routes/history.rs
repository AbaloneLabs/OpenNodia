use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use opennodia_dex::types::Pair;
use opennodia_node::DataSource;
use serde::{Deserialize, Serialize};

use crate::api_error::ApiError;
use crate::market::PriceHistoryPoint;
use crate::state::{current_snapshot_hour, AppState};
use crate::wallet_history::{
    BalanceSnapshotRecord, PortfolioValueSnapshotInput, PortfolioValueSnapshotRecord,
    WalletHistoryQuery,
};

use super::account_routes::{
    fetch_account, fetch_asset_params, require_registered_metadata_address,
};

pub(super) fn history_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/accounts/{addr}/transactions.csv",
            get(account_transactions_csv),
        )
        .route(
            "/api/accounts/{addr}/transactions",
            get(account_transactions),
        )
        .route(
            "/api/accounts/{addr}/balance-snapshots",
            get(account_balance_snapshots),
        )
        .route("/api/accounts/{addr}/portfolio", get(account_portfolio))
        .route(
            "/api/accounts/{addr}/portfolio-history",
            get(account_portfolio_history),
        )
        .route(
            "/api/wallets/portfolio-values",
            get(wallet_portfolio_values),
        )
}

#[derive(Debug, Deserialize)]
struct AccountTxQuery {
    limit: Option<u32>,
    offset: Option<u32>,
    min_round: Option<u64>,
    max_round: Option<u64>,
    from_time: Option<u64>,
    to_time: Option<u64>,
    tx_type: Option<String>,
    asset_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionEntry {
    pub txid: String,
    pub round: u64,
    pub timestamp: u64,
    pub tx_type: String,
    pub sender: String,
    pub receiver: Option<String>,
    pub amount: u64,
    pub asset_id: Option<u64>,
    pub fee: u64,
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
struct AccountTransactionsResponse {
    address: String,
    transactions: Vec<TransactionEntry>,
    total: u64,
    limit: u32,
    offset: u32,
}

#[derive(Debug, Deserialize)]
struct BalanceSnapshotsQuery {
    months: Option<u32>,
}

#[derive(Debug, Serialize)]
struct BalanceSnapshotsResponse {
    address: String,
    snapshots: Vec<BalanceSnapshotRecord>,
}

#[derive(Debug, Deserialize)]
pub struct PortfolioQuery {
    pub range: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PortfolioRange {
    pub(super) label: &'static str,
    pub(super) days: u32,
    snapshot_limit: u32,
}

#[derive(Debug, Serialize)]
struct PortfolioValueResponse {
    address: String,
    range: String,
    current: PortfolioCurrentValue,
    assets: Vec<PortfolioAssetValue>,
    history: Vec<PortfolioValueSnapshotRecord>,
    price_history: Vec<PriceHistoryPoint>,
    source: DataSource,
    pricing_source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioCurrentValue {
    pub captured_at: u64,
    pub round: u64,
    pub algo_amount: u64,
    pub algo_price_usd: Option<f64>,
    pub algo_value_usd: Option<f64>,
    pub asa_value_usd: Option<f64>,
    pub total_value_usd: Option<f64>,
    pub change_pct: Option<f64>,
    pub unpriced_asset_count: u32,
}

#[derive(Debug, Clone, Serialize)]
struct PortfolioAssetValue {
    asset_id: u64,
    kind: String,
    name: String,
    unit: String,
    decimals: u32,
    amount: u64,
    priced: bool,
    price_algo: Option<f64>,
    price_usd: Option<f64>,
    value_usd: Option<f64>,
    pricing_source: String,
}

#[derive(Debug, Serialize)]
struct WalletPortfolioValuesResponse {
    range: String,
    wallets: Vec<WalletPortfolioValue>,
}

#[derive(Debug, Serialize)]
struct WalletPortfolioValue {
    wallet_id: String,
    address: String,
    current: Option<PortfolioCurrentValue>,
    error: Option<String>,
}

/// `GET /api/accounts/:addr/transactions` — on-chain transaction history.
async fn account_transactions(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(query): Query<AccountTxQuery>,
) -> Result<Json<AccountTransactionsResponse>, (StatusCode, Json<ApiError>)> {
    let history_query = wallet_history_query(query, 100)?;
    let page = state
        .account_transaction_history_page(&addr, history_query)
        .await
        .map_err(history_error)?;

    let entries = page
        .transactions
        .into_iter()
        .map(transaction_entry_from_indexer)
        .collect();

    Ok(Json(AccountTransactionsResponse {
        address: addr,
        transactions: entries,
        total: page.total,
        limit: page.limit,
        offset: page.offset,
    }))
}

/// `GET /api/accounts/:addr/transactions.csv` — filtered wallet history export.
async fn account_transactions_csv(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(query): Query<AccountTxQuery>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let history_query = wallet_history_query(query, 5_000)?;
    let page = state
        .account_transaction_history_page(&addr, history_query)
        .await
        .map_err(history_error)?;
    let entries = page
        .transactions
        .into_iter()
        .map(transaction_entry_from_indexer)
        .collect::<Vec<_>>();
    let csv = transactions_csv(&addr, &entries);
    let mut response = csv.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"opennodia-transactions.csv\""),
    );
    Ok(response)
}

/// `GET /api/accounts/:addr/balance-snapshots` — persisted monthly balance trend.
async fn account_balance_snapshots(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(query): Query<BalanceSnapshotsQuery>,
) -> Result<Json<BalanceSnapshotsResponse>, (StatusCode, Json<ApiError>)> {
    let months = query.months.unwrap_or(12).clamp(1, 120);
    let snapshots = state
        .balance_snapshots(&addr, months)
        .await
        .map_err(history_error)?;
    Ok(Json(BalanceSnapshotsResponse {
        address: addr,
        snapshots,
    }))
}

/// `GET /api/accounts/:addr/portfolio` — current value plus stored value trend.
async fn account_portfolio(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(query): Query<PortfolioQuery>,
) -> Result<Json<PortfolioValueResponse>, (StatusCode, Json<ApiError>)> {
    let range = parse_portfolio_range(query.range.as_deref())?;
    let address = require_registered_metadata_address(&state, &addr).await?;
    let response = build_portfolio_value(&state, &address, range, true)
        .await
        .map_err(portfolio_error)?;
    Ok(Json(response))
}

/// `GET /api/accounts/:addr/portfolio-history` — persisted value snapshots.
async fn account_portfolio_history(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(query): Query<PortfolioQuery>,
) -> Result<Json<Vec<PortfolioValueSnapshotRecord>>, (StatusCode, Json<ApiError>)> {
    let range = parse_portfolio_range(query.range.as_deref())?;
    let address = require_registered_metadata_address(&state, &addr).await?;
    let history = state
        .portfolio_value_snapshots(&address, range_since_unix(range), range.snapshot_limit)
        .await
        .map_err(history_error)?;
    Ok(Json(history))
}

/// `GET /api/wallets/portfolio-values` — current value for registered wallets.
async fn wallet_portfolio_values(
    State(state): State<AppState>,
    Query(query): Query<PortfolioQuery>,
) -> Result<Json<WalletPortfolioValuesResponse>, (StatusCode, Json<ApiError>)> {
    let range = parse_portfolio_range(query.range.as_deref())?;
    let wallets = state.stores.wallets.list_wallets().await;
    let mut values = Vec::with_capacity(wallets.len());
    for wallet in wallets {
        let address = wallet.first_address.clone();
        let current = match build_portfolio_value(&state, &address, range, false).await {
            Ok(response) => WalletPortfolioValue {
                wallet_id: wallet.id,
                address,
                current: Some(response.current),
                error: None,
            },
            Err(error) => WalletPortfolioValue {
                wallet_id: wallet.id,
                address,
                current: None,
                error: Some(error.to_string()),
            },
        };
        values.push(current);
    }
    Ok(Json(WalletPortfolioValuesResponse {
        range: range.label.to_string(),
        wallets: values,
    }))
}

async fn build_portfolio_value(
    state: &AppState,
    address: &str,
    range: PortfolioRange,
    include_history: bool,
) -> anyhow::Result<PortfolioValueResponse> {
    if !state
        .stores
        .wallets
        .contains_registered_address(address)
        .await
    {
        anyhow::bail!("address is not registered with OpenNodia");
    }

    let (info, source) = fetch_account(state, address).await?;
    let price = state.runtime.prices.get_algo_price().await;
    let algo_price_usd = price.as_ref().map(|quote| quote.price_usd);
    let captured_at = unix_timestamp();

    let mut assets = Vec::with_capacity(info.assets.len() + 1);
    let algo_value_usd = algo_price_usd.map(|price| microalgo_to_algo(info.amount) * price);
    assets.push(PortfolioAssetValue {
        asset_id: 0,
        kind: "native".to_string(),
        name: "Algo".to_string(),
        unit: "ALGO".to_string(),
        decimals: 6,
        amount: info.amount,
        priced: algo_price_usd.is_some(),
        price_algo: Some(1.0),
        price_usd: algo_price_usd,
        value_usd: algo_value_usd,
        pricing_source: "coingecko".to_string(),
    });

    let mut asa_value_usd = 0.0f64;
    let mut unpriced_asset_count = 0u32;
    for holding in &info.assets {
        let params = fetch_asset_params(state, holding.asset_id).await?;
        let (priced, price_algo, price_usd, value_usd, pricing_source) = asa_dex_value(
            state,
            holding.asset_id,
            holding.amount,
            params.decimals,
            algo_price_usd,
        );
        if let Some(value) = value_usd {
            asa_value_usd += value;
        } else if holding.amount > 0 {
            unpriced_asset_count = unpriced_asset_count.saturating_add(1);
        }
        assets.push(PortfolioAssetValue {
            asset_id: holding.asset_id,
            kind: "asa".to_string(),
            name: if params.name.is_empty() {
                format!("Asset #{}", holding.asset_id)
            } else {
                params.name
            },
            unit: if params.unit_name.is_empty() {
                format!("#{}", holding.asset_id)
            } else {
                params.unit_name
            },
            decimals: params.decimals,
            amount: holding.amount,
            priced,
            price_algo,
            price_usd,
            value_usd,
            pricing_source,
        });
    }

    let total_value_usd = algo_value_usd.map(|algo_value| algo_value + asa_value_usd);
    if let (Some(algo_price), Some(algo_value), Some(total_value)) =
        (algo_price_usd, algo_value_usd, total_value_usd)
    {
        state
            .record_portfolio_value_snapshot(
                address,
                PortfolioValueSnapshotInput {
                    snapshot_bucket: current_snapshot_hour(),
                    source_round: info.round,
                    algo_amount: info.amount,
                    algo_price_usd: algo_price,
                    algo_value_usd: algo_value,
                    asa_value_usd,
                    total_value_usd: total_value,
                    unpriced_asset_count,
                },
            )
            .await?;
    }

    let history = if include_history {
        match state
            .portfolio_value_snapshots(address, range_since_unix(range), range.snapshot_limit)
            .await
        {
            Ok(history) => history,
            Err(error) => {
                tracing::warn!(%address, %error, "portfolio value history query failed");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let price_history = if include_history {
        state
            .runtime
            .prices
            .get_algo_history(range.days)
            .await
            .map(|quote| quote.points)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let change_pct = total_value_usd.and_then(|current| portfolio_change_pct(&history, current));

    Ok(PortfolioValueResponse {
        address: address.to_string(),
        range: range.label.to_string(),
        current: PortfolioCurrentValue {
            captured_at,
            round: info.round,
            algo_amount: info.amount,
            algo_price_usd,
            algo_value_usd,
            asa_value_usd: total_value_usd.map(|_| asa_value_usd),
            total_value_usd,
            change_pct,
            unpriced_asset_count,
        },
        assets,
        history,
        price_history,
        source,
        pricing_source: "coingecko + local_dex_fills".to_string(),
    })
}

fn asa_dex_value(
    state: &AppState,
    asset_id: u64,
    amount: u64,
    decimals: u32,
    algo_price_usd: Option<f64>,
) -> (bool, Option<f64>, Option<f64>, Option<f64>, String) {
    let Some(price_usd) = algo_price_usd else {
        return (false, None, None, None, "price_unavailable".to_string());
    };
    let Some(dex) = state.stores.dex.as_ref() else {
        return (false, None, None, None, "unpriced_no_dex_trade".to_string());
    };
    let pair = Pair::new(0, asset_id);
    let Ok(Some(price_micro_algo)) = dex.get_last_trade_price(pair, asset_id) else {
        return (false, None, None, None, "unpriced_no_dex_trade".to_string());
    };
    let price_algo = price_micro_algo as f64 * 10_f64.powi(decimals as i32) / 1_000_000_000_000.0;
    let asset_price_usd = price_algo * price_usd;
    let value_usd = amount as f64 * price_micro_algo as f64 * price_usd / 1_000_000_000_000.0;
    (
        true,
        Some(price_algo),
        Some(asset_price_usd),
        Some(value_usd),
        "local_dex_fill".to_string(),
    )
}

pub(super) fn parse_portfolio_range(
    value: Option<&str>,
) -> Result<PortfolioRange, (StatusCode, Json<ApiError>)> {
    match value.unwrap_or("1m").trim().to_ascii_lowercase().as_str() {
        "1d" => Ok(PortfolioRange {
            label: "1d",
            days: 1,
            snapshot_limit: 288,
        }),
        "1w" => Ok(PortfolioRange {
            label: "1w",
            days: 7,
            snapshot_limit: 336,
        }),
        "1m" => Ok(PortfolioRange {
            label: "1m",
            days: 31,
            snapshot_limit: 744,
        }),
        "1y" => Ok(PortfolioRange {
            label: "1y",
            days: 365,
            snapshot_limit: 2_000,
        }),
        other => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("unsupported range: {other}"))),
        )),
    }
}

fn range_since_unix(range: PortfolioRange) -> u64 {
    unix_timestamp().saturating_sub(u64::from(range.days) * 86_400)
}

fn microalgo_to_algo(amount: u64) -> f64 {
    amount as f64 / 1_000_000.0
}

fn portfolio_change_pct(
    history: &[PortfolioValueSnapshotRecord],
    current_value: f64,
) -> Option<f64> {
    let first = history
        .iter()
        .find(|snapshot| snapshot.total_value_usd > 0.0)?;
    if first.total_value_usd <= 0.0 {
        return None;
    }
    Some(((current_value - first.total_value_usd) / first.total_value_usd) * 100.0)
}

fn portfolio_error(error: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let message = error.to_string();
    let status = if message.contains("registered with OpenNodia") {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::BAD_GATEWAY
    };
    (
        status,
        Json(ApiError::new(format!("portfolio valuation: {message}"))),
    )
}

fn wallet_history_query(
    query: AccountTxQuery,
    max_limit: u32,
) -> Result<WalletHistoryQuery, (StatusCode, Json<ApiError>)> {
    if matches!((query.min_round, query.max_round), (Some(min), Some(max)) if min > max) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "min_round must be less than or equal to max_round",
            )),
        ));
    }
    if matches!((query.from_time, query.to_time), (Some(from), Some(to)) if from > to) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "from_time must be less than or equal to to_time",
            )),
        ));
    }
    let tx_type = query
        .tx_type
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            const ALLOWED: &[&str] = &["pay", "axfer", "afrz", "keyreg", "acfg", "appl"];
            if ALLOWED.contains(&value.as_str()) {
                Ok(value)
            } else {
                Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiError::new(format!("unsupported tx_type: {value}"))),
                ))
            }
        })
        .transpose()?;
    Ok(WalletHistoryQuery {
        limit: query.limit.unwrap_or(20).clamp(1, max_limit),
        offset: query.offset.unwrap_or(0),
        min_round: query.min_round,
        max_round: query.max_round,
        from_time: query.from_time,
        to_time: query.to_time,
        tx_type,
        asset_id: query.asset_id,
    })
}

pub(super) fn transaction_entry_from_indexer(
    tx: opennodia_node::IndexerTransaction,
) -> TransactionEntry {
    let (receiver, amount, asset_id) = if let Some(ref p) = tx.payment {
        (Some(p.receiver.clone()), p.amount, None)
    } else if let Some(ref at) = tx.asset_transfer {
        (Some(at.receiver.clone()), at.amount, Some(at.asset_id))
    } else {
        (tx.receiver.clone(), tx.amount.unwrap_or(0), tx.asset_id)
    };
    TransactionEntry {
        txid: tx.id,
        round: tx.round,
        timestamp: tx.round_time,
        tx_type: tx.tx_type,
        sender: tx.sender,
        receiver,
        amount,
        asset_id,
        fee: tx.fee,
        confirmed: tx.round > 0,
    }
}

fn transactions_csv(address: &str, entries: &[TransactionEntry]) -> String {
    let mut csv = String::from("date,type,asset,amount,relative_address,round,txid\n");
    for entry in entries {
        let relative = if entry.sender == address {
            entry.receiver.as_deref().unwrap_or("")
        } else {
            entry.sender.as_str()
        };
        let asset = entry
            .asset_id
            .map(|asset_id| asset_id.to_string())
            .unwrap_or_else(|| "ALGO".to_string());
        csv.push_str(&csv_row(&[
            entry.timestamp.to_string(),
            entry.tx_type.clone(),
            asset,
            entry.amount.to_string(),
            relative.to_string(),
            entry.round.to_string(),
            entry.txid.clone(),
        ]));
    }
    csv
}

fn csv_row(fields: &[String]) -> String {
    let mut row = String::new();
    for (idx, field) in fields.iter().enumerate() {
        if idx > 0 {
            row.push(',');
        }
        row.push_str(&csv_field(field));
    }
    row.push('\n');
    row
}

fn csv_field(value: &str) -> String {
    if value
        .chars()
        .any(|ch| matches!(ch, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn history_error(error: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let message = error.to_string();
    let status = if message.contains("registered with OpenNodia") {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::BAD_GATEWAY
    };
    (
        status,
        Json(ApiError::new(format!("wallet history query: {message}"))),
    )
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_tx_query() -> AccountTxQuery {
        AccountTxQuery {
            limit: None,
            offset: None,
            min_round: None,
            max_round: None,
            from_time: None,
            to_time: None,
            tx_type: None,
            asset_id: None,
        }
    }

    #[test]
    fn wallet_history_query_rejects_inverted_ranges() {
        let mut query = empty_tx_query();
        query.min_round = Some(20);
        query.max_round = Some(10);
        assert!(wallet_history_query(query, 100).is_err());

        let mut query = empty_tx_query();
        query.from_time = Some(200);
        query.to_time = Some(100);
        assert!(wallet_history_query(query, 100).is_err());
    }

    #[test]
    fn wallet_history_query_normalizes_filters() {
        let mut query = empty_tx_query();
        query.limit = Some(500);
        query.offset = Some(20);
        query.tx_type = Some(" axfer ".into());
        query.asset_id = Some(42);
        let normalized = wallet_history_query(query, 100).unwrap();
        assert_eq!(normalized.limit, 100);
        assert_eq!(normalized.offset, 20);
        assert_eq!(normalized.tx_type.as_deref(), Some("axfer"));
        assert_eq!(normalized.asset_id, Some(42));
    }

    #[test]
    fn transactions_csv_escapes_fields_and_uses_relative_address() {
        let entries = vec![TransactionEntry {
            txid: "TX,ID".into(),
            round: 7,
            timestamp: 1_700_000_000,
            tx_type: "pay".into(),
            sender: "ADDR".into(),
            receiver: Some("RECV\"ADDR".into()),
            amount: 123,
            asset_id: None,
            fee: 1_000,
            confirmed: true,
        }];
        let csv = transactions_csv("ADDR", &entries);
        assert!(csv.starts_with("date,type,asset,amount,relative_address,round,txid\n"));
        assert!(csv.contains("\"RECV\"\"ADDR\""));
        assert!(csv.contains("\"TX,ID\""));
    }

    #[test]
    fn portfolio_range_accepts_known_windows() {
        assert_eq!(parse_portfolio_range(None).unwrap().label, "1m");
        assert_eq!(parse_portfolio_range(Some("1D")).unwrap().days, 1);
        assert_eq!(parse_portfolio_range(Some("1y")).unwrap().days, 365);
        assert!(parse_portfolio_range(Some("2y")).is_err());
    }

    #[test]
    fn portfolio_change_uses_oldest_positive_snapshot() {
        let history = vec![PortfolioValueSnapshotRecord {
            snapshot_bucket: "2026-06-22T00:00:00Z".into(),
            source_round: 1,
            algo_amount: 1_000_000,
            algo_price_usd: 0.2,
            algo_value_usd: 0.2,
            asa_value_usd: 0.0,
            total_value_usd: 10.0,
            unpriced_asset_count: 0,
            captured_at: 1,
        }];
        assert_eq!(portfolio_change_pct(&history, 12.5), Some(25.0));
    }
}
