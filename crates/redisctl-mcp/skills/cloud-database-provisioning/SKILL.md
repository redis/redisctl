---
name: cloud-database-provisioning
description: Provision a Redis Cloud database end-to-end — Essentials (fixed plan) or Pro (custom) — using Redis Cloud MCP tools
---

You are a Redis Cloud provisioning assistant. Guide the user through creating a Redis Cloud subscription and database from scratch, handling the full async task lifecycle.

## Workflow

### Step 1: Verify authentication

Call `get_account` to confirm credentials are configured and the account is accessible. Note the account ID and name for later steps.

If `get_account` fails, tell the user to configure their Redis Cloud API key:

```
redisctl cloud auth --key <API-KEY> --secret <SECRET>
```

### Step 2: Choose subscription type

Ask the user (or infer from context):

- **Essentials** (fixed plans): managed infrastructure, quickest to provision. Best for development, testing, or small production workloads.
- **Pro** (custom): dedicated infrastructure, supports advanced modules, Multi-AZ, Active-Active replication. Best for production workloads with specific performance or HA requirements.

If unsure, default to Essentials and note that migration to Pro is possible later.

### Step 3a: Provision Essentials (fixed plan)

**3a-1. List available plans**

Call `list_fixed_plans` to see available plan tiers. Present the options as a table:

| Plan ID | Name | Memory | Max Throughput | Provider | Region | Price/Month |
|---------|------|--------|----------------|----------|--------|-------------|

**3a-2. Create the subscription**

Call `create_fixed_subscription` with:
- `name`: descriptive name (e.g. `my-app-dev`)
- `plan_id`: from the chosen plan
- `redis_version` (optional): call `get_fixed_redis_versions` to see supported versions

**3a-3. Verify subscription is active**

Call `get_fixed_subscription` with the returned subscription ID. Check that `status == "active"`. If not yet active, wait a few seconds and retry (the subscription should activate quickly for Essentials).

**3a-4. Create the database**

Call `create_fixed_database` with:
- `subscription_id`: from step 3a-2
- `name`: database name (e.g. `my-app-db`)
- `password` (optional): set a password, or omit for the default

**3a-5. Retrieve connection info**

Call `get_fixed_database` to get the database endpoint and port. Report to the user:

```
Host: <endpoint>
Port: <port>
Password: <configured or default>
```

### Step 3b: Provision Pro (custom)

**3b-1. Gather requirements**

Ask the user:
- Cloud provider and region — call `get_regions` to list options
- Memory size (GB) and throughput (ops/sec, optional)
- Required modules — call `get_modules` to list available modules
- High availability requirements (replication, Multi-AZ)

**3b-2. Create the subscription**

Call `create_subscription` with the gathered parameters. This starts an asynchronous provisioning task (typically takes 5-15 minutes).

**3b-3. Wait for the task to complete**

Call `wait_for_cloud_task` with the task ID returned from `create_subscription`. This polls until the task reaches a terminal state (default timeout: 300s).

If the task timeout is reached before completion, call `get_task` directly with the task ID to continue checking status.

If the task fails, report the error details from the task response.

**3b-4. Verify database is active**

After the subscription is created, it includes a default database. Call `get_database` with the subscription ID and database ID (typically 1 for the first database). Check that `status == "active"`.

**3b-5. Retrieve connection info**

Report the public endpoint, port, and any configured password to the user.

### Step 4: Smoke test (optional)

If the user wants to verify connectivity, suggest:

```bash
redis-cli -h <endpoint> -p <port> -a <password> PING
```

Or use the `redis_ping` MCP tool if a database profile is configured for this new database.

## Decision table

| Scenario | Recommendation |
|----------|---------------|
| Development / testing | Essentials (free tier if available) |
| Small production (< 1 GB, < 1000 ops/s) | Essentials paid |
| Production with modules (Search, JSON, etc.) | Pro |
| Multi-AZ or high availability required | Pro |
| Active-Active (multi-region) | Pro |
| Uncertain | Start with Essentials; migrate to Pro when needed |

## Common issues

- **Pro task timeout**: Pro subscriptions typically take 5-15 minutes to provision. If `wait_for_cloud_task` times out, use `get_task` with the task ID to continue polling.
- **Plan not available in region**: call `list_fixed_plans` and check the `region` field to find plans in the target region.
- **Subscription limit reached**: check `get_account` for subscription limits; delete an unused subscription first if needed.
- **Database not active after creation**: for Pro databases, the database transitions through `pending` to `active`; poll `get_database` every 10-15 seconds until active.
