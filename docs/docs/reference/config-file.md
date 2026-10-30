# Configuration File

Profile and settings file format reference.

Profiles are persistent user data covered by the
[Compatibility and Support Policy](compatibility.md). The explicit 1.0 schema version and migration
mechanism are being established before the policy takes effect with redisctl 1.0.0.

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

Placeholders must be inside quoted TOML string values. TOML is parsed first, then string
values are expanded, including strings in arrays. Quoted table names and field names remain
literal; numeric and boolean fields must use TOML values, not unquoted placeholders.
Replacement text is used exactly as supplied by the environment, without interpreting its
quotes, backslashes, or newlines as TOML syntax. Both `$VAR` and `${VAR}` are supported.

Variables are resolved when the configuration is loaded. CLI commands and MCP profile tools
retain that document's references when saving unrelated edits, even if the environment changes
after loading. `${VAR:-default}` is kept whole, and each reference stays on its original field;
two profiles whose variables resolve to the same value retain their own references. A command
that intentionally replaces a field, such as the credentials supplied to `profile set`, writes
that replacement literally. Untouched credentials, tags, Files API keys and login endpoints
keep their references. An unset variable stays unexpanded and does not stop the file loading.

Writes refuse a source that has changed since loading rather than adopting another writer's
configuration. Reload and retry the command after resolving the conflicting edit. These checks
are not an atomic filesystem transaction. Rust embedders should use `ConfigDocument` for
provenance-aware load/edit/save operations; bare `Config` values do not retain load history.

### Migration: literal expansion and configuration writers

Configuration loading parses TOML **before** expanding string values. Older raw-text expansion
could change TOML syntax, table names, keys, numbers and booleans. Those forms are no longer
supported. For example, replace `port = ${PORT}` with a literal numeric port; quoting the
placeholder is not a workaround for a numeric field. Keep environment references in string
fields such as credentials, not field names or table names:

```toml
[profiles.cache]
deployment_type = "database"
host = "localhost"
port = 6379
tls = false
password = "${REDIS_PASSWORD}"
```

Parser diagnostics report a byte offset when available and a migration hint without printing
configuration excerpts or rejected values. In Rust, parser-error conversions now return
`ConfigError::InvalidDocument { byte_offset }`. The older `ParseError(toml::de::Error)` variant
remains available for callers constructing it explicitly, with redacted formatting and no
parser source chain. Exhaustive matches on `ConfigError` need to account for the new variant.

The public `Config::save`, `Config::save_to_path` and `Config::save_to_path_owner_only`
signatures remain, but are deprecated. They accept only caller-created **literal-only**
configurations. If either the existing destination or outgoing string values contain an
environment reference, the write fails with guidance to use `ConfigDocument`, before creating
directories or changing file contents/permissions. Malformed or unreadable destinations are
not overwritten. The guard uses the loader's reference syntax without reading environment
values; unset references and references in arrays are included.

A bare `Config` cannot tell whether an already-resolved literal came from another file's
environment reference. A new or literal-only destination is therefore not evidence that an
untracked save-as is safe. For **all load/edit/save and save-as operations**, retain the
document from the original load:

```rust
use redisctl_core::{ConfigDocument, ConfigPathSegment};

let mut document = ConfigDocument::load_from_path(path)?;
document.allow_literal_replacement(&[ConfigPathSegment::Key("default_database".into())]);
document.config_mut().default_database = Some("cache".into());
document.save()?;
```

This explicitly replaces only the selected default while preserving unrelated references.
`ConfigDocumentError` and `ConfigPathSegment` are non-exhaustive so consumers should retain a
fallback match arm. Configuration schema versions, unknown-field preservation and atomic
migrations remain separate work; this change does not establish those guarantees.

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
