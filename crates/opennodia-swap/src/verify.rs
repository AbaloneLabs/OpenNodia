//! On-chain escrow state verification.

use opennodia_core::{Address, Round};
use opennodia_node::algod::{AlgodClient, DataSource};
use opennodia_node::asset::AccountInfo;

use crate::escrow::{
    escrow_address, EscrowAccount, BASE_ESCROW_FUNDING_MICROALGO, MIN_ESCROW_FUNDING_MICROALGO,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderVerification {
    pub valid: bool,
    pub actual_balance: u64,
    pub expected_balance: u64,
    pub actual_asset_amount: u64,
    pub expected_asset_amount: u64,
    pub expired: bool,
    pub mismatch_reason: String,
    pub data_source: DataSource,
}

impl OrderVerification {
    pub fn failed(reason: impl Into<String>, data_source: DataSource) -> Self {
        Self {
            valid: false,
            actual_balance: 0,
            expected_balance: 0,
            actual_asset_amount: 0,
            expected_asset_amount: 0,
            expired: false,
            mismatch_reason: reason.into(),
            data_source,
        }
    }
}

pub async fn verify_escrow(
    algod: &AlgodClient,
    public: Option<&AlgodClient>,
    escrow: &EscrowAccount,
    current_round: Round,
) -> opennodia_core::Result<OrderVerification> {
    if escrow_address(&escrow.program) != escrow.address {
        return Ok(OrderVerification::failed(
            "escrow program does not derive the stored address",
            DataSource::Local,
        ));
    }
    let (info, source) = algod
        .account_info_with_fallback(public, &escrow.address.to_string())
        .await?;
    Ok(verify_escrow_with_info(
        escrow,
        &info,
        current_round,
        source,
    ))
}

pub fn verify_escrow_with_info(
    escrow: &EscrowAccount,
    info: &AccountInfo,
    current_round: Round,
    source: DataSource,
) -> OrderVerification {
    let params = &escrow.params;
    let expired = current_round.as_u64() > params.expire_round;
    let expected_balance = if params.sell_asset == 0 {
        match params
            .sell_amount
            .checked_add(BASE_ESCROW_FUNDING_MICROALGO)
        {
            Some(amount) => amount,
            None => {
                return invalid(
                    info,
                    0,
                    0,
                    0,
                    expired,
                    "expected ALGO balance overflow".to_string(),
                    source,
                );
            }
        }
    } else {
        MIN_ESCROW_FUNDING_MICROALGO
    };
    let expected_asset_amount = if params.sell_asset == 0 {
        0
    } else {
        params.sell_amount
    };
    let holding = info
        .assets
        .iter()
        .find(|holding| holding.asset_id == params.sell_asset);
    let actual_asset_amount = holding.map(|holding| holding.amount).unwrap_or(0);

    if info.address != escrow.address.to_string() {
        return invalid(
            info,
            expected_balance,
            actual_asset_amount,
            expected_asset_amount,
            expired,
            format!(
                "algod returned account {} for escrow {}",
                info.address, escrow.address
            ),
            source,
        );
    }
    if info.amount != expected_balance {
        return invalid(
            info,
            expected_balance,
            actual_asset_amount,
            expected_asset_amount,
            expired,
            format!(
                "unexpected ALGO balance: have {}, expected {expected_balance}",
                info.amount
            ),
            source,
        );
    }
    if params.sell_asset != 0 {
        if holding.is_some_and(|holding| holding.is_frozen) {
            return invalid(
                info,
                expected_balance,
                actual_asset_amount,
                expected_asset_amount,
                expired,
                format!("escrow holding for asset {} is frozen", params.sell_asset),
                source,
            );
        }
        if actual_asset_amount != expected_asset_amount {
            return invalid(
                info,
                expected_balance,
                actual_asset_amount,
                expected_asset_amount,
                expired,
                format!(
                    "unexpected asset {} amount: have {actual_asset_amount}, expected {expected_asset_amount}",
                    params.sell_asset
                ),
                source,
            );
        }
    }
    if expired {
        return invalid(
            info,
            expected_balance,
            actual_asset_amount,
            expected_asset_amount,
            true,
            format!(
                "order expired: current {} > expire {}",
                current_round.as_u64(),
                params.expire_round
            ),
            source,
        );
    }

    OrderVerification {
        valid: true,
        actual_balance: info.amount,
        expected_balance,
        actual_asset_amount,
        expected_asset_amount,
        expired: false,
        mismatch_reason: String::new(),
        data_source: source,
    }
}

#[allow(clippy::too_many_arguments)]
fn invalid(
    info: &AccountInfo,
    expected_balance: u64,
    actual_asset_amount: u64,
    expected_asset_amount: u64,
    expired: bool,
    reason: String,
    source: DataSource,
) -> OrderVerification {
    OrderVerification {
        valid: false,
        actual_balance: info.amount,
        expected_balance,
        actual_asset_amount,
        expected_asset_amount,
        expired,
        mismatch_reason: reason,
        data_source: source,
    }
}

pub fn verify_escrow_address(program: &[u8], expected: Address) -> bool {
    !program.is_empty() && escrow_address(program) == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EscrowKind, EscrowParams, BASE_ESCROW_FUNDING_MICROALGO};
    use opennodia_node::asset::Holding;

    fn account_info(address: Address, amount: u64, assets: Vec<Holding>) -> AccountInfo {
        AccountInfo {
            round: 100,
            address: address.to_string(),
            amount,
            amount_without_pending_rewards: amount,
            pending_rewards: 0,
            reward_base: 0,
            rewards: 0,
            min_balance: 0,
            status: "Online".to_string(),
            assets,
            created_assets: Vec::new(),
            apps_local_state: Vec::new(),
        }
    }

    fn escrow(sell_asset: u64, sell_amount: u64, expire_round: u64) -> EscrowAccount {
        let owner = Address::from_bytes([1; 32]);
        let buy_asset = if sell_asset == 0 { 12345 } else { 0 };
        let params = EscrowParams::new(
            owner,
            sell_asset,
            sell_amount,
            buy_asset,
            2_000_000,
            expire_round,
        );
        EscrowAccount::from_program(EscrowKind::Sell, params, vec![8, 1, 2, 3]).unwrap()
    }

    #[test]
    fn empty_asa_escrow_is_invalid() {
        let escrow = escrow(12345, 1_000, 1_000);
        let info = account_info(escrow.address, 0, Vec::new());

        let verification = verify_escrow_with_info(&escrow, &info, Round(100), DataSource::Local);

        assert!(!verification.valid);
        assert_eq!(verification.expected_balance, 200_000);
        assert_eq!(verification.expected_asset_amount, 1_000);
        assert!(verification
            .mismatch_reason
            .contains("unexpected ALGO balance"));
    }

    #[test]
    fn funded_algo_escrow_is_invalid_after_expiry() {
        let escrow = escrow(0, 1_000_000, 100);
        let info = account_info(
            escrow.address,
            1_000_000 + BASE_ESCROW_FUNDING_MICROALGO,
            Vec::new(),
        );

        let verification = verify_escrow_with_info(&escrow, &info, Round(101), DataSource::Local);

        assert!(!verification.valid);
        assert!(verification.expired);
        assert!(verification.mismatch_reason.contains("order expired"));
    }
}
