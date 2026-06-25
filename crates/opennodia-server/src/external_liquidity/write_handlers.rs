use super::drafts::{
    external_add_liquidity_draft, external_remove_liquidity_draft, external_swap_draft,
};
use super::*;

pub(crate) async fn prepare_external_swap(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<ExternalSwapPrepareRequest>,
) -> ApiResult<Json<ExternalSwapPrepareResponse>> {
    ensure_external_swaps_enabled(&state)?;
    let trader = Address::from_str(req.fields.trader.trim())
        .map_err(|error| bad_request(format!("invalid trader address: {error}")))?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "external swap prepare");
    let (pool, quote, group, deadline) =
        external_swap_draft(algod, state.config.algod.network, &req.fields, trader).await?;
    let intent_id = store_external_intent(
        &state,
        &session,
        &req.wallet_id,
        ExternalLiquidityIntentAction::Swap {
            group: group.clone(),
            pool_before: pool.response.clone(),
            quote: quote.clone(),
            slippage_bps: req.fields.slippage_bps,
        },
    )
    .await?;

    Ok(Json(ExternalSwapPrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: ExternalSwapPreview {
            source: pool.response.source.clone(),
            trader: trader.to_string(),
            pool_id: pool.response.pool_id.clone(),
            asset_in: quote.asset_in,
            asset_out: quote.asset_out,
            amount_in: quote.amount_in,
            amount_out: quote.amount_out,
            minimum_out: quote.minimum_out,
            fee_bps: quote.fee_bps,
            fee_amount_estimate: quote.fee_amount_estimate,
            price_impact_bps: quote.price_impact_bps,
            source_round: quote.source_round,
            deadline_round: deadline.as_u64(),
            total_fee: group.total_fee(),
            atomic_group_size: group.descriptions().len(),
            verification_note:
                "OpenNodia built this protocol group locally and verified app call, deposit, fee, minimum output, close, and rekey fields"
                    .into(),
        },
    }))
}

pub(crate) async fn submit_external_swap(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<ExternalSwapSubmitRequest>,
) -> ApiResult<Json<ExternalSwapSubmitResponse>> {
    ensure_external_swaps_enabled(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_external_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let ExternalLiquidityIntentAction::Swap {
        group,
        pool_before,
        quote,
        slippage_bps,
    } = intent
    else {
        return Err(bad_request(
            "external liquidity intent is not a swap intent",
        ));
    };
    let trader = Address::from_str(req.fields.trader.trim())
        .map_err(|error| bad_request(format!("invalid trader address: {error}")))?;
    if group.signer() != trader {
        return Err(bad_request(
            "external swap trader does not match prepared intent",
        ));
    }
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, trader).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "external swap submit");
    let current = read_external_pool_by_id(
        algod,
        state.config.algod.network,
        &pool_before.source,
        &pool_before.pool_id,
    )
    .await?;
    if !same_external_pool(&pool_before, &current.response) {
        return Err(bad_request(
            "external pool identity changed since prepare; refresh and prepare again",
        ));
    }
    let current_quote =
        quote_external_exact_in(&current, quote.asset_in, quote.amount_in, slippage_bps)?;
    if current_quote.amount_out < quote.minimum_out {
        return Err(bad_request(format!(
            "external quote moved below prepared minimum-out: current {}, prepared minimum {}",
            current_quote.amount_out, quote.minimum_out
        )));
    }
    let account = fetch_account(algod, trader).await?;
    require_can_send(
        &account,
        quote.asset_in,
        quote.amount_in,
        group.total_fee(),
        "external swap",
    )?;
    require_can_receive(&account, quote.asset_out, "external swap output")?;
    validate_external_swap_group(
        &current.response,
        &quote,
        trader,
        &group,
        "external swap submit",
    )?;

    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "external swap",
        )
        .await?;
    let after_account = fetch_account(algod, trader).await.ok();
    let confirmed_amount_out = after_account.as_ref().and_then(|after| {
        confirmed_asset_increase(&account, after, quote.asset_out, group.total_fee())
    });
    let asset_out_balance_after = after_account
        .as_ref()
        .map(|after| account_asset_balance(after, quote.asset_out));
    Ok(Json(ExternalSwapSubmitResponse {
        txid: confirmed.txid,
        confirmed_round: confirmed.confirmed_round,
        confirmed_amount_out,
        asset_out_balance_after,
        pool: pool_response_with_capabilities(
            current.response,
            state.config.external_liquidity.swap_enabled,
            state.config.external_liquidity.liquidity_enabled,
        ),
        quote,
    }))
}

pub(super) async fn prepare_external_add(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<ExternalAddLiquidityPrepareRequest>,
) -> ApiResult<Json<ExternalAddLiquidityPrepareResponse>> {
    ensure_external_liquidity_enabled(&state)?;
    let provider = Address::from_str(req.fields.provider.trim())
        .map_err(|error| bad_request(format!("invalid provider address: {error}")))?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "external add liquidity prepare");
    let (pool, quote, group, deadline) =
        external_add_liquidity_draft(algod, state.config.algod.network, &req.fields, provider)
            .await?;
    let intent_id = store_external_intent(
        &state,
        &session,
        &req.wallet_id,
        ExternalLiquidityIntentAction::Add {
            group: group.clone(),
            pool_before: pool.response.clone(),
            quote: quote.clone(),
            slippage_bps: req.fields.slippage_bps,
        },
    )
    .await?;

    Ok(Json(ExternalAddLiquidityPrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: ExternalAddLiquidityPreview {
            source: pool.response.source.clone(),
            provider: provider.to_string(),
            app_id: pool.response.app_id,
            pool_id: pool.response.pool_id.clone(),
            amount_0: quote.amount_0,
            amount_1: quote.amount_1,
            minted_lp: quote.minted_lp,
            minimum_lp: quote.minimum_lp,
            deadline_round: deadline.as_u64(),
            total_fee: group.total_fee(),
            atomic_group_size: group.descriptions().len(),
            foreign_assets: external_add_foreign_assets(&pool.response),
            verification_note:
                "OpenNodia built this external LP add group locally and verified deposit order, app call arguments, LP minimum, fees, close, clawback, and rekey fields"
                    .into(),
        },
    }))
}

pub(super) async fn submit_external_add(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<ExternalAddLiquiditySubmitRequest>,
) -> ApiResult<Json<ExternalAddLiquiditySubmitResponse>> {
    ensure_external_liquidity_enabled(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_external_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let ExternalLiquidityIntentAction::Add {
        group,
        pool_before,
        quote,
        slippage_bps: _slippage_bps,
    } = intent
    else {
        return Err(bad_request(
            "external liquidity intent is not an add intent",
        ));
    };
    let provider = Address::from_str(req.fields.provider.trim())
        .map_err(|error| bad_request(format!("invalid provider address: {error}")))?;
    if group.signer() != provider {
        return Err(bad_request(
            "external add liquidity provider does not match prepared intent",
        ));
    }
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, provider).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "external add liquidity submit");
    let current = read_external_pool_by_id(
        algod,
        state.config.algod.network,
        &pool_before.source,
        &pool_before.pool_id,
    )
    .await?;
    if !same_external_pool(&pool_before, &current.response) {
        return Err(bad_request(
            "external pool identity changed since prepare; refresh and prepare again",
        ));
    }
    ensure_external_pool_liquidity_writable(&current.response)?;
    let current_minted =
        external_add_minted_floor(&current.response, quote.amount_0, quote.amount_1)?;
    if current_minted < quote.minimum_lp {
        return Err(bad_request(format!(
            "external add liquidity moved below prepared minimum LP: current {}, prepared minimum {}",
            current_minted, quote.minimum_lp
        )));
    }
    let account = fetch_account(algod, provider).await?;
    require_can_send(
        &account,
        current.response.asset_0,
        quote.amount_0,
        group.total_fee(),
        "external add liquidity",
    )?;
    require_can_send(
        &account,
        current.response.asset_1,
        quote.amount_1,
        0,
        "external add liquidity",
    )?;
    require_can_receive(
        &account,
        current.response.lp_asset_id,
        "external add liquidity LP mint",
    )?;
    validate_external_add_group(
        &current.response,
        &quote,
        provider,
        &group,
        "external add liquidity submit",
    )?;

    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "external add liquidity",
        )
        .await?;
    let lp_asset_id = current.response.lp_asset_id;
    let after_account = fetch_account(algod, provider).await.ok();
    let minted_lp = after_account
        .as_ref()
        .and_then(|after| confirmed_asset_increase(&account, after, lp_asset_id, 0));
    let lp_balance_after = after_account
        .as_ref()
        .map(|after| account_asset_balance(after, lp_asset_id));
    let latest = read_external_pool_by_id(
        algod,
        state.config.algod.network,
        &pool_before.source,
        &pool_before.pool_id,
    )
    .await
    .unwrap_or(current);
    Ok(Json(ExternalAddLiquiditySubmitResponse {
        txid: confirmed.txid,
        confirmed_round: confirmed.confirmed_round,
        minted_lp,
        lp_balance_after,
        pool: pool_response_with_capabilities(
            latest.response,
            state.config.external_liquidity.swap_enabled,
            state.config.external_liquidity.liquidity_enabled,
        ),
        quote,
    }))
}

pub(super) async fn prepare_external_remove(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<ExternalRemoveLiquidityPrepareRequest>,
) -> ApiResult<Json<ExternalRemoveLiquidityPrepareResponse>> {
    ensure_external_liquidity_enabled(&state)?;
    let provider = Address::from_str(req.fields.provider.trim())
        .map_err(|error| bad_request(format!("invalid provider address: {error}")))?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "external remove liquidity prepare");
    let (pool, quote, group, deadline) =
        external_remove_liquidity_draft(algod, state.config.algod.network, &req.fields, provider)
            .await?;
    let intent_id = store_external_intent(
        &state,
        &session,
        &req.wallet_id,
        ExternalLiquidityIntentAction::Remove {
            group: group.clone(),
            pool_before: pool.response.clone(),
            quote: quote.clone(),
            slippage_bps: req.fields.slippage_bps,
        },
    )
    .await?;

    Ok(Json(ExternalRemoveLiquidityPrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: ExternalRemoveLiquidityPreview {
            source: pool.response.source.clone(),
            provider: provider.to_string(),
            app_id: pool.response.app_id,
            pool_id: pool.response.pool_id.clone(),
            burn_lp: quote.burn_lp,
            amount_0: quote.amount_0,
            amount_1: quote.amount_1,
            minimum_0: quote.minimum_0,
            minimum_1: quote.minimum_1,
            deadline_round: deadline.as_u64(),
            total_fee: group.total_fee(),
            atomic_group_size: group.descriptions().len(),
            foreign_assets: external_remove_foreign_assets(&pool.response),
            verification_note:
                "OpenNodia built this external LP remove group locally and verified LP burn deposit, app call arguments, asset minimums, fees, close, clawback, and rekey fields"
                    .into(),
        },
    }))
}

pub(super) async fn submit_external_remove(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<ExternalRemoveLiquiditySubmitRequest>,
) -> ApiResult<Json<ExternalRemoveLiquiditySubmitResponse>> {
    ensure_external_liquidity_enabled(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_external_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let ExternalLiquidityIntentAction::Remove {
        group,
        pool_before,
        quote,
        slippage_bps,
    } = intent
    else {
        return Err(bad_request(
            "external liquidity intent is not a remove intent",
        ));
    };
    let provider = Address::from_str(req.fields.provider.trim())
        .map_err(|error| bad_request(format!("invalid provider address: {error}")))?;
    if group.signer() != provider {
        return Err(bad_request(
            "external remove liquidity provider does not match prepared intent",
        ));
    }
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, provider).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "external remove liquidity submit");
    let current = read_external_pool_by_id(
        algod,
        state.config.algod.network,
        &pool_before.source,
        &pool_before.pool_id,
    )
    .await?;
    if !same_external_pool(&pool_before, &current.response) {
        return Err(bad_request(
            "external pool identity changed since prepare; refresh and prepare again",
        ));
    }
    ensure_external_pool_liquidity_writable(&current.response)?;
    let current_quote = quote_external_remove(&current.response, quote.burn_lp, slippage_bps)?;
    if current_quote.amount_0 < quote.minimum_0 || current_quote.amount_1 < quote.minimum_1 {
        return Err(bad_request(format!(
            "external remove liquidity moved below prepared minimums: current {}/{}, prepared minimum {}/{}",
            current_quote.amount_0, current_quote.amount_1, quote.minimum_0, quote.minimum_1
        )));
    }
    let account = fetch_account(algod, provider).await?;
    require_can_send(
        &account,
        current.response.lp_asset_id,
        quote.burn_lp,
        group.total_fee(),
        "external remove liquidity",
    )?;
    require_can_receive(
        &account,
        current.response.asset_0,
        "external remove liquidity asset 0",
    )?;
    require_can_receive(
        &account,
        current.response.asset_1,
        "external remove liquidity asset 1",
    )?;
    validate_external_remove_group(
        &current.response,
        &quote,
        provider,
        &group,
        "external remove liquidity submit",
    )?;

    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "external remove liquidity",
        )
        .await?;
    let asset_0 = current.response.asset_0;
    let asset_1 = current.response.asset_1;
    let lp_asset_id = current.response.lp_asset_id;
    let after_account = fetch_account(algod, provider).await.ok();
    let amount_0 = after_account
        .as_ref()
        .and_then(|after| confirmed_asset_increase(&account, after, asset_0, group.total_fee()));
    let amount_1 = after_account
        .as_ref()
        .and_then(|after| confirmed_asset_increase(&account, after, asset_1, group.total_fee()));
    let lp_balance_after = after_account
        .as_ref()
        .map(|after| account_asset_balance(after, lp_asset_id));
    let latest = read_external_pool_by_id(
        algod,
        state.config.algod.network,
        &pool_before.source,
        &pool_before.pool_id,
    )
    .await
    .unwrap_or(current);
    Ok(Json(ExternalRemoveLiquiditySubmitResponse {
        txid: confirmed.txid,
        confirmed_round: confirmed.confirmed_round,
        amount_0,
        amount_1,
        lp_balance_after,
        pool: pool_response_with_capabilities(
            latest.response,
            state.config.external_liquidity.swap_enabled,
            state.config.external_liquidity.liquidity_enabled,
        ),
        quote,
    }))
}
