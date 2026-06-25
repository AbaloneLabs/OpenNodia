use opennodia_core::Network;

use super::{bad_request, ApiResult, ExternalSourceStatus};

const TINYMAN_V2_MAINNET_VALIDATOR_APP_ID: u64 = 1_002_541_853;
pub(super) const TINYMAN_V2_TESTNET_VALIDATOR_APP_ID: u64 = 148_607_000;

const PACT_MAINNET_FACTORY_CONSTANT_PRODUCT_APP_ID: u64 = 1_072_843_805;
const PACT_MAINNET_FOLKS_LENDING_POOL_ADAPTER_APP_ID: u64 = 1_123_472_996;
const PACT_TESTNET_FACTORY_CONSTANT_PRODUCT_APP_ID: u64 = 166_540_424;
const PACT_TESTNET_FOLKS_LENDING_POOL_ADAPTER_APP_ID: u64 = 228_284_187;
const FOLKS_MAINNET_POOL_MANAGER_APP_ID: u64 = 971_350_278;
const FOLKS_TESTNET_POOL_MANAGER_APP_ID: u64 = 147_157_634;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExternalSource {
    Tinyman,
    Pact,
}

impl ExternalSource {
    pub(super) fn parse(value: &str) -> ApiResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tinyman" | "tinyman-v2" => Ok(Self::Tinyman),
            "pact" | "pact-cp" => Ok(Self::Pact),
            other => Err(bad_request(format!("unsupported external source: {other}"))),
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Tinyman => "tinyman",
            Self::Pact => "pact",
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Tinyman => "Tinyman",
            Self::Pact => "Pact",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ExternalManifest {
    pub(super) tinyman_v2_validator_app_id: Option<u64>,
    pub(super) pact_constant_product_factory_app_id: Option<u64>,
    pub(super) pact_folks_lending_pool_adapter_app_id: Option<u64>,
    pub(super) folks_pool_manager_app_id: Option<u64>,
}

impl ExternalManifest {
    pub(super) fn for_network(network: Network) -> Self {
        match network {
            Network::Mainnet => Self {
                tinyman_v2_validator_app_id: Some(TINYMAN_V2_MAINNET_VALIDATOR_APP_ID),
                pact_constant_product_factory_app_id: Some(
                    PACT_MAINNET_FACTORY_CONSTANT_PRODUCT_APP_ID,
                ),
                pact_folks_lending_pool_adapter_app_id: Some(
                    PACT_MAINNET_FOLKS_LENDING_POOL_ADAPTER_APP_ID,
                ),
                folks_pool_manager_app_id: Some(FOLKS_MAINNET_POOL_MANAGER_APP_ID),
            },
            Network::Testnet => Self {
                tinyman_v2_validator_app_id: Some(TINYMAN_V2_TESTNET_VALIDATOR_APP_ID),
                pact_constant_product_factory_app_id: Some(
                    PACT_TESTNET_FACTORY_CONSTANT_PRODUCT_APP_ID,
                ),
                pact_folks_lending_pool_adapter_app_id: Some(
                    PACT_TESTNET_FOLKS_LENDING_POOL_ADAPTER_APP_ID,
                ),
                folks_pool_manager_app_id: Some(FOLKS_TESTNET_POOL_MANAGER_APP_ID),
            },
            Network::Betanet | Network::Local => Self {
                tinyman_v2_validator_app_id: None,
                pact_constant_product_factory_app_id: None,
                pact_folks_lending_pool_adapter_app_id: None,
                folks_pool_manager_app_id: None,
            },
        }
    }
}

pub(super) fn requested_sources(source: Option<&str>) -> ApiResult<Vec<ExternalSource>> {
    match source {
        Some(value) if !value.trim().is_empty() => Ok(vec![ExternalSource::parse(value)?]),
        _ => Ok(vec![ExternalSource::Tinyman, ExternalSource::Pact]),
    }
}

pub(super) fn source_statuses(
    network: Network,
    swap_enabled: bool,
    liquidity_enabled: bool,
) -> Vec<ExternalSourceStatus> {
    let manifest = ExternalManifest::for_network(network);
    let tinyman_supported = manifest.tinyman_v2_validator_app_id.is_some();
    let pact_supported = manifest.pact_constant_product_factory_app_id.is_some();
    vec![
        ExternalSourceStatus {
            source: ExternalSource::Tinyman.as_str().into(),
            label: ExternalSource::Tinyman.label().into(),
            quote_supported: tinyman_supported,
            swap_supported: tinyman_supported && swap_enabled,
            liquidity_supported: tinyman_supported && liquidity_enabled,
            position_supported: tinyman_supported,
            status: if tinyman_supported {
                if swap_enabled || liquidity_enabled {
                    "writes_enabled".into()
                } else {
                    "quote_only".into()
                }
            } else {
                "unsupported_network".into()
            },
            validator_app_id: manifest.tinyman_v2_validator_app_id,
            factory_app_id: None,
            folks_lending_pool_adapter_app_id: None,
            note: if tinyman_supported {
                if swap_enabled || liquidity_enabled {
                    format!(
                        "Tinyman V2 writes enabled: swaps={}, liquidity={}. OpenNodia validates protocol transaction groups locally before signing.",
                        swap_enabled, liquidity_enabled
                    )
                } else {
                    "Tinyman V2 pools are read from live pool account local state; writes are disabled by external_liquidity.*_enabled=false.".into()
                }
            } else {
                "Tinyman V2 manifest is not configured for this network.".into()
            },
        },
        ExternalSourceStatus {
            source: ExternalSource::Pact.as_str().into(),
            label: ExternalSource::Pact.label().into(),
            quote_supported: pact_supported,
            swap_supported: pact_supported && swap_enabled,
            liquidity_supported: pact_supported && liquidity_enabled,
            position_supported: pact_supported,
            status: if pact_supported {
                if swap_enabled || liquidity_enabled {
                    "writes_enabled".into()
                } else {
                    "quote_only".into()
                }
            } else {
                "unsupported_network".into()
            },
            validator_app_id: None,
            factory_app_id: manifest.pact_constant_product_factory_app_id,
            folks_lending_pool_adapter_app_id: manifest.pact_folks_lending_pool_adapter_app_id,
            note: if pact_supported {
                if swap_enabled || liquidity_enabled {
                    format!(
                        "Pact constant-product writes enabled: swaps={}, liquidity={}. OpenNodia validates protocol transaction groups locally before signing.",
                        swap_enabled, liquidity_enabled
                    )
                } else {
                    "Pact constant-product pools are discovered from the factory box allowlist; writes are disabled by external_liquidity.*_enabled=false.".into()
                }
            } else {
                "Pact manifest is not configured for this network.".into()
            },
        },
    ]
}
