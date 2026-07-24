//! `redisctl cloud auth login | status | logout`
//!
//! `login` is a credential bootstrapper: it runs an OIDC flow, exchanges the tokens with the
//! SM API to mint a CAPI key, and writes a normal cloud profile. Afterward every existing
//! `cloud` command works.
//!
//! Two shapes, matching the agent model:
//! - **Browser (loopback)** — the default on an interactive terminal: a single blocking
//!   `login` opens the browser and completes.
//! - **Device flow** — headless / `--device`: `login` *initiates* and returns the code
//!   immediately (non-blocking), so an agent can relay it; `status --wait` then blocks until
//!   the user approves, runs the SM exchange, and persists. `login --device --wait` collapses
//!   both into one blocking call for a human.

#![allow(dead_code)] // Used by binary target

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use redisctl_core::auth::{CloudAuthenticator, MintedCredentials};
use redisctl_core::{CloudAuthConfig, Config, CredentialStore, DeviceAuthorization, TokenSet};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::cli::CloudAuthCommands;
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use crate::output::{OutputFormat, print_formatted_output};
use crate::structured_error::StructuredError;

const SCOPES: [&str; 4] = ["openid", "profile", "email", "offline_access"];

/// Above this many `redisctl-*` CAPI keys on an account, `login` nudges the user to revoke
/// unused ones (a normal user has 1–3; more suggests login/logout cycles without cleanup).
const STALE_KEY_WARN_THRESHOLD: usize = 3;

/// Wrap an OIDC/SM `AuthError` into the structured exit contract (stable code + exit code).
fn auth_err(e: redisctl_core::AuthError) -> RedisCtlError {
    RedisCtlError::Structured(Box::new(StructuredError::from(e)))
}

pub async fn handle_auth_command(
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
    cmd: &CloudAuthCommands,
    output: OutputFormat,
) -> CliResult<()> {
    match cmd {
        CloudAuthCommands::Login {
            device,
            allow_plaintext,
            wait,
        } => login(conn_mgr, profile, *device, *allow_plaintext, *wait, output).await,
        CloudAuthCommands::Status { wait, timeout } => {
            status(conn_mgr, profile, *wait, *timeout, output).await
        }
        CloudAuthCommands::Logout => logout(conn_mgr, profile, output),
    }
}

/// The profile a login targets: `--profile`, else the configured default cloud profile, else
/// a conventional `cloud`.
fn target_profile(conn_mgr: &ConnectionManager, profile: Option<&str>) -> String {
    profile
        .map(str::to_string)
        .or_else(|| conn_mgr.config.default_cloud.clone())
        .unwrap_or_else(|| "cloud".to_string())
}

/// Resolve the login endpoints for `profile` and build an authenticator, or a structured
/// `not_authenticated` error if the environment isn't provisioned for login.
fn prepare(
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
) -> CliResult<(String, CloudAuthenticator, CloudAuthConfig)> {
    let profile_name = target_profile(conn_mgr, profile);
    let auth_cfg = conn_mgr.config.resolve_cloud_auth(&profile_name);
    if !auth_cfg.is_complete() {
        return Err(RedisCtlError::Structured(Box::new(
            StructuredError::not_authenticated(format!(
                "cloud login endpoints are not configured for profile '{profile_name}'. Add a \
                 [cloud_auth.{profile_name}] section (okta_issuer, okta_client_id, sm_api_url) to \
                 the config, or use a profile whose environment is provisioned."
            )),
        )));
    }
    let issuer = parse_url(&auth_cfg.okta_issuer, "okta_issuer")?;
    let sm_api_url = parse_url(&auth_cfg.sm_api_url, "sm_api_url")?;
    let authenticator = CloudAuthenticator::new(
        issuer,
        &auth_cfg.okta_client_id,
        sm_api_url,
        &auth_cfg.capi_url,
    );
    Ok((profile_name, authenticator, auth_cfg))
}

async fn login(
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
    device: bool,
    allow_plaintext: bool,
    wait: bool,
    output: OutputFormat,
) -> CliResult<()> {
    let (profile_name, authenticator, auth_cfg) = prepare(conn_mgr, profile)?;

    // Device flow is used with --device or when stderr isn't a TTY (headless / agent / piped);
    // otherwise the interactive browser (loopback) flow.
    let use_device = device || !std::io::stderr().is_terminal();

    // Non-blocking device initiate (the agent path): return the code now, complete via
    // `auth status --wait`. `--wait` (or the loopback flow) blocks through to completion.
    if use_device && !wait {
        return initiate_device_login(
            conn_mgr,
            &profile_name,
            &authenticator,
            allow_plaintext,
            output,
        )
        .await;
    }

    let tokens = if use_device {
        run_device_flow_blocking(&authenticator).await?
    } else {
        run_loopback_flow(&authenticator).await?
    };
    let creds = complete_and_persist(
        conn_mgr,
        &profile_name,
        &authenticator,
        &tokens,
        allow_plaintext,
        auth_cfg,
    )
    .await?;
    emit_signed_in(&creds, &profile_name, output)
}

/// Start the device flow, save a pending record, and return the device code to the caller
/// (agent) without blocking. Completion happens later via `auth status --wait`.
async fn initiate_device_login(
    conn_mgr: &ConnectionManager,
    profile_name: &str,
    authenticator: &CloudAuthenticator,
    allow_plaintext: bool,
    output: OutputFormat,
) -> CliResult<()> {
    let authz = authenticator
        .device()
        .start(&SCOPES)
        .await
        .map_err(auth_err)?;
    let verify = authz
        .verification_uri_complete()
        .unwrap_or_else(|| authz.verification_uri());

    save_pending(
        conn_mgr,
        &PendingAuth {
            profile: profile_name.to_string(),
            device_authorization: authz.clone(),
            allow_plaintext,
        },
    )?;

    eprintln!(
        "\nTo sign in, open:\n  {verify}\nand confirm the code:  {}\nThen run: redisctl cloud \
         auth status --wait   (or wait for the agent to poll)",
        authz.user_code()
    );
    print_formatted_output(
        serde_json::json!({
            "status": "authorization_pending",
            "profile": profile_name,
            "verification_uri": authz.verification_uri(),
            "verification_uri_complete": authz.verification_uri_complete(),
            "user_code": authz.user_code(),
            "expires_in": authz.expires_in(),
            "interval": authz.interval(),
        }),
        output,
    )
}

async fn run_device_flow_blocking(auth: &CloudAuthenticator) -> CliResult<TokenSet> {
    let client = auth.device();
    let authz = client.start(&SCOPES).await.map_err(auth_err)?;
    let verify = authz
        .verification_uri_complete()
        .unwrap_or_else(|| authz.verification_uri());
    eprintln!(
        "\nTo sign in, open:\n  {verify}\nand confirm the code:  {}\n(waiting for approval…)",
        authz.user_code()
    );

    // oauth2 owns the poll loop (honoring the server's interval / slow_down); `None` polls until
    // the device code's own lifetime expires, which surfaces as `device_code_expired` via auth_err.
    client.poll(&authz, None).await.map_err(auth_err)
}

async fn run_loopback_flow(auth: &CloudAuthenticator) -> CliResult<TokenSet> {
    let tokens = auth
        .loopback()
        .login(&SCOPES, |url| {
            eprintln!("Opening your browser to sign in…\n  {url}");
            let _ = open_browser(url);
        })
        .await
        .map_err(auth_err)?;
    Ok(tokens)
}

/// Run the SM exchange, persist the profile + secrets, and return the minted credentials.
async fn complete_and_persist(
    conn_mgr: &ConnectionManager,
    profile_name: &str,
    authenticator: &CloudAuthenticator,
    tokens: &TokenSet,
    allow_plaintext: bool,
    auth_cfg: CloudAuthConfig,
) -> CliResult<MintedCredentials> {
    let creds = authenticator
        .complete_login(tokens, &default_key_name())
        .await
        .map_err(auth_err)?;

    let store = if allow_plaintext {
        CredentialStore::plaintext()
    } else {
        CredentialStore::new()
    };
    let mut config = conn_mgr.config.clone();
    // On the keyring path, a store failure means the OS secret service is unavailable (D4):
    // surface a distinct `keyring_unavailable` (exit 2) pointing at `--allow-plaintext`.
    config
        .apply_cloud_login(&store, profile_name, &creds, Some(auth_cfg))
        .map_err(|e| {
            if allow_plaintext {
                RedisCtlError::from(e)
            } else {
                RedisCtlError::Structured(Box::new(StructuredError::keyring_unavailable(format!(
                    "failed to store credentials in the OS keyring ({e}). Re-run \
                     `redisctl cloud auth login --allow-plaintext` to store them in the config \
                     file (0600) instead."
                ))))
            }
        })?;
    save_config(conn_mgr, &config)?;
    Ok(creds)
}

fn emit_signed_in(
    creds: &MintedCredentials,
    profile_name: &str,
    output: OutputFormat,
) -> CliResult<()> {
    eprintln!(
        "\n\u{2713} Signed in as {}. Credentials saved to profile '{}'.",
        creds.email.as_deref().unwrap_or("your account"),
        profile_name
    );
    // D5: each login mints a new redisctl-* CAPI key. Warn (don't delete) when they pile up.
    let key_count = creds.redisctl_key_count;
    if key_count > STALE_KEY_WARN_THRESHOLD {
        eprintln!(
            "  note: this account now has {key_count} redisctl-* API keys. Revoke unused ones \
             in the Redis Cloud console (Access Management > API Keys)."
        );
    }
    print_formatted_output(
        serde_json::json!({
            "status": "ok",
            "authenticated": true,
            "profile": profile_name,
            "account_id": creds.account_id,
            "email": creds.email,
            "redisctl_key_count": key_count,
        }),
        output,
    )
}

async fn status(
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
    wait: bool,
    timeout: u64,
    output: OutputFormat,
) -> CliResult<()> {
    let profile_name = target_profile(conn_mgr, profile);

    // --wait completes a pending device-authorization login (the agent path): poll, exchange,
    // persist. With no pending record it falls through to a plain status report.
    if wait && let Some(pending) = load_pending(conn_mgr, &profile_name) {
        let (_, authenticator, auth_cfg) = prepare(conn_mgr, Some(&profile_name))?;
        match poll_pending(&authenticator, &pending, timeout).await? {
            Some(tokens) => {
                let creds = complete_and_persist(
                    conn_mgr,
                    &profile_name,
                    &authenticator,
                    &tokens,
                    pending.allow_plaintext,
                    auth_cfg,
                )
                .await?;
                clear_pending(conn_mgr, &profile_name);
                return emit_signed_in(&creds, &profile_name, output);
            }
            None => {
                // Timed out but the code may still be valid — keep the pending record so a
                // later `status --wait` can resume; report the pending state (exit 0).
                return print_formatted_output(
                    serde_json::json!({
                        "status": "authorization_pending",
                        "authenticated": false,
                        "profile": profile_name,
                    }),
                    output,
                );
            }
        }
    }

    let authenticated = conn_mgr
        .resolve_cloud_connection(Some(&profile_name))
        .is_ok();
    print_formatted_output(
        serde_json::json!({ "authenticated": authenticated, "profile": profile_name }),
        output,
    )
}

/// Poll a pending device authorization to completion. `Ok(Some)` = approved (tokens),
/// `Ok(None)` = local `--timeout` elapsed while still pending. `expired`/`denied` from the
/// server propagate as structured errors.
async fn poll_pending(
    auth: &CloudAuthenticator,
    pending: &PendingAuth,
    timeout_secs: u64,
) -> CliResult<Option<TokenSet>> {
    let client = auth.device();
    // oauth2 owns the poll loop; bound it by the caller's `--timeout`. If our wait elapses first
    // the login is still pending (`Ok(None)`); a real expiry/denial from the server surfaces as a
    // structured error. This keeps "your wait elapsed" distinct from "the device code expired".
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client.poll(&pending.device_authorization, None),
    )
    .await
    {
        Err(_elapsed) => Ok(None),
        Ok(result) => result.map(Some).map_err(auth_err),
    }
}

fn logout(
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
    output: OutputFormat,
) -> CliResult<()> {
    let profile_name = target_profile(conn_mgr, profile);
    // Clear locally-stored secrets (best effort). Note: the minted CAPI key still exists in
    // the Redis Cloud console until revoked there — server-side revocation is a follow-up.
    let store = CredentialStore::new();
    for suffix in ["cloud-api-key", "cloud-api-secret", "okta-refresh"] {
        let _ = store.delete_credential(&format!("{profile_name}-{suffix}"));
    }
    clear_pending(conn_mgr, &profile_name);
    let mut config = conn_mgr.config.clone();
    // Logout forgets credentials, not the environment: preserve the [cloud_auth.<profile>]
    // login endpoints across the profile removal so `auth login` still works afterward
    // (remove_profile drops them otherwise, which would break re-login on QA/staging).
    let saved_auth = config.cloud_auth.get(&profile_name).cloned();
    config.remove_profile(&profile_name);
    if let Some(auth) = saved_auth {
        config.cloud_auth.insert(profile_name.clone(), auth);
    }
    save_config(conn_mgr, &config)?;
    print_formatted_output(
        serde_json::json!({ "status": "ok", "profile": profile_name, "logged_out": true }),
        output,
    )
}

// ---- pending device-authorization store (bridges the non-blocking login and status --wait) ----

/// A device authorization awaiting approval, persisted between the `login` (initiate) and
/// `status --wait` (complete) process runs.
#[derive(Serialize, Deserialize)]
struct PendingAuth {
    profile: String,
    /// The full device authorization (serde-serializable), so a later `status --wait` — possibly
    /// a different process — can resume polling via `DeviceFlowClient::poll`.
    device_authorization: DeviceAuthorization,
    allow_plaintext: bool,
}

/// Directory for the pending file — next to the active config so it honors `--config-file`.
fn pending_dir(conn_mgr: &ConnectionManager) -> PathBuf {
    conn_mgr
        .config_path
        .as_ref()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .or_else(|| {
            Config::config_path()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
        })
        .unwrap_or_else(std::env::temp_dir)
}

fn pending_path(conn_mgr: &ConnectionManager, profile: &str) -> PathBuf {
    pending_dir(conn_mgr).join(format!("redisctl-pending-{profile}.json"))
}

fn save_pending(conn_mgr: &ConnectionManager, pending: &PendingAuth) -> CliResult<()> {
    let path = pending_path(conn_mgr, &pending.profile);
    let json = serde_json::to_vec_pretty(pending)?;
    write_private(&path, &json)
        .map_err(|e| RedisCtlError::Configuration(format!("could not save pending login: {e}")))
}

fn load_pending(conn_mgr: &ConnectionManager, profile: &str) -> Option<PendingAuth> {
    let bytes = std::fs::read(pending_path(conn_mgr, profile)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn clear_pending(conn_mgr: &ConnectionManager, profile: &str) {
    let _ = std::fs::remove_file(pending_path(conn_mgr, profile));
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

fn parse_url(value: &str, field: &str) -> CliResult<Url> {
    Url::parse(value)
        .map_err(|e| RedisCtlError::Configuration(format!("invalid {field} ({value:?}): {e}")))
}

fn save_config(conn_mgr: &ConnectionManager, config: &Config) -> CliResult<()> {
    match &conn_mgr.config_path {
        Some(path) => config.save_to_path(path)?,
        None => config.save()?,
    }
    Ok(())
}

/// A recognizable, unique-per-login CAPI key name (visible/revocable in the console).
fn default_key_name() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("redisctl-cli-{ts}")
}

/// Best-effort: open the given URL in the platform browser. Failure is non-fatal (the URL is
/// also printed for manual opening).
fn open_browser(url: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(url);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}
