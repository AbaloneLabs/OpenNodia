//! Shared HTTP API error helpers.

use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::intent::IntentStoreError;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

impl ApiError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { error: msg.into() }
    }
}

pub(crate) type ApiErrorResponse = (StatusCode, Json<ApiError>);
pub(crate) type ApiResult<T> = Result<T, ApiErrorResponse>;

pub(crate) fn bad_request(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::BAD_REQUEST, Json(ApiError::new(msg)))
}

pub(crate) fn not_found(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::NOT_FOUND, Json(ApiError::new(msg)))
}

pub(crate) fn service_unavailable(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::SERVICE_UNAVAILABLE, Json(ApiError::new(msg)))
}

pub(crate) fn internal(msg: impl Into<String>) -> ApiErrorResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(msg)))
}

pub(crate) fn intent_store_error(error: IntentStoreError, label: &str) -> ApiErrorResponse {
    match error {
        IntentStoreError::InvalidId => bad_request(format!("invalid {label} intent id")),
        IntentStoreError::Missing => bad_request(format!(
            "{label} intent is missing, expired, or already used"
        )),
        IntentStoreError::OwnerMismatch => bad_request(format!(
            "{label} intent does not belong to this session and wallet"
        )),
        IntentStoreError::Capacity => service_unavailable(format!(
            "too many pending {label} intents; retry after existing intents expire"
        )),
    }
}
