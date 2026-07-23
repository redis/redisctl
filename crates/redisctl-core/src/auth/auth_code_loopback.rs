//! OIDC Authorization Code + PKCE via a loopback redirect (RFC 8252) — the interactive
//! human login. The CLI opens the browser to the Okta sign-in page and self-hosts a
//! one-shot `127.0.0.1` listener to catch the redirect; **no callback page is hosted
//! anywhere** and the authorization code never leaves the machine.
//!
//! Flow: bind a loopback port → build PKCE (S256) + `state` → hand the authorize URL to the
//! caller's `open_browser` → wait for the redirect on the listener → validate `state` →
//! exchange the code (+ `code_verifier`) for tokens. Converges on the same [`TokenSet`] and
//! token-endpoint plumbing as the device flow.

use std::net::Ipv4Addr;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

use super::{AuthError, TokenSet, default_http_client, endpoint, form_body, post_token, token_set};

const AUTH_CODE_GRANT: &str = "authorization_code";
/// Loopback ports tried in order. Each `http://127.0.0.1:PORT/callback` must be registered
/// as a redirect URI on the Okta app (or the app configured to allow any loopback port).
const DEFAULT_PORTS: &[u16] = &[8899, 8898, 8900];
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// Deadline for reading the request line *after* a connection is accepted, so a local client
/// that connects but never finishes the request can't hang the login indefinitely.
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Authorization-code-with-PKCE client using a loopback redirect.
#[derive(Clone)]
pub struct LoopbackFlowClient {
    issuer: Url,
    client_id: String,
    http: reqwest::Client,
    ports: Vec<u16>,
    redirect_host: String,
    redirect_path: String,
    timeout: Duration,
}

impl LoopbackFlowClient {
    /// Build a client for the given issuer and public client id, with sensible loopback
    /// defaults (`127.0.0.1`, ports 8899/8898/8900, `/callback`, 5-minute timeout).
    pub fn new(issuer: Url, client_id: impl Into<String>) -> Self {
        Self {
            issuer,
            client_id: client_id.into(),
            http: default_http_client(),
            ports: DEFAULT_PORTS.to_vec(),
            redirect_host: "127.0.0.1".to_string(),
            redirect_path: "/callback".to_string(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Use a caller-provided reqwest client.
    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    /// Override the candidate loopback ports (e.g. `[0]` for an ephemeral port in tests).
    pub fn with_ports(mut self, ports: Vec<u16>) -> Self {
        self.ports = ports;
        self
    }

    /// Override how long to wait for the browser redirect.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Run the interactive login. `open_browser` is invoked with the authorize URL (the
    /// caller opens it or prints it); the method then blocks on the loopback redirect.
    pub async fn login<F>(&self, scopes: &[&str], open_browser: F) -> Result<TokenSet, AuthError>
    where
        F: FnOnce(&str),
    {
        let (listener, port) = self.bind().await?;
        let redirect_uri = format!(
            "http://{}:{}{}",
            self.redirect_host, port, self.redirect_path
        );

        let pkce = Pkce::generate()?;
        let state = random_urlsafe(16)?;
        let url = self.authorize_url(&redirect_uri, scopes, &pkce.challenge, &state);

        open_browser(&url);

        let code = self.wait_for_callback(listener, &state).await?;
        self.exchange_code(&code, &pkce.verifier, &redirect_uri)
            .await
    }

    async fn bind(&self) -> Result<(TcpListener, u16), AuthError> {
        for &port in &self.ports {
            if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await {
                let actual = listener
                    .local_addr()
                    .map_err(|e| AuthError::Protocol(format!("could not read local address: {e}")))?
                    .port();
                return Ok((listener, actual));
            }
        }
        Err(AuthError::Protocol(format!(
            "could not bind a loopback port (tried {:?})",
            self.ports
        )))
    }

    fn authorize_url(
        &self,
        redirect_uri: &str,
        scopes: &[&str],
        challenge: &str,
        state: &str,
    ) -> String {
        let scope = scopes.join(" ");
        let query = form_body(&[
            ("client_id", self.client_id.as_str()),
            ("response_type", "code"),
            ("scope", scope.as_str()),
            ("redirect_uri", redirect_uri),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("prompt", "login"),
        ]);
        format!("{}?{}", endpoint(&self.issuer, "v1/authorize"), query)
    }

    async fn wait_for_callback(
        &self,
        listener: TcpListener,
        expected_state: &str,
    ) -> Result<String, AuthError> {
        let accepted = tokio::time::timeout(self.timeout, listener.accept())
            .await
            .map_err(|_| {
                AuthError::Protocol("timed out waiting for the browser redirect".into())
            })?;
        let (mut stream, _) =
            accepted.map_err(|e| AuthError::Protocol(format!("accept failed: {e}")))?;

        let target = tokio::time::timeout(REQUEST_READ_TIMEOUT, read_request_target(&mut stream))
            .await
            .map_err(|_| {
                AuthError::Protocol("timed out reading the browser redirect request".into())
            })??;
        write_ok_page(&mut stream).await;

        // The request target is a path+query; parse it against a dummy base to read the query.
        let parsed = Url::parse(&format!("http://localhost{target}"))
            .map_err(|e| AuthError::Protocol(format!("could not parse callback URL: {e}")))?;
        let (mut code, mut state, mut error, mut error_desc) = (None, None, None, None);
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "code" => code = Some(v.into_owned()),
                "state" => state = Some(v.into_owned()),
                "error" => error = Some(v.into_owned()),
                "error_description" => error_desc = Some(v.into_owned()),
                _ => {}
            }
        }

        if let Some(err) = error {
            return match err.as_str() {
                "access_denied" => Err(AuthError::Denied),
                other => Err(AuthError::Protocol(format!(
                    "authorization error {other}: {}",
                    error_desc.unwrap_or_default()
                ))),
            };
        }
        if state.as_deref() != Some(expected_state) {
            return Err(AuthError::Protocol(
                "state mismatch on callback (possible CSRF or stale login)".into(),
            ));
        }
        code.ok_or_else(|| {
            AuthError::Protocol("callback did not include an authorization code".into())
        })
    }

    async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, AuthError> {
        let tr = post_token(
            &self.http,
            &endpoint(&self.issuer, "v1/token"),
            &[
                ("client_id", self.client_id.as_str()),
                ("grant_type", AUTH_CODE_GRANT),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("code_verifier", verifier),
            ],
        )
        .await?;
        if let Some(err) = tr.error.as_deref() {
            return Err(AuthError::Protocol(format!(
                "token exchange failed ({err}): {}",
                tr.description()
            )));
        }
        token_set(tr)
    }
}

/// A PKCE verifier/challenge pair (S256).
struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    fn generate() -> Result<Self, AuthError> {
        let verifier = random_urlsafe(32)?; // 32 bytes -> 43-char base64url (valid PKCE length)
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Ok(Self {
            verifier,
            challenge,
        })
    }
}

/// `n` random bytes, base64url-no-pad encoded — used for the PKCE verifier and `state`.
fn random_urlsafe(n: usize) -> Result<String, AuthError> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf)
        .map_err(|e| AuthError::Protocol(format!("secure RNG unavailable: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(&buf))
}

/// Read the HTTP request line's target (the path+query) from the loopback connection.
async fn read_request_target(stream: &mut TcpStream) -> Result<String, AuthError> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|e| AuthError::Protocol(format!("reading callback request failed: {e}")))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 16 * 1024 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let request_line = text.lines().next().unwrap_or_default();
    // e.g. "GET /callback?code=...&state=... HTTP/1.1"
    request_line
        .split_whitespace()
        .nth(1)
        .map(|s| s.to_string())
        .ok_or_else(|| AuthError::Protocol("malformed callback request line".into()))
}

/// Write a minimal "you can close this tab" page and close the connection.
async fn write_ok_page(stream: &mut TcpStream) {
    const BODY: &str = "<html><body style=\"font-family:sans-serif\">\
        <h2>Signed in - you can close this tab.</h2></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        BODY.len(),
        BODY
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn pkce_challenge_matches_verifier() {
        let p = Pkce::generate().unwrap();
        assert!((43..=128).contains(&p.verifier.len()));
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifier.as_bytes()));
        assert_eq!(p.challenge, expected);
    }

    #[test]
    fn random_urlsafe_is_urlsafe_and_unique() {
        let a = random_urlsafe(16).unwrap();
        let b = random_urlsafe(16).unwrap();
        assert_ne!(a, b);
        assert!(!a.contains('+') && !a.contains('/') && !a.contains('='));
    }

    fn client_with_issuer(issuer: &str) -> LoopbackFlowClient {
        LoopbackFlowClient::new(Url::parse(issuer).unwrap(), "cid")
    }

    #[test]
    fn authorize_url_has_required_params() {
        let c = client_with_issuer("https://issuer.example/oauth2/default");
        let url = c.authorize_url(
            "http://127.0.0.1:8899/callback",
            &["openid", "email"],
            "CHALLENGE",
            "STATE",
        );
        assert!(url.starts_with("https://issuer.example/oauth2/default/v1/authorize?"));
        let parsed = Url::parse(&url).unwrap();
        let q: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "cid");
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["code_challenge"], "CHALLENGE");
        assert_eq!(q["state"], "STATE");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:8899/callback");
        assert_eq!(q["scope"], "openid email");
        assert_eq!(q["prompt"], "login");
    }

    async fn ephemeral(server: &MockServer) -> LoopbackFlowClient {
        LoopbackFlowClient::new(Url::parse(&server.uri()).unwrap(), "cid")
            .with_ports(vec![0])
            .with_timeout(Duration::from_secs(5))
    }

    /// Simulate the browser: read redirect_uri + state from the authorize URL and GET the
    /// callback with the given extra query.
    fn simulate_browser(url: &str, extra: &str) {
        let parsed = Url::parse(url).unwrap();
        let q: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        let cb = format!("{}?{}&state={}", q["redirect_uri"], extra, q["state"]);
        tokio::spawn(async move {
            let _ = reqwest::get(&cb).await;
        });
    }

    #[tokio::test]
    async fn login_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "AT",
                "id_token": "IT",
                "refresh_token": "RT",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;

        let token = ephemeral(&server)
            .await
            .login(&["openid"], |url| simulate_browser(url, "code=THECODE"))
            .await
            .unwrap();
        assert_eq!(token.access_token, "AT");
        assert_eq!(token.refresh_token.as_deref(), Some("RT"));
    }

    #[tokio::test]
    async fn login_state_mismatch_is_error() {
        let server = MockServer::start().await;
        let res = ephemeral(&server)
            .await
            .login(&["openid"], |url| {
                // GET the callback with a wrong state (ignore the real one)
                let parsed = Url::parse(url).unwrap();
                let q: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
                let cb = format!("{}?code=X&state=WRONG", q["redirect_uri"]);
                tokio::spawn(async move {
                    let _ = reqwest::get(&cb).await;
                });
            })
            .await;
        assert!(matches!(res, Err(AuthError::Protocol(_))));
    }

    #[tokio::test]
    async fn login_access_denied_maps_to_denied() {
        let server = MockServer::start().await;
        let res = ephemeral(&server)
            .await
            .login(&["openid"], |url| {
                simulate_browser(url, "error=access_denied")
            })
            .await;
        assert!(matches!(res, Err(AuthError::Denied)));
    }

    #[tokio::test]
    async fn login_times_out_without_callback() {
        let server = MockServer::start().await;
        let res = ephemeral(&server)
            .await
            .with_timeout(Duration::from_millis(150))
            .login(&["openid"], |_url| { /* browser never redirects */ })
            .await;
        assert!(matches!(res, Err(AuthError::Protocol(_))));
    }
}
