//! Order deposit transaction construction.

use crate::escrow::{EscrowAccount, BASE_ESCROW_FUNDING_MICROALGO, MIN_ESCROW_FUNDING_MICROALGO};
use crate::tx::{
    assign_group_id, build_asset_opt_in, build_asset_transfer, build_payment, TransactionFields,
};

/// Result of building the exact deposit transaction set for one escrow.
#[derive(Clone, Debug)]
pub struct CreateOrderResult {
    pub escrow: EscrowAccount,
    pub owner_txs: Vec<TransactionFields>,
    pub logicsig_txs: Vec<TransactionFields>,
    pub group_id: Option<[u8; 32]>,
}

/// Build a deposit where `sell_asset` is always the asset funded by the owner.
pub fn build_deposit_group(
    escrow: &EscrowAccount,
    params: &crate::tx::TransactionParams,
) -> opennodia_core::Result<CreateOrderResult> {
    let owner = escrow.params.owner;

    if escrow.params.sell_asset == 0 {
        let amount = escrow
            .params
            .sell_amount
            .checked_add(BASE_ESCROW_FUNDING_MICROALGO)
            .ok_or_else(|| {
                opennodia_core::Error::Other("ALGO escrow funding overflow".to_string())
            })?;
        return Ok(CreateOrderResult {
            escrow: escrow.clone(),
            owner_txs: vec![build_payment(owner, escrow.address, amount, params)],
            logicsig_txs: Vec::new(),
            group_id: None,
        });
    }

    let mut funding = build_payment(owner, escrow.address, MIN_ESCROW_FUNDING_MICROALGO, params);
    funding.fee = params
        .fee
        .checked_mul(2)
        .ok_or_else(|| opennodia_core::Error::Other("deposit fee overflow".to_string()))?;

    let mut opt_in = build_asset_opt_in(escrow.address, escrow.params.sell_asset, params);
    opt_in.fee = 0;
    let deposit = build_asset_transfer(
        owner,
        escrow.address,
        escrow.params.sell_asset,
        escrow.params.sell_amount,
        params,
    );

    let mut group = vec![funding, opt_in, deposit];
    let group_id = assign_group_id(&mut group);
    let deposit = group.remove(2);
    let opt_in = group.remove(1);
    let funding = group.remove(0);

    Ok(CreateOrderResult {
        escrow: escrow.clone(),
        owner_txs: vec![funding, deposit],
        logicsig_txs: vec![opt_in],
        group_id: Some(group_id),
    })
}

/// Split a quantity exactly while preserving the total.
pub fn split_amounts(total: u64, splits: u32) -> Vec<u64> {
    if splits == 0 {
        return Vec::new();
    }
    let count = u64::from(splits);
    let base = total / count;
    let remainder = total % count;
    (0..count)
        .map(|index| if index < remainder { base + 1 } else { base })
        .collect()
}
