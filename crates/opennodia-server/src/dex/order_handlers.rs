use axum::extract::{Extension, State};
use axum::Json;
use opennodia_core::{Address, Round};
use opennodia_dex::types::{EntryStatus, OrderEntry};
use opennodia_node::AlgodClient;
use opennodia_swap::{
    build_deposit_group, build_fill_group, verify_escrow, EscrowAccount, EscrowKind, EscrowParams,
    OrderSide, TransactionParams,
};

use crate::routes::verify_pin;
use crate::session::Session;
use crate::state::AppState;

use super::{
    bad_request, canonical_escrow_from_entry, confirmed_trade, describe_tx, fetch_params, internal,
    not_found, parse_address, parse_kind, register_order, reject_escrow_regulated_assets,
    reject_regulated_asset, require_dex, require_dex_write, require_wallet_ownership,
    service_unavailable, store_intent, take_intent, unix_timestamp, ApiResult, CreateIntentOrder,
    DexIntentAction, OrderResponse, PrepareCancelRequest, PrepareCancelResponse,
    PrepareCreateRequest, PrepareCreateResponse, PrepareFillRequest, PrepareFillResponse,
    SubmitCancelRequest, SubmitCancelResponse, SubmitCreateRequest, SubmitCreateResponse,
    SubmitFillRequest, SubmitFillResponse,
};

pub(super) async fn prepare_create_orders_from_plans(
    algod: &AlgodClient,
    tx_params: &TransactionParams,
    plans: Vec<opennodia_swap::matching::EscrowDepositPlan>,
) -> ApiResult<Vec<CreateIntentOrder>> {
    let mut prepared = Vec::with_capacity(plans.len());
    for (index, plan) in plans.into_iter().enumerate() {
        let escrow = EscrowAccount::compile(algod, plan.kind, plan.params)
            .await
            .map_err(|error| service_unavailable(format!("compile remainder escrow: {error}")))?;
        let result = build_deposit_group(&escrow, tx_params)
            .map_err(|error| bad_request(format!("build remainder deposit group: {error}")))?;
        prepared.push((index as u32, escrow, result));
    }

    let parent_id = if prepared.len() > 1 {
        prepared
            .first()
            .map(|(_, escrow, _)| escrow.address.to_string())
    } else {
        None
    };
    Ok(prepared
        .into_iter()
        .map(|(split_index, escrow, result)| CreateIntentOrder {
            escrow,
            result,
            split_index,
            parent_id: parent_id.clone(),
        })
        .collect())
}

/// `POST /api/dex/prepare/create` — build a deposit group for a new order.
pub(super) async fn prepare_create(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<PrepareCreateRequest>,
) -> ApiResult<Json<PrepareCreateResponse>> {
    require_dex_write(&state)?;
    let _db = require_dex(&state)?;
    let owner = parse_address(&req.signer)?;
    let kind = parse_kind(&req.side)?;
    let split_count = req.split_count.max(1);
    if split_count > opennodia_swap::matching::MAX_SPLITS {
        return Err(bad_request(format!(
            "split_count must be between 1 and {}",
            opennodia_swap::matching::MAX_SPLITS
        )));
    }
    if u64::from(split_count) > req.sell_amount || u64::from(split_count) > req.buy_amount {
        return Err(bad_request(
            "split_count cannot exceed either raw order amount; every child escrow must have non-zero sell and buy amounts",
        ));
    }
    // Allow as few as 3 rounds (~10s on mainnet) so callers can express an
    // "instant" expiry preset. The on-chain guard uses txn FirstValid, so a
    // short window no longer risks being permanently unfillable.
    if !(3..=1_000_000).contains(&req.expire_rounds) {
        return Err(bad_request("expire_rounds must be between 3 and 1000000"));
    }

    if req.sell_asset_id != 0 {
        reject_regulated_asset(&state, req.sell_asset_id).await?;
    }
    if req.buy_asset_id != 0 {
        reject_regulated_asset(&state, req.buy_asset_id).await?;
    }

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "dex prepare create");
    let params = fetch_params(algod).await?;
    let expire_round = params
        .first_valid
        .as_u64()
        .checked_add(req.expire_rounds)
        .ok_or_else(|| bad_request("order expiry overflow"))?;
    let sell_amounts = opennodia_swap::split_amounts(req.sell_amount, split_count);
    let buy_amounts = opennodia_swap::split_amounts(req.buy_amount, split_count);
    let mut prepared = Vec::with_capacity(split_count as usize);
    for (index, (sell_amount, buy_amount)) in sell_amounts.into_iter().zip(buy_amounts).enumerate()
    {
        let escrow_params = EscrowParams::new(
            owner,
            req.sell_asset_id,
            sell_amount,
            req.buy_asset_id,
            buy_amount,
            expire_round,
        );
        opennodia_swap::validate_params(kind, &escrow_params)
            .map_err(|error| bad_request(error.to_string()))?;
        let escrow = EscrowAccount::compile(algod, kind, escrow_params)
            .await
            .map_err(|error| service_unavailable(format!("compile escrow: {error}")))?;
        let result = build_deposit_group(&escrow, &params)
            .map_err(|error| bad_request(format!("build deposit group: {error}")))?;
        prepared.push((index as u32, escrow, result));
    }
    let escrow_addresses: Vec<String> = prepared
        .iter()
        .map(|(_, escrow, _)| escrow.address.to_string())
        .collect();
    let parent_id = if split_count > 1 {
        escrow_addresses.first().cloned()
    } else {
        None
    };
    let orders: Vec<CreateIntentOrder> = prepared
        .into_iter()
        .map(|(split_index, escrow, result)| CreateIntentOrder {
            escrow,
            result,
            split_index,
            parent_id: parent_id.clone(),
        })
        .collect();
    let intent_id = store_intent(
        &state,
        &session,
        &req.wallet_id,
        DexIntentAction::Create {
            orders: orders.clone(),
        },
    )
    .await?;

    let owner_label = owner.to_string();
    let owner_txs = orders
        .iter()
        .flat_map(|order| order.result.owner_txs.iter())
        .map(|tx| describe_tx(tx, &owner_label))
        .collect();
    let logicsig_txs = orders
        .iter()
        .flat_map(|order| {
            let escrow_label = order.escrow.address.to_string();
            order
                .result
                .logicsig_txs
                .iter()
                .map(move |tx| describe_tx(tx, &escrow_label))
        })
        .collect();

    Ok(Json(PrepareCreateResponse {
        intent_id,
        escrow_address: escrow_addresses.first().cloned().unwrap_or_default(),
        escrow_addresses,
        split_count,
        kind: req.side.clone(),
        owner_txs,
        logicsig_txs,
    }))
}

pub(super) async fn submit_prepared_create_order(
    state: &AppState,
    db: &opennodia_dex::DexDb,
    algod: &AlgodClient,
    wallet_id: &str,
    pin: &str,
    owner: Address,
    order: CreateIntentOrder,
) -> ApiResult<(String, u64, OrderResponse)> {
    let CreateIntentOrder {
        escrow,
        result,
        split_index,
        parent_id,
    } = order;
    if escrow.params.owner != owner {
        return Err(internal("prepared split order owners do not match"));
    }
    if escrow.params.sell_asset != 0 {
        reject_regulated_asset(state, escrow.params.sell_asset).await?;
    }
    if escrow.params.buy_asset != 0 {
        reject_regulated_asset(state, escrow.params.buy_asset).await?;
    }

    let mut owner_signed = Vec::with_capacity(result.owner_txs.len());
    for transaction in &result.owner_txs {
        let unsigned = opennodia_swap::encode_transaction(transaction);
        let signed = state
            .stores
            .wallets
            .sign_transaction(wallet_id, pin, &owner.to_string(), &unsigned)
            .await
            .map_err(|error| internal(format!("sign deposit transaction: {error}")))?;
        owner_signed.push(signed);
    }

    let mut group_bytes = Vec::new();
    let first = owner_signed
        .first()
        .ok_or_else(|| internal("prepared deposit group has no owner transaction"))?;
    group_bytes.extend_from_slice(first);
    if let Some(opt_in) = result.logicsig_txs.first() {
        group_bytes.extend_from_slice(&opennodia_swap::sign_with_logicsig(
            opt_in.clone(),
            escrow.program.clone(),
        ));
    }
    for signed in owner_signed.iter().skip(1) {
        group_bytes.extend_from_slice(signed);
    }

    let txid = opennodia_swap::submit_signed_tx(algod, &group_bytes)
        .await
        .map_err(|error| internal(format!("submit create group: {error}")))?;
    let confirmed = opennodia_swap::wait_for_confirmation(algod, &txid, 20)
        .await
        .map_err(|error| internal(format!("create confirmation: {error}")))?;
    let verification = verify_escrow(algod, None, &escrow, Round(confirmed))
        .await
        .map_err(|error| internal(format!("verify confirmed escrow: {error}")))?;
    if !verification.valid {
        return Err(internal(format!(
            "deposit confirmed but escrow verification failed: {}",
            verification.mismatch_reason
        )));
    }

    register_order(
        db,
        &escrow,
        Round(confirmed),
        split_index,
        parent_id.clone(),
    )?;
    let side = match escrow.kind {
        EscrowKind::Sell => OrderSide::Sell,
        EscrowKind::Buy => OrderSide::Buy,
    };
    let response = OrderResponse::from(OrderEntry {
        escrow_addr: escrow.address,
        side,
        sell_asset: escrow.params.sell_asset,
        sell_amount: escrow.params.sell_amount,
        buy_asset: escrow.params.buy_asset,
        buy_amount: escrow.params.buy_amount,
        price: opennodia_dex::types::order_price(
            side,
            escrow.params.sell_asset,
            escrow.params.sell_amount,
            escrow.params.buy_asset,
            escrow.params.buy_amount,
        )
        .ok_or_else(|| internal("confirmed order price cannot be normalized"))?,
        owner: escrow.params.owner,
        created_round: Round(confirmed),
        expire_round: Round(escrow.params.expire_round),
        status: EntryStatus::Active,
        filled_amount: 0,
        split_index,
        parent_id,
        program: escrow.program.clone(),
        params: escrow.params.clone(),
    });
    Ok((txid, confirmed, response))
}

/// `POST /api/dex/submit/create` — sign and submit the prepared deposit group.
pub(super) async fn submit_create(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<SubmitCreateRequest>,
) -> ApiResult<Json<SubmitCreateResponse>> {
    require_dex_write(&state)?;
    let db = require_dex(&state)?;
    let action = take_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let DexIntentAction::Create { orders } = action else {
        return Err(bad_request("DEX intent is not a create intent"));
    };
    if orders.is_empty() {
        return Err(bad_request("DEX create intent has no prepared orders"));
    }
    let pin = verify_pin(&state, &req.pin).await?;
    let owner = orders[0].escrow.params.owner;
    require_wallet_ownership(&state, &req.wallet_id, &pin, owner).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "dex submit create");
    let mut tx_ids = Vec::with_capacity(orders.len());
    let mut confirmed_round = 0u64;
    let mut responses = Vec::with_capacity(orders.len());
    for order in orders {
        let (txid, confirmed, response) =
            submit_prepared_create_order(&state, &db, algod, &req.wallet_id, &pin, owner, order)
                .await?;
        tx_ids.push(txid);
        responses.push(response);
        confirmed_round = confirmed_round.max(confirmed);
    }

    Ok(Json(SubmitCreateResponse {
        tx_ids,
        confirmed_round,
        orders: responses,
    }))
}

/// `POST /api/dex/prepare/fill` — build and retain an exact fill group.
pub(super) async fn prepare_fill(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<PrepareFillRequest>,
) -> ApiResult<Json<PrepareFillResponse>> {
    require_dex_write(&state)?;
    let db = require_dex(&state)?;
    let filler = parse_address(&req.filler)?;
    let escrow_address = parse_address(&req.escrow_address)?;
    let entry = db
        .get_order(&escrow_address)
        .map_err(|error| internal(format!("load order: {error}")))?
        .ok_or_else(|| not_found("order not found"))?;
    if !entry.status.is_active() {
        return Err(bad_request("order is not active"));
    }
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "dex prepare fill");
    let escrow = canonical_escrow_from_entry(algod, &entry).await?;
    if filler == escrow.params.owner {
        return Err(bad_request("order owner cannot fill the same escrow"));
    }
    reject_escrow_regulated_assets(&state, &escrow).await?;
    let params = fetch_params(algod).await?;
    // Defensive pre-check: reject expired orders early with a clear message before
    // spending a verification round-trip. The in-contract `FirstValid` guard is the
    // authoritative enforcement; this mirrors it off-chain for UX.
    if !opennodia_swap::fill_allowed(&escrow, params.first_valid) {
        return Err(bad_request(format!(
            "order expired: current round {} > expire round {}",
            params.first_valid.as_u64(),
            escrow.params.expire_round
        )));
    }
    let verification = verify_escrow(algod, None, &escrow, params.first_valid)
        .await
        .map_err(|error| service_unavailable(format!("verify escrow: {error}")))?;
    if !verification.valid {
        return Err(bad_request(format!(
            "escrow not fillable: {}",
            verification.mismatch_reason
        )));
    }

    let lease = opennodia_swap::derive_lease(filler, escrow.address);
    let result = build_fill_group(&escrow, filler, lease, &params)
        .map_err(|error| bad_request(format!("build fill group: {error}")))?;
    let intent_id = store_intent(
        &state,
        &session,
        &req.wallet_id,
        DexIntentAction::Fill {
            escrow,
            filler,
            result: result.clone(),
        },
    )
    .await?;

    Ok(Json(PrepareFillResponse {
        intent_id,
        filler_tx: describe_tx(&result.filler_tx, &filler.to_string()),
        escrow_verified: true,
        verification: verification.into(),
    }))
}

/// `POST /api/dex/submit/fill` — sign and submit the exact prepared fill group.
pub(super) async fn submit_fill(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<SubmitFillRequest>,
) -> ApiResult<Json<SubmitFillResponse>> {
    require_dex_write(&state)?;
    let db = require_dex(&state)?;
    let action = take_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let DexIntentAction::Fill {
        escrow,
        filler,
        result,
    } = action
    else {
        return Err(bad_request("DEX intent is not a fill intent"));
    };
    let pin = verify_pin(&state, &req.pin).await?;
    require_wallet_ownership(&state, &req.wallet_id, &pin, filler).await?;
    reject_escrow_regulated_assets(&state, &escrow).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "dex submit fill");
    let status = algod
        .status()
        .await
        .map_err(|error| service_unavailable(format!("node status: {error}")))?;
    let verification = verify_escrow(algod, None, &escrow, status.last_round)
        .await
        .map_err(|error| service_unavailable(format!("verify escrow: {error}")))?;
    if !verification.valid {
        return Err(bad_request(format!(
            "escrow state changed since prepare: {}",
            verification.mismatch_reason
        )));
    }

    let filler_signed = state
        .stores
        .wallets
        .sign_transaction(
            &req.wallet_id,
            &pin,
            &filler.to_string(),
            &opennodia_swap::encode_transaction(&result.filler_tx),
        )
        .await
        .map_err(|error| internal(format!("sign fill transaction: {error}")))?;
    let mut group_bytes = filler_signed;
    for transaction in &result.escrow_txs {
        group_bytes.extend_from_slice(&opennodia_swap::sign_with_logicsig(
            transaction.clone(),
            escrow.program.clone(),
        ));
    }

    let txid = opennodia_swap::submit_signed_tx(algod, &group_bytes)
        .await
        .map_err(|error| internal(format!("submit fill group: {error}")))?;
    let confirmed = opennodia_swap::wait_for_confirmation(algod, &txid, 20)
        .await
        .map_err(|error| internal(format!("fill confirmation: {error}")))?;

    let trade = confirmed_trade(&escrow, filler, txid.clone(), confirmed, unix_timestamp())?;
    let record_error = db
        .record_fill(&escrow.address, escrow.params.sell_amount, &trade)
        .err()
        .map(|error| error.to_string());
    if let Some(error) = record_error.as_deref() {
        tracing::error!(
            escrow = %escrow.address,
            tx_id = %txid,
            %error,
            "confirmed fill requires reconciliation"
        );
    }

    Ok(Json(SubmitFillResponse {
        tx_id: txid,
        confirmed_round: confirmed,
        recorded: record_error.is_none(),
        record_error,
    }))
}

/// `POST /api/dex/prepare/cancel` — build an owner-authorized cancel group.
pub(super) async fn prepare_cancel(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<PrepareCancelRequest>,
) -> ApiResult<Json<PrepareCancelResponse>> {
    require_dex_write(&state)?;
    let db = require_dex(&state)?;
    let address = parse_address(&req.escrow_address)?;
    let entry = db
        .get_order(&address)
        .map_err(|error| internal(format!("load order: {error}")))?
        .ok_or_else(|| not_found("order not found"))?;
    if !matches!(entry.status, EntryStatus::Active | EntryStatus::Expired) {
        return Err(bad_request(
            "order cannot be cancelled in its current state",
        ));
    }
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "dex prepare cancel");
    let escrow = canonical_escrow_from_entry(algod, &entry).await?;
    let params = fetch_params(algod).await?;
    let result = opennodia_swap::build_cancel_group(&escrow, &params)
        .map_err(|error| bad_request(format!("build cancel group: {error}")))?;
    let intent_id = store_intent(
        &state,
        &session,
        &req.wallet_id,
        DexIntentAction::Cancel {
            escrow: escrow.clone(),
            result: result.clone(),
        },
    )
    .await?;
    let escrow_label = escrow.address.to_string();
    let escrow_txs = result
        .escrow_txs
        .iter()
        .map(|transaction| describe_tx(transaction, &escrow_label))
        .collect();

    Ok(Json(PrepareCancelResponse {
        intent_id,
        owner_auth_tx: describe_tx(&result.owner_auth_tx, &escrow.params.owner.to_string()),
        escrow_txs,
        recoverable_algo: result.recoverable_algo,
        recoverable_asset: result.recoverable_asset,
    }))
}

/// `POST /api/dex/submit/cancel` — sign and submit the owner-authorized group.
pub(super) async fn submit_cancel(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Json(req): Json<SubmitCancelRequest>,
) -> ApiResult<Json<SubmitCancelResponse>> {
    require_dex_write(&state)?;
    let db = require_dex(&state)?;
    let action = take_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    let DexIntentAction::Cancel { escrow, result } = action else {
        return Err(bad_request("DEX intent is not a cancel intent"));
    };
    let pin = verify_pin(&state, &req.pin).await?;
    require_wallet_ownership(&state, &req.wallet_id, &pin, escrow.params.owner).await?;
    let owner_signed = state
        .stores
        .wallets
        .sign_transaction(
            &req.wallet_id,
            &pin,
            &escrow.params.owner.to_string(),
            &opennodia_swap::encode_transaction(&result.owner_auth_tx),
        )
        .await
        .map_err(|error| internal(format!("sign cancel authorization: {error}")))?;
    let mut group_bytes = owner_signed;
    for transaction in &result.escrow_txs {
        group_bytes.extend_from_slice(&opennodia_swap::sign_with_logicsig(
            transaction.clone(),
            escrow.program.clone(),
        ));
    }

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "dex submit cancel");
    let txid = opennodia_swap::submit_signed_tx(algod, &group_bytes)
        .await
        .map_err(|error| internal(format!("submit cancel group: {error}")))?;
    let confirmed = opennodia_swap::wait_for_confirmation(algod, &txid, 20)
        .await
        .map_err(|error| internal(format!("cancel confirmation: {error}")))?;
    let record_error = db
        .record_cancel(&escrow.address, &txid, Round(confirmed))
        .err()
        .map(|error| error.to_string());
    if let Some(error) = record_error.as_deref() {
        tracing::error!(
            escrow = %escrow.address,
            tx_id = %txid,
            %error,
            "confirmed cancellation requires reconciliation"
        );
    }
    let recovered = result
        .recoverable_asset
        .map(|(_, amount)| amount)
        .unwrap_or(result.recoverable_algo);

    Ok(Json(SubmitCancelResponse {
        tx_id: txid,
        confirmed_round: confirmed,
        recovered_amount: recovered,
        recorded: record_error.is_none(),
        record_error,
    }))
}
