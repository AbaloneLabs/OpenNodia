//! Shared transaction prepare/submit helpers.
//!
//! Feature-specific modules still own their business rules and response
//! shapes. This module owns the cross-cutting write-flow mechanics that must
//! remain consistent across ASA issuance, native LP actions, DEX routing, and
//! future router/external-liquidity integrations.

use base64::Engine;
use opennodia_core::Address;
use opennodia_node::{AccountInfo, AlgodClient};
use opennodia_swap::{
    encode_transaction, fetch_tx_params, preview_transaction, submit_signed_tx,
    wait_for_confirmation, TransactionFields, TransactionParams,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::state::AppState;

pub(crate) use crate::api_error::{
    bad_request, internal, not_found, service_unavailable, ApiErrorResponse, ApiResult,
};

#[derive(Debug, Clone, Serialize)]
pub struct TxDescription {
    /// Human-readable summary of what this transaction does.
    pub summary: String,
    /// Transaction type ("pay", "axfer", "appl", "acfg").
    pub ty: String,
    /// Who must sign this transaction.
    pub signer: String,
    /// Base64-encoded unsigned transaction bytes.
    pub tx_bytes: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WalletTxGroup {
    signer: Address,
    txs: Vec<TransactionFields>,
    tx_hash: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfirmedSubmit {
    pub(crate) txid: String,
    pub(crate) confirmed_round: u64,
}

impl WalletTxGroup {
    pub(crate) fn new(signer: Address, txs: Vec<TransactionFields>) -> ApiResult<Self> {
        if txs.is_empty() {
            return Err(bad_request("transaction group must not be empty"));
        }
        let tx_hash = hash_transaction_group(&txs);
        Ok(Self {
            signer,
            txs,
            tx_hash,
        })
    }

    pub(crate) fn single(signer: Address, tx: TransactionFields) -> ApiResult<Self> {
        Self::new(signer, vec![tx])
    }

    pub(crate) fn signer(&self) -> Address {
        self.signer
    }

    pub(crate) fn tx_hash(&self) -> &str {
        &self.tx_hash
    }

    pub(crate) fn txs(&self) -> &[TransactionFields] {
        &self.txs
    }

    pub(crate) fn total_fee(&self) -> u64 {
        total_fee(&self.txs)
    }

    pub(crate) fn last_valid_round(&self) -> u64 {
        self.txs
            .iter()
            .map(|tx| tx.last_valid.as_u64())
            .min()
            .unwrap_or(0)
    }

    pub(crate) fn descriptions(&self) -> Vec<TxDescription> {
        self.txs
            .iter()
            .map(|tx| describe_tx(tx, &self.signer.to_string()))
            .collect()
    }

    pub(crate) fn single_tx_b64(&self) -> ApiResult<String> {
        let [tx] = self.txs.as_slice() else {
            return Err(bad_request("expected a single transaction"));
        };
        Ok(unsigned_tx_b64(tx))
    }

    pub(crate) async fn sign_submit_and_confirm(
        &self,
        state: &AppState,
        algod: &AlgodClient,
        wallet_id: &str,
        pin: &str,
        timeout_rounds: u64,
        context: &str,
    ) -> ApiResult<ConfirmedSubmit> {
        let status = algod.status().await.map_err(|error| {
            service_unavailable(format!("{context}: fetch algod status: {error}"))
        })?;
        self.validate_round_window(status.last_round.as_u64(), context)?;

        let mut signed_group = Vec::new();
        for tx in &self.txs {
            let unsigned = encode_transaction(tx);
            let signed = state
                .stores
                .wallets
                .sign_transaction(wallet_id, pin, &self.signer.to_string(), &unsigned)
                .await
                .map_err(|error| internal(format!("{context}: sign transaction: {error}")))?;
            signed_group.extend_from_slice(&signed);
        }

        let txid = submit_signed_tx(algod, &signed_group)
            .await
            .map_err(|error| internal(format!("{context}: submit transaction group: {error}")))?;
        let confirmed_round = wait_for_confirmation(algod, &txid, timeout_rounds)
            .await
            .map_err(|error| {
                internal(format!(
                    "{context}: submitted transaction group as {txid}, but confirmation failed: {error}. Check this TxID before preparing a replacement."
                ))
            })?;

        Ok(ConfirmedSubmit {
            txid,
            confirmed_round,
        })
    }

    fn validate_round_window(&self, current_round: u64, context: &str) -> ApiResult<()> {
        for tx in &self.txs {
            if current_round > tx.last_valid.as_u64() {
                return Err(bad_request(format!(
                    "{context}: prepared transaction expired at round {}; prepare again",
                    tx.last_valid.as_u64()
                )));
            }
        }
        Ok(())
    }
}

pub(crate) async fn fetch_params(algod: &AlgodClient) -> ApiResult<TransactionParams> {
    fetch_tx_params(algod)
        .await
        .map_err(|error| service_unavailable(format!("fetch transaction params: {error}")))
}

pub(crate) async fn fetch_account(algod: &AlgodClient, address: Address) -> ApiResult<AccountInfo> {
    algod
        .account_info(&address.to_string())
        .await
        .map_err(|error| service_unavailable(format!("account lookup failed: {error}")))
}

pub(crate) async fn require_wallet_address(
    state: &AppState,
    wallet_id: &str,
    pin: &str,
    address: Address,
) -> ApiResult<()> {
    if !state.stores.wallets.contains_wallet(wallet_id).await {
        return Err(not_found(format!("wallet not found: {wallet_id}")));
    }

    let normalized = address.to_string();
    let belongs_to_wallet = state
        .stores
        .wallets
        .contains_address(wallet_id, pin, &normalized)
        .await
        .map_err(|error| service_unavailable(format!("list wallet addresses: {error}")))?;

    if !belongs_to_wallet {
        return Err(not_found(format!(
            "address does not belong to wallet: {normalized}"
        )));
    }

    Ok(())
}

pub(crate) fn describe_tx(tx: &TransactionFields, signer_label: &str) -> TxDescription {
    let preview = preview_transaction(tx);
    TxDescription {
        summary: preview.summary,
        ty: preview.ty,
        signer: signer_label.to_string(),
        tx_bytes: unsigned_tx_b64(tx),
    }
}

pub(crate) fn unsigned_tx_b64(tx: &TransactionFields) -> String {
    base64::engine::general_purpose::STANDARD.encode(encode_transaction(tx))
}

pub(crate) fn total_fee(txs: &[TransactionFields]) -> u64 {
    txs.iter().map(|tx| tx.fee).sum()
}

fn hash_transaction_group(txs: &[TransactionFields]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"opennodia.transaction-group.v1");
    for tx in txs {
        let bytes = encode_transaction(tx);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennodia_core::Round;
    use opennodia_swap::TransactionType;

    fn sample_tx(sender_byte: u8, fee: u64) -> TransactionFields {
        TransactionFields {
            ty: TransactionType::Pay,
            sender: Address::from_bytes([sender_byte; 32]),
            fee,
            first_valid: Round(1),
            last_valid: Round(100),
            genesis_id: Some("testnet-v1.0".into()),
            genesis_hash: Some([9; 32]),
            note: None,
            lease: None,
            receiver: Some(Address::from_bytes([2; 32])),
            amount: Some(1_000),
            close_remainder_to: None,
            rekey_to: None,
            xfer_asset: None,
            asset_amount: None,
            asset_sender: None,
            asset_receiver: None,
            asset_close_to: None,
            asset_config_id: None,
            asset_params: None,
            app_id: None,
            on_completion: None,
            app_args: vec![],
            app_accounts: vec![],
            foreign_assets: vec![],
            foreign_apps: vec![],
            boxes: vec![],
            local_state_schema: None,
            global_state_schema: None,
            approval_program: None,
            clear_state_program: None,
            extra_program_pages: None,
            group: None,
        }
    }

    #[test]
    fn wallet_tx_group_hash_changes_with_transaction_bytes() {
        let signer = Address::from_bytes([1; 32]);
        let first = WalletTxGroup::single(signer, sample_tx(1, 1_000)).unwrap();
        let second = WalletTxGroup::single(signer, sample_tx(1, 2_000)).unwrap();

        assert_ne!(first.tx_hash(), second.tx_hash());
        assert_eq!(first.total_fee(), 1_000);
        assert_eq!(first.last_valid_round(), 100);
    }

    #[test]
    fn empty_wallet_tx_group_is_rejected() {
        let signer = Address::from_bytes([1; 32]);
        assert!(WalletTxGroup::new(signer, vec![]).is_err());
    }

    #[test]
    fn expired_wallet_tx_group_is_rejected_before_signing() {
        let signer = Address::from_bytes([1; 32]);
        let group = WalletTxGroup::single(signer, sample_tx(1, 1_000)).unwrap();

        assert!(group.validate_round_window(100, "test").is_ok());
        assert!(group.validate_round_window(101, "test").is_err());
    }
}
