//! Orchestrates a full `cloud auth login`: build the OIDC flow clients, and after a flow
//! yields tokens, run the SM exchange and mint a CAPI key.
//!
//! Returns [`MintedCredentials`] for the caller to persist (see
//! `config::Config::apply_cloud_login`). Persistence lives in the config layer so this stays
//! free of file/keyring I/O and easy to test. The flow itself (device polling with progress,
//! or loopback with a browser) is driven by the CLI using [`CloudAuthenticator::device_flow`]
//! / [`CloudAuthenticator::loopback`].

use url::Url;

use super::sm_api::{LoginFlow, SmAccount, SmApiClient, SmUser};
use super::{AuthError, DeviceFlowClient, LoopbackFlowClient, TokenSet, default_http_client};

/// Result of a completed login: a Redis Cloud CAPI key pair plus context, ready to persist.
///
/// Secret fields are redacted from `Debug`.
#[derive(Clone)]
pub struct MintedCredentials {
    /// Numeric account id, matching the `id` of the matching entry in `accounts`, so the two can
    /// be compared directly by a caller reading the JSON output.
    pub account_id: Option<u64>,
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
    /// Name of the account the key was minted for, when the API reports one.
    pub account_name: Option<String>,
    /// Every account the signed-in user belongs to. The key is scoped to exactly one of them —
    /// the session's *current* account — so the CLI can both name the one it used and list the
    /// alternatives, which are otherwise only discoverable in the console.
    pub accounts: Vec<LoginAccount>,
}

/// How the account to mint for is decided.
///
/// [`AccountChoice::Prompt`] exists because a picker cannot run before the exchange: listing the
/// accounts needs a session, and re-logging-in to act on the answer would mean a second sign-in
/// (and a second MFA challenge). The callback is invoked mid-exchange instead, on the one session.
pub enum AccountChoice {
    /// Whatever account the session is already on.
    Current,
    /// A specific account id.
    Id(u64),
    /// Decide once the accounts are known. Called with every account the user belongs to and the
    /// id of the current one, when the API reports it.
    Prompt(AccountPrompt),
}

/// Callback for [`AccountChoice::Prompt`]: pick an account id, or fail with the reason.
pub type AccountPrompt =
    Box<dyn Fn(&[LoginAccount], Option<u64>) -> Result<u64, AuthError> + Send + Sync>;

impl std::fmt::Debug for AccountChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Current => f.write_str("Current"),
            Self::Id(id) => write!(f, "Id({id})"),
            Self::Prompt(_) => f.write_str("Prompt(..)"),
        }
    }
}

/// One account the signed-in user belongs to, as reported during login.
#[derive(Debug, Clone)]
pub struct LoginAccount {
    pub id: u64,
    pub name: Option<String>,
}

impl LoginAccount {
    /// `Acme (#316941)`, or `#316941` when the API reports no name. Used both in the CLI listing
    /// and in the `UnknownAccount` message, so the two always read the same.
    pub fn label(&self) -> String {
        match &self.name {
            Some(n) => format!("{} (#{})", n, self.id),
            None => format!("#{}", self.id),
        }
    }
}

impl MintedCredentials {
    /// How many accounts the signed-in user belongs to.
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }

    /// The account the key is for, rendered like the listing (`Acme (#316941)`).
    ///
    /// Shared so every place that names the account spells it the same way. `account_id` is always
    /// one of `accounts` — both come from the same `/accounts` response — so the lookup only
    /// misses for a hand-built value.
    pub fn account_label(&self) -> String {
        self.account_id
            .and_then(|id| self.accounts.iter().find(|a| a.id == id))
            .map(LoginAccount::label)
            .unwrap_or_else(|| "your current account".to_string())
    }
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
            .field("account_name", &self.account_name)
            .field("accounts", &self.accounts)
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
    ///
    /// Propagates [`AuthError::MfaRequired`] if the account is MFA-protected; use
    /// [`CloudAuthenticator::complete_login_with_mfa`] to supply codes interactively.
    pub async fn complete_login(
        &self,
        tokens: &TokenSet,
        key_name: &str,
        flow: LoginFlow,
        account: AccountChoice,
    ) -> Result<MintedCredentials, AuthError> {
        self.complete_login_with_mfa(tokens, key_name, flow, account, |_, _| Ok(None))
            .await
    }

    /// As [`CloudAuthenticator::complete_login`], but `mfa_prompt` is consulted when SM challenges
    /// the login for multi-factor authentication.
    ///
    /// `mfa_prompt(factors, attempt)` is called with the factor types SM offered (possibly empty)
    /// and a 1-based attempt number. Return `Some(code)` to submit a TOTP code, or `None` to give
    /// up — which surfaces the original [`AuthError::MfaRequired`] to the caller, the right
    /// behaviour when there is no terminal to prompt on.
    pub async fn complete_login_with_mfa<F>(
        &self,
        tokens: &TokenSet,
        key_name: &str,
        flow: LoginFlow,
        account: AccountChoice,
        mut mfa_prompt: F,
    ) -> Result<MintedCredentials, AuthError>
    where
        F: FnMut(&[String], u32) -> Result<Option<String>, AuthError>,
    {
        let mut sm =
            SmApiClient::with_http_client(self.sm_api_url.clone(), self.http.clone(), flow);
        // Google/GitHub logins must not send Sm-Id-Token (SSO-only); see sm_api docs.
        match sm.login(&tokens.access_token, None).await {
            Ok(()) => {}
            Err(AuthError::MfaRequired { factors }) => {
                self.satisfy_mfa(&mut sm, tokens, &factors, &mut mfa_prompt)
                    .await?
            }
            Err(e) => return Err(e),
        }
        let mut user = sm.fetch_current_user().await?;
        let want = match account {
            AccountChoice::Current => None,
            AccountChoice::Id(id) => Some(id),
            // The picker needs the list, so fetch it here; `switch_account` re-reads it to
            // validate, which also covers ids that did not come from a picker.
            AccountChoice::Prompt(choose) => {
                let accounts = login_accounts(&sm.fetch_accounts().await?);
                let current = user
                    .current_account_id
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok());
                Some(choose(&accounts, current)?)
            }
        };
        if let Some(want) = want {
            user = self.switch_account(&sm, user, want).await?;
        }
        sm.ensure_capi_enabled().await?;
        // Pick the account matching the logged-in user's current_account_id. /accounts list
        // order isn't guaranteed, so taking the first entry could mint a key for the wrong
        // account in a multi-account org. Fall back to the first only when it's absent/unknown.
        let accounts = sm.fetch_accounts().await?;
        let all_accounts = login_accounts(&accounts);
        let account = select_account(accounts, user.current_account_id.as_deref())
            .ok_or_else(|| AuthError::Protocol("no accounts associated with this login".into()))?;
        let account_name = account.name.clone();
        // Report the account the key belongs to, taken from the same entry the key came from.
        // `user.current_account_id` can disagree with it (absent or unknown → `select_account`
        // falls back), and printing that id would name an account the key is not for.
        let account_id = Some(account.id);
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
            account_id,
            email: user.email,
            api_key,
            api_secret: minted.secret_key,
            api_url: self.capi_url.clone(),
            refresh_token: tokens.refresh_token.clone(),
            capi_key_name: minted.name,
            redisctl_key_count,
            account_name,
            accounts: all_accounts,
        })
    }

    /// Point the session at `want` before anything account-scoped happens.
    ///
    /// Verifies the switch actually took rather than assuming it: every later call resolves the
    /// account from the session, so a silent no-op here would mint the key on the wrong account.
    async fn switch_account(
        &self,
        sm: &SmApiClient,
        user: SmUser,
        want: u64,
    ) -> Result<SmUser, AuthError> {
        if user.current_account_id.as_deref() == Some(want.to_string().as_str()) {
            return Ok(user);
        }
        // Fetched here rather than reusing the later call: membership has to be checked *before*
        // `setcurrent`, while the later fetch has to come *after* `ensure_capi_enabled` to see the
        // account access key it creates. Only runs when `--account` was given.
        let accounts = sm.fetch_accounts().await?;
        if !accounts.iter().any(|a| a.id == want) {
            if accounts.is_empty() {
                return Err(AuthError::Protocol(
                    "this login is not associated with any Redis Cloud account, so there is \
                     nothing to switch to"
                        .into(),
                ));
            }
            return Err(AuthError::UnknownAccount {
                requested: want,
                available: accounts
                    .iter()
                    .map(|a| {
                        LoginAccount {
                            id: a.id,
                            name: a.name.clone(),
                        }
                        .label()
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        }
        sm.set_current_account(want).await?;
        let user = sm.fetch_current_user().await?;
        // Trust the server's answer, not the request's success.
        if user.current_account_id.as_deref() != Some(want.to_string().as_str()) {
            return Err(AuthError::Protocol(format!(
                "asked Redis Cloud to switch to account {want} but the session still reports {}",
                user.current_account_id.as_deref().unwrap_or("none")
            )));
        }
        Ok(user)
    }

    /// Drive the MFA retry loop against an already-challenged client.
    async fn satisfy_mfa<F>(
        &self,
        sm: &mut SmApiClient,
        tokens: &TokenSet,
        factors: &[String],
        mfa_prompt: &mut F,
    ) -> Result<(), AuthError>
    where
        F: FnMut(&[String], u32) -> Result<Option<String>, AuthError>,
    {
        for attempt in 1..=MFA_MAX_ATTEMPTS {
            let Some(code) = mfa_prompt(factors, attempt)? else {
                // Caller can't prompt (no TTY / agent): report the challenge, not a failure.
                return Err(AuthError::MfaRequired {
                    factors: factors.to_vec(),
                });
            };
            match sm.complete_mfa(&tokens.access_token, None, &code).await {
                Ok(()) => return Ok(()),
                // Wrong code: loop and let the caller re-prompt, unless attempts are spent.
                Err(AuthError::MfaInvalidCode) if attempt < MFA_MAX_ATTEMPTS => continue,
                Err(e) => return Err(e),
            }
        }
        Err(AuthError::MfaInvalidCode)
    }
}

/// How many TOTP codes a single login will accept before giving up. SM enforces its own quota
/// (`mfa-quota-exceeded`); this only bounds our prompting.
const MFA_MAX_ATTEMPTS: u32 = 3;

/// Choose the account matching `current_account_id` (the logged-in user context); fall back to
/// the first account only when the id is absent or not present in the list.
/// Project the API's accounts into [`LoginAccount`]s, in a stable order.
///
/// `/accounts` order is not guaranteed. Sorting here rather than at each use means the picker's
/// numbering, the printed listing and the JSON `accounts` array all agree between runs — the last
/// of which callers are told to read for ids.
fn login_accounts(accounts: &[SmAccount]) -> Vec<LoginAccount> {
    let mut out: Vec<LoginAccount> = accounts
        .iter()
        .map(|a| LoginAccount {
            id: a.id,
            name: a.name.clone(),
        })
        .collect();
    out.sort_by_key(|a| a.id);
    out
}

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

    /// The label is shared by the CLI's account listing and the `UnknownAccount` message, so
    /// both read identically — including when the API reports no name.
    #[test]
    fn login_account_labels_named_and_unnamed_accounts() {
        assert_eq!(
            LoginAccount {
                id: 316941,
                name: Some("Acme".to_string()),
            }
            .label(),
            "Acme (#316941)"
        );
        assert_eq!(
            LoginAccount {
                id: 316941,
                name: None,
            }
            .label(),
            "#316941"
        );
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
            account_id: Some(42),
            email: Some("u@example.com".to_string()),
            api_key: "AKEY-visible-should-not-appear".to_string(),
            api_secret: "SECRET-should-not-appear".to_string(),
            api_url: "https://api.example.com/v1".to_string(),
            refresh_token: Some("RT-should-not-appear".to_string()),
            capi_key_name: "redisctl-cli-1".to_string(),
            redisctl_key_count: 3,
            account_name: Some("Acme".to_string()),
            accounts: vec![
                LoginAccount {
                    id: 316941,
                    name: Some("Acme".to_string()),
                },
                LoginAccount {
                    id: 481022,
                    name: Some("Contoso".to_string()),
                },
            ],
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
            .complete_login(
                &tokens,
                "redisctl-test",
                LoginFlow::Loopback,
                AccountChoice::Current,
            )
            .await
            .unwrap();
        assert_eq!(creds.api_key, "ACCT-KEY");
        assert_eq!(creds.api_secret, "SECRET");
        assert_eq!(creds.api_url, "https://capi.example/v1");
        assert_eq!(creds.account_id, Some(112117));
        assert_eq!(creds.email.as_deref(), Some("u@e.com"));
        assert_eq!(creds.refresh_token.as_deref(), Some("RT"));
        assert_eq!(creds.capi_key_name, "redisctl-test");
        // secrets must not leak via Debug
        let dbg = format!("{creds:?}");
        assert!(!dbg.contains("SECRET") && !dbg.contains("ACCT-KEY") && !dbg.contains("RT"));
    }

    fn tokens() -> TokenSet {
        TokenSet {
            access_token: "AT".to_string(),
            refresh_token: None,
            expires_in: 3600,
        }
    }

    fn authenticator(server: &MockServer) -> CloudAuthenticator {
        CloudAuthenticator::new(
            Url::parse("https://issuer.example/oauth2/default").unwrap(),
            "client",
            Url::parse(&server.uri()).unwrap(),
            "https://capi.example/v1",
        )
    }

    /// Everything the exchange needs apart from the account-shaped endpoints each test varies.
    async fn common_login_mocks(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({}))
                    .append_header("Set-Cookie", "JSESSIONID=SID; Path=/"),
            )
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/csrf"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"csrfToken": {"csrf_token": "C"}})),
            )
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiAccessKey"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"cloudApiAccessKey": {"accessKey": "ACCT"}})),
            )
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/accounts/cloud-api/cloudApiKeys"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"name": "redisctl-test", "secret_key": "SECRET"}),
            ))
            .mount(server)
            .await;
    }

    /// `--account` has to switch the session *before* anything account-scoped runs: both
    /// `ensure_capi_enabled` and the mint resolve the account from the session, so a switch that
    /// happened afterwards would put the key on the previous account. The mocks answer
    /// `/users/me` differently before and after `setcurrent`, so the minted account is only right
    /// if the ordering held.
    #[tokio::test]
    async fn complete_login_switches_before_minting() {
        let server = MockServer::start().await;
        let switched = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        common_login_mocks(&server).await;

        // /users/me reports 111 until setcurrent runs, then 222.
        let flag = switched.clone();
        Mock::given(method("GET"))
            .and(path("/users/me"))
            .respond_with(move |_: &wiremock::Request| {
                let id = if flag.load(std::sync::atomic::Ordering::SeqCst) {
                    "222"
                } else {
                    "111"
                };
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "1", "current_account_id": id, "email": "u@e.com"
                }))
            })
            .mount(&server)
            .await;
        let flag = switched.clone();
        Mock::given(method("POST"))
            .and(path("/accounts/setcurrent/222"))
            .respond_with(move |_: &wiremock::Request| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({}))
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [
                    {"id": 111, "name": "One", "api_access_key": "KEY-111"},
                    {"id": 222, "name": "Two", "api_access_key": "KEY-222"}
                ]
            })))
            .mount(&server)
            .await;

        let creds = authenticator(&server)
            .complete_login(
                &tokens(),
                "redisctl-test",
                LoginFlow::Loopback,
                AccountChoice::Id(222),
            )
            .await
            .unwrap();
        // The key, the reported id and the reported name all describe the requested account.
        assert_eq!(creds.account_id, Some(222));
        assert_eq!(creds.account_name.as_deref(), Some("Two"));
        assert_eq!(creds.api_key, "KEY-222");
        assert_eq!(creds.account_count(), 2);
    }

    /// The picker runs mid-exchange, on the one session. It must see every account in a stable
    /// order (the API does not promise one) along with the session's current account, and its
    /// answer must be what gets minted.
    #[tokio::test]
    async fn complete_login_mints_what_the_picker_chose() {
        let server = MockServer::start().await;
        let switched = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        common_login_mocks(&server).await;

        let flag = switched.clone();
        Mock::given(method("GET"))
            .and(path("/users/me"))
            .respond_with(move |_: &wiremock::Request| {
                let id = if flag.load(std::sync::atomic::Ordering::SeqCst) {
                    "111"
                } else {
                    "222"
                };
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "1", "current_account_id": id, "email": "u@e.com"
                }))
            })
            .mount(&server)
            .await;
        let flag = switched.clone();
        Mock::given(method("POST"))
            .and(path("/accounts/setcurrent/111"))
            .respond_with(move |_: &wiremock::Request| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({}))
            })
            .mount(&server)
            .await;
        // Deliberately returned highest-id-first, so a stable order cannot come from the API.
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [
                    {"id": 222, "name": "Two", "api_access_key": "KEY-222"},
                    {"id": 111, "name": "One", "api_access_key": "KEY-111"}
                ]
            })))
            .mount(&server)
            .await;

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_current = std::sync::Arc::new(std::sync::Mutex::new(None));
        let (s2, c2) = (seen.clone(), seen_current.clone());
        let creds = authenticator(&server)
            .complete_login(
                &tokens(),
                "k",
                LoginFlow::Switch,
                AccountChoice::Prompt(Box::new(move |accounts, current| {
                    *s2.lock().unwrap() = accounts.iter().map(|a| a.id).collect::<Vec<_>>();
                    *c2.lock().unwrap() = current;
                    Ok(111)
                })),
            )
            .await
            .unwrap();

        // Sorted by id despite the API's order, so a positional choice is stable between runs.
        assert_eq!(*seen.lock().unwrap(), vec![111, 222]);
        // The session's account is passed through for context.
        assert_eq!(*seen_current.lock().unwrap(), Some(222));
        // And the picker's answer is what got minted.
        assert_eq!(creds.account_id, Some(111));
        assert_eq!(creds.api_key, "KEY-111");
        assert_eq!(creds.account_label(), "One (#111)");
    }

    /// Refusing at the picker abandons the switch instead of minting something unasked for.
    #[tokio::test]
    async fn complete_login_propagates_a_declined_picker() {
        let server = MockServer::start().await;
        common_login_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path("/users/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id": "1", "current_account_id": "111", "email": "u@e.com"}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [{"id": 111, "name": "One", "api_access_key": "KEY-111"}]
            })))
            .mount(&server)
            .await;
        let err = authenticator(&server)
            .complete_login(
                &tokens(),
                "k",
                LoginFlow::Switch,
                AccountChoice::Prompt(Box::new(|_, _| {
                    Err(AuthError::AccountRequired("declined".into()))
                })),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::AccountRequired(_)), "got {err:?}");
    }

    /// An account the user does not belong to is refused before any switch is attempted, and the
    /// message names the accounts they do have.
    #[tokio::test]
    async fn complete_login_refuses_an_account_the_user_is_not_in() {
        let server = MockServer::start().await;
        common_login_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path("/users/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id": "1", "current_account_id": "111", "email": "u@e.com"}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [{"id": 111, "name": "One", "api_access_key": "KEY-111"}]
            })))
            .mount(&server)
            .await;
        // No /accounts/setcurrent/* mock is mounted: reaching one would 404 and surface as a
        // different error, which is itself the assertion that no switch was attempted.
        match authenticator(&server)
            .complete_login(&tokens(), "k", LoginFlow::Loopback, AccountChoice::Id(999))
            .await
        {
            Err(AuthError::UnknownAccount {
                requested,
                available,
            }) => {
                assert_eq!(requested, 999);
                assert_eq!(available, "One (#111)");
            }
            other => panic!("expected UnknownAccount, got {other:?}"),
        }
    }

    /// A `setcurrent` that reports success but leaves the session on the old account must fail
    /// loudly: continuing would mint the key on the wrong account and report success.
    #[tokio::test]
    async fn complete_login_fails_when_the_switch_does_not_take() {
        let server = MockServer::start().await;
        common_login_mocks(&server).await;
        // Never changes, however many times it is asked.
        Mock::given(method("GET"))
            .and(path("/users/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id": "1", "current_account_id": "111", "email": "u@e.com"}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/accounts/setcurrent/222"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [
                    {"id": 111, "name": "One", "api_access_key": "KEY-111"},
                    {"id": 222, "name": "Two", "api_access_key": "KEY-222"}
                ]
            })))
            .mount(&server)
            .await;
        let err = authenticator(&server)
            .complete_login(&tokens(), "k", LoginFlow::Loopback, AccountChoice::Id(222))
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthError::Protocol(ref m) if m.contains("still reports")),
            "expected a switch-verification failure, got {err:?}"
        );
    }

    /// Asking for the account the session is already on must not issue a switch at all.
    #[tokio::test]
    async fn complete_login_skips_the_switch_when_already_current() {
        let server = MockServer::start().await;
        common_login_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path("/users/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id": "1", "current_account_id": "111", "email": "u@e.com"}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [{"id": 111, "name": "One", "api_access_key": "KEY-111"}]
            })))
            .mount(&server)
            .await;
        // Again, no setcurrent mount: if one were issued it would 404 and fail the login.
        let creds = authenticator(&server)
            .complete_login(&tokens(), "k", LoginFlow::Loopback, AccountChoice::Id(111))
            .await
            .unwrap();
        assert_eq!(creds.account_id, Some(111));
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
            auth.complete_login(&tokens, "k", LoginFlow::Loopback, AccountChoice::Current)
                .await,
            Err(AuthError::Protocol(_))
        ));
    }
}
