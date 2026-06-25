//! algod REST API client wrapper.
//!
//! Connects to a local Algorand node's algod REST API to query the ledger
//! and submit transactions. See:
//! <https://developer.algorand.org/docs/rest-apis/algod/>

use base64::Engine;
use opennodia_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::asset::{AccountInfo, ApplicationBox, ApplicationInfo};
use crate::status::{NodeStatus, NodeStatusResponse};

/// Indicates which data source provided a query result.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DataSource {
    /// Data came from the local algod node.
    Local,
    /// Data came from the public relay API (fallback).
    Public,
}

/// Client for a local algod REST API endpoint.
#[derive(Debug, Clone)]
pub struct AlgodClient {
    /// Base URL, e.g. `http://localhost:4001`.
    base_url: String,
    /// API token (algod token or KMD admin token).
    token: String,
    /// Inner HTTP client.
    http: reqwest::Client,
}

impl AlgodClient {
    /// Create a new algod client.
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

    /// Base URL of the algod server.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// API token used for authentication.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Perform a GET request to an algod endpoint.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        tracing::debug!(%url, "algod GET");
        let resp = self
            .http
            .get(&url)
            .header("X-Algo-API-Token", &self.token)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("GET {path}: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!("GET {path}: {status} {body}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| Error::Algod(format!("GET {path} decode: {e}")))
    }

    async fn get_optional<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let url = format!("{}{}", self.base_url, path);
        tracing::debug!(%url, "algod GET");
        let resp = self
            .http
            .get(&url)
            .header("X-Algo-API-Token", &self.token)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("GET {path}: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!("GET {path}: {status} {body}")));
        }
        resp.json::<T>()
            .await
            .map(Some)
            .map_err(|e| Error::Algod(format!("GET {path} decode: {e}")))
    }

    /// Perform a binary POST request to an algod endpoint.
    async fn post_bytes<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        tracing::debug!(%url, "algod POST");
        let resp = self
            .http
            .post(&url)
            .header("X-Algo-API-Token", &self.token)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("POST {path}: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!("POST {path}: {status} {body}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| Error::Algod(format!("POST {path} decode: {e}")))
    }

    /// `GET /v2/status` — current node status.
    pub async fn status(&self) -> Result<NodeStatus> {
        let r: NodeStatusResponse = self.get("/v2/status").await?;
        Ok(NodeStatus {
            last_round: opennodia_core::Round(r.last_round),
            last_version: r.last_version,
            time_since_last_round: r.time_since_last_round,
            catchup_time: r.catchup_time,
        })
    }

    /// `GET /v2/blocks/{round}?header-only=true` — block header only.
    ///
    /// Returns a lightweight block header (round, timestamp, txn count,
    /// proposer, fees, proposer payout) without the full payset.
    pub async fn block_header(&self, round: u64) -> Result<BlockHeader> {
        let url = format!("/v2/blocks/{round}?header-only=true");
        let resp: BlockResponse = self.get(&url).await?;
        Ok(resp.block)
    }

    /// `GET /v2/register/sync-round` is not needed; we use the status round.
    ///
    /// `GET /v2/participation` — list participation keys registered on the node.
    pub async fn participation_keys(&self) -> Result<Vec<ParticipationKey>> {
        self.get("/v2/participation").await
    }

    /// `GET /v2/status/wait-for-block-after/{round}` — wait for a round.
    pub async fn wait_for_block(&self, round: u64) -> Result<()> {
        let _: serde_json::Value = self
            .get(&format!("/v2/status/wait-for-block-after/{round}"))
            .await?;
        Ok(())
    }

    /// `GET /versions` — genesis ID and hash.
    pub async fn versions(&self) -> Result<VersionInfo> {
        self.get("/versions").await
    }

    /// `GET /v2/accounts/{address}` — account information including assets.
    pub async fn account_info(&self, address: &str) -> Result<AccountInfo> {
        self.get(&format!("/v2/accounts/{address}")).await
    }

    /// Return `None` when the account is closed and absent from the ledger.
    pub async fn account_info_optional(&self, address: &str) -> Result<Option<AccountInfo>> {
        self.get_optional(&format!("/v2/accounts/{address}")).await
    }

    /// `GET /v2/assets/{asset-id}` — asset parameters.
    pub async fn asset_params(&self, asset_id: u64) -> Result<crate::asset::AssetParams> {
        #[derive(Deserialize)]
        struct AssetResponse {
            params: crate::asset::AssetParams,
        }

        let response: AssetResponse = self.get(&format!("/v2/assets/{asset_id}")).await?;
        Ok(response.params)
    }

    /// `GET /v2/applications/{application-id}` — application parameters and global state.
    pub async fn application_info(&self, app_id: u64) -> Result<ApplicationInfo> {
        self.get(&format!("/v2/applications/{app_id}")).await
    }

    /// `GET /v2/applications/{application-id}/box` — application box by raw name.
    pub async fn application_box_by_name(
        &self,
        app_id: u64,
        name: &[u8],
    ) -> Result<Option<ApplicationBox>> {
        let url = format!("{}/v2/applications/{app_id}/box", self.base_url);
        tracing::debug!(%url, "algod GET");
        let encoded_name = format!(
            "b64:{}",
            base64::engine::general_purpose::STANDARD.encode(name)
        );
        let resp = self
            .http
            .get(&url)
            .query(&[("name", encoded_name)])
            .header("X-Algo-API-Token", &self.token)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("GET /v2/applications/{app_id}/box: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!(
                "GET /v2/applications/{app_id}/box: {status} {body}"
            )));
        }
        resp.json::<ApplicationBox>()
            .await
            .map(Some)
            .map_err(|e| Error::Algod(format!("GET /v2/applications/{app_id}/box decode: {e}")))
    }

    /// `GET /v2/ledger/supply` — total ALGO supply.
    pub async fn ledger_supply(&self) -> Result<LedgerSupply> {
        self.get("/v2/ledger/supply").await
    }

    /// `POST /v2/teal/compile` — compile TEAL through algod.
    pub async fn compile_teal(&self, source: &[u8]) -> Result<CompiledTeal> {
        self.post_bytes("/v2/teal/compile", "application/x-binary", source.to_vec())
            .await
    }

    /// `POST /v2/teal/dryrun` — evaluate signed transaction groups.
    pub async fn dryrun(&self, request: Vec<u8>) -> Result<serde_json::Value> {
        self.post_bytes("/v2/teal/dryrun", "application/msgpack", request)
            .await
    }
}

/// Result returned by algod's TEAL compiler.
#[derive(Debug, Clone, Deserialize)]
pub struct CompiledTeal {
    pub hash: String,
    pub result: String,
}

/// Fallback-aware query helpers.
///
/// These free functions try the local node first, then fall back to the
/// public relay if the local node is unreachable or returns stale data.
impl AlgodClient {
    /// Query account info with optional public fallback.
    ///
    /// If the local node is actively catching up (`catchup_time > 0`), the
    /// local ledger is stale, so we query the public relay directly for
    /// account data. Otherwise, we try the local node first and fall back to
    /// the public relay only on error.
    pub async fn account_info_with_fallback(
        &self,
        public: Option<&AlgodClient>,
        address: &str,
    ) -> Result<(AccountInfo, DataSource)> {
        // Check if the local node is still syncing. If so, the local ledger
        // may not contain recent transactions, so prefer the public relay.
        let local_syncing = match self.status().await {
            Ok(s) => s.catchup_time > 0,
            Err(_) => true, // unreachable local node → treat as syncing
        };

        if local_syncing {
            if let Some(pub_client) = public {
                tracing::debug!("local node is catching up, querying public relay for account");
                let info = pub_client.account_info(address).await?;
                return Ok((info, DataSource::Public));
            }
        }

        // Local node is synced (or no public fallback available): use local.
        match self.account_info(address).await {
            Ok(info) => Ok((info, DataSource::Local)),
            Err(local_err) => {
                if let Some(pub_client) = public {
                    tracing::warn!(
                        error = %local_err,
                        "local account_info failed, trying public fallback"
                    );
                    let info = pub_client
                        .account_info(address)
                        .await
                        .map_err(|e| Error::Algod(format!("local: {local_err}; public: {e}")))?;
                    Ok((info, DataSource::Public))
                } else {
                    Err(local_err)
                }
            }
        }
    }

    /// Query asset parameters with optional public fallback.
    ///
    /// If the local node is actively catching up (`catchup_time > 0`), the
    /// local ledger may not have the asset params, so we query the public
    /// relay directly. Otherwise, we try the local node first and fall back
    /// to the public relay only on error.
    pub async fn asset_params_with_fallback(
        &self,
        public: Option<&AlgodClient>,
        asset_id: u64,
    ) -> Result<(crate::asset::AssetParams, DataSource)> {
        let local_syncing = match self.status().await {
            Ok(s) => s.catchup_time > 0,
            Err(_) => true,
        };

        if local_syncing {
            if let Some(pub_client) = public {
                tracing::debug!(
                    "local node is catching up, querying public relay for asset params"
                );
                let params = pub_client.asset_params(asset_id).await?;
                return Ok((params, DataSource::Public));
            }
        }

        match self.asset_params(asset_id).await {
            Ok(params) => Ok((params, DataSource::Local)),
            Err(local_err) => {
                if let Some(pub_client) = public {
                    tracing::warn!(
                        error = %local_err,
                        asset_id,
                        "local asset_params failed, trying public fallback"
                    );
                    let params = pub_client
                        .asset_params(asset_id)
                        .await
                        .map_err(|e| Error::Algod(format!("local: {local_err}; public: {e}")))?;
                    Ok((params, DataSource::Public))
                } else {
                    Err(local_err)
                }
            }
        }
    }

    /// Query node status with optional public fallback.
    ///
    /// Tries the local node first. If unreachable, falls back to the public
    /// relay for status. Note: the public relay's status reflects the *network*
    /// state, not the local node's catch-up progress.
    pub async fn status_with_fallback(
        &self,
        public: Option<&AlgodClient>,
    ) -> Result<(NodeStatus, DataSource)> {
        match self.status().await {
            Ok(status) => Ok((status, DataSource::Local)),
            Err(local_err) => {
                if let Some(pub_client) = public {
                    tracing::warn!(
                        error = %local_err,
                        "local status failed, trying public fallback"
                    );
                    let status = pub_client
                        .status()
                        .await
                        .map_err(|e| Error::Algod(format!("local: {local_err}; public: {e}")))?;
                    Ok((status, DataSource::Public))
                } else {
                    Err(local_err)
                }
            }
        }
    }
}

/// Response from `GET /v2/versions`.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default, rename = "genesis_id")]
    pub genesis_id: String,
    #[serde(default, rename = "genesis_hash_b64")]
    pub genesis_hash_b64: String,
}

/// Response from `GET /v2/ledger/supply`.
#[derive(Debug, Clone, Deserialize)]
pub struct LedgerSupply {
    #[serde(rename = "current-round")]
    pub current_round: u64,
    #[serde(rename = "total-money")]
    pub total_money: u64,
    #[serde(rename = "online-money")]
    pub online_money: u64,
}

/// Wrapper for the `GET /v2/blocks/{round}` JSON response.
#[derive(Debug, Clone, Deserialize)]
pub struct BlockResponse {
    pub block: BlockHeader,
}

/// Lightweight block header data from `GET /v2/blocks/{round}?header-only=true`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlockHeader {
    /// Block round.
    #[serde(rename = "rnd", default)]
    pub round: u64,
    /// Timestamp in seconds since epoch.
    #[serde(rename = "ts", default)]
    pub timestamp: i64,
    /// Genesis ID.
    #[serde(rename = "gen", default)]
    pub genesis_id: String,
    /// Cumulative transaction counter (next txn number after this block).
    #[serde(rename = "tc", default)]
    pub txn_counter: u64,
    /// Block proposer address (base32). Present when proposer payouts are enabled.
    #[serde(rename = "prp", default)]
    pub proposer: String,
    /// Total fees collected in this block (microAlgos).
    #[serde(rename = "fc", default)]
    pub fees_collected: u64,
    /// Proposer payout amount (microAlgos).
    #[serde(rename = "pp", default)]
    pub proposer_payout: u64,
}

impl BlockHeader {
    /// Number of transactions included in this block, derived from the
    /// difference in the cumulative txn counter vs the previous block.
    /// When `prev_txn_counter` is `None`, returns `txn_counter` as-is.
    pub fn txn_count(&self, prev_txn_counter: Option<u64>) -> u64 {
        match prev_txn_counter {
            Some(prev) => self.txn_counter.saturating_sub(prev),
            None => self.txn_counter,
        }
    }
}

/// Participation key registered on the node (`GET /v2/participation`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParticipationKey {
    /// Participation key ID.
    pub id: String,
    /// Key detail (contains the participating account address).
    pub key: ParticipationKeyDetail,
}

/// Inner detail of a participation key.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParticipationKeyDetail {
    /// The participating account address (base32).
    #[serde(rename = "parent", default)]
    pub parent: String,
    /// First valid round for this key.
    #[serde(rename = "effective-first-valid", default)]
    pub effective_first_valid: u64,
    /// Last valid round for this key.
    #[serde(rename = "effective-last-valid", default)]
    pub effective_last_valid: u64,
}
