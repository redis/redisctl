//! Orchestrates a full `cloud auth login`: build the OIDC flow clients, and after a flow
//! yields tokens, run the SM exchange and mint a CAPI key.
//!
//! Returns [`MintedCredentials`] for the caller to persist (see
//! `config::Config::apply_cloud_login`). Persistence lives in the config layer so this stays
//! free of file/keyring I/O and easy to test. The flow itself (device polling with progress,
//! or loopback with a browser) is driven by the CLI using [`CloudAuthenticator::device_flow`]
//! / [`CloudAuthenticator::loopback`].

use url::Url;

use super::sm_api::{LoginFlow, SmAccount, SmApiClient};
use super::{AuthError, DeviceFlowClient, LoopbackFlowClient, TokenSet, default_http_client};

/// Result of a completed login: a Redis Cloud CAPI key pair plus context, ready to persist.
///
/// Secret fields are redacted from `Debug`.
#[derive(Clone)]
pub struct MintedCredentials {
    pub account_id: Option<String>,
    pub email: Option<String>,
    /// Account-level CAPI key (`x-api-key`).
    pub api_key: String,
    /// Minted user secret (`x-api-secret-key`).
    pub api_secret: String,
    /// CAPI base URL to record in the resulting cloud profile.
    pub api_url: String,
    /// Okta refresh token (rotating) to persist for silent re-auth, if the IdP issued one.
    pub refresh_token: Option<String>,
    /// Name of the minted `redisctl-*` CAPI key (visible/revocable in the console).
    pub capi_key_name: String,
    /// How many `redisctl-*` CAPI keys the account has after this mint (best-effort; 0 if the
    /// listing failed). The CLI warns when this grows, since each login mints a new key (D5).
    pub redisctl_key_count: usize,
}

impl std::fmt::Debug for MintedCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MintedCredentials")
            .field("account_id", &self.account_id)
            .field("email", &self.email)
            .field("api_key", &"<redacted>")
            .field("api_secret", &"<redacted>")
            .field("api_url", &self.api_url)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("capi_key_name", &self.capi_key_name)
            .field("redisctl_key_count", &self.redisctl_key_count)
            .finish()
    }
}

/// Ties the OIDC endpoints (Okta) and the SM API together for one environment.
#[derive(Clone)]
pub struct CloudAuthenticator {
    issuer: Url,
    client_id: String,
    sm_api_url: Url,
    capi_url: String,
    http: reqwest::Client,
}

impl CloudAuthenticator {
    /// Build for one environment. `issuer`/`client_id` drive the Okta flows, `sm_api_url` the
    /// key-minting exchange, and `capi_url` is recorded in the resulting profile.
    pub fn new(
        issuer: Url,
        client_id: impl Into<String>,
        sm_api_url: Url,
        capi_url: impl Into<String>,
    ) -> Self {
        Self {
            issuer,
            client_id: client_id.into(),
            sm_api_url,
            capi_url: capi_url.into(),
            http: default_http_client(),
        }
    }

    /// Use a caller-provided reqwest client (tests / shared client).
    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    /// Device-authorization-grant client for headless / agent logins. The flow runs on the
    /// `oauth2` crate's own HTTP stack, so it does not share this authenticator's SM client.
    pub fn device(&self) -> DeviceFlowClient {
        DeviceFlowClient::new(self.issuer.clone(), self.client_id.clone())
    }

    /// Auth-code + PKCE loopback client for interactive human logins.
    pub fn loopback(&self) -> LoopbackFlowClient {
        LoopbackFlowClient::new(self.issuer.clone(), self.client_id.clone())
    }

    /// Refresh an Okta refresh token for a fresh token set (Okta rotates it). The grant is
    /// flow-agnostic, so it goes straight through `oidc` rather than a specific flow client.
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenSet, AuthError> {
        super::oidc::refresh(&self.issuer, &self.client_id, refresh_token).await
    }

    /// Given tokens from a flow, run the SM exchange and mint a CAPI key named `key_name`.
    pub async fn complete_login(
        &self,
        tokens: &TokenSet,
        key_name: &str,
        flow: LoginFlow,
    ) -> Result<MintedCredentials, AuthError> {
        let mut sm =
            SmApiClient::with_http_client(self.sm_api_url.clone(), self.http.clone(), flow);
        // Google/GitHub logins must not send Sm-Id-Token (SSO-only); see sm_api docs.
        sm.login(&tokens.access_token, None).await?;
        let user = sm.fetch_current_user().await?;
        sm.ensure_capi_enabled().await?;
        // Pick the account matching the logged-in user's current_account_id. /accounts list
        // order isn't guaranteed, so taking the first entry could mint a key for the wrong
        // account in a multi-account org. Fall back to the first only when it's absent/unknown.
        let account = select_account(
            sm.fetch_accounts().await?,
            user.current_account_id.as_deref(),
        )
        .ok_or_else(|| AuthError::Protocol("no accounts associated with this login".into()))?;
        let api_key = account.api_access_key.ok_or_else(|| {
            AuthError::Protocol("account has no CAPI access key after enabling CAPI".into())
        })?;
        let minted = sm.mint_capi_key(key_name, user.user_account()?).await?;
        // Best-effort: count our keys so the CLI can warn about sprawl (D5). Never fail login
        // over this — a listing error just means no warning.
        let redisctl_key_count = sm
            .fetch_capi_keys()
            .await
            .map(|keys| keys.iter().filter(|n| n.starts_with("redisctl-")).count())
            .unwrap_or(0);
        Ok(MintedCredentials {
            account_id: user.current_account_id,
            email: user.email,
            api_key,
            api_secret: minted.secret_key,
            api_url: self.capi_url.clone(),
            refresh_token: tokens.refresh_token.clone(),
            capi_key_name: minted.name,
            redisctl_key_count,
        })
    }
}

/// Choose the account matching `current_account_id` (the logged-in user context); fall back to
/// the first account only when the id is absent or not present in the list.
fn select_account(accounts: Vec<SmAccount>, current_account_id: Option<&str>) -> Option<SmAccount> {
    let target = current_account_id.and_then(|s| s.parse::<u64>().ok());
    let idx = target
        .and_then(|id| accounts.iter().position(|a| a.id == id))
        .unwrap_or(0);
    accounts.into_iter().nth(idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn account(id: u64) -> SmAccount {
        serde_json::from_value(serde_json::json!({
            "id": id, "api_access_key": format!("KEY-{id}")
        }))
        .unwrap()
    }

    #[test]
    fn select_account_prefers_current_account_id() {
        let accts = vec![account(111), account(222), account(333)];
        // Matches the user's current account, not the first in the list.
        let chosen = select_account(accts, Some("222")).unwrap();
        assert_eq!(chosen.id, 222);
    }

    #[test]
    fn select_account_falls_back_to_first_when_absent_or_unknown() {
        assert_eq!(
            select_account(vec![account(111), account(222)], None)
                .unwrap()
                .id,
            111
        );
        // current_account_id present but not in the list → first (defensive fallback).
        assert_eq!(
            select_account(vec![account(111), account(222)], Some("999"))
                .unwrap()
                .id,
            111
        );
        assert!(select_account(vec![], Some("1")).is_none());
    }

    #[test]
    fn debug_redacts_secrets() {
        let creds = MintedCredentials {
            account_id: Some("42".to_string()),
            email: Some("u@example.com".to_string()),
            api_key: "AKEY-visible-should-not-appear".to_string(),
            api_secret: "SECRET-should-not-appear".to_string(),
            api_url: "https://api.example.com/v1".to_string(),
            refresh_token: Some("RT-should-not-appear".to_string()),
            capi_key_name: "redisctl-cli-1".to_string(),
            redisctl_key_count: 3,
        };
        let dbg = format!("{creds:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(!dbg.contains("AKEY-visible-should-not-appear"));
        assert!(!dbg.contains("SECRET-should-not-appear"));
        assert!(!dbg.contains("RT-should-not-appear"));
        // Non-secret fields remain visible for diagnostics.
        assert!(dbg.contains("u@example.com"));
        assert!(dbg.contains("redisctl-cli-1"));
    }

    #[tokio::test]
    async fn complete_login_runs_the_full_exchange() {
        let server = MockServer::start().await;
        let mount = |m: &str, p: &'static str, body: serde_json::Value, cookie: bool| {
            let mut tmpl = ResponseTemplate::new(200).set_body_json(body);
            if cookie {
                tmpl = tmpl.append_header("Set-Cookie", "JSESSIONID=SID; Path=/");
            }
            Mock::given(method(m)).and(path(p)).respond_with(tmpl)
        };
        mount("POST", "/login", serde_json::json!({}), true)
            .mount(&server)
            .await;
        mount(
            "GET",
            "/csrf",
            serde_json::json!({"csrfToken": {"csrf_token": "C"}}),
            false,
        )
        .mount(&server)
        .await;
        mount(
            "GET",
            "/users/me",
            serde_json::json!({"id": "114429", "current_account_id": "112117", "email": "u@e.com"}),
            false,
        )
        .mount(&server)
        .await;
        mount(
            "POST",
            "/accounts/cloud-api/cloudApiAccessKey",
            serde_json::json!({"cloudApiAccessKey": {"accessKey": "ACCT"}}),
            false,
        )
        .mount(&server)
        .await;
        mount(
            "GET",
            "/accounts",
            serde_json::json!({"accounts": [{"id": 112117, "api_access_key": "ACCT-KEY"}]}),
            false,
        )
        .mount(&server)
        .await;
        mount(
            "POST",
            "/accounts/cloud-api/cloudApiKeys",
            serde_json::json!({"name": "redisctl-test", "secret_key": "SECRET"}),
            false,
        )
        .mount(&server)
        .await;

        let auth = CloudAuthenticator::new(
            Url::parse("https://issuer.example/oauth2/default").unwrap(),
            "cid",
            Url::parse(&server.uri()).unwrap(),
            "https://capi.example/v1",
        );
        let tokens = TokenSet {
            access_token: "AT".into(),
            refresh_token: Some("RT".into()),
            expires_in: 3600,
        };

        let creds = auth
            .complete_login(&tokens, "redisctl-test", LoginFlow::Loopback)
            .await
            .unwrap();
        assert_eq!(creds.api_key, "ACCT-KEY");
        assert_eq!(creds.api_secret, "SECRET");
        assert_eq!(creds.api_url, "https://capi.example/v1");
        assert_eq!(creds.account_id.as_deref(), Some("112117"));
        assert_eq!(creds.email.as_deref(), Some("u@e.com"));
        assert_eq!(creds.refresh_token.as_deref(), Some("RT"));
        assert_eq!(creds.capi_key_name, "redisctl-test");
        // secrets must not leak via Debug
        let dbg = format!("{creds:?}");
        assert!(!dbg.contains("SECRET") && !dbg.contains("ACCT-KEY") && !dbg.contains("RT"));
    }

    #[tokio::test]
    async fn complete_login_errors_when_login_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(401).append_header("Set-Cookie", "JSESSIONID=S"))
            .mount(&server)
            .await;
        let auth = CloudAuthenticator::new(
            Url::parse("https://issuer.example/oauth2/default").unwrap(),
            "cid",
            Url::parse(&server.uri()).unwrap(),
            "https://capi.example/v1",
        );
        let tokens = TokenSet {
            access_token: "AT".into(),
            refresh_token: None,
            expires_in: 3600,
        };
        assert!(matches!(
            auth.complete_login(&tokens, "k", LoginFlow::Loopback).await,
            Err(AuthError::Protocol(_))
        ));
    }
}
