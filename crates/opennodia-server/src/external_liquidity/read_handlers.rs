use super::*;

pub(super) async fn external_liquidity_status(
    State(state): State<AppState>,
) -> Json<ExternalLiquidityStatusResponse> {
    Json(ExternalLiquidityStatusResponse {
        network: state.config.algod.network.to_string(),
        sources: source_statuses(
            state.config.algod.network,
            state.config.external_liquidity.swap_enabled,
            state.config.external_liquidity.liquidity_enabled,
        ),
    })
}

pub(super) async fn list_external_pools(
    State(state): State<AppState>,
    Query(query): Query<ExternalPoolListQuery>,
) -> ApiResult<Json<ExternalPoolListResponse>> {
    let (algod, status, data_source) = state
        .current_read_ledger()
        .await
        .map_err(service_unavailable)?;
    let round = status.last_round.as_u64();

    let Some(asset_a) = query.asset_a else {
        return Ok(Json(ExternalPoolListResponse {
            pools: Vec::new(),
            source: data_source,
            source_round: round,
            discovery_note: Some("asset_a is required for external pool discovery".into()),
        }));
    };
    let Some(asset_b) = query.asset_b else {
        return Ok(Json(ExternalPoolListResponse {
            pools: Vec::new(),
            source: data_source,
            source_round: round,
            discovery_note: Some("asset_b is required for external pool discovery".into()),
        }));
    };
    if asset_a == asset_b {
        return Err(bad_request("asset_a and asset_b must differ"));
    }

    let sources = requested_sources(query.source.as_deref())?;
    let manifest = ExternalManifest::for_network(state.config.algod.network);
    let swap_enabled = state.config.external_liquidity.swap_enabled;
    let liquidity_enabled = state.config.external_liquidity.liquidity_enabled;
    let mut pools = Vec::new();
    let mut notes = Vec::new();

    for source in sources {
        match source {
            ExternalSource::Tinyman => {
                match read_tinyman_v2_pool_by_pair(algod, manifest, asset_a, asset_b, round).await {
                    Ok(Some(pool)) => pools.push(pool_response_with_capabilities(
                        pool.response,
                        swap_enabled,
                        liquidity_enabled,
                    )),
                    Ok(None) => {}
                    Err(error) => notes.push(format!("Tinyman unavailable: {}", api_error(&error))),
                }
            }
            ExternalSource::Pact => {
                match discover_pact_constant_product_pools(algod, manifest, asset_a, asset_b, round)
                    .await
                {
                    Ok(found) => pools.extend(found.into_iter().map(|pool| {
                        pool_response_with_capabilities(
                            pool.response,
                            swap_enabled,
                            liquidity_enabled,
                        )
                    })),
                    Err(error) => notes.push(format!("Pact unavailable: {}", api_error(&error))),
                }
            }
        }
    }

    let discovery_note = if pools.is_empty() && notes.is_empty() {
        Some("no verified external pools found for this pair".into())
    } else if notes.is_empty() {
        None
    } else {
        Some(notes.join("; "))
    };

    Ok(Json(ExternalPoolListResponse {
        pools,
        source: data_source,
        source_round: round,
        discovery_note,
    }))
}

pub(super) async fn quote_external_pool(
    State(state): State<AppState>,
    Json(req): Json<ExternalQuoteRequest>,
) -> ApiResult<Json<ExternalQuoteResponse>> {
    let source = ExternalSource::parse(&req.source)?;
    let (algod, status, data_source) = state
        .current_read_ledger()
        .await
        .map_err(service_unavailable)?;
    let round = status.last_round.as_u64();
    let manifest = ExternalManifest::for_network(state.config.algod.network);
    let pool = match source {
        ExternalSource::Tinyman => {
            let address = Address::from_str(req.pool_id.trim())
                .map_err(|error| bad_request(format!("invalid Tinyman pool address: {error}")))?;
            read_tinyman_v2_pool_by_address(algod, manifest, address, round)
                .await?
                .ok_or_else(|| bad_request("Tinyman pool was not found or is not initialized"))?
        }
        ExternalSource::Pact => {
            let app_id = req
                .pool_id
                .trim()
                .parse::<u64>()
                .map_err(|error| bad_request(format!("invalid Pact pool app ID: {error}")))?;
            read_pact_constant_product_pool(algod, manifest, app_id, round).await?
        }
    };
    if !pool.response.quote_supported || !pool.response.tradable {
        return Err(bad_request(format!(
            "{} pool is not quoteable: {}",
            pool.response.source, pool.response.status_note
        )));
    }

    let quote = quote_external_exact_in(&pool, req.asset_in, req.amount_in, req.slippage_bps)?;
    Ok(Json(ExternalQuoteResponse {
        pool: pool_response_with_capabilities(
            pool.response,
            state.config.external_liquidity.swap_enabled,
            state.config.external_liquidity.liquidity_enabled,
        ),
        quote,
        source: data_source,
    }))
}

pub(crate) async fn external_route_quote_candidates(
    state: &AppState,
    asset_in: u64,
    asset_out: u64,
    amount_in: u64,
    slippage_bps: u16,
) -> ApiResult<ExternalRouteQuoteCandidates> {
    if asset_in == asset_out {
        return Err(bad_request("asset_in and asset_out must differ"));
    }
    if amount_in == 0 {
        return Err(bad_request("amount_in must be greater than zero"));
    }

    let (algod, status, data_source) = state
        .current_read_ledger()
        .await
        .map_err(service_unavailable)?;
    let round = status.last_round.as_u64();
    let manifest = ExternalManifest::for_network(state.config.algod.network);
    let swap_enabled = state.config.external_liquidity.swap_enabled;
    let liquidity_enabled = state.config.external_liquidity.liquidity_enabled;
    let asset_a = asset_in.min(asset_out);
    let asset_b = asset_in.max(asset_out);
    let mut pools = Vec::new();
    let mut warnings = Vec::new();

    match read_tinyman_v2_pool_by_pair(algod, manifest, asset_a, asset_b, round).await {
        Ok(Some(pool)) => pools.push(pool),
        Ok(None) => {}
        Err(error) => warnings.push(format!("Tinyman unavailable: {}", api_error(&error))),
    }

    match discover_pact_constant_product_pools(algod, manifest, asset_a, asset_b, round).await {
        Ok(found) => pools.extend(found),
        Err(error) => warnings.push(format!("Pact unavailable: {}", api_error(&error))),
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for pool in pools {
        let key = (pool.response.source.clone(), pool.response.pool_id.clone());
        if !seen.insert(key) {
            continue;
        }
        if !pool.response.tradable || !pool.response.quote_supported {
            warnings.push(format!(
                "{} pool {} skipped: {}",
                pool.response.source, pool.response.pool_id, pool.response.status_note
            ));
            continue;
        }
        match quote_external_exact_in(&pool, asset_in, amount_in, slippage_bps) {
            Ok(quote) if quote.asset_out == asset_out => {
                candidates.push(ExternalRouteQuoteCandidate {
                    pool: pool_response_with_capabilities(
                        pool.response,
                        swap_enabled,
                        liquidity_enabled,
                    ),
                    quote,
                });
            }
            Ok(_) => warnings.push(format!(
                "{} pool {} returned an unexpected output asset",
                pool.response.source, pool.response.pool_id
            )),
            Err(error) => warnings.push(format!(
                "{} pool {} quote failed: {}",
                pool.response.source,
                pool.response.pool_id,
                api_error(&error)
            )),
        }
    }

    Ok(ExternalRouteQuoteCandidates {
        candidates,
        source: data_source,
        source_round: round,
        warnings,
    })
}

pub(super) async fn list_external_positions(
    State(state): State<AppState>,
    Query(query): Query<ExternalPositionQuery>,
) -> ApiResult<Json<ExternalPositionListResponse>> {
    let address = Address::from_str(query.address.trim())
        .map_err(|error| bad_request(format!("invalid position address: {error}")))?;
    let (algod, status, data_source) = state
        .current_read_ledger()
        .await
        .map_err(service_unavailable)?;
    let round = status.last_round.as_u64();
    let Some(asset_a) = query.asset_a else {
        return Ok(Json(ExternalPositionListResponse {
            address: address.to_string(),
            positions: Vec::new(),
            source: data_source,
            source_round: round,
            discovery_note: Some(
                "asset_a and asset_b are required for external LP position discovery".into(),
            ),
        }));
    };
    let Some(asset_b) = query.asset_b else {
        return Ok(Json(ExternalPositionListResponse {
            address: address.to_string(),
            positions: Vec::new(),
            source: data_source,
            source_round: round,
            discovery_note: Some(
                "asset_a and asset_b are required for external LP position discovery".into(),
            ),
        }));
    };
    if asset_a == asset_b {
        return Err(bad_request("asset_a and asset_b must differ"));
    }

    let account = fetch_account(algod, address).await?;
    let holdings: HashMap<u64, u64> = account
        .assets
        .iter()
        .filter(|holding| holding.amount > 0 && !holding.is_frozen)
        .map(|holding| (holding.asset_id, holding.amount))
        .collect();
    if holdings.is_empty() {
        return Ok(Json(ExternalPositionListResponse {
            address: address.to_string(),
            positions: Vec::new(),
            source: data_source,
            source_round: round,
            discovery_note: Some("address has no non-zero ASA holdings".into()),
        }));
    }

    let sources = requested_sources(query.source.as_deref())?;
    let manifest = ExternalManifest::for_network(state.config.algod.network);
    let swap_enabled = state.config.external_liquidity.swap_enabled;
    let liquidity_enabled = state.config.external_liquidity.liquidity_enabled;
    let mut pools = Vec::new();
    let mut notes = Vec::new();
    for source in sources {
        match source {
            ExternalSource::Tinyman => {
                match read_tinyman_v2_pool_by_pair(algod, manifest, asset_a, asset_b, round).await {
                    Ok(Some(pool)) => pools.push(pool.response),
                    Ok(None) => {}
                    Err(error) => notes.push(format!("Tinyman unavailable: {}", api_error(&error))),
                }
            }
            ExternalSource::Pact => {
                match discover_pact_constant_product_pools(algod, manifest, asset_a, asset_b, round)
                    .await
                {
                    Ok(found) => pools.extend(found.into_iter().map(|pool| pool.response)),
                    Err(error) => notes.push(format!("Pact unavailable: {}", api_error(&error))),
                }
            }
        }
    }

    let mut positions = Vec::new();
    let mut seen = HashSet::new();
    for pool in pools {
        if !pool.tradable || !seen.insert((pool.source.clone(), pool.pool_id.clone())) {
            continue;
        }
        let Some(lp_balance) = holdings.get(&pool.lp_asset_id).copied() else {
            continue;
        };
        positions.push(external_position_response(
            pool_response_with_capabilities(pool, swap_enabled, liquidity_enabled),
            lp_balance,
            liquidity_enabled,
        ));
    }

    let discovery_note = if positions.is_empty() {
        if notes.is_empty() {
            Some("no external LP positions found for this address and pair".into())
        } else {
            Some(format!(
                "no external LP positions found; {}",
                notes.join("; ")
            ))
        }
    } else if notes.is_empty() {
        None
    } else {
        Some(notes.join("; "))
    };

    Ok(Json(ExternalPositionListResponse {
        address: address.to_string(),
        positions,
        source: data_source,
        source_round: round,
        discovery_note,
    }))
}
