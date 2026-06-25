use std::str::FromStr;

use opennodia_amm::{AddLiquidityQuote, RemoveLiquidityQuote, SwapQuote};
use opennodia_core::Address;
use opennodia_swap::{
    assign_group_id, build_application_call, build_asset_transfer, build_payment,
    TransactionFields, TransactionParams, TransactionType,
};

use crate::tx_flow::WalletTxGroup;

use super::{bad_request, internal, ApiResult, ExternalPoolResponse};

const TINYMAN_ADD_LIQUIDITY_ARG: &[u8] = b"add_liquidity";
const TINYMAN_ADD_FLEXIBLE_ARG: &[u8] = b"flexible";
const TINYMAN_REMOVE_LIQUIDITY_ARG: &[u8] = b"remove_liquidity";
const PACT_ADD_LIQUIDITY_ARG: &[u8] = b"ADDLIQ";
const PACT_REMOVE_LIQUIDITY_ARG: &[u8] = b"REMLIQ";

pub(super) fn ensure_external_pool_liquidity_writable(
    pool: &ExternalPoolResponse,
) -> ApiResult<()> {
    if !pool.tradable || !pool.quote_supported {
        return Err(bad_request(format!(
            "{} pool is not active for liquidity operations: {}",
            pool.source, pool.status_note
        )));
    }
    if !pool.adapter_swap_supported {
        return Err(bad_request(format!(
            "{} pool is read-only because protocol version {} is not enabled for external liquidity writes",
            pool.source, pool.protocol_version
        )));
    }
    if pool.folks_backed {
        return Err(bad_request(
            "Folks-backed Pact pools are displayed read-only until adapter liquidity verification is implemented",
        ));
    }
    if pool.lp_asset_id == 0 {
        return Err(bad_request("external pool has no LP asset"));
    }
    Ok(())
}

pub(super) fn build_external_swap_group(
    pool: &ExternalPoolResponse,
    quote: &SwapQuote,
    trader: Address,
    params: &TransactionParams,
) -> ApiResult<Vec<TransactionFields>> {
    let pool_address = Address::from_str(&pool.app_address)
        .map_err(|error| internal(format!("invalid external pool app address: {error}")))?;
    let deposit = if quote.asset_in == 0 {
        build_payment(trader, pool_address, quote.amount_in, params)
    } else {
        build_asset_transfer(
            trader,
            pool_address,
            quote.asset_in,
            quote.amount_in,
            params,
        )
    };

    let mut app_call = match pool.source.as_str() {
        "tinyman" => {
            let mut tx = build_application_call(
                trader,
                pool.app_id,
                vec![
                    b"swap".to_vec(),
                    b"fixed-input".to_vec(),
                    quote.minimum_out.to_be_bytes().to_vec(),
                ],
                vec![pool_address],
                tinyman_foreign_assets(pool),
                Vec::new(),
                params,
            );
            tx.fee = params
                .fee
                .checked_mul(2)
                .ok_or_else(|| bad_request("Tinyman swap app-call fee overflow"))?;
            tx
        }
        "pact" => {
            let mut tx = build_application_call(
                trader,
                pool.app_id,
                vec![b"SWAP".to_vec(), quote.minimum_out.to_be_bytes().to_vec()],
                Vec::new(),
                vec![pool.asset_0, pool.asset_1],
                Vec::new(),
                params,
            );
            tx.fee = params
                .fee
                .checked_mul(2)
                .ok_or_else(|| bad_request("Pact swap app-call fee overflow"))?;
            tx
        }
        other => return Err(bad_request(format!("unsupported external source: {other}"))),
    };
    app_call.note = None;

    let mut txs = vec![deposit, app_call];
    assign_group_id(&mut txs);
    Ok(txs)
}

pub(super) fn build_external_add_group(
    pool: &ExternalPoolResponse,
    quote: &AddLiquidityQuote,
    provider: Address,
    params: &TransactionParams,
) -> ApiResult<Vec<TransactionFields>> {
    let pool_address = Address::from_str(&pool.app_address)
        .map_err(|error| internal(format!("invalid external pool app address: {error}")))?;
    let mut txs = match pool.source.as_str() {
        "tinyman" => {
            let (asset_1_id, asset_2_id) = tinyman_asset_order(pool);
            let asset_1_amount =
                amount_for_external_asset(pool, quote.amount_0, quote.amount_1, asset_1_id)?;
            let asset_2_amount =
                amount_for_external_asset(pool, quote.amount_0, quote.amount_1, asset_2_id)?;
            let mut app_call = build_application_call(
                provider,
                pool.app_id,
                vec![
                    TINYMAN_ADD_LIQUIDITY_ARG.to_vec(),
                    TINYMAN_ADD_FLEXIBLE_ARG.to_vec(),
                    quote.minimum_lp.to_be_bytes().to_vec(),
                ],
                vec![pool_address],
                vec![pool.lp_asset_id],
                Vec::new(),
                params,
            );
            app_call.fee = params
                .fee
                .checked_mul(3)
                .ok_or_else(|| bad_request("Tinyman add liquidity app-call fee overflow"))?;
            app_call.note = None;
            vec![
                build_external_asset_deposit(
                    provider,
                    pool_address,
                    asset_1_id,
                    asset_1_amount,
                    params,
                ),
                build_external_asset_deposit(
                    provider,
                    pool_address,
                    asset_2_id,
                    asset_2_amount,
                    params,
                ),
                app_call,
            ]
        }
        "pact" => {
            let mut app_call = build_application_call(
                provider,
                pool.app_id,
                vec![
                    PACT_ADD_LIQUIDITY_ARG.to_vec(),
                    quote.minimum_lp.to_be_bytes().to_vec(),
                ],
                Vec::new(),
                vec![pool.asset_0, pool.asset_1, pool.lp_asset_id],
                Vec::new(),
                params,
            );
            app_call.fee = params
                .fee
                .checked_mul(3)
                .ok_or_else(|| bad_request("Pact add liquidity app-call fee overflow"))?;
            app_call.note = None;
            vec![
                build_external_asset_deposit(
                    provider,
                    pool_address,
                    pool.asset_0,
                    quote.amount_0,
                    params,
                ),
                build_external_asset_deposit(
                    provider,
                    pool_address,
                    pool.asset_1,
                    quote.amount_1,
                    params,
                ),
                app_call,
            ]
        }
        other => return Err(bad_request(format!("unsupported external source: {other}"))),
    };
    assign_group_id(&mut txs);
    Ok(txs)
}

pub(super) fn build_external_remove_group(
    pool: &ExternalPoolResponse,
    quote: &RemoveLiquidityQuote,
    provider: Address,
    params: &TransactionParams,
) -> ApiResult<Vec<TransactionFields>> {
    let pool_address = Address::from_str(&pool.app_address)
        .map_err(|error| internal(format!("invalid external pool app address: {error}")))?;
    let mut txs = match pool.source.as_str() {
        "tinyman" => {
            let (asset_1_id, asset_2_id) = tinyman_asset_order(pool);
            let minimum_1 =
                amount_for_external_asset(pool, quote.minimum_0, quote.minimum_1, asset_1_id)?;
            let minimum_2 =
                amount_for_external_asset(pool, quote.minimum_0, quote.minimum_1, asset_2_id)?;
            let mut app_call = build_application_call(
                provider,
                pool.app_id,
                vec![
                    TINYMAN_REMOVE_LIQUIDITY_ARG.to_vec(),
                    minimum_1.to_be_bytes().to_vec(),
                    minimum_2.to_be_bytes().to_vec(),
                ],
                vec![pool_address],
                tinyman_foreign_assets(pool),
                Vec::new(),
                params,
            );
            app_call.fee = params
                .fee
                .checked_mul(3)
                .ok_or_else(|| bad_request("Tinyman remove liquidity app-call fee overflow"))?;
            app_call.note = None;
            vec![
                build_asset_transfer(
                    provider,
                    pool_address,
                    pool.lp_asset_id,
                    quote.burn_lp,
                    params,
                ),
                app_call,
            ]
        }
        "pact" => {
            let mut app_call = build_application_call(
                provider,
                pool.app_id,
                vec![
                    PACT_REMOVE_LIQUIDITY_ARG.to_vec(),
                    quote.minimum_0.to_be_bytes().to_vec(),
                    quote.minimum_1.to_be_bytes().to_vec(),
                ],
                Vec::new(),
                vec![pool.asset_0, pool.asset_1],
                Vec::new(),
                params,
            );
            app_call.fee = params
                .fee
                .checked_mul(3)
                .ok_or_else(|| bad_request("Pact remove liquidity app-call fee overflow"))?;
            app_call.note = None;
            vec![
                build_asset_transfer(
                    provider,
                    pool_address,
                    pool.lp_asset_id,
                    quote.burn_lp,
                    params,
                ),
                app_call,
            ]
        }
        other => return Err(bad_request(format!("unsupported external source: {other}"))),
    };
    assign_group_id(&mut txs);
    Ok(txs)
}

fn build_external_asset_deposit(
    sender: Address,
    receiver: Address,
    asset_id: u64,
    amount: u64,
    params: &TransactionParams,
) -> TransactionFields {
    if asset_id == 0 {
        build_payment(sender, receiver, amount, params)
    } else {
        build_asset_transfer(sender, receiver, asset_id, amount, params)
    }
}

pub(super) fn validate_external_swap_group(
    pool: &ExternalPoolResponse,
    quote: &SwapQuote,
    trader: Address,
    group: &WalletTxGroup,
    context: &str,
) -> ApiResult<()> {
    let txs = group.txs();
    validate_external_group_basics(txs, trader, 2, context, "swap")?;

    let pool_address = Address::from_str(&pool.app_address)
        .map_err(|error| internal(format!("invalid external pool address: {error}")))?;
    validate_external_deposit_tx(&txs[0], quote, pool_address, context)?;
    validate_external_app_call_tx(&txs[1], pool, quote, trader, pool_address, context)?;
    Ok(())
}

pub(super) fn validate_external_add_group(
    pool: &ExternalPoolResponse,
    quote: &AddLiquidityQuote,
    provider: Address,
    group: &WalletTxGroup,
    context: &str,
) -> ApiResult<()> {
    let txs = group.txs();
    validate_external_group_basics(txs, provider, 3, context, "add liquidity")?;
    let pool_address = Address::from_str(&pool.app_address)
        .map_err(|error| internal(format!("invalid external pool address: {error}")))?;
    match pool.source.as_str() {
        "tinyman" => {
            let (asset_1_id, asset_2_id) = tinyman_asset_order(pool);
            let asset_1_amount =
                amount_for_external_asset(pool, quote.amount_0, quote.amount_1, asset_1_id)?;
            let asset_2_amount =
                amount_for_external_asset(pool, quote.amount_0, quote.amount_1, asset_2_id)?;
            validate_external_transfer_tx(
                &txs[0],
                asset_1_id,
                asset_1_amount,
                pool_address,
                context,
            )?;
            validate_external_transfer_tx(
                &txs[1],
                asset_2_id,
                asset_2_amount,
                pool_address,
                context,
            )?;
            let expected_args = vec![
                TINYMAN_ADD_LIQUIDITY_ARG.to_vec(),
                TINYMAN_ADD_FLEXIBLE_ARG.to_vec(),
                quote.minimum_lp.to_be_bytes().to_vec(),
            ];
            validate_external_application_base(&txs[2], pool, provider, context)?;
            if txs[2].app_args != expected_args
                || txs[2].app_accounts != vec![pool_address]
                || txs[2].foreign_assets != vec![pool.lp_asset_id]
            {
                return Err(bad_request(format!(
                    "{context}: invalid Tinyman add liquidity app call fields"
                )));
            }
            validate_external_app_fee(&txs[2], txs[0].fee, 3, context)?;
        }
        "pact" => {
            validate_external_transfer_tx(
                &txs[0],
                pool.asset_0,
                quote.amount_0,
                pool_address,
                context,
            )?;
            validate_external_transfer_tx(
                &txs[1],
                pool.asset_1,
                quote.amount_1,
                pool_address,
                context,
            )?;
            let expected_args = vec![
                PACT_ADD_LIQUIDITY_ARG.to_vec(),
                quote.minimum_lp.to_be_bytes().to_vec(),
            ];
            validate_external_application_base(&txs[2], pool, provider, context)?;
            if txs[2].app_args != expected_args
                || !txs[2].app_accounts.is_empty()
                || txs[2].foreign_assets != vec![pool.asset_0, pool.asset_1, pool.lp_asset_id]
            {
                return Err(bad_request(format!(
                    "{context}: invalid Pact add liquidity app call fields"
                )));
            }
            validate_external_app_fee(&txs[2], txs[0].fee, 3, context)?;
        }
        other => {
            return Err(bad_request(format!(
                "{context}: unsupported source {other}"
            )))
        }
    }
    validate_external_app_no_extra_refs(&txs[2], context, "add liquidity")
}

pub(super) fn validate_external_remove_group(
    pool: &ExternalPoolResponse,
    quote: &RemoveLiquidityQuote,
    provider: Address,
    group: &WalletTxGroup,
    context: &str,
) -> ApiResult<()> {
    let txs = group.txs();
    validate_external_group_basics(txs, provider, 2, context, "remove liquidity")?;
    let pool_address = Address::from_str(&pool.app_address)
        .map_err(|error| internal(format!("invalid external pool address: {error}")))?;
    validate_external_transfer_tx(
        &txs[0],
        pool.lp_asset_id,
        quote.burn_lp,
        pool_address,
        context,
    )?;
    validate_external_application_base(&txs[1], pool, provider, context)?;
    match pool.source.as_str() {
        "tinyman" => {
            let (asset_1_id, asset_2_id) = tinyman_asset_order(pool);
            let minimum_1 =
                amount_for_external_asset(pool, quote.minimum_0, quote.minimum_1, asset_1_id)?;
            let minimum_2 =
                amount_for_external_asset(pool, quote.minimum_0, quote.minimum_1, asset_2_id)?;
            let expected_args = vec![
                TINYMAN_REMOVE_LIQUIDITY_ARG.to_vec(),
                minimum_1.to_be_bytes().to_vec(),
                minimum_2.to_be_bytes().to_vec(),
            ];
            if txs[1].app_args != expected_args
                || txs[1].app_accounts != vec![pool_address]
                || txs[1].foreign_assets != tinyman_foreign_assets(pool)
            {
                return Err(bad_request(format!(
                    "{context}: invalid Tinyman remove liquidity app call fields"
                )));
            }
            validate_external_app_fee(&txs[1], txs[0].fee, 3, context)?;
        }
        "pact" => {
            let expected_args = vec![
                PACT_REMOVE_LIQUIDITY_ARG.to_vec(),
                quote.minimum_0.to_be_bytes().to_vec(),
                quote.minimum_1.to_be_bytes().to_vec(),
            ];
            if txs[1].app_args != expected_args
                || !txs[1].app_accounts.is_empty()
                || txs[1].foreign_assets != vec![pool.asset_0, pool.asset_1]
            {
                return Err(bad_request(format!(
                    "{context}: invalid Pact remove liquidity app call fields"
                )));
            }
            validate_external_app_fee(&txs[1], txs[0].fee, 3, context)?;
        }
        other => {
            return Err(bad_request(format!(
                "{context}: unsupported source {other}"
            )))
        }
    }
    validate_external_app_no_extra_refs(&txs[1], context, "remove liquidity")
}

fn validate_external_group_basics(
    txs: &[TransactionFields],
    signer: Address,
    expected_len: usize,
    context: &str,
    operation: &str,
) -> ApiResult<()> {
    if txs.len() != expected_len {
        return Err(bad_request(format!(
            "{context}: {operation} group must contain {expected_len} transactions"
        )));
    }
    let group_id = txs[0]
        .group
        .ok_or_else(|| bad_request(format!("{context}: first transaction has no group id")))?;
    for tx in txs {
        if tx.group != Some(group_id) {
            return Err(bad_request(format!("{context}: group id mismatch")));
        }
        if tx.sender != signer {
            return Err(bad_request(format!("{context}: unexpected signer")));
        }
        if tx.rekey_to.is_some()
            || tx.close_remainder_to.is_some()
            || tx.asset_close_to.is_some()
            || tx.asset_sender.is_some()
        {
            return Err(bad_request(format!(
                "{context}: close, clawback, or rekey fields are not allowed"
            )));
        }
    }
    Ok(())
}

fn validate_external_deposit_tx(
    tx: &TransactionFields,
    quote: &SwapQuote,
    pool_address: Address,
    context: &str,
) -> ApiResult<()> {
    if quote.asset_in == 0 {
        if tx.ty != TransactionType::Pay
            || tx.receiver != Some(pool_address)
            || tx.amount != Some(quote.amount_in)
        {
            return Err(bad_request(format!(
                "{context}: invalid ALGO deposit transaction"
            )));
        }
        return Ok(());
    }
    if tx.ty != TransactionType::Axfer
        || tx.asset_receiver != Some(pool_address)
        || tx.xfer_asset != Some(quote.asset_in)
        || tx.asset_amount != Some(quote.amount_in)
    {
        return Err(bad_request(format!(
            "{context}: invalid ASA deposit transaction"
        )));
    }
    Ok(())
}

fn validate_external_transfer_tx(
    tx: &TransactionFields,
    asset_id: u64,
    amount: u64,
    pool_address: Address,
    context: &str,
) -> ApiResult<()> {
    if asset_id == 0 {
        if tx.ty != TransactionType::Pay
            || tx.receiver != Some(pool_address)
            || tx.amount != Some(amount)
        {
            return Err(bad_request(format!(
                "{context}: invalid ALGO transfer transaction"
            )));
        }
        return Ok(());
    }
    if tx.ty != TransactionType::Axfer
        || tx.asset_receiver != Some(pool_address)
        || tx.xfer_asset != Some(asset_id)
        || tx.asset_amount != Some(amount)
    {
        return Err(bad_request(format!(
            "{context}: invalid ASA transfer transaction"
        )));
    }
    Ok(())
}

fn validate_external_app_call_tx(
    tx: &TransactionFields,
    pool: &ExternalPoolResponse,
    quote: &SwapQuote,
    trader: Address,
    pool_address: Address,
    context: &str,
) -> ApiResult<()> {
    if tx.ty != TransactionType::Appl
        || tx.sender != trader
        || tx.app_id != Some(pool.app_id)
        || tx.on_completion != Some(opennodia_swap::OnCompletion::NoOp)
    {
        return Err(bad_request(format!("{context}: invalid application call")));
    }
    match pool.source.as_str() {
        "tinyman" => {
            let expected_args = vec![
                b"swap".to_vec(),
                b"fixed-input".to_vec(),
                quote.minimum_out.to_be_bytes().to_vec(),
            ];
            if tx.app_args != expected_args
                || tx.app_accounts != vec![pool_address]
                || tx.foreign_assets != tinyman_foreign_assets(pool)
            {
                return Err(bad_request(format!(
                    "{context}: invalid Tinyman app call fields"
                )));
            }
        }
        "pact" => {
            let expected_args = vec![b"SWAP".to_vec(), quote.minimum_out.to_be_bytes().to_vec()];
            if tx.app_args != expected_args
                || !tx.app_accounts.is_empty()
                || tx.foreign_assets != vec![pool.asset_0, pool.asset_1]
            {
                return Err(bad_request(format!(
                    "{context}: invalid Pact app call fields"
                )));
            }
        }
        other => {
            return Err(bad_request(format!(
                "{context}: unsupported source {other}"
            )))
        }
    }
    if !tx.foreign_apps.is_empty() || !tx.boxes.is_empty() {
        return Err(bad_request(format!(
            "{context}: unexpected foreign apps or boxes in external swap"
        )));
    }
    Ok(())
}

fn validate_external_application_base(
    tx: &TransactionFields,
    pool: &ExternalPoolResponse,
    signer: Address,
    context: &str,
) -> ApiResult<()> {
    if tx.ty != TransactionType::Appl
        || tx.sender != signer
        || tx.app_id != Some(pool.app_id)
        || tx.on_completion != Some(opennodia_swap::OnCompletion::NoOp)
    {
        return Err(bad_request(format!("{context}: invalid application call")));
    }
    Ok(())
}

fn validate_external_app_no_extra_refs(
    tx: &TransactionFields,
    context: &str,
    operation: &str,
) -> ApiResult<()> {
    if !tx.foreign_apps.is_empty() || !tx.boxes.is_empty() {
        return Err(bad_request(format!(
            "{context}: unexpected foreign apps or boxes in external {operation}"
        )));
    }
    Ok(())
}

fn validate_external_app_fee(
    tx: &TransactionFields,
    base_fee: u64,
    multiplier: u64,
    context: &str,
) -> ApiResult<()> {
    let expected = base_fee
        .checked_mul(multiplier)
        .ok_or_else(|| bad_request(format!("{context}: expected fee overflow")))?;
    if tx.fee != expected {
        return Err(bad_request(format!(
            "{context}: invalid external app-call fee, expected {expected}, got {}",
            tx.fee
        )));
    }
    Ok(())
}

pub(super) fn external_add_foreign_assets(pool: &ExternalPoolResponse) -> Vec<u64> {
    match pool.source.as_str() {
        "tinyman" => vec![pool.lp_asset_id],
        "pact" => vec![pool.asset_0, pool.asset_1, pool.lp_asset_id],
        _ => Vec::new(),
    }
}

pub(super) fn external_remove_foreign_assets(pool: &ExternalPoolResponse) -> Vec<u64> {
    match pool.source.as_str() {
        "tinyman" => tinyman_foreign_assets(pool),
        "pact" => vec![pool.asset_0, pool.asset_1],
        _ => Vec::new(),
    }
}

fn tinyman_foreign_assets(pool: &ExternalPoolResponse) -> Vec<u64> {
    vec![pool.asset_1, pool.asset_0]
}

fn tinyman_asset_order(pool: &ExternalPoolResponse) -> (u64, u64) {
    (pool.asset_1, pool.asset_0)
}

fn amount_for_external_asset(
    pool: &ExternalPoolResponse,
    amount_0: u64,
    amount_1: u64,
    asset_id: u64,
) -> ApiResult<u64> {
    if asset_id == pool.asset_0 {
        Ok(amount_0)
    } else if asset_id == pool.asset_1 {
        Ok(amount_1)
    } else {
        Err(bad_request(format!(
            "asset {asset_id} is not in external pool {}",
            pool.pool_id
        )))
    }
}

pub(super) fn same_external_pool(
    left: &ExternalPoolResponse,
    right: &ExternalPoolResponse,
) -> bool {
    left.source == right.source
        && left.pool_id == right.pool_id
        && left.app_id == right.app_id
        && left.app_address == right.app_address
        && left.lp_asset_id == right.lp_asset_id
        && left.asset_0 == right.asset_0
        && left.asset_1 == right.asset_1
        && left.fee_bps == right.fee_bps
        && left.protocol_version == right.protocol_version
}
