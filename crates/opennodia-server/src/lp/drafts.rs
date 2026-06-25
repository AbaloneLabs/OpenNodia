use super::*;

pub(super) async fn pool_create_draft(
    state: &AppState,
    algod: &AlgodClient,
    fields: &PoolCreateFields,
) -> ApiResult<(PoolKey, Vec<TransactionFields>)> {
    let creator = parse_address(&fields.creator, "creator")?;
    let fee = FeeTier::from_bps(fields.fee_bps)
        .map_err(|error| bad_request(format!("invalid fee tier: {error}")))?;
    validate_pool_assets(algod, fields.asset_a, fields.asset_b).await?;
    let tx_params = fetch_params(algod).await?;
    let genesis_hash = tx_params.genesis_hash;
    let pool_key = PoolKey::new(
        genesis_hash,
        fields.asset_a,
        fields.asset_b,
        fee,
        opennodia_amm::CURRENT_CONTRACT_VERSION,
    )
    .map_err(|error| bad_request(format!("build pool key: {error}")))?;
    reject_registered_duplicate_pool(state, &pool_key).await?;
    let programs = compile_native_pool_programs(algod).await?;
    reject_network_duplicate_pool(state, algod, &pool_key, &programs).await?;
    let request = PoolCreateRequest {
        creator,
        genesis_hash,
        asset_a: fields.asset_a,
        asset_b: fields.asset_b,
        fee,
        approval_program: current_pool_approval_program(&programs)?.clone(),
        clear_state_program: programs.clear_state_program.clone(),
    };
    if let Some(registry_app_id) = state.config.lp.native_registry_app_id {
        let registry_programs = compile_native_registry_programs(algod).await?;
        let registry_active_count =
            validate_native_registry_app(algod, registry_app_id, &registry_programs, &programs)
                .await?;
        reject_registry_duplicate_pool(algod, registry_app_id, &pool_key).await?;
        let draft = build_registered_pool_create(
            RegisteredPoolCreateRequest {
                pool: request,
                registry_app_id,
                registry_app_address: Address::from_app_id(registry_app_id),
                registry_active_count,
            },
            &tx_params,
        )
        .map_err(|error| {
            bad_request(format!(
                "build registered pool create transaction group: {error}"
            ))
        })?;
        return Ok((draft.pool_key, draft.txs));
    }
    if state.config.lp.require_registry {
        return Err(bad_request(
            "native AMM registry is required but native_registry_app_id is not configured",
        ));
    }
    let draft = build_pool_create(request, &tx_params)
        .map_err(|error| bad_request(format!("build pool create transaction: {error}")))?;
    Ok((draft.pool_key, vec![draft.tx]))
}

pub(super) async fn pool_registry_create_draft(
    algod: &AlgodClient,
    creator: Address,
) -> ApiResult<(
    opennodia_amm::transactions::RegistryCreateDraft,
    TransactionFields,
)> {
    let tx_params = fetch_params(algod).await?;
    let genesis_hash = genesis_hash(algod).await?;
    let pool_programs = compile_native_pool_programs(algod).await?;
    let registry_programs = compile_native_registry_programs(algod).await?;
    let draft = opennodia_amm::transactions::build_registry_create(
        opennodia_amm::transactions::RegistryCreateRequest {
            creator,
            genesis_hash,
            registry_approval_program: registry_programs.approval_program,
            registry_clear_state_program: registry_programs.clear_state_program,
            pool_approval_program: current_pool_approval_program(&pool_programs)?.clone(),
            pool_clear_state_program: pool_programs.clear_state_program,
        },
        &tx_params,
    )
    .map_err(|error| bad_request(format!("build registry create transaction: {error}")))?;
    let tx = draft.tx.clone();
    Ok((draft, tx))
}

pub(super) async fn pool_setup_draft(
    algod: &AlgodClient,
    fields: &PoolSetupFields,
) -> ApiResult<(PoolState, PoolGroupDraft)> {
    let creator = parse_address(&fields.creator, "creator")?;
    let status = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod status: {error}")))?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        fields.app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await?;
    if pool.lp_asset_id != 0 {
        return Err(bad_request("pool setup has already been completed"));
    }
    let tx_params = fetch_params(algod).await?;
    let draft = build_pool_setup(
        PoolSetupRequest {
            creator,
            app_id: fields.app_id,
            app_address: Address::from_app_id(fields.app_id),
            pool_key: pool.key.clone(),
            funding_microalgo: fields.funding_microalgo,
        },
        &tx_params,
    )
    .map_err(|error| bad_request(format!("build pool setup transaction group: {error}")))?;
    Ok((pool, draft))
}

pub(super) async fn pool_bootstrap_draft(
    algod: &AlgodClient,
    fields: &PoolBootstrapFields,
) -> ApiResult<(
    PoolState,
    AddLiquidityQuote,
    PoolGroupDraft,
    Address,
    Round,
    AccountInfo,
)> {
    let provider = parse_address(&fields.provider, "provider")?;
    let status = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod status: {error}")))?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        fields.app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await?;
    if pool.lp_asset_id == 0 {
        return Err(bad_request("pool setup must be completed before bootstrap"));
    }
    if pool.reserve_0 != 0 || pool.reserve_1 != 0 || pool.total_lp_supply != 0 {
        return Err(bad_request(
            "pool already has liquidity; use add liquidity instead",
        ));
    }
    let quote = quote_initial_liquidity(fields.amount_0, fields.amount_1, fields.slippage_bps)
        .map_err(|error| bad_request(format!("bootstrap quote failed: {error}")))?;
    let tx_params = fetch_params(algod).await?;
    let (tx_params, deadline) = tx_params_with_deadline(tx_params, fields.expire_rounds)?;
    let draft = build_pool_bootstrap(
        BootstrapRequest {
            provider,
            app_id: fields.app_id,
            app_address: Address::from_app_id(fields.app_id),
            pool_key: pool.key.clone(),
            lp_asset_id: pool.lp_asset_id,
            amount_0: quote.amount_0,
            amount_1: quote.amount_1,
            minimum_lp: quote.minimum_lp,
            deadline,
        },
        &tx_params,
    )
    .map_err(|error| bad_request(format!("build pool bootstrap transaction group: {error}")))?;

    let account = fetch_account(algod, provider).await?;
    let fee = total_fee(&draft.txs);
    require_can_send(&account, pool.key.asset_0, quote.amount_0, fee, "bootstrap")?;
    require_can_send(&account, pool.key.asset_1, quote.amount_1, 0, "bootstrap")?;
    require_can_receive(&account, pool.lp_asset_id, "bootstrap LP mint")?;
    Ok((pool, quote, draft, provider, deadline, account))
}

pub(super) async fn pool_add_draft(
    algod: &AlgodClient,
    fields: &PoolAddFields,
) -> ApiResult<(PoolState, AddLiquidityQuote, PoolGroupDraft, Address, Round)> {
    let provider = parse_address(&fields.provider, "provider")?;
    let status = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod status: {error}")))?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        fields.app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await?;
    if pool.lp_asset_id == 0 {
        return Err(bad_request(
            "pool setup must be completed before adding liquidity",
        ));
    }
    ensure_pool_is_tradable(&pool, "add liquidity")?;
    let quote = quote_balanced_add(
        &pool,
        fields.desired_0,
        fields.desired_1,
        fields.slippage_bps,
    )
    .map_err(|error| bad_request(format!("add liquidity quote failed: {error}")))?;
    let tx_params = fetch_params(algod).await?;
    let (tx_params, deadline) = tx_params_with_deadline(tx_params, fields.expire_rounds)?;
    let draft = build_pool_add_liquidity(
        AddLiquidityRequest {
            provider,
            app_id: fields.app_id,
            app_address: Address::from_app_id(fields.app_id),
            pool_key: pool.key.clone(),
            lp_asset_id: pool.lp_asset_id,
            amount_0: quote.amount_0,
            amount_1: quote.amount_1,
            minimum_lp: quote.minimum_lp,
            deadline,
        },
        &tx_params,
    )
    .map_err(|error| bad_request(format!("build add liquidity transaction group: {error}")))?;

    let account = fetch_account(algod, provider).await?;
    let fee = total_fee(&draft.txs);
    require_can_send(
        &account,
        pool.key.asset_0,
        quote.amount_0,
        fee,
        "add liquidity",
    )?;
    require_can_send(
        &account,
        pool.key.asset_1,
        quote.amount_1,
        0,
        "add liquidity",
    )?;
    require_can_receive(&account, pool.lp_asset_id, "add liquidity LP mint")?;
    Ok((pool, quote, draft, provider, deadline))
}

pub(super) async fn pool_remove_draft(
    algod: &AlgodClient,
    fields: &PoolRemoveFields,
) -> ApiResult<(
    PoolState,
    RemoveLiquidityQuote,
    PoolGroupDraft,
    Address,
    Round,
)> {
    let provider = parse_address(&fields.provider, "provider")?;
    let status = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod status: {error}")))?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        fields.app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await?;
    if pool.lp_asset_id == 0 {
        return Err(bad_request(
            "pool setup must be completed before removing liquidity",
        ));
    }
    ensure_pool_is_tradable(&pool, "remove liquidity")?;
    let quote = quote_remove(&pool, fields.burn_lp, fields.slippage_bps)
        .map_err(|error| bad_request(format!("remove liquidity quote failed: {error}")))?;
    let tx_params = fetch_params(algod).await?;
    let (tx_params, deadline) = tx_params_with_deadline(tx_params, fields.expire_rounds)?;
    let draft = build_pool_remove_liquidity(
        RemoveLiquidityRequest {
            provider,
            app_id: fields.app_id,
            app_address: Address::from_app_id(fields.app_id),
            pool_key: pool.key.clone(),
            lp_asset_id: pool.lp_asset_id,
            burn_lp: quote.burn_lp,
            minimum_0: quote.minimum_0,
            minimum_1: quote.minimum_1,
            deadline,
        },
        &tx_params,
    )
    .map_err(|error| bad_request(format!("build remove liquidity transaction group: {error}")))?;

    let account = fetch_account(algod, provider).await?;
    let fee = total_fee(&draft.txs);
    require_can_send(
        &account,
        pool.lp_asset_id,
        quote.burn_lp,
        fee,
        "remove liquidity",
    )?;
    require_can_receive(&account, pool.key.asset_0, "remove liquidity asset 0")?;
    require_can_receive(&account, pool.key.asset_1, "remove liquidity asset 1")?;
    Ok((pool, quote, draft, provider, deadline))
}

pub(crate) async fn pool_swap_draft(
    algod: &AlgodClient,
    fields: &PoolSwapFields,
) -> ApiResult<(PoolState, SwapQuote, PoolGroupDraft, Address, Round)> {
    let trader = parse_address(&fields.trader, "trader")?;
    let status = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod status: {error}")))?;
    let genesis_hash = genesis_hash(algod).await?;
    let expected_programs = compile_native_pool_programs(algod).await?;
    let pool = read_pool(
        algod,
        fields.app_id,
        genesis_hash,
        status.last_round.as_u64(),
        &expected_programs,
    )
    .await?;
    ensure_pool_is_tradable(&pool, "swap")?;
    let quote = quote_exact_in(
        &pool,
        fields.asset_in,
        fields.amount_in,
        fields.slippage_bps,
    )
    .map_err(|error| bad_request(format!("swap quote failed: {error}")))?;
    let tx_params = fetch_params(algod).await?;
    let (tx_params, deadline) = tx_params_with_deadline(tx_params, fields.expire_rounds)?;
    let draft = build_pool_swap(
        SwapRequest {
            trader,
            app_id: fields.app_id,
            app_address: Address::from_app_id(fields.app_id),
            pool_key: pool.key.clone(),
            asset_in: quote.asset_in,
            amount_in: quote.amount_in,
            minimum_out: quote.minimum_out,
            deadline,
        },
        &tx_params,
    )
    .map_err(|error| bad_request(format!("build swap transaction group: {error}")))?;

    let account = fetch_account(algod, trader).await?;
    let fee = total_fee(&draft.txs);
    require_can_send(&account, quote.asset_in, quote.amount_in, fee, "swap")?;
    require_can_receive(&account, quote.asset_out, "swap output")?;
    Ok((pool, quote, draft, trader, deadline))
}
