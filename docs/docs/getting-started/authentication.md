# Authentication

Configure credentials for Redis Cloud, Redis Enterprise, and direct database connections.

## Overview

redisctl supports three credential sources, in order of precedence:

1. **Command-line flags** - Highest priority
2. **Environment variables** - Good for CI/CD
3. **Profiles** - Best for daily use

## Redis Cloud: sign in with `cloud auth login`

For Redis Cloud, the easiest way to authenticate is to let redisctl mint the API key for you:

```bash
redisctl cloud auth login
```

This runs a browser OIDC sign-in, creates a Cloud API key for your account, and stores it in a
profile — no manual key creation or copying. On a headless machine or from an agent, use
`redisctl cloud auth login --device`. See **[Authentication commands](../cloud/commands/auth.md)**
for the full flow (browser and device), `status` / `logout`, and the error-code contract.

The manual methods below (environment variables, hand-created API keys) still work if you prefer to
manage keys yourself.

## Credential Method Comparison

| Method | Security | Persistence | Best for |
|--------|----------|-------------|----------|
| Config file (plaintext) | Low | Persistent | Local dev |
| OS keyring | High | Persistent | Production workstations |
| Environment variables | Medium | Session | CI/CD, containers |
| Env var references in config | Medium | Persistent | CI/CD with profiles |

## Environment Variables

### Redis Cloud

| Variable | Description |
|----------|-------------|
| `REDIS_CLOUD_API_KEY` | API account key |
| `REDIS_CLOUD_SECRET_KEY` | API secret key |

```bash
export REDIS_CLOUD_API_KEY="your-api-key"
export REDIS_CLOUD_SECRET_KEY="your-secret-key"
redisctl cloud subscription list
```

### Redis Enterprise

| Variable | Description |
|----------|-------------|
| `REDIS_ENTERPRISE_URL` | Cluster API URL (e.g., `https://cluster:9443`) |
| `REDIS_ENTERPRISE_USER` | Admin username |
| `REDIS_ENTERPRISE_PASSWORD` | Admin password |
| `REDIS_ENTERPRISE_INSECURE` | Skip TLS verification (`true`/`false`) |

```bash
export REDIS_ENTERPRISE_URL="https://cluster.example.com:9443"
export REDIS_ENTERPRISE_USER="admin@cluster.local"
export REDIS_ENTERPRISE_PASSWORD="your-password"
redisctl enterprise cluster get
```

## Profiles

Profiles store credentials for multiple environments. Much better than juggling environment variables.

### Interactive Setup

The fastest way to get started:

```bash
redisctl profile init
```

The wizard guides you through choosing a profile name, deployment type, and entering credentials.

### Create a Profile

```bash
# Redis Cloud profile
redisctl profile set prod-cloud --type cloud \
  --api-key "$API_KEY" \
  --api-secret "$SECRET_KEY"

# Redis Enterprise profile
redisctl profile set prod-enterprise --type enterprise \
  --url "https://cluster.example.com:9443" \
  --username "admin@cluster.local" \
  --password "$PASSWORD"

# Database profile
redisctl profile set my-cache --type database \
  --host "redis-12345.cloud.redislabs.com" \
  --port 12345 \
  --password "$PASSWORD"
```

### Use a Profile

```bash
# Specify profile per command
redisctl --profile prod-cloud cloud subscription list

# Or set a default for each profile type
redisctl profile default-cloud prod-cloud
redisctl profile default-enterprise prod-enterprise
redisctl profile default-database my-cache

# Now commands use the appropriate default
redisctl cloud subscription list  # Uses prod-cloud
```

### List Profiles

```bash
redisctl profile list
```

### Secure Storage

!!! warning "Default Storage"
    By default, credentials are stored in plaintext in `~/.config/redisctl/config.toml`. Use secure storage for production credentials.

#### OS Keyring

Store credentials in your operating system's keychain:

```bash
redisctl profile set prod --type cloud \
  --api-key "$KEY" \
  --api-secret "$SECRET" \
  --use-keyring
```

#### Environment Variable References

Reference environment variables instead of storing values:

```bash
redisctl profile set prod --type cloud \
  --api-key '${REDIS_CLOUD_API_KEY}' \
  --api-secret '${REDIS_CLOUD_SECRET_KEY}'
```

The variables are resolved at runtime.

## Configuration File

Profiles are stored in:

| Platform | Location |
|----------|----------|
| Linux/macOS | `~/.config/redisctl/config.toml` |
| Windows | `%APPDATA%\redis\redisctl\config.toml` |

Example:

```toml
default_cloud = "prod-cloud"
default_enterprise = "prod-enterprise"

[profiles.prod-cloud]
deployment_type = "cloud"
api_key = "your-api-key"
api_secret = "your-secret-key"

[profiles.prod-enterprise]
deployment_type = "enterprise"
url = "https://cluster.example.com:9443"
username = "admin@cluster.local"
password = "${PROD_PASSWORD}"
```

## Next Steps

- [Profiles](../common/profiles.md) - Full profile management guide
- [Output Formats](../common/output-formats.md) - JSON, YAML, and table output
- [JMESPath Queries](../common/jmespath.md) - Filter and transform output
