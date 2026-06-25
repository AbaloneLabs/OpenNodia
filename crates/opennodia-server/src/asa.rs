//! ASA issuance HTTP handlers.
//!
//! Asset creation uses the same prepare/submit pattern as transfers: prepare
//! builds and validates an unsigned transaction against the current ledger,
//! while submit verifies the PIN, signs through kmd, submits to algod, and
//! returns the confirmed asset ID.

use axum::extract::{Extension, Query, State};
use axum::Json;
use base64::Engine;
use opennodia_assets::AssetPolicyGrade;
use opennodia_core::{Address, MicroAlgo};
use opennodia_node::{AccountInfo, AlgodClient, AssetParams};
use opennodia_swap::{
    build_asset_config, build_asset_create, AssetCreateParams, TransactionFields, TransactionParams,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::time::Duration;

use crate::asa_history::{AsaIssueInsert, AsaIssueRecord};
use crate::intent::IntentStoreError;
use crate::routes::verify_pin;
use crate::session::Session;
use crate::state::AppState;
use crate::tx_flow::{
    bad_request, fetch_account, fetch_params, internal, not_found, require_wallet_address,
    service_unavailable, ApiErrorResponse, ApiResult, WalletTxGroup,
};

const MAX_UNIT_NAME_BYTES: usize = 8;
const MAX_ASSET_NAME_BYTES: usize = 32;
const MAX_URL_BYTES: usize = 96;
const MAX_DECIMALS: u32 = 19;
const CONFIRMATION_TIMEOUT_ROUNDS: u64 = 10;
const MAX_METADATA_BYTES: usize = 256 * 1024;
const METADATA_HTTP_TIMEOUT_SECS: u64 = 5;

/// ASA creation fields shared by prepare and submit requests.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetCreateFields {
    pub creator: String,
    pub total: u64,
    pub decimals: u32,
    #[serde(default)]
    pub unit_name: String,
    #[serde(default)]
    pub asset_name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub metadata_hash_b64: Option<String>,
    #[serde(default)]
    pub default_frozen: bool,
    #[serde(default)]
    pub manager: Option<String>,
    #[serde(default)]
    pub reserve: Option<String>,
    #[serde(default)]
    pub freeze: Option<String>,
    #[serde(default)]
    pub clawback: Option<String>,
    #[serde(default)]
    pub allow_managed_authorities: bool,
}

/// Request to preview an ASA creation transaction.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetCreatePrepareRequest {
    pub wallet_id: String,
    #[serde(flatten)]
    pub fields: AssetCreateFields,
}

/// Request to build, sign, submit, and confirm an ASA creation transaction.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetCreateSubmitRequest {
    pub wallet_id: String,
    pub pin: String,
    pub intent_id: String,
    #[serde(flatten)]
    pub fields: AssetCreateFields,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetConfigPrepareRequest {
    pub wallet_id: String,
    pub signer: String,
    pub asset_id: u64,
    #[serde(default)]
    pub manager: String,
    #[serde(default)]
    pub reserve: String,
    #[serde(default)]
    pub freeze: String,
    #[serde(default)]
    pub clawback: String,
}

#[derive(Debug, Serialize)]
pub struct AssetConfigPrepareResponse {
    pub tx_hash: String,
    pub tx_bytes: String,
    pub preview: AssetConfigPreview,
}

#[derive(Debug, Serialize)]
pub struct AssetConfigPreview {
    pub asset_id: u64,
    pub signer: String,
    pub current_policy: AssetPolicySnapshot,
    pub next_policy: AssetPolicySnapshot,
    pub manager: String,
    pub reserve: String,
    pub freeze: String,
    pub clawback: String,
    pub warnings: Vec<String>,
    pub fee: u64,
}

/// Response from ASA creation prepare: unsigned transaction bytes plus preview.
#[derive(Debug, Serialize)]
pub struct AssetCreatePrepareResponse {
    /// One-time server-side intent identifier.
    pub intent_id: String,
    /// Random server nonce bound to this intent.
    pub nonce: String,
    /// Configured Algorand network name bound to this intent.
    pub network: String,
    /// First round after which this prepared transaction is no longer valid.
    pub expires_at_round: u64,
    /// Stable hash of the exact unsigned transaction bytes prepared for review.
    pub tx_hash: String,
    /// Unsigned transaction bytes, base64-encoded.
    pub tx_bytes: String,
    /// Metadata validation and warnings for the URL/hash pair.
    pub metadata: AssetMetadataValidation,
    /// Policy and DEX/LP eligibility that would apply to this ASA if created.
    pub policy: AssetPolicySnapshot,
    pub preview: AssetCreatePreview,
}

/// Human-readable preview of an ASA creation transaction.
#[derive(Debug, Serialize)]
pub struct AssetCreatePreview {
    pub creator: String,
    pub total: u64,
    pub decimals: u32,
    pub unit_name: String,
    pub asset_name: String,
    pub url: String,
    pub default_frozen: bool,
    pub manager: Option<String>,
    pub reserve: Option<String>,
    pub freeze: Option<String>,
    pub clawback: Option<String>,
    pub fee: u64,
    pub current_balance: u64,
    pub current_min_balance: u64,
    pub required_min_balance: u64,
    pub required_balance: u64,
    pub metadata_hash_b64: Option<String>,
}

/// Response after algod accepts and confirms an ASA creation transaction.
#[derive(Debug, Serialize)]
pub struct AssetCreateSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
    pub asset_id: u64,
    pub creator: String,
    pub total: u64,
    pub decimals: u32,
    pub unit_name: String,
    pub asset_name: String,
    pub policy: AssetPolicySnapshot,
    pub balance_before: u64,
    pub min_balance_before: u64,
    pub balance_after: u64,
    pub min_balance_after: u64,
}

#[derive(Debug, Serialize)]
pub struct AssetIssuesResponse {
    pub network: String,
    pub wallet_id: String,
    pub assets: Vec<AsaIssueRecord>,
}

#[derive(Debug, Deserialize)]
pub struct AssetIssuesQuery {
    pub wallet_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AssetMetadataValidation {
    pub url: String,
    pub has_metadata_hash: bool,
    pub arc3_marker_present: bool,
    pub remote_checked: bool,
    pub hash_verified: bool,
    pub mime_type: Option<String>,
    pub content_length: Option<u64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AssetPolicySnapshot {
    pub grade: String,
    pub dex_eligible: bool,
    pub lp_eligible: bool,
    pub control_capable: bool,
    pub default_frozen: bool,
    pub manager_enabled: bool,
    pub reserve_enabled: bool,
    pub freeze_enabled: bool,
    pub clawback_enabled: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedAssetCreate {
    creator: Address,
    params: AssetCreateParams,
}

/// Stored ASA creation action prepared for one session and one wallet.
#[derive(Debug, Clone)]
pub(crate) struct AssetCreateIntent {
    group: WalletTxGroup,
    validated: ValidatedAssetCreate,
    wallet_id: String,
    network: String,
    nonce: String,
    preview_tx_hash: String,
    expires_at_round: u64,
    metadata: AssetMetadataValidation,
    policy: AssetPolicySnapshot,
}

fn utf8_len(value: &str) -> usize {
    value.len()
}

fn parse_optional_address(value: &Option<String>, field: &str) -> Result<Option<Address>, String> {
    let Some(raw) = value.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let address = raw
        .parse::<Address>()
        .map_err(|e| format!("invalid {field} address: {e}"))?;
    Ok((address != Address::zero()).then_some(address))
}

fn parse_metadata_hash(value: &Option<String>) -> Result<Option<[u8; 32]>, String> {
    let Some(raw) = value.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| format!("metadata_hash_b64 must be valid base64: {e}"))?;
    if decoded.len() != 32 {
        return Err(format!(
            "metadata_hash_b64 must decode to exactly 32 bytes, got {}",
            decoded.len()
        ));
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&decoded);
    Ok(Some(hash))
}

fn metadata_hash_b64(value: Option<[u8; 32]>) -> Option<String> {
    value.map(|hash| base64::engine::general_purpose::STANDARD.encode(hash))
}

fn optional_authority_enabled(value: Option<Address>) -> bool {
    value.is_some_and(|address| !address.is_zero())
}

fn params_authority_enabled(value: &str) -> Result<bool, String> {
    if value.trim().is_empty() {
        return Ok(false);
    }
    let address = value
        .parse::<Address>()
        .map_err(|error| format!("invalid asset authority address: {error}"))?;
    Ok(!address.is_zero())
}

fn policy_snapshot_from_authorities(
    default_frozen: bool,
    manager_enabled: bool,
    reserve_enabled: bool,
    freeze_enabled: bool,
    clawback_enabled: bool,
) -> AssetPolicySnapshot {
    let grade = AssetPolicyGrade::classify(freeze_enabled, clawback_enabled, default_frozen);
    let dex_eligible = grade.is_tradeable_by_default();
    let lp_eligible = grade.is_tradeable_by_default();
    let control_capable =
        default_frozen || manager_enabled || reserve_enabled || freeze_enabled || clawback_enabled;
    let mut warnings = Vec::new();
    if freeze_enabled || clawback_enabled || default_frozen {
        warnings.push(
            "This asset is regulated/control-capable; DEX orders and native LP pool creation are disabled by default."
                .to_string(),
        );
    } else if manager_enabled || reserve_enabled {
        warnings.push(
            "Manager or reserve authority remains enabled; review the issuer trust model before listing."
                .to_string(),
        );
    } else {
        warnings.push(
            "No manager, freeze, or clawback authority remains; creator identity alone is not a verification signal."
                .to_string(),
        );
    }

    AssetPolicySnapshot {
        grade: match grade {
            AssetPolicyGrade::Open => "open",
            AssetPolicyGrade::Bridged => "bridged",
            AssetPolicyGrade::Regulated => "regulated",
        }
        .to_string(),
        dex_eligible,
        lp_eligible,
        control_capable,
        default_frozen,
        manager_enabled,
        reserve_enabled,
        freeze_enabled,
        clawback_enabled,
        warnings,
    }
}

fn policy_snapshot_from_validated(validated: &ValidatedAssetCreate) -> AssetPolicySnapshot {
    let params = &validated.params;
    policy_snapshot_from_authorities(
        params.default_frozen,
        optional_authority_enabled(params.manager),
        optional_authority_enabled(params.reserve),
        optional_authority_enabled(params.freeze),
        optional_authority_enabled(params.clawback),
    )
}

fn policy_snapshot_from_params(params: &AssetParams) -> Result<AssetPolicySnapshot, String> {
    Ok(policy_snapshot_from_authorities(
        params.default_frozen,
        params_authority_enabled(&params.manager)?,
        params_authority_enabled(&params.reserve)?,
        params_authority_enabled(&params.freeze)?,
        params_authority_enabled(&params.clawback)?,
    ))
}

async fn validate_metadata_for_request(
    validated: &ValidatedAssetCreate,
) -> ApiResult<AssetMetadataValidation> {
    let url = validated.params.url.trim().to_string();
    let metadata_hash = validated.params.metadata_hash;
    let has_metadata_hash = metadata_hash.is_some();
    let arc3_marker_present = url.contains("#arc3");
    let mut validation = AssetMetadataValidation {
        url: url.clone(),
        has_metadata_hash,
        arc3_marker_present,
        remote_checked: false,
        hash_verified: false,
        mime_type: None,
        content_length: None,
        warnings: Vec::new(),
    };

    if url.is_empty() {
        return Ok(validation);
    }
    if !has_metadata_hash {
        validation.warnings.push(
            "Metadata URL is set without a metadata hash; the referenced metadata can change after issuance."
                .to_string(),
        );
    }
    if has_metadata_hash && !arc3_marker_present {
        validation.warnings.push(
            "Metadata hash is present but the URL does not include the ARC-3 #arc3 marker."
                .to_string(),
        );
    }

    let parsed = reqwest::Url::parse(&url)
        .map_err(|error| bad_request(format!("url must be a valid URI: {error}")))?;
    match parsed.scheme() {
        "https" => {
            if has_metadata_hash {
                reject_private_metadata_host(&parsed)?;
                verify_https_metadata(&parsed, metadata_hash.unwrap(), &mut validation).await?;
            }
        }
        "ipfs" => {
            validation.warnings.push(
                "IPFS metadata is accepted by URL/hash format only; no self-hosted IPFS provider is configured and OpenNodia does not upload metadata automatically."
                    .to_string(),
            );
        }
        "http" => {
            validation.warnings.push(
                "HTTP metadata URLs are not fetched by the server. Use HTTPS with a metadata hash for remote verification."
                    .to_string(),
            );
        }
        other => {
            return Err(bad_request(format!(
                "unsupported metadata URL scheme: {other}; use https, ipfs, or leave URL empty"
            )));
        }
    }

    Ok(validation)
}

fn reject_private_metadata_host(url: &reqwest::Url) -> ApiResult<()> {
    let Some(host) = url.host_str() else {
        return Err(bad_request("metadata URL must include a host"));
    };
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower.ends_with(".local")
    {
        return Err(bad_request(
            "metadata URL host must be public; localhost and .local names are not fetched",
        ));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(bad_request(
                "metadata URL host must be a public IP address or public DNS name",
            ));
        }
    }
    Ok(())
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.octets()[0] == 0)
        }
        IpAddr::V6(ip) => {
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
    }
}

async fn verify_https_metadata(
    url: &reqwest::Url,
    expected_hash: [u8; 32],
    validation: &mut AssetMetadataValidation,
) -> ApiResult<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(METADATA_HTTP_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| internal(format!("metadata HTTP client: {error}")))?;
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| bad_request(format!("fetch metadata URL failed: {error}")))?;
    if !response.status().is_success() {
        return Err(bad_request(format!(
            "metadata URL returned HTTP status {}",
            response.status()
        )));
    }

    validation.mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string());
    if let Some(length) = response.content_length() {
        validation.content_length = Some(length);
        if length > MAX_METADATA_BYTES as u64 {
            return Err(bad_request(format!(
                "metadata response is too large: {length} bytes"
            )));
        }
    }
    if let Some(mime) = validation.mime_type.as_deref() {
        let is_json = mime == "application/json"
            || mime == "application/arc+json"
            || mime == "text/json"
            || mime.ends_with("+json");
        if !is_json {
            return Err(bad_request(format!(
                "metadata MIME type must be JSON, got {mime}"
            )));
        }
    } else {
        validation
            .warnings
            .push("Metadata response has no Content-Type header.".to_string());
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| bad_request(format!("read metadata body failed: {error}")))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_METADATA_BYTES {
            return Err(bad_request(format!(
                "metadata response exceeds {} bytes",
                MAX_METADATA_BYTES
            )));
        }
        body.extend_from_slice(&chunk);
    }
    validation.content_length = Some(body.len() as u64);
    let json_value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| bad_request(format!("metadata body must be JSON: {error}")))?;
    if !json_value.is_object() {
        return Err(bad_request("metadata body must be a JSON object"));
    }
    let digest: [u8; 32] = Sha256::digest(&body).into();
    if digest != expected_hash {
        return Err(bad_request(
            "metadata hash does not match fetched metadata body",
        ));
    }
    validation.remote_checked = true;
    validation.hash_verified = true;
    Ok(())
}

fn validate_asset_create(req: &AssetCreateFields) -> Result<ValidatedAssetCreate, String> {
    if req.total == 0 {
        return Err("total must be greater than zero".into());
    }
    if req.decimals > MAX_DECIMALS {
        return Err(format!("decimals must be {MAX_DECIMALS} or less"));
    }

    let unit_name = req.unit_name.trim().to_string();
    let asset_name = req.asset_name.trim().to_string();
    let url = req.url.trim().to_string();

    if unit_name.is_empty() {
        return Err("unit_name is required".into());
    }
    if asset_name.is_empty() {
        return Err("asset_name is required".into());
    }
    if utf8_len(&unit_name) > MAX_UNIT_NAME_BYTES {
        return Err(format!(
            "unit_name must not exceed {MAX_UNIT_NAME_BYTES} bytes"
        ));
    }
    if utf8_len(&asset_name) > MAX_ASSET_NAME_BYTES {
        return Err(format!(
            "asset_name must not exceed {MAX_ASSET_NAME_BYTES} bytes"
        ));
    }
    if utf8_len(&url) > MAX_URL_BYTES {
        return Err(format!("url must not exceed {MAX_URL_BYTES} bytes"));
    }

    let creator = req
        .creator
        .parse::<Address>()
        .map_err(|e| format!("invalid creator address: {e}"))?;
    let manager = parse_optional_address(&req.manager, "manager")?;
    let reserve = parse_optional_address(&req.reserve, "reserve")?;
    let freeze = parse_optional_address(&req.freeze, "freeze")?;
    let clawback = parse_optional_address(&req.clawback, "clawback")?;

    if req.default_frozen && freeze.is_none() {
        return Err("default_frozen requires a freeze address".into());
    }
    let has_managed_authority = req.default_frozen
        || manager.is_some()
        || reserve.is_some()
        || freeze.is_some()
        || clawback.is_some();
    if has_managed_authority && !req.allow_managed_authorities {
        return Err(
            "managed ASA authorities require allow_managed_authorities=true; open assets must leave manager/reserve/freeze/clawback empty".into(),
        );
    }

    Ok(ValidatedAssetCreate {
        creator,
        params: AssetCreateParams {
            total: req.total,
            decimals: req.decimals,
            default_frozen: req.default_frozen,
            unit_name,
            asset_name,
            url,
            metadata_hash: parse_metadata_hash(&req.metadata_hash_b64)?,
            manager,
            reserve,
            freeze,
            clawback,
        },
    })
}

fn build_asset_create_transaction(
    validated: &ValidatedAssetCreate,
    params: &TransactionParams,
) -> TransactionFields {
    build_asset_create(validated.creator, validated.params.clone(), params)
}

fn build_asset_create_group(
    validated: &ValidatedAssetCreate,
    params: &TransactionParams,
) -> ApiResult<WalletTxGroup> {
    let tx = build_asset_create_transaction(validated, params);
    WalletTxGroup::single(validated.creator, tx)
}

fn parse_config_authority(value: &str, field: &str) -> Result<Address, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Address::zero());
    }
    trimmed
        .parse::<Address>()
        .map_err(|error| format!("invalid {field} address: {error}"))
}

fn build_asset_config_group(
    signer: Address,
    asset_id: u64,
    manager: Address,
    reserve: Address,
    freeze: Address,
    clawback: Address,
    params: &TransactionParams,
) -> ApiResult<WalletTxGroup> {
    let tx = build_asset_config(
        signer,
        asset_id,
        AssetCreateParams {
            manager: Some(manager),
            reserve: Some(reserve),
            freeze: Some(freeze),
            clawback: Some(clawback),
            ..AssetCreateParams::default()
        },
        params,
    );
    WalletTxGroup::single(signer, tx)
}

fn new_intent_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn required_asset_create_balance(fee: u64, account: &AccountInfo) -> Result<(u64, u64), String> {
    let required_min_balance = account
        .min_balance
        .checked_add(MicroAlgo::PER_ASSET_MIN_BALANCE.as_micro())
        .ok_or_else(|| "minimum balance is too large".to_string())?;
    let required_balance = required_min_balance
        .checked_add(fee)
        .ok_or_else(|| "minimum balance plus fee is too large".to_string())?;
    Ok((required_min_balance, required_balance))
}

fn validate_asset_create_balance(fee: u64, account: &AccountInfo) -> Result<(), String> {
    let (_, required_balance) = required_asset_create_balance(fee, account)?;
    if account.amount < required_balance {
        return Err(format!(
            "insufficient ALGO for ASA creation: balance {}, required {}",
            MicroAlgo(account.amount).fmt_algo(),
            MicroAlgo(required_balance).fmt_algo()
        ));
    }
    Ok(())
}

async fn validate_asset_create_ledger(
    algod: &AlgodClient,
    creator: Address,
    fee: u64,
) -> ApiResult<AccountInfo> {
    let account = fetch_account(algod, creator).await?;
    validate_asset_create_balance(fee, &account).map_err(bad_request)?;
    Ok(account)
}

async fn fetch_created_asset_id(algod: &AlgodClient, txid: &str) -> ApiResult<u64> {
    #[derive(Debug, Deserialize)]
    struct PendingResp {
        #[serde(rename = "asset-index", default)]
        asset_index: u64,
    }

    let url = format!("{}/v2/transactions/pending/{txid}", algod.base_url());
    let resp = reqwest::Client::new()
        .get(&url)
        .header("X-Algo-API-Token", algod.token())
        .send()
        .await
        .map_err(|e| internal(format!("fetch pending transaction: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(internal(format!(
            "fetch pending transaction {status}: {body}"
        )));
    }

    let parsed: PendingResp = resp
        .json()
        .await
        .map_err(|e| internal(format!("decode pending transaction: {e}")))?;
    if parsed.asset_index == 0 {
        return Err(internal(format!(
            "confirmed asset creation {txid} did not include an asset-index"
        )));
    }
    Ok(parsed.asset_index)
}

fn verify_confirmed_asset_params(
    validated: &ValidatedAssetCreate,
    confirmed: &AssetParams,
) -> Result<(), String> {
    let expected = &validated.params;
    if confirmed.creator != validated.creator.to_string() {
        return Err(format!(
            "confirmed creator mismatch: expected {}, got {}",
            validated.creator, confirmed.creator
        ));
    }
    if confirmed.total != expected.total {
        return Err(format!(
            "confirmed total mismatch: expected {}, got {}",
            expected.total, confirmed.total
        ));
    }
    if confirmed.decimals != expected.decimals {
        return Err(format!(
            "confirmed decimals mismatch: expected {}, got {}",
            expected.decimals, confirmed.decimals
        ));
    }
    if confirmed.default_frozen != expected.default_frozen {
        return Err("confirmed default_frozen mismatch".to_string());
    }
    if confirmed.unit_name != expected.unit_name {
        return Err(format!(
            "confirmed unit_name mismatch: expected {}, got {}",
            expected.unit_name, confirmed.unit_name
        ));
    }
    if confirmed.name != expected.asset_name {
        return Err(format!(
            "confirmed asset_name mismatch: expected {}, got {}",
            expected.asset_name, confirmed.name
        ));
    }
    if confirmed.url != expected.url {
        return Err(format!(
            "confirmed url mismatch: expected {}, got {}",
            expected.url, confirmed.url
        ));
    }
    if confirmed_metadata_hash(confirmed)? != expected.metadata_hash {
        return Err("confirmed metadata_hash mismatch".to_string());
    }
    compare_authority("manager", expected.manager, &confirmed.manager)?;
    compare_authority("reserve", expected.reserve, &confirmed.reserve)?;
    compare_authority("freeze", expected.freeze, &confirmed.freeze)?;
    compare_authority("clawback", expected.clawback, &confirmed.clawback)?;
    Ok(())
}

fn confirmed_metadata_hash(confirmed: &AssetParams) -> Result<Option<[u8; 32]>, String> {
    let raw = confirmed.metadata_hash.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|error| format!("confirmed metadata hash is not base64: {error}"))?;
    if decoded.len() != 32 {
        return Err(format!(
            "confirmed metadata hash decoded to {} bytes",
            decoded.len()
        ));
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&decoded);
    Ok(Some(hash))
}

fn compare_authority(
    field: &str,
    expected: Option<Address>,
    confirmed: &str,
) -> Result<(), String> {
    let expected_text = expected
        .filter(|address| !address.is_zero())
        .map(|address| address.to_string())
        .unwrap_or_default();
    if confirmed.trim() != expected_text {
        return Err(format!(
            "confirmed {field} mismatch: expected {expected_text}, got {confirmed}"
        ));
    }
    Ok(())
}

fn non_empty_string(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn preview_from(
    validated: &ValidatedAssetCreate,
    fee: u64,
    account: &AccountInfo,
) -> ApiResult<AssetCreatePreview> {
    let (required_min_balance, required_balance) =
        required_asset_create_balance(fee, account).map_err(bad_request)?;
    let p = &validated.params;
    Ok(AssetCreatePreview {
        creator: validated.creator.to_string(),
        total: p.total,
        decimals: p.decimals,
        unit_name: p.unit_name.clone(),
        asset_name: p.asset_name.clone(),
        url: p.url.clone(),
        default_frozen: p.default_frozen,
        manager: p.manager.map(|address| address.to_string()),
        reserve: p.reserve.map(|address| address.to_string()),
        freeze: p.freeze.map(|address| address.to_string()),
        clawback: p.clawback.map(|address| address.to_string()),
        fee,
        current_balance: account.amount,
        current_min_balance: account.min_balance,
        required_min_balance,
        required_balance,
        metadata_hash_b64: metadata_hash_b64(p.metadata_hash),
    })
}

async fn store_asset_create_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent: AssetCreateIntent,
) -> ApiResult<String> {
    if !state.stores.wallets.contains_wallet(wallet_id).await {
        return Err(not_found(format!("wallet not found: {wallet_id}")));
    }

    let ttl = Duration::from_secs(state.config.dex.intent_ttl_secs.max(30));
    state
        .intents
        .asset_create
        .store(&session.sid, wallet_id, ttl, intent)
        .await
        .map_err(asset_intent_error)
}

async fn take_asset_create_intent(
    state: &AppState,
    session: &Session,
    wallet_id: &str,
    intent_id: &str,
) -> ApiResult<AssetCreateIntent> {
    state
        .intents
        .asset_create
        .take(&session.sid, wallet_id, intent_id)
        .await
        .map_err(asset_intent_error)
}

fn asset_intent_error(error: IntentStoreError) -> ApiErrorResponse {
    crate::api_error::intent_store_error(error, "ASA creation")
}

/// `POST /api/assets/create/prepare` — build an unsigned ASA creation tx.
pub async fn prepare_asset_create_handler(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<AssetCreatePrepareRequest>,
) -> ApiResult<Json<AssetCreatePrepareResponse>> {
    let validated = validate_asset_create(&req.fields).map_err(bad_request)?;
    let metadata = validate_metadata_for_request(&validated).await?;
    let policy = policy_snapshot_from_validated(&validated);
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "ASA create prepare");
    let params = fetch_params(algod).await?;
    let group = build_asset_create_group(&validated, &params)?;
    let account = validate_asset_create_ledger(algod, validated.creator, group.total_fee()).await?;
    let tx_hash = group.tx_hash().to_string();
    let tx_bytes = group.single_tx_b64()?;
    let network = state.config.algod.network.to_string();
    let nonce = new_intent_nonce();
    let expires_at_round = group.last_valid_round();
    let preview = preview_from(&validated, group.total_fee(), &account)?;
    let intent_id = store_asset_create_intent(
        &state,
        &session,
        &req.wallet_id,
        AssetCreateIntent {
            wallet_id: req.wallet_id.clone(),
            network: network.clone(),
            nonce: nonce.clone(),
            preview_tx_hash: tx_hash.clone(),
            expires_at_round,
            metadata: metadata.clone(),
            policy: policy.clone(),
            group,
            validated: validated.clone(),
        },
    )
    .await?;

    Ok(Json(AssetCreatePrepareResponse {
        intent_id,
        nonce,
        network,
        expires_at_round,
        tx_hash,
        tx_bytes,
        metadata,
        policy,
        preview,
    }))
}

/// `POST /api/assets/config/prepare` — preview an ASA authority update tx.
pub async fn prepare_asset_config_handler(
    State(state): State<AppState>,
    Json(req): Json<AssetConfigPrepareRequest>,
) -> ApiResult<Json<AssetConfigPrepareResponse>> {
    if req.asset_id == 0 {
        return Err(bad_request("asset_id must be an ASA id"));
    }
    if !state.stores.wallets.contains_wallet(&req.wallet_id).await {
        return Err(not_found(format!("wallet not found: {}", req.wallet_id)));
    }
    let signer = req
        .signer
        .parse::<Address>()
        .map_err(|error| bad_request(format!("invalid signer address: {error}")))?;
    let signer_text = signer.to_string();
    let wallet_addresses = state
        .stores
        .wallets
        .tracked_wallet_addresses(&req.wallet_id)
        .await
        .map_err(|error| not_found(error.to_string()))?;
    if !wallet_addresses
        .iter()
        .any(|address| address == &signer_text)
    {
        return Err(not_found(format!(
            "address does not belong to wallet: {signer_text}"
        )));
    }

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, asset_id = req.asset_id, "ASA config prepare");
    let current = algod
        .asset_params(req.asset_id)
        .await
        .map_err(|error| bad_request(format!("asset {} not found: {error}", req.asset_id)))?;
    if current.destroyed_at_round != 0 {
        return Err(bad_request(format!("asset {} is destroyed", req.asset_id)));
    }
    if current.manager.trim().is_empty() {
        return Err(bad_request(
            "asset manager authority is locked; no further AssetConfig update is possible",
        ));
    }
    let current_manager = current
        .manager
        .parse::<Address>()
        .map_err(|error| internal(format!("invalid current manager address: {error}")))?;
    if current_manager.is_zero() {
        return Err(bad_request(
            "asset manager authority is locked; no further AssetConfig update is possible",
        ));
    }
    if current_manager != signer {
        return Err(bad_request(
            "signer must be the current asset manager authority",
        ));
    }

    let manager = parse_config_authority(&req.manager, "manager").map_err(bad_request)?;
    let reserve = parse_config_authority(&req.reserve, "reserve").map_err(bad_request)?;
    let freeze = parse_config_authority(&req.freeze, "freeze").map_err(bad_request)?;
    let clawback = parse_config_authority(&req.clawback, "clawback").map_err(bad_request)?;
    let current_policy = policy_snapshot_from_params(&current).map_err(internal)?;
    let next_policy = policy_snapshot_from_authorities(
        current.default_frozen,
        !manager.is_zero(),
        !reserve.is_zero(),
        !freeze.is_zero(),
        !clawback.is_zero(),
    );
    let params = fetch_params(algod).await?;
    let group = build_asset_config_group(
        signer,
        req.asset_id,
        manager,
        reserve,
        freeze,
        clawback,
        &params,
    )?;
    let signer_account = fetch_account(algod, signer).await?;
    if signer_account.amount < signer_account.min_balance.saturating_add(group.total_fee()) {
        return Err(bad_request(format!(
            "insufficient ALGO for AssetConfig fee: balance {}, required {}",
            MicroAlgo(signer_account.amount).fmt_algo(),
            MicroAlgo(signer_account.min_balance.saturating_add(group.total_fee())).fmt_algo()
        )));
    }

    let mut warnings = Vec::new();
    if manager.is_zero() {
        warnings.push(
            "Manager is set to zero; this ASA cannot be reconfigured again after confirmation."
                .to_string(),
        );
    }
    if !next_policy.dex_eligible {
        warnings.push(
            "Freeze, clawback, or default-frozen control remains; DEX/LP eligibility stays blocked."
                .to_string(),
        );
    }

    Ok(Json(AssetConfigPrepareResponse {
        tx_hash: group.tx_hash().to_string(),
        tx_bytes: group.single_tx_b64()?,
        preview: AssetConfigPreview {
            asset_id: req.asset_id,
            signer: signer_text,
            current_policy,
            next_policy,
            manager: manager.to_string(),
            reserve: reserve.to_string(),
            freeze: freeze.to_string(),
            clawback: clawback.to_string(),
            warnings,
            fee: group.total_fee(),
        },
    }))
}

/// `GET /api/assets/issued` — local ASA issuance history for a wallet/network.
pub async fn list_issued_assets_handler(
    State(state): State<AppState>,
    Query(query): Query<AssetIssuesQuery>,
) -> ApiResult<Json<AssetIssuesResponse>> {
    if !state.stores.wallets.contains_wallet(&query.wallet_id).await {
        return Err(not_found(format!("wallet not found: {}", query.wallet_id)));
    }
    let network = state.config.algod.network.to_string();
    let assets = match state.stores.asa_issues.as_ref() {
        Some(store) => store
            .list(&network, &query.wallet_id)
            .map_err(|error| internal(format!("list ASA issuance history: {error}")))?,
        None => Vec::new(),
    };
    Ok(Json(AssetIssuesResponse {
        network,
        wallet_id: query.wallet_id,
        assets,
    }))
}

/// `POST /api/assets/create` — build, sign, submit, and confirm an ASA.
pub async fn create_asset_handler(
    Extension(session): Extension<Session>,
    State(state): State<AppState>,
    Json(req): Json<AssetCreateSubmitRequest>,
) -> ApiResult<Json<AssetCreateSubmitResponse>> {
    let pin = verify_pin(&state, &req.pin).await?;
    let intent = take_asset_create_intent(&state, &session, &req.wallet_id, &req.intent_id).await?;
    if intent.wallet_id != req.wallet_id {
        return Err(bad_request("ASA creation intent wallet mismatch"));
    }
    let current_network = state.config.algod.network.to_string();
    if intent.network != current_network {
        return Err(bad_request(format!(
            "ASA creation intent was prepared for network {}, current network is {current_network}",
            intent.network
        )));
    }
    if intent.preview_tx_hash != intent.group.tx_hash() {
        return Err(internal(
            "ASA creation intent transaction hash changed unexpectedly",
        ));
    }
    if intent.nonce.is_empty() {
        return Err(internal("ASA creation intent has no nonce"));
    }
    if intent.expires_at_round == 0 {
        return Err(internal("ASA creation intent has no expiry round"));
    }
    let request_validated = validate_asset_create(&req.fields).map_err(bad_request)?;
    if request_validated != intent.validated {
        return Err(bad_request(
            "ASA creation fields changed after prepare; prepare a new transaction",
        ));
    }
    if policy_snapshot_from_validated(&request_validated) != intent.policy {
        return Err(bad_request(
            "ASA creation policy changed after prepare; prepare a new transaction",
        ));
    }
    let submit_metadata = validate_metadata_for_request(&request_validated).await?;
    if submit_metadata.has_metadata_hash
        && intent.metadata.remote_checked
        && !submit_metadata.hash_verified
    {
        return Err(bad_request(
            "metadata could not be reverified before submit; prepare again",
        ));
    }
    let validated = intent.validated;

    require_wallet_address(&state, &req.wallet_id, &pin, validated.creator).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "ASA create submit");
    let before_account =
        validate_asset_create_ledger(algod, validated.creator, intent.group.total_fee()).await?;
    let confirmed = intent
        .group
        .sign_submit_and_confirm(
            &state,
            algod,
            &req.wallet_id,
            &pin,
            CONFIRMATION_TIMEOUT_ROUNDS,
            "ASA create",
        )
        .await?;
    let txid = confirmed.txid;
    let confirmed_round = confirmed.confirmed_round;
    let asset_id = fetch_created_asset_id(algod, &txid).await?;
    let confirmed_params = algod
        .asset_params(asset_id)
        .await
        .map_err(|error| service_unavailable(format!("fetch confirmed ASA params: {error}")))?;
    verify_confirmed_asset_params(&validated, &confirmed_params).map_err(internal)?;
    let policy = policy_snapshot_from_params(&confirmed_params).map_err(internal)?;
    let after_account = fetch_account(algod, validated.creator).await?;

    let creator = validated.creator.to_string();
    state.caches.account_info.lock().await.remove(&creator);
    state.caches.asset_params.lock().await.remove(&asset_id);
    if let Some(store) = state.stores.asa_issues.as_ref() {
        if let Err(error) = store.record(&AsaIssueInsert {
            network: current_network,
            wallet_id: req.wallet_id.clone(),
            asset_id,
            creator: creator.clone(),
            txid: txid.clone(),
            confirmed_round,
            policy_grade: policy.grade.clone(),
            dex_eligible: policy.dex_eligible,
            lp_eligible: policy.lp_eligible,
            control_capable: policy.control_capable,
            default_frozen: policy.default_frozen,
            manager: non_empty_string(confirmed_params.manager.clone()),
            reserve: non_empty_string(confirmed_params.reserve.clone()),
            freeze: non_empty_string(confirmed_params.freeze.clone()),
            clawback: non_empty_string(confirmed_params.clawback.clone()),
        }) {
            tracing::warn!(asset_id, %error, "record ASA issuance history failed");
        }
    }
    let sync_state = state.clone();
    let sync_address = creator.clone();
    tokio::spawn(async move {
        if let Err(error) = sync_state.sync_wallet_history_address(&sync_address).await {
            tracing::warn!(address = %sync_address, %error, "wallet history sync after ASA creation failed");
        }
    });

    Ok(Json(AssetCreateSubmitResponse {
        txid,
        confirmed_round,
        asset_id,
        creator,
        total: validated.params.total,
        decimals: validated.params.decimals,
        unit_name: validated.params.unit_name,
        asset_name: validated.params.asset_name,
        policy,
        balance_before: before_account.amount,
        min_balance_before: before_account.min_balance,
        balance_after: after_account.amount,
        min_balance_after: after_account.min_balance,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(byte: u8) -> String {
        Address::from_bytes([byte; 32]).to_string()
    }

    fn valid_fields() -> AssetCreateFields {
        AssetCreateFields {
            creator: address(1),
            total: 1_000_000,
            decimals: 6,
            unit_name: "NODIA".into(),
            asset_name: "OpenNodia Test".into(),
            url: "https://opennodia.local/asset.json".into(),
            metadata_hash_b64: None,
            default_frozen: false,
            manager: None,
            reserve: None,
            freeze: None,
            clawback: None,
            allow_managed_authorities: false,
        }
    }

    fn account(amount: u64, min_balance: u64) -> AccountInfo {
        AccountInfo {
            round: 1,
            address: address(1),
            amount,
            amount_without_pending_rewards: amount,
            pending_rewards: 0,
            reward_base: 0,
            rewards: 0,
            min_balance,
            status: "Offline".into(),
            assets: vec![],
            created_assets: vec![],
            apps_local_state: vec![],
        }
    }

    #[test]
    fn validates_minimal_open_asset() {
        let fields = valid_fields();
        let validated = validate_asset_create(&fields).unwrap();

        assert_eq!(validated.creator, Address::from_bytes([1; 32]));
        assert_eq!(validated.params.unit_name, "NODIA");
        assert_eq!(validated.params.asset_name, "OpenNodia Test");
        assert_eq!(validated.params.manager, None);
        assert_eq!(validated.params.freeze, None);
    }

    #[test]
    fn rejects_default_frozen_without_freeze_address() {
        let mut fields = valid_fields();
        fields.default_frozen = true;
        fields.allow_managed_authorities = true;

        let err = validate_asset_create(&fields).unwrap_err();
        assert!(err.contains("default_frozen requires"));
    }

    #[test]
    fn rejects_managed_authorities_without_explicit_opt_in() {
        let mut fields = valid_fields();
        fields.manager = Some(address(1));

        let err = validate_asset_create(&fields).unwrap_err();
        assert!(err.contains("allow_managed_authorities"));
    }

    #[test]
    fn allows_managed_authorities_with_explicit_opt_in() {
        let mut fields = valid_fields();
        fields.manager = Some(address(1));
        fields.allow_managed_authorities = true;

        let validated = validate_asset_create(&fields).unwrap();
        assert_eq!(validated.params.manager, Some(Address::from_bytes([1; 32])));
    }

    #[test]
    fn decodes_metadata_hash() {
        let mut fields = valid_fields();
        fields.metadata_hash_b64 =
            Some(base64::engine::general_purpose::STANDARD.encode([9u8; 32]));

        let validated = validate_asset_create(&fields).unwrap();
        assert_eq!(validated.params.metadata_hash, Some([9u8; 32]));
    }

    #[tokio::test]
    async fn warns_for_url_without_metadata_hash() {
        let fields = valid_fields();
        let validated = validate_asset_create(&fields).unwrap();

        let metadata = validate_metadata_for_request(&validated).await.unwrap();

        assert!(!metadata.has_metadata_hash);
        assert!(metadata
            .warnings
            .iter()
            .any(|warning| warning.contains("without a metadata hash")));
    }

    #[tokio::test]
    async fn rejects_private_https_metadata_host() {
        let mut fields = valid_fields();
        fields.url = "https://127.0.0.1/asset.json#arc3".into();
        fields.metadata_hash_b64 =
            Some(base64::engine::general_purpose::STANDARD.encode([9u8; 32]));
        let validated = validate_asset_create(&fields).unwrap();

        let err = validate_metadata_for_request(&validated).await.unwrap_err();

        assert!(err.1.error.contains("public IP"));
    }

    #[test]
    fn policy_snapshot_matches_dex_lp_eligibility() {
        let fields = valid_fields();
        let validated = validate_asset_create(&fields).unwrap();
        let policy = policy_snapshot_from_validated(&validated);
        assert_eq!(policy.grade, "open");
        assert!(policy.dex_eligible);
        assert!(policy.lp_eligible);

        let mut fields = valid_fields();
        fields.freeze = Some(address(1));
        fields.allow_managed_authorities = true;
        let validated = validate_asset_create(&fields).unwrap();
        let policy = policy_snapshot_from_validated(&validated);
        assert_eq!(policy.grade, "regulated");
        assert!(!policy.dex_eligible);
        assert!(!policy.lp_eligible);
    }

    #[test]
    fn rejects_overlong_names() {
        let mut fields = valid_fields();
        fields.unit_name = "TOO-LONG-UNIT".into();
        let err = validate_asset_create(&fields).unwrap_err();
        assert!(err.contains("unit_name"));

        let mut fields = valid_fields();
        fields.asset_name = "x".repeat(MAX_ASSET_NAME_BYTES + 1);
        let err = validate_asset_create(&fields).unwrap_err();
        assert!(err.contains("asset_name"));
    }

    #[test]
    fn validates_required_balance() {
        let acct = account(202_000, 100_000);
        assert!(validate_asset_create_balance(1_000, &acct).is_ok());

        let acct = account(200_999, 100_000);
        let err = validate_asset_create_balance(1_000, &acct).unwrap_err();
        assert!(err.contains("insufficient ALGO"));
    }
}
