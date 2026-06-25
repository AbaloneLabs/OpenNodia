//! Order link encoding: shareable, verifiable order payloads.
//!
//! Order links encode the full order parameters (side, assets, amounts, owner,
//! escrow address, expiry) into a compact base64url string suitable for URLs:
//!
//! ```text
//! /#/dex/order/{payload}
//! ```
//!
//! The link is **untrusted** — it is informational only. When opened, the
//! frontend must re-derive the escrow address from the embedded params and
//! verify it matches, then query the chain for the actual escrow state.

use opennodia_core::Address;
use serde::{Deserialize, Serialize};

use crate::escrow::EscrowKind;
use crate::order::OrderSide;

/// Current order link format version.
pub const ORDER_LINK_VERSION: u8 = 1;

/// A shareable order payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLinkPayload {
    /// Format version (currently 1).
    pub version: u8,
    /// Side of the order (sell or buy escrow).
    pub side: OrderSide,
    /// ASA being sold (Sell) or received (Buy).
    pub sell_asset: u64,
    /// Raw units of the sell asset.
    pub sell_amount: u64,
    /// Asset being bought (Sell) or paid (Buy). 0 = ALGO.
    pub buy_asset: u64,
    /// Raw units of the buy asset.
    pub buy_amount: u64,
    /// Owner address (32 bytes).
    pub owner: [u8; 32],
    /// Escrow LogicSig address (32 bytes).
    pub escrow: [u8; 32],
    /// Expiry round (enforced in-contract via `txn FirstValid`).
    pub expire_round: u64,
}

impl OrderLinkPayload {
    /// Build a payload from order parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        side: OrderSide,
        sell_asset: u64,
        sell_amount: u64,
        buy_asset: u64,
        buy_amount: u64,
        owner: Address,
        escrow: Address,
        expire_round: u64,
    ) -> Self {
        Self {
            version: ORDER_LINK_VERSION,
            side,
            sell_asset,
            sell_amount,
            buy_asset,
            buy_amount,
            owner: *owner.as_bytes(),
            escrow: *escrow.as_bytes(),
            expire_round,
        }
    }

    /// The owner as an [`Address`].
    pub fn owner_address(&self) -> Address {
        Address::from_bytes(self.owner)
    }

    /// The escrow as an [`Address`].
    pub fn escrow_address(&self) -> Address {
        Address::from_bytes(self.escrow)
    }

    /// The escrow kind (Sell or Buy) corresponding to this payload's side.
    pub fn escrow_kind(&self) -> EscrowKind {
        match self.side {
            OrderSide::Sell => EscrowKind::Sell,
            OrderSide::Buy => EscrowKind::Buy,
        }
    }
}

/// Encode an order payload to a base64url string (no padding).
///
/// The payload is msgpack-encoded then base64url'd for URL safety.
pub fn encode_order_link(payload: &OrderLinkPayload) -> opennodia_core::Result<String> {
    let bytes = rmp_serde::to_vec_named(payload)
        .map_err(|e| opennodia_core::Error::Other(format!("order link encode: {e}")))?;
    Ok(base64_url_encode(&bytes))
}

/// Decode a base64url order link string into a payload.
///
/// Validates the version and basic structure. Does NOT verify the escrow
/// address — callers must re-derive and verify on-chain before trusting.
pub fn decode_order_link(s: &str) -> opennodia_core::Result<OrderLinkPayload> {
    let bytes = base64_url_decode(s)
        .ok_or_else(|| opennodia_core::Error::Other("order link: invalid base64url".into()))?;
    let payload: OrderLinkPayload = rmp_serde::from_slice(&bytes)
        .map_err(|e| opennodia_core::Error::Other(format!("order link decode: {e}")))?;
    if payload.version != ORDER_LINK_VERSION {
        return Err(opennodia_core::Error::Other(format!(
            "order link: unsupported version {} (expected {ORDER_LINK_VERSION})",
            payload.version
        )));
    }
    Ok(payload)
}

/// Build the full URL fragment for an order link: `/#/dex/order/{payload}`.
pub fn order_link_url(payload: &OrderLinkPayload) -> opennodia_core::Result<String> {
    let encoded = encode_order_link(payload)?;
    Ok(format!("/#/dex/order/{encoded}"))
}

// ----------------------------------------------------------------------------
// base64url helpers (no external dependency for this small use case)
// ----------------------------------------------------------------------------

const B64_URL_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes as base64url (no padding).
fn base64_url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 3 <= data.len() {
        let b0 = data[i];
        let b1 = data[i + 1];
        let b2 = data[i + 2];
        out.push(B64_URL_ALPHABET[(b0 >> 2) as usize] as char);
        out.push(B64_URL_ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(B64_URL_ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        out.push(B64_URL_ALPHABET[(b2 & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let b0 = data[i];
        out.push(B64_URL_ALPHABET[(b0 >> 2) as usize] as char);
        out.push(B64_URL_ALPHABET[((b0 & 0x03) << 4) as usize] as char);
    } else if rem == 2 {
        let b0 = data[i];
        let b1 = data[i + 1];
        out.push(B64_URL_ALPHABET[(b0 >> 2) as usize] as char);
        out.push(B64_URL_ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(B64_URL_ALPHABET[((b1 & 0x0f) << 2) as usize] as char);
    }
    out
}

/// Decode a base64url string (no padding). Accepts standard base64 too.
fn base64_url_decode(s: &str) -> Option<Vec<u8>> {
    let mut chars: Vec<u8> = s.bytes().collect();
    // Translate standard base64 to url alphabet.
    for b in chars.iter_mut() {
        match *b {
            b'+' => *b = b'-',
            b'/' => *b = b'_',
            _ => {}
        }
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut nbits: u32 = 0;
    for &b in &chars {
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'-' => 62,
            b'_' => 63,
            b'=' => continue, // padding (shouldn't appear in url, but tolerate)
            _ if (b as char).is_whitespace() => continue,
            _ => return None,
        };
        buf = (buf << 6) | val;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((buf >> nbits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> OrderLinkPayload {
        OrderLinkPayload::new(
            OrderSide::Sell,
            12345,
            1_000,
            0,
            2_000_000,
            Address::from_bytes([1u8; 32]),
            Address::from_bytes([2u8; 32]),
            100_000,
        )
    }

    #[test]
    fn encode_decode_roundtrip() {
        let p = sample_payload();
        let s = encode_order_link(&p).unwrap();
        let p2 = decode_order_link(&s).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn encoded_is_url_safe() {
        let p = sample_payload();
        let s = encode_order_link(&p).unwrap();
        // No padding, no standard-base64 chars that need URL escaping.
        assert!(!s.contains('='));
        assert!(!s.contains('+'));
        assert!(!s.contains('/'));
    }

    #[test]
    fn url_fragment_format() {
        let p = sample_payload();
        let url = order_link_url(&p).unwrap();
        assert!(url.starts_with("/#/dex/order/"));
    }

    #[test]
    fn decode_rejects_bad_base64() {
        assert!(decode_order_link("!!!not-base64!!!").is_err());
    }

    #[test]
    fn decode_rejects_wrong_version() {
        let mut p = sample_payload();
        p.version = 99;
        let s = encode_order_link(&p).unwrap();
        // encode succeeds (we don't validate on encode), but decode rejects.
        let res = decode_order_link(&s);
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("version"));
    }

    #[test]
    fn buy_side_roundtrip() {
        let p = OrderLinkPayload::new(
            OrderSide::Buy,
            12345,
            1_000,
            0,
            2_000_000,
            Address::from_bytes([3u8; 32]),
            Address::from_bytes([4u8; 32]),
            50_000,
        );
        let s = encode_order_link(&p).unwrap();
        let p2 = decode_order_link(&s).unwrap();
        assert_eq!(p2.side, OrderSide::Buy);
        assert_eq!(p2.escrow_kind(), EscrowKind::Buy);
    }

    #[test]
    fn base64url_roundtrip_various_lengths() {
        for len in [1, 2, 3, 4, 5, 10, 32, 100] {
            let data: Vec<u8> = (0..len).map(|i| (i * 7) as u8).collect();
            let enc = base64_url_encode(&data);
            let dec = base64_url_decode(&enc).unwrap();
            assert_eq!(dec, data, "roundtrip failed for len {len}");
        }
    }

    #[test]
    fn owner_and_escrow_accessors() {
        let p = sample_payload();
        assert_eq!(p.owner_address(), Address::from_bytes([1u8; 32]));
        assert_eq!(p.escrow_address(), Address::from_bytes([2u8; 32]));
    }
}
