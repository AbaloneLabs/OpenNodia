use opennodia_core::Round;
use opennodia_dex::types::{EntryStatus, OrderEntry};
use opennodia_swap::{encode_order_link, EscrowAccount, OrderLinkPayload};

use super::{bad_request, internal, ApiResult, OrderLinkGenerateResponse};

pub(super) fn order_link_payload_from_entry(entry: &OrderEntry) -> OrderLinkPayload {
    OrderLinkPayload::new(
        entry.side,
        entry.sell_asset,
        entry.sell_amount,
        entry.buy_asset,
        entry.buy_amount,
        entry.owner,
        entry.escrow_addr,
        entry.expire_round.as_u64(),
    )
}

pub(super) fn order_link_response_from_payload(
    payload: OrderLinkPayload,
) -> ApiResult<OrderLinkGenerateResponse> {
    let encoded = encode_order_link(&payload)
        .map_err(|error| internal(format!("encode order link: {error}")))?;
    Ok(OrderLinkGenerateResponse {
        url: format!("/#/dex/order/{encoded}"),
        payload: encoded,
        decoded: (&payload).into(),
    })
}

pub(super) fn order_entry_from_link_payload(
    payload: &OrderLinkPayload,
    escrow: &EscrowAccount,
) -> ApiResult<OrderEntry> {
    let price = opennodia_dex::types::order_price(
        payload.side,
        payload.sell_asset,
        payload.sell_amount,
        payload.buy_asset,
        payload.buy_amount,
    )
    .ok_or_else(|| bad_request("linked order price cannot be normalized"))?;
    Ok(OrderEntry {
        escrow_addr: escrow.address,
        side: payload.side,
        sell_asset: payload.sell_asset,
        sell_amount: payload.sell_amount,
        buy_asset: payload.buy_asset,
        buy_amount: payload.buy_amount,
        price,
        owner: payload.owner_address(),
        created_round: Round(0),
        expire_round: Round(payload.expire_round),
        status: EntryStatus::Active,
        filled_amount: 0,
        split_index: 0,
        parent_id: None,
        program: escrow.program.clone(),
        params: escrow.params.clone(),
    })
}
