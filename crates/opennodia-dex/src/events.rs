//! On-chain escrow reconciliation.
//!
//! Account state proves whether an escrow is still funded, but a closed
//! account alone cannot distinguish a fill from a cancellation. Closed
//! escrows are resolved only after the confirmed Indexer group is validated
//! against the stored order parameters.

use std::collections::HashSet;

use base64::Engine;
use opennodia_core::{Address, Round};
use opennodia_node::algod::AlgodClient;
use opennodia_node::asset::AccountInfo;
use opennodia_node::{IndexerClient, IndexerTransaction};
use opennodia_swap::{cancel_note, OrderSide};

use crate::db::DexDb;
use crate::types::{order_price, EntryStatus, OrderEntry, Pair, Trade};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EscrowEvent {
    Unchanged,
    Filled(Trade),
    Cancelled { tx_id: String, round: Round },
    ClosedUnresolved { round: Round },
    Expired,
}

#[derive(Clone, Debug, Default)]
pub struct SweepResult {
    pub checked: u64,
    pub expiries: Vec<Address>,
    pub fills: Vec<(Address, String)>,
    pub cancellations: Vec<(Address, String)>,
    pub unresolved: Vec<(Address, Round)>,
    pub errors: Vec<(Address, String)>,
}

impl SweepResult {
    pub fn total_changes(&self) -> usize {
        self.expiries.len() + self.fills.len() + self.cancellations.len() + self.unresolved.len()
    }
}

pub fn classify_escrow_event(
    info: &AccountInfo,
    entry: &OrderEntry,
    current_round: Round,
) -> EscrowEvent {
    if info.amount == 0 && info.assets.is_empty() {
        return EscrowEvent::ClosedUnresolved {
            round: Round(info.round.max(current_round.as_u64())),
        };
    }

    let still_holds = if entry.sell_asset == 0 {
        info.amount
            == entry
                .sell_amount
                .saturating_add(opennodia_swap::BASE_ESCROW_FUNDING_MICROALGO)
    } else {
        info.amount == opennodia_swap::MIN_ESCROW_FUNDING_MICROALGO
            && info.assets.iter().any(|holding| {
                holding.asset_id == entry.sell_asset
                    && holding.amount == entry.sell_amount
                    && !holding.is_frozen
            })
    };

    if current_round.as_u64() > entry.expire_round.as_u64() && still_holds {
        EscrowEvent::Expired
    } else {
        EscrowEvent::Unchanged
    }
}

/// Validate one confirmed atomic group as a fill or cancellation.
pub fn classify_confirmed_group(
    entry: &OrderEntry,
    transactions: &[IndexerTransaction],
) -> Option<EscrowEvent> {
    if transactions.is_empty() {
        return None;
    }
    let mut group = transactions.to_vec();
    group.sort_by_key(|transaction| transaction.intra_round_offset);
    let first = group.first()?;
    if first.id.is_empty() || first.round == 0 {
        return None;
    }
    if is_cancel_group(entry, &group) {
        return Some(EscrowEvent::Cancelled {
            tx_id: first.id.clone(),
            round: Round(first.round),
        });
    }
    let filler = is_fill_group(entry, &group)?;
    let side = entry.side;
    let price = order_price(
        side,
        entry.sell_asset,
        entry.sell_amount,
        entry.buy_asset,
        entry.buy_amount,
    )?;
    let (buyer, seller) = match side {
        OrderSide::Sell => (filler, entry.owner),
        OrderSide::Buy => (entry.owner, filler),
    };
    Some(EscrowEvent::Filled(Trade {
        tx_id: first.id.clone(),
        pair: Pair::new(entry.sell_asset, entry.buy_asset),
        side,
        price,
        base_asset: Some(match side {
            OrderSide::Sell => entry.sell_asset,
            OrderSide::Buy => entry.buy_asset,
        }),
        amount: entry.base_amount(),
        buyer,
        seller,
        round: Round(first.round),
        timestamp: first.round_time,
    }))
}

fn is_fill_group(entry: &OrderEntry, group: &[IndexerTransaction]) -> Option<Address> {
    let expected_size = if entry.sell_asset == 0 { 2 } else { 3 };
    if group.len() != expected_size {
        return None;
    }
    let payment = &group[0];
    let filler: Address = payment.sender.parse().ok()?;
    if filler == entry.owner || !payment_matches_order(payment, entry, filler) {
        return None;
    }

    let escrow = entry.escrow_addr.to_string();
    if entry.sell_asset == 0 {
        let release = &group[1];
        let detail = release.payment.as_ref()?;
        if release.tx_type != "pay"
            || release.sender != escrow
            || detail.receiver != filler.to_string()
            || detail.amount != entry.sell_amount
            || detail.close_remainder_to.as_deref() != Some(&entry.owner.to_string())
        {
            return None;
        }
    } else {
        let release = &group[1];
        let asset = release.asset_transfer.as_ref()?;
        let close = &group[2];
        let close_payment = close.payment.as_ref()?;
        if release.tx_type != "axfer"
            || release.sender != escrow
            || asset.asset_id != entry.sell_asset
            || asset.amount != entry.sell_amount
            || asset.receiver != filler.to_string()
            || asset.close_to.as_deref() != Some(&filler.to_string())
            || close.tx_type != "pay"
            || close.sender != escrow
            || close_payment.amount != 0
            || close_payment.receiver != entry.owner.to_string()
            || close_payment.close_remainder_to.as_deref() != Some(&entry.owner.to_string())
        {
            return None;
        }
    }
    Some(filler)
}

fn payment_matches_order(
    transaction: &IndexerTransaction,
    entry: &OrderEntry,
    filler: Address,
) -> bool {
    if entry.buy_asset == 0 {
        let Some(payment) = transaction.payment.as_ref() else {
            return false;
        };
        transaction.tx_type == "pay"
            && transaction.sender == filler.to_string()
            && payment.receiver == entry.owner.to_string()
            && payment.amount == entry.buy_amount
            && payment.close_remainder_to.is_none()
    } else {
        let Some(transfer) = transaction.asset_transfer.as_ref() else {
            return false;
        };
        transaction.tx_type == "axfer"
            && transaction.sender == filler.to_string()
            && transfer.asset_id == entry.buy_asset
            && transfer.receiver == entry.owner.to_string()
            && transfer.amount == entry.buy_amount
            && transfer.close_to.is_none()
    }
}

fn is_cancel_group(entry: &OrderEntry, group: &[IndexerTransaction]) -> bool {
    let expected_size = if entry.sell_asset == 0 { 2 } else { 3 };
    if group.len() != expected_size {
        return false;
    }
    let auth = &group[0];
    let expected_note =
        base64::engine::general_purpose::STANDARD.encode(cancel_note(&entry.params));
    let Some(payment) = auth.payment.as_ref() else {
        return false;
    };
    if auth.tx_type != "pay"
        || auth.sender != entry.owner.to_string()
        || payment.receiver != entry.owner.to_string()
        || payment.amount != 0
        || payment.close_remainder_to.is_some()
        || auth.note.as_deref() != Some(expected_note.as_str())
    {
        return false;
    }

    let escrow = entry.escrow_addr.to_string();
    if entry.sell_asset == 0 {
        let Some(close) = group[1].payment.as_ref() else {
            return false;
        };
        group[1].tx_type == "pay"
            && group[1].sender == escrow
            && close.receiver == entry.owner.to_string()
            && close.amount == 0
            && close.close_remainder_to.as_deref() == Some(&entry.owner.to_string())
    } else {
        let Some(asset) = group[1].asset_transfer.as_ref() else {
            return false;
        };
        let Some(close) = group[2].payment.as_ref() else {
            return false;
        };
        group[1].tx_type == "axfer"
            && group[1].sender == escrow
            && asset.asset_id == entry.sell_asset
            && asset.amount == 0
            && asset.close_amount == entry.sell_amount
            && asset.receiver == entry.owner.to_string()
            && asset.close_to.as_deref() == Some(&entry.owner.to_string())
            && group[2].tx_type == "pay"
            && group[2].sender == escrow
            && close.receiver == entry.owner.to_string()
            && close.amount == 0
            && close.close_remainder_to.as_deref() == Some(&entry.owner.to_string())
    }
}

/// Reconcile every order whose final state is not yet proven locally.
pub async fn sweep_active_orders(
    db: &DexDb,
    algod: &AlgodClient,
    public: Option<&AlgodClient>,
    indexer: Option<&IndexerClient>,
    current_round: Round,
) -> opennodia_core::Result<SweepResult> {
    let entries = db.get_reconcilable_orders()?;
    let mut result = SweepResult::default();
    for entry in entries {
        result.checked = result.checked.saturating_add(1);
        match reconcile_order(db, algod, public, indexer, &entry, current_round).await {
            Ok(EscrowEvent::Expired) => result.expiries.push(entry.escrow_addr),
            Ok(EscrowEvent::Filled(trade)) => result.fills.push((entry.escrow_addr, trade.tx_id)),
            Ok(EscrowEvent::Cancelled { tx_id, .. }) => {
                result.cancellations.push((entry.escrow_addr, tx_id))
            }
            Ok(EscrowEvent::ClosedUnresolved { round }) => {
                result.unresolved.push((entry.escrow_addr, round))
            }
            Ok(EscrowEvent::Unchanged) => {}
            Err(error) => result.errors.push((entry.escrow_addr, error.to_string())),
        }
    }
    if result.errors.is_empty() {
        db.set_last_synced_round(current_round.as_u64())?;
    }
    Ok(result)
}

pub async fn poll_order(
    db: &DexDb,
    algod: &AlgodClient,
    public: Option<&AlgodClient>,
    indexer: Option<&IndexerClient>,
    entry: &OrderEntry,
    current_round: Round,
) -> opennodia_core::Result<EscrowEvent> {
    reconcile_order(db, algod, public, indexer, entry, current_round).await
}

async fn reconcile_order(
    db: &DexDb,
    algod: &AlgodClient,
    public: Option<&AlgodClient>,
    indexer: Option<&IndexerClient>,
    entry: &OrderEntry,
    current_round: Round,
) -> opennodia_core::Result<EscrowEvent> {
    if matches!(
        entry.status,
        EntryStatus::ClosedUnresolved | EntryStatus::Filled | EntryStatus::Cancelled
    ) {
        if let Some(indexer) = indexer {
            if let Some(event) = resolve_closed_order(indexer, entry).await? {
                persist_resolved_event(db, entry, &event)?;
                return Ok(event);
            }
        }
    }

    let address = entry.escrow_addr.to_string();
    let info = match algod.account_info_optional(&address).await {
        Ok(info) => info,
        Err(local_error) => {
            if let Some(public) = public {
                public
                    .account_info_optional(&address)
                    .await
                    .map_err(|error| {
                        opennodia_core::Error::Algod(format!(
                            "local: {local_error}; public: {error}"
                        ))
                    })?
            } else {
                return Err(local_error);
            }
        }
    };
    let state_event = info
        .as_ref()
        .map(|info| classify_escrow_event(info, entry, current_round))
        .unwrap_or(EscrowEvent::ClosedUnresolved {
            round: current_round,
        });
    match state_event {
        EscrowEvent::Expired => {
            if entry.status != EntryStatus::Expired {
                db.update_order_status_checked(&entry.escrow_addr, EntryStatus::Expired)?;
            }
            Ok(EscrowEvent::Expired)
        }
        EscrowEvent::ClosedUnresolved { round } => {
            if let Some(indexer) = indexer {
                if let Some(event) = resolve_closed_order(indexer, entry).await? {
                    persist_resolved_event(db, entry, &event)?;
                    return Ok(event);
                }
            }
            db.mark_closed_unresolved(&entry.escrow_addr, round)?;
            Ok(EscrowEvent::ClosedUnresolved { round })
        }
        _ => Ok(EscrowEvent::Unchanged),
    }
}

fn persist_resolved_event(
    db: &DexDb,
    entry: &OrderEntry,
    event: &EscrowEvent,
) -> opennodia_core::Result<()> {
    match event {
        EscrowEvent::Filled(trade) => db.record_fill(&entry.escrow_addr, entry.sell_amount, trade),
        EscrowEvent::Cancelled { tx_id, round } => {
            db.record_cancel(&entry.escrow_addr, tx_id, *round)
        }
        _ => Ok(()),
    }
}

pub async fn resolve_closed_order(
    indexer: &IndexerClient,
    entry: &OrderEntry,
) -> opennodia_core::Result<Option<EscrowEvent>> {
    let history = indexer
        .account_transactions(&entry.escrow_addr.to_string(), 100)
        .await?;
    let mut seen = HashSet::new();
    for transaction in history {
        if transaction.round < entry.created_round.as_u64()
            || transaction.sender != entry.escrow_addr.to_string()
        {
            continue;
        }
        let Some(group_id) = transaction.group.as_deref() else {
            continue;
        };
        if !seen.insert(group_id.to_string()) {
            continue;
        }
        let group = indexer.transactions_by_group(group_id).await?;
        if let Some(event) = classify_confirmed_group(entry, &group) {
            return Ok(Some(event));
        }
    }
    Ok(None)
}

pub use opennodia_node::DataSource as EventDataSource;

#[cfg(test)]
mod tests {
    use super::*;
    use opennodia_node::indexer::{AssetTransferDetail, PaymentDetail};
    use opennodia_swap::{escrow_address, EscrowParams};

    fn entry() -> OrderEntry {
        let owner = Address::from_bytes([1; 32]);
        let program = vec![8, 1, 2, 3];
        OrderEntry {
            escrow_addr: escrow_address(&program),
            side: OrderSide::Sell,
            sell_asset: 12345,
            sell_amount: 1_000,
            buy_asset: 0,
            buy_amount: 2_000_000,
            price: 2_000_000_000,
            owner,
            created_round: Round(100),
            expire_round: Round(1_000),
            status: EntryStatus::Active,
            filled_amount: 0,
            split_index: 0,
            parent_id: None,
            program,
            params: EscrowParams::new(owner, 12345, 1_000, 0, 2_000_000, 1_000),
        }
    }

    fn transaction(id: &str, offset: u64, tx_type: &str, sender: Address) -> IndexerTransaction {
        IndexerTransaction {
            id: id.to_string(),
            round: 500,
            intra_round_offset: offset,
            round_time: 1_700_000_000,
            tx_type: tx_type.to_string(),
            sender: sender.to_string(),
            group: Some("group".to_string()),
            fee: 1_000,
            first_valid: 499,
            last_valid: 1_499,
            payment: None,
            asset_transfer: None,
            note: None,
            receiver: None,
            amount: None,
            asset_id: None,
        }
    }

    #[test]
    fn funded_order_expires_only_after_expiry_round() {
        let order = entry();
        let info = AccountInfo {
            round: 1_001,
            address: order.escrow_addr.to_string(),
            amount: opennodia_swap::MIN_ESCROW_FUNDING_MICROALGO,
            amount_without_pending_rewards: 0,
            pending_rewards: 0,
            reward_base: 0,
            rewards: 0,
            min_balance: 0,
            status: String::new(),
            assets: vec![opennodia_node::Holding {
                asset_id: order.sell_asset,
                amount: order.sell_amount,
                is_frozen: false,
                creator: String::new(),
            }],
            created_assets: Vec::new(),
            apps_local_state: Vec::new(),
        };
        assert_eq!(
            classify_escrow_event(&info, &order, Round(1_001)),
            EscrowEvent::Expired
        );
    }

    #[test]
    fn validates_confirmed_asa_fill_group() {
        let order = entry();
        let filler = Address::from_bytes([2; 32]);
        let mut payment = transaction("PAY", 0, "pay", filler);
        payment.payment = Some(PaymentDetail {
            receiver: order.owner.to_string(),
            amount: order.buy_amount,
            close_amount: 0,
            close_remainder_to: None,
        });
        let mut release = transaction("RELEASE", 1, "axfer", order.escrow_addr);
        release.asset_transfer = Some(AssetTransferDetail {
            receiver: filler.to_string(),
            amount: order.sell_amount,
            asset_id: order.sell_asset,
            sender: String::new(),
            close_to: Some(filler.to_string()),
            close_amount: 0,
        });
        let mut close = transaction("CLOSE", 2, "pay", order.escrow_addr);
        close.payment = Some(PaymentDetail {
            receiver: order.owner.to_string(),
            amount: 0,
            close_amount: opennodia_swap::MIN_ESCROW_FUNDING_MICROALGO,
            close_remainder_to: Some(order.owner.to_string()),
        });

        let event = classify_confirmed_group(&order, &[close, payment, release]).unwrap();
        let EscrowEvent::Filled(trade) = event else {
            panic!("expected fill");
        };
        assert_eq!(trade.tx_id, "PAY");
        assert_eq!(trade.amount, order.sell_amount);
        assert_eq!(trade.buyer, filler);
        assert_eq!(trade.seller, order.owner);
    }

    #[test]
    fn validates_confirmed_asa_cancel_group() {
        let order = entry();
        let mut auth = transaction("AUTH", 0, "pay", order.owner);
        auth.note =
            Some(base64::engine::general_purpose::STANDARD.encode(cancel_note(&order.params)));
        auth.payment = Some(PaymentDetail {
            receiver: order.owner.to_string(),
            amount: 0,
            close_amount: 0,
            close_remainder_to: None,
        });
        let mut release = transaction("RELEASE", 1, "axfer", order.escrow_addr);
        release.asset_transfer = Some(AssetTransferDetail {
            receiver: order.owner.to_string(),
            amount: 0,
            asset_id: order.sell_asset,
            sender: String::new(),
            close_to: Some(order.owner.to_string()),
            close_amount: order.sell_amount,
        });
        let mut close = transaction("CLOSE", 2, "pay", order.escrow_addr);
        close.payment = Some(PaymentDetail {
            receiver: order.owner.to_string(),
            amount: 0,
            close_amount: opennodia_swap::MIN_ESCROW_FUNDING_MICROALGO,
            close_remainder_to: Some(order.owner.to_string()),
        });

        assert_eq!(
            classify_confirmed_group(&order, &[auth, release, close]),
            Some(EscrowEvent::Cancelled {
                tx_id: "AUTH".to_string(),
                round: Round(500),
            })
        );
    }

    #[test]
    fn rejects_group_with_modified_fill_amount() {
        let order = entry();
        let filler = Address::from_bytes([2; 32]);
        let mut payment = transaction("PAY", 0, "pay", filler);
        payment.payment = Some(PaymentDetail {
            receiver: order.owner.to_string(),
            amount: order.buy_amount - 1,
            close_amount: 0,
            close_remainder_to: None,
        });
        assert_eq!(classify_confirmed_group(&order, &[payment]), None);
    }
}
