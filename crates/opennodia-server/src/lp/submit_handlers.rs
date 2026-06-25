use super::*;

pub(super) async fn create_registry(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<RegistryCreateSubmitRequest>,
) -> ApiResult<Json<RegistryCreateSubmitResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_lp_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let LpIntentAction::RegistryCreate { group } = intent else {
        return Err(bad_request("LP intent is not a registry create intent"));
    };
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, group.signer()).await?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP registry create submit");
    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "LP registry create",
        )
        .await?;
    let app_id = fetch_created_app_id(algod, &confirmed.txid).await?;
    Ok(Json(RegistryCreateSubmitResponse {
        txid: confirmed.txid,
        confirmed_round: confirmed.confirmed_round,
        app_id,
        app_address: Address::from_app_id(app_id).to_string(),
    }))
}

pub(super) async fn create_pool(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolCreateSubmitRequest>,
) -> ApiResult<Json<PoolCreateSubmitResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_lp_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let LpIntentAction::Create { group, pool_key } = intent else {
        return Err(bad_request("LP intent is not a pool create intent"));
    };
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, group.signer()).await?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool create submit");
    let expected_programs = compile_native_pool_programs(algod).await?;
    reject_registered_duplicate_pool(&state, &pool_key).await?;
    reject_network_duplicate_pool(&state, algod, &pool_key, &expected_programs).await?;
    if let Some(registry_app_id) = state.config.lp.native_registry_app_id {
        let registry_programs = compile_native_registry_programs(algod).await?;
        validate_native_registry_app(
            algod,
            registry_app_id,
            &registry_programs,
            &expected_programs,
        )
        .await?;
        reject_registry_duplicate_pool(algod, registry_app_id, &pool_key).await?;
    }
    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "LP pool create",
        )
        .await?;
    let txid = confirmed.txid;
    let confirmed_round = confirmed.confirmed_round;
    let app_id = fetch_created_app_id(algod, &txid).await?;
    let app_address = Address::from_app_id(app_id).to_string();
    let pool = read_pool(
        algod,
        app_id,
        pool_key.genesis_hash,
        confirmed_round,
        &expected_programs,
    )
    .await?;
    record_pool_state(&state, &pool).await;

    Ok(Json(PoolCreateSubmitResponse {
        txid,
        confirmed_round,
        app_id,
        app_address,
        pool_id: pool_key.id(),
        asset_0: pool_key.asset_0,
        asset_1: pool_key.asset_1,
        fee_bps: pool_key.fee_bps,
    }))
}

pub(super) async fn setup_pool(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolSetupSubmitRequest>,
) -> ApiResult<Json<PoolSetupSubmitResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_lp_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let LpIntentAction::Setup { group, pool_before } = intent else {
        return Err(bad_request("LP intent is not a pool setup intent"));
    };
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, group.signer()).await?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool setup submit");
    let current = require_prepared_pool_state(algod, &pool_before, "pool setup").await?;
    if current.lp_asset_id != 0 {
        return Err(bad_request("pool setup has already been completed"));
    }
    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "LP pool setup",
        )
        .await?;
    let txid = confirmed.txid;
    let confirmed_round = confirmed.confirmed_round;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        pool_before.app_id,
        genesis_hash,
        confirmed_round,
        &expected_programs,
    )
    .await?;
    if pool.lp_asset_id == 0 {
        return Err(internal(
            "pool setup confirmed but LP asset was not recorded",
        ));
    }
    record_pool_state(&state, &pool).await;
    Ok(Json(PoolSetupSubmitResponse {
        txid,
        confirmed_round,
        pool: pool_response(&pool),
    }))
}

pub(super) async fn bootstrap_pool(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolBootstrapSubmitRequest>,
) -> ApiResult<Json<PoolBootstrapSubmitResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_lp_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let LpIntentAction::Bootstrap {
        group,
        pool_before,
        quote,
    } = intent
    else {
        return Err(bad_request("LP intent is not a pool bootstrap intent"));
    };
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, group.signer()).await?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool bootstrap submit");
    let current = require_prepared_pool_state(algod, &pool_before, "pool bootstrap").await?;
    if current.lp_asset_id == 0 {
        return Err(bad_request("pool setup must be completed before bootstrap"));
    }
    if current.reserve_0 != 0 || current.reserve_1 != 0 || current.total_lp_supply != 0 {
        return Err(bad_request(
            "pool already has liquidity; refresh and prepare add liquidity instead",
        ));
    }
    let account = fetch_account(algod, group.signer()).await?;
    require_can_send(
        &account,
        current.key.asset_0,
        quote.amount_0,
        group.total_fee(),
        "bootstrap",
    )?;
    require_can_send(
        &account,
        current.key.asset_1,
        quote.amount_1,
        0,
        "bootstrap",
    )?;
    require_can_receive(&account, current.lp_asset_id, "bootstrap LP mint")?;
    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "LP pool bootstrap",
        )
        .await?;
    let txid = confirmed.txid;
    let confirmed_round = confirmed.confirmed_round;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        pool_before.app_id,
        genesis_hash,
        confirmed_round,
        &expected_programs,
    )
    .await?;
    record_pool_state(&state, &pool).await;
    Ok(Json(PoolBootstrapSubmitResponse {
        txid,
        confirmed_round,
        pool: pool_response(&pool),
        quote,
    }))
}

pub(super) async fn add_pool_liquidity(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolAddSubmitRequest>,
) -> ApiResult<Json<PoolAddSubmitResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_lp_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let LpIntentAction::Add {
        group,
        pool_before,
        quote,
    } = intent
    else {
        return Err(bad_request("LP intent is not an add liquidity intent"));
    };
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, group.signer()).await?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool add liquidity submit");
    let current = require_prepared_pool_state(algod, &pool_before, "add liquidity").await?;
    ensure_pool_is_tradable(&current, "add liquidity")?;
    let account = fetch_account(algod, group.signer()).await?;
    require_can_send(
        &account,
        current.key.asset_0,
        quote.amount_0,
        group.total_fee(),
        "add liquidity",
    )?;
    require_can_send(
        &account,
        current.key.asset_1,
        quote.amount_1,
        0,
        "add liquidity",
    )?;
    require_can_receive(&account, current.lp_asset_id, "add liquidity LP mint")?;
    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "LP pool add liquidity",
        )
        .await?;
    let txid = confirmed.txid;
    let confirmed_round = confirmed.confirmed_round;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        pool_before.app_id,
        genesis_hash,
        confirmed_round,
        &expected_programs,
    )
    .await?;
    record_pool_state(&state, &pool).await;
    Ok(Json(PoolAddSubmitResponse {
        txid,
        confirmed_round,
        pool: pool_response(&pool),
        quote,
    }))
}

pub(super) async fn remove_pool_liquidity(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolRemoveSubmitRequest>,
) -> ApiResult<Json<PoolRemoveSubmitResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_lp_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let LpIntentAction::Remove {
        group,
        pool_before,
        quote,
    } = intent
    else {
        return Err(bad_request("LP intent is not a remove liquidity intent"));
    };
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, group.signer()).await?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool remove liquidity submit");
    let current = require_prepared_pool_state(algod, &pool_before, "remove liquidity").await?;
    ensure_pool_is_tradable(&current, "remove liquidity")?;
    let account = fetch_account(algod, group.signer()).await?;
    require_can_send(
        &account,
        current.lp_asset_id,
        quote.burn_lp,
        group.total_fee(),
        "remove liquidity",
    )?;
    require_can_receive(&account, current.key.asset_0, "remove liquidity asset 0")?;
    require_can_receive(&account, current.key.asset_1, "remove liquidity asset 1")?;
    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "LP pool remove liquidity",
        )
        .await?;
    let txid = confirmed.txid;
    let confirmed_round = confirmed.confirmed_round;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        pool_before.app_id,
        genesis_hash,
        confirmed_round,
        &expected_programs,
    )
    .await?;
    record_pool_state(&state, &pool).await;
    Ok(Json(PoolRemoveSubmitResponse {
        txid,
        confirmed_round,
        pool: pool_response(&pool),
        quote,
    }))
}

pub(crate) async fn swap_pool_exact_in(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolSwapSubmitRequest>,
) -> ApiResult<Json<PoolSwapSubmitResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_lp_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let LpIntentAction::Swap {
        group,
        pool_before,
        quote,
    } = intent
    else {
        return Err(bad_request("LP intent is not a swap intent"));
    };
    tx_flow::require_wallet_address(&state, &req.wallet_id, &pin, group.signer()).await?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool swap submit");
    let current = require_prepared_pool_state(algod, &pool_before, "swap").await?;
    let account = fetch_account(algod, group.signer()).await?;
    require_can_send(
        &account,
        quote.asset_in,
        quote.amount_in,
        group.total_fee(),
        "swap",
    )?;
    require_can_receive(&account, quote.asset_out, "swap output")?;
    ensure_pool_is_tradable(&current, "swap")?;
    let confirmed = group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "LP pool swap",
        )
        .await?;
    let txid = confirmed.txid;
    let confirmed_round = confirmed.confirmed_round;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        pool_before.app_id,
        genesis_hash,
        confirmed_round,
        &expected_programs,
    )
    .await?;
    record_pool_state(&state, &pool).await;
    Ok(Json(PoolSwapSubmitResponse {
        txid,
        confirmed_round,
        pool: pool_response(&pool),
        quote,
    }))
}
