//! kmd (Key Management Daemon) REST API client.
//!
//! Connects to Algorand's kmd to create wallets, generate/import keys,
//! and manage wallet handle tokens. See:
//! <https://github.com/algorand/go-algorand/tree/develop/daemon/kmd>

use base64::Engine;
use opennodia_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Client for a kmd REST API endpoint.
#[derive(Debug, Clone)]
pub struct KmdClient {
    /// Base URL, e.g. `http://localhost:7833`.
    base_url: String,
    /// kmd API token.
    token: String,
    /// Inner HTTP client.
    http: reqwest::Client,
}

impl KmdClient {
    /// Create a new kmd client.
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

    /// Base URL of the kmd server.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // ---- HTTP helpers ----

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        tracing::debug!(%url, "kmd GET");
        let resp = self
            .http
            .get(&url)
            .header("X-KMD-API-Token", &self.token)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("kmd GET {path}: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!("kmd GET {path}: {status} {body}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| Error::Algod(format!("kmd GET {path} decode: {e}")))
    }

    async fn post<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        tracing::debug!(%url, "kmd POST");
        let resp = self
            .http
            .post(&url)
            .header("X-KMD-API-Token", &self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| Error::Algod(format!("kmd POST {path}: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Algod(format!("kmd POST {path}: {status} {body}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| Error::Algod(format!("kmd POST {path} decode: {e}")))
    }

    // ---- Wallet lifecycle ----

    /// `GET /v1/wallets` — list all wallets.
    pub async fn list_wallets(&self) -> Result<Vec<WalletInfo>> {
        let resp: ListWalletsResponse = self.get("/v1/wallets").await?;
        Ok(resp.wallets)
    }

    /// `POST /v1/wallet` — create a new wallet.
    pub async fn create_wallet(&self, name: &str, password: &str) -> Result<WalletInfo> {
        let req = CreateWalletRequest {
            wallet_driver_name: "sqlite".to_string(),
            wallet_name: name.to_string(),
            wallet_password: password.to_string(),
            master_derivation_key: None,
        };
        let resp: CreateWalletResponse = self.post("/v1/wallet", &req).await?;
        Ok(resp.wallet)
    }

    /// `POST /v1/wallet/init` — unlock a wallet, returning a handle token.
    pub async fn init_wallet_handle(&self, wallet_id: &str, password: &str) -> Result<String> {
        let req = InitWalletRequest {
            wallet_id: wallet_id.to_string(),
            wallet_password: password.to_string(),
        };
        let resp: InitWalletResponse = self.post("/v1/wallet/init", &req).await?;
        Ok(resp.wallet_handle_token)
    }

    /// `POST /v1/wallet/release` — release (invalidate) a handle token.
    pub async fn release_wallet_handle(&self, handle_token: &str) -> Result<()> {
        let req = HandleTokenRequest {
            wallet_handle_token: handle_token.to_string(),
        };
        let _: EmptyResponse = self.post("/v1/wallet/release", &req).await?;
        Ok(())
    }

    /// `POST /v1/wallet/renew` — renew a handle token's expiration.
    pub async fn renew_wallet_handle(&self, handle_token: &str) -> Result<u64> {
        let req = HandleTokenRequest {
            wallet_handle_token: handle_token.to_string(),
        };
        let resp: RenewHandleResponse = self.post("/v1/wallet/renew", &req).await?;
        Ok(resp.expires_seconds)
    }

    // ---- Key management ----

    /// `POST /v1/key` — generate the next key in the wallet's HD sequence.
    pub async fn generate_key(&self, handle_token: &str) -> Result<String> {
        let req = GenerateKeyRequest {
            wallet_handle_token: handle_token.to_string(),
            display_mnemonic: false,
        };
        let resp: GenerateKeyResponse = self.post("/v1/key", &req).await?;
        Ok(resp.address)
    }

    /// `POST /v1/key/list` — list all addresses in a wallet.
    pub async fn list_keys(&self, handle_token: &str) -> Result<Vec<String>> {
        let req = HandleTokenRequest {
            wallet_handle_token: handle_token.to_string(),
        };
        let resp: ListKeysResponse = self.post("/v1/key/list", &req).await?;
        Ok(resp.addresses)
    }

    /// `POST /v1/key/import` — import an externally generated ed25519 private key.
    pub async fn import_key(&self, handle_token: &str, private_key: &[u8; 32]) -> Result<String> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(private_key);
        let req = ImportKeyRequest {
            wallet_handle_token: handle_token.to_string(),
            private_key: encoded,
        };
        let resp: ImportKeyResponse = self.post("/v1/key/import", &req).await?;
        Ok(resp.address)
    }

    /// `POST /v1/transaction/sign` — sign a transaction using a wallet handle.
    ///
    /// `tx_bytes` is the raw msgpack-encoded transaction (unsigned).
    /// Returns the signed msgpack bytes.
    pub async fn sign_transaction(
        &self,
        handle_token: &str,
        wallet_password: &str,
        signer_public_key: &[u8; 32],
        tx_bytes: &[u8],
    ) -> Result<Vec<u8>> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(tx_bytes);
        let req = SignTransactionRequest {
            wallet_handle_token: handle_token.to_string(),
            wallet_password: wallet_password.to_string(),
            public_key: *signer_public_key,
            transaction: encoded,
        };
        let resp: SignTransactionResponse = self.post("/v1/transaction/sign", &req).await?;
        decode_signed_transaction(&resp.signed_transaction)
    }
}

// ---- Response / Request types ----

/// kmd wallet info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub id: String,
    pub name: String,
    pub driver_name: String,
    #[serde(default)]
    pub driver_version: u32,
    #[serde(default)]
    pub mnemonic_ux: bool,
    #[serde(default)]
    pub supported_txs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ListWalletsResponse {
    #[serde(default)]
    wallets: Vec<WalletInfo>,
}

#[derive(Debug, Serialize)]
struct CreateWalletRequest {
    wallet_driver_name: String,
    wallet_name: String,
    wallet_password: String,
    master_derivation_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateWalletResponse {
    wallet: WalletInfo,
}

#[derive(Debug, Serialize)]
struct InitWalletRequest {
    wallet_id: String,
    wallet_password: String,
}

#[derive(Debug, Deserialize)]
struct InitWalletResponse {
    wallet_handle_token: String,
}

#[derive(Debug, Serialize)]
struct HandleTokenRequest {
    wallet_handle_token: String,
}

#[derive(Debug, Deserialize)]
struct RenewHandleResponse {
    expires_seconds: u64,
}

#[derive(Debug, Serialize)]
struct GenerateKeyRequest {
    wallet_handle_token: String,
    display_mnemonic: bool,
}

#[derive(Debug, Deserialize)]
struct GenerateKeyResponse {
    address: String,
}

#[derive(Debug, Deserialize)]
struct ListKeysResponse {
    #[serde(default)]
    addresses: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ImportKeyRequest {
    wallet_handle_token: String,
    private_key: String,
}

#[derive(Debug, Deserialize)]
struct ImportKeyResponse {
    address: String,
}

#[derive(Debug, Deserialize)]
struct EmptyResponse {}

#[derive(Debug, Serialize)]
struct SignTransactionRequest {
    wallet_handle_token: String,
    wallet_password: String,
    public_key: [u8; 32],
    transaction: String,
}

#[derive(Debug, Deserialize)]
struct SignTransactionResponse {
    signed_transaction: String,
}

fn decode_signed_transaction(encoded: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| Error::Algod(format!("kmd signed transaction base64 decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_wallet_request_serializes() {
        let req = CreateWalletRequest {
            wallet_driver_name: "sqlite".into(),
            wallet_name: "test".into(),
            wallet_password: "pass".into(),
            master_derivation_key: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("sqlite"));
        assert!(json.contains("test"));
    }

    #[test]
    fn sign_transaction_request_serializes_kmd_contract() {
        let req = SignTransactionRequest {
            wallet_handle_token: "handle".into(),
            wallet_password: "secret".into(),
            public_key: [7; 32],
            transaction: base64::engine::general_purpose::STANDARD.encode([1, 2, 3]),
        };

        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["wallet_handle_token"], "handle");
        assert_eq!(json["wallet_password"], "secret");
        assert_eq!(json["transaction"], "AQID");
        assert_eq!(json["public_key"].as_array().unwrap().len(), 32);
        assert_eq!(json["public_key"][0], 7);
    }

    #[test]
    fn signed_transaction_response_decodes_base64() {
        let response: SignTransactionResponse =
            serde_json::from_value(serde_json::json!({"signed_transaction": "AQID"})).unwrap();

        assert_eq!(
            decode_signed_transaction(&response.signed_transaction).unwrap(),
            vec![1, 2, 3]
        );
    }
}
