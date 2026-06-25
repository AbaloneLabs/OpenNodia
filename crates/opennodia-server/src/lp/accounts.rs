use opennodia_core::{Address, MicroAlgo};
use opennodia_node::{AccountInfo, AlgodClient};

use super::{bad_request, service_unavailable, ApiResult};

pub(super) async fn fetch_account(algod: &AlgodClient, address: Address) -> ApiResult<AccountInfo> {
    algod
        .account_info(&address.to_string())
        .await
        .map_err(|error| service_unavailable(format!("account lookup failed: {error}")))
}

pub(super) fn available_algo(account: &AccountInfo) -> u64 {
    account.amount.saturating_sub(account.min_balance)
}

fn require_algo(account: &AccountInfo, required_microalgo: u64) -> ApiResult<()> {
    let available = available_algo(account);
    if available < required_microalgo {
        return Err(bad_request(format!(
            "insufficient ALGO spendable balance: available {}, required {}",
            MicroAlgo(available).fmt_algo(),
            MicroAlgo(required_microalgo).fmt_algo()
        )));
    }
    Ok(())
}

fn require_asset_holding(account: &AccountInfo, asset_id: u64, context: &str) -> ApiResult<u64> {
    let holding = account
        .assets
        .iter()
        .find(|holding| holding.asset_id == asset_id)
        .ok_or_else(|| {
            bad_request(format!(
                "{context}: address is not opted in to ASA {asset_id}"
            ))
        })?;
    if holding.is_frozen {
        return Err(bad_request(format!(
            "{context}: holding for ASA {asset_id} is frozen"
        )));
    }
    Ok(holding.amount)
}

pub(super) fn require_can_send(
    account: &AccountInfo,
    asset_id: u64,
    amount: u64,
    algo_fee_microalgo: u64,
    context: &str,
) -> ApiResult<()> {
    if asset_id == 0 {
        let required = amount
            .checked_add(algo_fee_microalgo)
            .ok_or_else(|| bad_request(format!("{context}: ALGO amount plus fee is too large")))?;
        return require_algo(account, required);
    }

    require_algo(account, algo_fee_microalgo)?;
    let holding_amount = require_asset_holding(account, asset_id, context)?;
    if holding_amount < amount {
        return Err(bad_request(format!(
            "{context}: insufficient ASA {asset_id} balance, available {holding_amount}, required {amount}"
        )));
    }
    Ok(())
}

pub(super) fn require_can_receive(
    account: &AccountInfo,
    asset_id: u64,
    context: &str,
) -> ApiResult<()> {
    if asset_id == 0 {
        return Ok(());
    }
    require_asset_holding(account, asset_id, context)?;
    Ok(())
}
