---
name: redisctl-cloud-quickstart
description: Provision a free Redis Cloud database for an app or agent in one step and connect to it. Use when someone needs a working Redis quickly, wants a REDIS_URL in their .env, or asks to "spin up a Redis database". Covers login, quick-database, and safe credential handling.
---

## Overview

Go from nothing to a working Redis database with three commands. `redisctl` authenticates the
user once, provisions a **free** database, and writes the connection string to a file — never
to stdout — so credentials can't leak into logs or an agent's captured output.

## The sequence

Headless (agent): `auth login` returns the device code immediately; relay it, then block on
`status --wait`. A human at a terminal can instead run a single blocking `auth login` (browser).

```bash
# 1. Initiate — returns the code right away, no blocking, no secret.
redisctl cloud auth login -o json
#    → {"status":"authorization_pending","verification_uri_complete":"…","user_code":"WDJB-MJHT"}

# 2. Relay URL + code to the human, then block until approved. Mints the Cloud API key into
#    the profile; every other `redisctl cloud` command works afterward.
redisctl cloud auth status --wait --timeout 600 -o json
#    → {"status":"ok","authenticated":true,"account_id":"…"}

# 3. Provision (or reuse) a free database. The connection string is written to ./.env,
#    NOT printed. JSON on stdout is a non-secret status report.
redisctl cloud workflow quick-database --name my-app -o json

# 4. Use it. Read REDIS_URL from the file; verify with a ping.
redis-cli -u "$(grep '^REDIS_URL=' .env | cut -d= -f2-)" PING   # → PONG
```

Step 2 prints only metadata:

```json
{
  "status": "ok",
  "database": { "id": "9001", "name": "my-app", "region": "us-east-1", "plan": "free", "tls": true },
  "credentials_written_to": "./.env",
  "credentials_variable": "REDIS_URL"
}
```

## What the credentials file contains

`quick-database` writes the full URL plus broken-out fields, so any app can read whichever form
it expects:

```dotenv
REDIS_URL=rediss://default:••••••@host.example.com:12000
REDIS_HOST=host.example.com
REDIS_PORT=12000
REDIS_PASSWORD=••••••
REDIS_USERNAME=default
REDIS_TLS=true
```

The file is created `0600` (unix) and auto-added to `.gitignore` inside a git repo.

## Branching on the result (for agents)

- `"status": "ok"` — a database was provisioned (or a half-finished run was resumed).
- `"status": "reused"` — a database with this name already existed; nothing was created.
- On failure, a JSON error envelope is printed to stdout and the process exits non-zero:
  `2` fix-and-retry (e.g. `invalid_name`, `not_authenticated`), `3` retryable
  (`transient_api_error`, `rate_limited`), `4` limit reached (`free_db_exists`). Branch on
  `.error.code`, not the message.

```bash
out=$(redisctl cloud workflow quick-database --name my-app -o json) || rc=$?
case "$(printf '%s' "$out" | jq -r '.error.code // empty')" in
  "")                              echo "ready" ;;
  not_authenticated)               redisctl cloud auth login ;;   # then retry
  transient_api_error|rate_limited) sleep 5; exec "$0" ;;
  free_db_exists)                  echo "account already has a free DB — see the console" ;;
esac
```

## Security conventions (important)

- **Never read the credentials file back to display it, and never echo `REDIS_URL`/`REDIS_PASSWORD`.**
  Pass them through by reference (`-u "$REDIS_URL"`, `$REDIS_PASSWORD`), not by printing.
- **Use placeholders in any docs, READMEs, or code you generate** (`rediss://…`, `${REDIS_URL}`),
  never the real value.
- **Verify connectivity with a ping**, not by importing a client library that echoes the URL.
- **Don't fetch credentials via `redisctl cloud database get`** — it can print the password by
  design. Use the file `quick-database` wrote.

## Headless / non-interactive

`cloud auth login` auto-detects no TTY and uses the OIDC device flow (prints a URL + code to
approve elsewhere). Force it with `redisctl cloud auth login --device`. Where no OS keyring is
available (many CI/Linux containers), add `--allow-plaintext` to store the key in the config
file instead.

## MCP

The same flow is exposed as MCP tools: `cloud_auth_status` (read-only; check first) and
`cloud_quick_database` (write; provisions and writes the file). They call the identical engine
as the CLI.
