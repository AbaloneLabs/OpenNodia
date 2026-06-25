//! Algorand address type.
//!
//! Algorand addresses are 32 bytes encoded in base32 with a 4-byte checksum,
//! resulting in a 58-character string. The checksum is the last 4 bytes of
//! SHA-512/256 of the 32-byte public key.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512_256};
use thiserror::Error;

/// A 32-byte Algorand public key / address.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address([u8; 32]);

impl Address {
    /// Create an address from raw 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the raw 32 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The zero address (all zeros), used as a sentinel.
    pub const fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Whether this is the zero address.
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }

    /// Derive the Algorand application account address for an app ID.
    pub fn from_app_id(app_id: u64) -> Self {
        let mut hasher = Sha512_256::new();
        hasher.update(b"appID");
        hasher.update(app_id.to_be_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&digest);
        Self(bytes)
    }

    /// Compute the 4-byte checksum (last 4 bytes of SHA-512/256).
    fn checksum(bytes: &[u8; 32]) -> [u8; 4] {
        let hash = Sha512_256::digest(bytes);
        let mut out = [0u8; 4];
        out.copy_from_slice(&hash[hash.len() - 4..]);
        out
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Encode: 32-byte pubkey + 4-byte checksum, all in base32 (no padding).
        let mut raw = [0u8; 36];
        raw[..32].copy_from_slice(&self.0);
        raw[32..].copy_from_slice(&Self::checksum(&self.0));
        write!(f, "{}", base32_encode(&raw))
    }
}

// ---- Base32 (RFC 4648, no padding) ----

const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Encode bytes using RFC 4648 base32 without padding.
fn base32_encode(data: &[u8]) -> String {
    let mut bits: u64 = 0;
    let mut nbits = 0u32;
    let mut out = String::with_capacity((data.len() * 8).div_ceil(5));
    for &b in data {
        bits = (bits << 8) | b as u64;
        nbits += 8;
        while nbits >= 5 {
            nbits -= 5;
            let idx = ((bits >> nbits) & 0x1f) as usize;
            out.push(BASE32_ALPHABET[idx] as char);
        }
    }
    if nbits > 0 {
        let idx = ((bits << (5 - nbits)) & 0x1f) as usize;
        out.push(BASE32_ALPHABET[idx] as char);
    }
    out
}

/// Decode an RFC 4648 base32 string (no padding).
fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut bits: u64 = 0;
    let mut nbits = 0u32;
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    for ch in s.chars() {
        let val = match ch {
            'A'..='Z' => ch as u64 - 'A' as u64,
            'a'..='z' => ch as u64 - 'a' as u64, // accept lowercase
            '2'..='7' => ch as u64 - '2' as u64 + 26,
            _ => return None,
        };
        bits = (bits << 5) | val;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push(((bits >> nbits) & 0xff) as u8);
        }
    }
    Some(out)
}

#[derive(Debug, Error)]
pub enum AddressParseError {
    #[error("invalid address length")]
    InvalidLength,
    #[error("invalid checksum")]
    InvalidChecksum,
    #[error("invalid base32 encoding")]
    InvalidEncoding,
}

impl FromStr for Address {
    type Err = AddressParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Algorand addresses are 58 chars (36 bytes in base32).
        if s.len() != 58 {
            return Err(AddressParseError::InvalidLength);
        }
        let decoded = base32_decode(s).ok_or(AddressParseError::InvalidEncoding)?;
        if decoded.len() != 36 {
            return Err(AddressParseError::InvalidLength);
        }
        let mut pubkey = [0u8; 32];
        pubkey.copy_from_slice(&decoded[..32]);
        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&decoded[32..]);

        // Verify checksum.
        let expected = Self::checksum(&pubkey);
        if checksum != expected {
            return Err(AddressParseError::InvalidChecksum);
        }
        Ok(Self(pubkey))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_address() {
        let z = Address::zero();
        assert!(z.is_zero());
    }

    #[test]
    fn from_roundtrips_bytes() {
        let bytes = [7u8; 32];
        let addr = Address::from_bytes(bytes);
        assert_eq!(addr.as_bytes(), &bytes);
    }

    #[test]
    fn display_non_empty() {
        let addr = Address::from_bytes([1u8; 32]);
        let s = format!("{addr}");
        assert!(!s.is_empty());
    }

    #[test]
    fn known_address_roundtrip() {
        // A real Algorand address derived from a valid mnemonic.
        let addr_str = "AOQQPP7TZYIL4HLQ3UMOOS6ATFT6JVRQTOSQ2XY53SDGIESVGG4MPFYUMQ";
        let addr: Address = addr_str.parse().unwrap();
        // Round-trip: display should produce the same string.
        assert_eq!(format!("{addr}"), addr_str);
    }

    #[test]
    fn reject_bad_checksum() {
        // Flip a char in the checksum region (not just padding bits).
        let bad = "BOQQPP7TZYIL4HLQ3UMOOS6ATFT6JVRQTOSQ2XY53SDGIESVGG4MPFYUMQ";
        assert!(matches!(
            bad.parse::<Address>(),
            Err(AddressParseError::InvalidChecksum)
        ));
    }

    #[test]
    fn reject_wrong_length() {
        assert!("too_short".parse::<Address>().is_err());
    }

    #[test]
    fn base32_roundtrip() {
        let data = [0x42u8; 36];
        let encoded = base32_encode(&data);
        let decoded = base32_decode(&encoded).unwrap();
        assert_eq!(decoded, data.to_vec());
    }

    #[test]
    fn app_address_is_domain_separated_from_raw_index() {
        let app = Address::from_app_id(1);
        assert_ne!(
            app,
            Address::from_bytes(1u64.to_be_bytes().repeat(4).try_into().unwrap())
        );
        assert_eq!(format!("{app}").len(), 58);
        assert_eq!(app, Address::from_app_id(1));
        assert_ne!(app, Address::from_app_id(2));
    }
}
