use super::drafts::{
    pool_add_draft, pool_bootstrap_draft, pool_create_draft, pool_registry_create_draft,
    pool_remove_draft, pool_setup_draft, pool_swap_draft,
};
use super::*;

pub(super) async fn prepare_registry_create(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<RegistryCreatePrepareRequest>,
) -> ApiResult<Json<RegistryCreatePrepareResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let creator = parse_address(&req.fields.creator, "creator")?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP registry create prepare");
    let (draft, tx) = pool_registry_create_draft(algod, creator).await?;
    let group = WalletTxGroup::single(creator, tx)?;
    let intent_id = store_lp_intent(
        &state,
        &session,
        &req.wallet_id,
        LpIntentAction::RegistryCreate {
            group: group.clone(),
        },
    )
    .await?;
    Ok(Json(RegistryCreatePrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        tx_bytes: group.single_tx_b64()?,
        preview: RegistryCreatePreview {
            creator: req.fields.creator,
            registry_version: 1,
            pool_approval_hash: hex::encode(draft.pool_approval_hash),
            pool_clear_hash: hex::encode(draft.pool_clear_hash),
            app_create_fee: group.total_fee(),
        },
    }))
}

pub(super) async fn prepare_pool_create(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolCreatePrepareRequest>,
) -> ApiResult<Json<PoolCreatePrepareResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool create prepare");
    let creator = parse_address(&req.fields.creator, "creator")?;
    let (pool_key, txs) = pool_create_draft(&state, algod, &req.fields).await?;
    let group = wallet_group(creator, txs)?;
    let intent_id = store_lp_intent(
        &state,
        &session,
        &req.wallet_id,
        LpIntentAction::Create {
            group: group.clone(),
            pool_key: pool_key.clone(),
        },
    )
    .await?;
    let txs = group.descriptions();
    let tx_bytes = txs
        .first()
        .map(|tx| tx.tx_bytes.clone())
        .ok_or_else(|| bad_request("pool create transaction group is empty"))?;
    Ok(Json(PoolCreatePrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        tx_bytes,
        txs,
        preview: PoolCreatePreview {
            creator: req.fields.creator,
            asset_0: pool_key.asset_0,
            asset_1: pool_key.asset_1,
            fee_bps: pool_key.fee_bps,
            pool_id: pool_key.id(),
            app_create_fee: group.total_fee(),
            registered_on_create: state.config.lp.native_registry_app_id.is_some(),
            registry_app_id: state.config.lp.native_registry_app_id,
        },
    }))
}

pub(super) async fn prepare_pool_setup(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolSetupPrepareRequest>,
) -> ApiResult<Json<PoolSetupPrepareResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool setup prepare");
    let creator = parse_address(&req.fields.creator, "creator")?;
    let (pool, draft) = pool_setup_draft(algod, &req.fields).await?;
    let group = wallet_group(creator, draft.txs.clone())?;
    let intent_id = store_lp_intent(
        &state,
        &session,
        &req.wallet_id,
        LpIntentAction::Setup {
            group: group.clone(),
            pool_before: pool.clone(),
        },
    )
    .await?;
    let setup_fee = group.total_fee();
    Ok(Json(PoolSetupPrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: PoolSetupPreview {
            creator: req.fields.creator,
            app_id: req.fields.app_id,
            app_address: Address::from_app_id(req.fields.app_id).to_string(),
            pool_id: pool.key.id(),
            funding_microalgo: req.fields.funding_microalgo,
            funding_algo: MicroAlgo(req.fields.funding_microalgo).fmt_algo(),
            setup_fee,
            foreign_assets: draft
                .txs
                .last()
                .map(|tx| tx.foreign_assets.clone())
                .unwrap_or_default(),
        },
    }))
}

pub(super) async fn prepare_pool_bootstrap(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolBootstrapPrepareRequest>,
) -> ApiResult<Json<PoolBootstrapPrepareResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool bootstrap prepare");
    let (pool, quote, draft, provider, deadline, account) =
        pool_bootstrap_draft(algod, &req.fields).await?;
    let group = wallet_group(provider, draft.txs.clone())?;
    let intent_id = store_lp_intent(
        &state,
        &session,
        &req.wallet_id,
        LpIntentAction::Bootstrap {
            group: group.clone(),
            pool_before: pool.clone(),
            quote: quote.clone(),
        },
    )
    .await?;
    Ok(Json(PoolBootstrapPrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: PoolAddLiquidityPreview {
            provider: req.fields.provider,
            app_id: req.fields.app_id,
            pool_id: pool.key.id(),
            operation: "bootstrap".into(),
            amount_0: quote.amount_0,
            amount_1: quote.amount_1,
            minted_lp: quote.minted_lp,
            minimum_lp: quote.minimum_lp,
            deadline_round: deadline.as_u64(),
            total_fee: total_fee(&draft.txs),
            atomic_group_size: draft.txs.len(),
            foreign_assets: draft_foreign_assets(&draft),
            bootstrap: Some(bootstrap_safety_preview(&pool, &quote, &draft, &account)),
        },
    }))
}

pub(super) async fn prepare_pool_add(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolAddPrepareRequest>,
) -> ApiResult<Json<PoolAddPrepareResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool add liquidity prepare");
    let (pool, quote, draft, provider, deadline) = pool_add_draft(algod, &req.fields).await?;
    let group = wallet_group(provider, draft.txs.clone())?;
    let intent_id = store_lp_intent(
        &state,
        &session,
        &req.wallet_id,
        LpIntentAction::Add {
            group: group.clone(),
            pool_before: pool.clone(),
            quote: quote.clone(),
        },
    )
    .await?;
    Ok(Json(PoolAddPrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: PoolAddLiquidityPreview {
            provider: req.fields.provider,
            app_id: req.fields.app_id,
            pool_id: pool.key.id(),
            operation: "add".into(),
            amount_0: quote.amount_0,
            amount_1: quote.amount_1,
            minted_lp: quote.minted_lp,
            minimum_lp: quote.minimum_lp,
            deadline_round: deadline.as_u64(),
            total_fee: total_fee(&draft.txs),
            atomic_group_size: draft.txs.len(),
            foreign_assets: draft_foreign_assets(&draft),
            bootstrap: None,
        },
    }))
}

pub(super) async fn prepare_pool_remove(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolRemovePrepareRequest>,
) -> ApiResult<Json<PoolRemovePrepareResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool remove liquidity prepare");
    let (pool, quote, draft, provider, deadline) = pool_remove_draft(algod, &req.fields).await?;
    let group = wallet_group(provider, draft.txs.clone())?;
    let intent_id = store_lp_intent(
        &state,
        &session,
        &req.wallet_id,
        LpIntentAction::Remove {
            group: group.clone(),
            pool_before: pool.clone(),
            quote: quote.clone(),
        },
    )
    .await?;
    Ok(Json(PoolRemovePrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: PoolRemoveLiquidityPreview {
            provider: req.fields.provider,
            app_id: req.fields.app_id,
            pool_id: pool.key.id(),
            burn_lp: quote.burn_lp,
            amount_0: quote.amount_0,
            amount_1: quote.amount_1,
            minimum_0: quote.minimum_0,
            minimum_1: quote.minimum_1,
            deadline_round: deadline.as_u64(),
            total_fee: total_fee(&draft.txs),
            foreign_assets: draft_foreign_assets(&draft),
        },
    }))
}

pub(crate) async fn prepare_pool_swap(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<PoolSwapPrepareRequest>,
) -> ApiResult<Json<PoolSwapPrepareResponse>> {
    ensure_native_amm_writes_allowed(&state)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "LP pool swap prepare");
    let (pool, quote, draft, trader, deadline) = pool_swap_draft(algod, &req.fields).await?;
    let group = wallet_group(trader, draft.txs.clone())?;
    let intent_id = store_lp_intent(
        &state,
        &session,
        &req.wallet_id,
        LpIntentAction::Swap {
            group: group.clone(),
            pool_before: pool.clone(),
            quote: quote.clone(),
        },
    )
    .await?;
    Ok(Json(PoolSwapPrepareResponse {
        intent_id,
        tx_hash: group.tx_hash().to_string(),
        txs: group.descriptions(),
        preview: PoolSwapPreview {
            trader: req.fields.trader,
            app_id: req.fields.app_id,
            pool_id: pool.key.id(),
            asset_in: quote.asset_in,
            asset_out: quote.asset_out,
            amount_in: quote.amount_in,
            amount_out: quote.amount_out,
            minimum_out: quote.minimum_out,
            fee_bps: quote.fee_bps,
            price_impact_bps: quote.price_impact_bps,
            deadline_round: deadline.as_u64(),
            total_fee: total_fee(&draft.txs),
            foreign_assets: draft_foreign_assets(&draft),
        },
    }))
}
