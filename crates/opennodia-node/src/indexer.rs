//! Algorand Indexer REST API client wrapper.
//!
//! Connects to an Algorand Indexer instance to search assets, query
//! transaction history, and discover applications (LP pools). Indexer is a
//! read-only service — it never participates in transaction submission.
//!
//! See: <https://developer.algorand.org/docs/rest-apis/indexer/>

use opennodia_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::asset::{AssetParams, Holding, TealKeyValue};

/// Client for an Algorand Indexer REST API endpoint.
#[derive(Debug, Clone)]
pub struct IndexerClient {
    /// Base URL, e.g. `http://localhost:8980`.
    base_url: String,
    /// API token (often empty for local instances).
    token: String,
    /// Inner HTTP client.
    http: reqwest::Client,
}

impl IndexerClient {
    /// Create a new indexer client.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client build"),
        }
    }

    /// Base URL of the indexer server.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// API token used for authentication.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Perform a GET request to an indexer endpoint.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        tracing::debug!(%url, "indexer GET");
        let resp = self
            .http
            .get(&url)
            .header("X-Indexer-API-Token", &self.token)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("indexer GET {path}: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!("indexer GET {path}: {status} {body}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| Error::Algod(format!("indexer GET {path} decode: {e}")))
    }

    /// `GET /health` — indexer health and synchronization status.
    ///
    /// Returns the latest round the indexer has processed and whether it is
    /// still catching up to the algod node.
    pub async fn health(&self) -> Result<IndexerHealth> {
        let raw: serde_json::Value = self.get("/health").await?;
        // Indexer /health returns bare JSON fields (not wrapped in an object key).
        // Some versions return { "round": N, ... }, others return bare fields.
        let round = raw.get("round").and_then(|v| v.as_u64()).unwrap_or(0);
        let current_round = raw
            .get("current-round")
            .and_then(|v| v.as_u64())
            .or_else(|| raw.get("currentRound").and_then(|v| v.as_u64()))
            .unwrap_or(round);
        let catching_up = current_round > round;
        Ok(IndexerHealth {
            round,
            current_round,
            catching_up,
        })
    }

    /// `GET /v2/assets?name={query}` — search assets by name or unit.
    ///
    /// Returns matching assets with their full parameters. The indexer
    /// performs a substring match on both `name` and `unit-name` fields.
    pub async fn search_assets(&self, query: &str) -> Result<AssetSearchResponse> {
        // Use reqwest's query builder for proper URL encoding.
        let url = format!("{}/v2/assets", self.base_url);
        tracing::debug!(%url, query, "indexer asset search");
        let resp = self
            .http
            .get(&url)
            .header("X-Indexer-API-Token", &self.token)
            .query(&[("name", query), ("limit", "20")])
            .send()
            .await
            .map_err(|e| Error::Algod(format!("indexer search assets: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!(
                "indexer search assets: {status} {body}"
            )));
        }
        resp.json::<AssetSearchResponse>()
            .await
            .map_err(|e| Error::Algod(format!("indexer search assets decode: {e}")))
    }

    /// `GET /v2/assets?creator={address}` — assets created by one account.
    pub async fn assets_by_creator(
        &self,
        creator: &str,
        limit: u32,
    ) -> Result<AssetSearchResponse> {
        let url = format!("{}/v2/assets", self.base_url);
        tracing::debug!(%url, creator, limit, "indexer assets by creator");
        let resp = self
            .http
            .get(&url)
            .header("X-Indexer-API-Token", &self.token)
            .query(&[
                ("creator", creator.to_string()),
                ("limit", limit.clamp(1, 100).to_string()),
            ])
            .send()
            .await
            .map_err(|e| Error::Algod(format!("indexer assets by creator: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!(
                "indexer assets by creator: {status} {body}"
            )));
        }
        resp.json::<AssetSearchResponse>()
            .await
            .map_err(|e| Error::Algod(format!("indexer assets by creator decode: {e}")))
    }

    /// `GET /v2/assets/{id}/transactions` — recent transactions for an asset.
    pub async fn asset_transactions(
        &self,
        asset_id: u64,
        limit: u32,
    ) -> Result<Vec<IndexerTransaction>> {
        let resp = self
            .asset_transactions_page(asset_id, limit, None, None, None, None)
            .await?;
        Ok(resp.transactions)
    }

    /// Fetch one page of asset transactions with optional filters.
    pub async fn asset_transactions_page(
        &self,
        asset_id: u64,
        limit: u32,
        min_round: Option<u64>,
        max_round: Option<u64>,
        tx_type: Option<&str>,
        next_token: Option<&str>,
    ) -> Result<TransactionListResponse> {
        let url = format!("{}/v2/assets/{asset_id}/transactions", self.base_url);
        tracing::debug!(
            %url,
            limit,
            ?min_round,
            ?max_round,
            ?tx_type,
            has_next_token = next_token.is_some(),
            "indexer asset transactions page"
        );

        let mut query = vec![("limit", limit.clamp(1, 1_000).to_string())];
        if let Some(round) = min_round {
            query.push(("min-round", round.to_string()));
        }
        if let Some(round) = max_round {
            query.push(("max-round", round.to_string()));
        }
        if let Some(tx_type) = tx_type {
            query.push(("tx-type", tx_type.to_string()));
        }
        if let Some(token) = next_token {
            query.push(("next", token.to_string()));
        }

        let resp = self
            .http
            .get(&url)
            .header("X-Indexer-API-Token", &self.token)
            .query(&query)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("indexer asset transactions: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!(
                "indexer asset transactions: {status} {body}"
            )));
        }
        resp.json::<TransactionListResponse>()
            .await
            .map_err(|e| Error::Algod(format!("indexer asset transactions decode: {e}")))
    }

    /// `GET /v2/accounts/{addr}/transactions` — transaction history for an account.
    pub async fn account_transactions(
        &self,
        address: &str,
        limit: u32,
    ) -> Result<Vec<IndexerTransaction>> {
        let resp = self
            .account_transactions_page(address, limit, None, None, None)
            .await?;
        Ok(resp.transactions)
    }

    /// Fetch one page of account transactions with optional round bounds.
    ///
    /// The local recent-history Indexer uses `min_round`; the public
    /// historical backfill uses a stable `max_round` plus `next_token`.
    pub async fn account_transactions_page(
        &self,
        address: &str,
        limit: u32,
        min_round: Option<u64>,
        max_round: Option<u64>,
        next_token: Option<&str>,
    ) -> Result<TransactionListResponse> {
        let url = format!("{}/v2/accounts/{address}/transactions", self.base_url);
        tracing::debug!(
            %url,
            limit,
            ?min_round,
            ?max_round,
            has_next_token = next_token.is_some(),
            "indexer account transactions page"
        );

        let mut query = vec![("limit", limit.to_string())];
        if let Some(round) = min_round {
            query.push(("min-round", round.to_string()));
        }
        if let Some(round) = max_round {
            query.push(("max-round", round.to_string()));
        }
        if let Some(token) = next_token {
            query.push(("next", token.to_string()));
        }

        let resp = self
            .http
            .get(&url)
            .header("X-Indexer-API-Token", &self.token)
            .query(&query)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("indexer account transactions: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!(
                "indexer account transactions: {status} {body}"
            )));
        }
        resp.json::<TransactionListResponse>()
            .await
            .map_err(|e| Error::Algod(format!("indexer account transactions decode: {e}")))
    }

    /// `GET /v2/transactions/{txid}` — look up a single transaction.
    pub async fn transaction(&self, txid: &str) -> Result<IndexerTransaction> {
        let resp: SingleTransactionResponse = self.get(&format!("/v2/transactions/{txid}")).await?;
        Ok(resp.transaction)
    }

    /// `GET /v2/transactions?group-id={group}` — all transactions in one
    /// confirmed atomic group.
    pub async fn transactions_by_group(&self, group_id: &str) -> Result<Vec<IndexerTransaction>> {
        let url = format!("{}/v2/transactions", self.base_url);
        tracing::debug!(%url, group_id, "indexer group transaction query");
        let resp = self
            .http
            .get(&url)
            .header("X-Indexer-API-Token", &self.token)
            .query(&[("group-id", group_id), ("limit", "32")])
            .send()
            .await
            .map_err(|e| Error::Algod(format!("indexer group transactions: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!(
                "indexer group transactions: {status} {body}"
            )));
        }
        let mut decoded = resp
            .json::<TransactionListResponse>()
            .await
            .map_err(|e| Error::Algod(format!("indexer group transactions decode: {e}")))?;
        decoded
            .transactions
            .sort_by_key(|transaction| transaction.intra_round_offset);
        Ok(decoded.transactions)
    }

    /// `GET /v2/applications?asset-id={id}` — applications involving an asset
    /// (e.g. LP pools, AMMs that trade this asset).
    pub async fn applications_by_asset(&self, asset_id: u64) -> Result<Vec<IndexerApplication>> {
        self.applications_by_asset_limited(asset_id, 20).await
    }

    /// `GET /v2/applications?asset-id={id}&limit={limit}`.
    pub async fn applications_by_asset_limited(
        &self,
        asset_id: u64,
        limit: u32,
    ) -> Result<Vec<IndexerApplication>> {
        let resp: ApplicationListResponse = self
            .get(&format!(
                "/v2/applications?asset-id={asset_id}&limit={}",
                limit.clamp(1, 100)
            ))
            .await?;
        Ok(resp.applications)
    }

    /// `GET /v2/accounts?asset-id={id}` — accounts holding one asset.
    pub async fn accounts_by_asset(
        &self,
        asset_id: u64,
        limit: u32,
    ) -> Result<AccountListResponse> {
        self.get(&format!(
            "/v2/accounts?asset-id={asset_id}&limit={}",
            limit.clamp(1, 100)
        ))
        .await
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Indexer health status from `GET /health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerHealth {
    /// The latest round the indexer has fully processed.
    pub round: u64,
    /// The current network round (from algod). Equal to `round` when caught up.
    pub current_round: u64,
    /// Whether the indexer is still catching up (`current_round > round`).
    pub catching_up: bool,
}

/// Response from `GET /v2/assets?name=...`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSearchResponse {
    #[serde(rename = "current-round")]
    pub current_round: u64,
    #[serde(default)]
    pub assets: Vec<AssetSearchResult>,
    #[serde(default, rename = "next-token")]
    pub next_token: Option<String>,
}

/// A single asset search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSearchResult {
    /// The asset ID.
    #[serde(default)]
    pub index: u64,
    /// When the asset was created (0 if unknown).
    #[serde(default, rename = "created-at-round")]
    pub created_at_round: u64,
    /// When the asset was destroyed (0 if still active).
    #[serde(default, rename = "destroyed-at-round")]
    pub destroyed_at_round: u64,
    /// Asset parameters.
    #[serde(default)]
    pub params: AssetParams,
}

/// Response wrapper for transaction list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionListResponse {
    #[serde(rename = "current-round")]
    pub current_round: u64,
    #[serde(default)]
    pub transactions: Vec<IndexerTransaction>,
    #[serde(default, rename = "next-token")]
    pub next_token: Option<String>,
}

/// Response wrapper for single transaction lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTransactionResponse {
    #[serde(rename = "current-round")]
    pub current_round: u64,
    pub transaction: IndexerTransaction,
}

/// Response wrapper for account list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountListResponse {
    #[serde(rename = "current-round")]
    pub current_round: u64,
    #[serde(default)]
    pub accounts: Vec<IndexerAccount>,
    #[serde(default, rename = "next-token")]
    pub next_token: Option<String>,
}

/// Minimal account shape returned by Indexer list queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerAccount {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub amount: u64,
    #[serde(default, rename = "round")]
    pub round: u64,
    #[serde(default)]
    pub assets: Vec<Holding>,
}

/// An indexer transaction record.
///
/// This is a flexible representation — indexer transaction payloads vary
/// significantly by type, so optional fields use `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerTransaction {
    /// Transaction ID (base32).
    #[serde(default, rename = "id")]
    pub id: String,
    /// The round in which the transaction was confirmed.
    #[serde(default, rename = "confirmed-round")]
    pub round: u64,
    /// Intra-round offset (position within the block).
    #[serde(default, rename = "intra-round-offset")]
    pub intra_round_offset: u64,
    /// Block timestamp (Unix seconds).
    #[serde(default, rename = "round-time")]
    pub round_time: u64,
    /// Transaction type (`pay`, `axfer`, `appl`, etc.).
    #[serde(default, rename = "tx-type")]
    pub tx_type: String,
    /// Sender address.
    #[serde(default, rename = "sender")]
    pub sender: String,
    /// Base64-encoded atomic group ID.
    #[serde(default)]
    pub group: Option<String>,
    /// Fee paid (microAlgos).
    #[serde(default, rename = "fee")]
    pub fee: u64,
    /// First valid round.
    #[serde(default, rename = "first-valid")]
    pub first_valid: u64,
    /// Last valid round.
    #[serde(default, rename = "last-valid")]
    pub last_valid: u64,
    /// The raw payment/transfer details, varying by type.
    #[serde(default, rename = "payment-transaction")]
    pub payment: Option<PaymentDetail>,
    #[serde(default, rename = "asset-transfer-transaction")]
    pub asset_transfer: Option<AssetTransferDetail>,
    /// Note field (base64 encoded, if present).
    #[serde(default, rename = "note")]
    pub note: Option<String>,
    /// Receiver address (extracted for convenience).
    #[serde(default, rename = "receiver")]
    pub receiver: Option<String>,
    /// Amount in microAlgos (for `pay` type).
    #[serde(default, rename = "amount")]
    pub amount: Option<u64>,
    /// Asset ID (for `axfer` type).
    #[serde(default, rename = "asset-id")]
    pub asset_id: Option<u64>,
}

/// Payment transaction details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetail {
    #[serde(default, rename = "receiver")]
    pub receiver: String,
    #[serde(default, rename = "amount")]
    pub amount: u64,
    #[serde(default, rename = "close-amount")]
    pub close_amount: u64,
    #[serde(default, rename = "close-remainder-to")]
    pub close_remainder_to: Option<String>,
}

/// Asset transfer transaction details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetTransferDetail {
    #[serde(default, rename = "receiver")]
    pub receiver: String,
    #[serde(default, rename = "amount")]
    pub amount: u64,
    #[serde(default, rename = "asset-id")]
    pub asset_id: u64,
    #[serde(default, rename = "sender")]
    pub sender: String,
    #[serde(default, rename = "close-to")]
    pub close_to: Option<String>,
    #[serde(default, rename = "close-amount")]
    pub close_amount: u64,
}

/// Response wrapper for application list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationListResponse {
    #[serde(rename = "current-round")]
    pub current_round: u64,
    #[serde(default)]
    pub applications: Vec<IndexerApplication>,
    #[serde(default, rename = "next-token")]
    pub next_token: Option<String>,
}

/// An indexer application record (e.g. LP pool, AMM).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexerApplication {
    /// Application ID.
    #[serde(default)]
    pub id: u64,
    /// When the app was created.
    #[serde(default, rename = "created-at-round")]
    pub created_at_round: u64,
    /// Application parameters.
    #[serde(default)]
    pub params: IndexerAppParams,
}

/// Application parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexerAppParams {
    /// Creator address.
    #[serde(default)]
    pub creator: String,
    /// Approval program (base64).
    #[serde(default, rename = "approval-program")]
    pub approval_program: String,
    /// Clear state program (base64).
    #[serde(default, rename = "clear-state-program")]
    pub clear_state_program: String,
    /// Application global state.
    #[serde(default, rename = "global-state")]
    pub global_state: Vec<TealKeyValue>,
    /// Global state schema.
    #[serde(default, rename = "global-state-schema")]
    pub global_state_schema: Option<AppStateSchema>,
    /// Local state schema.
    #[serde(default, rename = "local-state-schema")]
    pub local_state_schema: Option<AppStateSchema>,
}

/// Application state schema (number of uints and byte slices).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateSchema {
    #[serde(default, rename = "num-uint")]
    pub num_uint: u64,
    #[serde(default, rename = "num-byte-slice")]
    pub num_byte_slice: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_health() {
        let json = r#"{
            "data": null,
            "db-available": true,
            "is-migrating": false,
            "message": "0",
            "round": 42000000,
            "version": "3.19.3"
        }"#;
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let round = v.get("round").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(round, 42000000);
    }

    #[test]
    fn parse_asset_search_response() {
        let json = r#"{
            "current-round": 42000000,
            "next-token": "abc123",
            "assets": [
                {
                    "index": 10458941,
                    "created-at-round": 23000000,
                    "destroyed-at-round": 0,
                    "params": {
                        "total": 1000000000,
                        "decimals": 6,
                        "default-frozen": false,
                        "unit-name": "USDC",
                        "name": "USD Coin",
                        "url": "https://www.centre.io",
                        "manager": "MGR",
                        "reserve": "RSV",
                        "freeze": "FRZ",
                        "clawback": "CLW"
                    }
                }
            ]
        }"#;
        let resp: AssetSearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.current_round, 42000000);
        assert_eq!(resp.assets.len(), 1);
        assert_eq!(resp.assets[0].index, 10458941);
        assert_eq!(resp.assets[0].params.unit_name, "USDC");
        assert_eq!(resp.assets[0].params.name, "USD Coin");
        assert_eq!(resp.assets[0].params.decimals, 6);
        assert_eq!(resp.next_token.as_deref(), Some("abc123"));
    }

    #[test]
    fn parse_transaction_list() {
        let json = r#"{
            "current-round": 42000000,
            "transactions": [
                {
                    "id": "ABC123",
                    "confirmed-round": 41999999,
                    "intra-round-offset": 5,
                    "round-time": 1700000000,
                    "tx-type": "pay",
                    "sender": "SENDERADDR",
                    "fee": 1000,
                    "first-valid": 41999998,
                    "last-valid": 42000098,
                    "payment-transaction": {
                        "receiver": "RECVADDR",
                        "amount": 5000000,
                        "close-amount": 0
                    }
                }
            ]
        }"#;
        let resp: TransactionListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.transactions.len(), 1);
        let tx = &resp.transactions[0];
        assert_eq!(tx.id, "ABC123");
        assert_eq!(tx.tx_type, "pay");
        assert_eq!(tx.sender, "SENDERADDR");
        assert_eq!(tx.fee, 1000);
        assert!(tx.payment.is_some());
        assert_eq!(tx.payment.as_ref().unwrap().amount, 5000000);
    }

    #[test]
    fn parses_group_and_close_fields() {
        let json = r#"{
            "current-round": 42000000,
            "transactions": [{
                "id": "ABC123",
                "confirmed-round": 41999999,
                "intra-round-offset": 5,
                "round-time": 1700000000,
                "tx-type": "axfer",
                "sender": "SENDERADDR",
                "group": "R1JPVVA=",
                "asset-transfer-transaction": {
                    "receiver": "RECVADDR",
                    "amount": 0,
                    "asset-id": 42,
                    "close-to": "CLOSEADDR",
                    "close-amount": 77
                }
            }]
        }"#;
        let response: TransactionListResponse = serde_json::from_str(json).unwrap();
        let transaction = &response.transactions[0];
        assert_eq!(transaction.group.as_deref(), Some("R1JPVVA="));
        let transfer = transaction.asset_transfer.as_ref().unwrap();
        assert_eq!(transfer.close_to.as_deref(), Some("CLOSEADDR"));
        assert_eq!(transfer.close_amount, 77);
    }

    #[test]
    fn parse_application_list() {
        let json = r#"{
            "current-round": 42000000,
            "applications": [
                {
                    "id": 999,
                    "created-at-round": 10000000,
                    "params": {
                        "creator": "CREATORADDR",
                        "approval-program": "AQ==",
                        "clear-state-program": "AQ==",
                        "global-state": [
                            {
                                "key": "ZmVlX2Jwcw==",
                                "value": { "type": 2, "uint": 30 }
                            }
                        ],
                        "global-state-schema": {
                            "num-uint": 5,
                            "num-byte-slice": 3
                        }
                    }
                }
            ]
        }"#;
        let resp: ApplicationListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.applications.len(), 1);
        assert_eq!(resp.applications[0].id, 999);
        assert_eq!(resp.applications[0].params.creator, "CREATORADDR");
        assert_eq!(
            resp.applications[0]
                .params
                .global_state_schema
                .as_ref()
                .unwrap()
                .num_uint,
            5
        );
        assert_eq!(resp.applications[0].params.global_state.len(), 1);
    }

    #[test]
    fn parse_empty_asset_search() {
        let json = r#"{
            "current-round": 42000000,
            "assets": []
        }"#;
        let resp: AssetSearchResponse = serde_json::from_str(json).unwrap();
        assert!(resp.assets.is_empty());
        assert!(resp.next_token.is_none());
    }
}
