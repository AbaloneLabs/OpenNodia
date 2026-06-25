//! Wallet management: registry, kmd interaction, and handle caching.
//!
//! OpenNodia manages two kinds of wallets:
//! - **Kmd**: created directly via kmd (new key generated internally).
//! - **Imported**: an externally generated key (from a 25-word mnemonic)
//!   imported into a kmd wallet for handle-based access.
//!
//! All wallets live in kmd and are protected by the user's PIN as the kmd
//! wallet password. kmd is never exposed externally — only OpenNodia's
//! PIN-authenticated API can reach it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opennodia_core::Address;
use opennodia_node::KmdClient;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use zeroize::Zeroize;

/// How a wallet was registered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WalletSource {
    /// Created fresh via kmd (`generate_key`).
    Kmd,
    /// Imported from an external mnemonic (`import_key`).
    Imported,
}

/// A registered wallet in the OpenNodia registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredWallet {
    /// kmd wallet ID.
    pub id: String,
    /// Human-friendly name.
    pub name: String,
    /// How this wallet was added.
    pub source: WalletSource,
    /// First address in the wallet (derived at creation/import time).
    pub first_address: String,
    /// Public addresses registered for history synchronization.
    ///
    /// Older registry files omit this field; `first_address` is always
    /// included by the accessors for backward compatibility.
    #[serde(default)]
    pub addresses: Vec<String>,
}

/// The persistent wallet registry (JSON file in the data directory).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WalletRegistry {
    /// All registered wallets.
    pub wallets: Vec<RegisteredWallet>,
    /// Currently active wallet ID (if any).
    pub active_wallet_id: Option<String>,
}

/// Manages wallets: registry persistence, kmd calls, and handle caching.
#[derive(Clone)]
pub struct WalletManager {
    kmd: KmdClient,
    registry: Arc<Mutex<WalletRegistry>>,
    registry_path: PathBuf,
    /// Cached kmd handle tokens, keyed by wallet ID.
    handles: Arc<Mutex<HashMap<String, String>>>,
}

impl std::fmt::Debug for WalletManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalletManager")
            .field("kmd_url", &self.kmd.base_url())
            .field("registry_path", &self.registry_path)
            .finish_non_exhaustive()
    }
}

impl WalletManager {
    /// Create a new wallet manager, loading the registry from disk if it exists.
    pub fn new(kmd: KmdClient, data_dir: &Path) -> anyhow::Result<Self> {
        let registry_path = data_dir.join("wallets.json");
        let registry = if registry_path.exists() {
            let text = std::fs::read_to_string(&registry_path)?;
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            WalletRegistry::default()
        };

        Ok(Self {
            kmd,
            registry: Arc::new(Mutex::new(registry)),
            registry_path,
            handles: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Persist the registry to disk.
    async fn save_registry(&self) -> anyhow::Result<()> {
        let registry = self.registry.lock().await;
        let json = serde_json::to_string_pretty(&*registry)?;
        if let Some(parent) = self.registry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.registry_path, json)?;
        Ok(())
    }

    // ---- Wallet lifecycle ----

    /// Create a brand-new wallet via kmd and register it.
    ///
    /// The PIN is used as the kmd wallet password. A first address is
    /// generated automatically. The wallet is set as active.
    pub async fn create_wallet(&self, name: &str, pin: &str) -> anyhow::Result<RegisteredWallet> {
        // Create the kmd wallet.
        let wallet = self.kmd.create_wallet(name, pin).await?;
        let wallet_id = wallet.id.clone();

        // Init a handle and generate the first key.
        let handle = self.kmd.init_wallet_handle(&wallet_id, pin).await?;
        let first_address = self.kmd.generate_key(&handle).await?;

        // Cache the handle.
        self.handles.lock().await.insert(wallet_id.clone(), handle);

        let registered = RegisteredWallet {
            id: wallet_id,
            name: name.to_string(),
            source: WalletSource::Kmd,
            first_address: first_address.clone(),
            addresses: vec![first_address.clone()],
        };

        // Update registry.
        {
            let mut reg = self.registry.lock().await;
            reg.wallets.push(registered.clone());
            reg.active_wallet_id = Some(registered.id.clone());
        }
        self.save_registry().await?;

        tracing::info!(name, addr = %first_address, "wallet created");
        Ok(registered)
    }

    /// Import an externally generated key (from a 25-word mnemonic) into a
    /// new kmd wallet and register it.
    ///
    /// The mnemonic is converted to a 32-byte private key, imported into kmd,
    /// then immediately zeroized. The PIN is used as the kmd wallet password.
    pub async fn import_wallet(
        &self,
        name: &str,
        mnemonic: &str,
        pin: &str,
    ) -> anyhow::Result<RegisteredWallet> {
        // Derive the private key from the mnemonic.
        let mut private_key = crate::mnemonic::mnemonic_to_private_key(mnemonic)?;

        // Create the kmd wallet.
        let wallet = self.kmd.create_wallet(name, pin).await?;
        let wallet_id = wallet.id.clone();

        // Init a handle and import the key.
        let handle = self.kmd.init_wallet_handle(&wallet_id, pin).await?;
        let first_address = self.kmd.import_key(&handle, &private_key).await?;

        // Zeroize the private key immediately.
        private_key.zeroize();

        // Cache the handle.
        self.handles.lock().await.insert(wallet_id.clone(), handle);

        let registered = RegisteredWallet {
            id: wallet_id,
            name: name.to_string(),
            source: WalletSource::Imported,
            first_address: first_address.clone(),
            addresses: vec![first_address.clone()],
        };

        // Update registry.
        {
            let mut reg = self.registry.lock().await;
            reg.wallets.push(registered.clone());
            reg.active_wallet_id = Some(registered.id.clone());
        }
        self.save_registry().await?;

        tracing::info!(name, addr = %first_address, "wallet imported");
        Ok(registered)
    }

    /// List all registered wallets.
    pub async fn list_wallets(&self) -> Vec<RegisteredWallet> {
        self.registry.lock().await.wallets.clone()
    }

    /// Get the active wallet (if any).
    pub async fn active_wallet(&self) -> Option<RegisteredWallet> {
        let reg = self.registry.lock().await;
        let active_id = reg.active_wallet_id.as_ref()?;
        reg.wallets.iter().find(|w| &w.id == active_id).cloned()
    }

    /// Whether a wallet ID is registered with OpenNodia.
    pub async fn contains_wallet(&self, wallet_id: &str) -> bool {
        self.registry
            .lock()
            .await
            .wallets
            .iter()
            .any(|wallet| wallet.id == wallet_id)
    }

    /// Return all public addresses registered with OpenNodia.
    pub async fn tracked_addresses(&self) -> Vec<String> {
        let registry = self.registry.lock().await;
        let mut addresses = Vec::new();
        for wallet in &registry.wallets {
            if !addresses.contains(&wallet.first_address) {
                addresses.push(wallet.first_address.clone());
            }
            for address in &wallet.addresses {
                if !addresses.contains(address) {
                    addresses.push(address.clone());
                }
            }
        }
        addresses
    }

    /// Whether an address belongs to a registered OpenNodia wallet.
    pub async fn contains_registered_address(&self, address: &str) -> bool {
        self.tracked_addresses()
            .await
            .iter()
            .any(|candidate| candidate == address)
    }

    /// Return tracked addresses for one registered wallet.
    pub async fn tracked_wallet_addresses(&self, wallet_id: &str) -> anyhow::Result<Vec<String>> {
        let registry = self.registry.lock().await;
        let wallet = registry
            .wallets
            .iter()
            .find(|wallet| wallet.id == wallet_id)
            .ok_or_else(|| anyhow::anyhow!("wallet not found: {wallet_id}"))?;
        let mut addresses = vec![wallet.first_address.clone()];
        for address in &wallet.addresses {
            if !addresses.contains(address) {
                addresses.push(address.clone());
            }
        }
        Ok(addresses)
    }

    /// Whether another registered wallet tracks the same public address.
    pub async fn address_registered_elsewhere(&self, wallet_id: &str, address: &str) -> bool {
        let registry = self.registry.lock().await;
        registry.wallets.iter().any(|wallet| {
            wallet.id != wallet_id
                && (wallet.first_address == address
                    || wallet
                        .addresses
                        .iter()
                        .any(|candidate| candidate == address))
        })
    }

    /// Set the active wallet by ID.
    pub async fn activate(&self, wallet_id: &str) -> anyhow::Result<()> {
        let mut reg = self.registry.lock().await;
        if !reg.wallets.iter().any(|w| w.id == wallet_id) {
            anyhow::bail!("wallet not found: {wallet_id}");
        }
        reg.active_wallet_id = Some(wallet_id.to_string());
        drop(reg);
        self.save_registry().await?;
        Ok(())
    }

    /// Rename a wallet in the registry.
    pub async fn rename_wallet(&self, wallet_id: &str, new_name: &str) -> anyhow::Result<()> {
        let mut reg = self.registry.lock().await;
        let wallet = reg
            .wallets
            .iter_mut()
            .find(|w| w.id == wallet_id)
            .ok_or_else(|| anyhow::anyhow!("wallet not found: {wallet_id}"))?;
        wallet.name = new_name.to_string();
        drop(reg);
        self.save_registry().await?;
        Ok(())
    }

    /// Remove a wallet from the registry (does not delete from kmd).
    pub async fn remove(&self, wallet_id: &str) -> anyhow::Result<()> {
        let mut reg = self.registry.lock().await;
        reg.wallets.retain(|w| w.id != wallet_id);
        if reg.active_wallet_id.as_deref() == Some(wallet_id) {
            reg.active_wallet_id = reg.wallets.first().map(|w| w.id.clone());
        }
        drop(reg);
        self.handles.lock().await.remove(wallet_id);
        self.save_registry().await?;
        Ok(())
    }

    // ---- Address management ----

    /// Ensure we have a valid (cached or freshly-inited) handle for a wallet.
    async fn ensure_handle(&self, wallet_id: &str, pin: &str) -> anyhow::Result<String> {
        // Check cache first.
        {
            let handles = self.handles.lock().await;
            if let Some(handle) = handles.get(wallet_id) {
                // Try to use the cached handle by listing keys (validates it).
                if self.kmd.list_keys(handle).await.is_ok() {
                    return Ok(handle.clone());
                }
            }
        }

        // Handle missing or expired — re-init.
        let handle = self.kmd.init_wallet_handle(wallet_id, pin).await?;
        self.handles
            .lock()
            .await
            .insert(wallet_id.to_string(), handle.clone());
        Ok(handle)
    }

    /// List all addresses in a wallet.
    pub async fn list_addresses(&self, wallet_id: &str, pin: &str) -> anyhow::Result<Vec<String>> {
        let handle = self.ensure_handle(wallet_id, pin).await?;
        let addresses = self.kmd.list_keys(&handle).await?;
        Ok(addresses)
    }

    /// Whether an address belongs to a wallet.
    pub async fn contains_address(
        &self,
        wallet_id: &str,
        pin: &str,
        address: &str,
    ) -> anyhow::Result<bool> {
        Ok(self
            .list_addresses(wallet_id, pin)
            .await?
            .iter()
            .any(|candidate| candidate == address))
    }

    /// Generate a new address in a wallet.
    pub async fn generate_address(&self, wallet_id: &str, pin: &str) -> anyhow::Result<String> {
        let handle = self.ensure_handle(wallet_id, pin).await?;
        let address = self.kmd.generate_key(&handle).await?;
        {
            let mut registry = self.registry.lock().await;
            let wallet = registry
                .wallets
                .iter_mut()
                .find(|wallet| wallet.id == wallet_id)
                .ok_or_else(|| anyhow::anyhow!("wallet not found: {wallet_id}"))?;
            if !wallet.addresses.contains(&address) {
                wallet.addresses.push(address.clone());
            }
        }
        self.save_registry().await?;
        Ok(address)
    }

    /// Sign a transaction using the wallet's key managed by kmd.
    ///
    /// The PIN unlocks the wallet handle (cached), then kmd signs the
    /// transaction with the private key. The private key never leaves kmd.
    pub async fn sign_transaction(
        &self,
        wallet_id: &str,
        pin: &str,
        signer_address: &str,
        tx_bytes: &[u8],
    ) -> anyhow::Result<Vec<u8>> {
        let signer = signer_address.parse::<Address>()?;
        let handle = self.ensure_handle(wallet_id, pin).await?;
        let signed = self
            .kmd
            .sign_transaction(&handle, pin, signer.as_bytes(), tx_bytes)
            .await?;
        Ok(signed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_serializes() {
        let reg = WalletRegistry {
            wallets: vec![RegisteredWallet {
                id: "abc".into(),
                name: "test".into(),
                source: WalletSource::Kmd,
                first_address: "ADDR".into(),
                addresses: vec!["ADDR".into()],
            }],
            active_wallet_id: Some("abc".into()),
        };
        let json = serde_json::to_string(&reg).unwrap();
        assert!(json.contains("kmd"));
        assert!(json.contains("abc"));
    }

    #[test]
    fn registry_defaults_empty() {
        let reg = WalletRegistry::default();
        assert!(reg.wallets.is_empty());
        assert!(reg.active_wallet_id.is_none());
    }
}
