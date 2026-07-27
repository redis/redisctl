//! Shared OIDC vocabulary and token-endpoint plumbing used by both login flows.
//!
//! Holds the public [`TokenSet`] / [`AuthError`] types plus the crate-private helpers the
//! device-flow and loopback clients build on. Kept out of `mod.rs` so that stays a thin
//! facade, matching the other modules in this crate.

use serde::Deserialize;
use thiserror::Error;
use url::Url;

/// OIDC tokens returned by any login flow (device flow or auth-code loopback).
///
/// `Debug` is hand-written so token material never lands in logs, panics, or `{:?}` output.
#[derive(Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub id_token: String,
    pub refresh_token: Option<String>,
    /// Access-token lifetime in seconds (0 if the IdP omitted it).
    pub expires_in: u64,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field("id_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

/// Errors from the OIDC token-acquisition flows.
///
/// Exit-code mapping is applied at the CLI layer in the error-contract work unit;
/// here we only classify the failure.
#[derive(Debug, Error)]
pub enum AuthError {
    /// The device/authorization code expired before the user approved (`expired_token`).
    #[error("the login code expired before it was approved; start login again")]
    Expired,

    /// The user denied the authorization request (`access_denied`).
    #[error("the login request was denied")]
    Denied,

    /// Network/transport failure talking to the identity provider.
    #[error("network error contacting the identity provider: {0}")]
    Network(#[from] reqwest::Error),

    /// The identity provider returned something unexpected or unparseable.
    #[error("unexpected identity-provider response: {0}")]
    Protocol(String),
}

/// Raw token-endpoint response — either the token fields or an OAuth `error` object.
#[derive(Deserialize)]
pub(crate) struct TokenResponse {
    pub(crate) access_token: Option<String>,
    pub(crate) id_token: Option<String>,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_in: Option<u64>,
    pub(crate) error: Option<String>,
    pub(crate) error_description: Option<String>,
}

impl TokenResponse {
    /// The `error_description`, or a placeholder — for building error messages.
    pub(crate) fn description(&self) -> &str {
        self.error_description
            .as_deref()
            .unwrap_or("(no description)")
    }
}

/// Build `{issuer}/{path}`, tolerant of a trailing slash on the issuer.
pub(crate) fn endpoint(issuer: &Url, path: &str) -> String {
    format!(
        "{}/{}",
        issuer.as_str().trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Encode name/value pairs as an `application/x-www-form-urlencoded` string (body or query).
pub(crate) fn form_body(pairs: &[(&str, &str)]) -> String {
    serde_urlencoded::to_string(pairs).unwrap_or_default()
}

/// POST an `application/x-www-form-urlencoded` body to the token endpoint and parse the
/// response. Does not treat non-2xx as fatal — the OAuth error body is parsed regardless.
pub(crate) async fn post_token(
    http: &reqwest::Client,
    token_url: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse, AuthError> {
    let body = http
        .post(token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body(form))
        .send()
        .await?
        .text()
        .await?;
    serde_json::from_str(&body)
        .map_err(|e| AuthError::Protocol(format!("could not parse token response: {e}")))
}

/// Convert a successful token response into a [`TokenSet`].
pub(crate) fn token_set(tr: TokenResponse) -> Result<TokenSet, AuthError> {
    let access_token = tr
        .access_token
        .ok_or_else(|| AuthError::Protocol("token response missing access_token".into()))?;
    Ok(TokenSet {
        access_token,
        id_token: tr.id_token.unwrap_or_default(),
        refresh_token: tr.refresh_token,
        expires_in: tr.expires_in.unwrap_or(0),
    })
}

/// Exchange a refresh token for a fresh [`TokenSet`] via the refresh-token grant.
///
/// Okta rotates the refresh token, so the caller must persist the new one. The grant is
/// flow-agnostic (a token from either login flow refreshes identically), so it lives here
/// rather than on a specific flow client.
pub(crate) async fn refresh(
    http: &reqwest::Client,
    issuer: &Url,
    client_id: &str,
    refresh_token: &str,
) -> Result<TokenSet, AuthError> {
    let tr = post_token(
        http,
        &endpoint(issuer, "v1/token"),
        &[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
    .await?;
    if let Some(err) = tr.error.as_deref() {
        return Err(AuthError::Protocol(format!(
            "refresh failed ({err}): {}",
            tr.description()
        )));
    }
    token_set(tr)
}

/// A reqwest client with the redisctl user agent.
pub(crate) fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("redisctl/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("building the reqwest client should not fail")
}

/// Truncate a string for inclusion in an error message (char-boundary safe).
pub(crate) fn truncate(s: &str) -> String {
    const MAX: usize = 200;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let head: String = s.chars().take(MAX).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mount_token(server: &MockServer, status: u16, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn refresh_returns_rotated_token() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            200,
            serde_json::json!({
                "access_token": "AT2",
                "refresh_token": "RT2",
                "expires_in": 3600
            }),
        )
        .await;

        let issuer = Url::parse(&server.uri()).unwrap();
        let t = refresh(&default_http_client(), &issuer, "test-client", "RT1")
            .await
            .unwrap();
        assert_eq!(t.access_token, "AT2");
        // Okta rotates the refresh token — the caller must persist the new one.
        assert_eq!(t.refresh_token.as_deref(), Some("RT2"));
    }

    #[tokio::test]
    async fn refresh_error_is_protocol() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            400,
            serde_json::json!({"error": "invalid_grant", "error_description": "expired"}),
        )
        .await;
        let issuer = Url::parse(&server.uri()).unwrap();
        assert!(matches!(
            refresh(&default_http_client(), &issuer, "test-client", "RT1").await,
            Err(AuthError::Protocol(_))
        ));
    }
}
