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

/// SM error code on the CAPI-enable call when the signed-in user is not the account owner. Note
/// this endpoint nests its errors as a JSON-encoded string, so it is matched on the body rather
/// than through the `/login` error envelope.
const INSUFFICIENT_PERMISSION_CODE: &str = "insufficient-permission";
/// SM error code returned when a password-only account must be linked to social sign-in before it
/// can authenticate this way. The consent step is console-only.
const SOCIAL_MIGRATION_REQUIRED_CODE: &str = "user-agreement-for-social-login-migration-missing";
/// SM error code for an MFA challenge on `/login`.
const MFA_REQUIRED_CODE: &str = "user-mfa-required";
/// SM error code for a rejected MFA code.
const MFA_INVALID_CODE: &str = "mfa-invalid-code";
/// SM error code for an `mfa_type` it does not recognise — a client bug, not a user error.
const MFA_INVALID_TYPE_CODE: &str = "mfa-invalid-type";
/// SM error code for too many MFA attempts.
const MFA_QUOTA_EXCEEDED_CODE: &str = "mfa-quota-exceeded";
/// The only MFA type we submit. SM also knows SMS and Email, but TOTP is what a CLI can prompt for.
///
/// Case matters: SM resolves this with `EnumMFAType.toEnum`, which compares against the enum
/// constant name (`SMS`, `Totp`, `Email`) verbatim — there is no custom `toString`. Sending
/// `"totp"` is rejected with `mfa-invalid-type`. RedisInsight sends the same `"Totp"`.
const MFA_TYPE_TOTP: &str = "Totp";

/// Attribution sent on `POST /login`, mirroring what RedisInsight sends. SM records these on the
/// signup path (`registerOktaUser` → `buildRegistrationItem`), so without them a user whose first
/// contact with Redis Cloud is `cloud auth login` is registered with no originating tool.
const UTM_SOURCE: &str = "redisctl";
/// Coarse channel, kept stable so dashboards can group on it; the flow goes in `utm_campaign`.
const UTM_MEDIUM: &str = "cli";

/// Which login flow produced the tokens. Reported as `utm_campaign`, so interactive and
/// agent-driven sign-ins can be told apart in analytics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginFlow {
    /// Browser on the same machine, redirect caught on loopback.
    Loopback,
    /// Device-authorization grant — headless machines and agents.
    Device,
}

impl LoginFlow {
    fn as_str(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Device => "device",
        }
    }
}

/// SM API client. Stateless until [`SmApiClient::login`] establishes a session.
pub struct SmApiClient {
    base_url: Url,
    http: reqwest::Client,
    session: Option<Session>,
    /// Which flow produced the tokens, reported to SM as `utm_campaign`.
    flow: LoginFlow,
    /// JSESSIONID from an MFA-challenged `/login`. SM keeps the challenge state in that session,
    /// so [`SmApiClient::complete_mfa`] must reuse this exact cookie — a fresh session has no
    /// challenge to verify against.
    pending_mfa_cookie: Option<String>,
}

struct Session {
    /// Full cookie header value, e.g. `JSESSIONID=abc123`.
    cookie: String,
    csrf: String,
}

/// SM's error envelope: `{"errors": {"status": 401, "code": "…", "params": …}}`.
#[derive(Debug, Default, Deserialize)]
struct SmErrorEnvelope {
    errors: Option<SmError>,
}

#[derive(Debug, Default, Deserialize)]
struct SmError {
    code: Option<String>,
    /// Free-form; for MFA it carries the offered factors. Shape is not contractual, so it is
    /// parsed best-effort and never allowed to fail the classification.
    params: Option<serde_json::Value>,
}

/// Pull the error code out of an SM response body, if it has the usual envelope.
fn sm_error_code(body: &str) -> Option<(String, Option<serde_json::Value>)> {
    let env: SmErrorEnvelope = serde_json::from_str(body).ok()?;
    let err = env.errors?;
    Some((err.code?, err.params))
}

/// Best-effort extraction of MFA factor names from SM's `params`. Returns an empty list rather
/// than failing: the factor list is cosmetic (it only enriches the prompt).
fn mfa_factors(params: Option<&serde_json::Value>) -> Vec<String> {
    fn strings(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => {
                // `params` is sometimes a JSON-encoded string; try one level of nesting.
                if let Ok(inner) = serde_json::from_str::<serde_json::Value>(s) {
                    strings(&inner, out);
                } else if !s.is_empty() {
                    out.push(s.clone());
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
            serde_json::Value::Object(map) => {
                for key in ["type", "factorType", "mfaType"] {
                    if let Some(serde_json::Value::String(s)) = map.get(key) {
                        out.push(s.clone());
                        return;
                    }
                }
                map.values().for_each(|v| strings(v, out));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Some(p) = params {
        strings(p, &mut out);
    }
    out.dedup();
    out
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
    pub fn new(base_url: Url, flow: LoginFlow) -> Self {
        Self {
            base_url,
            http: default_http_client(),
            session: None,
            flow,
            pending_mfa_cookie: None,
        }
    }

    /// Build with a caller-provided reqwest client (tests / shared client).
    pub fn with_http_client(base_url: Url, http: reqwest::Client, flow: LoginFlow) -> Self {
        Self {
            base_url,
            http,
            session: None,
            flow,
            pending_mfa_cookie: None,
        }
    }

    /// Establish a session: `POST /login` with the Okta access token, then fetch the CSRF
    /// token. Pass `sm_id_token` only for SSO logins (omit for Google/GitHub).
    ///
    /// Returns [`AuthError::MfaRequired`] when SM challenges the login; call
    /// [`SmApiClient::complete_mfa`] on this same client to finish it.
    pub async fn login(
        &mut self,
        access_token: &str,
        sm_id_token: Option<&str>,
    ) -> Result<(), AuthError> {
        self.post_login(access_token, sm_id_token, None, None).await
    }

    /// Finish an MFA-challenged login with a TOTP code, reusing the challenged session.
    ///
    /// Errors with [`AuthError::Protocol`] if no challenge is outstanding — submitting a code on a
    /// fresh session cannot work, because SM verifies it against challenge state held in the
    /// session it issued.
    pub async fn complete_mfa(
        &mut self,
        access_token: &str,
        sm_id_token: Option<&str>,
        code: &str,
    ) -> Result<(), AuthError> {
        let cookie = self.pending_mfa_cookie.clone().ok_or_else(|| {
            AuthError::Protocol("no outstanding SM multi-factor challenge to complete".into())
        })?;
        self.post_login(access_token, sm_id_token, Some(code), Some(&cookie))
            .await
    }

    async fn post_login(
        &mut self,
        access_token: &str,
        sm_id_token: Option<&str>,
        mfa_code: Option<&str>,
        mfa_cookie: Option<&str>,
    ) -> Result<(), AuthError> {
        let mut body = serde_json::json!({
            "utm_source": UTM_SOURCE,
            "utm_medium": UTM_MEDIUM,
            "utm_campaign": self.flow.as_str(),
        });
        if let Some(code) = mfa_code {
            body["mfa_type"] = MFA_TYPE_TOTP.into();
            body["mfa_code"] = code.into();
        }
        let mut req = self
            .http
            .post(endpoint(&self.base_url, "login"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        if let Some(id) = sm_id_token {
            req = req.header("sm-id-token", id);
        }
        if let Some(c) = mfa_cookie {
            req = req.header(reqwest::header::COOKIE, format!("JSESSIONID={c}"));
        }
        let resp = req.send().await?;
        let status = resp.status();
        // Read the cookie before consuming the body: on an MFA challenge the session carrying the
        // challenge arrives on the *error* response, and the retry must reuse it.
        let cookie = extract_jsessionid(&resp);
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(self.classify_login_error(status, &body, cookie, mfa_cookie));
        }
        let cookie = cookie
            .or_else(|| mfa_cookie.map(str::to_string))
            .ok_or_else(|| AuthError::Protocol("SM /login did not set a JSESSIONID".into()))?;
        let csrf = self.fetch_csrf(&cookie).await?;
        self.session = Some(Session {
            cookie: format!("JSESSIONID={cookie}"),
            csrf,
        });
        self.pending_mfa_cookie = None;
        Ok(())
    }

    fn classify_login_error(
        &mut self,
        status: reqwest::StatusCode,
        body: &str,
        cookie: Option<String>,
        previous_cookie: Option<&str>,
    ) -> AuthError {
        match sm_error_code(body) {
            Some((code, params)) if code == MFA_REQUIRED_CODE => {
                // Keep the challenged session for the retry; SM may or may not re-issue it.
                self.pending_mfa_cookie = cookie.or_else(|| previous_cookie.map(str::to_string));
                AuthError::MfaRequired {
                    factors: mfa_factors(params.as_ref()),
                }
            }
            Some((code, _)) if code == SOCIAL_MIGRATION_REQUIRED_CODE => {
                AuthError::MigrationRequired
            }
            Some((code, _)) if code == MFA_INVALID_CODE => AuthError::MfaInvalidCode,
            // We sent an mfa_type SM does not accept. Never the user's fault, and retrying the
            // same request cannot help, so say so plainly rather than blaming their code.
            Some((code, _)) if code == MFA_INVALID_TYPE_CODE => AuthError::Protocol(
                "the multi-factor type this client sent was rejected by Redis Cloud \
                 (mfa-invalid-type); this is a bug in redisctl, please report it"
                    .to_string(),
            ),
            Some((code, _)) if code == MFA_QUOTA_EXCEEDED_CODE => AuthError::MfaQuotaExceeded,
            _ => AuthError::Protocol(format!("SM /login failed ({status}): {}", truncate(body))),
        }
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
        // Only an account owner can turn on programmatic access. Nothing the CLI can do, but the
        // user can ask the owner to enable it once — after which the call above is a no-op.
        if body.contains(INSUFFICIENT_PERMISSION_CODE) {
            return Err(AuthError::NotAccountOwner);
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

    /// Rebind the session to `account_id` (`POST /accounts/setcurrent/{id}`).
    ///
    /// Every CAPI call resolves the account from the session — `createApiSecretKey` uses the
    /// session's `userAccountId` — so this must happen *before* enabling access or minting, or the
    /// key lands on the previous account. Annotated `LEGACY_ONLY` server-side, which the JSESSIONID
    /// established by [`SmApiClient::login`] satisfies.
    pub async fn set_current_account(&self, account_id: u64) -> Result<(), AuthError> {
        let resp = self
            .authed_post_json(
                &format!("accounts/setcurrent/{account_id}"),
                serde_json::json!({}),
            )
            .await?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(AuthError::Protocol(format!(
            "could not switch to account {account_id} ({status}): {}",
            truncate(&body)
        )))
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
    use wiremock::matchers::{body_string_contains, header, method, path};
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
        SmApiClient::new(Url::parse(&server.uri()).unwrap(), LoginFlow::Loopback)
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

    /// Only an owner may enable programmatic access. SM says so precisely, but nests the code in a
    /// JSON-encoded string, so it is matched on the body — this pins that it is still classified.
    #[tokio::test]
    async fn ensure_capi_enabled_reports_owner_only_distinctly() {
        let server = MockServer::start().await;
        let c = logged_in(&server).await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiAccessKey"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "errors": "[{\"field_name\":null,\"error_code\":\"insufficient-permission\",\"params\":[{\"key\":\"allowed-roles\",\"value\":[\"owner\"]}]}]"
            })))
            .mount(&server)
            .await;
        assert!(matches!(
            c.ensure_capi_enabled().await,
            Err(AuthError::NotAccountOwner)
        ));
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

    /// SM records `utm_*` on the signup path, so a first-ever login through redisctl must carry
    /// attribution or the tool is invisible in signup analytics. The mock matches only if all three
    /// fields are present, and `utm_campaign` distinguishes the two flows.
    /// Every account-scoped call resolves the account from the session, so a switch has to be a
    /// real request to the documented path — the mock only matches that exact URL.
    #[tokio::test]
    async fn set_current_account_posts_to_setcurrent() {
        let server = MockServer::start().await;
        let c = logged_in(&server).await;
        Mock::given(method("POST"))
            .and(path("/accounts/setcurrent/424242"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        c.set_current_account(424242).await.unwrap();
    }

    /// A refused switch must fail loudly; silently continuing would mint on the previous account.
    #[tokio::test]
    async fn set_current_account_surfaces_a_refusal() {
        let server = MockServer::start().await;
        let c = logged_in(&server).await;
        Mock::given(method("POST"))
            .and(path("/accounts/setcurrent/1"))
            .respond_with(ResponseTemplate::new(403).set_body_string("nope"))
            .mount(&server)
            .await;
        assert!(matches!(
            c.set_current_account(1).await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn login_sends_utm_attribution_per_flow() {
        for (flow, campaign) in [
            (LoginFlow::Loopback, "loopback"),
            (LoginFlow::Device, "device"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/login"))
                .and(body_string_contains("\"utm_source\":\"redisctl\""))
                .and(body_string_contains("\"utm_medium\":\"cli\""))
                .and(body_string_contains(format!(
                    "\"utm_campaign\":\"{campaign}\""
                )))
                .respond_with(
                    ResponseTemplate::new(200).append_header("Set-Cookie", "JSESSIONID=S"),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/csrf"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "csrfToken": { "csrf_token": "CSRF", "csrf_enabled": true, "errors": [] }
                })))
                .mount(&server)
                .await;
            let mut c = SmApiClient::new(Url::parse(&server.uri()).unwrap(), flow);
            c.login("ACCESS", None).await.unwrap();
        }
    }

    /// The MFA retry must keep the attribution alongside the code.
    #[tokio::test]
    async fn mfa_retry_still_carries_utm() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .and(body_string_contains("\"mfa_code\":\"123456\""))
            // Case-sensitive on SM's side; lowercase is rejected as mfa-invalid-type.
            .and(body_string_contains("\"mfa_type\":\"Totp\""))
            .and(body_string_contains("\"utm_source\":\"redisctl\""))
            .respond_with(ResponseTemplate::new(200).append_header("Set-Cookie", "JSESSIONID=S"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/csrf"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "csrfToken": { "csrf_token": "CSRF", "csrf_enabled": true, "errors": [] }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(401)
                    .append_header("Set-Cookie", "JSESSIONID=CH; Path=/")
                    .set_body_json(serde_json::json!({
                        "errors": { "status": 401, "code": "user-mfa-required" }
                    })),
            )
            .mount(&server)
            .await;
        let mut c = client(&server);
        assert!(matches!(
            c.login("ACCESS", None).await,
            Err(AuthError::MfaRequired { .. })
        ));
        c.complete_mfa("ACCESS", None, "123456").await.unwrap();
    }

    /// A password-only account that hasn't been linked to social sign-in gets its own error, so
    /// the CLI can point the user at the one-time console step instead of a generic failure.
    #[tokio::test]
    async fn login_social_migration_required_is_classified() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "errors": {
                    "status": 422,
                    "code": "user-agreement-for-social-login-migration-missing"
                }
            })))
            .mount(&server)
            .await;
        let mut c = client(&server);
        assert!(matches!(
            c.login("ACCESS", None).await,
            Err(AuthError::MigrationRequired)
        ));
    }

    /// SM answers the first `/login` with `user-mfa-required`; the factors come back for the prompt.
    #[tokio::test]
    async fn login_reports_mfa_challenge_with_factors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(401)
                    .append_header("Set-Cookie", "JSESSIONID=CHALLENGED; Path=/")
                    .set_body_json(serde_json::json!({
                        "errors": { "status": 401, "code": "user-mfa-required",
                                    "params": [{ "type": "totp" }] }
                    })),
            )
            .mount(&server)
            .await;
        let mut c = client(&server);
        match c.login("ACCESS", None).await {
            Err(AuthError::MfaRequired { factors }) => assert_eq!(factors, vec!["totp"]),
            other => panic!("expected MfaRequired, got {other:?}"),
        }
    }

    /// The regression that matters: SM holds the challenge in the session it issued on the *401*,
    /// so the retry must send that exact JSESSIONID back. The mock only matches if it does.
    #[tokio::test]
    async fn complete_mfa_reuses_the_challenged_session() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .and(header("cookie", "JSESSIONID=CHALLENGED"))
            .and(body_string_contains("\"mfa_code\":\"123456\""))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/csrf"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "csrfToken": { "csrf_token": "CSRF", "csrf_enabled": true, "errors": [] }
            })))
            .mount(&server)
            .await;
        // Unmatched-request fallback: the challenge itself.
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(401)
                    .append_header("Set-Cookie", "JSESSIONID=CHALLENGED; Path=/")
                    .set_body_json(serde_json::json!({
                        "errors": { "status": 401, "code": "user-mfa-required" }
                    })),
            )
            .mount(&server)
            .await;

        let mut c = client(&server);
        assert!(matches!(
            c.login("ACCESS", None).await,
            Err(AuthError::MfaRequired { .. })
        ));
        // Succeeds only because the challenged cookie was carried over.
        c.complete_mfa("ACCESS", None, "123456").await.unwrap();
    }

    /// A code with no outstanding challenge can never succeed against SM, so fail locally rather
    /// than sending a request that would trip over missing session state.
    #[tokio::test]
    async fn complete_mfa_without_a_challenge_errors() {
        let server = MockServer::start().await;
        let mut c = client(&server);
        assert!(matches!(
            c.complete_mfa("ACCESS", None, "123456").await,
            Err(AuthError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn mfa_error_codes_are_classified() {
        for (code, want_invalid) in [("mfa-invalid-code", true), ("mfa-quota-exceeded", false)] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/login"))
                .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "errors": { "status": 400, "code": code }
                })))
                .mount(&server)
                .await;
            let mut c = client(&server);
            let got = c.login("ACCESS", None).await;
            if want_invalid {
                assert!(matches!(got, Err(AuthError::MfaInvalidCode)), "{code}");
            } else {
                assert!(matches!(got, Err(AuthError::MfaQuotaExceeded)), "{code}");
            }
        }
    }

    #[test]
    fn mfa_factors_tolerates_shapes_we_have_not_seen() {
        assert!(mfa_factors(None).is_empty());
        assert!(mfa_factors(Some(&serde_json::json!({}))).is_empty());
        // A JSON-encoded string payload, which SM sometimes uses for `params`.
        assert_eq!(
            mfa_factors(Some(&serde_json::json!(
                r#"[{"factorType":"token:software:totp"}]"#
            ))),
            vec!["token:software:totp"]
        );
        // Never panics on unexpected scalars.
        assert!(mfa_factors(Some(&serde_json::json!(7))).is_empty());
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
