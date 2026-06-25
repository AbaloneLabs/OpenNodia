use super::*;

pub(super) async fn external_swap_draft(
    algod: &AlgodClient,
    network: Network,
    fields: &ExternalSwapFields,
    trader: Address,
) -> ApiResult<(ExternalPoolState, SwapQuote, WalletTxGroup, Round)> {
    if fields.expire_rounds == 0 || fields.expire_rounds > DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS {
        return Err(bad_request(format!(
            "expire_rounds must be between 1 and {DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS}"
        )));
    }
    let pool = read_external_pool_by_id(algod, network, &fields.source, &fields.pool_id).await?;
    if !pool.response.tradable || !pool.response.quote_supported {
        return Err(bad_request(format!(
            "{} pool is not swapable: {}",
            pool.response.source, pool.response.status_note
        )));
    }
    if !pool.response.adapter_swap_supported {
        return Err(bad_request(format!(
            "{} pool is quote-only because protocol version {} is not enabled for adapter swaps",
            pool.response.source, pool.response.protocol_version
        )));
    }
    if pool.response.folks_backed {
        return Err(bad_request(
            "Folks-backed Pact pools are displayed read-only until adapter swap verification is implemented",
        ));
    }
    let quote = quote_external_exact_in(
        &pool,
        fields.asset_in,
        fields.amount_in,
        fields.slippage_bps,
    )?;
    let params = fetch_tx_params(algod)
        .await
        .map_err(|error| service_unavailable(format!("fetch transaction params: {error}")))?;
    let (params, deadline) = tx_params_with_deadline(params, fields.expire_rounds)?;
    let txs = build_external_swap_group(&pool.response, &quote, trader, &params)?;
    let group = WalletTxGroup::new(trader, txs)?;
    validate_external_swap_group(
        &pool.response,
        &quote,
        trader,
        &group,
        "external swap prepare",
    )?;
    let account = fetch_account(algod, trader).await?;
    require_can_send(
        &account,
        quote.asset_in,
        quote.amount_in,
        group.total_fee(),
        "external swap",
    )?;
    require_can_receive(&account, quote.asset_out, "external swap output")?;
    Ok((pool, quote, group, deadline))
}

pub(super) async fn external_add_liquidity_draft(
    algod: &AlgodClient,
    network: Network,
    fields: &ExternalAddLiquidityFields,
    provider: Address,
) -> ApiResult<(ExternalPoolState, AddLiquidityQuote, WalletTxGroup, Round)> {
    if fields.expire_rounds == 0 || fields.expire_rounds > DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS {
        return Err(bad_request(format!(
            "expire_rounds must be between 1 and {DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS}"
        )));
    }
    let pool = read_external_pool_by_id(algod, network, &fields.source, &fields.pool_id).await?;
    ensure_external_pool_liquidity_writable(&pool.response)?;
    let quote = quote_external_balanced_add(
        &pool.response,
        fields.amount_0,
        fields.amount_1,
        fields.slippage_bps,
    )?;
    let params = fetch_tx_params(algod)
        .await
        .map_err(|error| service_unavailable(format!("fetch transaction params: {error}")))?;
    let (params, deadline) = tx_params_with_deadline(params, fields.expire_rounds)?;
    let txs = build_external_add_group(&pool.response, &quote, provider, &params)?;
    let group = WalletTxGroup::new(provider, txs)?;
    validate_external_add_group(
        &pool.response,
        &quote,
        provider,
        &group,
        "external add liquidity prepare",
    )?;

    let account = fetch_account(algod, provider).await?;
    require_can_send(
        &account,
        pool.response.asset_0,
        quote.amount_0,
        group.total_fee(),
        "external add liquidity",
    )?;
    require_can_send(
        &account,
        pool.response.asset_1,
        quote.amount_1,
        0,
        "external add liquidity",
    )?;
    require_can_receive(
        &account,
        pool.response.lp_asset_id,
        "external add liquidity LP mint",
    )?;
    Ok((pool, quote, group, deadline))
}

pub(super) async fn external_remove_liquidity_draft(
    algod: &AlgodClient,
    network: Network,
    fields: &ExternalRemoveLiquidityFields,
    provider: Address,
) -> ApiResult<(
    ExternalPoolState,
    RemoveLiquidityQuote,
    WalletTxGroup,
    Round,
)> {
    if fields.expire_rounds == 0 || fields.expire_rounds > DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS {
        return Err(bad_request(format!(
            "expire_rounds must be between 1 and {DEFAULT_EXTERNAL_SWAP_EXPIRE_ROUNDS}"
        )));
    }
    let pool = read_external_pool_by_id(algod, network, &fields.source, &fields.pool_id).await?;
    ensure_external_pool_liquidity_writable(&pool.response)?;
    let quote = quote_external_remove(&pool.response, fields.burn_lp, fields.slippage_bps)?;
    let params = fetch_tx_params(algod)
        .await
        .map_err(|error| service_unavailable(format!("fetch transaction params: {error}")))?;
    let (params, deadline) = tx_params_with_deadline(params, fields.expire_rounds)?;
    let txs = build_external_remove_group(&pool.response, &quote, provider, &params)?;
    let group = WalletTxGroup::new(provider, txs)?;
    validate_external_remove_group(
        &pool.response,
        &quote,
        provider,
        &group,
        "external remove liquidity prepare",
    )?;

    let account = fetch_account(algod, provider).await?;
    require_can_send(
        &account,
        pool.response.lp_asset_id,
        quote.burn_lp,
        group.total_fee(),
        "external remove liquidity",
    )?;
    require_can_receive(
        &account,
        pool.response.asset_0,
        "external remove liquidity asset 0",
    )?;
    require_can_receive(
        &account,
        pool.response.asset_1,
        "external remove liquidity asset 1",
    )?;
    Ok((pool, quote, group, deadline))
}
