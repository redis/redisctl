# Configuration File

Profile and settings file format reference.

## Location

| Platform | Path |
|----------|------|
| Linux | `~/.config/redisctl/config.toml` |
| macOS | `~/.config/redisctl/config.toml` |
| Windows | `%APPDATA%\redis\redisctl\config.toml` |

## File Format

```toml
# Default profiles for each deployment type
default_cloud = "cloud-prod"
default_enterprise = "enterprise-prod"
default_database = "my-cache"

# Redis Cloud profile
[profiles.cloud-prod]
deployment_type = "cloud"
api_key = "your-api-key"
api_secret = "your-secret-key"
tags = ["prod", "us-east"]

# Redis Enterprise profile
[profiles.enterprise-prod]
deployment_type = "enterprise"
url = "https://cluster.example.com:9443"
username = "admin@cluster.local"
password = "your-password"
insecure = true
tags = ["prod"]

# Database profile
[profiles.my-cache]
deployment_type = "database"
host = "redis-12345.cloud.redislabs.com"
port = 12345
password = "your-password"
tls = true
username = "default"

# Profile with environment variable references
[profiles.ci]
deployment_type = "cloud"
api_key = "${REDIS_CLOUD_API_KEY}"
api_secret = "${REDIS_CLOUD_SECRET_KEY}"

# Profile with keyring storage
[profiles.secure]
deployment_type = "cloud"
api_key = "keyring:prod-api-key"
api_secret = "keyring:prod-secret-key"

# Files.com API key for support package uploads
[global]
files_api_key = "your-files-api-key"
```

## Top-Level Fields

| Field | Description |
|-------|-------------|
| `default_cloud` | Default profile for Cloud commands |
| `default_enterprise` | Default profile for Enterprise commands |
| `default_database` | Default profile for Database commands |

## Profile Fields

Every profile requires a `deployment_type` field (`cloud`, `enterprise`, or `database`).

### Common Fields (All Types)

| Field | Description |
|-------|-------------|
| `deployment_type` | Profile type: `"cloud"`, `"enterprise"`, or `"database"` (required) |
| `tags` | Array of strings for organizing profiles (optional) |

### Redis Cloud

| Field | Description |
|-------|-------------|
| `deployment_type` | Must be `"cloud"` |
| `api_key` | API account key |
| `api_secret` | API secret key |
| `api_url` | Custom API URL (optional) |

### Redis Enterprise

| Field | Description |
|-------|-------------|
| `deployment_type` | Must be `"enterprise"` |
| `url` | Cluster API URL (e.g., `https://cluster:9443`) |
| `username` | Admin username |
| `password` | Admin password |
| `insecure` | Skip TLS verification (`true`/`false`) |
| `ca_cert` | Path to custom CA certificate |

### Database

| Field | Description |
|-------|-------------|
| `deployment_type` | Must be `"database"` |
| `host` | Redis server hostname |
| `port` | Redis server port |
| `password` | Redis password (optional) |
| `username` | Redis ACL username (optional, default: `default`) |
| `tls` | Enable TLS (`true`/`false`, default: `true`) |
| `database` | Redis database number (optional, default: `0`) |

### Global Settings

| Field | Description |
|-------|-------------|
| `files_api_key` | Files.com API key for support uploads |

## Credential Storage Options

### Plaintext (Default)

```toml
[profiles.dev]
deployment_type = "cloud"
api_key = "actual-key-value"
api_secret = "actual-secret-value"
```

!!! warning
    Credentials stored in plaintext. Use for development only.

### Environment Variables

```toml
[profiles.ci]
deployment_type = "cloud"
api_key = "${REDIS_CLOUD_API_KEY}"
api_secret = "${REDIS_CLOUD_SECRET_KEY}"
```

Variables are resolved at runtime.

### OS Keyring

```toml
[profiles.prod]
deployment_type = "cloud"
api_key = "keyring:prod-api-key"
api_secret = "keyring:prod-secret-key"
```

Credentials stored in:
- macOS: Keychain
- Windows: Credential Manager
- Linux: Secret Service

## Managing Profiles

### Create Profile

```bash
redisctl profile set myprofile --type cloud \
  --api-key "$KEY" \
  --api-secret "$SECRET"
```

### List Profiles

```bash
redisctl profile list
```

### Set Default

```bash
redisctl profile default-cloud myprofile
redisctl profile default-enterprise myprofile
redisctl profile default-database myprofile
```

### Delete Profile

```bash
redisctl profile remove myprofile
```

## Example Configurations

### Multi-Environment Setup

```toml
default_enterprise = "dev"

[profiles.dev]
deployment_type = "enterprise"
url = "https://dev-cluster:9443"
username = "admin@dev.local"
password = "${DEV_PASSWORD}"
insecure = true
tags = ["dev"]

[profiles.staging]
deployment_type = "enterprise"
url = "https://staging-cluster:9443"
username = "admin@staging.local"
password = "${STAGING_PASSWORD}"
tags = ["staging"]

[profiles.prod]
deployment_type = "enterprise"
url = "https://prod-cluster:9443"
username = "admin@prod.local"
password = "keyring:prod-password"
tags = ["prod"]
```

### Mixed Deployment Types

```toml
default_cloud = "cloud-prod"
default_enterprise = "ent-prod"
default_database = "cache-prod"

[profiles.cloud-prod]
deployment_type = "cloud"
api_key = "${REDIS_CLOUD_API_KEY}"
api_secret = "${REDIS_CLOUD_SECRET_KEY}"

[profiles.ent-prod]
deployment_type = "enterprise"
url = "https://cluster:9443"
username = "admin@cluster.local"
password = "${ENTERPRISE_PASSWORD}"

[profiles.cache-prod]
deployment_type = "database"
host = "redis-12345.cloud.redislabs.com"
port = 12345
password = "keyring:cache-password"
tls = true
```
