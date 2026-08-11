# redisctl-mcp

MCP (Model Context Protocol) server for Redis Cloud and Enterprise management.

This standalone binary exposes Redis management operations as tools that AI assistants
like Claude can use to help manage your Redis infrastructure.

## Installation

```bash
# From source
cargo install --path crates/redisctl-mcp

# Or build directly
cargo build --release -p redisctl-mcp
```

## Quick Start

### With Claude Desktop

Add to your Claude Desktop configuration (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "redis": {
      "command": "/path/to/redisctl-mcp",
      "args": ["--profile", "my-profile"]
    }
  }
}
```

### With Claude Code

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "redis": {
      "command": "redisctl-mcp",
      "args": ["--profile", "default"]
    }
  }
}
```

## Usage

### Stdio Transport (Default)

For local integrations with Claude Desktop, Claude Code, or other MCP clients:

```bash
# Use the default profile
redisctl-mcp

# Use a specific profile
redisctl-mcp --profile production

# Enable write operations (read-only is the default)
redisctl-mcp --profile production --read-only=false

# With a direct Redis database connection
redisctl-mcp --database-url redis://localhost:6379
```

### HTTP Transport

The HTTP transport is unauthenticated. Keep the default loopback bind for local
use, or put shared deployments behind a trusted gateway or reverse proxy that
provides authentication, authorization, and TLS.

```bash
# Local HTTP server
redisctl-mcp --transport http --port 8080

# With custom concurrency and timeout limits
redisctl-mcp --transport http --port 8080 \
  --max-concurrent 20 \
  --request-timeout-secs 60
```

## Available Tools

### Redis Cloud

| Tool | Description |
| --- | --- |
| `list_subscriptions` | List all Redis Cloud subscriptions |
| `get_subscription` | Get details of a specific subscription |
| `list_databases` | List databases in a subscription |
| `get_database` | Get database configuration details |

### Redis Enterprise

| Tool | Description |
| --- | --- |
| `get_cluster` | Get cluster information (name, version, config) |
| `list_enterprise_databases` | List all databases on the cluster |
| `get_enterprise_database` | Get database details by UID |
| `list_nodes` | List all cluster nodes |

### Direct Redis Operations

| Tool | Description |
| --- | --- |
| `redis_ping` | Test connectivity to a Redis database |
| `redis_info` | Get Redis INFO output (optionally by section) |
| `redis_keys` | List keys matching a pattern (uses SCAN) |

## Configuration

### Profile-Based Authentication

The server uses `redisctl` profiles for credential management. Configure profiles in `~/.config/redisctl/config.toml`:

```toml
default_cloud_profile = "cloud-prod"
default_enterprise_profile = "enterprise-dev"

[profiles.cloud-prod]
type = "cloud"
api_key = "${REDIS_CLOUD_API_KEY}"
api_secret = "${REDIS_CLOUD_SECRET_KEY}"

[profiles.enterprise-dev]
type = "enterprise"
url = "https://cluster.example.com:9443"
username = "admin"
password = "${RE_PASSWORD}"
insecure = true
```

### Environment Variables

For deployments that use environment-backed credentials instead of storing
credential values directly in profiles:

```bash
# Redis Cloud
export REDIS_CLOUD_API_KEY=your-key
export REDIS_CLOUD_SECRET_KEY=your-secret

# Redis Enterprise
export REDIS_ENTERPRISE_URL=https://cluster:9443
export REDIS_ENTERPRISE_USER=admin
export REDIS_ENTERPRISE_PASSWORD=secret
export REDIS_ENTERPRISE_INSECURE=true  # optional, for self-signed certs

# Direct Redis connection
export REDIS_URL=redis://localhost:6379
```

## Command Line Options

```text
Options:
  -t, --transport <TRANSPORT>      Transport mode [default: stdio]
                                   - stdio: For CLI integrations
                                   - http: For web deployments
  -p, --profile <PROFILE>          Profile name(s) for credentials; repeatable
      --read-only <BOOL>           Read-only mode [default: true]
      --policy <PATH>              TOML policy file
      --database-url <URL>         Redis URL for direct connections
      --cluster                    Enable Redis Cluster mode
      --client-name <NAME>         Redis client name [default: redisctl-mcp]
      --tools <SPECS>              Toolsets or sub-modules to expose
      --skills-dir <PATH>          Directory of SKILL.md prompt packages

  HTTP Options:
      --host <HOST>                Bind host [default: 127.0.0.1]
      --port <PORT>                Bind port [default: 8080]
      --max-concurrent <N>         Max concurrent requests [default: 10]
      --request-timeout-secs <S>   Request timeout [default: 30]

  Logging:
      --log-level <LEVEL>          Log level [default: info]
```

## Library Usage

You can build the same policy-filtered router used by the binary. The builder
installs policy, visibility presets, system tools, prompts, skills, and server
instructions as one unit:

```rust
use redisctl_mcp::{CredentialSource, McpServerBuilder, PolicyConfig};

fn build_router() -> anyhow::Result<tower_mcp::McpRouter> {
    let server = McpServerBuilder::new(
        CredentialSource::Profiles(vec!["default".to_string()]),
        PolicyConfig::default(), // read-only by default
        "embedded default",
    )
    .with_tool_specs(["cloud", "enterprise", "app"])?
    .with_client_name(Some("my-embedded-server".to_string()))
    .build()?;

    Ok(server.into_router())
}
```

The `test-support` feature exposes unstable direct tool constructors for this
repository's integration tests. It is not part of the supported 1.x Rust API.

## Security Considerations

- Keep the default read-only mode unless write tools are explicitly required
- Keep HTTP bound to loopback or protect it with an authenticating gateway
- Store credentials using environment variables or secure credential storage
- The server respects profile-based credential isolation
