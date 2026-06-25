//! Algorand network identifier.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The Algorand network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    /// Algorand MainNet.
    Mainnet,
    /// Algorand TestNet.
    Testnet,
    /// Algorand BetaNet.
    Betanet,
    /// A local private network (e.g. Algorand Sandbox).
    Local,
}

impl Network {
    /// Genesis ID string for this network.
    pub fn genesis_id(self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet-v1.0",
            Network::Testnet => "testnet-v1.0",
            Network::Betanet => "betanet-v1.0",
            Network::Local => "sandnet-v1",
        }
    }

    /// Public relay API base URL for querying the latest network round.
    ///
    /// Returns `None` for local/private networks which have no public relay.
    /// These are Algorand Foundation's free public relay endpoints
    /// (<https://algonode.io>).
    pub fn public_api_url(self) -> Option<&'static str> {
        match self {
            Network::Mainnet => Some("https://mainnet-api.algonode.cloud"),
            Network::Testnet => Some("https://testnet-api.algonode.cloud"),
            Network::Betanet => Some("https://betanet-api.algonode.cloud"),
            Network::Local => None,
        }
    }

    /// Public indexer relay base URL for asset search and transaction history.
    ///
    /// Returns `None` for local/private networks which have no public indexer.
    /// These are Algorand Foundation's free public indexer endpoints
    /// (<https://algonode.io>).
    pub fn public_indexer_url(self) -> Option<&'static str> {
        match self {
            Network::Mainnet => Some("https://mainnet-idx.algonode.cloud"),
            Network::Testnet => Some("https://testnet-idx.algonode.cloud"),
            Network::Betanet => Some("https://betanet-idx.algonode.cloud"),
            Network::Local => None,
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet => write!(f, "testnet"),
            Network::Betanet => write!(f, "betanet"),
            Network::Local => write!(f, "local"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_ids() {
        assert_eq!(Network::Mainnet.genesis_id(), "mainnet-v1.0");
        assert_eq!(Network::Testnet.genesis_id(), "testnet-v1.0");
    }

    #[test]
    fn display() {
        assert_eq!(Network::Mainnet.to_string(), "mainnet");
        assert_eq!(Network::Local.to_string(), "local");
    }

    #[test]
    fn serde_lowercase() {
        // TOML/JSON uses lowercase variants (e.g. "local", "mainnet").
        assert_eq!(serde_json::to_string(&Network::Local).unwrap(), "\"local\"");
        let n: Network = serde_json::from_str("\"mainnet\"").unwrap();
        assert_eq!(n, Network::Mainnet);
    }
}
