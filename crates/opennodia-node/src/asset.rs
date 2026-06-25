//! Asset and account types returned by the algod client.
//!
//! These mirror the algod REST API response shapes for account information
//! and asset parameters. See:
//! <https://developer.algorand.org/docs/rest-apis/algod/#get-v2accountsaddress>

use serde::{Deserialize, Serialize};

/// `GET /v2/accounts/{address}` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(rename = "round")]
    pub round: u64,
    #[serde(rename = "address")]
    pub address: String,
    #[serde(rename = "amount")]
    pub amount: u64,
    #[serde(rename = "amount-without-pending-rewards")]
    pub amount_without_pending_rewards: u64,
    #[serde(rename = "pending-rewards", default)]
    pub pending_rewards: u64,
    #[serde(rename = "reward-base", default)]
    pub reward_base: u64,
    #[serde(rename = "rewards", default)]
    pub rewards: u64,
    #[serde(rename = "min-balance", default)]
    pub min_balance: u64,
    #[serde(rename = "status", default)]
    pub status: String,
    #[serde(rename = "assets", default)]
    pub assets: Vec<Holding>,
    #[serde(rename = "created-assets", default)]
    pub created_assets: Vec<AssetHolding>,
    #[serde(rename = "apps-local-state", default)]
    pub apps_local_state: Vec<ApplicationLocalState>,
}

/// A single ASA holding within an account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holding {
    #[serde(rename = "asset-id")]
    pub asset_id: u64,
    #[serde(rename = "amount", default)]
    pub amount: u64,
    #[serde(rename = "is-frozen", default)]
    pub is_frozen: bool,
    #[serde(rename = "creator", default)]
    pub creator: String,
}

/// A created asset's parameters + index, from `created-assets`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetHolding {
    #[serde(rename = "index")]
    pub index: u64,
    #[serde(rename = "params")]
    pub params: AssetParams,
}

/// A single application local-state entry held by an account.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplicationLocalState {
    #[serde(rename = "id")]
    pub id: u64,
    #[serde(rename = "key-value", default)]
    pub key_value: Vec<TealKeyValue>,
}

/// `GET /v2/assets/{asset-id}` response (asset parameters).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetParams {
    #[serde(rename = "total")]
    pub total: u64,
    #[serde(rename = "decimals", default)]
    pub decimals: u32,
    #[serde(rename = "default-frozen", default)]
    pub default_frozen: bool,
    #[serde(rename = "unit-name", default, alias = "unit-name")]
    pub unit_name: String,
    #[serde(rename = "name", default)]
    pub name: String,
    #[serde(rename = "url", default)]
    pub url: String,
    #[serde(rename = "metadata-hash", default)]
    pub metadata_hash: String,
    #[serde(rename = "manager", default)]
    pub manager: String,
    #[serde(rename = "reserve", default)]
    pub reserve: String,
    #[serde(rename = "freeze", default)]
    pub freeze: String,
    #[serde(rename = "clawback", default)]
    pub clawback: String,
    #[serde(rename = "creator", default)]
    pub creator: String,
    #[serde(rename = "created-at-round", default)]
    pub created_at_round: u64,
    #[serde(rename = "destroyed-at-round", default)]
    pub destroyed_at_round: u64,
}

/// `GET /v2/applications/{application-id}` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationInfo {
    #[serde(rename = "id")]
    pub id: u64,
    #[serde(rename = "params")]
    pub params: ApplicationParams,
}

/// Application box returned by algod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationBox {
    #[serde(rename = "round")]
    pub round: u64,
    /// Base64-encoded box name.
    #[serde(rename = "name")]
    pub name: String,
    /// Base64-encoded box value.
    #[serde(rename = "value")]
    pub value: String,
}

/// Application parameters returned by algod.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplicationParams {
    #[serde(rename = "creator", default)]
    pub creator: String,
    #[serde(rename = "approval-program", default)]
    pub approval_program: String,
    #[serde(rename = "clear-state-program", default)]
    pub clear_state_program: String,
    #[serde(rename = "global-state", default)]
    pub global_state: Vec<TealKeyValue>,
    #[serde(rename = "global-state-schema", default)]
    pub global_state_schema: Option<ApplicationStateSchema>,
    #[serde(rename = "local-state-schema", default)]
    pub local_state_schema: Option<ApplicationStateSchema>,
    #[serde(rename = "extra-program-pages", default)]
    pub extra_program_pages: u32,
}

/// TEAL key/value pair returned in application global or local state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TealKeyValue {
    /// Base64-encoded key bytes.
    #[serde(rename = "key")]
    pub key: String,
    #[serde(rename = "value")]
    pub value: TealValue,
}

/// TEAL value. Type 1 is bytes and type 2 is uint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TealValue {
    #[serde(rename = "type", default)]
    pub value_type: u64,
    #[serde(rename = "bytes", default)]
    pub bytes: String,
    #[serde(rename = "uint", default)]
    pub uint: u64,
}

/// Application local/global state schema.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplicationStateSchema {
    #[serde(rename = "num-uint", default)]
    pub num_uint: u64,
    #[serde(rename = "num-byte-slice", default)]
    pub num_byte_slice: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_account_info() {
        let json = r#"{
            "round": 100,
            "address": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "amount": 1000000,
            "amount-without-pending-rewards": 990000,
            "pending-rewards": 10000,
            "min-balance": 100000,
            "status": "Online",
            "assets": [
                {"asset-id": 312769, "amount": 500, "is-frozen": false, "creator": "XYZ"}
            ],
            "created-assets": []
        }"#;
        let info: AccountInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.amount, 1000000);
        assert_eq!(info.assets.len(), 1);
        assert_eq!(info.assets[0].asset_id, 312769);
    }

    #[test]
    fn parse_asset_params() {
        let json = r#"{
            "total": 1000000000,
            "decimals": 6,
            "unit-name": "USDC",
            "name": "USD Coin",
            "url": "https://www.centre.io",
            "manager": "MGR",
            "reserve": "RSV",
            "freeze": "FRZ",
            "clawback": "CLW"
        }"#;
        let p: AssetParams = serde_json::from_str(json).unwrap();
        assert_eq!(p.unit_name, "USDC");
        assert_eq!(p.decimals, 6);
    }

    #[test]
    fn parse_application_info_with_global_state() {
        let json = r#"{
            "id": 123,
            "params": {
                "creator": "CREATOR",
                "approval-program": "AQ==",
                "clear-state-program": "AQ==",
                "global-state": [
                    {
                        "key": "YXNzZXRfMA==",
                        "value": { "type": 2, "uint": 0 }
                    },
                    {
                        "key": "cG9vbF9rZXk=",
                        "value": { "type": 1, "bytes": "AQID" }
                    }
                ],
                "global-state-schema": {
                    "num-uint": 8,
                    "num-byte-slice": 1
                }
            }
        }"#;
        let info: ApplicationInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, 123);
        assert_eq!(info.params.global_state.len(), 2);
        assert_eq!(info.params.global_state[0].value.value_type, 2);
        assert_eq!(info.params.global_state[1].value.bytes, "AQID");
        assert_eq!(
            info.params.global_state_schema.as_ref().unwrap().num_uint,
            8
        );
    }

    #[test]
    fn parse_application_box() {
        let json = r#"{
            "round": 42,
            "name": "AQID",
            "value": "AAAAAAAABNI="
        }"#;
        let info: ApplicationBox = serde_json::from_str(json).unwrap();
        assert_eq!(info.round, 42);
        assert_eq!(info.name, "AQID");
        assert_eq!(info.value, "AAAAAAAABNI=");
    }
}
