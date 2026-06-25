//! Atomic order fill transaction construction.

use opennodia_core::{Address, Round};

use crate::escrow::{EscrowAccount, EscrowKind};
use crate::tx::{
    assign_group_id, build_asset_transfer, build_payment, TransactionFields, TransactionParams,
};

#[derive(Clone, Debug)]
pub struct FillResult {
    pub escrow: EscrowAccount,
    pub filler_tx: TransactionFields,
    pub escrow_txs: Vec<TransactionFields>,
    pub group_id: [u8; 32],
}

/// Build the exact atomic group that pays the owner and closes the escrow.
pub fn build_fill_group(
    escrow: &EscrowAccount,
    filler: Address,
    lease: [u8; 32],
    params: &TransactionParams,
) -> opennodia_core::Result<FillResult> {
    if lease == [0; 32] {
        return Err(opennodia_core::Error::Other(
            "fill lease must not be zero".to_string(),
        ));
    }

    let owner = escrow.params.owner;
    let mut filler_tx = if escrow.params.buy_asset == 0 {
        build_payment(filler, owner, escrow.params.buy_amount, params)
    } else {
        build_asset_transfer(
            filler,
            owner,
            escrow.params.buy_asset,
            escrow.params.buy_amount,
            params,
        )
    };

    let mut escrow_txs = if escrow.params.sell_asset == 0 {
        let mut release = build_payment(escrow.address, filler, escrow.params.sell_amount, params);
        release.close_remainder_to = Some(owner);
        release.lease = Some(lease);
        vec![release]
    } else {
        let mut release = build_asset_transfer(
            escrow.address,
            filler,
            escrow.params.sell_asset,
            escrow.params.sell_amount,
            params,
        );
        release.asset_close_to = Some(filler);
        release.lease = Some(lease);
        let mut close_algo = build_payment(escrow.address, owner, 0, params);
        close_algo.close_remainder_to = Some(owner);
        vec![release, close_algo]
    };

    let group_size = u64::try_from(escrow_txs.len() + 1)
        .map_err(|_| opennodia_core::Error::Other("fill group size overflow".to_string()))?;
    filler_tx.fee = params
        .fee
        .checked_mul(group_size)
        .ok_or_else(|| opennodia_core::Error::Other("fill group fee overflow".to_string()))?;
    for transaction in &mut escrow_txs {
        transaction.fee = 0;
    }

    let mut group = Vec::with_capacity(escrow_txs.len() + 1);
    group.push(filler_tx);
    group.extend(escrow_txs);
    let group_id = assign_group_id(&mut group);
    let filler_tx = group.remove(0);

    Ok(FillResult {
        escrow: escrow.clone(),
        filler_tx,
        escrow_txs: group,
        group_id,
    })
}

pub fn build_fill_from_params(
    kind: EscrowKind,
    escrow_params: crate::escrow::EscrowParams,
    program: Vec<u8>,
    filler: Address,
    lease: [u8; 32],
    params: &TransactionParams,
) -> opennodia_core::Result<FillResult> {
    let escrow = EscrowAccount::from_program(kind, escrow_params, program)?;
    build_fill_group(&escrow, filler, lease, params)
}

pub fn derive_lease(filler: Address, escrow: Address) -> [u8; 32] {
    use sha2::{Digest, Sha512_256};
    let mut hasher = Sha512_256::new();
    hasher.update(b"OpenNodiaFillLease");
    hasher.update(filler.as_bytes());
    hasher.update(escrow.as_bytes());
    let digest = hasher.finalize();
    let mut lease = [0u8; 32];
    lease.copy_from_slice(&digest);
    lease[0] |= 0x01;
    lease
}

pub fn fill_allowed(escrow: &EscrowAccount, current_round: Round) -> bool {
    current_round.as_u64() <= escrow.params.expire_round
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::escrow::EscrowParams;

    fn escrow_with_expire(expire_round: u64) -> EscrowAccount {
        let params = EscrowParams::new(
            Address::from_bytes([1u8; 32]),
            123,
            1_000,
            0,
            2_000,
            expire_round,
        );
        // Build a minimal escrow account for logic checks (program bytes are not
        // exercised by `fill_allowed`, only `params.expire_round`).
        EscrowAccount {
            kind: EscrowKind::Sell,
            params,
            program: vec![],
            address: Address::from_bytes([2u8; 32]),
        }
    }

    #[test]
    fn fill_allowed_at_and_past_expiry() {
        let escrow = escrow_with_expire(1000);
        assert!(fill_allowed(&escrow, Round(1000)), "fill allowed at expiry");
        assert!(
            fill_allowed(&escrow, Round(999)),
            "fill allowed before expiry"
        );
        assert!(
            !fill_allowed(&escrow, Round(1001)),
            "fill not allowed after expiry"
        );
    }

    #[test]
    fn derive_lease_is_deterministic_and_nonzero() {
        let filler = Address::from_bytes([3u8; 32]);
        let escrow = Address::from_bytes([4u8; 32]);
        let lease = derive_lease(filler, escrow);
        assert_ne!(lease, [0u8; 32], "lease must be nonzero");
        assert_eq!(
            lease,
            derive_lease(filler, escrow),
            "lease is deterministic"
        );
        // Different inputs should yield different leases.
        let other = derive_lease(Address::from_bytes([5u8; 32]), escrow);
        assert_ne!(lease, other, "lease differs for different fillers");
    }
}
