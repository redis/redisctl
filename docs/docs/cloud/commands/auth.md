# Authentication Commands

Sign in to Redis Cloud and manage the stored session.

`cloud auth login` is a credential **bootstrapper**: it runs a standard OIDC sign-in, mints a
Redis Cloud API key for your account, and writes it into a normal profile — so you never have to
create and paste an API key by hand. Afterward every other `cloud` command works with that profile.

!!! note "Prerequisite"
    The profile needs its login endpoints — a `[cloud_auth.<profile>]` section, or the built-in
    production defaults. See [Configuration](#configuration) below.

## Log In

```bash
redisctl cloud auth login
```

Opens your browser to sign in (the interactive default), then stores the minted API key in the
profile's secure storage.

### Examples

```bash
# Interactive browser sign-in for the default cloud profile
redisctl cloud auth login

# A specific profile
redisctl cloud auth login --profile qa

# Headless machine with no local browser — use the device flow
redisctl cloud auth login --device

# Store credentials in the config file when no OS keyring is available
redisctl cloud auth login --allow-plaintext
```

| Flag | Description |
|------|-------------|
| `--device` | Use the device-authorization flow (print a URL + code) instead of opening a browser. |
| `--wait` | With `--device`: block until approved (one-shot). Without it, `login --device` returns immediately and `auth status --wait` completes the login. |
| `--allow-plaintext` | Store credentials in the config file when no OS keyring is available. |

### Browser (loopback) flow

The default on an interactive terminal — a single command opens the browser and completes:

```mermaid
sequenceDiagram
    actor U as User
    participant CLI as redisctl
    participant OK as Okta (OIDC)
    participant SM as Redis Cloud API
    participant CFG as profile and keyring
    U->>CLI: cloud auth login
    Note over CLI: bind loopback port, build PKCE and state
    CLI->>OK: open browser to authorize (PKCE challenge, state)
    U->>OK: sign in
    OK-->>CLI: redirect to loopback callback (code, state)
    Note over CLI: validate state BEFORE responding, then success page
    CLI->>OK: exchange code and PKCE verifier for tokens
    OK-->>CLI: access and refresh tokens
    CLI->>SM: sign in, enable programmatic access, mint an API key
    SM-->>CLI: account API key and user secret
    CLI->>CFG: write cloud profile (secrets to keyring)
    CLI-->>U: signed in - profile ready
```

### Device flow (headless / agents)

With `--device`, `login` is **non-blocking**: it prints the verification URL + code and returns, so
an agent can relay them; `auth status --wait` then completes the login. `login --device --wait`
collapses both into one blocking call for a human.

```mermaid
sequenceDiagram
    actor A as Agent
    actor H as Human
    participant CLI as redisctl
    participant OK as Okta (OIDC)
    participant SM as Redis Cloud API
    participant CFG as profile and keyring
    A->>CLI: cloud auth login --device
    CLI->>OK: request device authorization
    OK-->>CLI: user_code, verification_uri, device_code
    CLI-->>A: authorization_pending and code (writes pending record)
    A->>H: relay verification URL and code
    H->>OK: open URL, sign in, confirm code
    A->>CLI: cloud auth status --wait
    CLI->>OK: poll for tokens until approved
    OK-->>CLI: access and refresh tokens
    CLI->>SM: sign in, enable access, mint an API key
    SM-->>CLI: account API key and user secret
    CLI->>CFG: write cloud profile (secrets to keyring)
    CLI-->>A: authenticated and account_id
```

## Status

```bash
redisctl cloud auth status
```

Reports whether the profile is authenticated. With `--wait`, it completes a pending device login.

### Examples

```bash
redisctl cloud auth status

# Complete a pending `login --device`, waiting up to 5 minutes
redisctl cloud auth status --wait --timeout 300
```

| Flag | Description |
|------|-------------|
| `--wait` | Block until a pending device login is approved (or is denied / the code expires), then run the exchange and persist. |
| `--timeout <secs>` | Max seconds to wait with `--wait` (default 600). If it elapses while still pending, exits `0` reporting `authorization_pending` — run again to keep waiting. |

## Log Out

```bash
redisctl cloud auth logout
```

Removes the locally stored credentials for the profile (keyring entries and the profile), while
preserving the `[cloud_auth.<profile>]` login endpoints so you can log in again.

!!! note
    The minted API key still exists in the Redis Cloud console until you revoke it there —
    server-side revocation on logout is a planned follow-up.

## Configuration

`cloud auth login` reads its OIDC endpoints from a `[cloud_auth.<profile>]` section, falling back
to built-in production defaults (so for production you usually need no section at all):

```toml
[cloud_auth.myenv]
okta_issuer    = "https://<your-okta-issuer>/oauth2/default"
okta_client_id = "<public-client-id>"
sm_api_url     = "https://<sm-api-host>/api/v1"
capi_url       = "https://api.redislabs.com/v1"
```

## Error handling (agents)

In `-o json` / `-o yaml` mode, failures on this surface print a machine-branchable envelope to
**stdout** and exit with a mapped code, so scripts and agents can branch on `.error.code` instead
of parsing prose:

```json
{ "status": "error", "error": { "code": "device_code_expired", "message": "...", "retryable": true } }
```

| Exit code | Meaning | Example codes |
|-----------|---------|---------------|
| `1` | unknown / backend failure | `sm_exchange_failed` |
| `2` | usage / precondition to fix | `not_authenticated`, `auth_denied`, `keyring_unavailable` |
| `3` | transient / retryable | `device_code_expired`, `transient_api_error`, `rate_limited` |

Human (non-JSON) mode prints the usual diagnostic to stderr and keeps today's `0`/`1` exit behavior.
