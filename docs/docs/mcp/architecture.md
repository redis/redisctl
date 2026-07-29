# Architecture

How the redisctl MCP server is structured internally.

## Overview

`redisctl-mcp` is a standalone binary built on [tower-mcp](https://crates.io/crates/tower-mcp), a Rust MCP framework. It exposes Redis management operations as MCP tools that AI assistants can discover and invoke.

```
AI Assistant (Claude, Cursor, etc.)
    |
    | MCP protocol (stdio or HTTP/SSE)
    v
redisctl-mcp
    |
    +-- Policy engine (tier checks, allow/deny lists)
    +-- Audit layer (structured logging of tool calls)
    +-- Tool router (392 tools across 4 toolsets)
    |       |
    |       +-- Cloud tools -> redis-cloud client -> Cloud REST API
    |       +-- Enterprise tools -> redis-enterprise client -> Enterprise REST API
    |       +-- Database tools -> redis crate -> Redis protocol
    |       +-- App tools -> redisctl-core -> local config
    |
    +-- Credential resolution (profiles or direct database URL)
```

## Feature Flags

The binary is compiled with feature flags that gate platform-specific dependencies:

| Feature | Default | Gates |
|---------|---------|-------|
| `cloud` | yes | `redis-cloud` crate + cloud toolset |
| `enterprise` | yes | `redis-enterprise` crate + enterprise toolset |
| `database` | yes | `redis` crate + database toolset |
| `http` | yes | Unauthenticated HTTP/SSE transport |
| `secure-storage` | yes | Keyring-backed profile credentials through `redisctl-core` |

Profile/app tools and the two system tools are always compiled in (they only depend on `redisctl-core`).

Compile with `--no-default-features` and selectively enable what you need:

```bash
# Cloud-only binary
cargo install redisctl-mcp --no-default-features --features cloud

# Database-only binary
cargo install redisctl-mcp --no-default-features --features database
```

## Toolset and Sub-Module System

Tools are organized into **toolsets** (cloud, enterprise, database, app) and **sub-modules** within each toolset. Each sub-module is a Rust module that declares:

- A `TOOL_NAMES` constant listing all tool names in the module
- A `router()` function that registers tools with the MCP router

The `mcp_module!` macro generates both from a single declaration:

```rust
mcp_module! {
    ping => "redis_ping",
    info => "redis_info",
    dbsize => "redis_dbsize",
}
```

This produces a `TOOL_NAMES` array and a `router()` function that builds tools and merges them into the MCP router.

### Tool Macros

Three platform-specific macros eliminate per-tool boilerplate:

- `database_tool!` -- resolves a Redis connection from URL or profile, then runs the handler
- `cloud_tool!` -- resolves a Cloud API client from profile, then runs the handler
- `enterprise_tool!` -- resolves an Enterprise API client from profile, then runs the handler

Each macro accepts a safety tier (`read_only`, `write`, or `destructive`) that sets the tool's MCP annotations and generates runtime permission guards:

```rust
database_tool!(read_only, ping, "redis_ping",
    "Test connectivity by sending a PING command",
    {} => |conn, _input| {
        let response: String = redis::cmd("PING")
            .query_async(&mut conn).await.tool_context("PING failed")?;
        Ok(CallToolResult::text(format!("Response: {}", response)))
    }
);
```

## Transport Modes

### stdio (default)

Standard input/output. The AI assistant spawns the `redisctl-mcp` process and communicates over stdin/stdout. This is the standard MCP transport used by Claude Desktop, Cursor, Claude Code, and others.

### HTTP/SSE

HTTP transport with Server-Sent Events for streaming. Useful for shared deployments where multiple clients connect to a single MCP server instance.

```bash
redisctl-mcp --transport http --host 0.0.0.0 --port 8080
```

The HTTP transport does not provide built-in authentication. Keep the default
loopback bind for local use. Shared deployments must put the server behind a
trusted gateway or reverse proxy that provides authentication, authorization,
and TLS.

## Credential Resolution

Credentials are resolved at tool invocation time, not at server startup. This allows multi-profile configurations where different tools target different clusters.

### Profile-based (stdio mode)

1. Tool call includes optional `profile` parameter
2. If no profile specified, use the first `--profile` from CLI args
3. If no CLI profile, fall back to the default profile from `~/.config/redisctl/config.toml`
4. Resolve credentials from the profile (including keyring lookups)
5. Build or reuse a cached API client for that profile

## Policy Engine

The policy engine evaluates every tool call against the active policy before execution. See [Configuration](configuration.md) for the full policy reference.

The evaluation flow:

1. **Registration-time filtering** -- tools outside the active tier are hidden from the AI (not registered with the router)
2. **Runtime guards** -- write and destructive tools check the policy tier again inside the handler as defense-in-depth
3. **Allow/deny lists** -- evaluated per the [policy evaluation order](configuration.md#evaluation-order)

## Audit Layer

A tower middleware (`AuditLayer`) sits between the transport and the tool router. It intercepts every tool call and emits structured events via the `tracing` crate with `target: "audit"`. See [Audit Logging](audit-logging.md) for configuration details.

## Concurrency and Timeouts

The server bounds request concurrency and HTTP request duration via CLI flags:

- `--max-concurrent 10` -- maximum parallel tool calls (default: 10)
- `--request-timeout-secs 30` -- per-request timeout for HTTP transport (default: 30)

## Request Flow

A typical tool call flows through these layers:

1. **Transport** -- receives MCP JSON-RPC message (stdio or HTTP)
2. **Audit layer** -- records the call, starts timer
3. **Capability filter** -- checks if the tool is visible under the current preset
4. **Router** -- dispatches to the tool handler
5. **Tool handler** -- runs the platform macro which:
    - Checks safety tier (write/destructive guard)
    - Resolves credentials and builds/reuses API client
    - Executes the operation
    - Returns `CallToolResult`
6. **Audit layer** -- records result status and duration
7. **Transport** -- sends MCP JSON-RPC response
