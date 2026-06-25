//! Reusable one-time transaction intent storage.
//!
//! Write flows use a prepare/submit pattern: prepare stores the exact action
//! that the user reviewed, and submit consumes it once after rechecking the
//! session and wallet binding. Keeping this logic shared prevents future ASA,
//! LP, and routing flows from reimplementing slightly different intent rules.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::RngCore;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum IntentStoreError {
    #[error("invalid intent id")]
    InvalidId,
    #[error("intent is missing, expired, or already used")]
    Missing,
    #[error("intent does not belong to this session and wallet")]
    OwnerMismatch,
    #[error("too many pending intents")]
    Capacity,
}

#[derive(Debug)]
struct StoredIntent<T> {
    session_id: String,
    wallet_id: String,
    expires_at: Instant,
    action: T,
}

#[derive(Debug)]
pub(crate) struct IntentStore<T> {
    inner: Arc<Mutex<HashMap<String, StoredIntent<T>>>>,
    max_pending: usize,
}

impl<T> Clone for IntentStore<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            max_pending: self.max_pending,
        }
    }
}

impl<T> IntentStore<T> {
    pub(crate) fn new(max_pending: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_pending,
        }
    }

    pub(crate) async fn store(
        &self,
        session_id: &str,
        wallet_id: &str,
        ttl: Duration,
        action: T,
    ) -> Result<String, IntentStoreError> {
        let mut intents = self.inner.lock().await;
        prune_expired(&mut intents);
        if intents.len() >= self.max_pending {
            return Err(IntentStoreError::Capacity);
        }

        let intent_id = unique_intent_id(&intents);
        intents.insert(
            intent_id.clone(),
            StoredIntent {
                session_id: session_id.to_string(),
                wallet_id: wallet_id.to_string(),
                expires_at: Instant::now() + ttl,
                action,
            },
        );
        Ok(intent_id)
    }

    pub(crate) async fn take(
        &self,
        session_id: &str,
        wallet_id: &str,
        intent_id: &str,
    ) -> Result<T, IntentStoreError> {
        if !is_valid_intent_id(intent_id) {
            return Err(IntentStoreError::InvalidId);
        }

        let mut intents = self.inner.lock().await;
        prune_expired(&mut intents);
        let intent = intents.get(intent_id).ok_or(IntentStoreError::Missing)?;
        if intent.session_id != session_id || intent.wallet_id != wallet_id {
            return Err(IntentStoreError::OwnerMismatch);
        }
        let intent = intents.remove(intent_id).ok_or(IntentStoreError::Missing)?;
        Ok(intent.action)
    }
}

fn prune_expired<T>(intents: &mut HashMap<String, StoredIntent<T>>) {
    let now = Instant::now();
    intents.retain(|_, intent| intent.expires_at > now);
}

fn is_valid_intent_id(intent_id: &str) -> bool {
    intent_id.len() == 64 && intent_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn unique_intent_id<T>(intents: &HashMap<String, StoredIntent<T>>) -> String {
    loop {
        let mut random = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut random);
        let intent_id = hex::encode(random);
        if !intents.contains_key(&intent_id) {
            return intent_id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_and_take_requires_matching_owner() {
        let store = IntentStore::new(8);
        let id = store
            .store("session-a", "wallet-a", Duration::from_secs(60), "action")
            .await
            .unwrap();

        let err = store.take("session-b", "wallet-a", &id).await.unwrap_err();
        assert_eq!(err, IntentStoreError::OwnerMismatch);

        assert_eq!(
            store.take("session-a", "wallet-a", &id).await.unwrap(),
            "action"
        );
    }

    #[tokio::test]
    async fn take_consumes_intent_once() {
        let store = IntentStore::new(8);
        let id = store
            .store("session-a", "wallet-a", Duration::from_secs(60), 42)
            .await
            .unwrap();

        assert_eq!(store.take("session-a", "wallet-a", &id).await.unwrap(), 42);
        let err = store.take("session-a", "wallet-a", &id).await.unwrap_err();
        assert_eq!(err, IntentStoreError::Missing);
    }

    #[tokio::test]
    async fn expired_intent_is_removed() {
        let store = IntentStore::new(8);
        let id = store
            .store("session-a", "wallet-a", Duration::from_millis(1), 42)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        let err = store.take("session-a", "wallet-a", &id).await.unwrap_err();
        assert_eq!(err, IntentStoreError::Missing);
    }

    #[tokio::test]
    async fn capacity_is_enforced_after_pruning() {
        let store = IntentStore::new(1);
        store
            .store("session-a", "wallet-a", Duration::from_millis(1), 1)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        store
            .store("session-a", "wallet-a", Duration::from_secs(60), 2)
            .await
            .unwrap();
        let err = store
            .store("session-a", "wallet-a", Duration::from_secs(60), 3)
            .await
            .unwrap_err();
        assert_eq!(err, IntentStoreError::Capacity);
    }
}
