//! PIN authentication using argon2id.
//!
//! The first-time setup PIN must be derived from / verified against an
//! Algorand 25-word mnemonic. After that, the user can change the PIN freely.
//! The mnemonic itself is never persisted; only the argon2id hash of the PIN
//! is stored on disk.

use std::path::Path;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use opennodia_core::{Error, Result};
use zeroize::Zeroize;

/// Minimum and maximum PIN length.
pub const PIN_MIN_LEN: usize = 4;
pub const PIN_MAX_LEN: usize = 32;

/// A PIN that is wiped from memory when dropped.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Pin(String);

impl Pin {
    /// Create a PIN from a string, validating length.
    pub fn new(pin: impl Into<String>) -> Result<Self> {
        let pin = pin.into();
        if pin.len() < PIN_MIN_LEN || pin.len() > PIN_MAX_LEN {
            return Err(Error::Auth(format!(
                "PIN must be between {PIN_MIN_LEN} and {PIN_MAX_LEN} characters"
            )));
        }
        Ok(Self(pin))
    }

    /// Access the raw PIN string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A 25-word Algorand mnemonic, zeroized on drop.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Mnemonic(String);

impl Mnemonic {
    pub fn new(mnemonic: impl Into<String>) -> Self {
        Self(mnemonic.into())
    }

    /// Validate that the mnemonic is 25 BIP-39 words.
    ///
    /// Note: full checksum verification requires the Algorand SDK. Here we
    /// perform structural validation (25 whitespace-separated words). The
    /// server's setup endpoint will additionally derive the address to
    /// confirm validity.
    pub fn validate(&self) -> Result<()> {
        let words: Vec<&str> = self.0.split_whitespace().collect();
        if words.len() != 25 {
            return Err(Error::Auth(format!(
                "mnemonic must be 25 words, got {}",
                words.len()
            )));
        }
        // Basic character set check (BIP-39 words are lowercase ascii).
        for w in &words {
            if !w.chars().all(|c| c.is_ascii_lowercase()) {
                return Err(Error::Auth("mnemonic contains invalid characters".into()));
            }
        }
        Ok(())
    }
}

/// The on-disk PIN store: an argon2id hash string.
#[derive(Debug, Clone)]
pub struct PinStore {
    hash: String,
}

impl PinStore {
    /// Hash a PIN and create a new store.
    pub fn from_pin(pin: &Pin) -> Result<Self> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(pin.as_str().as_bytes(), &salt)
            .map_err(|e| Error::Auth(format!("hash pin: {e}")))?
            .to_string();
        Ok(Self { hash })
    }

    /// Save the hash to disk.
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, &self.hash)?;
        Ok(())
    }

    /// Load a hash from disk. Returns `None` if the file does not exist
    /// (meaning setup has not been completed).
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let hash = std::fs::read_to_string(path)?;
        let hash = hash.trim().to_string();
        if hash.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self { hash }))
    }

    /// Verify a PIN against the stored hash.
    pub fn verify(&self, pin: &Pin) -> bool {
        let parsed = match PasswordHash::new(&self.hash) {
            Ok(h) => h,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(pin.as_str().as_bytes(), &parsed)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_roundtrip() {
        let pin = Pin::new("correct horse").unwrap();
        let store = PinStore::from_pin(&pin).unwrap();
        assert!(store.verify(&pin));
        assert!(!store.verify(&Pin::new("wrong pin").unwrap()));
    }

    #[test]
    fn pin_length_validation() {
        assert!(Pin::new("abc").is_err()); // too short
        assert!(Pin::new("1234").is_ok());
        assert!(Pin::new("x".repeat(33)).is_err()); // too long
    }

    #[test]
    fn mnemonic_validation() {
        let good = Mnemonic::new(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art",
        );
        assert!(good.validate().is_ok());

        let short = Mnemonic::new("abandon abandon abandon");
        assert!(short.validate().is_err());

        let bad_chars = Mnemonic::new(format!("{} {}", "abandon ".repeat(24), "ART1"));
        assert!(bad_chars.validate().is_err());
    }

    #[test]
    fn pin_store_load_save() {
        let dir = std::env::temp_dir().join("opennodia_test_pin");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pin.hash");

        // No file -> None
        assert!(PinStore::load(&path).unwrap().is_none());

        // Save and reload
        let pin = Pin::new("supersecret").unwrap();
        let store = PinStore::from_pin(&pin).unwrap();
        store.save(&path).unwrap();

        let loaded = PinStore::load(&path).unwrap().unwrap();
        assert!(loaded.verify(&pin));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
