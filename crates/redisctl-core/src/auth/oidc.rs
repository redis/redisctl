//! Shared OIDC vocabulary and token-endpoint plumbing used by both login flows.
//!
//! Holds the public [`TokenSet`] / [`AuthError`] types plus the crate-private helpers the
//! device-flow and loopback clients build on. The OAuth2/OIDC protocol itself is handled by
//! the maintained [`oauth2`] crate; this module owns the endpoint layout (Okta `v1/*` paths),
//! the shared HTTP clients, and the mapping from `oauth2` errors to [`AuthError`]. Kept out of
//! `mod.rs` so that stays a thin facade, matching the other modules in this crate.

use oauth2::basic::{BasicClient, BasicRequestTokenError, BasicTokenResponse};
use oauth2::{
    AuthType, AuthUrl, ClientId, DeviceAuthorizationUrl, DeviceCodeErrorResponse,
    DeviceCodeErrorResponseType, EndpointNotSet, EndpointSet, RefreshToken, RequestTokenError,
    TokenResponse, TokenUrl,
};
use thiserror::Error;
use url::Url;

/// OIDC tokens returned by any login flow (device flow or auth-code loopback).
///
/// `Debug` is hand-written so token material never lands in logs, panics, or `{:?}` output.
#[derive(Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Access-token lifetime in seconds (0 if the IdP omitted it).
    pub expires_in: u64,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
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

    /// Network/transport failure talking to the SM API (the `oauth2` flows classify their own
    /// transport failures as [`AuthError::Protocol`] because they run on a separate HTTP stack).
    #[error("network error contacting the identity provider: {0}")]
    Network(#[from] reqwest::Error),

    /// The identity provider returned something unexpected or unparseable.
    #[error("unexpected identity-provider response: {0}")]
    Protocol(String),

    /// The Redis Cloud account still authenticates with a password and has not been linked to a
    /// social/SSO identity, so the token exchange cannot complete. Linking is a one-time step the
    /// user performs in the Redis Cloud console.
    #[error(
        "this Redis Cloud account must be linked to social sign-in once before the CLI can use it"
    )]
    MigrationRequired,

    /// Programmatic (CAPI) access is off for the account and only its owner may enable it, so the
    /// login cannot mint a key. A one-time console step for the owner, not a retryable failure.
    #[error(
        "enabling programmatic access requires the Redis Cloud account owner; ask them to enable \
         it once in the console, then run login again"
    )]
    NotAccountOwner,

    /// `--account` named an account the signed-in user does not belong to. Carries what they do
    /// have, so the caller can list the options instead of just refusing.
    #[error("account {requested} is not one of yours; you belong to: {available}")]
    UnknownAccount { requested: u64, available: String },

    /// SM challenged the login for multi-factor authentication (`user-mfa-required`). Carries the
    /// factor types SM offered, when it reports them.
    #[error("this account requires multi-factor authentication")]
    MfaRequired { factors: Vec<String> },

    /// The submitted MFA code was rejected (`mfa-invalid-code`).
    #[error("the multi-factor code was not accepted")]
    MfaInvalidCode,

    /// Too many MFA attempts (`mfa-quota-exceeded`); retrying now will not help.
    #[error("too many multi-factor attempts; wait before trying again")]
    MfaQuotaExceeded,
}

/// A `BasicClient` with the Okta authorize / token / device-authorization endpoints set. The
/// remaining typestate slots (introspection, revocation) stay unset — we never call those.
pub(crate) type OktaClient =
    BasicClient<EndpointSet, EndpointSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

/// Build `{issuer}/{path}`, tolerant of a trailing slash on the issuer.
pub(crate) fn endpoint(issuer: &Url, path: &str) -> String {
    format!(
        "{}/{}",
        issuer.as_str().trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Configure an [`oauth2`] client for the Okta tenant behind `issuer` as a public client
/// (no secret; credentials go in the request body via [`AuthType::RequestBody`]).
///
/// Okta's endpoints are derived from the issuer: `v1/authorize`, `v1/token`,
/// `v1/device/authorize`. The loopback flow additionally calls `set_redirect_uri` on the
/// returned client once it has bound a port.
pub(crate) fn okta_client(issuer: &Url, client_id: &str) -> Result<OktaClient, AuthError> {
    let auth = AuthUrl::new(endpoint(issuer, "v1/authorize"))
        .map_err(|e| AuthError::Protocol(format!("invalid authorize URL: {e}")))?;
    let token = TokenUrl::new(endpoint(issuer, "v1/token"))
        .map_err(|e| AuthError::Protocol(format!("invalid token URL: {e}")))?;
    let device = DeviceAuthorizationUrl::new(endpoint(issuer, "v1/device/authorize"))
        .map_err(|e| AuthError::Protocol(format!("invalid device-authorization URL: {e}")))?;
    Ok(BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(auth)
        .set_token_uri(token)
        .set_device_authorization_url(device)
        .set_auth_type(AuthType::RequestBody))
}

/// HTTP client for the [`oauth2`] flows. Redirects are disabled so the token/authorize
/// requests can never be silently bounced to another host.
pub(crate) fn oauth_http_client() -> Result<oauth2::reqwest::Client, AuthError> {
    oauth2::reqwest::Client::builder()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .user_agent(crate::USER_AGENT)
        .build()
        .map_err(|e| AuthError::Protocol(format!("could not build the OAuth HTTP client: {e}")))
}

/// A reqwest client with the redisctl user agent (used by the SM API exchange).
pub(crate) fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .build()
        .expect("building the reqwest client should not fail")
}

/// Convert a successful [`oauth2`] token response into a [`TokenSet`].
pub(crate) fn to_token_set(resp: &BasicTokenResponse) -> TokenSet {
    TokenSet {
        access_token: resp.access_token().secret().clone(),
        refresh_token: resp.refresh_token().map(|r| r.secret().clone()),
        expires_in: resp.expires_in().map(|d| d.as_secs()).unwrap_or(0),
    }
}

/// Map an `oauth2` token/authorize error (with the *basic* error body) to an [`AuthError`].
///
/// Used by the auth-code, refresh, and device-authorize requests. Transport failures land in
/// [`AuthError::Protocol`] because the `oauth2` flows run on `oauth2`'s bundled reqwest, whose
/// error type differs from the one wrapped by [`AuthError::Network`].
pub(crate) fn map_basic_token_error<RE>(err: BasicRequestTokenError<RE>) -> AuthError
where
    RE: std::error::Error,
{
    match err {
        RequestTokenError::ServerResponse(resp) => match resp.error().as_ref() {
            "access_denied" => AuthError::Denied,
            "expired_token" => AuthError::Expired,
            _ => AuthError::Protocol(format!("identity-provider error: {resp}")),
        },
        other => AuthError::Protocol(other.to_string()),
    }
}

/// Map an `oauth2` device-access-token error to an [`AuthError`]. The device grant has its own
/// error vocabulary (`authorization_pending` / `slow_down` are handled inside the crate's poll
/// loop, so only the terminal outcomes reach here).
pub(crate) fn map_device_token_error<RE>(
    err: RequestTokenError<RE, DeviceCodeErrorResponse>,
) -> AuthError
where
    RE: std::error::Error,
{
    match err {
        RequestTokenError::ServerResponse(resp) => match resp.error() {
            DeviceCodeErrorResponseType::ExpiredToken => AuthError::Expired,
            DeviceCodeErrorResponseType::AccessDenied => AuthError::Denied,
            _ => AuthError::Protocol(format!("identity-provider error: {resp}")),
        },
        other => AuthError::Protocol(other.to_string()),
    }
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

/// Exchange a refresh token for a fresh [`TokenSet`] via the refresh-token grant.
///
/// Okta rotates the refresh token, so the caller must persist the new one. The grant is
/// flow-agnostic (a token from either login flow refreshes identically), so it lives here
/// rather than on a specific flow client.
pub(crate) async fn refresh(
    issuer: &Url,
    client_id: &str,
    refresh_token: &str,
) -> Result<TokenSet, AuthError> {
    let client = okta_client(issuer, client_id)?;
    let http = oauth_http_client()?;
    let resp = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
        .request_async(&http)
        .await
        .map_err(map_basic_token_error)?;
    Ok(to_token_set(&resp))
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
                "token_type": "Bearer",
                "refresh_token": "RT2",
                "expires_in": 3600
            }),
        )
        .await;

        let issuer = Url::parse(&server.uri()).unwrap();
        let t = refresh(&issuer, "test-client", "RT1").await.unwrap();
        assert_eq!(t.access_token, "AT2");
        // Okta rotates the refresh token — the caller must persist the new one.
        assert_eq!(t.refresh_token.as_deref(), Some("RT2"));
        assert_eq!(t.expires_in, 3600);
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
            refresh(&issuer, "test-client", "RT1").await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[test]
    fn token_set_debug_redacts_secrets() {
        let t = TokenSet {
            access_token: "AT-should-not-appear".into(),
            refresh_token: Some("RT-should-not-appear".into()),
            expires_in: 3600,
        };
        let dbg = format!("{t:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(!dbg.contains("AT-should-not-appear"));
        assert!(!dbg.contains("RT-should-not-appear"));
        assert!(dbg.contains("3600"));
    }
}
