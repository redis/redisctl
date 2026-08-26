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

use redisctl_core::AuthError;
use redisctl_core::auth::{
    AccountChoice, CloudAuthenticator, LoginAccount, LoginFlow, MintedCredentials,
};
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
            account,
        } => {
            login(
                conn_mgr,
                profile,
                *device,
                *allow_plaintext,
                *wait,
                *account,
                output,
            )
            .await
        }
        CloudAuthCommands::Switch { account } => switch(conn_mgr, profile, *account, output).await,
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
    account: Option<u64>,
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
            account,
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
        auth_cfg,
        LoginRun {
            flow: if use_device {
                LoginFlow::Device
            } else {
                LoginFlow::Loopback
            },
            account: match account {
                Some(id) => AccountChoice::Id(id),
                None => AccountChoice::Current,
            },
            allow_plaintext,
        },
    )
    .await?;
    emit_signed_in(&creds, &profile_name, output)
}

/// Switch the profile's key to another account, reusing the sign-in stored at login.
///
/// An API key is scoped to one account, so this mints a key for the chosen account and replaces
/// the profile's current one. The refresh token saved at login stands in for the browser; minting
/// itself cannot be avoided, because the key is per-account.
async fn switch(
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
    account: Option<u64>,
    output: OutputFormat,
) -> CliResult<()> {
    let (profile_name, authenticator, auth_cfg) = prepare(conn_mgr, profile)?;

    // Choosing from a list needs somewhere to ask. Checked before signing in, so a
    // non-interactive caller fails immediately rather than after doing the work.
    let interactive = std::io::stderr().is_terminal() && std::io::stdin().is_terminal();
    if account.is_none() && !interactive {
        return Err(RedisCtlError::Structured(Box::new(
            StructuredError::account_required(
                "there is no terminal to choose an account on; pass the id instead \
                 (`redisctl cloud auth switch <ID>`). A completed login reports the accounts you \
                 belong to in its `accounts` field.",
            ),
        )));
    }

    // Which account this profile is on today, as recorded at the last login/switch.
    let on_account = auth_cfg.account_id;

    let store = CredentialStore::new();
    let refresh_token = store
        .get_credential(&format!("keyring:{profile_name}-okta-refresh"), None)
        .map_err(|_| {
            RedisCtlError::Structured(Box::new(StructuredError::not_authenticated(format!(
                "there is no stored sign-in for profile '{profile_name}' to switch with \
                 (credentials saved with --allow-plaintext cannot be reused this way). Run \
                 `redisctl cloud auth login --account <ID>` instead."
            ))))
        })?;

    // The browser is only ever needed to obtain a refresh token in the first place.
    let tokens = authenticator.refresh(&refresh_token).await.map_err(|_| {
        RedisCtlError::Structured(Box::new(StructuredError::not_authenticated(format!(
            "the stored sign-in for profile '{profile_name}' is no longer usable — refresh tokens \
             expire and are rotated. Run `redisctl cloud auth login --account <ID>`."
        ))))
    })?;

    let creds = complete_and_persist(
        conn_mgr,
        &profile_name,
        &authenticator,
        &tokens,
        auth_cfg,
        LoginRun {
            flow: LoginFlow::Loopback,
            account: match account {
                Some(id) => AccountChoice::Id(id),
                // `on_account` comes from the profile, not the session: `setcurrent` is
                // session-scoped server-side, so a fresh sign-in reports the user's default
                // account and would mark the wrong row as current.
                None => AccountChoice::Prompt(Box::new(move |accounts, _session_account| {
                    prompt_account(accounts, on_account)
                })),
            },
            // A refresh token only exists on the keyring path, so this is never plaintext.
            allow_plaintext: false,
        },
    )
    .await?;

    let which = creds
        .account_id
        .as_deref()
        .and_then(|id| id.parse::<u64>().ok())
        .and_then(|id| creds.accounts.iter().find(|a| a.id == id))
        .map(LoginAccount::label)
        .unwrap_or_else(|| "the selected account".to_string());
    eprintln!("\n\u{2713} Profile '{profile_name}' now uses {which}.");
    print_formatted_output(
        serde_json::json!({
            "status": "ok",
            "profile": profile_name,
            "account_id": creds.account_id,
            "account_name": creds.account_name,
            "email": creds.email,
        }),
        output,
    )
}

/// Ask which account to switch to, listing them with the current one marked.
///
/// Only reached on a terminal (the caller checks). An error abandons the switch before anything
/// is minted or written.
fn prompt_account(accounts: &[LoginAccount], current: Option<u64>) -> Result<u64, AuthError> {
    if accounts.len() < 2 {
        return Err(AuthError::Protocol(
            "this login belongs to a single Redis Cloud account, so there is nothing to switch to"
                .into(),
        ));
    }
    eprintln!("\nAccounts you belong to:");
    for (i, a) in accounts.iter().enumerate() {
        let marker = if Some(a.id) == current {
            "  (current)"
        } else {
            ""
        };
        eprintln!("  {}) {}{}", i + 1, a.label(), marker);
    }
    if current.is_none() {
        eprintln!("  (which one this profile is on is unknown — sign in again to record it)");
    }
    eprint!("Switch to which? [1-{}]: ", accounts.len());
    use std::io::Write;
    let _ = std::io::stderr().flush();

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| AuthError::Protocol(format!("could not read a choice: {e}")))?;
    resolve_account_choice(accounts, &line)
}

/// Map what the user typed onto an account id. Split out from the prompt so the selection rules
/// are testable without a terminal.
fn resolve_account_choice(accounts: &[LoginAccount], input: &str) -> Result<u64, AuthError> {
    let typed = input.trim();
    // An id is accepted as well as a list position: it is what the listing shows, so typing one
    // back is the obvious thing to try.
    if let Ok(id) = typed.parse::<u64>()
        && let Some(a) = accounts.iter().find(|a| a.id == id)
    {
        return Ok(a.id);
    }
    let choice: usize = typed.parse().map_err(|_| {
        AuthError::Protocol(format!(
            "'{typed}' is not one of 1-{} or an account id",
            accounts.len()
        ))
    })?;
    accounts
        .get(choice.wrapping_sub(1))
        .map(|a| a.id)
        .ok_or_else(|| {
            AuthError::Protocol(format!(
                "{choice} is not one of 1-{} or an account id",
                accounts.len()
            ))
        })
}

/// Sign-in instructions for the device flow. `verification_uri_complete` pre-fills the code on the
/// activation page, so the user confirms it rather than typing it — RFC 8628 §3.3.1 still requires
/// that explicit confirmation, so the step itself can't be skipped.
fn device_instructions(authz: &DeviceAuthorization) -> String {
    match authz.verification_uri_complete() {
        Some(complete) => format!(
            "\nTo sign in, open:\n  {complete}\nand confirm the code:  {}  (already filled in)\n",
            authz.user_code(),
        ),
        None => format!(
            "\nTo sign in, open:\n  {}\nand enter the code:  {}\n",
            authz.verification_uri(),
            authz.user_code(),
        ),
    }
}

/// Start the device flow, save a pending record, and return the device code to the caller
/// (agent) without blocking. Completion happens later via `auth status --wait`.
async fn initiate_device_login(
    conn_mgr: &ConnectionManager,
    profile_name: &str,
    authenticator: &CloudAuthenticator,
    allow_plaintext: bool,
    account: Option<u64>,
    output: OutputFormat,
) -> CliResult<()> {
    let authz = authenticator
        .device()
        .start(&SCOPES)
        .await
        .map_err(auth_err)?;
    save_pending(
        conn_mgr,
        &PendingAuth {
            profile: profile_name.to_string(),
            device_authorization: authz.clone(),
            allow_plaintext,
            account,
        },
    )?;

    eprintln!(
        "{}Then run: redisctl cloud auth status --wait   (or wait for the agent to poll)",
        device_instructions(&authz),
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
    eprintln!("{}(waiting for approval…)", device_instructions(&authz),);

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

/// Ask the user for a TOTP code when SM challenges the login for MFA.
///
/// Returns `Ok(None)` when there is no terminal to prompt on — an agent can't produce a
/// time-based code, so the caller reports `mfa_required` and the human re-runs interactively.
/// The `^\d{6}$` check is local on purpose: a malformed value would otherwise consume one of the
/// user's server-side attempts.
fn prompt_mfa_code(factors: &[String], attempt: u32) -> Result<Option<String>, AuthError> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Ok(None);
    }
    if attempt == 1 {
        let detail = if factors.is_empty() {
            String::new()
        } else {
            format!(" ({})", factors.join(", "))
        };
        eprintln!("\nThis account requires multi-factor authentication{detail}.");
    } else {
        eprintln!(
            "That code wasn't accepted. {} attempt(s) left.",
            4 - attempt
        );
    }
    loop {
        eprint!("Enter the 6-digit code from your authenticator app: ");
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            return Ok(None); // EOF / Ctrl-D
        }
        let code = line.trim();
        if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
            return Ok(Some(code.to_string()));
        }
        eprintln!("Codes are exactly 6 digits.");
    }
}

/// What one login run decided, threaded to the persist step together rather than as loose
/// positional arguments.
struct LoginRun {
    flow: LoginFlow,
    /// Which account to mint for: the session's current one, an explicit id, or a picker.
    account: AccountChoice,
    allow_plaintext: bool,
}

/// Run the SM exchange, persist the profile + secrets, and return the minted credentials.
async fn complete_and_persist(
    conn_mgr: &ConnectionManager,
    profile_name: &str,
    authenticator: &CloudAuthenticator,
    tokens: &TokenSet,
    auth_cfg: CloudAuthConfig,
    run: LoginRun,
) -> CliResult<MintedCredentials> {
    let creds = authenticator
        .complete_login_with_mfa(
            tokens,
            &default_key_name(),
            run.flow,
            run.account,
            prompt_mfa_code,
        )
        .await
        .map_err(auth_err)?;

    let store = if run.allow_plaintext {
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
            if run.allow_plaintext {
                RedisCtlError::from(e)
            } else {
                RedisCtlError::Structured(Box::new(StructuredError::keyring_unavailable(format!(
                    "failed to store credentials in the OS keyring ({e}). Re-run \
                     `redisctl cloud auth login --allow-plaintext` to store them in the config \
                     file instead."
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
    // The key is scoped to one account — whichever is *current* for this user, decided server-side
    // from the session. Say which was used when there was more than one it could have been.
    if creds.account_count() > 1 {
        // Render through `label()` like the list below, so one account is never spelled two
        // different ways two lines apart. `account_id` is always one of `accounts` — both come
        // from the same `/accounts` response — so the lookup only misses for a hand-built value.
        let which = creds
            .account_id
            .and_then(|id| creds.accounts.iter().find(|a| a.id == id))
            .map(LoginAccount::label)
            .unwrap_or_else(|| "your current account".to_string());
        eprintln!(
            "  note: the key is for {which} — 1 of {} accounts you belong to:",
            creds.account_count()
        );
        eprintln!(
            "    {}",
            creds
                .accounts
                .iter()
                .map(LoginAccount::label)
                .collect::<Vec<_>>()
                .join(" · ")
        );
        // Name the profile actually in use: a `<name>` placeholder invites inventing a new one,
        // and an unconfigured profile silently resolves to the *production* endpoints.
        eprintln!(
            "  To use another: redisctl --profile {profile_name} cloud auth login --account <id>"
        );
    }
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
            "account_name": creds.account_name,
            "account_count": creds.account_count(),
            "accounts": creds.accounts.iter().map(|a| serde_json::json!({
                "id": a.id,
                "name": a.name,
            })).collect::<Vec<_>>(),
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
                    auth_cfg,
                    LoginRun {
                        flow: LoginFlow::Device,
                        // Whatever the initiating `login --device --account` asked for; `None`
                        // (no flag) keeps the session's current account.
                        account: match pending.account {
                            Some(id) => AccountChoice::Id(id),
                            None => AccountChoice::Current,
                        },
                        allow_plaintext: pending.allow_plaintext,
                    },
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
    // A pending device login is invisible in a bare `authenticated: false`, which reads as "login
    // failed" when in fact it is half-finished. Say so, and name the command that completes it.
    let pending = !authenticated && load_pending(conn_mgr, &profile_name).is_some();
    if pending {
        eprintln!(
            "A device login for profile '{profile_name}' is waiting for approval.\nComplete it \
             with: redisctl cloud auth status --wait"
        );
    }
    let mut report = serde_json::json!({ "authenticated": authenticated, "profile": profile_name });
    if pending {
        report["status"] = "authorization_pending".into();
    }
    print_formatted_output(report, output)
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
    /// `--account` from the initiating `login`, so the account it asked for survives into the
    /// `status --wait` that actually mints. Defaulted for pending files written before this field.
    #[serde(default)]
    account: Option<u64>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts() -> Vec<LoginAccount> {
        vec![
            LoginAccount {
                id: 316941,
                name: Some("Acme".to_string()),
            },
            LoginAccount {
                id: 481022,
                name: None,
            },
        ]
    }

    /// The listing shows both a position and an id, so both have to be accepted — and anything
    /// else has to be refused rather than resolving to some account by accident.
    #[test]
    fn account_choice_accepts_a_position_or_an_id() {
        assert_eq!(resolve_account_choice(&accounts(), "1").unwrap(), 316941);
        assert_eq!(resolve_account_choice(&accounts(), " 2\n").unwrap(), 481022);
        // An id the user copied out of the list.
        assert_eq!(
            resolve_account_choice(&accounts(), "481022").unwrap(),
            481022
        );
    }

    #[test]
    fn account_choice_refuses_anything_else() {
        for input in ["", "0", "3", "999999", "abc", "1.5", "-1"] {
            assert!(
                resolve_account_choice(&accounts(), input).is_err(),
                "{input:?} should not resolve to an account"
            );
        }
    }

    /// A single-account user has nothing to switch to; saying so beats offering a list of one.
    #[test]
    fn prompt_refuses_when_there_is_only_one_account() {
        let one = vec![LoginAccount {
            id: 316941,
            name: Some("Acme".to_string()),
        }];
        // Does not read stdin: it returns before prompting.
        let err = prompt_account(&one, Some(316941)).unwrap_err();
        assert!(
            matches!(err, AuthError::Protocol(ref m) if m.contains("single Redis Cloud account")),
            "got {err:?}"
        );
    }
}
