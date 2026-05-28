# Quick Start

Get running in 60 seconds with Docker.

## Try It Now

### Redis Cloud

```bash
# Set your credentials
export REDIS_CLOUD_API_KEY="your-api-key"
export REDIS_CLOUD_SECRET_KEY="your-secret-key"

# Run a command
docker run --rm \
  -e REDIS_CLOUD_API_KEY \
  -e REDIS_CLOUD_SECRET_KEY \
  ghcr.io/redis/redisctl cloud subscription list
```

!!! tip "Getting API Keys"
    Get your API keys from the [Redis Cloud console](https://app.redislabs.com/) under **Access Management > API Keys**.

### Redis Enterprise

```bash
# Set your credentials
export REDIS_ENTERPRISE_URL="https://cluster.example.com:9443"
export REDIS_ENTERPRISE_USER="admin@cluster.local"
export REDIS_ENTERPRISE_PASSWORD="your-password"
export REDIS_ENTERPRISE_INSECURE="true"  # for self-signed certs

# Run a command
docker run --rm \
  -e REDIS_ENTERPRISE_URL \
  -e REDIS_ENTERPRISE_USER \
  -e REDIS_ENTERPRISE_PASSWORD \
  -e REDIS_ENTERPRISE_INSECURE \
  ghcr.io/redis/redisctl enterprise cluster get
```

That's it! You just ran your first redisctl command.

## What Just Happened?

1. **Docker pulled the image** - Pre-built with everything you need
2. **Environment variables** - Passed your credentials securely
3. **Command executed** - Called the Redis API and formatted the output

## Common First Commands

!!! tip "Prefix-free commands"
    The `cloud` and `enterprise` prefixes are optional when your profile makes the platform unambiguous. The examples below use the short form — add `cloud` or `enterprise` if you have profiles for both platforms.

### List Resources

```bash
# List all subscriptions (cloud-only command, prefix optional)
redisctl subscription list

# List databases in a subscription (inferred from profile)
redisctl database list --subscription-id 123456

# Get cluster info (enterprise-only command, prefix optional)
redisctl cluster get

# Or be explicit
redisctl enterprise database list
```

### Get JSON Output

Add `-o json` to any command for structured output:

```bash
redisctl cloud subscription list -o json
```

### Filter with JMESPath

Use `-q` to query and filter results:

```bash
# Get just subscription names
redisctl cloud subscription list -o json -q '[].name'

# Count databases
redisctl enterprise database list -o json -q 'length(@)'
```

## Next Steps

Choose your path:

<div class="grid cards" markdown>

-   :material-school:{ .lg .middle } __New to redisctl?__

    ---

    Understand the architecture and concepts

    [:octicons-arrow-right-24: Common Features](../common/index.md)

-   :material-cloud:{ .lg .middle } __Redis Cloud User__

    ---

    Manage subscriptions and databases

    [:octicons-arrow-right-24: Cloud Commands](../cloud/index.md)

-   :material-server:{ .lg .middle } __Redis Enterprise User__

    ---

    Control clusters and generate support packages

    [:octicons-arrow-right-24: Enterprise Commands](../enterprise/index.md)

-   :material-book-open:{ .lg .middle } __Learn by Example__

    ---

    Step-by-step guides for common tasks

    [:octicons-arrow-right-24: Cookbook](../cookbook/index.md)

</div>
