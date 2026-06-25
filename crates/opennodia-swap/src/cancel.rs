//! Owner-authorized escrow cancellation transaction construction.

use crate::escrow::{
    cancel_note, EscrowAccount, BASE_ESCROW_FUNDING_MICROALGO, MIN_ESCROW_FUNDING_MICROALGO,
};
use crate::tx::{
    assign_group_id, build_payment, TransactionFields, TransactionParams, TransactionType,
};

#[derive(Clone, Debug)]
pub struct CancelResult {
    pub escrow: EscrowAccount,
    pub owner_auth_tx: TransactionFields,
    pub escrow_txs: Vec<TransactionFields>,
    pub group_id: [u8; 32],
    pub recoverable_algo: u64,
    pub recoverable_asset: Option<(u64, u64)>,
}

pub fn build_cancel_group(
    escrow: &EscrowAccount,
    params: &TransactionParams,
) -> opennodia_core::Result<CancelResult> {
    let owner = escrow.params.owner;
    let (escrow_txs, recoverable_algo, recoverable_asset) = if escrow.params.sell_asset == 0 {
        let mut close_algo = build_payment(escrow.address, owner, 0, params);
        close_algo.close_remainder_to = Some(owner);
        close_algo.fee = 0;
        (
            vec![close_algo],
            escrow
                .params
                .sell_amount
                .checked_add(BASE_ESCROW_FUNDING_MICROALGO)
                .ok_or_else(|| {
                    opennodia_core::Error::Other("recoverable ALGO amount overflow".to_string())
                })?,
            None,
        )
    } else {
        let mut close_asset =
            TransactionFields::base(TransactionType::Axfer, escrow.address, params);
        close_asset.xfer_asset = Some(escrow.params.sell_asset);
        close_asset.asset_amount = Some(0);
        close_asset.asset_receiver = Some(owner);
        close_asset.asset_close_to = Some(owner);
        close_asset.fee = 0;

        let mut close_algo = build_payment(escrow.address, owner, 0, params);
        close_algo.close_remainder_to = Some(owner);
        close_algo.fee = 0;
        (
            vec![close_asset, close_algo],
            MIN_ESCROW_FUNDING_MICROALGO,
            Some((escrow.params.sell_asset, escrow.params.sell_amount)),
        )
    };

    let group_size = u64::try_from(escrow_txs.len() + 1)
        .map_err(|_| opennodia_core::Error::Other("cancel group size overflow".to_string()))?;
    let mut owner_auth_tx = build_payment(owner, owner, 0, params);
    owner_auth_tx.note = Some(cancel_note(&escrow.params));
    owner_auth_tx.fee = params
        .fee
        .checked_mul(group_size)
        .ok_or_else(|| opennodia_core::Error::Other("cancel group fee overflow".to_string()))?;

    let mut group = Vec::with_capacity(escrow_txs.len() + 1);
    group.push(owner_auth_tx);
    group.extend(escrow_txs);
    let group_id = assign_group_id(&mut group);
    let owner_auth_tx = group.remove(0);

    Ok(CancelResult {
        escrow: escrow.clone(),
        owner_auth_tx,
        escrow_txs: group,
        group_id,
        recoverable_algo,
        recoverable_asset,
    })
}
