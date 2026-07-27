//! OIDC Device Authorization Grant (RFC 8628) client for the Redis Cloud Okta tenant.
//!
//! Two requests against the issuer:
//! - `POST {issuer}/v1/device/authorize` — start; returns the user code + verification URI.
//! - `POST {issuer}/v1/token` (device-code grant) — polled until approved/expired/denied.
//!
//! The caller owns the polling loop (so the CLI can print progress and honor `--timeout`);
//! [`DeviceFlowClient::poll_once`] performs a single attempt. Refreshing a token is
//! flow-agnostic and lives in [`super::oidc::refresh`].

use serde::Deserialize;
use url::Url;

use super::{
    AuthError, TokenSet, default_http_client, endpoint, form_body, post_token, token_set, truncate,
};

const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// RFC 8628 default poll interval when the server omits one.
const DEFAULT_INTERVAL: u64 = 5;

/// Device-authorization-grant client bound to one Okta issuer + public client id.
#[derive(Clone)]
pub struct DeviceFlowClient {
    issuer: Url,
    client_id: String,
    http: reqwest::Client,
}

/// The device-authorization response — what `auth login` surfaces to the developer.
///
/// `device_code` is retained to poll the token endpoint; it is a secret and is never printed.
#[derive(Clone)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    /// The page the user opens and then *types* the `user_code` into.
    pub verification_uri: String,
    /// The same page with the `user_code` pre-embedded (optional per RFC 8628), so the user
    /// can just open it and confirm — no manual code entry. Prefer this when present.
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

/// Outcome of a single token-endpoint poll. The caller decides when to poll again.
#[derive(Debug)]
pub enum PollOutcome {
    /// Still waiting on the user; sleep `interval` and poll again.
    Pending,
    /// Server asked us to back off; increase the interval, then keep polling.
    SlowDown,
    /// Approved — tokens issued.
    Ready(TokenSet),
}

/// Raw `/device/authorize` wire response. Kept separate from the public [`DeviceAuthorization`]
/// so the wire shape (`interval` optional per RFC 8628) stays isolated from the resolved public
/// type (`interval` defaulted); `start()` maps one to the other.
#[derive(Deserialize)]
struct DeviceAuthzResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

impl DeviceFlowClient {
    /// Build a client for the given issuer (e.g.
    /// `https://<your-okta-issuer>/oauth2/default`) and public client id.
    pub fn new(issuer: Url, client_id: impl Into<String>) -> Self {
        Self {
            issuer,
            client_id: client_id.into(),
            http: default_http_client(),
        }
    }

    /// Use a caller-provided reqwest client (tests / shared client / custom user agent).
    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    /// Start device authorization: `POST /v1/device/authorize`.
    pub async fn start(&self, scopes: &[&str]) -> Result<DeviceAuthorization, AuthError> {
        let scope = scopes.join(" ");
        let resp = self
            .http
            .post(endpoint(&self.issuer, "v1/device/authorize"))
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form_body(&[
                ("client_id", self.client_id.as_str()),
                ("scope", scope.as_str()),
            ]))
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(AuthError::Protocol(format!(
                "device authorize returned {status}: {}",
                truncate(&body)
            )));
        }
        let d: DeviceAuthzResponse = serde_json::from_str(&body).map_err(|e| {
            AuthError::Protocol(format!("could not parse device-authorize response: {e}"))
        })?;
        Ok(DeviceAuthorization {
            device_code: d.device_code,
            user_code: d.user_code,
            verification_uri: d.verification_uri,
            verification_uri_complete: d.verification_uri_complete,
            expires_in: d.expires_in,
            interval: d.interval.unwrap_or(DEFAULT_INTERVAL),
        })
    }

    /// One poll of the token endpoint with the device-code grant. The caller owns the loop.
    ///
    /// The OAuth device flow returns HTTP 400 with an `error` body for the pending/slow-down/
    /// error cases, so we read and parse the body regardless of status.
    pub async fn poll_once(&self, device_code: &str) -> Result<PollOutcome, AuthError> {
        let tr = post_token(
            &self.http,
            &endpoint(&self.issuer, "v1/token"),
            &[
                ("client_id", self.client_id.as_str()),
                ("grant_type", DEVICE_CODE_GRANT),
                ("device_code", device_code),
            ],
        )
        .await?;

        if let Some(err) = tr.error.as_deref() {
            return match err {
                "authorization_pending" => Ok(PollOutcome::Pending),
                "slow_down" => Ok(PollOutcome::SlowDown),
                "expired_token" => Err(AuthError::Expired),
                "access_denied" => Err(AuthError::Denied),
                other => Err(AuthError::Protocol(format!(
                    "token endpoint error {other}: {}",
                    tr.description()
                ))),
            };
        }
        Ok(PollOutcome::Ready(token_set(tr)?))
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
        Mock::given(method("POST"))
            .and(path("/v1/device/authorize"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "DC",
                "user_code": "WDJB-MJHT",
                "verification_uri": "https://x/activate",
                "verification_uri_complete": "https://x/activate?user_code=WDJB-MJHT",
                "expires_in": 600,
                "interval": 5
            })))
            .mount(&server)
            .await;

        let d = client(&server)
            .start(&["openid", "offline_access"])
            .await
            .unwrap();
        assert_eq!(d.device_code, "DC");
        assert_eq!(d.user_code, "WDJB-MJHT");
        assert_eq!(d.interval, 5);
        assert_eq!(
            d.verification_uri_complete.as_deref(),
            Some("https://x/activate?user_code=WDJB-MJHT")
        );
    }

    #[tokio::test]
    async fn start_defaults_interval_when_absent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/device/authorize"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "DC",
                "user_code": "U",
                "verification_uri": "https://x",
                "expires_in": 600
            })))
            .mount(&server)
            .await;

        let d = client(&server).start(&["openid"]).await.unwrap();
        assert_eq!(d.interval, DEFAULT_INTERVAL);
    }

    #[tokio::test]
    async fn poll_pending() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            400,
            serde_json::json!({"error": "authorization_pending"}),
        )
        .await;
        assert!(matches!(
            client(&server).poll_once("DC").await.unwrap(),
            PollOutcome::Pending
        ));
    }

    #[tokio::test]
    async fn poll_slow_down() {
        let server = MockServer::start().await;
        mount_token(&server, 400, serde_json::json!({"error": "slow_down"})).await;
        assert!(matches!(
            client(&server).poll_once("DC").await.unwrap(),
            PollOutcome::SlowDown
        ));
    }

    #[tokio::test]
    async fn poll_ready_returns_tokens() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            200,
            serde_json::json!({
                "access_token": "AT",
                "id_token": "IT",
                "refresh_token": "RT",
                "expires_in": 3600
            }),
        )
        .await;

        match client(&server).poll_once("DC").await.unwrap() {
            PollOutcome::Ready(t) => {
                assert_eq!(t.access_token, "AT");
                assert_eq!(t.id_token, "IT");
                assert_eq!(t.refresh_token.as_deref(), Some("RT"));
                assert_eq!(t.expires_in, 3600);
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn poll_expired_maps_to_error() {
        let server = MockServer::start().await;
        mount_token(&server, 400, serde_json::json!({"error": "expired_token"})).await;
        assert!(matches!(
            client(&server).poll_once("DC").await,
            Err(AuthError::Expired)
        ));
    }

    #[tokio::test]
    async fn poll_denied_maps_to_error() {
        let server = MockServer::start().await;
        mount_token(&server, 400, serde_json::json!({"error": "access_denied"})).await;
        assert!(matches!(
            client(&server).poll_once("DC").await,
            Err(AuthError::Denied)
        ));
    }

    #[tokio::test]
    async fn poll_unknown_error_is_protocol() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            400,
            serde_json::json!({"error": "weird", "error_description": "nope"}),
        )
        .await;
        assert!(matches!(
            client(&server).poll_once("DC").await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn poll_malformed_body_is_protocol() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        assert!(matches!(
            client(&server).poll_once("DC").await,
            Err(AuthError::Protocol(_))
        ));
    }
}
