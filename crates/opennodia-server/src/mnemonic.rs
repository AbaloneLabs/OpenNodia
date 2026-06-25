//! Algorand mnemonic (25-word) to ed25519 private key conversion.
//!
//! Algorand uses a custom mnemonic scheme (not BIP-39):
//! - 2048-word wordlist (same as BIP-39 English)
//! - 25 words encode 32 bytes (256 bits) of seed data + an 8-bit checksum
//! - Each word encodes 11 bits; 25 words = 275 bits, of which 264 are data
//! - The checksum is the first 8 bits of SHA-512/256 of the 32-byte seed
//! - The 32-byte seed is used directly as the ed25519 private key

use ed25519_dalek::SigningKey;
use opennodia_core::{Error, Result};
use sha2::{Digest, Sha512_256};
use zeroize::Zeroize;

/// Convert a 25-word mnemonic to a 32-byte ed25519 private key.
///
/// The returned key should be zeroized after use.
pub fn mnemonic_to_seed(mnemonic: &str) -> Result<[u8; 32]> {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    if words.len() != 25 {
        return Err(Error::Auth(format!(
            "mnemonic must be 25 words, got {}",
            words.len()
        )));
    }

    // Look up each word in the wordlist to get its 11-bit index.
    let mut bits: Vec<u8> = Vec::with_capacity(25 * 11);
    for word in &words {
        let idx = wordlist::lookup(word)
            .ok_or_else(|| Error::Auth(format!("word not in wordlist: {word}")))?;
        // Push 11 bits MSB-first.
        for i in (0..11).rev() {
            bits.push(((idx >> i) & 1) as u8);
        }
    }

    // First 256 bits = seed, last 8 bits (of 264) = checksum, remaining are padding.
    // Actually: 25 words * 11 bits = 275 bits.
    // 32 bytes * 8 = 256 bits of seed, then 8 bits checksum = 264 bits used.
    // The remaining 11 bits (275 - 264) are zero padding in the last word.

    let mut seed = [0u8; 32];
    for i in 0..256 {
        seed[i / 8] |= bits[i] << (7 - (i % 8));
    }

    // Verify checksum: first byte of SHA-512/256(seed) should match bits 256..264.
    let hash = Sha512_256::digest(seed);
    let checksum_byte = hash[0];

    let mut extracted_checksum = 0u8;
    for i in 0..8 {
        extracted_checksum |= bits[256 + i] << (7 - i);
    }

    if extracted_checksum != checksum_byte {
        return Err(Error::Auth("mnemonic checksum verification failed".into()));
    }

    Ok(seed)
}

/// Derive the Algorand address (as a base32 string) from a mnemonic.
///
/// The seed is zeroized after deriving the public key.
pub fn mnemonic_to_address(mnemonic: &str) -> Result<String> {
    let mut seed = mnemonic_to_seed(mnemonic)?;
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let pubkey = verifying_key.to_bytes();

    // Encode as Algorand address: base32(pubkey + checksum).
    let mut raw = [0u8; 36];
    raw[..32].copy_from_slice(&pubkey);
    let hash = Sha512_256::digest(pubkey);
    raw[32..].copy_from_slice(&hash[hash.len() - 4..]);

    let address = base32_encode(&raw);

    seed.zeroize();
    Ok(address)
}

/// Encode bytes using RFC 4648 base32 without padding.
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits: u64 = 0;
    let mut nbits = 0u32;
    let mut out = String::with_capacity((data.len() * 8).div_ceil(5));
    for &b in data {
        bits = (bits << 8) | b as u64;
        nbits += 8;
        while nbits >= 5 {
            nbits -= 5;
            let idx = ((bits >> nbits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if nbits > 0 {
        let idx = ((bits << (5 - nbits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Get the raw 32-byte private key from a mnemonic for kmd import.
///
/// The caller is responsible for zeroizing the returned key.
pub fn mnemonic_to_private_key(mnemonic: &str) -> Result<[u8; 32]> {
    mnemonic_to_seed(mnemonic)
}

mod wordlist {
    //! Algorand uses the same 2048-word English wordlist as BIP-39.
    use once_cell::sync::Lazy;
    use std::collections::HashMap;

    static WORDLIST_TEXT: &str = include_str!("wordlist.txt");

    pub static WORDS: Lazy<Vec<&'static str>> = Lazy::new(|| WORDLIST_TEXT.lines().collect());

    pub static WORD_MAP: Lazy<HashMap<&'static str, usize>> =
        Lazy::new(|| WORDS.iter().enumerate().map(|(i, &w)| (w, i)).collect());

    /// Look up a word and return its 11-bit index.
    pub fn lookup(word: &str) -> Option<usize> {
        WORD_MAP.get(word).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnemonic_runs_without_panic() {
        // A valid 25-word Algorand mnemonic (generated from seed 0x00..0x1f).
        let mnemonic = "abandon amount liar amount expire adjust cage candy arch \
                        gather drum bullet absurd math era live bid rhythm alien \
                        crouch range attend journey wage abandon";

        let result = mnemonic_to_address(mnemonic);
        assert!(result.is_ok(), "mnemonic should be valid: {result:?}");
        let addr = result.unwrap();
        assert_eq!(addr.len(), 58, "address should be 58 chars");
        assert!(addr
            .chars()
            .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
    }

    #[test]
    fn reject_short_mnemonic() {
        assert!(mnemonic_to_seed("abandon abandon abandon").is_err());
    }

    #[test]
    fn reject_unknown_word() {
        let m = "abandon abandon abandon abandon abandon abandon abandon abandon \
                 abandon abandon abandon abandon abandon abandon abandon abandon \
                 abandon abandon abandon abandon abandon abandon abandon abandon zzqqxx";
        assert!(mnemonic_to_seed(m).is_err());
    }
}
