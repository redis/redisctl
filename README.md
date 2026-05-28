# redisctl

**Manage Redis Cloud, Redis Enterprise, and Redis databases from one tool** -- as a CLI for humans or an MCP server for AI agents.

[![Crates.io](https://img.shields.io/crates/v/redisctl.svg)](https://crates.io/crates/redisctl)
[![CI](https://github.com/redis/redisctl/actions/workflows/ci.yml/badge.svg)](https://github.com/redis/redisctl/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/redis/redisctl#license)

redisctl replaces curl-and-jq scripts against the Redis Cloud and Enterprise REST APIs with a single binary that handles authentication, async polling, output formatting, and error handling. The same tool ships as an MCP server so AI assistants can manage Redis infrastructure directly.

---

## Pick Your Path

| I want to... | Start here |
|---|---|
| Manage **Redis Cloud** from my terminal | [CLI Quick Start](#quick-start) |
| Manage **Redis Enterprise** from my terminal | [CLI Quick Start](#quick-start) |
| Let an **AI assistant** manage Redis | [MCP Server](#mcp-server) |
| Query or inspect a **Redis database** directly | [Database Tools](#database-tools) |

---

## Quick Start

### Install

```bash
# Homebrew (macOS/Linux)
brew install redis/homebrew-tap/redisctl

# Cargo
cargo install redisctl

# Binary releases: https://github.com/redis/redisctl/releases
```

### Configure a Profile

```bash
# Redis Cloud
redisctl profile set prod \
  --deployment cloud \
  --api-key "$REDIS_CLOUD_API_KEY" \
  --api-secret "$REDIS_CLOUD_SECRET_KEY"

# Redis Enterprise
redisctl profile set dev \
  --deployment enterprise \
  --url "https://cluster.local:9443" \
  --username "admin@redis.local" \
  --password "$REDIS_ENTERPRISE_PASSWORD"
```

### Run Commands

```bash
# List databases (platform inferred from profile)
redisctl database list

# Create a database and wait for it to be ready
redisctl database create @db-config.json --wait

# Get cluster info as a table
redisctl cluster get -o table

# Filter output with JMESPath
redisctl database list -q 'databases[?status==`active`].name'
```

The `cloud`/`enterprise` prefix is optional -- the CLI infers the platform from your profile. Use explicit prefixes (`redisctl cloud database list`) in scripts or when you have profiles for both platforms.

---

## MCP Server

`redisctl-mcp` exposes 300+ tools to AI assistants (Claude Desktop, Cursor, VS Code, or any MCP client). It covers the full Cloud and Enterprise APIs plus direct Redis database operations -- all with a safety-first policy system.

### Set Up

```json
{
  "mcpServers": {
    "redisctl": {
      "command": "redisctl-mcp",
      "args": ["--profile", "my-profile"]
    }
  }
}
```

**Read-only by default.** Write and destructive operations require explicit opt-in via a policy file (`mcp-policy.toml`):

```toml
tier = "read-write"  # read-only (default) | read-write | full
```

See the [MCP configuration docs](https://redis-field-engineering.github.io/redisctl-docs/mcp/configuration/) for the full policy schema and precedence rules.

### Zero-Install with Docker

`REDIS_ENTERPRISE_PASSWORD` is passed without a value so Docker forwards it from your host environment (keeps the secret out of the config file):

```json
{
  "mcpServers": {
    "redisctl": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "REDIS_ENTERPRISE_URL=https://cluster:9443",
        "-e", "REDIS_ENTERPRISE_USER=admin@redis.local",
        "-e", "REDIS_ENTERPRISE_PASSWORD",
        "ghcr.io/redis/redisctl",
        "redisctl-mcp"
      ]
    }
  }
}
```

See the [MCP documentation](https://redis-field-engineering.github.io/redisctl-docs/mcp/) for client configuration guides, the full tool catalog, and safety policy reference.

---

## What's Covered

### Redis Cloud -- Full API Coverage

Subscriptions, databases, VPC peering, Transit Gateway, PrivateLink, Private Service Connect, ACLs, cloud accounts, tasks, and async operations.

### Redis Enterprise -- Full API Coverage

Clusters, nodes, shards, databases (BDBs), Active-Active (CRDBs), users, roles, LDAP, logs, metrics, alerts, support packages, and diagnostics.

### Database Tools

Connect directly to any Redis instance for key inspection, data structure operations, server diagnostics, and health checks. Available in both the CLI and MCP server.

### Key Capabilities

- **Async operations** -- `--wait` automatically polls long-running operations to completion
- **Output formats** -- tables, JSON, YAML, with JMESPath filtering (`-q`)
- **Profiles** -- manage multiple environments with optional keyring-backed credential storage
- **Workflows** -- high-level commands that compose multi-step operations (e.g., `subscription-setup`)
- **Raw API access** -- `redisctl api cloud get /subscriptions/12345` for any endpoint
- **Streaming** -- `--follow` for real-time log tailing

---

## Documentation

**[Full Documentation](https://redis-field-engineering.github.io/redisctl-docs/)**

- [Getting Started](https://redis-field-engineering.github.io/redisctl-docs/getting-started/)
- [Configuration](https://redis-field-engineering.github.io/redisctl-docs/configuration/)
- [MCP Server](https://redis-field-engineering.github.io/redisctl-docs/mcp/)
- [Command Reference](https://redis-field-engineering.github.io/redisctl-docs/reference/)
- [Workflows](https://redis-field-engineering.github.io/redisctl-docs/workflows/)

---

## Changelogs

- [redisctl CLI](crates/redisctl/CHANGELOG.md)
- [redisctl-core](crates/redisctl-core/CHANGELOG.md)
- [redisctl-mcp](crates/redisctl-mcp/CHANGELOG.md)

API client libraries (separate repositories):
- [redis-cloud](https://github.com/redis-developer/redis-cloud-rs)
- [redis-enterprise](https://github.com/redis-developer/redis-enterprise-rs)

---

## Contributing

```bash
git clone https://github.com/redis/redisctl.git
cd redisctl
cargo build --release
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

See the [Contributing Guide](https://redis-field-engineering.github.io/redisctl-docs/developer/contributing.html).

---

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
