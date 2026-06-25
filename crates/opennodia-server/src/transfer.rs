//! ALGO/ASA transfer and ASA opt-in HTTP handlers.
//!
//! Transactions are built with fresh algod parameters, signed by kmd, submitted
//! to algod, and (for state-changing endpoints) awaited until confirmation.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
use opennodia_core::{Address, MicroAlgo};
use opennodia_node::{AccountInfo, AlgodClient, AssetParams};
use opennodia_swap::{
    build_asset_opt_in, build_asset_transfer, build_payment, encode_transaction, fetch_tx_params,
    submit_signed_tx, wait_for_confirmation, TransactionFields, TransactionParams,
};
use serde::{Deserialize, Serialize};

use crate::routes::{verify_pin, ApiError};
use crate::state::AppState;

const MAX_NOTE_BYTES: usize = 1024;
const CONFIRMATION_TIMEOUT_ROUNDS: u64 = 10;

type ApiErrorResponse = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<T, ApiErrorResponse>;

/// Request to prepare a transfer transaction.
#[derive(Debug, Deserialize)]
pub struct TransferPrepareRequest {
    /// Sender address.
    pub from: String,
    /// Recipient address.
    pub to: String,
    /// Asset ID: 0 = ALGO, otherwise ASA ID.
    pub asset_id: u64,
    /// Amount in raw units (microAlgo for ALGO, raw units for ASA).
    pub amount: u64,
    /// Optional UTF-8 note.
    #[serde(default)]
    pub note: Option<String>,
}

/// Request to build, sign, and submit a transfer.
#[derive(Debug, Deserialize)]
pub struct TransferSendRequest {
    pub wallet_id: String,
    pub pin: String,
    pub from: String,
    pub to: String,
    pub asset_id: u64,
    pub amount: u64,
    #[serde(default)]
    pub note: Option<String>,
}

impl TransferSendRequest {
    fn transfer_request(&self) -> TransferPrepareRequest {
        TransferPrepareRequest {
            from: self.from.clone(),
            to: self.to.clone(),
            asset_id: self.asset_id,
            amount: self.amount,
            note: self.note.clone(),
        }
    }
}

/// Response from transfer prepare: unsigned transaction bytes plus preview.
#[derive(Debug, Serialize)]
pub struct TransferPrepareResponse {
    /// Unsigned transaction bytes, base64-encoded.
    pub tx_bytes: String,
    pub preview: TransferPreview,
}

/// Human-readable preview of a transfer.
#[derive(Debug, Serialize)]
pub struct TransferPreview {
    pub from: String,
    pub to: String,
    pub asset_id: u64,
    pub asset_name: String,
    pub amount: u64,
    pub fee: u64,
    pub note: Option<String>,
}

/// Request to build, sign, and submit an ASA opt-in.
#[derive(Debug, Deserialize)]
pub struct OptInRequest {
    pub wallet_id: String,
    pub pin: String,
    pub address: String,
    pub asset_id: u64,
}

/// Request to preview an ASA opt-in without signing it.
#[derive(Debug, Deserialize)]
pub struct OptInPrepareRequest {
    pub address: String,
    pub asset_id: u64,
}

/// Response after algod accepts and confirms a transaction.
#[derive(Debug, Serialize)]
pub struct TransferSubmitResponse {
    pub txid: String,
    pub confirmed_round: u64,
}

#[derive(Debug)]
struct ValidatedTransfer {
    from: Address,
    to: Address,
    note: Option<Vec<u8>>,
}

fn bad_request(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::BAD_REQUEST, Json(ApiError::new(msg)))
}

fn not_found(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::NOT_FOUND, Json(ApiError::new(msg)))
}

fn service_unavailable(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::SERVICE_UNAVAILABLE, Json(ApiError::new(msg)))
}

fn internal(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(msg)))
}

fn validate_transfer(req: &TransferPrepareRequest) -> Result<ValidatedTransfer, String> {
    if req.amount == 0 {
        return Err("amount must be greater than zero".into());
    }

    let from = req
        .from
        .parse::<Address>()
        .map_err(|e| format!("invalid sender address: {e}"))?;
    let to = req
        .to
        .parse::<Address>()
        .map_err(|e| format!("invalid recipient address: {e}"))?;
    let note = req.note.as_ref().map(|value| value.as_bytes().to_vec());

    if note
        .as_ref()
        .is_some_and(|value| value.len() > MAX_NOTE_BYTES)
    {
        return Err(format!("note must not exceed {MAX_NOTE_BYTES} bytes"));
    }

    Ok(ValidatedTransfer { from, to, note })
}

fn build_transfer_transaction(
    req: &TransferPrepareRequest,
    validated: &ValidatedTransfer,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = if req.asset_id == 0 {
        build_payment(validated.from, validated.to, req.amount, params)
    } else {
        build_asset_transfer(
            validated.from,
            validated.to,
            req.asset_id,
            req.amount,
            params,
        )
    };
    tx.note = validated.note.clone();
    tx
}

fn validate_opt_in(address: &str, asset_id: u64) -> Result<Address, String> {
    if asset_id == 0 {
        return Err("asset_id must be greater than zero for opt-in".into());
    }
    address
        .parse::<Address>()
        .map_err(|e| format!("invalid opt-in address: {e}"))
}

fn build_opt_in_transaction(
    address: Address,
    asset_id: u64,
    params: &TransactionParams,
) -> TransactionFields {
    build_asset_opt_in(address, asset_id, params)
}

async fn fetch_params(algod: &AlgodClient) -> ApiResult<TransactionParams> {
    fetch_tx_params(algod)
        .await
        .map_err(|e| service_unavailable(format!("fetch transaction params: {e}")))
}

async fn fetch_asset(algod: &AlgodClient, asset_id: u64) -> ApiResult<AssetParams> {
    algod
        .asset_params(asset_id)
        .await
        .map_err(|e| bad_request(format!("asset {asset_id} not found: {e}")))
}

async fn asset_name(algod: &AlgodClient, asset_id: u64) -> ApiResult<String> {
    if asset_id == 0 {
        return Ok("ALGO".into());
    }

    let params = fetch_asset(algod, asset_id).await?;
    Ok(if params.name.trim().is_empty() {
        format!("ASA {asset_id}")
    } else {
        params.name
    })
}

async fn fetch_account(algod: &AlgodClient, address: Address) -> ApiResult<AccountInfo> {
    algod
        .account_info(&address.to_string())
        .await
        .map_err(|e| service_unavailable(format!("account lookup failed: {e}")))
}

fn validate_transfer_balance(
    req: &TransferPrepareRequest,
    fee: u64,
    sender: &AccountInfo,
    recipient: Option<&AccountInfo>,
) -> Result<(), String> {
    let available_for_fee = sender.amount.saturating_sub(sender.min_balance);

    if req.asset_id == 0 {
        let required = req
            .amount
            .checked_add(fee)
            .ok_or_else(|| "amount plus fee is too large".to_string())?;
        if available_for_fee < required {
            return Err(format!(
                "insufficient ALGO balance: spendable {}, required {}",
                MicroAlgo(available_for_fee).fmt_algo(),
                MicroAlgo(required).fmt_algo()
            ));
        }
        return Ok(());
    }

    if available_for_fee < fee {
        return Err(format!(
            "insufficient ALGO for fee: spendable {}, required {}",
            MicroAlgo(available_for_fee).fmt_algo(),
            MicroAlgo(fee).fmt_algo()
        ));
    }

    let holding = sender
        .assets
        .iter()
        .find(|holding| holding.asset_id == req.asset_id)
        .ok_or_else(|| format!("sender is not opted in to asset {}", req.asset_id))?;
    if holding.is_frozen {
        return Err(format!(
            "sender holding for asset {} is frozen",
            req.asset_id
        ));
    }
    if holding.amount < req.amount {
        return Err(format!(
            "insufficient asset balance: available {}, required {}",
            holding.amount, req.amount
        ));
    }

    let recipient =
        recipient.ok_or_else(|| format!("recipient is not opted in to asset {}", req.asset_id))?;
    let recipient_holding = recipient
        .assets
        .iter()
        .find(|holding| holding.asset_id == req.asset_id)
        .ok_or_else(|| format!("recipient is not opted in to asset {}", req.asset_id))?;
    if recipient_holding.is_frozen {
        return Err(format!(
            "recipient holding for asset {} is frozen",
            req.asset_id
        ));
    }

    Ok(())
}

async fn validate_transfer_ledger(
    algod: &AlgodClient,
    req: &TransferPrepareRequest,
    validated: &ValidatedTransfer,
    fee: u64,
) -> ApiResult<()> {
    let sender = fetch_account(algod, validated.from).await?;
    let recipient = if req.asset_id == 0 {
        None
    } else {
        Some(
            algod
                .account_info(&validated.to.to_string())
                .await
                .map_err(|_| {
                    bad_request(format!(
                        "recipient is not opted in to asset {}",
                        req.asset_id
                    ))
                })?,
        )
    };

    validate_transfer_balance(req, fee, &sender, recipient.as_ref()).map_err(bad_request)
}

fn validate_opt_in_balance(asset_id: u64, fee: u64, account: &AccountInfo) -> Result<(), String> {
    if account
        .assets
        .iter()
        .any(|holding| holding.asset_id == asset_id)
    {
        return Err(format!("account is already opted in to asset {asset_id}"));
    }

    let required_min_balance = account
        .min_balance
        .checked_add(MicroAlgo::PER_ASSET_MIN_BALANCE.as_micro())
        .ok_or_else(|| "minimum balance is too large".to_string())?;
    let required = required_min_balance
        .checked_add(fee)
        .ok_or_else(|| "minimum balance plus fee is too large".to_string())?;
    if account.amount < required {
        return Err(format!(
            "insufficient ALGO for opt-in: balance {}, required {}",
            MicroAlgo(account.amount).fmt_algo(),
            MicroAlgo(required).fmt_algo()
        ));
    }

    Ok(())
}

async fn validate_opt_in_ledger(
    algod: &AlgodClient,
    address: Address,
    asset_id: u64,
    fee: u64,
) -> ApiResult<String> {
    let params = fetch_asset(algod, asset_id).await?;
    let account = fetch_account(algod, address).await?;
    validate_opt_in_balance(asset_id, fee, &account).map_err(bad_request)?;

    Ok(if params.name.trim().is_empty() {
        format!("ASA {asset_id}")
    } else {
        params.name
    })
}

async fn require_wallet_address(
    state: &AppState,
    wallet_id: &str,
    pin: &str,
    address: Address,
) -> ApiResult<()> {
    if !state.stores.wallets.contains_wallet(wallet_id).await {
        return Err(not_found(format!("wallet not found: {wallet_id}")));
    }

    let normalized_address = address.to_string();
    let belongs_to_wallet = state
        .stores
        .wallets
        .contains_address(wallet_id, pin, &normalized_address)
        .await
        .map_err(|e| service_unavailable(format!("list wallet addresses: {e}")))?;

    if !belongs_to_wallet {
        return Err(not_found(format!(
            "address does not belong to wallet: {normalized_address}"
        )));
    }

    Ok(())
}

async fn sign_submit_and_confirm(
    state: &AppState,
    algod: &AlgodClient,
    wallet_id: &str,
    pin: &str,
    signer: Address,
    tx: &TransactionFields,
) -> ApiResult<TransferSubmitResponse> {
    let unsigned_bytes = encode_transaction(tx);
    let signed_bytes = state
        .stores
        .wallets
        .sign_transaction(wallet_id, pin, &signer.to_string(), &unsigned_bytes)
        .await
        .map_err(|e| internal(format!("sign transaction: {e}")))?;
    let txid = submit_signed_tx(algod, &signed_bytes)
        .await
        .map_err(|e| internal(format!("submit transaction: {e}")))?;
    let confirmed_round = wait_for_confirmation(algod, &txid, CONFIRMATION_TIMEOUT_ROUNDS)
        .await
        .map_err(|e| internal(format!("confirm transaction: {e}")))?;

    Ok(TransferSubmitResponse {
        txid,
        confirmed_round,
    })
}

/// `POST /api/transfer/prepare` — build an unsigned ALGO/ASA transfer.
pub async fn prepare_transfer_handler(
    State(state): State<AppState>,
    Json(req): Json<TransferPrepareRequest>,
) -> ApiResult<Json<TransferPrepareResponse>> {
    let validated = validate_transfer(&req).map_err(bad_request)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "transfer prepare");
    let params = fetch_params(algod).await?;
    let tx = build_transfer_transaction(&req, &validated, &params);
    validate_transfer_ledger(algod, &req, &validated, tx.fee).await?;
    let tx_bytes = encode_transaction(&tx);
    let asset_name = asset_name(algod, req.asset_id).await?;

    Ok(Json(TransferPrepareResponse {
        tx_bytes: base64::engine::general_purpose::STANDARD.encode(tx_bytes),
        preview: TransferPreview {
            from: validated.from.to_string(),
            to: validated.to.to_string(),
            asset_id: req.asset_id,
            asset_name,
            amount: req.amount,
            fee: tx.fee,
            note: req.note,
        },
    }))
}

/// `POST /api/transfer/send` — build, sign, submit, and confirm a transfer.
pub async fn send_transfer_handler(
    State(state): State<AppState>,
    Json(req): Json<TransferSendRequest>,
) -> ApiResult<Json<TransferSubmitResponse>> {
    let pin = verify_pin(&state, &req.pin).await?;
    let transfer_req = req.transfer_request();
    let validated = validate_transfer(&transfer_req).map_err(bad_request)?;

    require_wallet_address(&state, &req.wallet_id, &pin, validated.from).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "transfer send");
    let params = fetch_params(algod).await?;
    let tx = build_transfer_transaction(&transfer_req, &validated, &params);
    validate_transfer_ledger(algod, &transfer_req, &validated, tx.fee).await?;
    let response =
        sign_submit_and_confirm(&state, algod, &req.wallet_id, &pin, validated.from, &tx).await?;
    Ok(Json(response))
}

/// `POST /api/transfer/opt-in/prepare` — preview an unsigned ASA opt-in.
pub async fn prepare_opt_in_handler(
    State(state): State<AppState>,
    Json(req): Json<OptInPrepareRequest>,
) -> ApiResult<Json<TransferPrepareResponse>> {
    let address = validate_opt_in(&req.address, req.asset_id).map_err(bad_request)?;
    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "opt-in prepare");
    let params = fetch_params(algod).await?;
    let tx = build_opt_in_transaction(address, req.asset_id, &params);
    let asset_name = validate_opt_in_ledger(algod, address, req.asset_id, tx.fee).await?;
    let tx_bytes = encode_transaction(&tx);

    Ok(Json(TransferPrepareResponse {
        tx_bytes: base64::engine::general_purpose::STANDARD.encode(tx_bytes),
        preview: TransferPreview {
            from: address.to_string(),
            to: address.to_string(),
            asset_id: req.asset_id,
            asset_name,
            amount: 0,
            fee: tx.fee,
            note: None,
        },
    }))
}

/// `POST /api/transfer/opt-in` — build, sign, submit, and confirm an ASA opt-in.
pub async fn opt_in_handler(
    State(state): State<AppState>,
    Json(req): Json<OptInRequest>,
) -> ApiResult<Json<TransferSubmitResponse>> {
    let pin = verify_pin(&state, &req.pin).await?;
    let address = validate_opt_in(&req.address, req.asset_id).map_err(bad_request)?;

    require_wallet_address(&state, &req.wallet_id, &pin, address).await?;

    let (algod, source) = state
        .effective_write_client()
        .await
        .map_err(service_unavailable)?;
    tracing::debug!(source = ?source, "opt-in submit");
    let params = fetch_params(algod).await?;
    let tx = build_opt_in_transaction(address, req.asset_id, &params);
    validate_opt_in_ledger(algod, address, req.asset_id, tx.fee).await?;
    let response =
        sign_submit_and_confirm(&state, algod, &req.wallet_id, &pin, address, &tx).await?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use opennodia_core::Round;
    use opennodia_node::Holding;
    use opennodia_swap::TransactionType;

    use super::*;

    fn sample_params() -> TransactionParams {
        let mut params = TransactionParams::new(Round(100), "testnet-v1.0".into(), [9; 32]);
        params.fee = 1000;
        params
    }

    fn address(byte: u8) -> String {
        Address::from_bytes([byte; 32]).to_string()
    }

    fn transfer_request(asset_id: u64) -> TransferPrepareRequest {
        TransferPrepareRequest {
            from: address(1),
            to: address(2),
            asset_id,
            amount: 123,
            note: Some("memo".into()),
        }
    }

    fn account(amount: u64, min_balance: u64, assets: Vec<Holding>) -> AccountInfo {
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
            assets,
            created_assets: vec![],
            apps_local_state: vec![],
        }
    }

    fn holding(asset_id: u64, amount: u64, is_frozen: bool) -> Holding {
        Holding {
            asset_id,
            amount,
            is_frozen,
            creator: String::new(),
        }
    }

    #[test]
    fn builds_algo_payment() {
        let req = transfer_request(0);
        let validated = validate_transfer(&req).unwrap();
        let tx = build_transfer_transaction(&req, &validated, &sample_params());

        assert_eq!(tx.ty, TransactionType::Pay);
        assert_eq!(tx.sender, Address::from_bytes([1; 32]));
        assert_eq!(tx.receiver, Some(Address::from_bytes([2; 32])));
        assert_eq!(tx.amount, Some(123));
        assert_eq!(tx.fee, 1000);
        assert_eq!(tx.note, Some(b"memo".to_vec()));
    }

    #[test]
    fn builds_asa_transfer() {
        let req = transfer_request(42);
        let validated = validate_transfer(&req).unwrap();
        let tx = build_transfer_transaction(&req, &validated, &sample_params());

        assert_eq!(tx.ty, TransactionType::Axfer);
        assert_eq!(tx.xfer_asset, Some(42));
        assert_eq!(tx.asset_amount, Some(123));
        assert_eq!(tx.asset_receiver, Some(Address::from_bytes([2; 32])));
    }

    #[test]
    fn builds_asset_opt_in() {
        let signer = validate_opt_in(&address(3), 77).unwrap();
        let tx = build_opt_in_transaction(signer, 77, &sample_params());

        assert_eq!(signer, Address::from_bytes([3; 32]));
        assert_eq!(tx.ty, TransactionType::Axfer);
        assert_eq!(tx.sender, signer);
        assert_eq!(tx.xfer_asset, Some(77));
        assert_eq!(tx.asset_amount, Some(0));
        assert_eq!(tx.asset_receiver, Some(signer));
    }

    #[test]
    fn validates_amount_and_note_size() {
        let mut req = transfer_request(0);
        req.amount = 0;
        assert_eq!(
            validate_transfer(&req).unwrap_err(),
            "amount must be greater than zero"
        );

        req.amount = 1;
        req.note = Some("a".repeat(MAX_NOTE_BYTES));
        assert!(validate_transfer(&req).is_ok());

        req.note = Some("a".repeat(MAX_NOTE_BYTES + 1));
        assert!(validate_transfer(&req).is_err());
    }

    #[test]
    fn rejects_invalid_addresses_and_algo_opt_in() {
        let mut req = transfer_request(0);
        req.from = "invalid".into();
        assert!(validate_transfer(&req).is_err());

        assert!(validate_opt_in(&address(1), 0).is_err());
    }

    #[test]
    fn validates_algo_spendable_balance() {
        let req = transfer_request(0);
        let sender = account(224, 100, vec![]);
        assert!(validate_transfer_balance(&req, 1, &sender, None).is_ok());

        let sender = account(223, 100, vec![]);
        assert!(validate_transfer_balance(&req, 1, &sender, None).is_err());
    }

    #[test]
    fn validates_asa_holdings_and_recipient_opt_in() {
        let req = transfer_request(42);
        let sender = account(1_000, 100, vec![holding(42, 123, false)]);
        let recipient = account(1_000, 200, vec![holding(42, 0, false)]);
        assert!(validate_transfer_balance(&req, 1, &sender, Some(&recipient)).is_ok());

        assert!(validate_transfer_balance(&req, 1, &sender, None).is_err());
        let frozen = account(1_000, 100, vec![holding(42, 123, true)]);
        assert!(validate_transfer_balance(&req, 1, &frozen, Some(&recipient)).is_err());
    }

    #[test]
    fn validates_opt_in_minimum_balance() {
        let info = account(201_000, 100_000, vec![]);
        assert!(validate_opt_in_balance(42, 1_000, &info).is_ok());

        let info = account(200_999, 100_000, vec![]);
        assert!(validate_opt_in_balance(42, 1_000, &info).is_err());

        let info = account(500_000, 200_000, vec![holding(42, 0, false)]);
        assert!(validate_opt_in_balance(42, 1_000, &info).is_err());
    }
}
