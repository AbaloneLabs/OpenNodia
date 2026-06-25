//! Native OpenNodia AMM pool model and integer math.
//!
//! This crate is deliberately contract-agnostic. It defines the canonical pool
//! key and CPMM quote math that the backend, UI, and eventual TEAL contract
//! verification must agree on.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512_256};

pub mod contract;
pub mod transactions;

pub const CURVE_CPMM_V1: u16 = 1;
pub const CONTRACT_VERSION_V1: u16 = 1;
pub const CONTRACT_VERSION_V2: u16 = 2;
pub const CONTRACT_VERSION_V3: u16 = 3;
pub const CURRENT_CONTRACT_VERSION: u16 = CONTRACT_VERSION_V3;
pub const FEE_DENOMINATOR_BPS: u64 = 10_000;
pub const MINIMUM_LOCKED_LP: u64 = 1_000;
pub const MAX_LP_SUPPLY: u64 = u64::MAX;
pub const MAX_AMM_TXN_FEE: u64 = 20_000;
pub const REGISTRY_BOX_VALUE_BYTES: u64 = 8;
pub const REGISTRY_APP_BASE_MIN_BALANCE_MICROALGO: u64 = 100_000;
pub const REGISTRY_BOX_MIN_BALANCE_MICROALGO: u64 = 2_500 + 400 * (32 + REGISTRY_BOX_VALUE_BYTES);

pub const GLOBAL_KEY_ASSET_0: &[u8] = b"asset_0";
pub const GLOBAL_KEY_ASSET_1: &[u8] = b"asset_1";
pub const GLOBAL_KEY_CURVE_ID: &[u8] = b"curve_id";
pub const GLOBAL_KEY_FEE_BPS: &[u8] = b"fee_bps";
pub const GLOBAL_KEY_VERSION: &[u8] = b"version";
pub const GLOBAL_KEY_LP_ASSET: &[u8] = b"lp_asset";
pub const GLOBAL_KEY_RESERVE_0: &[u8] = b"reserve_0";
pub const GLOBAL_KEY_RESERVE_1: &[u8] = b"reserve_1";
pub const GLOBAL_KEY_TOTAL_LP: &[u8] = b"total_lp";
pub const GLOBAL_KEY_POOL_KEY: &[u8] = b"pool_key";
pub const GLOBAL_KEY_POOL_APPROVAL_HASH: &[u8] = b"pool_approval_hash";
pub const GLOBAL_KEY_POOL_CLEAR_HASH: &[u8] = b"pool_clear_hash";
pub const GLOBAL_KEY_REGISTRY_VERSION: &[u8] = b"registry_version";
pub const GLOBAL_KEY_REGISTRY_ACTIVE_COUNT: &[u8] = b"active_count";
pub const GLOBAL_KEY_REGISTRY_GENESIS_HASH: &[u8] = b"genesis_hash";

pub const APP_ARG_CREATE: &[u8] = b"create";
pub const APP_ARG_SETUP: &[u8] = b"setup";
pub const APP_ARG_BOOTSTRAP: &[u8] = b"bootstrap";
pub const APP_ARG_ADD: &[u8] = b"add";
pub const APP_ARG_REMOVE: &[u8] = b"remove";
pub const APP_ARG_SWAP: &[u8] = b"swap";
pub const APP_ARG_REGISTER: &[u8] = b"register";

pub fn registry_registration_funding_microalgo(active_count: u64) -> u64 {
    REGISTRY_BOX_MIN_BALANCE_MICROALGO
        + if active_count == 0 {
            REGISTRY_APP_BASE_MIN_BALANCE_MICROALGO
        } else {
            0
        }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AmmError {
    #[error("unsupported fee tier: {0} bps")]
    UnsupportedFeeTier(u16),
    #[error("assets must differ")]
    SameAsset,
    #[error("amount must be greater than zero")]
    ZeroAmount,
    #[error("pool reserves must be greater than zero")]
    EmptyPool,
    #[error("asset {0} is not in this pool")]
    AssetNotInPool(u64),
    #[error("insufficient liquidity")]
    InsufficientLiquidity,
    #[error("integer overflow")]
    Overflow,
    #[error("missing pool state key: {0}")]
    MissingStateKey(&'static str),
    #[error("invalid pool state key {0}: {1}")]
    InvalidStateKey(&'static str, &'static str),
    #[error("unsupported curve id: {0}")]
    UnsupportedCurve(u16),
    #[error("unsupported contract version: {0}")]
    UnsupportedContractVersion(u16),
    #[error("stored pool key does not match canonical key")]
    PoolKeyMismatch,
}

pub type Result<T> = std::result::Result<T, AmmError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeTier {
    bps: u16,
}

impl FeeTier {
    pub const STABLE_005: Self = Self { bps: 5 };
    pub const STANDARD_030: Self = Self { bps: 30 };
    pub const VOLATILE_100: Self = Self { bps: 100 };

    pub fn from_bps(bps: u16) -> Result<Self> {
        match bps {
            5 | 30 | 100 => Ok(Self { bps }),
            other => Err(AmmError::UnsupportedFeeTier(other)),
        }
    }

    pub const fn bps(self) -> u16 {
        self.bps
    }

    pub const fn multiplier(self) -> u64 {
        FEE_DENOMINATOR_BPS - self.bps as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoolSource {
    Native,
    Tinyman,
    Pact,
    FolksBacked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolKey {
    pub genesis_hash: [u8; 32],
    pub asset_0: u64,
    pub asset_1: u64,
    pub curve_id: u16,
    pub fee_bps: u16,
    pub contract_version: u16,
}

impl PoolKey {
    pub fn new(
        genesis_hash: [u8; 32],
        asset_a: u64,
        asset_b: u64,
        fee: FeeTier,
        contract_version: u16,
    ) -> Result<Self> {
        if asset_a == asset_b {
            return Err(AmmError::SameAsset);
        }
        let (asset_0, asset_1) = if asset_a < asset_b {
            (asset_a, asset_b)
        } else {
            (asset_b, asset_a)
        };
        Ok(Self {
            genesis_hash,
            asset_0,
            asset_1,
            curve_id: CURVE_CPMM_V1,
            fee_bps: fee.bps(),
            contract_version,
        })
    }

    pub fn id(&self) -> String {
        hex::encode(self.digest())
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha512_256::new();
        hasher.update(b"OpenNodiaPoolKeyV1");
        hasher.update(self.genesis_hash);
        hasher.update(self.asset_0.to_be_bytes());
        hasher.update(self.asset_1.to_be_bytes());
        hasher.update(self.curve_id.to_be_bytes());
        hasher.update(self.fee_bps.to_be_bytes());
        hasher.update(self.contract_version.to_be_bytes());
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }

    pub fn contains(&self, asset_id: u64) -> bool {
        self.asset_0 == asset_id || self.asset_1 == asset_id
    }

    pub fn other(&self, asset_id: u64) -> Result<u64> {
        if asset_id == self.asset_0 {
            Ok(self.asset_1)
        } else if asset_id == self.asset_1 {
            Ok(self.asset_0)
        } else {
            Err(AmmError::AssetNotInPool(asset_id))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolState {
    pub key: PoolKey,
    pub source: PoolSource,
    pub app_id: u64,
    pub lp_asset_id: u64,
    pub reserve_0: u64,
    pub reserve_1: u64,
    pub total_lp_supply: u64,
    pub source_round: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolGlobalValue {
    Uint(u64),
    Bytes(Vec<u8>),
}

pub type PoolGlobalState = HashMap<Vec<u8>, PoolGlobalValue>;

impl PoolState {
    pub fn reserve_for(&self, asset_id: u64) -> Result<u64> {
        if asset_id == self.key.asset_0 {
            Ok(self.reserve_0)
        } else if asset_id == self.key.asset_1 {
            Ok(self.reserve_1)
        } else {
            Err(AmmError::AssetNotInPool(asset_id))
        }
    }

    pub fn reserves_for_swap(&self, asset_in: u64) -> Result<(u64, u64, u64)> {
        if asset_in == self.key.asset_0 {
            Ok((self.reserve_0, self.reserve_1, self.key.asset_1))
        } else if asset_in == self.key.asset_1 {
            Ok((self.reserve_1, self.reserve_0, self.key.asset_0))
        } else {
            Err(AmmError::AssetNotInPool(asset_in))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapQuote {
    pub pool_id: String,
    pub asset_in: u64,
    pub asset_out: u64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub minimum_out: u64,
    pub fee_bps: u16,
    pub fee_amount_estimate: u64,
    pub price_impact_bps: u64,
    pub source_round: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddLiquidityQuote {
    pub amount_0: u64,
    pub amount_1: u64,
    pub minted_lp: u64,
    pub minimum_lp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveLiquidityQuote {
    pub burn_lp: u64,
    pub amount_0: u64,
    pub amount_1: u64,
    pub minimum_0: u64,
    pub minimum_1: u64,
}

pub fn quote_exact_in(
    pool: &PoolState,
    asset_in: u64,
    amount_in: u64,
    slippage_bps: u16,
) -> Result<SwapQuote> {
    if amount_in == 0 {
        return Err(AmmError::ZeroAmount);
    }
    let (reserve_in, reserve_out, asset_out) = pool.reserves_for_swap(asset_in)?;
    if reserve_in == 0 || reserve_out == 0 {
        return Err(AmmError::EmptyPool);
    }

    let fee = FeeTier::from_bps(pool.key.fee_bps)?;
    let amount_in_with_fee = u128::from(amount_in)
        .checked_mul(u128::from(fee.multiplier()))
        .ok_or(AmmError::Overflow)?;
    let numerator = amount_in_with_fee
        .checked_mul(u128::from(reserve_out))
        .ok_or(AmmError::Overflow)?;
    let denominator = u128::from(reserve_in)
        .checked_mul(u128::from(FEE_DENOMINATOR_BPS))
        .and_then(|value| value.checked_add(amount_in_with_fee))
        .ok_or(AmmError::Overflow)?;
    let amount_out = u64::try_from(numerator / denominator).map_err(|_| AmmError::Overflow)?;
    if amount_out == 0 || amount_out >= reserve_out {
        return Err(AmmError::InsufficientLiquidity);
    }
    reserve_in
        .checked_add(amount_in)
        .ok_or(AmmError::Overflow)?;
    reserve_out
        .checked_sub(amount_out)
        .ok_or(AmmError::Overflow)?;

    let minimum_out = apply_slippage_floor(amount_out, slippage_bps)?;
    let fee_amount_estimate = u64::try_from(
        u128::from(amount_in)
            .checked_mul(u128::from(pool.key.fee_bps))
            .ok_or(AmmError::Overflow)?
            / u128::from(FEE_DENOMINATOR_BPS),
    )
    .map_err(|_| AmmError::Overflow)?;

    Ok(SwapQuote {
        pool_id: pool.key.id(),
        asset_in,
        asset_out,
        amount_in,
        amount_out,
        minimum_out,
        fee_bps: pool.key.fee_bps,
        fee_amount_estimate,
        price_impact_bps: price_impact_bps(amount_in, amount_out, reserve_in, reserve_out)?,
        source_round: pool.source_round,
    })
}

pub fn quote_initial_liquidity(
    amount_0: u64,
    amount_1: u64,
    slippage_bps: u16,
) -> Result<AddLiquidityQuote> {
    if amount_0 == 0 || amount_1 == 0 {
        return Err(AmmError::ZeroAmount);
    }
    let product = u128::from(amount_0)
        .checked_mul(u128::from(amount_1))
        .ok_or(AmmError::Overflow)?;
    let root = integer_sqrt(product);
    if root <= u128::from(MINIMUM_LOCKED_LP) {
        return Err(AmmError::InsufficientLiquidity);
    }
    let minted_lp =
        u64::try_from(root - u128::from(MINIMUM_LOCKED_LP)).map_err(|_| AmmError::Overflow)?;
    minted_lp
        .checked_add(MINIMUM_LOCKED_LP)
        .ok_or(AmmError::Overflow)?;
    Ok(AddLiquidityQuote {
        amount_0,
        amount_1,
        minted_lp,
        minimum_lp: apply_slippage_floor(minted_lp, slippage_bps)?,
    })
}

pub fn quote_balanced_add(
    pool: &PoolState,
    desired_0: u64,
    desired_1: u64,
    slippage_bps: u16,
) -> Result<AddLiquidityQuote> {
    if desired_0 == 0 || desired_1 == 0 {
        return Err(AmmError::ZeroAmount);
    }
    if pool.reserve_0 == 0 || pool.reserve_1 == 0 || pool.total_lp_supply == 0 {
        return Err(AmmError::EmptyPool);
    }

    let lp_from_0 = mul_div_floor(desired_0, pool.total_lp_supply, pool.reserve_0)?;
    let lp_from_1 = mul_div_floor(desired_1, pool.total_lp_supply, pool.reserve_1)?;
    let minted_lp = lp_from_0.min(lp_from_1);
    if minted_lp == 0 {
        return Err(AmmError::InsufficientLiquidity);
    }

    let amount_0 = mul_div_ceil(minted_lp, pool.reserve_0, pool.total_lp_supply)?;
    let amount_1 = mul_div_ceil(minted_lp, pool.reserve_1, pool.total_lp_supply)?;
    pool.reserve_0
        .checked_add(amount_0)
        .ok_or(AmmError::Overflow)?;
    pool.reserve_1
        .checked_add(amount_1)
        .ok_or(AmmError::Overflow)?;
    pool.total_lp_supply
        .checked_add(minted_lp)
        .ok_or(AmmError::Overflow)?;
    Ok(AddLiquidityQuote {
        amount_0,
        amount_1,
        minted_lp,
        minimum_lp: apply_slippage_floor(minted_lp, slippage_bps)?,
    })
}

pub fn quote_remove(
    pool: &PoolState,
    burn_lp: u64,
    slippage_bps: u16,
) -> Result<RemoveLiquidityQuote> {
    if burn_lp == 0 {
        return Err(AmmError::ZeroAmount);
    }
    if pool.reserve_0 == 0 || pool.reserve_1 == 0 || pool.total_lp_supply == 0 {
        return Err(AmmError::EmptyPool);
    }
    if burn_lp >= pool.total_lp_supply.saturating_sub(MINIMUM_LOCKED_LP) {
        return Err(AmmError::InsufficientLiquidity);
    }

    let amount_0 = mul_div_floor(burn_lp, pool.reserve_0, pool.total_lp_supply)?;
    let amount_1 = mul_div_floor(burn_lp, pool.reserve_1, pool.total_lp_supply)?;
    if amount_0 == 0 || amount_1 == 0 {
        return Err(AmmError::InsufficientLiquidity);
    }
    Ok(RemoveLiquidityQuote {
        burn_lp,
        amount_0,
        amount_1,
        minimum_0: apply_slippage_floor(amount_0, slippage_bps)?,
        minimum_1: apply_slippage_floor(amount_1, slippage_bps)?,
    })
}

pub fn decode_pool_state(
    app_id: u64,
    genesis_hash: [u8; 32],
    source_round: u64,
    global_state: &PoolGlobalState,
) -> Result<PoolState> {
    let asset_0 = require_uint(global_state, GLOBAL_KEY_ASSET_0, "asset_0")?;
    let asset_1 = require_uint(global_state, GLOBAL_KEY_ASSET_1, "asset_1")?;
    let curve_id = u16::try_from(require_uint(global_state, GLOBAL_KEY_CURVE_ID, "curve_id")?)
        .map_err(|_| AmmError::InvalidStateKey("curve_id", "must fit in u16"))?;
    if curve_id != CURVE_CPMM_V1 {
        return Err(AmmError::UnsupportedCurve(curve_id));
    }

    let fee = FeeTier::from_bps(
        u16::try_from(require_uint(global_state, GLOBAL_KEY_FEE_BPS, "fee_bps")?)
            .map_err(|_| AmmError::InvalidStateKey("fee_bps", "must fit in u16"))?,
    )?;
    let contract_version =
        u16::try_from(require_uint(global_state, GLOBAL_KEY_VERSION, "version")?)
            .map_err(|_| AmmError::InvalidStateKey("version", "must fit in u16"))?;
    if !matches!(contract_version, CONTRACT_VERSION_V2 | CONTRACT_VERSION_V3) {
        return Err(AmmError::UnsupportedContractVersion(contract_version));
    }

    let key = PoolKey::new(genesis_hash, asset_0, asset_1, fee, contract_version)?;
    if let Some(stored_key) = optional_bytes(global_state, GLOBAL_KEY_POOL_KEY, "pool_key")? {
        let expected_key = key.digest();
        if stored_key.as_slice() != expected_key.as_slice() {
            return Err(AmmError::PoolKeyMismatch);
        }
    }

    Ok(PoolState {
        key,
        source: PoolSource::Native,
        app_id,
        lp_asset_id: require_uint(global_state, GLOBAL_KEY_LP_ASSET, "lp_asset")?,
        reserve_0: require_uint(global_state, GLOBAL_KEY_RESERVE_0, "reserve_0")?,
        reserve_1: require_uint(global_state, GLOBAL_KEY_RESERVE_1, "reserve_1")?,
        total_lp_supply: require_uint(global_state, GLOBAL_KEY_TOTAL_LP, "total_lp")?,
        source_round,
    })
}

pub fn apply_slippage_floor(amount: u64, slippage_bps: u16) -> Result<u64> {
    if slippage_bps as u64 > FEE_DENOMINATOR_BPS {
        return Err(AmmError::Overflow);
    }
    let keep_bps = FEE_DENOMINATOR_BPS - u64::from(slippage_bps);
    mul_div_floor(amount, keep_bps, FEE_DENOMINATOR_BPS)
}

pub fn mul_div_floor(value: u64, numerator: u64, denominator: u64) -> Result<u64> {
    if denominator == 0 {
        return Err(AmmError::Overflow);
    }
    let out = u128::from(value)
        .checked_mul(u128::from(numerator))
        .ok_or(AmmError::Overflow)?
        / u128::from(denominator);
    u64::try_from(out).map_err(|_| AmmError::Overflow)
}

pub fn mul_div_ceil(value: u64, numerator: u64, denominator: u64) -> Result<u64> {
    if denominator == 0 {
        return Err(AmmError::Overflow);
    }
    let product = u128::from(value)
        .checked_mul(u128::from(numerator))
        .ok_or(AmmError::Overflow)?;
    let out = product
        .checked_add(u128::from(denominator - 1))
        .ok_or(AmmError::Overflow)?
        / u128::from(denominator);
    u64::try_from(out).map_err(|_| AmmError::Overflow)
}

fn price_impact_bps(
    amount_in: u64,
    amount_out: u64,
    reserve_in: u64,
    reserve_out: u64,
) -> Result<u64> {
    let ideal_out = u128::from(amount_in)
        .checked_mul(u128::from(reserve_out))
        .ok_or(AmmError::Overflow)?
        / u128::from(reserve_in);
    if ideal_out == 0 || u128::from(amount_out) >= ideal_out {
        return Ok(0);
    }
    let diff = ideal_out - u128::from(amount_out);
    let bps = diff
        .checked_mul(u128::from(FEE_DENOMINATOR_BPS))
        .ok_or(AmmError::Overflow)?
        / ideal_out;
    u64::try_from(bps).map_err(|_| AmmError::Overflow)
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut x0 = value / 2;
    let mut x1 = (x0 + value / x0) / 2;
    while x1 < x0 {
        x0 = x1;
        x1 = (x0 + value / x0) / 2;
    }
    x0
}

fn require_uint(state: &PoolGlobalState, key: &'static [u8], name: &'static str) -> Result<u64> {
    match state.get(key) {
        Some(PoolGlobalValue::Uint(value)) => Ok(*value),
        Some(PoolGlobalValue::Bytes(_)) => Err(AmmError::InvalidStateKey(name, "expected uint")),
        None => Err(AmmError::MissingStateKey(name)),
    }
}

fn optional_bytes(
    state: &PoolGlobalState,
    key: &'static [u8],
    name: &'static str,
) -> Result<Option<Vec<u8>>> {
    match state.get(key) {
        Some(PoolGlobalValue::Bytes(value)) => Ok(Some(value.clone())),
        Some(PoolGlobalValue::Uint(_)) => Err(AmmError::InvalidStateKey(name, "expected bytes")),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(asset_a: u64, asset_b: u64, fee_bps: u16) -> PoolKey {
        PoolKey::new(
            [7u8; 32],
            asset_a,
            asset_b,
            FeeTier::from_bps(fee_bps).unwrap(),
            CURRENT_CONTRACT_VERSION,
        )
        .unwrap()
    }

    fn pool() -> PoolState {
        PoolState {
            key: key(0, 42, 30),
            source: PoolSource::Native,
            app_id: 10,
            lp_asset_id: 11,
            reserve_0: 1_000_000_000,
            reserve_1: 2_000_000,
            total_lp_supply: 44_721_359,
            source_round: 123,
        }
    }

    fn global_state() -> PoolGlobalState {
        let key = key(0, 42, 30);
        HashMap::from([
            (
                GLOBAL_KEY_ASSET_0.to_vec(),
                PoolGlobalValue::Uint(key.asset_0),
            ),
            (
                GLOBAL_KEY_ASSET_1.to_vec(),
                PoolGlobalValue::Uint(key.asset_1),
            ),
            (
                GLOBAL_KEY_CURVE_ID.to_vec(),
                PoolGlobalValue::Uint(u64::from(CURVE_CPMM_V1)),
            ),
            (
                GLOBAL_KEY_FEE_BPS.to_vec(),
                PoolGlobalValue::Uint(u64::from(key.fee_bps)),
            ),
            (
                GLOBAL_KEY_VERSION.to_vec(),
                PoolGlobalValue::Uint(u64::from(CURRENT_CONTRACT_VERSION)),
            ),
            (GLOBAL_KEY_LP_ASSET.to_vec(), PoolGlobalValue::Uint(99)),
            (
                GLOBAL_KEY_RESERVE_0.to_vec(),
                PoolGlobalValue::Uint(1_000_000),
            ),
            (
                GLOBAL_KEY_RESERVE_1.to_vec(),
                PoolGlobalValue::Uint(2_000_000),
            ),
            (
                GLOBAL_KEY_TOTAL_LP.to_vec(),
                PoolGlobalValue::Uint(1_414_213),
            ),
            (
                GLOBAL_KEY_POOL_KEY.to_vec(),
                PoolGlobalValue::Bytes(key.digest().to_vec()),
            ),
        ])
    }

    #[test]
    fn fee_tiers_are_limited() {
        assert_eq!(FeeTier::from_bps(5).unwrap().bps(), 5);
        assert_eq!(FeeTier::from_bps(30).unwrap().bps(), 30);
        assert_eq!(FeeTier::from_bps(100).unwrap().bps(), 100);
        assert!(matches!(
            FeeTier::from_bps(25),
            Err(AmmError::UnsupportedFeeTier(25))
        ));
    }

    #[test]
    fn pool_key_is_canonical_by_asset_id() {
        let a = key(0, 42, 30);
        let b = key(42, 0, 30);
        assert_eq!(a, b);
        assert_eq!(a.asset_0, 0);
        assert_eq!(a.asset_1, 42);
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn quote_exact_in_uses_cpmm_fee_math() {
        let quote = quote_exact_in(&pool(), 0, 10_000_000, 50).unwrap();
        assert_eq!(quote.asset_out, 42);
        assert_eq!(quote.amount_out, 19_743);
        assert_eq!(quote.minimum_out, 19_644);
        assert_eq!(quote.fee_bps, 30);
        assert_eq!(quote.source_round, 123);
    }

    #[test]
    fn quote_works_in_reverse_direction() {
        let quote = quote_exact_in(&pool(), 42, 10_000, 100).unwrap();
        assert_eq!(quote.asset_out, 0);
        assert_eq!(quote.amount_out, 4_960_273);
        assert_eq!(quote.minimum_out, 4_910_670);
    }

    #[test]
    fn cpmm_golden_vectors_cover_fee_tiers_and_directions() {
        let cases = [
            (5, 0, 10_000_000, 19_792, 19_693, 5_000),
            (5, 42, 10_000, 4_972_649, 4_947_785, 5),
            (30, 0, 10_000_000, 19_743, 19_644, 30_000),
            (30, 42, 10_000, 4_960_273, 4_935_471, 30),
            (100, 0, 10_000_000, 19_605, 19_506, 100_000),
            (100, 42, 10_000, 4_925_618, 4_900_989, 100),
        ];

        for (fee_bps, asset_in, amount_in, amount_out, minimum_out, fee_estimate) in cases {
            let mut pool = pool();
            pool.key = key(0, 42, fee_bps);
            let quote = quote_exact_in(&pool, asset_in, amount_in, 50).unwrap();
            assert_eq!(quote.amount_out, amount_out);
            assert_eq!(quote.minimum_out, minimum_out);
            assert_eq!(quote.fee_amount_estimate, fee_estimate);
        }
    }

    #[test]
    fn swaps_preserve_constant_product_after_fee_rounding() {
        let cases = [(0, 10_000), (0, 10_000_000), (42, 10_000), (42, 1_000_000)];

        for (asset_in, amount_in) in cases {
            let pool = pool();
            let quote = quote_exact_in(&pool, asset_in, amount_in, 50).unwrap();
            let (reserve_in, reserve_out, _) = pool.reserves_for_swap(asset_in).unwrap();
            let before = u128::from(reserve_in) * u128::from(reserve_out);
            let after = u128::from(reserve_in + quote.amount_in)
                * u128::from(reserve_out - quote.amount_out);
            assert!(after >= before);
            assert!(quote.amount_out < reserve_out);
            assert!(quote.minimum_out <= quote.amount_out);
        }
    }

    #[test]
    fn initial_liquidity_locks_minimum_lp() {
        let quote = quote_initial_liquidity(1_000_000_000, 2_000_000, 50).unwrap();
        assert_eq!(quote.minted_lp, 44_720_359);
        assert_eq!(quote.minimum_lp, 44_496_757);
    }

    #[test]
    fn initial_liquidity_rejects_dust_that_cannot_exceed_locked_lp() {
        assert!(matches!(
            quote_initial_liquidity(1_000, 1_000, 50),
            Err(AmmError::InsufficientLiquidity)
        ));
        assert!(matches!(
            quote_initial_liquidity(0, 1_000_000, 50),
            Err(AmmError::ZeroAmount)
        ));
    }

    #[test]
    fn initial_liquidity_rounding_never_mints_locked_lp() {
        let cases = [
            (1_001, 1_001),
            (1_001, 1_003),
            (2_500, 777),
            (10_000, 10_000),
            (1_000_000_000, 2_000_000),
        ];

        for (amount_0, amount_1) in cases {
            let root = integer_sqrt(u128::from(amount_0) * u128::from(amount_1));
            let quote = quote_initial_liquidity(amount_0, amount_1, 0).unwrap();
            assert_eq!(
                u128::from(quote.minted_lp) + u128::from(MINIMUM_LOCKED_LP),
                root
            );
            assert!(quote.minimum_lp <= quote.minted_lp);
        }
    }

    #[test]
    fn balanced_add_uses_limiting_side() {
        let quote = quote_balanced_add(&pool(), 100_000_000, 300_000, 50).unwrap();
        assert_eq!(quote.amount_0, 99_999_980);
        assert_eq!(quote.amount_1, 200_000);
        assert_eq!(quote.minted_lp, 4_472_135);
    }

    #[test]
    fn balanced_add_never_mints_from_the_donated_side() {
        let pool = pool();
        let cases = [
            (100_000_000, 300_000),
            (100_000_000, 9_000_000),
            (1_000_000, 10_000),
            (9_000_000_000, 1_000_000),
            (u64::MAX / 100_000, 10_000),
        ];

        for (desired_0, desired_1) in cases {
            let quote = quote_balanced_add(&pool, desired_0, desired_1, 50).unwrap();
            let lp_from_0 = mul_div_floor(desired_0, pool.total_lp_supply, pool.reserve_0).unwrap();
            let lp_from_1 = mul_div_floor(desired_1, pool.total_lp_supply, pool.reserve_1).unwrap();
            assert_eq!(quote.minted_lp, lp_from_0.min(lp_from_1));
            assert!(quote.amount_0 <= desired_0);
            assert!(quote.amount_1 <= desired_1);
            assert!(quote.minimum_lp <= quote.minted_lp);
        }
    }

    #[test]
    fn add_liquidity_rounding_cannot_inflate_lp_supply() {
        let mut pool = pool();
        pool.reserve_0 = 3_333_333;
        pool.reserve_1 = 7_777;
        pool.total_lp_supply = 161_111;
        let cases = [(1, 1), (99, 13), (10_000, 20), (500_000, 1_234)];

        for (desired_0, desired_1) in cases {
            let Ok(quote) = quote_balanced_add(&pool, desired_0, desired_1, 0) else {
                continue;
            };
            let max_lp_from_used_0 =
                mul_div_floor(quote.amount_0, pool.total_lp_supply, pool.reserve_0).unwrap();
            let max_lp_from_used_1 =
                mul_div_floor(quote.amount_1, pool.total_lp_supply, pool.reserve_1).unwrap();
            assert!(quote.minted_lp <= max_lp_from_used_0);
            assert!(quote.minted_lp <= max_lp_from_used_1);
            assert!(pool.total_lp_supply.checked_add(quote.minted_lp).is_some());
        }
    }

    #[test]
    fn tiny_swap_inputs_do_not_create_zero_output_quotes() {
        let mut pool = pool();
        pool.reserve_0 = 1_000_000_000;
        pool.reserve_1 = 2;
        assert!(matches!(
            quote_exact_in(&pool, 0, 1, 0),
            Err(AmmError::InsufficientLiquidity)
        ));

        pool.reserve_0 = 2;
        pool.reserve_1 = 1_000_000_000;
        assert!(matches!(
            quote_exact_in(&pool, 42, 1, 0),
            Err(AmmError::InsufficientLiquidity)
        ));
    }

    #[test]
    fn stateful_rounding_edge_sequence_preserves_locked_lp() {
        let mut pool = pool();
        pool.reserve_0 = 3_333_333;
        pool.reserve_1 = 7_777;
        pool.total_lp_supply = 161_111;
        let mut previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);

        for step in 0..36u64 {
            match step % 3 {
                0 => {
                    let quote = quote_exact_in(&pool, 0, 1_000 + step * 37, 0).unwrap();
                    apply_swap(&mut pool, &quote);
                    let product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                    assert!(product >= previous_product);
                    previous_product = product;
                }
                1 => {
                    let quote = quote_balanced_add(&pool, 99 + step, 1 + step % 5, 0).unwrap();
                    let lp_before = pool.total_lp_supply;
                    apply_add(&mut pool, &quote);
                    assert!(pool.total_lp_supply > lp_before);
                    previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                }
                _ => {
                    let burn = ((pool.total_lp_supply - MINIMUM_LOCKED_LP) / 23).max(1);
                    let quote = quote_remove(&pool, burn, 0).unwrap();
                    assert!(quote.amount_0 < pool.reserve_0);
                    assert!(quote.amount_1 < pool.reserve_1);
                    apply_remove(&mut pool, &quote);
                    assert!(pool.total_lp_supply > MINIMUM_LOCKED_LP);
                    previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                }
            }

            assert_pool_invariants(&pool);
        }
    }

    fn apply_add(pool: &mut PoolState, quote: &AddLiquidityQuote) {
        pool.reserve_0 = pool.reserve_0.checked_add(quote.amount_0).unwrap();
        pool.reserve_1 = pool.reserve_1.checked_add(quote.amount_1).unwrap();
        pool.total_lp_supply = pool.total_lp_supply.checked_add(quote.minted_lp).unwrap();
        pool.source_round += 1;
    }

    fn apply_remove(pool: &mut PoolState, quote: &RemoveLiquidityQuote) {
        pool.reserve_0 = pool.reserve_0.checked_sub(quote.amount_0).unwrap();
        pool.reserve_1 = pool.reserve_1.checked_sub(quote.amount_1).unwrap();
        pool.total_lp_supply = pool.total_lp_supply.checked_sub(quote.burn_lp).unwrap();
        pool.source_round += 1;
    }

    fn apply_swap(pool: &mut PoolState, quote: &SwapQuote) {
        if quote.asset_in == pool.key.asset_0 {
            pool.reserve_0 = pool.reserve_0.checked_add(quote.amount_in).unwrap();
            pool.reserve_1 = pool.reserve_1.checked_sub(quote.amount_out).unwrap();
        } else {
            pool.reserve_1 = pool.reserve_1.checked_add(quote.amount_in).unwrap();
            pool.reserve_0 = pool.reserve_0.checked_sub(quote.amount_out).unwrap();
        }
        pool.source_round += 1;
    }

    fn assert_pool_invariants(pool: &PoolState) {
        assert!(pool.reserve_0 > 0);
        assert!(pool.reserve_1 > 0);
        assert!(pool.total_lp_supply > MINIMUM_LOCKED_LP);
        assert!(pool.key.contains(pool.key.asset_0));
        assert!(pool.key.contains(pool.key.asset_1));
    }

    #[test]
    fn stateful_quote_sequence_preserves_pool_invariants() {
        let mut pool = pool();
        let mut previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);

        for step in 0..48u64 {
            match step % 4 {
                0 => {
                    let quote = quote_exact_in(&pool, 0, 1_000 + step * 17, 25).unwrap();
                    apply_swap(&mut pool, &quote);
                    let product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                    assert!(product >= previous_product);
                    previous_product = product;
                }
                1 => {
                    let quote = quote_exact_in(&pool, 42, 25 + step * 3, 25).unwrap();
                    apply_swap(&mut pool, &quote);
                    let product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                    assert!(product >= previous_product);
                    previous_product = product;
                }
                2 => {
                    let quote =
                        quote_balanced_add(&pool, 10_000 + step * 101, 50 + step, 25).unwrap();
                    let lp_before = pool.total_lp_supply;
                    apply_add(&mut pool, &quote);
                    assert!(pool.total_lp_supply > lp_before);
                    previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                }
                _ => {
                    let burn = (pool.total_lp_supply - MINIMUM_LOCKED_LP) / 17;
                    let quote = quote_remove(&pool, burn.max(1), 25).unwrap();
                    let lp_before = pool.total_lp_supply;
                    apply_remove(&mut pool, &quote);
                    assert!(pool.total_lp_supply < lp_before);
                    previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                }
            }
            assert_pool_invariants(&pool);
        }
    }

    #[test]
    fn donation_imbalanced_stateful_sequence_preserves_lp_accounting() {
        enum Action {
            Donate0(u64),
            Donate1(u64),
            Add(u64, u64),
            Swap(u64, u64),
            RemoveByDivisor(u64),
        }

        let mut pool = pool();
        let actions = [
            Action::Donate0(17_000_000),
            Action::Add(80_000_000, 25_000),
            Action::Swap(0, 4_000_000),
            Action::Donate1(77_777),
            Action::Add(2_000_000, 90_000),
            Action::Swap(42, 8_500),
            Action::RemoveByDivisor(11),
            Action::Donate0(3),
            Action::Add(15_000, 15),
            Action::Swap(0, 1_111),
            Action::RemoveByDivisor(19),
            Action::Donate1(1),
        ];
        let mut previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);

        for action in actions {
            match action {
                Action::Donate0(amount) => {
                    let lp_before = pool.total_lp_supply;
                    pool.reserve_0 = pool.reserve_0.checked_add(amount).unwrap();
                    pool.source_round += 1;
                    assert_eq!(pool.total_lp_supply, lp_before);
                    let product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                    assert!(product >= previous_product);
                    previous_product = product;
                }
                Action::Donate1(amount) => {
                    let lp_before = pool.total_lp_supply;
                    pool.reserve_1 = pool.reserve_1.checked_add(amount).unwrap();
                    pool.source_round += 1;
                    assert_eq!(pool.total_lp_supply, lp_before);
                    let product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                    assert!(product >= previous_product);
                    previous_product = product;
                }
                Action::Add(desired_0, desired_1) => {
                    let quote = quote_balanced_add(&pool, desired_0, desired_1, 25).unwrap();
                    let lp_from_0 =
                        mul_div_floor(desired_0, pool.total_lp_supply, pool.reserve_0).unwrap();
                    let lp_from_1 =
                        mul_div_floor(desired_1, pool.total_lp_supply, pool.reserve_1).unwrap();
                    let lp_before = pool.total_lp_supply;
                    assert_eq!(quote.minted_lp, lp_from_0.min(lp_from_1));
                    assert!(quote.amount_0 <= desired_0);
                    assert!(quote.amount_1 <= desired_1);
                    assert!(quote.minimum_lp <= quote.minted_lp);
                    apply_add(&mut pool, &quote);
                    assert_eq!(pool.total_lp_supply, lp_before + quote.minted_lp);
                    previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                }
                Action::Swap(asset_in, amount_in) => {
                    let quote = quote_exact_in(&pool, asset_in, amount_in, 25).unwrap();
                    let (reserve_in, reserve_out, _) = pool.reserves_for_swap(asset_in).unwrap();
                    let before = u128::from(reserve_in) * u128::from(reserve_out);
                    assert!(quote.minimum_out <= quote.amount_out);
                    assert!(quote.amount_out < reserve_out);
                    apply_swap(&mut pool, &quote);
                    let (new_reserve_in, new_reserve_out, _) =
                        pool.reserves_for_swap(asset_in).unwrap();
                    let after = u128::from(new_reserve_in) * u128::from(new_reserve_out);
                    assert!(after >= before);
                    assert!(after >= previous_product);
                    previous_product = after;
                }
                Action::RemoveByDivisor(divisor) => {
                    let burn = ((pool.total_lp_supply - MINIMUM_LOCKED_LP) / divisor).max(1);
                    let quote = quote_remove(&pool, burn, 25).unwrap();
                    let lp_before = pool.total_lp_supply;
                    assert!(quote.minimum_0 <= quote.amount_0);
                    assert!(quote.minimum_1 <= quote.amount_1);
                    assert!(quote.amount_0 < pool.reserve_0);
                    assert!(quote.amount_1 < pool.reserve_1);
                    apply_remove(&mut pool, &quote);
                    assert_eq!(pool.total_lp_supply, lp_before - burn);
                    assert!(pool.total_lp_supply > MINIMUM_LOCKED_LP);
                    previous_product = u128::from(pool.reserve_0) * u128::from(pool.reserve_1);
                }
            }

            assert_pool_invariants(&pool);
        }
    }

    #[test]
    fn imbalanced_deposits_do_not_mint_against_donated_side() {
        let mut pool = pool();
        let desired_0_values = [1, 99, 10_000, pool.reserve_0 / 3, pool.reserve_0 * 7];
        let desired_1_values = [1, 11, 1_000, pool.reserve_1 / 2, pool.reserve_1 * 13];

        for desired_0 in desired_0_values {
            for desired_1 in desired_1_values {
                let Ok(quote) = quote_balanced_add(&pool, desired_0, desired_1, 0) else {
                    continue;
                };
                let lp_from_0 =
                    mul_div_floor(desired_0, pool.total_lp_supply, pool.reserve_0).unwrap();
                let lp_from_1 =
                    mul_div_floor(desired_1, pool.total_lp_supply, pool.reserve_1).unwrap();
                assert_eq!(quote.minted_lp, lp_from_0.min(lp_from_1));
                assert!(quote.amount_0 <= desired_0);
                assert!(quote.amount_1 <= desired_1);
                assert!(quote.amount_0 > 0);
                assert!(quote.amount_1 > 0);
            }
        }

        let quote = quote_balanced_add(&pool, pool.reserve_0 * 100, 2, 0).unwrap();
        apply_add(&mut pool, &quote);
        assert_pool_invariants(&pool);
    }

    #[test]
    fn balanced_add_rejects_empty_or_partial_pool() {
        let mut pool = pool();
        pool.reserve_1 = 0;
        assert!(matches!(
            quote_balanced_add(&pool, 100_000, 100_000, 50),
            Err(AmmError::EmptyPool)
        ));
        pool.reserve_1 = 2_000_000;
        pool.total_lp_supply = 0;
        assert!(matches!(
            quote_balanced_add(&pool, 100_000, 100_000, 50),
            Err(AmmError::EmptyPool)
        ));
    }

    #[test]
    fn remove_liquidity_returns_proportional_reserves() {
        let quote = quote_remove(&pool(), 4_472_135, 50).unwrap();
        assert_eq!(quote.amount_0, 99_999_979);
        assert_eq!(quote.amount_1, 199_999);
        assert_eq!(quote.minimum_0, 99_499_979);
        assert_eq!(quote.minimum_1, 198_999);
    }

    #[test]
    fn remove_liquidity_cannot_drain_locked_liquidity() {
        let pool = pool();
        assert!(matches!(
            quote_remove(&pool, pool.total_lp_supply, 50),
            Err(AmmError::InsufficientLiquidity)
        ));
        assert!(matches!(
            quote_remove(&pool, pool.total_lp_supply - MINIMUM_LOCKED_LP, 50),
            Err(AmmError::InsufficientLiquidity)
        ));
    }

    #[test]
    fn remove_liquidity_rounding_cannot_touch_locked_liquidity() {
        let pool = pool();
        let burns = [
            1_000,
            pool.total_lp_supply / 3,
            pool.total_lp_supply - MINIMUM_LOCKED_LP - 1,
        ];

        for burn_lp in burns {
            let quote = quote_remove(&pool, burn_lp, 50).unwrap();
            assert!(quote.amount_0 < pool.reserve_0);
            assert!(quote.amount_1 < pool.reserve_1);
            assert!(burn_lp + MINIMUM_LOCKED_LP < pool.total_lp_supply);
            assert!(quote.minimum_0 <= quote.amount_0);
            assert!(quote.minimum_1 <= quote.amount_1);
        }
    }

    #[test]
    fn quote_swap_rejects_post_reserve_overflow() {
        let mut pool = pool();
        pool.reserve_0 = u64::MAX - 10;
        pool.reserve_1 = 1_000_000;

        assert!(matches!(
            quote_exact_in(&pool, 0, 1_000_000_000_000_000, 50),
            Err(AmmError::Overflow)
        ));
    }

    #[test]
    fn quote_add_rejects_post_reserve_overflow() {
        let mut pool = pool();
        pool.reserve_0 = u64::MAX - 10;
        pool.reserve_1 = 1_000_000;
        pool.total_lp_supply = 1_000_000;

        assert!(matches!(
            quote_balanced_add(&pool, u64::MAX, 1, 50),
            Err(AmmError::Overflow)
        ));
    }

    #[test]
    fn decodes_native_pool_global_state() {
        let decoded = decode_pool_state(123, [7u8; 32], 555, &global_state()).unwrap();
        assert_eq!(decoded.app_id, 123);
        assert_eq!(decoded.lp_asset_id, 99);
        assert_eq!(decoded.reserve_0, 1_000_000);
        assert_eq!(decoded.reserve_1, 2_000_000);
        assert_eq!(decoded.total_lp_supply, 1_414_213);
        assert_eq!(decoded.source_round, 555);
        assert_eq!(decoded.key.id(), key(42, 0, 30).id());
    }

    #[test]
    fn rejects_pool_key_mismatch() {
        let mut state = global_state();
        state.insert(
            GLOBAL_KEY_POOL_KEY.to_vec(),
            PoolGlobalValue::Bytes(vec![1; 32]),
        );

        assert!(matches!(
            decode_pool_state(123, [7u8; 32], 555, &state),
            Err(AmmError::PoolKeyMismatch)
        ));
    }

    #[test]
    fn rejects_wrong_state_type() {
        let mut state = global_state();
        state.insert(
            GLOBAL_KEY_RESERVE_0.to_vec(),
            PoolGlobalValue::Bytes(vec![0]),
        );

        assert!(matches!(
            decode_pool_state(123, [7u8; 32], 555, &state),
            Err(AmmError::InvalidStateKey("reserve_0", "expected uint"))
        ));
    }
}
