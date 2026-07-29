//! OIDC Device Authorization Grant (RFC 8628) client for the Redis Cloud Okta tenant.
//!
//! Two requests against the issuer, handled by the [`oauth2`] crate:
//! - `POST {issuer}/v1/device/authorize` — [`start`](DeviceFlowClient::start); returns the user
//!   code + verification URI plus the (secret) device code needed to resume.
//! - `POST {issuer}/v1/token` (device-code grant) — [`poll`](DeviceFlowClient::poll); the crate
//!   owns the polling loop (honoring the server's `interval`/`slow_down`) until the request is
//!   approved, denied, or times out.
//!
//! `login --device` is deliberately split: the CLI calls [`start`](DeviceFlowClient::start),
//! prints/persists the returned [`DeviceAuthorization`] (which is `serde`-serializable), and
//! returns. A later `status --wait` — possibly a *different* process — rebuilds the client from
//! the issuer + client id, deserializes the [`DeviceAuthorization`], and calls
//! [`poll`](DeviceFlowClient::poll). Refreshing a token is flow-agnostic and lives in
//! [`super::oidc::refresh`].

use std::time::Duration;

use oauth2::{Scope, StandardDeviceAuthorizationResponse};
use serde::{Deserialize, Serialize};
use url::Url;

use super::oidc::{
    map_basic_token_error, map_device_token_error, oauth_http_client, okta_client, to_token_set,
};
use super::{AuthError, TokenSet};

/// Device-authorization-grant client bound to one Okta issuer + public client id.
///
/// Cheap to construct and holds no network state, so `status --wait` can rebuild it from the
/// persisted issuer + client id to resume a login started by an earlier `login --device`.
#[derive(Clone)]
pub struct DeviceFlowClient {
    issuer: Url,
    client_id: String,
}

/// The device-authorization response — what `auth login --device` surfaces to the developer and
/// persists so a later poll (in any process) can resume.
///
/// Wraps the RFC 8628 response from the [`oauth2`] crate. It is `serde`-serializable so the CLI
/// can persist it between the non-blocking `login --device` and a later `status --wait`. The
/// `device_code` it carries is a secret; `Debug` redacts it (as do the other code fields) via
/// the underlying `oauth2` secret types.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceAuthorization {
    inner: StandardDeviceAuthorizationResponse,
}

impl DeviceAuthorization {
    /// The end-user verification code (shown to the user to type at the verification URI).
    pub fn user_code(&self) -> &str {
        self.inner.user_code().secret().as_str()
    }

    /// The page the user opens and then *types* the `user_code` into.
    pub fn verification_uri(&self) -> &str {
        self.inner.verification_uri().as_str()
    }

    /// The same page with the `user_code` pre-embedded (optional per RFC 8628), so the user can
    /// just open it and confirm — no manual code entry. Prefer this when present.
    pub fn verification_uri_complete(&self) -> Option<&str> {
        self.inner
            .verification_uri_complete()
            .map(|v| v.secret().as_str())
    }

    /// Lifetime of the device/user code, in seconds.
    pub fn expires_in(&self) -> u64 {
        self.inner.expires_in().as_secs()
    }

    /// Minimum poll interval requested by the server, in seconds.
    pub fn interval(&self) -> u64 {
        self.inner.interval().as_secs()
    }

    /// Borrow the underlying RFC 8628 response (escape hatch for callers that need the raw type).
    pub fn as_standard(&self) -> &StandardDeviceAuthorizationResponse {
        &self.inner
    }
}

impl DeviceFlowClient {
    /// Build a client for the given issuer (e.g.
    /// `https://<your-okta-issuer>/oauth2/default`) and public client id.
    pub fn new(issuer: Url, client_id: impl Into<String>) -> Self {
        Self {
            issuer,
            client_id: client_id.into(),
        }
    }

    /// Start device authorization: `POST /v1/device/authorize`.
    ///
    /// Returns the codes to display *and* the device code needed to resume polling — persist the
    /// returned [`DeviceAuthorization`] and hand it to [`poll`](Self::poll) later.
    pub async fn start(&self, scopes: &[&str]) -> Result<DeviceAuthorization, AuthError> {
        let client = okta_client(&self.issuer, &self.client_id)?;
        let http = oauth_http_client()?;
        let mut request = client.exchange_device_code();
        for scope in scopes {
            request = request.add_scope(Scope::new((*scope).to_string()));
        }
        let inner: StandardDeviceAuthorizationResponse = request
            .request_async(&http)
            .await
            .map_err(map_basic_token_error)?;
        Ok(DeviceAuthorization { inner })
    }

    /// Poll the token endpoint until the user approves, the code is denied/expires, or `timeout`
    /// elapses. The [`oauth2`] crate runs the loop internally (respecting the server's poll
    /// interval and `slow_down`), so this is a single blocking call.
    ///
    /// `timeout` bounds the whole wait; `None` falls back to the device code's own lifetime. A
    /// timeout surfaces as [`AuthError::Expired`]. Callable from a freshly built client after
    /// deserializing `authz`.
    pub async fn poll(
        &self,
        authz: &DeviceAuthorization,
        timeout: Option<Duration>,
    ) -> Result<TokenSet, AuthError> {
        let client = okta_client(&self.issuer, &self.client_id)?;
        let http = oauth_http_client()?;
        let resp = client
            .exchange_device_access_token(&authz.inner)
            .request_async(&http, tokio::time::sleep, timeout)
            .await
            .map_err(map_device_token_error)?;
        Ok(to_token_set(&resp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> DeviceFlowClient {
        DeviceFlowClient::new(Url::parse(&server.uri()).unwrap(), "test-client")
    }

    async fn mount_device_authorize(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/v1/device/authorize"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn mount_token(server: &MockServer, status: u16, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn start_parses_device_authorization() {
        let server = MockServer::start().await;
        mount_device_authorize(
            &server,
            serde_json::json!({
                "device_code": "DC",
                "user_code": "WDJB-MJHT",
                "verification_uri": "https://x/activate",
                "verification_uri_complete": "https://x/activate?user_code=WDJB-MJHT",
                "expires_in": 600,
                "interval": 5
            }),
        )
        .await;

        let d = client(&server)
            .start(&["openid", "offline_access"])
            .await
            .unwrap();
        assert_eq!(d.user_code(), "WDJB-MJHT");
        assert_eq!(d.verification_uri(), "https://x/activate");
        assert_eq!(
            d.verification_uri_complete(),
            Some("https://x/activate?user_code=WDJB-MJHT")
        );
        assert_eq!(d.expires_in(), 600);
        assert_eq!(d.interval(), 5);
        // Debug must not leak the secret device code / user code.
        assert!(!format!("{d:?}").contains("WDJB-MJHT"));
    }

    #[tokio::test]
    async fn start_defaults_interval_when_absent() {
        let server = MockServer::start().await;
        mount_device_authorize(
            &server,
            serde_json::json!({
                "device_code": "DC",
                "user_code": "U",
                "verification_uri": "https://x",
                "expires_in": 600
            }),
        )
        .await;

        // RFC 8628 default poll interval when the server omits one.
        let d = client(&server).start(&["openid"]).await.unwrap();
        assert_eq!(d.interval(), 5);
    }

    #[tokio::test]
    async fn poll_ready_returns_tokens() {
        let server = MockServer::start().await;
        mount_device_authorize(
            &server,
            serde_json::json!({
                "device_code": "DC", "user_code": "U",
                "verification_uri": "https://x", "expires_in": 600, "interval": 5
            }),
        )
        .await;
        mount_token(
            &server,
            200,
            serde_json::json!({
                "access_token": "AT",
                "token_type": "Bearer",
                "refresh_token": "RT",
                "expires_in": 3600
            }),
        )
        .await;

        let c = client(&server);
        let authz = c.start(&["openid"]).await.unwrap();
        let t = c.poll(&authz, Some(Duration::from_secs(5))).await.unwrap();
        assert_eq!(t.access_token, "AT");
        assert_eq!(t.refresh_token.as_deref(), Some("RT"));
        assert_eq!(t.expires_in, 3600);
    }

    #[tokio::test]
    async fn poll_expired_maps_to_error() {
        let server = MockServer::start().await;
        mount_device_authorize(
            &server,
            serde_json::json!({
                "device_code": "DC", "user_code": "U",
                "verification_uri": "https://x", "expires_in": 600, "interval": 5
            }),
        )
        .await;
        mount_token(&server, 400, serde_json::json!({"error": "expired_token"})).await;

        let c = client(&server);
        let authz = c.start(&["openid"]).await.unwrap();
        assert!(matches!(
            c.poll(&authz, Some(Duration::from_secs(5))).await,
            Err(AuthError::Expired)
        ));
    }

    #[tokio::test]
    async fn poll_denied_maps_to_error() {
        let server = MockServer::start().await;
        mount_device_authorize(
            &server,
            serde_json::json!({
                "device_code": "DC", "user_code": "U",
                "verification_uri": "https://x", "expires_in": 600, "interval": 5
            }),
        )
        .await;
        mount_token(&server, 400, serde_json::json!({"error": "access_denied"})).await;

        let c = client(&server);
        let authz = c.start(&["openid"]).await.unwrap();
        assert!(matches!(
            c.poll(&authz, Some(Duration::from_secs(5))).await,
            Err(AuthError::Denied)
        ));
    }

    /// The device authorization must survive a serialize/deserialize round-trip and still be
    /// usable to poll — this is the `login --device` (persist) then `status --wait` (resume,
    /// possibly in another process) contract.
    #[tokio::test]
    async fn device_authorization_round_trips_and_resumes() {
        let server = MockServer::start().await;
        mount_device_authorize(
            &server,
            serde_json::json!({
                "device_code": "DC", "user_code": "U",
                "verification_uri": "https://x", "expires_in": 600, "interval": 5
            }),
        )
        .await;
        mount_token(
            &server,
            200,
            serde_json::json!({"access_token": "AT", "token_type": "Bearer", "expires_in": 3600}),
        )
        .await;

        let started = client(&server).start(&["openid"]).await.unwrap();
        let json = serde_json::to_string(&started).unwrap();
        let resumed: DeviceAuthorization = serde_json::from_str(&json).unwrap();

        // A freshly built client (as a separate process would build) can complete the poll.
        let fresh = DeviceFlowClient::new(Url::parse(&server.uri()).unwrap(), "test-client");
        let t = fresh
            .poll(&resumed, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        assert_eq!(t.access_token, "AT");
    }
}
