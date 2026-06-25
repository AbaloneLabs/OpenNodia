use super::*;

pub(super) async fn pool_quote(
    State(state): State<AppState>,
    Json(req): Json<QuoteRequest>,
) -> ApiResult<Json<PoolQuoteResponse>> {
    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        req.app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await?;
    ensure_pool_is_tradable(&pool, "quote")?;
    let quote = quote_exact_in(&pool, req.asset_in, req.amount_in, req.slippage_bps)
        .map_err(|error| bad_request(format!("quote failed: {error}")))?;

    Ok(Json(PoolQuoteResponse {
        pool: pool_response(&pool),
        quote,
        source,
    }))
}
