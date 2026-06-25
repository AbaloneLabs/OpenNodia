use super::*;

pub(super) fn mul_div_floor(value: u64, numerator: u64, denominator: u64) -> ApiResult<u64> {
    if denominator == 0 {
        return Err(bad_request("division by zero"));
    }
    let out = u128::from(value)
        .checked_mul(u128::from(numerator))
        .ok_or_else(|| bad_request("multiplication overflow"))?
        / u128::from(denominator);
    u64::try_from(out).map_err(|_| bad_request("calculation overflow"))
}

pub(super) fn mul_div_ceil(value: u64, numerator: u64, denominator: u64) -> ApiResult<u64> {
    if denominator == 0 {
        return Err(bad_request("division by zero"));
    }
    let product = u128::from(value)
        .checked_mul(u128::from(numerator))
        .ok_or_else(|| bad_request("multiplication overflow"))?;
    let denominator = u128::from(denominator);
    let out = product
        .checked_add(denominator - 1)
        .ok_or_else(|| bad_request("calculation overflow"))?
        / denominator;
    u64::try_from(out).map_err(|_| bad_request("calculation overflow"))
}
