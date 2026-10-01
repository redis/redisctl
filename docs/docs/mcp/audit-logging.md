# Audit Logging

Track every tool invocation for compliance, debugging, and operational visibility.

## Enabling Audit Logging

Audit logging is disabled by default. Enable it in your policy file:

```toml
[audit]
enabled = true
```

Or for a more complete configuration:

```toml
[audit]
enabled = true
level = "all"
include_args = true
redact_fields = ["password", "db_password", "global_password", "api_key", "api_secret", "access_secret_key", "console_password", "bind_pass", "license_key", "secret", "token"]
```

## Configuration Reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `false` | Master switch for audit logging |
| `level` | string | `"all"` | Which events to log (see levels below) |
| `include_args` | bool | `false` | Include tool arguments in log entries |
| `redact_fields` | list | Common password, API credential, key, secret, and token fields | Additional field names to redact when `include_args` is true |

## Audit Levels

| Level | Logs |
|-------|------|
| `all` | Every tool call (success, error, and denied) |
| `writes` | Write and destructive calls, including their errors, plus every policy denial |
| `destructive` | Destructive calls, including their errors, plus every policy denial |
| `denied` | Only policy-denied calls |

Policy denials are logged at every level. Other tool errors follow the safety
classification of the tool that was called. The previously documented
`"mutations"` value remains accepted as an alias for `"writes"`, but new
configuration should use `"writes"`.

Choose based on your needs:

- **Production monitoring**: `"denied"` -- catch policy violations without log noise
- **Development/debugging**: `"all"` -- see everything
- **Compliance**: `"writes"` or `"all"` with `include_args = true` -- full mutation audit trail

## Log Format

Audit events are emitted as structured JSON via the `tracing` crate with `target: "audit"`. When using JSON log output (`--log-level` with `RUST_LOG` filtering), each event includes:

```json
{
  "timestamp": "2026-03-05T10:30:00.123Z",
  "level": "INFO",
  "target": "audit",
  "fields": {
    "event": "tool_invocation",
    "tool": "list_enterprise_databases",
    "toolset": "enterprise",
    "result": "success",
    "duration_ms": 45
  }
}
```

### Event Types

| Event | Description |
|-------|-------------|
| `tool_invocation` | Tool was called and completed successfully |
| `tool_denied` | Tool call was blocked by the router or a handler-time policy guard |
| `tool_error` | Tool call failed with an error |

### With Arguments

When `include_args = true`, an `arguments` field is added containing the tool's input parameters. Built-in credential fields and the additional fields listed in `redact_fields` are replaced with `[REDACTED]`:

```json
{
  "fields": {
    "event": "tool_invocation",
    "tool": "create_enterprise_database",
    "toolset": "enterprise",
    "result": "success",
    "duration_ms": 1200,
    "arguments": "{\"name\":\"cache-db\",\"memory_size\":104857600,\"password\":\"[REDACTED]\"}"
  }
}
```

Redaction is recursive and matches common credential-field spellings across
snake_case, kebab-case, and camelCase. Redis credentials embedded in
`redis://`, `rediss://`, `redis+unix://`, and `unix://` URLs are always
sanitized. The arbitrary-command tools retain the command name for diagnosis
but always redact every positional operand, since Redis command arguments can
carry credentials or application secrets. These mandatory protections cannot
be disabled by changing `redact_fields`.

## Routing Audit Logs

Audit events use `target: "audit"` so you can route them separately from application logs using `RUST_LOG` filtering:

```bash
# All logs at info, audit at debug
RUST_LOG=info,audit=debug redisctl-mcp --profile my-profile

# Only audit logs
RUST_LOG=warn,audit=info redisctl-mcp --profile my-profile
```

### Log Aggregation

To send audit logs to an external system, pipe JSON output to your collector:

```bash
# Datadog
redisctl-mcp --profile my-profile 2>&1 | datadog-agent pipe

# Fluentd
redisctl-mcp --profile my-profile 2>&1 | fluent-cat audit.mcp

# File-based (rotate with logrotate)
redisctl-mcp --profile my-profile 2>> /var/log/redisctl-mcp-audit.jsonl
```

Since the MCP server uses stderr for logs and stdout for MCP protocol messages, redirect stderr (`2>`) to capture audit output without interfering with the MCP transport.

## Practical Examples

### Compliance: Full Mutation Audit

Track all state-changing operations with arguments:

```toml
tier = "read-write"

[audit]
enabled = true
level = "writes"
include_args = true
redact_fields = ["password", "api_key", "api_secret", "secret", "token"]
```

### Security: Denied Operations Only

Alert on policy violations:

```toml
tier = "read-only"

[audit]
enabled = true
level = "denied"
```

### Debugging: Everything

Full visibility during development:

```toml
tier = "full"

[audit]
enabled = true
level = "all"
include_args = true
```
