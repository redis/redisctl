//! SM API exchange: turn Okta tokens into a Redis Cloud CAPI key.
//!
//! Sequence:
//! 1. `POST /login` with `Authorization: Bearer <access_token>` → `Set-Cookie: JSESSIONID`.
//!    **No `Sm-Id-Token` header for Google/GitHub logins** — sending it drives a SAML
//!    account-mapping path that 400s; pass it only for SSO.
//! 2. `GET /csrf` → token nested at `csrfToken.csrf_token`; echoed back as `X-CSRF-Token`.
//! 3. `GET /users/me`, `GET /accounts` → resolve the user + account (+ account api key).
//! 4. `POST /accounts/cloud-api/cloudApiAccessKey` to enable CAPI — a `400
//!    account_api_key_already_exists` means it's already on (idempotent).
//! 5. `POST /accounts/cloud-api/cloudApiKeys` → mints the user secret key (`secret_key`).
//!
//! This workspace's `reqwest` has no cookie-store feature, so the `JSESSIONID` cookie and
//! CSRF token are carried manually on each request.

use serde::Deserialize;
use url::Url;

use super::{AuthError, default_http_client, endpoint, truncate};

/// SM API client. Stateless until [`SmApiClient::login`] establishes a session.
pub struct SmApiClient {
    base_url: Url,
    http: reqwest::Client,
    session: Option<Session>,
}

struct Session {
    /// Full cookie header value, e.g. `JSESSIONID=abc123`.
    cookie: String,
    csrf: String,
}

/// The authenticated user (`GET /users/me`), trimmed to what the bootstrap needs.
#[derive(Debug, Clone, Deserialize)]
pub struct SmUser {
    /// User id (string in the API); parse via [`SmUser::user_account`] for the mint call.
    pub id: String,
    #[serde(default)]
    pub current_account_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub product_type: Option<String>,
}

impl SmUser {
    /// The numeric user account id used as `user_account` when minting a CAPI key.
    pub fn user_account(&self) -> Result<u64, AuthError> {
        self.id.parse().map_err(|_| {
            AuthError::Protocol(format!("unexpected non-numeric user id {:?}", self.id))
        })
    }
}

/// An account (`GET /accounts`), trimmed to what the bootstrap needs.
#[derive(Debug, Clone, Deserialize)]
pub struct SmAccount {
    pub id: u64,
    #[serde(default)]
    pub name: Option<String>,
    /// The account-level CAPI access key (`x-api-key`), present once CAPI is enabled.
    #[serde(default)]
    pub api_access_key: Option<String>,
}

/// A minted CAPI user key. `secret_key` is redacted from `Debug`.
#[derive(Clone)]
pub struct CapiKey {
    pub name: String,
    pub secret_key: String,
}

impl std::fmt::Debug for CapiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapiKey")
            .field("name", &self.name)
            .field("secret_key", &"<redacted>")
            .finish()
    }
}

#[derive(Deserialize)]
struct CsrfEnvelope {
    #[serde(rename = "csrfToken")]
    token: CsrfToken,
}

#[derive(Deserialize)]
struct CsrfToken {
    csrf_token: String,
}

#[derive(Deserialize)]
struct AccountsEnvelope {
    #[serde(default)]
    accounts: Vec<SmAccount>,
}

impl SmApiClient {
    /// Build a client for the SM API base (e.g. `https://<sm-api-host>/api/v1`).
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            http: default_http_client(),
            session: None,
        }
    }

    /// Build with a caller-provided reqwest client (tests / shared client).
    pub fn with_http_client(base_url: Url, http: reqwest::Client) -> Self {
        Self {
            base_url,
            http,
            session: None,
        }
    }

    /// Establish a session: `POST /login` with the Okta access token, then fetch the CSRF
    /// token. Pass `sm_id_token` only for SSO logins (omit for Google/GitHub).
    pub async fn login(
        &mut self,
        access_token: &str,
        sm_id_token: Option<&str>,
    ) -> Result<(), AuthError> {
        let mut req = self
            .http
            .post(endpoint(&self.base_url, "login"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body("{}");
        if let Some(id) = sm_id_token {
            req = req.header("sm-id-token", id);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let cookie = extract_jsessionid(&resp);
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::Protocol(format!(
                "SM /login failed ({status}): {}",
                truncate(&body)
            )));
        }
        let cookie = cookie
            .ok_or_else(|| AuthError::Protocol("SM /login did not set a JSESSIONID".into()))?;
        let csrf = self.fetch_csrf(&cookie).await?;
        self.session = Some(Session {
            cookie: format!("JSESSIONID={cookie}"),
            csrf,
        });
        Ok(())
    }

    async fn fetch_csrf(&self, jsessionid: &str) -> Result<String, AuthError> {
        let body = self
            .http
            .get(endpoint(&self.base_url, "csrf"))
            .header(reqwest::header::COOKIE, format!("JSESSIONID={jsessionid}"))
            .send()
            .await?
            .text()
            .await?;
        let env: CsrfEnvelope = serde_json::from_str(&body)
            .map_err(|e| AuthError::Protocol(format!("could not parse /csrf response: {e}")))?;
        Ok(env.token.csrf_token)
    }

    /// `GET /users/me`.
    pub async fn fetch_current_user(&self) -> Result<SmUser, AuthError> {
        let body = self.authed_get("users/me").await?.text().await?;
        serde_json::from_str(&body)
            .map_err(|e| AuthError::Protocol(format!("could not parse /users/me: {e}")))
    }

    /// `GET /accounts`.
    pub async fn fetch_accounts(&self) -> Result<Vec<SmAccount>, AuthError> {
        let body = self.authed_get("accounts").await?.text().await?;
        let env: AccountsEnvelope = serde_json::from_str(&body)
            .map_err(|e| AuthError::Protocol(format!("could not parse /accounts: {e}")))?;
        Ok(env.accounts)
    }

    /// `POST /accounts/cloud-api/cloudApiAccessKey` — enable programmatic access. Idempotent:
    /// a `400 account_api_key_already_exists` is treated as success.
    pub async fn ensure_capi_enabled(&self) -> Result<(), AuthError> {
        let resp = self
            .authed_post_json(
                "accounts/cloud-api/cloudApiAccessKey",
                serde_json::json!({}),
            )
            .await?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if body.contains("account_api_key_already_exists") {
            return Ok(());
        }
        Err(AuthError::Protocol(format!(
            "enabling CAPI failed ({status}): {}",
            truncate(&body)
        )))
    }

    /// `POST /accounts/cloud-api/cloudApiKeys` — mint a named user secret key.
    pub async fn mint_capi_key(&self, name: &str, user_account: u64) -> Result<CapiKey, AuthError> {
        let body = self
            .authed_post_json(
                "accounts/cloud-api/cloudApiKeys",
                serde_json::json!({
                    "cloudApiKey": { "name": name, "user_account": user_account, "ip_whitelist": [] }
                }),
            )
            .await?
            .text()
            .await?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| AuthError::Protocol(format!("could not parse mint response: {e}")))?;
        // secret_key may be top-level or nested under `cloudApiKey`.
        let obj = value.get("cloudApiKey").unwrap_or(&value);
        let secret_key = obj
            .get("secret_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AuthError::Protocol("mint response missing secret_key".into()))?
            .to_string();
        let key_name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(name)
            .to_string();
        Ok(CapiKey {
            name: key_name,
            secret_key,
        })
    }

    /// `GET /accounts/cloud-api/cloudApiKeys` — list existing CAPI key names. Best-effort: used
    /// only to warn about `redisctl-*` key sprawl at login, so tolerant of response shape.
    pub async fn fetch_capi_keys(&self) -> Result<Vec<String>, AuthError> {
        let body = self
            .authed_get("accounts/cloud-api/cloudApiKeys")
            .await?
            .text()
            .await?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| AuthError::Protocol(format!("could not parse cloudApiKeys list: {e}")))?;
        // Tolerate a top-level array or a wrapper object ({"cloudApiKeys":[...]}).
        let arr = value
            .get("cloudApiKeys")
            .and_then(|v| v.as_array())
            .or_else(|| value.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(arr
            .iter()
            .filter_map(|k| {
                let obj = k.get("cloudApiKey").unwrap_or(k);
                obj.get("name").and_then(|v| v.as_str()).map(String::from)
            })
            .collect())
    }

    fn session(&self) -> Result<&Session, AuthError> {
        self.session
            .as_ref()
            .ok_or_else(|| AuthError::Protocol("not logged in to the SM API".into()))
    }

    async fn authed_get(&self, path: &str) -> Result<reqwest::Response, AuthError> {
        let s = self.session()?;
        Ok(self
            .http
            .get(endpoint(&self.base_url, path))
            .header(reqwest::header::COOKIE, &s.cookie)
            .header("x-csrf-token", &s.csrf)
            .send()
            .await?)
    }

    async fn authed_post_json(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, AuthError> {
        let s = self.session()?;
        Ok(self
            .http
            .post(endpoint(&self.base_url, path))
            .header(reqwest::header::COOKIE, &s.cookie)
            .header("x-csrf-token", &s.csrf)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await?)
    }
}

/// Pull the `JSESSIONID` value out of the response's `Set-Cookie` headers.
fn extract_jsessionid(resp: &reqwest::Response) -> Option<String> {
    for value in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        let Ok(text) = value.to_str() else { continue };
        for part in text.split(';') {
            if let Some(v) = part.trim().strip_prefix("JSESSIONID=") {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mount_login_and_csrf(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Set-Cookie", "JSESSIONID=SID123; Path=/; HttpOnly"),
            )
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/csrf"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "csrfToken": { "csrf_token": "CSRF-XYZ", "csrf_enabled": true, "errors": [] }
            })))
            .mount(server)
            .await;
    }

    fn client(server: &MockServer) -> SmApiClient {
        SmApiClient::new(Url::parse(&server.uri()).unwrap())
    }

    async fn logged_in(server: &MockServer) -> SmApiClient {
        mount_login_and_csrf(server).await;
        let mut c = client(server);
        c.login("ACCESS", None).await.unwrap();
        c
    }

    #[tokio::test]
    async fn login_then_users_me_sends_cookie_and_csrf() {
        let server = MockServer::start().await;
        // /users/me only matches when the session cookie + csrf header are present.
        Mock::given(method("GET"))
            .and(path("/users/me"))
            .and(header("cookie", "JSESSIONID=SID123"))
            .and(header("x-csrf-token", "CSRF-XYZ"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "114429",
                "current_account_id": "112117",
                "email": "user@example.com",
                "product_type": "unifiedrc"
            })))
            .mount(&server)
            .await;

        let c = logged_in(&server).await;
        let user = c.fetch_current_user().await.unwrap();
        assert_eq!(user.id, "114429");
        assert_eq!(user.user_account().unwrap(), 114429);
        assert_eq!(user.current_account_id.as_deref(), Some("112117"));
        assert_eq!(user.email.as_deref(), Some("user@example.com"));
    }

    #[tokio::test]
    async fn accounts_extracts_api_access_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [
                    { "id": 112117, "name": "Krum", "api_access_key": "ACCT-KEY", "has_paid": false }
                ]
            })))
            .mount(&server)
            .await;

        let c = logged_in(&server).await;
        let accounts = c.fetch_accounts().await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, 112117);
        assert_eq!(accounts[0].api_access_key.as_deref(), Some("ACCT-KEY"));
    }

    #[tokio::test]
    async fn ensure_capi_enabled_ok_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiAccessKey"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "cloudApiAccessKey": { "accessKey": "ACCT-KEY" }
            })))
            .mount(&server)
            .await;
        let c = logged_in(&server).await;
        assert!(c.ensure_capi_enabled().await.is_ok());
    }

    #[tokio::test]
    async fn ensure_capi_enabled_ok_when_already_exists() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiAccessKey"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": { "status": 400, "code": "account_api_key_already_exists", "message": "" }
            })))
            .mount(&server)
            .await;
        let c = logged_in(&server).await;
        assert!(c.ensure_capi_enabled().await.is_ok());
    }

    #[tokio::test]
    async fn ensure_capi_enabled_errors_on_other_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiAccessKey"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let c = logged_in(&server).await;
        assert!(matches!(
            c.ensure_capi_enabled().await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn mint_capi_key_reads_secret_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiKeys"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 999, "name": "redisctl-x", "secret_key": "SECRET", "user_account": 114429,
                "ip_whitelist": [], "errors": []
            })))
            .mount(&server)
            .await;
        let c = logged_in(&server).await;
        let key = c.mint_capi_key("redisctl-x", 114429).await.unwrap();
        assert_eq!(key.name, "redisctl-x");
        assert_eq!(key.secret_key, "SECRET");
        // Debug must not leak the secret.
        assert!(!format!("{key:?}").contains("SECRET"));
    }

    #[tokio::test]
    async fn mint_capi_key_reads_wrapped_secret_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiKeys"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "cloudApiKey": { "name": "redisctl-y", "secret_key": "SEK" }
            })))
            .mount(&server)
            .await;
        let c = logged_in(&server).await;
        let key = c.mint_capi_key("redisctl-y", 1).await.unwrap();
        assert_eq!(key.secret_key, "SEK");
    }

    #[tokio::test]
    async fn mint_capi_key_missing_secret_is_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiKeys"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "x", "errors": ["nope"]
            })))
            .mount(&server)
            .await;
        let c = logged_in(&server).await;
        assert!(matches!(
            c.mint_capi_key("x", 1).await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn login_without_jsessionid_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let mut c = client(&server);
        assert!(matches!(
            c.login("ACCESS", None).await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn login_failure_status_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(401)
                    .append_header("Set-Cookie", "JSESSIONID=SID; Path=/")
                    .set_body_json(serde_json::json!({
                        "errors": { "status": 401, "code": "user-invalid-access-token" }
                    })),
            )
            .mount(&server)
            .await;
        let mut c = client(&server);
        assert!(matches!(
            c.login("ACCESS", None).await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn login_sends_sm_id_token_only_when_provided() {
        let server = MockServer::start().await;
        // This mock matches ONLY when sm-id-token is present.
        Mock::given(method("POST"))
            .and(path("/login"))
            .and(header("sm-id-token", "IDT"))
            .respond_with(ResponseTemplate::new(200).append_header("Set-Cookie", "JSESSIONID=S"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/csrf"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "csrfToken": { "csrf_token": "C" }
            })))
            .mount(&server)
            .await;
        let mut c = client(&server);
        // With the id token, login matches the header-gated mock and succeeds.
        assert!(c.login("ACCESS", Some("IDT")).await.is_ok());
    }
}
