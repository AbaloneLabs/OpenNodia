//! Session management with HMAC-signed tokens.
//!
//! Tokens are stateless (JWT-like): a base64 payload + HMAC-SHA256 signature.
//! The server keeps an in-memory allowlist of active session IDs for
//! revocation on logout.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use sha2::Sha256;
use tokio::sync::Mutex;

type HmacSha256 = Hmac<Sha256>;

/// Name of the HttpOnly browser session cookie.
pub const SESSION_COOKIE_NAME: &str = "opennodia_session";

/// Default session lifetime.
const DEFAULT_TTL: Duration = Duration::from_secs(8 * 60 * 60);

/// A session manager that issues and validates tokens.
#[derive(Debug, Clone)]
pub struct SessionStore {
    inner: Arc<Mutex<Inner>>,
    signing_key: [u8; 32],
    ttl: Duration,
}

#[derive(Debug, Default)]
struct Inner {
    /// Active session IDs (for revocation).
    active: HashSet<String>,
    /// Failed-attempt tracking per client IP (rate limiting hook).
    last_activity: std::collections::HashMap<String, Instant>,
}

/// A decoded, valid session.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Session {
    pub sid: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

/// Raw authenticated session token carried through request extensions.
#[derive(Debug, Clone)]
pub struct SessionToken(pub String);

impl SessionToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SessionStore {
    /// Create a new session store with a random signing key.
    pub fn new(ttl: Duration) -> Self {
        use rand::RngCore;
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            signing_key: key,
            ttl,
        }
    }

    /// Create a session store with a fixed key (for testing).
    #[cfg(test)]
    pub fn with_key(ttl: Duration, key: [u8; 32]) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            signing_key: key,
            ttl,
        }
    }

    /// Issue a new session token string.
    pub async fn issue(&self) -> String {
        use rand::Rng;
        let sid: String = (0..16)
            .map(|_| rand::thread_rng().gen_range(0..36))
            .map(|i| {
                if i < 10 {
                    (b'0' + i as u8) as char
                } else {
                    (b'a' + (i - 10) as u8) as char
                }
            })
            .collect();

        let now = now_secs();
        let payload = Payload {
            sid: sid.clone(),
            issued_at: now,
            expires_at: now + self.ttl.as_secs(),
        };

        {
            let mut inner = self.inner.lock().await;
            inner.active.insert(sid.clone());
            inner.last_activity.insert(sid, Instant::now());
        }

        self.encode(&payload)
    }

    /// Validate a token string. Returns the session if valid and active.
    pub async fn validate(&self, token: &str) -> Option<Session> {
        let payload = self.decode(token)?;
        let now = now_secs();
        if now > payload.expires_at {
            return None;
        }
        let inner = self.inner.lock().await;
        if !inner.active.contains(&payload.sid) {
            return None;
        }
        Some(Session {
            sid: payload.sid,
            issued_at: payload.issued_at,
            expires_at: payload.expires_at,
        })
    }

    /// Revoke a session (logout).
    pub async fn revoke(&self, token: &str) -> bool {
        let Some(payload) = self.decode(token) else {
            return false;
        };
        let mut inner = self.inner.lock().await;
        inner.active.remove(&payload.sid)
    }

    fn encode(&self, payload: &Payload) -> String {
        let json = serde_json::to_vec(payload).unwrap();
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&json);
        let mut mac = HmacSha256::new_from_slice(&self.signing_key).expect("hmac key length");
        mac.update(b64.as_bytes());
        let sig = mac.finalize().into_bytes();
        let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sig);
        format!("{b64}.{sig_b64}")
    }

    fn decode(&self, token: &str) -> Option<Payload> {
        let (b64, sig_b64) = token.split_once('.')?;
        let mut mac = HmacSha256::new_from_slice(&self.signing_key).ok()?;
        mac.update(b64.as_bytes());
        let given = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sig_b64)
            .ok()?;
        if mac.verify_slice(&given).is_err() {
            return None;
        }
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .ok()?;
        let payload: Payload = serde_json::from_slice(&json).ok()?;
        Some(payload)
    }
}

/// Build the browser session cookie value.
pub fn session_cookie(token: &str, max_age_secs: u64) -> String {
    format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; Max-Age={max_age_secs}; HttpOnly; SameSite=Strict"
    )
}

/// Build a cookie value that expires the browser session immediately.
pub fn clear_session_cookie() -> String {
    format!("{SESSION_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict")
}

impl Default for SessionStore {
    fn default() -> Self {
        static DEFAULT: Lazy<Duration> = Lazy::new(|| DEFAULT_TTL);
        Self::new(*DEFAULT)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Payload {
    sid: String,
    issued_at: u64,
    expires_at: u64,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn issue_and_validate() {
        let store = SessionStore::new(Duration::from_secs(3600));
        let token = store.issue().await;
        let session = store.validate(&token).await;
        assert!(session.is_some());
        let s = session.unwrap();
        assert!(s.expires_at > s.issued_at);
    }

    #[tokio::test]
    async fn revoke_invalidates() {
        let store = SessionStore::new(Duration::from_secs(3600));
        let token = store.issue().await;
        assert!(store.validate(&token).await.is_some());
        assert!(store.revoke(&token).await);
        assert!(store.validate(&token).await.is_none());
    }

    #[tokio::test]
    async fn tampered_token_rejected() {
        let store = SessionStore::new(Duration::from_secs(3600));
        let token = store.issue().await;
        let tampered = format!("{}X", &token[..token.len() - 1]);
        assert!(store.validate(&tampered).await.is_none());
    }

    #[tokio::test]
    async fn expired_token_rejected() {
        let store = SessionStore::new(Duration::from_secs(0));
        let token = store.issue().await;
        // expires_at == now, so now > expires_at is true on next tick
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(store.validate(&token).await.is_none());
    }

    #[test]
    fn session_cookie_is_httponly_and_strict() {
        let cookie = session_cookie("token.value", 60);
        assert!(cookie.starts_with("opennodia_session=token.value;"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Max-Age=60"));
    }

    #[test]
    fn clear_cookie_expires_session_cookie() {
        let cookie = clear_session_cookie();
        assert!(cookie.starts_with("opennodia_session=;"));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("HttpOnly"));
    }
}
