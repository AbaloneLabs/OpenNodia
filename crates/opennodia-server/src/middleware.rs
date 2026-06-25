//! HTTP middleware for authentication and security headers.

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::session::{SessionToken, SESSION_COOKIE_NAME};
use crate::state::AppState;

const CSRF_HEADER: &str = "x-opennodia-csrf";

/// Error response for auth failures.
pub struct AuthError(pub &'static str);

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": self.0 })),
        )
            .into_response()
    }
}

/// Middleware that requires a valid session token.
///
/// Browser requests use the HttpOnly session cookie. API clients may still use
/// `Authorization: Bearer <token>` for compatibility.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let method = req.method().clone();
    let auth = session_token(req.headers()).ok_or(AuthError("missing or invalid session"))?;

    if auth.source == TokenSource::Cookie && requires_csrf_header(&method) {
        require_csrf_header(req.headers())?;
    }

    let session = state
        .runtime
        .sessions
        .validate(&auth.token)
        .await
        .ok_or(AuthError("invalid or expired session"))?;

    tracing::debug!(sid = %session.sid, "auth ok");

    req.extensions_mut().insert(session);
    req.extensions_mut().insert(SessionToken(auth.token));
    Ok(next.run(req).await)
}

/// Add conservative security headers to every HTTP response.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; \
             connect-src 'self'; \
             base-uri 'none'; \
             frame-ancestors 'none'",
        ),
    );
    response
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenSource {
    Bearer,
    Cookie,
}

#[derive(Debug)]
struct AuthToken {
    token: String,
    source: TokenSource,
}

fn session_token(headers: &HeaderMap) -> Option<AuthToken> {
    bearer_token(headers)
        .map(|token| AuthToken {
            token,
            source: TokenSource::Bearer,
        })
        .or_else(|| {
            cookie_token(headers).map(|token| AuthToken {
                token,
                source: TokenSource::Cookie,
            })
        })
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::to_string)
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == SESSION_COOKIE_NAME && !value.is_empty()).then(|| value.to_string())
    })
}

fn requires_csrf_header(method: &Method) -> bool {
    !matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

fn require_csrf_header(headers: &HeaderMap) -> Result<(), AuthError> {
    match headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
    {
        Some("1") => Ok(()),
        _ => Err(AuthError("missing CSRF header")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_cookie_session_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; opennodia_session=abc.def; other=1"),
        );
        let token = session_token(&headers).expect("session token");
        assert_eq!(token.token, "abc.def");
        assert_eq!(token.source, TokenSource::Cookie);
    }

    #[test]
    fn bearer_session_token_takes_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer api-token"),
        );
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("opennodia_session=cookie-token"),
        );
        let token = session_token(&headers).expect("session token");
        assert_eq!(token.token, "api-token");
        assert_eq!(token.source, TokenSource::Bearer);
    }

    #[test]
    fn unsafe_cookie_requests_require_csrf_header() {
        assert!(requires_csrf_header(&Method::POST));
        assert!(!requires_csrf_header(&Method::GET));

        let headers = HeaderMap::new();
        assert!(require_csrf_header(&headers).is_err());

        let mut headers = HeaderMap::new();
        headers.insert(CSRF_HEADER, HeaderValue::from_static("1"));
        assert!(require_csrf_header(&headers).is_ok());
    }
}
