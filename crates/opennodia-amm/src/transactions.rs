//! Native AMM transaction group builders.
//!
//! These builders define the client-side group layout that the stateful AMM
//! approval program validates. They do not sign or submit transactions.

use opennodia_core::{Address, Round};
use opennodia_swap::{
    assign_group_id, build_application_call, build_application_create, build_asset_transfer,
    build_payment, BoxReference, StateSchema, TransactionFields, TransactionParams,
};
use sha2::{Digest, Sha512_256};

use crate::{
    registry_registration_funding_microalgo, FeeTier, PoolKey, Result, APP_ARG_ADD,
    APP_ARG_BOOTSTRAP, APP_ARG_CREATE, APP_ARG_REGISTER, APP_ARG_REMOVE, APP_ARG_SETUP,
    APP_ARG_SWAP, CURRENT_CONTRACT_VERSION, MAX_AMM_TXN_FEE, MAX_LP_SUPPLY,
};

pub const POOL_GLOBAL_UINTS: u64 = 9;
pub const POOL_GLOBAL_BYTES: u64 = 1;
pub const POOL_LOCAL_UINTS: u64 = 0;
pub const POOL_LOCAL_BYTES: u64 = 0;
pub const POOL_SETUP_BASE_FUNDING_MICROALGO: u64 = 500_000;
pub const REGISTRY_GLOBAL_UINTS: u64 = 2;
pub const REGISTRY_GLOBAL_BYTES: u64 = 3;
pub const REGISTRY_LOCAL_UINTS: u64 = 0;
pub const REGISTRY_LOCAL_BYTES: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolCreateDraft {
    pub pool_key: PoolKey,
    pub tx: TransactionFields,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredPoolCreateDraft {
    pub pool_key: PoolKey,
    pub txs: Vec<TransactionFields>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolCreateRequest {
    pub creator: Address,
    pub genesis_hash: [u8; 32],
    pub asset_a: u64,
    pub asset_b: u64,
    pub fee: FeeTier,
    pub approval_program: Vec<u8>,
    pub clear_state_program: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryCreateDraft {
    pub pool_approval_hash: [u8; 32],
    pub pool_clear_hash: [u8; 32],
    pub tx: TransactionFields,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryCreateRequest {
    pub creator: Address,
    pub genesis_hash: [u8; 32],
    pub registry_approval_program: Vec<u8>,
    pub registry_clear_state_program: Vec<u8>,
    pub pool_approval_program: Vec<u8>,
    pub pool_clear_state_program: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredPoolCreateRequest {
    pub pool: PoolCreateRequest,
    pub registry_app_id: u64,
    pub registry_app_address: Address,
    pub registry_active_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolGroupDraft {
    pub txs: Vec<TransactionFields>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolSetupRequest {
    pub creator: Address,
    pub app_id: u64,
    pub app_address: Address,
    pub pool_key: PoolKey,
    pub funding_microalgo: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapRequest {
    pub provider: Address,
    pub app_id: u64,
    pub app_address: Address,
    pub pool_key: PoolKey,
    pub lp_asset_id: u64,
    pub amount_0: u64,
    pub amount_1: u64,
    pub minimum_lp: u64,
    pub deadline: Round,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddLiquidityRequest {
    pub provider: Address,
    pub app_id: u64,
    pub app_address: Address,
    pub pool_key: PoolKey,
    pub lp_asset_id: u64,
    pub amount_0: u64,
    pub amount_1: u64,
    pub minimum_lp: u64,
    pub deadline: Round,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveLiquidityRequest {
    pub provider: Address,
    pub app_id: u64,
    pub app_address: Address,
    pub pool_key: PoolKey,
    pub lp_asset_id: u64,
    pub burn_lp: u64,
    pub minimum_0: u64,
    pub minimum_1: u64,
    pub deadline: Round,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapRequest {
    pub trader: Address,
    pub app_id: u64,
    pub app_address: Address,
    pub pool_key: PoolKey,
    pub asset_in: u64,
    pub amount_in: u64,
    pub minimum_out: u64,
    pub deadline: Round,
}

pub fn build_pool_create(
    request: PoolCreateRequest,
    tx_params: &TransactionParams,
) -> Result<PoolCreateDraft> {
    validate_base_fee(tx_params)?;
    let pool_key = PoolKey::new(
        request.genesis_hash,
        request.asset_a,
        request.asset_b,
        request.fee,
        CURRENT_CONTRACT_VERSION,
    )?;
    let tx = build_application_create(
        request.creator,
        request.approval_program,
        request.clear_state_program,
        StateSchema::new(POOL_GLOBAL_UINTS, POOL_GLOBAL_BYTES),
        StateSchema::new(POOL_LOCAL_UINTS, POOL_LOCAL_BYTES),
        vec![
            APP_ARG_CREATE.to_vec(),
            u64_arg(pool_key.asset_0),
            u64_arg(pool_key.asset_1),
            u64_arg(u64::from(pool_key.fee_bps)),
            pool_key.digest().to_vec(),
        ],
        tx_params,
    );
    Ok(PoolCreateDraft { pool_key, tx })
}

pub fn build_registry_create(
    request: RegistryCreateRequest,
    tx_params: &TransactionParams,
) -> Result<RegistryCreateDraft> {
    validate_base_fee(tx_params)?;
    let pool_approval_hash = program_hash(&request.pool_approval_program);
    let pool_clear_hash = program_hash(&request.pool_clear_state_program);
    let tx = build_application_create(
        request.creator,
        request.registry_approval_program,
        request.registry_clear_state_program,
        StateSchema::new(REGISTRY_GLOBAL_UINTS, REGISTRY_GLOBAL_BYTES),
        StateSchema::new(REGISTRY_LOCAL_UINTS, REGISTRY_LOCAL_BYTES),
        vec![
            APP_ARG_CREATE.to_vec(),
            request.genesis_hash.to_vec(),
            pool_approval_hash.to_vec(),
            pool_clear_hash.to_vec(),
        ],
        tx_params,
    );
    Ok(RegistryCreateDraft {
        pool_approval_hash,
        pool_clear_hash,
        tx,
    })
}

pub fn build_registered_pool_create(
    request: RegisteredPoolCreateRequest,
    tx_params: &TransactionParams,
) -> Result<RegisteredPoolCreateDraft> {
    validate_pool_addresses(request.registry_app_id, request.registry_app_address);
    let pool = build_pool_create(request.pool, tx_params)?;
    let registry_funding = registry_registration_funding_microalgo(request.registry_active_count);
    let funding = build_payment(
        pool.tx.sender,
        request.registry_app_address,
        registry_funding,
        tx_params,
    );
    let mut register = build_application_call(
        pool.tx.sender,
        request.registry_app_id,
        vec![APP_ARG_REGISTER.to_vec(), pool.pool_key.digest().to_vec()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        tx_params,
    );
    register.boxes = vec![BoxReference {
        app_index: 0,
        name: pool.pool_key.digest().to_vec(),
    }];
    let mut txs = vec![pool.tx, funding, register];
    assign_group_id(&mut txs);
    Ok(RegisteredPoolCreateDraft {
        pool_key: pool.pool_key,
        txs,
    })
}

pub fn build_pool_setup(
    request: PoolSetupRequest,
    tx_params: &TransactionParams,
) -> Result<PoolGroupDraft> {
    validate_base_fee(tx_params)?;
    validate_pool_addresses(request.app_id, request.app_address);
    let funding = build_payment(
        request.creator,
        request.app_address,
        request.funding_microalgo,
        tx_params,
    );
    let inner_count = 1 + asset_opt_in_count(&request.pool_key);
    let mut call = app_call(
        request.creator,
        request.app_id,
        vec![APP_ARG_SETUP.to_vec()],
        Vec::new(),
        pool_foreign_assets(&request.pool_key, &[]),
        tx_params,
    );
    call.fee = pooled_app_fee(tx_params, inner_count)?;
    grouped(vec![funding, call])
}

pub fn build_pool_bootstrap(
    request: BootstrapRequest,
    tx_params: &TransactionParams,
) -> Result<PoolGroupDraft> {
    validate_base_fee(tx_params)?;
    validate_positive(request.amount_0)?;
    validate_positive(request.amount_1)?;
    validate_positive(request.minimum_lp)?;
    let mut txs = vec![
        asset_transfer(
            request.provider,
            request.app_address,
            request.pool_key.asset_0,
            request.amount_0,
            tx_params,
        ),
        asset_transfer(
            request.provider,
            request.app_address,
            request.pool_key.asset_1,
            request.amount_1,
            tx_params,
        ),
    ];
    let mut call = app_call(
        request.provider,
        request.app_id,
        vec![
            APP_ARG_BOOTSTRAP.to_vec(),
            u64_arg(request.minimum_lp),
            u64_arg(request.deadline.as_u64()),
        ],
        Vec::new(),
        pool_foreign_assets(&request.pool_key, &[request.lp_asset_id]),
        tx_params,
    );
    call.fee = pooled_app_fee(tx_params, 1)?;
    txs.push(call);
    grouped(txs)
}

pub fn build_pool_add_liquidity(
    request: AddLiquidityRequest,
    tx_params: &TransactionParams,
) -> Result<PoolGroupDraft> {
    validate_base_fee(tx_params)?;
    validate_positive(request.amount_0)?;
    validate_positive(request.amount_1)?;
    validate_positive(request.minimum_lp)?;
    let mut txs = vec![
        asset_transfer(
            request.provider,
            request.app_address,
            request.pool_key.asset_0,
            request.amount_0,
            tx_params,
        ),
        asset_transfer(
            request.provider,
            request.app_address,
            request.pool_key.asset_1,
            request.amount_1,
            tx_params,
        ),
    ];
    let mut call = app_call(
        request.provider,
        request.app_id,
        vec![
            APP_ARG_ADD.to_vec(),
            u64_arg(request.minimum_lp),
            u64_arg(request.deadline.as_u64()),
        ],
        Vec::new(),
        pool_foreign_assets(&request.pool_key, &[request.lp_asset_id]),
        tx_params,
    );
    call.fee = pooled_app_fee(tx_params, 1)?;
    txs.push(call);
    grouped(txs)
}

pub fn build_pool_remove_liquidity(
    request: RemoveLiquidityRequest,
    tx_params: &TransactionParams,
) -> Result<PoolGroupDraft> {
    validate_base_fee(tx_params)?;
    validate_positive(request.burn_lp)?;
    let mut txs = vec![asset_transfer(
        request.provider,
        request.app_address,
        request.lp_asset_id,
        request.burn_lp,
        tx_params,
    )];
    let mut call = app_call(
        request.provider,
        request.app_id,
        vec![
            APP_ARG_REMOVE.to_vec(),
            u64_arg(request.minimum_0),
            u64_arg(request.minimum_1),
            u64_arg(request.deadline.as_u64()),
        ],
        Vec::new(),
        pool_foreign_assets(&request.pool_key, &[request.lp_asset_id]),
        tx_params,
    );
    call.fee = pooled_app_fee(tx_params, 2)?;
    txs.push(call);
    grouped(txs)
}

pub fn build_pool_swap(
    request: SwapRequest,
    tx_params: &TransactionParams,
) -> Result<PoolGroupDraft> {
    validate_base_fee(tx_params)?;
    validate_positive(request.amount_in)?;
    validate_positive(request.minimum_out)?;
    let asset_out = request.pool_key.other(request.asset_in)?;
    let mut txs = vec![asset_transfer(
        request.trader,
        request.app_address,
        request.asset_in,
        request.amount_in,
        tx_params,
    )];
    let mut call = app_call(
        request.trader,
        request.app_id,
        vec![
            APP_ARG_SWAP.to_vec(),
            u64_arg(request.asset_in),
            u64_arg(request.minimum_out),
            u64_arg(request.deadline.as_u64()),
        ],
        Vec::new(),
        pool_foreign_assets(&request.pool_key, &[asset_out]),
        tx_params,
    );
    call.fee = pooled_app_fee(tx_params, 1)?;
    txs.push(call);
    grouped(txs)
}

fn asset_transfer(
    sender: Address,
    receiver: Address,
    asset_id: u64,
    amount: u64,
    params: &TransactionParams,
) -> TransactionFields {
    if asset_id == 0 {
        build_payment(sender, receiver, amount, params)
    } else {
        build_asset_transfer(sender, receiver, asset_id, amount, params)
    }
}

fn app_call(
    sender: Address,
    app_id: u64,
    app_args: Vec<Vec<u8>>,
    app_accounts: Vec<Address>,
    foreign_assets: Vec<u64>,
    params: &TransactionParams,
) -> TransactionFields {
    build_application_call(
        sender,
        app_id,
        app_args,
        app_accounts,
        foreign_assets,
        Vec::new(),
        params,
    )
}

fn grouped(mut txs: Vec<TransactionFields>) -> Result<PoolGroupDraft> {
    assign_group_id(&mut txs);
    Ok(PoolGroupDraft { txs })
}

fn pooled_app_fee(params: &TransactionParams, inner_count: u64) -> Result<u64> {
    let fee = params
        .fee
        .checked_mul(inner_count.saturating_add(1))
        .ok_or(crate::AmmError::Overflow)?;
    if fee > MAX_AMM_TXN_FEE {
        return Err(crate::AmmError::Overflow);
    }
    Ok(fee)
}

fn pool_foreign_assets(pool_key: &PoolKey, extra_assets: &[u64]) -> Vec<u64> {
    let mut assets = Vec::new();
    for asset_id in [pool_key.asset_0, pool_key.asset_1] {
        push_asset(&mut assets, asset_id);
    }
    for asset_id in extra_assets {
        push_asset(&mut assets, *asset_id);
    }
    assets
}

fn push_asset(assets: &mut Vec<u64>, asset_id: u64) {
    if asset_id != 0 && !assets.contains(&asset_id) {
        assets.push(asset_id);
    }
}

fn asset_opt_in_count(pool_key: &PoolKey) -> u64 {
    [pool_key.asset_0, pool_key.asset_1]
        .into_iter()
        .filter(|asset_id| *asset_id != 0)
        .count() as u64
}

fn validate_positive(amount: u64) -> Result<()> {
    if amount == 0 {
        return Err(crate::AmmError::ZeroAmount);
    }
    Ok(())
}

fn validate_base_fee(params: &TransactionParams) -> Result<()> {
    if params.fee > MAX_AMM_TXN_FEE {
        return Err(crate::AmmError::Overflow);
    }
    Ok(())
}

fn validate_pool_addresses(app_id: u64, app_address: Address) {
    debug_assert_eq!(Address::from_app_id(app_id), app_address);
}

fn program_hash(program: &[u8]) -> [u8; 32] {
    let digest = Sha512_256::digest(program);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn u64_arg(value: u64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

pub fn lp_asset_params(app_address: Address) -> opennodia_swap::AssetCreateParams {
    opennodia_swap::AssetCreateParams {
        total: MAX_LP_SUPPLY,
        decimals: 6,
        default_frozen: false,
        unit_name: "NODIALP".into(),
        asset_name: "OpenNodia LP Share".into(),
        url: "https://opennodia.org/lp".into(),
        metadata_hash: None,
        manager: None,
        reserve: Some(app_address),
        freeze: None,
        clawback: None,
    }
}

#[cfg(test)]
mod tests {
    use opennodia_core::Round;
    use opennodia_swap::TransactionType;

    use super::*;

    fn params() -> TransactionParams {
        let mut params = TransactionParams::new(Round(1000), "testnet-v1.0".into(), [1; 32]);
        params.fee = 1_000;
        params
    }

    fn address(byte: u8) -> Address {
        Address::from_bytes([byte; 32])
    }

    fn key() -> PoolKey {
        PoolKey::new(
            [1; 32],
            0,
            42,
            FeeTier::STANDARD_030,
            CURRENT_CONTRACT_VERSION,
        )
        .unwrap()
    }

    #[test]
    fn create_draft_uses_canonical_args_and_schema() {
        let draft = build_pool_create(
            PoolCreateRequest {
                creator: address(7),
                genesis_hash: [1; 32],
                asset_a: 42,
                asset_b: 0,
                fee: FeeTier::STANDARD_030,
                approval_program: vec![1, 32, 1, 1, 34],
                clear_state_program: vec![1, 32, 1, 1, 34],
            },
            &params(),
        )
        .unwrap();

        assert_eq!(draft.pool_key.asset_0, 0);
        assert_eq!(draft.pool_key.asset_1, 42);
        assert_eq!(draft.tx.ty, TransactionType::Appl);
        assert_eq!(draft.tx.global_state_schema, Some(StateSchema::new(9, 1)));
        assert_eq!(draft.tx.local_state_schema, Some(StateSchema::new(0, 0)));
        assert_eq!(draft.tx.app_args[0], APP_ARG_CREATE);
        assert_eq!(draft.tx.app_args[4], draft.pool_key.digest());
    }

    #[test]
    fn registry_create_stores_pool_program_hashes() {
        let draft = build_registry_create(
            RegistryCreateRequest {
                creator: address(7),
                genesis_hash: [9; 32],
                registry_approval_program: vec![1, 2, 3],
                registry_clear_state_program: vec![4, 5, 6],
                pool_approval_program: vec![7, 8, 9],
                pool_clear_state_program: vec![10, 11, 12],
            },
            &params(),
        )
        .unwrap();

        assert_eq!(draft.tx.ty, TransactionType::Appl);
        assert_eq!(
            draft.tx.global_state_schema,
            Some(StateSchema::new(
                REGISTRY_GLOBAL_UINTS,
                REGISTRY_GLOBAL_BYTES
            ))
        );
        assert_eq!(draft.tx.app_args[0], APP_ARG_CREATE);
        assert_eq!(draft.tx.app_args[1], [9; 32]);
        assert_eq!(draft.tx.app_args[2], draft.pool_approval_hash);
        assert_eq!(draft.tx.app_args[3], draft.pool_clear_hash);
    }

    #[test]
    fn registered_create_groups_pool_create_funding_and_registration() {
        let registry_app_id = 55;
        let draft = build_registered_pool_create(
            RegisteredPoolCreateRequest {
                pool: PoolCreateRequest {
                    creator: address(7),
                    genesis_hash: [1; 32],
                    asset_a: 42,
                    asset_b: 0,
                    fee: FeeTier::STANDARD_030,
                    approval_program: vec![1, 32, 1, 1, 34],
                    clear_state_program: vec![1, 32, 1, 1, 34],
                },
                registry_app_id,
                registry_app_address: Address::from_app_id(registry_app_id),
                registry_active_count: 0,
            },
            &params(),
        )
        .unwrap();

        assert_eq!(draft.txs.len(), 3);
        assert!(draft.txs.iter().all(|tx| tx.group.is_some()));
        assert_eq!(draft.txs[0].ty, TransactionType::Appl);
        assert_eq!(draft.txs[0].app_id, None);
        assert_eq!(draft.txs[1].ty, TransactionType::Pay);
        assert_eq!(
            draft.txs[1].receiver,
            Some(Address::from_app_id(registry_app_id))
        );
        assert_eq!(
            draft.txs[1].amount,
            Some(registry_registration_funding_microalgo(0))
        );
        assert_eq!(draft.txs[2].app_id, Some(registry_app_id));
        assert_eq!(draft.txs[2].app_args[0], APP_ARG_REGISTER);
        assert_eq!(draft.txs[2].app_args[1], draft.pool_key.digest());
        assert_eq!(draft.txs[2].boxes.len(), 1);
        assert_eq!(draft.txs[2].boxes[0].app_index, 0);
        assert_eq!(draft.txs[2].boxes[0].name, draft.pool_key.digest());
    }

    #[test]
    fn setup_group_funds_app_and_overpays_for_inner_transactions() {
        let app_id = 1234;
        let draft = build_pool_setup(
            PoolSetupRequest {
                creator: address(7),
                app_id,
                app_address: Address::from_app_id(app_id),
                pool_key: key(),
                funding_microalgo: POOL_SETUP_BASE_FUNDING_MICROALGO,
            },
            &params(),
        )
        .unwrap();

        assert_eq!(draft.txs.len(), 2);
        assert!(draft.txs.iter().all(|tx| tx.group.is_some()));
        assert_eq!(draft.txs[0].ty, TransactionType::Pay);
        assert_eq!(draft.txs[1].ty, TransactionType::Appl);
        assert_eq!(draft.txs[1].foreign_assets, vec![42]);
        assert_eq!(draft.txs[1].fee, 3_000);
    }

    #[test]
    fn bootstrap_group_deposits_ordered_assets_then_calls_app() {
        let app_id = 1234;
        let draft = build_pool_bootstrap(
            BootstrapRequest {
                provider: address(7),
                app_id,
                app_address: Address::from_app_id(app_id),
                pool_key: key(),
                lp_asset_id: 99,
                amount_0: 1_000_000,
                amount_1: 2_000_000,
                minimum_lp: 1_000,
                deadline: Round(1200),
            },
            &params(),
        )
        .unwrap();

        assert_eq!(draft.txs.len(), 3);
        assert_eq!(draft.txs[0].ty, TransactionType::Pay);
        assert_eq!(draft.txs[1].ty, TransactionType::Axfer);
        assert_eq!(draft.txs[2].ty, TransactionType::Appl);
        assert_eq!(draft.txs[2].foreign_assets, vec![42, 99]);
        assert_eq!(draft.txs[2].app_args[0], APP_ARG_BOOTSTRAP);
        assert_eq!(draft.txs[2].fee, 2_000);
    }

    #[test]
    fn swap_group_uses_input_asset_and_output_reference() {
        let app_id = 1234;
        let draft = build_pool_swap(
            SwapRequest {
                trader: address(7),
                app_id,
                app_address: Address::from_app_id(app_id),
                pool_key: key(),
                asset_in: 0,
                amount_in: 1_000_000,
                minimum_out: 100,
                deadline: Round(1200),
            },
            &params(),
        )
        .unwrap();

        assert_eq!(draft.txs.len(), 2);
        assert_eq!(draft.txs[0].ty, TransactionType::Pay);
        assert_eq!(draft.txs[1].foreign_assets, vec![42]);
        assert_eq!(draft.txs[1].app_args[0], APP_ARG_SWAP);
    }

    #[test]
    fn remove_group_burns_lp_then_calls_app() {
        let app_id = 1234;
        let draft = build_pool_remove_liquidity(
            RemoveLiquidityRequest {
                provider: address(7),
                app_id,
                app_address: Address::from_app_id(app_id),
                pool_key: key(),
                lp_asset_id: 99,
                burn_lp: 1_000,
                minimum_0: 10,
                minimum_1: 20,
                deadline: Round(1200),
            },
            &params(),
        )
        .unwrap();

        assert_eq!(draft.txs.len(), 2);
        assert_eq!(draft.txs[0].ty, TransactionType::Axfer);
        assert_eq!(draft.txs[0].xfer_asset, Some(99));
        assert_eq!(draft.txs[1].foreign_assets, vec![42, 99]);
        assert_eq!(draft.txs[1].fee, 3_000);
    }
}
