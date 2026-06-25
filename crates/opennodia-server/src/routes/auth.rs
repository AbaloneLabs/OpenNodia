//! Authentication and session HTTP routes.

use axum::extract::{Extension, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api_error::ApiError;
use crate::auth::{Pin, PinStore};
use crate::session::{clear_session_cookie, session_cookie, Session, SessionToken};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    pub pin: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub pin: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePinRequest {
    pub current_pin: String,
    pub new_pin: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub expires_at: u64,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub expires_at: u64,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub setup_complete: bool,
    pub node_reachable: bool,
    pub network: String,
}

/// `GET /api/status` — whether setup is done and node is reachable.
pub async fn api_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let setup_complete = state.is_setup().await;
    let node_reachable = state.ledger.algod.status().await.is_ok();
    Json(StatusResponse {
        setup_complete,
        node_reachable,
        network: state.config.algod.network.to_string(),
    })
}

/// `POST /api/setup` — first-time setup. Requires only a chosen PIN.
pub async fn api_setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    if state.is_setup().await {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "setup already complete; use change-pin to update",
            )),
        ));
    }

    let pin = Pin::new(req.pin)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError::new(e.to_string()))))?;
    let store = PinStore::from_pin(&pin).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(e.to_string())),
        )
    })?;
    store.save(&state.runtime.pin_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(e.to_string())),
        )
    })?;

    *state.runtime.pin.lock().await = Some(store);

    tracing::info!("initial PIN setup complete");

    let token = state.runtime.sessions.issue().await;
    let expires_at = token_expires(&state, &token).await;
    auth_response(StatusCode::CREATED, &token, expires_at)
}

/// `POST /api/login` — verify PIN, issue session token.
pub async fn api_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    ensure_pin_attempt_allowed(&state).await?;
    let guard = state.runtime.pin.lock().await;
    let Some(store) = guard.as_ref() else {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new("setup not complete")),
        ));
    };

    let pin = Pin::new(req.pin)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError::new(e.to_string()))))?;

    if !store.verify(&pin) {
        drop(guard);
        if let Some(retry_after_secs) = record_pin_failure(&state).await {
            return Err(pin_lockout_error(retry_after_secs));
        }
        return Err((StatusCode::FORBIDDEN, Json(ApiError::new("invalid PIN"))));
    }
    drop(guard);
    record_pin_success(&state).await;

    let token = state.runtime.sessions.issue().await;
    let expires_at = token_expires(&state, &token).await;
    auth_response(StatusCode::OK, &token, expires_at)
}

/// `GET /api/session` — validate the current session cookie/token.
pub async fn api_session(Extension(session): Extension<Session>) -> Json<SessionResponse> {
    Json(SessionResponse {
        expires_at: session.expires_at,
    })
}

/// `POST /api/change-pin` — change PIN (requires current PIN).
pub async fn api_change_pin(
    State(state): State<AppState>,
    Json(req): Json<ChangePinRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    ensure_pin_attempt_allowed(&state).await?;
    let mut guard = state.runtime.pin.lock().await;
    let Some(store) = guard.as_ref() else {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new("setup not complete")),
        ));
    };

    let current = Pin::new(req.current_pin)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError::new(e.to_string()))))?;
    if !store.verify(&current) {
        drop(guard);
        if let Some(retry_after_secs) = record_pin_failure(&state).await {
            return Err(pin_lockout_error(retry_after_secs));
        }
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("current PIN incorrect")),
        ));
    }

    let new_pin = Pin::new(req.new_pin)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError::new(e.to_string()))))?;
    let new_store = PinStore::from_pin(&new_pin).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(e.to_string())),
        )
    })?;
    new_store.save(&state.runtime.pin_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(e.to_string())),
        )
    })?;

    *guard = Some(new_store);
    drop(guard);
    record_pin_success(&state).await;
    tracing::info!("PIN changed");
    Ok(StatusCode::OK)
}

/// `POST /api/logout` — revoke the current session.
pub async fn api_logout(
    State(state): State<AppState>,
    Extension(token): Extension<SessionToken>,
) -> Response {
    state.runtime.sessions.revoke(token.as_str()).await;
    let mut response = StatusCode::OK.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_session_cookie())
            .expect("clear session cookie is a valid header value"),
    );
    response
}

/// Verify the PIN against the stored hash. Returns the PIN string on success
/// (used as the kmd wallet password).
pub(crate) async fn verify_pin(
    state: &AppState,
    pin_str: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    ensure_pin_attempt_allowed(state).await?;
    let guard = state.runtime.pin.lock().await;
    let Some(store) = guard.as_ref() else {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new("setup not complete")),
        ));
    };
    let pin = Pin::new(pin_str)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError::new(e.to_string()))))?;
    if !store.verify(&pin) {
        drop(guard);
        if let Some(retry_after_secs) = record_pin_failure(state).await {
            return Err(pin_lockout_error(retry_after_secs));
        }
        return Err((StatusCode::UNAUTHORIZED, Json(ApiError::new("invalid PIN"))));
    }
    drop(guard);
    record_pin_success(state).await;
    Ok(pin_str.to_string())
}

fn pin_lockout_error(retry_after_secs: u64) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ApiError::new(format!(
            "too many invalid PIN attempts; retry in {retry_after_secs} seconds"
        ))),
    )
}

async fn ensure_pin_attempt_allowed(state: &AppState) -> Result<(), (StatusCode, Json<ApiError>)> {
    let mut attempts = state.runtime.pin_attempts.lock().await;
    if let Some(remaining) = attempts.locked_remaining() {
        return Err(pin_lockout_error(remaining.as_secs().max(1)));
    }
    Ok(())
}

async fn record_pin_success(state: &AppState) {
    state.runtime.pin_attempts.lock().await.record_success();
}

async fn record_pin_failure(state: &AppState) -> Option<u64> {
    let max_attempts = state.config.server.max_pin_attempts;
    let lockout = std::time::Duration::from_secs(state.config.server.lockout_secs);
    state
        .runtime
        .pin_attempts
        .lock()
        .await
        .record_failure(max_attempts, lockout)
        .map(|duration| duration.as_secs().max(1))
}

fn auth_response(
    status: StatusCode,
    token: &str,
    expires_at: u64,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let max_age_secs = expires_at.saturating_sub(unix_timestamp());
    let cookie = HeaderValue::from_str(&session_cookie(token, max_age_secs)).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("session cookie header: {error}"))),
        )
    })?;
    let mut response = (
        status,
        Json(AuthResponse {
            token: String::new(),
            expires_at,
        }),
    )
        .into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(response)
}

async fn token_expires(state: &AppState, token: &str) -> u64 {
    state
        .runtime
        .sessions
        .validate(token)
        .await
        .map(|s| s.expires_at)
        .unwrap_or(0)
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
