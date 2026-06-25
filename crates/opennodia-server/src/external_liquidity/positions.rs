use super::*;

pub(super) fn external_position_response(
    pool: ExternalPoolResponse,
    lp_balance: u64,
    liquidity_enabled: bool,
) -> ExternalLpPositionResponse {
    let total = pool.total_lp_supply.max(1);
    let underlying_0 = proportional_amount(lp_balance, pool.reserve_0, total);
    let underlying_1 = proportional_amount(lp_balance, pool.reserve_1, total);
    let pool_share_ppm = proportional_amount(lp_balance, 1_000_000, total);
    let liquidity_supported = liquidity_enabled
        && pool.tradable
        && pool.adapter_swap_supported
        && !pool.folks_backed
        && pool.lp_asset_id != 0;
    ExternalLpPositionResponse {
        lp_asset_id: pool.lp_asset_id,
        lp_balance,
        pool_share_ppm,
        underlying_0,
        underlying_1,
        position_source: pool.source.clone(),
        add_supported: liquidity_supported,
        remove_supported: liquidity_supported && lp_balance > 0,
        reward_apr_included: false,
        note: if liquidity_supported {
            "external LP add/remove is enabled for this deployment; OpenNodia rebuilds and verifies the protocol group before signing"
                .into()
        } else if pool.folks_backed {
            "external LP position is read-only; Folks lending yield is shown separately and is not folded into swap fee APR"
                .into()
        } else {
            "external LP position is read-only; enable external_liquidity.liquidity_enabled only after protocol QA for this network"
                .into()
        },
        pool,
    }
}

fn proportional_amount(balance: u64, reserve: u64, total_supply: u64) -> u64 {
    if total_supply == 0 {
        return 0;
    }
    let value = u128::from(balance) * u128::from(reserve) / u128::from(total_supply);
    value.min(u128::from(u64::MAX)) as u64
}
