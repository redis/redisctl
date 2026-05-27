# Create Your First Database

Go from zero to a running Redis database in 5 minutes.

## Prerequisites

- Redis Cloud account ([sign up free](https://redis.com/try-free/))
- API keys from Redis Cloud console (Access Management > API Keys)
- redisctl installed (`brew install redis/homebrew-tap/redisctl`)

!!! tip "Prefix optional"
    The `cloud` prefix is optional when your profile config is unambiguous. The examples below use the full form for copy-paste convenience in scripts. See [Platform Inference](../../common/profiles.md#platform-inference).

## Step 1: Configure Credentials

```bash
export REDIS_CLOUD_API_KEY="your-api-key"
export REDIS_CLOUD_SECRET_KEY="your-secret-key"
```

Or save to a profile for repeated use:

```bash
redisctl profile set mycloud --type cloud \
  --api-key "$REDIS_CLOUD_API_KEY" \
  --api-secret "$REDIS_CLOUD_SECRET_KEY"
```

## Step 2: Find Your Subscription

```bash
redisctl cloud subscription list
```

If you don't have a subscription yet, you'll need to create one first (or use the free tier from the web console).

Note your subscription ID for the next step.

## Step 3: Create the Database

```bash
redisctl cloud database create \
  --subscription-id 123456 \
  --name my-first-db \
  --memory-limit-in-gb 1 \
  --wait
```

The `--wait` flag blocks until the database is ready (usually 30-60 seconds).

## Step 4: Get Connection Details

```bash
redisctl cloud database list --subscription-id 123456 -o json -q '[?name==`my-first-db`].{
  endpoint: publicEndpoint,
  password: password
}'
```

## Step 5: Connect

```bash
# Get the endpoint
ENDPOINT=$(redisctl cloud database list --subscription-id 123456 \
  -o json -q "[?name=='my-first-db'].publicEndpoint | [0]")

# Connect with redis-cli
redis-cli -u "redis://default:PASSWORD@$ENDPOINT"
```

## Complete Script

```bash
#!/bin/bash
set -e

SUB_ID="${1:?Usage: $0 <subscription-id>}"
DB_NAME="my-app-cache"

echo "Creating database '$DB_NAME'..."

redisctl cloud database create \
  --subscription-id "$SUB_ID" \
  --name "$DB_NAME" \
  --memory-limit-in-gb 1 \
  --wait

echo "Database created! Getting connection info..."

redisctl cloud database list --subscription-id "$SUB_ID" \
  -o json -q "[?name=='$DB_NAME'] | [0].{
    name: name,
    endpoint: publicEndpoint,
    status: status
  }"
```

## Next Steps

- [Configure ACLs](acls.md) - Secure database access
- [Set up backups](backup-restore.md) - Protect your data
- [VPC Peering](vpc-peering.md) - Private networking
