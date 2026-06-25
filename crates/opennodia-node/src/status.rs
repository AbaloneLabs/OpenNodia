//! Node status types returned by the algod client.

use serde::{Deserialize, Serialize};

/// Normalized node status, converted from the raw algod response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    /// The latest round known to the node.
    pub last_round: opennodia_core::Round,
    /// The consensus protocol version.
    #[serde(default)]
    pub last_version: String,
    /// Nanoseconds since the last round was added.
    #[serde(default)]
    pub time_since_last_round: u64,
    /// Catchup progress in nanoseconds (0 when caught up).
    #[serde(default)]
    pub catchup_time: u64,
}

impl NodeStatus {
    /// Whether the node is caught up to the network (no active catchup).
    pub fn is_caught_up(&self) -> bool {
        self.catchup_time == 0
    }

    /// Human-readable sync status.
    pub fn sync_label(&self) -> &'static str {
        if self.is_caught_up() {
            "Synced"
        } else {
            "Catching up"
        }
    }
}

/// Raw algod `GET /v2/status` response.
#[derive(Debug, Clone, Deserialize)]
pub struct NodeStatusResponse {
    #[serde(rename = "last-round")]
    pub last_round: u64,
    #[serde(rename = "last-version", default)]
    pub last_version: String,
    #[serde(rename = "time-since-last-round", default)]
    pub time_since_last_round: u64,
    #[serde(rename = "catchup-time", default)]
    pub catchup_time: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_label() {
        let synced = NodeStatus {
            last_round: opennodia_core::Round(100),
            last_version: "future".into(),
            time_since_last_round: 3_300_000_000,
            catchup_time: 0,
        };
        assert!(synced.is_caught_up());
        assert_eq!(synced.sync_label(), "Synced");

        let catching = NodeStatus {
            catchup_time: 5000,
            ..synced
        };
        assert_eq!(catching.sync_label(), "Catching up");
    }
}
