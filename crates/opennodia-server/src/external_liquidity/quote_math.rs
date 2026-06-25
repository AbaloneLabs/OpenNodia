//! External liquidity quote math.

use opennodia_amm::{
    apply_slippage_floor, AddLiquidityQuote, RemoveLiquidityQuote, SwapQuote, FEE_DENOMINATOR_BPS,
    MINIMUM_LOCKED_LP,
};

use super::{
    bad_request, mul_div_ceil, mul_div_floor, ApiResult, ExternalPoolResponse, ExternalPoolState,
    ExternalQuoteMath,
};

pub(super) fn quote_external_exact_in(
    pool: &ExternalPoolState,
    asset_in: u64,
    amount_in: u64,
    slippage_bps: u16,
) -> ApiResult<SwapQuote> {
    if amount_in == 0 {
        return Err(bad_request("amount_in must be greater than zero"));
    }
    let (reserve_in, reserve_out, asset_out) = reserves_for_asset(&pool.response, asset_in)?;
    if reserve_in == 0 || reserve_out == 0 {
        return Err(bad_request("external pool has no active liquidity"));
    }
    let (amount_out, fee_amount_estimate) = match pool.quote_math {
        ExternalQuoteMath::TinymanV2InputFee => {
            quote_input_fee_cpmm(reserve_in, reserve_out, amount_in, pool.response.fee_bps)?
        }
        ExternalQuoteMath::PactConstantProductOutputFee => {
            quote_output_fee_cpmm(reserve_in, reserve_out, amount_in, pool.response.fee_bps)?
        }
    };
    if amount_out == 0 || amount_out >= reserve_out {
        return Err(bad_request(
            "external quote has insufficient output liquidity",
        ));
    }
    let minimum_out = apply_slippage_floor(amount_out, slippage_bps)
        .map_err(|error| bad_request(format!("slippage calculation failed: {error}")))?;
    let price_impact_bps = price_impact_bps(amount_in, amount_out, reserve_in, reserve_out)?;

    Ok(SwapQuote {
        pool_id: pool.response.pool_id.clone(),
        asset_in,
        asset_out,
        amount_in,
        amount_out,
        minimum_out,
        fee_bps: pool.response.fee_bps,
        fee_amount_estimate,
        price_impact_bps,
        source_round: pool.response.source_round,
    })
}

pub(super) fn quote_external_balanced_add(
    pool: &ExternalPoolResponse,
    desired_0: u64,
    desired_1: u64,
    slippage_bps: u16,
) -> ApiResult<AddLiquidityQuote> {
    if desired_0 == 0 || desired_1 == 0 {
        return Err(bad_request(
            "add liquidity amounts must be greater than zero",
        ));
    }
    if pool.reserve_0 == 0 || pool.reserve_1 == 0 || pool.total_lp_supply == 0 {
        return Err(bad_request("external pool has no active liquidity"));
    }
    let minted_lp = external_add_minted_floor(pool, desired_0, desired_1)?;
    if minted_lp == 0 {
        return Err(bad_request(
            "add liquidity amounts are too small for this external pool",
        ));
    }
    let amount_0 = mul_div_ceil(minted_lp, pool.reserve_0, pool.total_lp_supply)?;
    let amount_1 = mul_div_ceil(minted_lp, pool.reserve_1, pool.total_lp_supply)?;
    if amount_0 == 0 || amount_1 == 0 || amount_0 > desired_0 || amount_1 > desired_1 {
        return Err(bad_request(
            "add liquidity amounts are too small to produce a balanced external deposit",
        ));
    }
    pool.reserve_0
        .checked_add(amount_0)
        .ok_or_else(|| bad_request("external add reserve_0 overflow"))?;
    pool.reserve_1
        .checked_add(amount_1)
        .ok_or_else(|| bad_request("external add reserve_1 overflow"))?;
    pool.total_lp_supply
        .checked_add(minted_lp)
        .ok_or_else(|| bad_request("external add LP supply overflow"))?;
    let minimum_lp = apply_slippage_floor(minted_lp, slippage_bps)
        .map_err(|error| bad_request(format!("slippage calculation failed: {error}")))?;
    Ok(AddLiquidityQuote {
        amount_0,
        amount_1,
        minted_lp,
        minimum_lp,
    })
}

pub(super) fn external_add_minted_floor(
    pool: &ExternalPoolResponse,
    amount_0: u64,
    amount_1: u64,
) -> ApiResult<u64> {
    if amount_0 == 0 || amount_1 == 0 {
        return Err(bad_request(
            "add liquidity amounts must be greater than zero",
        ));
    }
    if pool.reserve_0 == 0 || pool.reserve_1 == 0 || pool.total_lp_supply == 0 {
        return Err(bad_request("external pool has no active liquidity"));
    }
    let lp_from_0 = mul_div_floor(amount_0, pool.total_lp_supply, pool.reserve_0)?;
    let lp_from_1 = mul_div_floor(amount_1, pool.total_lp_supply, pool.reserve_1)?;
    Ok(lp_from_0.min(lp_from_1))
}

pub(super) fn quote_external_remove(
    pool: &ExternalPoolResponse,
    burn_lp: u64,
    slippage_bps: u16,
) -> ApiResult<RemoveLiquidityQuote> {
    if burn_lp == 0 {
        return Err(bad_request("burn_lp must be greater than zero"));
    }
    if pool.reserve_0 == 0 || pool.reserve_1 == 0 || pool.total_lp_supply == 0 {
        return Err(bad_request("external pool has no active liquidity"));
    }
    if burn_lp >= pool.total_lp_supply {
        return Err(bad_request("burn_lp must be below total LP supply"));
    }
    let tinyman_full_remove = pool.source == "tinyman"
        && burn_lp
            .checked_add(MINIMUM_LOCKED_LP)
            .is_some_and(|value| value >= pool.total_lp_supply);
    let (amount_0, amount_1) = if tinyman_full_remove {
        (pool.reserve_0, pool.reserve_1)
    } else {
        (
            mul_div_floor(burn_lp, pool.reserve_0, pool.total_lp_supply)?,
            mul_div_floor(burn_lp, pool.reserve_1, pool.total_lp_supply)?,
        )
    };
    if amount_0 == 0 || amount_1 == 0 {
        return Err(bad_request(
            "burn_lp is too small for this external pool position",
        ));
    }
    let minimum_0 = apply_slippage_floor(amount_0, slippage_bps)
        .map_err(|error| bad_request(format!("slippage calculation failed: {error}")))?;
    let minimum_1 = apply_slippage_floor(amount_1, slippage_bps)
        .map_err(|error| bad_request(format!("slippage calculation failed: {error}")))?;
    Ok(RemoveLiquidityQuote {
        burn_lp,
        amount_0,
        amount_1,
        minimum_0,
        minimum_1,
    })
}

pub(super) fn quote_input_fee_cpmm(
    reserve_in: u64,
    reserve_out: u64,
    amount_in: u64,
    fee_bps: u16,
) -> ApiResult<(u64, u64)> {
    if u64::from(fee_bps) >= FEE_DENOMINATOR_BPS {
        return Err(bad_request("fee_bps must be below 10000"));
    }
    let fee_amount = u64::try_from(
        u128::from(amount_in)
            .checked_mul(u128::from(fee_bps))
            .ok_or_else(|| bad_request("fee calculation overflow"))?
            / u128::from(FEE_DENOMINATOR_BPS),
    )
    .map_err(|_| bad_request("fee calculation overflow"))?;
    let swap_amount = amount_in
        .checked_sub(fee_amount)
        .ok_or_else(|| bad_request("fee exceeds amount_in"))?;
    if swap_amount == 0 || fee_amount == 0 {
        return Err(bad_request(
            "amount_in is too small for this external fee tier",
        ));
    }
    let k = u128::from(reserve_in)
        .checked_mul(u128::from(reserve_out))
        .ok_or_else(|| bad_request("reserve product overflow"))?;
    let final_reserve_out = k / u128::from(
        reserve_in
            .checked_add(swap_amount)
            .ok_or_else(|| bad_request("reserve input overflow"))?,
    );
    let amount_out = u128::from(reserve_out)
        .checked_sub(final_reserve_out)
        .and_then(|value| value.checked_sub(1))
        .unwrap_or(0);
    Ok((
        u64::try_from(amount_out).map_err(|_| bad_request("quote output overflow"))?,
        fee_amount,
    ))
}

pub(super) fn quote_output_fee_cpmm(
    reserve_in: u64,
    reserve_out: u64,
    amount_in: u64,
    fee_bps: u16,
) -> ApiResult<(u64, u64)> {
    if u64::from(fee_bps) >= FEE_DENOMINATOR_BPS {
        return Err(bad_request("fee_bps must be below 10000"));
    }
    let gross_out = u64::try_from(
        u128::from(reserve_out)
            .checked_mul(u128::from(amount_in))
            .ok_or_else(|| bad_request("quote numerator overflow"))?
            / u128::from(
                reserve_in
                    .checked_add(amount_in)
                    .ok_or_else(|| bad_request("reserve input overflow"))?,
            ),
    )
    .map_err(|_| bad_request("quote output overflow"))?;
    if gross_out == 0 {
        return Err(bad_request("amount_in is too small for this external pool"));
    }
    let amount_out = u64::try_from(
        u128::from(gross_out)
            .checked_mul(u128::from(FEE_DENOMINATOR_BPS - u64::from(fee_bps)))
            .ok_or_else(|| bad_request("fee calculation overflow"))?
            / u128::from(FEE_DENOMINATOR_BPS),
    )
    .map_err(|_| bad_request("fee calculation overflow"))?;
    let fee_amount = gross_out
        .checked_sub(amount_out)
        .ok_or_else(|| bad_request("fee calculation underflow"))?;
    Ok((amount_out, fee_amount))
}

pub(super) fn price_impact_bps(
    amount_in: u64,
    amount_out: u64,
    reserve_in: u64,
    reserve_out: u64,
) -> ApiResult<u64> {
    let ideal_out = u128::from(amount_in)
        .checked_mul(u128::from(reserve_out))
        .ok_or_else(|| bad_request("price impact numerator overflow"))?
        / u128::from(reserve_in);
    if ideal_out == 0 || u128::from(amount_out) >= ideal_out {
        return Ok(0);
    }
    let diff = ideal_out - u128::from(amount_out);
    u64::try_from(
        diff.checked_mul(u128::from(FEE_DENOMINATOR_BPS))
            .ok_or_else(|| bad_request("price impact overflow"))?
            / ideal_out,
    )
    .map_err(|_| bad_request("price impact overflow"))
}

pub(super) fn reserves_for_asset(
    pool: &ExternalPoolResponse,
    asset_in: u64,
) -> ApiResult<(u64, u64, u64)> {
    if asset_in == pool.asset_0 {
        Ok((pool.reserve_0, pool.reserve_1, pool.asset_1))
    } else if asset_in == pool.asset_1 {
        Ok((pool.reserve_1, pool.reserve_0, pool.asset_0))
    } else {
        Err(bad_request(format!(
            "asset {asset_in} is not in external pool {}",
            pool.pool_id
        )))
    }
}

pub(super) fn normalize_pair_reserves(
    asset_a: u64,
    asset_b: u64,
    reserve_a: u64,
    reserve_b: u64,
) -> (u64, u64, u64, u64) {
    if asset_a < asset_b {
        (asset_a, asset_b, reserve_a, reserve_b)
    } else {
        (asset_b, asset_a, reserve_b, reserve_a)
    }
}

pub(super) fn ordered_pair(asset_a: u64, asset_b: u64) -> (u64, u64) {
    if asset_a < asset_b {
        (asset_a, asset_b)
    } else {
        (asset_b, asset_a)
    }
}
