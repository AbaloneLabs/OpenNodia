use base64::Engine;
use opennodia_core::Address;
use opennodia_node::AlgodClient;

use super::folks::folks_backed_info_for_pact_pool;
use super::quote_math::ordered_pair;
use super::sources::{ExternalManifest, ExternalSource};
use super::{
    api_error, bad_request, decode_u64_list, internal, optional_state_text, optional_state_uint,
    service_unavailable, state_bytes, state_uint, teal_state_map, ApiResult, ExternalPoolResponse,
    ExternalPoolState, ExternalQuoteMath,
};

#[derive(Debug, Clone)]
struct PactFactoryState {
    pool_version: u64,
    allowed_fee_bps: Vec<u64>,
}

pub(super) async fn discover_pact_constant_product_pools(
    algod: &AlgodClient,
    manifest: ExternalManifest,
    asset_a: u64,
    asset_b: u64,
    round: u64,
) -> ApiResult<Vec<ExternalPoolState>> {
    let Some(factory_app_id) = manifest.pact_constant_product_factory_app_id else {
        return Ok(Vec::new());
    };
    let factory = read_pact_factory_state(algod, factory_app_id).await?;
    let (primary, secondary) = ordered_pair(asset_a, asset_b);
    let mut pools = Vec::new();
    for fee_bps in factory.allowed_fee_bps {
        let box_name = pact_pool_box_name(primary, secondary, fee_bps, factory.pool_version);
        let Some(app_id) = read_pact_factory_pool_id(algod, factory_app_id, &box_name).await?
        else {
            continue;
        };
        match read_pact_constant_product_pool(algod, manifest, app_id, round).await {
            Ok(pool)
                if pool.response.asset_0 == primary
                    && pool.response.asset_1 == secondary
                    && u64::from(pool.response.fee_bps) == fee_bps =>
            {
                pools.push(pool);
            }
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(
                    app_id,
                    error = api_error(&error),
                    "skipping Pact pool that failed verification"
                );
            }
        }
    }
    Ok(pools)
}

async fn read_pact_factory_state(
    algod: &AlgodClient,
    factory_app_id: u64,
) -> ApiResult<PactFactoryState> {
    let app = algod
        .application_info(factory_app_id)
        .await
        .map_err(|error| service_unavailable(format!("fetch Pact factory app: {error}")))?;
    let state = teal_state_map(&app.params.global_state)?;
    let pool_version = state_uint(&state, "POOL_CONTRACT_VERSION")?;
    let allowed_fee_bps = decode_u64_list(&state_bytes(&state, "ALLOWED_FEE_BPS")?)?;
    if allowed_fee_bps.is_empty() {
        return Err(bad_request("Pact factory has no allowed fee tiers"));
    }
    Ok(PactFactoryState {
        pool_version,
        allowed_fee_bps,
    })
}

async fn read_pact_factory_pool_id(
    algod: &AlgodClient,
    factory_app_id: u64,
    box_name: &[u8],
) -> ApiResult<Option<u64>> {
    let Some(app_box) = algod
        .application_box_by_name(factory_app_id, box_name)
        .await
        .map_err(|error| service_unavailable(format!("fetch Pact factory box: {error}")))?
    else {
        return Ok(None);
    };
    let value = base64::engine::general_purpose::STANDARD
        .decode(app_box.value)
        .map_err(|error| internal(format!("decode Pact factory box value: {error}")))?;
    if value.len() != 8 {
        return Err(internal(format!(
            "Pact factory box value must be 8 bytes, got {}",
            value.len()
        )));
    }
    Ok(Some(u64::from_be_bytes(value.try_into().map_err(
        |_| internal("invalid Pact factory app ID bytes"),
    )?)))
}

pub(super) async fn read_pact_constant_product_pool(
    algod: &AlgodClient,
    manifest: ExternalManifest,
    app_id: u64,
    round: u64,
) -> ApiResult<ExternalPoolState> {
    if manifest.pact_constant_product_factory_app_id.is_none() {
        return Err(bad_request("Pact is not configured for this network"));
    }
    let app = algod
        .application_info(app_id)
        .await
        .map_err(|error| service_unavailable(format!("fetch Pact pool app: {error}")))?;
    let state = teal_state_map(&app.params.global_state)?;
    let config = decode_u64_list(&state_bytes(&state, "CONFIG")?)?;
    if config.len() < 3 {
        return Err(bad_request(
            "Pact pool CONFIG must contain asset A, asset B, and fee",
        ));
    }
    let asset_a = config[0];
    let asset_b = config[1];
    let fee_bps =
        u16::try_from(config[2]).map_err(|_| bad_request("Pact fee bps must fit in u16"))?;
    let (asset_0, asset_1) = ordered_pair(asset_a, asset_b);
    if asset_a != asset_0 || asset_b != asset_1 {
        return Err(bad_request("Pact pool assets are not in canonical order"));
    }

    let contract_name = optional_state_text(&state, "CONTRACT_NAME")?;
    if let Some(name) = contract_name.as_deref() {
        if name != "PACT AMM" {
            return Err(bad_request(format!(
                "unsupported Pact pool contract type: {name}"
            )));
        }
    }
    let reserve_0 = state_uint(&state, "A")?;
    let reserve_1 = state_uint(&state, "B")?;
    let total_lp_supply = state_uint(&state, "L")?;
    let lp_asset_id = state_uint(&state, "LTID")?;
    let pact_fee_bps =
        optional_state_uint(&state, "PACT_FEE_BPS").and_then(|value| u16::try_from(value).ok());
    let version = optional_state_uint(&state, "VERSION").unwrap_or(0);
    let expected_pool_version = match manifest.pact_constant_product_factory_app_id {
        Some(factory_app_id) => match read_pact_factory_state(algod, factory_app_id).await {
            Ok(factory) => Some(factory.pool_version),
            Err(error) => {
                tracing::debug!(
                    app_id,
                    error = api_error(&error),
                    "Pact factory version could not be verified for adapter swaps"
                );
                None
            }
        },
        None => None,
    };
    let adapter_swap_supported =
        expected_pool_version.is_some_and(|expected| version != 0 && version == expected);
    let tradable = reserve_0 > 0 && reserve_1 > 0 && total_lp_supply > 0;
    let status_note = if tradable && adapter_swap_supported {
        "verified Pact constant-product pool with non-zero reserves".to_string()
    } else if tradable {
        format!(
            "Pact pool is quote-only because adapter swap support for version {} is not verified",
            if version == 0 {
                "legacy".to_string()
            } else {
                version.to_string()
            }
        )
    } else {
        "Pact pool exists but has no active liquidity".to_string()
    };

    let folks = folks_backed_info_for_pact_pool(algod, manifest, asset_0, asset_1)
        .await
        .unwrap_or_else(|error| {
            tracing::debug!(
                app_id,
                error = api_error(&error),
                "Pact pool is not marked as Folks-backed"
            );
            None
        });
    let folks_backed = folks.is_some();

    Ok(ExternalPoolState {
        response: ExternalPoolResponse {
            pool_id: app_id.to_string(),
            source: ExternalSource::Pact.as_str().into(),
            app_id,
            app_address: Address::from_app_id(app_id).to_string(),
            lp_asset_id,
            asset_0,
            asset_1,
            fee_bps,
            protocol_fee_bps: pact_fee_bps,
            protocol_version: if version == 0 {
                "legacy".into()
            } else {
                version.to_string()
            },
            reserve_0,
            reserve_1,
            total_lp_supply,
            source_round: round,
            quote_supported: tradable,
            swap_supported: false,
            adapter_swap_supported,
            position_supported: false,
            tradable,
            folks_backed,
            folks,
            status: if tradable {
                "quote_only".into()
            } else {
                "inactive".into()
            },
            status_note,
        },
        quote_math: ExternalQuoteMath::PactConstantProductOutputFee,
    })
}

pub(super) fn pact_pool_box_name(
    primary: u64,
    secondary: u64,
    fee_bps: u64,
    version: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    for value in [primary, secondary, fee_bps, version] {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out
}
