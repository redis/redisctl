//! Cloud authentication: OIDC flows that bootstrap a Redis Cloud CAPI key.
//!
//! `cloud auth login` is a credential *bootstrapper*: obtain Okta tokens, exchange them with
//! the SM API, mint a CAPI key, and hand it to the config layer to persist as a normal cloud
//! profile. This module holds the OIDC token-acquisition clients plus the SM exchange.
//!
//! Two front doors, one back end:
//! - [`device_flow::DeviceFlowClient`] — device authorization grant (headless / agent).
//! - [`auth_code_loopback::LoopbackFlowClient`] — auth-code + PKCE via a loopback redirect
//!   (interactive human login).
//!
//! Both yield a [`TokenSet`] and share the token-endpoint plumbing below. No JWT/JWKS
//! validation happens here — the SM API is the verifier of the token downstream.

pub mod auth_code_loopback;
pub mod authenticator;
pub mod device_flow;
pub mod sm_api;

pub use auth_code_loopback::LoopbackFlowClient;
pub use authenticator::{CloudAuthenticator, MintedCredentials};
pub use device_flow::{DeviceAuthorization, DeviceFlowClient, PollOutcome};
pub use sm_api::{CapiKey, SmAccount, SmApiClient, SmUser};

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

// ---------------------------------------------------------------------------
// Shared OIDC plumbing used by both flows.
// ---------------------------------------------------------------------------

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
