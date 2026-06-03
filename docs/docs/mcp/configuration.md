# Configuration

The redisctl MCP server has three configuration axes:

1. **What to load** -- which toolsets and sub-modules are active (`--tools`)
2. **What to permit** -- which operations are allowed (`--read-only`, policy files)
3. **How to connect** -- transport and credentials (`--transport`, `--profile`, `--database-url`)

This page covers all three, with emphasis on `--tools` for controlling the tool surface.

## CLI Reference

| Flag | Short | Env Var | Default | Description |
|------|-------|---------|---------|-------------|
| `--transport` | `-t` | -- | `stdio` | Transport mode (`stdio` or `http`) |
| `--profile` | `-p` | `REDISCTL_PROFILE` | -- | Profile name(s) for credential resolution (repeatable) |
| `--read-only` | -- | -- | `true` | Read-only mode; use `--read-only=false` for writes. Ignored when a policy file is active |
| `--policy` | -- | `REDISCTL_MCP_POLICY` | -- | Path to TOML policy file for granular access control. Overrides `--read-only` |
| `--database-url` | -- | `REDIS_URL` | -- | Redis URL for direct database connections |
| `--tools` | -- | -- | -- | Comma-delimited toolset/sub-module selection (see below) |
| `--host` | -- | -- | `127.0.0.1` | HTTP bind host (HTTP transport only) |
| `--port` | -- | -- | `8080` | HTTP bind port (HTTP transport only) |
| `--oauth` | -- | -- | `false` | Enable OAuth authentication (HTTP transport only) |
| `--oauth-issuer` | -- | `OAUTH_ISSUER` | -- | OAuth issuer URL |
| `--oauth-audience` | -- | `OAUTH_AUDIENCE` | -- | OAuth audience |
| `--jwks-uri` | -- | `OAUTH_JWKS_URI` | -- | JWKS URI for token validation |
| `--max-concurrent` | -- | -- | `10` | Maximum concurrent requests |
| `--rate-limit-ms` | -- | -- | `100` | Rate limit interval in milliseconds |
| `--request-timeout-secs` | -- | -- | `30` | Request timeout in seconds (HTTP transport only) |
| `--log-level` | -- | `RUST_LOG` | `info` | Log level |

## The `--tools` Flag

By default, the MCP server loads all compiled-in toolsets. With `--tools`, you control exactly which toolsets and sub-modules are active. This is useful when you want to:

- Reduce the tool surface to what you actually need
- Keep token usage low by exposing fewer tool descriptions
- Scope an AI assistant to a specific domain (e.g., Cloud only)

### Syntax

```
--tools <spec>[,<spec>...]
```

Each `<spec>` is either:

- **Bare name** -- loads all sub-modules for that toolset: `cloud`, `enterprise`, `database`, `app`
- **Colon syntax** -- loads a single sub-module: `cloud:subscriptions`, `enterprise:observability`

Specs are comma-delimited. You can mix bare and colon forms freely.

### Resolution Priority

The server resolves which toolsets to load in this order:

1. **Explicit `--tools`** -- always wins when provided
2. **Auto-detect from profiles** -- infers toolsets from your configured profile types (e.g., a Cloud profile enables the `cloud` toolset)
3. **All compiled-in features** -- fallback when neither of the above applies

!!! note
    When `--tools` is not explicitly set, toolsets marked `enabled = false` in a policy file are also removed. When `--tools` is explicit, policy-based toolset disabling is skipped.

### Available Toolsets and Sub-Modules

| Toolset | Sub-modules | Total Tools |
|---------|-------------|-------------|
| `cloud` | `subscriptions`, `account`, `networking`, `fixed`, `raw` | 151 |
| `enterprise` | `cluster`, `databases`, `rbac`, `observability`, `proxy`, `services`, `raw` | 92 |
| `database` | `server`, `keys`, `structures`, `diagnostics`, `json`, `search`, `aliases`, `bulk`, `raw` | 139 |
| `app` | *(none -- flat toolset)* | 8 |
| *(system)* | *(always loaded)* | 2 |

The two system tools (`list_available_tools` and `show_policy`) are always registered regardless of `--tools` selection.

### Examples

**Cloud only** -- all Cloud sub-modules (151 tools + system):

```bash
redisctl-mcp --profile my-cloud --tools cloud
```

**Cloud subscriptions and networking only** (87 tools + system):

```bash
redisctl-mcp --profile my-cloud --tools cloud:subscriptions,cloud:networking
```

**Enterprise monitoring** -- cluster info + observability (40 tools + system):

```bash
redisctl-mcp --profile my-re --tools enterprise:cluster,enterprise:observability
```

**Database only** -- direct Redis operations (139 tools + system):

```bash
redisctl-mcp --database-url redis://localhost:6379 --tools database
```

**Minimal Cloud** -- just account info and subscriptions (69 tools + system):

```bash
redisctl-mcp --profile my-cloud --tools cloud:account,cloud:subscriptions
```

**Everything except database** -- Cloud + Enterprise + profile management:

```bash
redisctl-mcp --profile my-cloud --profile my-re --tools cloud,enterprise,app
```

**Mixed selective and bare** -- all Enterprise + specific Cloud sub-modules:

```bash
redisctl-mcp --tools enterprise,cloud:subscriptions,cloud:account
```

### Bare-Overrides-Selective Rule

If you specify both a bare name and colon-syntax for the same toolset, the bare name wins. For example:

```bash
# "cloud" overrides "cloud:subscriptions" -- all Cloud sub-modules are loaded
--tools cloud:subscriptions,cloud
```

This makes it easy to "upgrade" a selective choice to the full toolset without removing the specific entries.

### Error Behavior

The server exits with an error if:

- An unknown toolset name is used (e.g., `--tools nosuch`)
- An unknown sub-module is used (e.g., `--tools cloud:nosuch`)
- A sub-module is specified for `app`, which has no sub-modules (e.g., `--tools app:anything`)

Error messages include the list of valid toolset or sub-module names.

## Safety Tiers

Every MCP tool carries annotation hints that describe its safety characteristics:

- `readOnlyHint = true` -- reads data, never modifies state
- `destructiveHint = false` -- writes data but is non-destructive (create, update, backup)
- `destructiveHint = true` -- irreversible operation (delete, flush)

The server enforces three safety tiers that control which categories of operations are permitted:

| Tier | Flag / Policy Value | Behavior |
|------|-------------------|----------|
| **Read-only** | `--read-only` (default) / `"read-only"` | Only tools with `readOnlyHint = true` |
| **Read-write** | -- / `"read-write"` | Reads + non-destructive writes (`destructiveHint = false`) |
| **Full** | `--read-only=false` / `"full"` | All operations including destructive ones |

The `--read-only` CLI flag maps to read-only (`true`, default) and full (`false`) tiers. For the intermediate read-write tier, use a policy file.

Tools that fall outside the active tier are hidden from the AI and return an "unauthorized" error if called directly.

## Policy Files

Policy files give you granular control beyond the `--read-only` flag. A policy file is a TOML document that configures the safety tier, per-toolset overrides, explicit allow/deny lists, tool visibility presets, and audit logging.

!!! warning
    When a policy file is active, it overrides `--read-only`. The policy file is the authoritative source for safety configuration.

### Policy File Resolution

The server looks for a policy file in this order:

1. **`--policy` flag** -- explicit path always wins
2. **`REDISCTL_MCP_POLICY` env var** -- path from environment
3. **Default location** -- `~/.config/redisctl/mcp-policy.toml` (Linux/macOS)
4. **Built-in default** -- read-only tier with raw API tools denied

```bash
# Explicit path
redisctl-mcp --profile my-profile --policy /path/to/mcp-policy.toml

# Environment variable
REDISCTL_MCP_POLICY=/path/to/mcp-policy.toml redisctl-mcp --profile my-profile

# Auto-discovered from default location (no flags needed)
# Place your file at ~/.config/redisctl/mcp-policy.toml
redisctl-mcp --profile my-profile
```

Use the `show_policy` system tool at runtime to see which policy is active and where it was loaded from.

### Full TOML Schema

```toml
# Global default safety tier: "read-only" (default), "read-write", or "full"
tier = "read-write"

# Deny entire categories globally (currently supports "destructive")
deny_categories = ["destructive"]

# Global explicit allow list -- these tools are allowed regardless of tier
allow = ["backup_database"]

# Global explicit deny list -- these tools are always blocked (wins over allow)
deny = ["flush_database", "delete_subscription"]

# Per-toolset overrides
[cloud]
enabled = true           # true (default) or false to disable the entire toolset
tier = "read-write"      # overrides the global tier for Cloud tools
allow = []               # per-toolset allow list
deny = []                # per-toolset deny list

[enterprise]
enabled = true
tier = "read-only"

[database]
tier = "read-only"
allow = ["redis_set", "redis_expire"]  # allow specific writes despite read-only tier

[app]
# app toolset has no sub-modules; omit to use global defaults

# Tool visibility presets
[tools]
preset = "all"           # "all" (default) or "essentials"
include = []             # add specific tools on top of the preset
exclude = []             # remove specific tools from the resolved set

# Audit logging
[audit]
enabled = false          # enable/disable audit logging
level = "all"            # "all", "denied", or "mutations"
include_args = false     # include tool arguments in log entries
redact_fields = ["password", "secret_key"]  # redact sensitive fields
```

All fields are optional. An empty file is equivalent to the default read-only policy.

### Evaluation Order

When a tool is invoked, the policy evaluates access in this order:

1. **Global deny list** -- if the tool is in `deny`, it is blocked
2. **Per-toolset deny list** -- if the tool is in its toolset's `deny`, it is blocked
3. **Category deny** -- if `deny_categories` includes `"destructive"` and the tool has `destructiveHint = true`, it is blocked
4. **Global allow list** -- if the tool is in `allow`, it is permitted (overrides tier)
5. **Per-toolset allow list** -- if the tool is in its toolset's `allow`, it is permitted (overrides tier)
6. **Per-toolset tier** -- if the toolset has a `tier` override, evaluate the tool's annotations against it
7. **Global tier** -- evaluate the tool's annotations against the global `tier`

Key rules:

- **Deny always wins over allow.** A tool in both `allow` and `deny` is blocked.
- **Per-toolset tier overrides global tier** for tools in that toolset.
- **Allow lists override tier restrictions.** You can allow specific write tools even at read-only tier.

### Per-Toolset Overrides

Each toolset (`cloud`, `enterprise`, `database`, `app`) can have its own policy section that overrides the global settings:

```toml
# Global: read-only
tier = "read-only"

# Cloud: allow non-destructive writes
[cloud]
tier = "read-write"

# Enterprise: stay read-only (inherits global)

# Database: read-only but allow SET and EXPIRE
[database]
allow = ["redis_set", "redis_expire"]
```

You can also disable an entire toolset, preventing its tools from being registered at all:

```toml
[enterprise]
enabled = false

[database]
enabled = false
```

!!! note
    `enabled = false` prevents tools from being registered with the MCP router. This is different from deny lists, which register the tool but block invocation. Disabled toolsets save memory and reduce tool discovery noise.

### Raw API Tools

Three "raw" passthrough tools provide direct API/command access:

- `cloud_raw_api` -- arbitrary Redis Cloud API calls
- `enterprise_raw_api` -- arbitrary Redis Enterprise API calls
- `redis_command` -- arbitrary Redis commands

These are powerful but potentially dangerous. By default (when no policy file is loaded), all three are denied. When you load a custom policy file, raw tools follow normal tier/allow/deny rules -- they are not auto-denied.

To explicitly enable raw tools in a policy file:

```toml
tier = "full"
# raw tools are allowed because tier is full and deny list is empty
```

To enable raw tools selectively:

```toml
tier = "read-only"
allow = ["redis_command"]  # allow redis_command despite read-only tier
```

To keep raw tools denied in a custom policy:

```toml
tier = "full"
deny = ["cloud_raw_api", "enterprise_raw_api", "redis_command"]
```

!!! tip
    The `redis_command` tool has its own built-in blocklist that prevents dangerous commands like `SHUTDOWN`, `DEBUG`, `CLUSTER FAILOVER`, and others regardless of policy tier. This provides defense-in-depth even at full tier.

## Presets

Presets control tool **visibility** -- which tools are presented to the AI model. This is independent of `--tools` (which controls what is *loaded*) and safety tiers (which control what is *permitted*).

Two presets are available:

| Preset | Description |
|--------|-------------|
| `"all"` | Every loaded tool is visible (default) |
| `"essentials"` | A curated subset per toolset: ~20 Cloud, ~18 Enterprise, ~15 Database, all 8 App tools |

Configure presets in the policy file:

```toml
[tools]
preset = "essentials"
include = ["enterprise_raw_api"]  # add tools on top of the preset
exclude = ["flush_database"]      # remove tools from the resolved set
```

!!! tip
    Use the `list_available_tools` system tool at runtime to see which tools are active vs. hidden under the current preset. This lets you discover tools you might want to add to the `include` list.

## Practical Examples

### Read-only exploration (default)

No policy file needed. Just run the server:

```bash
redisctl-mcp --profile my-profile
```

### Development environment with writes

Allow creates and updates, but block destructive operations:

```toml
# ~/.config/redisctl/mcp-policy.toml
tier = "read-write"
```

### Production monitoring

Read-only with only Cloud and Enterprise, no database tools:

```toml
tier = "read-only"

[database]
enabled = false
```

### CI/CD automation

Full access with audit logging enabled:

```toml
tier = "full"

[audit]
enabled = true
level = "mutations"
include_args = true
redact_fields = ["password", "secret_key", "api_key"]
```

### Locked-down shared environment

Read-only globally, allow specific Cloud writes, deny all destructive operations:

```toml
tier = "read-only"
deny_categories = ["destructive"]

[cloud]
tier = "read-write"
deny = ["delete_subscription", "delete_database"]
```

### Minimal tool surface

Essentials preset with a few additions:

```toml
tier = "read-write"

[tools]
preset = "essentials"
include = ["get_enterprise_crdb", "list_enterprise_crdb"]
exclude = ["flush_database"]
```

## Choosing the Right Approach

| Goal | Approach |
|------|----------|
| Quick read-only exploration | Default settings (no extra flags) |
| Allow writes but not destructive ops | Policy file with `tier = "read-write"` |
| Full access for development | `--read-only=false` or policy with `tier = "full"` |
| Limit to one product (Cloud or Enterprise) | `--tools cloud` or `--tools enterprise` |
| Reduce tool surface within a product | `--tools cloud:subscriptions,cloud:account` |
| Curate which tools the AI sees | Policy file with `preset = "essentials"` |
| Fine-grained per-tool control | Policy file with `allow` / `deny` lists |
| Per-toolset safety levels | Policy file with `[cloud]`, `[enterprise]` sections |
| Block all destructive ops regardless of tier | Policy file with `deny_categories = ["destructive"]` |
| Enable raw API passthrough | Policy file with `tier = "full"` (or `allow` specific raw tools) |

## Next Steps

- [Tools Reference](tools-reference.md) -- see what tools are available per toolset and sub-module
- [Getting Started](getting-started.md) -- installation and IDE setup
- [Advanced Usage](advanced-usage.md) -- JMESPath integration and analytics
