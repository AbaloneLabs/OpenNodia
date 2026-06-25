use base64::Engine;
use opennodia_core::Address;
use opennodia_node::AlgodClient;
use sha2::{Digest, Sha512_256};

use super::quote_math::normalize_pair_reserves;
use super::sources::{ExternalManifest, ExternalSource};
use super::{
    bad_request, internal, service_unavailable, state_uint, teal_state_map, ApiResult,
    ExternalPoolResponse, ExternalPoolState, ExternalQuoteMath,
};

const TINYMAN_V2_POOL_LOGICSIG_TEMPLATE_B64: &str =
    "BoAYAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgQBbNQA0ADEYEkQxGYEBEkSBAUM=";

pub(super) async fn read_tinyman_v2_pool_by_pair(
    algod: &AlgodClient,
    manifest: ExternalManifest,
    asset_a: u64,
    asset_b: u64,
    round: u64,
) -> ApiResult<Option<ExternalPoolState>> {
    let Some(validator_app_id) = manifest.tinyman_v2_validator_app_id else {
        return Ok(None);
    };
    let address = tinyman_v2_pool_address(validator_app_id, asset_a, asset_b)?;
    read_tinyman_v2_pool_by_address(algod, manifest, address, round).await
}

pub(super) async fn read_tinyman_v2_pool_by_address(
    algod: &AlgodClient,
    manifest: ExternalManifest,
    address: Address,
    round: u64,
) -> ApiResult<Option<ExternalPoolState>> {
    let Some(validator_app_id) = manifest.tinyman_v2_validator_app_id else {
        return Ok(None);
    };
    let account = match algod.account_info_optional(&address.to_string()).await {
        Ok(Some(account)) => account,
        Ok(None) => return Ok(None),
        Err(error) => {
            return Err(service_unavailable(format!(
                "fetch Tinyman pool account: {error}"
            )));
        }
    };
    let Some(local_state) = account
        .apps_local_state
        .iter()
        .find(|state| state.id == validator_app_id)
    else {
        return Ok(None);
    };
    let state = teal_state_map(&local_state.key_value)?;
    let asset_1_id = state_uint(&state, "asset_1_id")?;
    let asset_2_id = state_uint(&state, "asset_2_id")?;
    let expected_address = tinyman_v2_pool_address(validator_app_id, asset_1_id, asset_2_id)?;
    if expected_address != address {
        return Err(bad_request(
            "Tinyman pool address does not match local state assets",
        ));
    }
    let asset_1_reserves = state_uint(&state, "asset_1_reserves")?;
    let asset_2_reserves = state_uint(&state, "asset_2_reserves")?;
    let issued_pool_tokens = state_uint(&state, "issued_pool_tokens")?;
    let pool_token_asset_id = state_uint(&state, "pool_token_asset_id")?;
    let total_fee_share = u16::try_from(state_uint(&state, "total_fee_share")?)
        .map_err(|_| bad_request("Tinyman total_fee_share must fit in u16"))?;
    let _protocol_fee_ratio = state_uint(&state, "protocol_fee_ratio").unwrap_or(0);

    let (asset_0, asset_1, reserve_0, reserve_1) =
        normalize_pair_reserves(asset_1_id, asset_2_id, asset_1_reserves, asset_2_reserves);
    let tradable = reserve_0 > 0 && reserve_1 > 0 && issued_pool_tokens > 0;
    let status_note = if tradable {
        "verified Tinyman V2 pool with non-zero reserves".to_string()
    } else {
        "Tinyman V2 pool exists but has no active liquidity".to_string()
    };

    Ok(Some(ExternalPoolState {
        response: ExternalPoolResponse {
            pool_id: address.to_string(),
            source: ExternalSource::Tinyman.as_str().into(),
            app_id: validator_app_id,
            app_address: address.to_string(),
            lp_asset_id: pool_token_asset_id,
            asset_0,
            asset_1,
            fee_bps: total_fee_share,
            protocol_fee_bps: None,
            protocol_version: "v2".into(),
            reserve_0,
            reserve_1,
            total_lp_supply: issued_pool_tokens,
            source_round: account.round.max(round),
            quote_supported: tradable,
            swap_supported: false,
            adapter_swap_supported: true,
            position_supported: false,
            tradable,
            folks_backed: false,
            folks: None,
            status: if tradable {
                "quote_only".into()
            } else {
                "inactive".into()
            },
            status_note,
        },
        quote_math: ExternalQuoteMath::TinymanV2InputFee,
    }))
}

pub(super) fn tinyman_v2_pool_address(
    validator_app_id: u64,
    asset_a: u64,
    asset_b: u64,
) -> ApiResult<Address> {
    let mut program = base64::engine::general_purpose::STANDARD
        .decode(TINYMAN_V2_POOL_LOGICSIG_TEMPLATE_B64)
        .map_err(|error| internal(format!("decode Tinyman pool LogicSig template: {error}")))?;
    if program.len() < 27 {
        return Err(internal("Tinyman pool LogicSig template is too short"));
    }
    let asset_1_id = asset_a.max(asset_b);
    let asset_2_id = asset_a.min(asset_b);
    program[3..11].copy_from_slice(&validator_app_id.to_be_bytes());
    program[11..19].copy_from_slice(&asset_1_id.to_be_bytes());
    program[19..27].copy_from_slice(&asset_2_id.to_be_bytes());

    let mut hasher = Sha512_256::new();
    hasher.update(b"Program");
    hasher.update(&program);
    let digest = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest);
    Ok(Address::from_bytes(bytes))
}
