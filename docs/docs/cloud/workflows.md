# Cloud Workflows

Multi-step operations for Redis Cloud.

## Subscription Setup

Create a subscription with a database in one command:

```bash
redisctl cloud workflow subscription-setup \
  --name production \
  --provider AWS \
  --region us-east-1 \
  --database-name cache \
  --database-memory-gb 2 \
  --wait
```

This creates:
1. A new subscription
2. A database within it
3. Waits for both to be ready

### Options

| Option | Description |
|--------|-------------|
| `--name` | Subscription name |
| `--provider` | AWS, GCP, or Azure |
| `--region` | Cloud region |
| `--database-name` | Database name |
| `--database-memory-gb` | Database memory in GB |
| `--wait` | Wait for completion |

## Quick Database

Go from nothing to a running **free** database with one command. `quick-database` creates (or
reuses) a free Essentials subscription and database, waits for it to be ready, and writes the
connection string to a file:

```bash
redisctl cloud workflow quick-database --name my-app
```

It is **idempotent by name**: re-running with the same `--name` returns the existing database
instead of creating another, and it resumes a half-finished previous run. Requires an
authenticated profile — see [`cloud auth login`](commands/auth.md).

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--name` | *(required)* | Database name; also names the subscription (prefixed `redisctl-`). Must match `^[a-z][a-z0-9-]{1,38}[a-z0-9]$`. |
| `--output-credentials` | `./.env` | File to write the connection string into (created if missing; managed keys updated in place, other lines preserved). |
| `--variable` | `REDIS_URL` | Environment-variable name for the primary URL. Broken-out fields derive their prefix from it. |
| `--wait-timeout` | `600` | Max seconds to wait for each async operation. |
| `--wait-interval` | `5` | Polling interval in seconds. |

### Output

The connection string and password are written **only to the file** — never to stdout. In
`-o json` / `-o yaml` mode the command prints a non-secret report:

```json
{
  "status": "ok",
  "database": { "id": "9001", "name": "my-app", "region": "us-east-1", "plan": "free", "tls": true },
  "credentials_written_to": "./.env",
  "credentials_variable": "REDIS_URL"
}
```

`status` is `ok` when a database was provisioned (or a half-finished run resumed) and `reused`
when one with that name already existed. The file receives the URL plus broken-out fields:

```dotenv
REDIS_URL=rediss://default:••••••@host.example.com:12000
REDIS_HOST=host.example.com
REDIS_PORT=12000
REDIS_PASSWORD=••••••
REDIS_USERNAME=default
REDIS_TLS=true
```

It is created `0600` (unix) and auto-added to `.gitignore` inside a git repo. On failure the
command emits a machine-branchable envelope and a mapped exit code — see
[Agent Error Codes](../reference/agent-error-codes.md).

## Database Credentials

Export an **existing** database's connection string to a file, without provisioning anything:

```bash
redisctl cloud workflow database-credentials \
  --subscription-id 123456 \
  --database-id 67890
```

| Option | Default | Description |
|--------|---------|-------------|
| `--subscription-id` | *(required)* | Subscription id of the existing database. |
| `--database-id` | *(required)* | Database id within that subscription. |
| `--output-credentials` | `./.env` | File to write the connection string into. |
| `--variable` | `REDIS_URL` | Environment-variable name for the primary URL. |
| `--wait-timeout` | `600` | Max seconds to wait for the endpoint to be readable. |
| `--wait-interval` | `5` | Polling interval in seconds. |

Output matches `quick-database` (same file format and JSON report, with `status: existing`), and
the same secret-safety applies: credentials go to the file, never to stdout.

## When to Use Workflows

**Use workflows when:**
- Setting up new environments
- Creating multiple related resources
- Need atomic-like operations

**Use individual commands when:**
- Managing existing resources
- Need fine-grained control
- Debugging issues

## Coming Soon

Additional workflows planned:
- Database migration
- Active-Active setup
- VPC peering setup

## Manual Multi-Step Operations

For complex scenarios not covered by workflows:

```bash
#!/bin/bash
set -e

# Step 1: Create subscription
SUB_ID=$(redisctl cloud subscription create \
  --name production \
  --cloud-provider AWS \
  --region us-east-1 \
  --wait \
  -o json -q 'id')

echo "Subscription created: $SUB_ID"

# Step 2: Create database
redisctl cloud database create \
  --subscription-id "$SUB_ID" \
  --name cache \
  --memory-limit-in-gb 2 \
  --wait

# Step 3: Get connection info
redisctl cloud database list --subscription-id "$SUB_ID" \
  -o json -q '[0].{endpoint: publicEndpoint}'
```

## Related

- [Authentication commands](commands/auth.md) — sign in before `quick-database`
- [Agent Error Codes](../reference/agent-error-codes.md) — the machine-branchable failure contract
- [From sign-in to REDIS_URL](../cookbook/cloud/quick-database.md) — the full agent walkthrough
- [Subscription Commands](commands/subscriptions.md)
- [Database Commands](commands/databases.md)
- [Async Operations](../common/async-operations.md)
