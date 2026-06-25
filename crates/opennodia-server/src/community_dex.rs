//! Community DEX market registry endpoints.
//!
//! A community market is operator-authenticated metadata around official ASA
//! pairs. The server never grants an official badge from submitted metadata
//! alone: each ASA is checked against live algod asset parameters.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use opennodia_assets::AssetPolicyGrade;
use opennodia_core::{Address, Round};
use opennodia_dex::{CommunityMarket, Pair, Trade};
use opennodia_node::DataSource;
use opennodia_swap::OrderSide;
use serde::{Deserialize, Serialize};

use crate::routes::ApiError;
use crate::state::AppState;

type ApiErrorResponse = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<T, ApiErrorResponse>;

const DEFAULT_MARKET_LIMIT: u32 = 50;
const MAX_MARKET_LIMIT: u32 = 100;
const DEFAULT_DEPTH: u32 = 50;
const MAX_DEPTH: u32 = 100;
const DEFAULT_TRADE_LIMIT: u32 = 50;
const MAX_TRADE_LIMIT: u32 = 100;

/// Build the community DEX sub-router. Mounted under the protected auth layer.
pub fn community_dex_router() -> Router<AppState> {
    Router::new()
        .route("/api/dex/markets", get(list_markets).post(create_market))
        .route("/api/dex/markets/{id}", get(get_market).put(update_market))
        .route("/api/dex/markets/{id}/pairs", get(market_pairs))
        .route("/api/dex/markets/{id}/orderbook", get(market_orderbook))
        .route("/api/dex/markets/{id}/trades", get(market_trades))
}

#[derive(Debug, Deserialize)]
pub struct MarketRequest {
    pub id: String,
    pub operator: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub logo_url: String,
    pub asset_ids: Vec<u64>,
    pub pairs: Vec<PairRequest>,
    #[serde(default)]
    pub migration_notice: Option<String>,
    #[serde(default)]
    pub announcement_channel: Option<String>,
    pub signature: String,
    pub updated_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct PairRequest {
    pub asset_a: u64,
    pub asset_b: u64,
}

#[derive(Debug, Deserialize)]
pub struct MarketListQuery {
    pub operator: Option<String>,
    pub asset_id: Option<u64>,
    pub q: Option<String>,
    #[serde(default = "default_market_limit")]
    pub limit: u32,
}

fn default_market_limit() -> u32 {
    DEFAULT_MARKET_LIMIT
}

#[derive(Debug, Deserialize)]
pub struct MarketOrderbookQuery {
    pub asset_a: u64,
    pub asset_b: u64,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    DEFAULT_DEPTH
}

#[derive(Debug, Deserialize)]
pub struct MarketTradesQuery {
    pub asset_a: u64,
    pub asset_b: u64,
    #[serde(default = "default_trade_limit")]
    pub limit: u32,
}

fn default_trade_limit() -> u32 {
    DEFAULT_TRADE_LIMIT
}

#[derive(Debug, Serialize)]
pub struct MarketListResponse {
    pub markets: Vec<CommunityMarketResponse>,
    pub source_round: u64,
    pub source: DataSource,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CommunityMarketResponse {
    pub id: String,
    pub operator: String,
    pub name: String,
    pub description: String,
    pub logo_url: String,
    pub asset_ids: Vec<u64>,
    pub pairs: Vec<CommunityPairResponse>,
    pub migration_notice: Option<String>,
    pub announcement_channel: Option<String>,
    pub signature: String,
    pub updated_at: u64,
    pub official: bool,
    pub verification: Vec<AssetVerificationResponse>,
    pub warnings: Vec<String>,
    pub signing_payload: String,
}

#[derive(Debug, Serialize)]
pub struct MarketPairsResponse {
    pub market: CommunityMarketResponse,
    pub pairs: Vec<CommunityPairResponse>,
    pub source_round: u64,
    pub source: DataSource,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunityPairResponse {
    pub asset_a: u64,
    pub asset_b: u64,
    pub display: String,
    pub official: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetVerificationResponse {
    pub asset_id: u64,
    pub kind: String,
    pub name: String,
    pub unit: String,
    pub decimals: u32,
    pub creator: String,
    pub policy: String,
    pub creator_matches_operator: bool,
    pub tradeable_by_default: bool,
    pub official: bool,
    pub source: String,
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MarketOrderbookResponse {
    pub market_id: String,
    pub operator: String,
    pub pair: CommunityPairResponse,
    pub spoofing_warning: Option<String>,
    pub orderbook: CommunityOrderbookResponse,
}

#[derive(Debug, Serialize)]
pub struct CommunityOrderbookResponse {
    pub pair: String,
    pub bids: Vec<PriceLevelResponse>,
    pub asks: Vec<PriceLevelResponse>,
    pub spread: u64,
    pub last_price: Option<u64>,
    pub last_update_round: u64,
    pub source: DataSource,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PriceLevelResponse {
    pub price: u64,
    pub amount: u64,
    pub total: u64,
    pub order_count: usize,
}

#[derive(Debug, Serialize)]
pub struct MarketTradesResponse {
    pub market_id: String,
    pub operator: String,
    pub pair: CommunityPairResponse,
    pub trades: Vec<TradeResponse>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TradeResponse {
    pub tx_id: String,
    pub pair: String,
    pub side: String,
    pub price: u64,
    pub amount: u64,
    pub buyer: String,
    pub seller: String,
    pub round: u64,
    pub timestamp: u64,
}

pub async fn create_market(
    State(state): State<AppState>,
    Json(req): Json<MarketRequest>,
) -> ApiResult<(StatusCode, Json<CommunityMarketResponse>)> {
    let market = market_from_request(req, None)?;
    verify_market_signature(&market)?;
    let db = require_dex(&state)?;
    db.upsert_community_market(&market)
        .map_err(|error| bad_request(format!("invalid community market: {error}")))?;
    let (status, source) = ledger_status(&state).await?;
    let response = market_response(&state, market, status.last_round, source).await;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn update_market(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<MarketRequest>,
) -> ApiResult<Json<CommunityMarketResponse>> {
    let market = market_from_request(req, Some(&id))?;
    verify_market_signature(&market)?;
    let db = require_dex(&state)?;
    db.upsert_community_market(&market)
        .map_err(|error| bad_request(format!("invalid community market: {error}")))?;
    let (status, source) = ledger_status(&state).await?;
    Ok(Json(
        market_response(&state, market, status.last_round, source).await,
    ))
}

pub async fn get_market(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<CommunityMarketResponse>> {
    let db = require_dex(&state)?;
    let market = db
        .get_community_market(&normalize_market_id(&id))
        .map_err(|error| internal(format!("load community market: {error}")))?
        .ok_or_else(|| not_found(format!("community market '{id}' not found")))?;
    let (status, source) = ledger_status(&state).await?;
    Ok(Json(
        market_response(&state, market, status.last_round, source).await,
    ))
}

pub async fn list_markets(
    State(state): State<AppState>,
    Query(query): Query<MarketListQuery>,
) -> ApiResult<Json<MarketListResponse>> {
    let db = require_dex(&state)?;
    let operator = query.operator.as_deref().map(parse_address).transpose()?;
    let limit = query.limit.clamp(1, MAX_MARKET_LIMIT);
    let mut markets = db
        .list_community_markets(operator.as_ref(), query.asset_id, limit)
        .map_err(|error| internal(format!("list community markets: {error}")))?;
    if let Some(term) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|term| !term.is_empty())
    {
        let needle = term.to_ascii_lowercase();
        markets.retain(|market| {
            market.id.contains(&needle)
                || market.name.to_ascii_lowercase().contains(&needle)
                || market.description.to_ascii_lowercase().contains(&needle)
        });
    }

    let (status, source) = ledger_status(&state).await?;
    let mut responses = Vec::with_capacity(markets.len());
    let mut warnings = Vec::new();
    for market in markets {
        let response = market_response(&state, market, status.last_round, source).await;
        warnings.extend(response.warnings.clone());
        responses.push(response);
    }

    Ok(Json(MarketListResponse {
        markets: responses,
        source_round: status.last_round.as_u64(),
        source,
        warnings: dedupe_strings(warnings),
    }))
}

pub async fn market_pairs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<MarketPairsResponse>> {
    let db = require_dex(&state)?;
    let market = load_market(&db, &id)?;
    let (status, source) = ledger_status(&state).await?;
    let response = market_response(&state, market, status.last_round, source).await;
    Ok(Json(MarketPairsResponse {
        pairs: response.pairs.clone(),
        warnings: response.warnings.clone(),
        market: response,
        source_round: status.last_round.as_u64(),
        source,
    }))
}

pub async fn market_orderbook(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<MarketOrderbookQuery>,
) -> ApiResult<Json<MarketOrderbookResponse>> {
    let db = require_dex(&state)?;
    let market = load_market(&db, &id)?;
    let pair = Pair::new(query.asset_a, query.asset_b);
    require_market_pair(&market, pair)?;
    let (status, source) = ledger_status(&state).await?;
    let verification = verify_market_assets(&state, &market).await;
    let pair_response = pair_response(pair, &market, &verification);
    let current_round = status.last_round;
    let snapshot = opennodia_dex::get_orderbook(&db, pair, query.asset_a, current_round)
        .map_err(|error| internal(format!("community orderbook: {error}")))?;
    let depth = query.depth.clamp(1, MAX_DEPTH);
    let mut orderbook = CommunityOrderbookResponse {
        pair: snapshot.pair.display(),
        bids: snapshot.bids.into_iter().map(Into::into).collect(),
        asks: snapshot.asks.into_iter().map(Into::into).collect(),
        spread: snapshot.spread,
        last_price: snapshot.last_price,
        last_update_round: snapshot.last_update_round.as_u64(),
        source,
        warnings: pair_response.warnings.clone(),
    };
    orderbook.bids.truncate(depth as usize);
    orderbook.asks.truncate(depth as usize);
    Ok(Json(MarketOrderbookResponse {
        market_id: market.id,
        operator: market.operator.to_string(),
        spoofing_warning: spoofing_warning(&pair_response),
        pair: pair_response,
        orderbook,
    }))
}

pub async fn market_trades(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<MarketTradesQuery>,
) -> ApiResult<Json<MarketTradesResponse>> {
    let db = require_dex(&state)?;
    let market = load_market(&db, &id)?;
    let pair = Pair::new(query.asset_a, query.asset_b);
    require_market_pair(&market, pair)?;
    let verification = verify_market_assets(&state, &market).await;
    let pair_response = pair_response(pair, &market, &verification);
    let trades = db
        .get_recent_trades(pair, query.limit.clamp(1, MAX_TRADE_LIMIT))
        .map_err(|error| internal(format!("community trades: {error}")))?;
    Ok(Json(MarketTradesResponse {
        market_id: market.id,
        operator: market.operator.to_string(),
        warnings: pair_response.warnings.clone(),
        pair: pair_response,
        trades: trades
            .into_iter()
            .map(|trade| TradeResponse::from_view(trade, Some(query.asset_a)))
            .collect(),
    }))
}

fn market_from_request(req: MarketRequest, path_id: Option<&str>) -> ApiResult<CommunityMarket> {
    let id = normalize_market_id(&req.id);
    if let Some(path_id) = path_id {
        let normalized_path = normalize_market_id(path_id);
        if id != normalized_path {
            return Err(bad_request(format!(
                "request id '{id}' does not match path id '{normalized_path}'"
            )));
        }
    }

    let mut asset_ids = req.asset_ids;
    asset_ids.sort_unstable();
    asset_ids.dedup();

    let mut pairs: Vec<Pair> = req
        .pairs
        .into_iter()
        .map(|pair| Pair::new(pair.asset_a, pair.asset_b))
        .collect();
    pairs.sort_by_key(|pair| (pair.asset_a, pair.asset_b));
    pairs.dedup();

    Ok(CommunityMarket {
        id,
        operator: parse_address(&req.operator)?,
        name: req.name.trim().to_string(),
        description: req.description.trim().to_string(),
        logo_url: req.logo_url.trim().to_string(),
        asset_ids,
        pairs,
        migration_notice: normalize_optional(req.migration_notice),
        announcement_channel: normalize_optional(req.announcement_channel),
        signature: req.signature.trim().to_string(),
        updated_at: req.updated_at,
    })
}

fn normalize_market_id(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn verify_market_signature(market: &CommunityMarket) -> ApiResult<String> {
    let payload = community_market_signing_payload(market);
    let signature_bytes = STANDARD
        .decode(market.signature.trim())
        .map_err(|error| bad_request(format!("invalid market signature base64: {error}")))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| bad_request(format!("invalid market signature: {error}")))?;
    let key = VerifyingKey::from_bytes(market.operator.as_bytes())
        .map_err(|error| bad_request(format!("invalid operator signing key: {error}")))?;
    key.verify(payload.as_bytes(), &signature)
        .map_err(|_| bad_request("invalid market signature for canonical payload"))?;
    Ok(payload)
}

fn community_market_signing_payload(market: &CommunityMarket) -> String {
    let assets = market
        .asset_ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let pairs = market
        .pairs
        .iter()
        .map(|pair| format!("{}-{}", pair.asset_a, pair.asset_b))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "OpenNodiaCommunityMarketV1\nid={}\noperator={}\nname={}\ndescription={}\nlogo_url={}\nasset_ids={}\npairs={}\nmigration_notice={}\nannouncement_channel={}\nupdated_at={}",
        market.id,
        market.operator,
        market.name,
        market.description,
        market.logo_url,
        assets,
        pairs,
        market.migration_notice.as_deref().unwrap_or_default(),
        market.announcement_channel.as_deref().unwrap_or_default(),
        market.updated_at
    )
}

async fn market_response(
    state: &AppState,
    market: CommunityMarket,
    _source_round: Round,
    _source: DataSource,
) -> CommunityMarketResponse {
    let verification = verify_market_assets(state, &market).await;
    let pairs = market
        .pairs
        .iter()
        .copied()
        .map(|pair| pair_response(pair, &market, &verification))
        .collect::<Vec<_>>();
    let mut warnings = verification
        .values()
        .filter_map(|entry| entry.warning.clone())
        .collect::<Vec<_>>();
    warnings.extend(
        pairs
            .iter()
            .flat_map(|pair| pair.warnings.clone())
            .collect::<Vec<_>>(),
    );
    let official = !market.asset_ids.is_empty()
        && market.asset_ids.iter().all(|asset_id| {
            verification
                .get(asset_id)
                .is_some_and(|entry| entry.official)
        })
        && pairs.iter().all(|pair| pair.official);

    CommunityMarketResponse {
        id: market.id.clone(),
        operator: market.operator.to_string(),
        name: market.name.clone(),
        description: market.description.clone(),
        logo_url: market.logo_url.clone(),
        asset_ids: market.asset_ids.clone(),
        pairs,
        migration_notice: market.migration_notice.clone(),
        announcement_channel: market.announcement_channel.clone(),
        signature: market.signature.clone(),
        updated_at: market.updated_at,
        official,
        verification: market
            .asset_ids
            .iter()
            .filter_map(|asset_id| verification.get(asset_id).cloned())
            .collect(),
        warnings: dedupe_strings(warnings),
        signing_payload: community_market_signing_payload(&market),
    }
}

async fn verify_market_assets(
    state: &AppState,
    market: &CommunityMarket,
) -> HashMap<u64, AssetVerificationResponse> {
    let mut ids = market.asset_ids.clone();
    for pair in &market.pairs {
        if pair.asset_a != 0 {
            ids.push(pair.asset_a);
        }
        if pair.asset_b != 0 {
            ids.push(pair.asset_b);
        }
    }
    ids.sort_unstable();
    ids.dedup();

    let mut out = HashMap::new();
    for asset_id in ids {
        if asset_id == 0 {
            continue;
        }
        let response = match fetch_asset_params(state, asset_id).await {
            Ok(params) => {
                let policy = AssetPolicyGrade::classify(
                    authority_is_enabled(&params.freeze).unwrap_or(true),
                    authority_is_enabled(&params.clawback).unwrap_or(true),
                    params.default_frozen,
                );
                let creator_matches_operator = params.creator == market.operator.to_string();
                let tradeable_by_default = policy.is_tradeable_by_default();
                let official = creator_matches_operator && tradeable_by_default;
                let warning = if !creator_matches_operator {
                    Some(format!(
                        "asset {asset_id} creator does not match market operator"
                    ))
                } else if !tradeable_by_default {
                    Some(format!(
                        "asset {asset_id} policy is {} and is not DEX-tradeable by default",
                        policy_str(policy)
                    ))
                } else {
                    None
                };
                AssetVerificationResponse {
                    asset_id,
                    kind: "asa".into(),
                    name: if params.name.is_empty() {
                        format!("Asset #{asset_id}")
                    } else {
                        params.name
                    },
                    unit: if params.unit_name.is_empty() {
                        format!("#{asset_id}")
                    } else {
                        params.unit_name
                    },
                    decimals: params.decimals,
                    creator: params.creator,
                    policy: policy_str(policy).into(),
                    creator_matches_operator,
                    tradeable_by_default,
                    official,
                    source: "algod".into(),
                    warning,
                }
            }
            Err(error) => AssetVerificationResponse {
                asset_id,
                kind: "asa".into(),
                name: format!("Asset #{asset_id}"),
                unit: format!("#{asset_id}"),
                decimals: 0,
                creator: String::new(),
                policy: "unknown".into(),
                creator_matches_operator: false,
                tradeable_by_default: false,
                official: false,
                source: "unavailable".into(),
                warning: Some(format!(
                    "asset {asset_id} lookup failed: {}",
                    api_error(&error)
                )),
            },
        };
        out.insert(asset_id, response);
    }
    out
}

fn pair_response(
    pair: Pair,
    market: &CommunityMarket,
    verification: &HashMap<u64, AssetVerificationResponse>,
) -> CommunityPairResponse {
    let claimed: HashSet<u64> = market.asset_ids.iter().copied().collect();
    let mut warnings = Vec::new();
    for asset_id in [pair.asset_a, pair.asset_b] {
        if asset_id == 0 {
            continue;
        }
        if !claimed.contains(&asset_id) {
            warnings.push(format!(
                "asset {asset_id} is not claimed by market operator"
            ));
            continue;
        }
        match verification.get(&asset_id) {
            Some(entry) if entry.official => {}
            Some(entry) => {
                if let Some(warning) = &entry.warning {
                    warnings.push(warning.clone());
                } else {
                    warnings.push(format!("asset {asset_id} is not official"));
                }
            }
            None => warnings.push(format!("asset {asset_id} was not verified")),
        }
    }

    CommunityPairResponse {
        asset_a: pair.asset_a,
        asset_b: pair.asset_b,
        display: pair.display(),
        official: warnings.is_empty(),
        warnings: dedupe_strings(warnings),
    }
}

fn spoofing_warning(pair: &CommunityPairResponse) -> Option<String> {
    if pair.official {
        None
    } else {
        Some(
            "community pair is not fully operator-verified; do not display an official badge"
                .into(),
        )
    }
}

fn load_market(db: &opennodia_dex::DexDb, id: &str) -> ApiResult<CommunityMarket> {
    db.get_community_market(&normalize_market_id(id))
        .map_err(|error| internal(format!("load community market: {error}")))?
        .ok_or_else(|| not_found(format!("community market '{id}' not found")))
}

fn require_market_pair(market: &CommunityMarket, pair: Pair) -> ApiResult<()> {
    if market.pairs.contains(&pair) {
        Ok(())
    } else {
        Err(not_found(format!(
            "pair {} is not registered for market '{}'",
            pair.display(),
            market.id
        )))
    }
}

async fn ledger_status(state: &AppState) -> ApiResult<(opennodia_node::NodeStatus, DataSource)> {
    let (_, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    Ok((status, source))
}

fn require_dex(state: &AppState) -> ApiResult<std::sync::Arc<opennodia_dex::DexDb>> {
    state
        .stores
        .dex
        .clone()
        .ok_or_else(|| service_unavailable("DEX orderbook database unavailable"))
}

async fn fetch_asset_params(
    state: &AppState,
    asset_id: u64,
) -> ApiResult<opennodia_node::AssetParams> {
    const TTL: std::time::Duration = std::time::Duration::from_secs(300);
    {
        let cache = state.caches.asset_params.lock().await;
        if let Some((fetched_at, params)) = cache.get(&asset_id) {
            if fetched_at.elapsed() < TTL {
                return Ok(params.clone());
            }
        }
    }
    let (params, _) = state
        .ledger
        .algod
        .asset_params_with_fallback(state.ledger.public_algod.as_ref(), asset_id)
        .await
        .map_err(|error| {
            service_unavailable(format!("fetch asset {asset_id} parameters: {error}"))
        })?;
    {
        let mut cache = state.caches.asset_params.lock().await;
        cache.insert(asset_id, (std::time::Instant::now(), params.clone()));
    }
    Ok(params)
}

fn authority_is_enabled(value: &str) -> ApiResult<bool> {
    if value.is_empty() {
        return Ok(false);
    }
    let address: Address = value.parse().map_err(|error| {
        service_unavailable(format!("invalid asset authority address: {error}"))
    })?;
    Ok(!address.is_zero())
}

fn policy_str(policy: AssetPolicyGrade) -> &'static str {
    match policy {
        AssetPolicyGrade::Open => "open",
        AssetPolicyGrade::Bridged => "bridged",
        AssetPolicyGrade::Regulated => "regulated",
    }
}

fn parse_address(value: &str) -> ApiResult<Address> {
    value
        .trim()
        .parse::<Address>()
        .map_err(|error| bad_request(format!("invalid address '{}': {error}", value.trim())))
}

fn bad_request(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::BAD_REQUEST, Json(ApiError::new(msg)))
}

fn not_found(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::NOT_FOUND, Json(ApiError::new(msg)))
}

fn service_unavailable(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::SERVICE_UNAVAILABLE, Json(ApiError::new(msg)))
}

fn internal(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(msg)))
}

fn api_error(error: &ApiErrorResponse) -> String {
    error.1.error.clone()
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

impl From<opennodia_dex::PriceLevel> for PriceLevelResponse {
    fn from(value: opennodia_dex::PriceLevel) -> Self {
        Self {
            price: value.price,
            amount: value.amount,
            total: value.total,
            order_count: value.order_count,
        }
    }
}

impl TradeResponse {
    fn from_view(trade: Trade, view_base_asset: Option<u64>) -> Self {
        let should_invert = view_base_asset
            .zip(trade.base_asset)
            .is_some_and(|(view_base, trade_base)| view_base != trade_base);
        let price = if should_invert {
            opennodia_dex::types::invert_price(trade.price)
        } else {
            trade.price
        };
        let amount = if should_invert {
            ((u128::from(trade.amount) * u128::from(trade.price)) / 1_000_000u128)
                .min(u128::from(u64::MAX)) as u64
        } else {
            trade.amount
        };
        let side = if should_invert {
            match trade.side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            }
        } else {
            trade.side
        };
        Self {
            tx_id: trade.tx_id,
            pair: trade.pair.display(),
            side: side.as_str().to_string(),
            price,
            amount,
            buyer: trade.buyer.to_string(),
            seller: trade.seller.to_string(),
            round: trade.round.as_u64(),
            timestamp: trade.timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_market() -> CommunityMarket {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let operator = Address::from_bytes(signing_key.verifying_key().to_bytes());
        let mut market = CommunityMarket {
            id: "official-qat".into(),
            operator,
            name: "Official QAT".into(),
            description: "QAT community market".into(),
            logo_url: "https://example.invalid/qat.png".into(),
            asset_ids: vec![42, 99],
            pairs: vec![Pair::new(0, 42), Pair::new(42, 99)],
            migration_notice: None,
            announcement_channel: Some("https://example.invalid/announcements".into()),
            signature: String::new(),
            updated_at: 1_720_000_000,
        };
        let payload = community_market_signing_payload(&market);
        let signature = signing_key.sign(payload.as_bytes());
        market.signature = STANDARD.encode(signature.to_bytes());
        market
    }

    #[test]
    fn verifies_canonical_market_signature() {
        let market = signed_market();
        let payload = verify_market_signature(&market).unwrap();
        assert!(payload.starts_with("OpenNodiaCommunityMarketV1\n"));
    }

    #[test]
    fn rejects_tampered_market_signature() {
        let mut market = signed_market();
        market.name = "Changed Name".into();
        assert!(verify_market_signature(&market).is_err());
    }

    #[test]
    fn pair_response_requires_all_non_algo_assets_to_be_verified() {
        let market = CommunityMarket {
            id: "official-qat".into(),
            operator: Address::from_bytes([1u8; 32]),
            name: "Official QAT".into(),
            description: String::new(),
            logo_url: String::new(),
            asset_ids: vec![42],
            pairs: vec![Pair::new(42, 99)],
            migration_notice: None,
            announcement_channel: None,
            signature: "sig".into(),
            updated_at: 1,
        };
        let mut verification = HashMap::new();
        verification.insert(
            42,
            AssetVerificationResponse {
                asset_id: 42,
                kind: "asa".into(),
                name: "QAT".into(),
                unit: "QAT".into(),
                decimals: 0,
                creator: market.operator.to_string(),
                policy: "open".into(),
                creator_matches_operator: true,
                tradeable_by_default: true,
                official: true,
                source: "algod".into(),
                warning: None,
            },
        );

        let pair = pair_response(Pair::new(42, 99), &market, &verification);
        assert!(!pair.official);
        assert!(pair
            .warnings
            .iter()
            .any(|warning| warning.contains("not claimed")));
    }

    #[test]
    fn pair_response_rejects_creator_mismatch_as_official() {
        let market = CommunityMarket {
            id: "official-qat".into(),
            operator: Address::from_bytes([1u8; 32]),
            name: "Official QAT".into(),
            description: String::new(),
            logo_url: String::new(),
            asset_ids: vec![42],
            pairs: vec![Pair::new(0, 42)],
            migration_notice: None,
            announcement_channel: None,
            signature: "sig".into(),
            updated_at: 1,
        };
        let mut verification = HashMap::new();
        verification.insert(
            42,
            AssetVerificationResponse {
                asset_id: 42,
                kind: "asa".into(),
                name: "QAT".into(),
                unit: "QAT".into(),
                decimals: 0,
                creator: Address::from_bytes([2u8; 32]).to_string(),
                policy: "open".into(),
                creator_matches_operator: false,
                tradeable_by_default: true,
                official: false,
                source: "algod".into(),
                warning: Some("asset 42 creator does not match market operator".into()),
            },
        );

        let pair = pair_response(Pair::new(0, 42), &market, &verification);
        assert!(!pair.official);
        assert!(pair
            .warnings
            .iter()
            .any(|warning| warning.contains("creator does not match")));
    }
}
