# From Sign-In to REDIS_URL

Go from nothing to a working Redis database and a `REDIS_URL` in your `.env` — the full
agent-native arc: sign in once, provision a free database, connect. Credentials are written to a
file, never printed, so they can't leak into logs or an agent's captured output.

## Prerequisites

- A Redis Cloud account.
- redisctl installed (`brew install redis/homebrew-tap/redisctl`).

No API keys to create or paste by hand — `cloud auth login` mints one for you.

## Step 1: Sign in

```bash
redisctl cloud auth login
```

On an interactive terminal this opens your browser, signs you in, mints a Cloud API key, and
stores it in a profile. On a headless machine or from an agent it automatically switches to the
device flow (prints a URL + code); force it with `--device`. See
[Authentication commands](../../cloud/commands/auth.md) for the device / `status --wait` split.

Confirm:

```bash
redisctl cloud auth status
# → { "authenticated": true, "profile": "…" }
```

## Step 2: Provision a free database

```bash
redisctl cloud workflow quick-database --name my-app
```

This creates (or reuses) a free database and writes the connection string to `./.env`. It's
idempotent by name — safe to re-run. The command prints only non-secret metadata:

```json
{
  "status": "ok",
  "database": { "id": "9001", "name": "my-app", "region": "us-east-1", "plan": "free", "tls": true },
  "credentials_written_to": "./.env",
  "credentials_variable": "REDIS_URL"
}
```

Your `.env` now holds the URL plus broken-out fields (created `0600`, auto-gitignored):

```dotenv
REDIS_URL=rediss://default:••••••@host.example.com:12000
REDIS_HOST=host.example.com
REDIS_PORT=12000
REDIS_PASSWORD=••••••
REDIS_USERNAME=default
REDIS_TLS=true
```

## Step 3: Connect

Read `REDIS_URL` from the file — don't echo it:

```bash
redis-cli -u "$(grep '^REDIS_URL=' .env | cut -d= -f2-)" PING
# → PONG
```

Already have a database and just want its connection string in a file? Use
`database-credentials` instead of provisioning:

```bash
redisctl cloud workflow database-credentials --subscription-id 123456 --database-id 67890
```

## For agents

- **Check before you provision.** `cloud auth status` (or the `cloud_auth_status` MCP tool) is an
  offline check — branch on `authenticated` before calling `quick-database`.
- **Branch on the error code, not the message.** Failures print a JSON envelope with a stable
  `.error.code` and a mapped exit code — see [Agent Error Codes](../../reference/agent-error-codes.md).
- **Never echo the URL or password.** Pass them by reference (`-u "$REDIS_URL"`), use placeholders
  in any generated code or docs, and verify with a ping rather than a client that prints the URL.

## MCP

The same flow is available to agents over MCP: `cloud_auth_status` (read-only; check first) and
`cloud_quick_database` (provisions and writes the file). They call the identical engine as the
CLI. See the `redisctl-cloud-quickstart` skill.

## Related

- [Authentication commands](../../cloud/commands/auth.md)
- [Cloud workflows](../../cloud/workflows.md)
- [Agent Error Codes](../../reference/agent-error-codes.md)
