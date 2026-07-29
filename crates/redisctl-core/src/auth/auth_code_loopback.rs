//! OIDC Authorization Code + PKCE via a loopback redirect (RFC 8252) — the interactive
//! human login. The CLI opens the browser to the Okta sign-in page and self-hosts a
//! `127.0.0.1` listener to catch the redirect; **no callback page is hosted anywhere** and the
//! authorization code never leaves the machine.
//!
//! Sequence:
//! 1. Bind a loopback port on 127.0.0.1 (the redirect target; no page is hosted anywhere).
//! 2. Build a PKCE S256 verifier/challenge + a random `state` (both from the [`oauth2`] crate).
//! 3. Hand the `/v1/authorize` URL to the caller's `open_browser`; the user signs in.
//! 4. Catch the browser's redirect to `127.0.0.1/callback?code&state` on the listener.
//! 5. Validate `state` (CSRF guard) and extract the code — *before* rendering any success page.
//! 6. Exchange the code (+ `code_verifier`) at `/v1/token` for a `TokenSet`.
//!
//! Converges on the same [`TokenSet`] and the shared `oidc` plumbing as the device flow.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use oauth2::{AuthorizationCode, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

use super::oidc::{map_basic_token_error, oauth_http_client, okta_client, to_token_set};
use super::{AuthError, TokenSet};

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
            ports: DEFAULT_PORTS.to_vec(),
            redirect_host: "127.0.0.1".to_string(),
            redirect_path: "/callback".to_string(),
            timeout: DEFAULT_TIMEOUT,
        }
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

        let client = okta_client(&self.issuer, &self.client_id)?.set_redirect_uri(
            RedirectUrl::new(redirect_uri.clone())
                .map_err(|e| AuthError::Protocol(format!("invalid redirect URL: {e}")))?,
        );

        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(challenge)
            .add_extra_param("prompt", "login");
        for scope in scopes {
            request = request.add_scope(Scope::new((*scope).to_string()));
        }
        let (url, csrf) = request.url();

        open_browser(url.as_str());

        let code = self.wait_for_callback(listener, csrf.secret()).await?;

        let http = oauth_http_client()?;
        let resp = client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(verifier)
            .request_async(&http)
            .await
            .map_err(map_basic_token_error)?;
        Ok(to_token_set(&resp))
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

    /// Wait for the browser's redirect, tolerating stray local connections. Two security
    /// properties this upholds:
    /// - The success page is written **only after** `state` (and any `error`) is validated, so a
    ///   forged/mismatched callback never sees a "signed in" page.
    /// - The listener keeps `accept()`ing (bounded by `self.timeout`) so an unrelated local
    ///   request — which carries no `state` — is answered briefly and does not end the login; a
    ///   real callback with a mismatched `state` is still a terminal CSRF rejection.
    async fn wait_for_callback(
        &self,
        listener: TcpListener,
        expected_state: &str,
    ) -> Result<String, AuthError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AuthError::Protocol(
                    "timed out waiting for the browser redirect".into(),
                ));
            }

            let (mut stream, _) = match tokio::time::timeout(remaining, listener.accept()).await {
                Err(_) => {
                    return Err(AuthError::Protocol(
                        "timed out waiting for the browser redirect".into(),
                    ));
                }
                Ok(Err(e)) => return Err(AuthError::Protocol(format!("accept failed: {e}"))),
                Ok(Ok(pair)) => pair,
            };

            // A connection that never completes its request must not hang the login: bound the
            // read; on failure, answer briefly and keep waiting for the real callback.
            let target =
                match tokio::time::timeout(REQUEST_READ_TIMEOUT, read_request_target(&mut stream))
                    .await
                {
                    Ok(Ok(t)) => t,
                    _ => {
                        write_page(
                            &mut stream,
                            400,
                            "Bad Request",
                            "Could not read the request.",
                        )
                        .await;
                        continue;
                    }
                };

            // The request target is a path+query; parse it against a dummy base to read the query.
            let parsed = match Url::parse(&format!("http://localhost{target}")) {
                Ok(u) => u,
                Err(_) => {
                    write_page(&mut stream, 400, "Bad Request", "Malformed callback URL.").await;
                    continue;
                }
            };
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

            // Validate BEFORE writing any success page. An explicit IdP `error` is a terminal
            // outcome for this login.
            if let Some(err) = error {
                write_page(
                    &mut stream,
                    400,
                    "Bad Request",
                    "Login failed. You can close this tab.",
                )
                .await;
                return match err.as_str() {
                    "access_denied" => Err(AuthError::Denied),
                    other => Err(AuthError::Protocol(format!(
                        "authorization error {other}: {}",
                        error_desc.unwrap_or_default()
                    ))),
                };
            }

            match state.as_deref() {
                // Our callback with a matching state: only now is a success page correct.
                Some(s) if s == expected_state => {
                    return match code {
                        Some(c) => {
                            write_page(
                                &mut stream,
                                200,
                                "OK",
                                "Signed in - you can close this tab.",
                            )
                            .await;
                            Ok(c)
                        }
                        None => {
                            write_page(
                                &mut stream,
                                400,
                                "Bad Request",
                                "Login failed. You can close this tab.",
                            )
                            .await;
                            Err(AuthError::Protocol(
                                "callback did not include an authorization code".into(),
                            ))
                        }
                    };
                }
                // A callback carrying a mismatched state → CSRF / stale login. Reject with an
                // error page, never a success page.
                Some(_) => {
                    write_page(
                        &mut stream,
                        400,
                        "Bad Request",
                        "Login failed (state mismatch). You can close this tab.",
                    )
                    .await;
                    return Err(AuthError::Protocol(
                        "state mismatch on callback (possible CSRF or stale login)".into(),
                    ));
                }
                // No state at all → a stray/unrelated local request. Answer briefly and keep
                // waiting for the real callback.
                None => {
                    write_page(
                        &mut stream,
                        404,
                        "Not Found",
                        "Waiting for the login callback.",
                    )
                    .await;
                    continue;
                }
            }
        }
    }
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

/// Write a minimal HTML page and close the connection. Used for both the success page and the
/// neutral error/"still waiting" pages, so the wording is decided by the caller after validation.
async fn write_page(stream: &mut TcpStream, status: u16, reason: &str, message: &str) {
    let body =
        format!("<html><body style=\"font-family:sans-serif\"><h2>{message}</h2></body></html>");
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "AT",
                "token_type": "Bearer",
                "refresh_token": "RT",
                "expires_in": 3600
            })))
            .mount(server)
            .await;
    }

    async fn ephemeral(server: &MockServer) -> LoopbackFlowClient {
        LoopbackFlowClient::new(Url::parse(&server.uri()).unwrap(), "cid")
            .with_ports(vec![0])
            .with_timeout(Duration::from_secs(5))
    }

    fn query_of(url: &str) -> HashMap<String, String> {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    #[tokio::test]
    async fn login_happy_path_and_authorize_url_params() {
        let server = MockServer::start().await;
        mount_token(&server).await;

        let captured = Arc::new(Mutex::new(String::new()));
        let cap = captured.clone();
        let token = ephemeral(&server)
            .await
            .login(&["openid", "email"], move |url| {
                *cap.lock().unwrap() = url.to_string();
                let q = query_of(url);
                let cb = format!("{}?code=THECODE&state={}", q["redirect_uri"], q["state"]);
                tokio::spawn(async move {
                    let _ = reqwest::get(&cb).await;
                });
            })
            .await
            .unwrap();
        assert_eq!(token.access_token, "AT");
        assert_eq!(token.refresh_token.as_deref(), Some("RT"));

        // The authorize URL the browser was handed carries the PKCE + CSRF params.
        let url = captured.lock().unwrap().clone();
        assert!(url.contains("/v1/authorize?"));
        let q = query_of(&url);
        assert_eq!(q["client_id"], "cid");
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["code_challenge_method"], "S256");
        assert!(q.contains_key("code_challenge"));
        assert!(q.contains_key("state"));
        assert_eq!(q["prompt"], "login");
        assert_eq!(q["scope"], "openid email");
        assert!(q["redirect_uri"].starts_with("http://127.0.0.1:"));
        assert!(q["redirect_uri"].ends_with("/callback"));
    }

    /// Bug fix (a): a callback with a mismatched `state` is rejected AND never receives a
    /// success page.
    #[tokio::test]
    async fn login_state_mismatch_is_rejected_without_success_page() {
        let server = MockServer::start().await;
        mount_token(&server).await;

        // Capture the body the "browser" receives so we can assert it is not the success page.
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        let res = ephemeral(&server)
            .await
            .login(&["openid"], move |url| {
                let q = query_of(url);
                let cb = format!("{}?code=X&state=WRONG-STATE", q["redirect_uri"]);
                tokio::spawn(async move {
                    let body = match reqwest::get(&cb).await {
                        Ok(r) => r.text().await.unwrap_or_default(),
                        Err(_) => String::new(),
                    };
                    let _ = tx.send(body);
                });
            })
            .await;

        assert!(matches!(res, Err(AuthError::Protocol(_))));
        let body = rx.await.unwrap();
        assert!(
            !body.contains("Signed in"),
            "mismatched state must not get a success page, got: {body}"
        );
    }

    #[tokio::test]
    async fn login_access_denied_maps_to_denied() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let res = ephemeral(&server)
            .await
            .login(&["openid"], |url| {
                let q = query_of(url);
                let cb = format!(
                    "{}?error=access_denied&state={}",
                    q["redirect_uri"], q["state"]
                );
                tokio::spawn(async move {
                    let _ = reqwest::get(&cb).await;
                });
            })
            .await;
        assert!(matches!(res, Err(AuthError::Denied)));
    }

    #[tokio::test]
    async fn login_times_out_without_callback() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let res = ephemeral(&server)
            .await
            .with_timeout(Duration::from_millis(150))
            .login(&["openid"], |_url| { /* browser never redirects */ })
            .await;
        assert!(matches!(res, Err(AuthError::Protocol(_))));
    }

    /// Bug fix (b): a stray request to the loopback port must not end the flow — the real
    /// callback still succeeds.
    #[tokio::test]
    async fn login_ignores_stray_request() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let token = ephemeral(&server)
            .await
            .login(&["openid"], |url| {
                let q = query_of(url);
                let redirect = q["redirect_uri"].clone();
                let state = q["state"].clone();
                // A stray, unrelated local request (no `state`) arrives first.
                let stray = redirect.clone();
                tokio::spawn(async move {
                    let _ = reqwest::get(&stray).await;
                });
                // The legitimate callback follows shortly after.
                let real = format!("{redirect}?code=THECODE&state={state}");
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    let _ = reqwest::get(&real).await;
                });
            })
            .await
            .unwrap();
        assert_eq!(token.access_token, "AT");
    }
}
