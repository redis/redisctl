# AGENTS.md

Context and instructions for AI coding agents working on the redisctl project.

## Project Overview

redisctl is a Rust workspace with three crates that share a lockstep version (currently 0.9.1):

| Crate | Path | Purpose |
|-------|------|---------|
| **redisctl-core** | `crates/redisctl-core/` | Shared library: config/profile management, Cloud and Enterprise API clients, error handling |
| **redisctl** | `crates/redisctl/` | CLI binary: commands, workflows, connection management |
| **redisctl-mcp** | `crates/redisctl-mcp/` | MCP server for AI agents: ~371 tools, policy engine, skills system |

## Build and Test

```bash
# Format (required before push)
cargo fmt --all

# Lint (all warnings are errors in CI)
cargo clippy --all-targets --all-features -- -D warnings

# Unit tests
cargo test --workspace

# Integration tests (requires Docker for Redis)
cargo test --workspace --test '*' --all-features

# Reuse Docker containers across test runs
REUSE_CONTAINERS=1 cargo test --workspace --test '*' --all-features
```

**Always run fmt + clippy before pushing.** CI will reject unformatted code or clippy warnings.

## CI Checks

PRs must pass:
1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. Unit tests per crate (parallel)
4. Integration tests
5. Platform builds (Linux required; macOS and Windows are optional)

## Crate Architecture

### Feature Flags

**redisctl** (CLI):
- `default = ["full", "secure-storage"]`
- `full = ["cloud", "enterprise", "upload"]`
- `secure-storage` enables keyring-based credential storage

**redisctl-mcp** (MCP server):
- `default = ["http", "cloud", "enterprise", "database", "secure-storage"]`
- `secure-storage` enables keyring-backed profile credentials
- Each toolset (`cloud`, `enterprise`, `database`) can be compiled independently

### Key Patterns

**database_tool! macro** (`crates/redisctl-mcp/src/tools/macros.rs`):
Generates MCP tool structs with safety annotations. Three safety tiers:
- `database_tool!(read_only, ...)` -- no side effects
- `database_tool!(write, ...)` -- creates or modifies data
- `database_tool!(destructive, ...)` -- irreversible operations

Field attributes like `#[serde(...)]` pass through the macro. Numeric parameters that might arrive as strings from MCP clients use custom deserializers from `serde_helpers.rs`.

**mcp_module! macro**: Generates `TOOL_NAMES` constant and `router()` function for each tool sub-module.

**Policy system** (`crates/redisctl-mcp/src/policy.rs`): TOML-based policy engine with three safety tiers (ReadOnly, ReadWrite, Full), per-toolset overrides, and allow/deny lists.

**Profile system** (`crates/redisctl-core/`): TOML config at `~/.config/redisctl/config.toml` with profiles for Cloud, Enterprise, and direct Redis connections. Supports env var substitution.

### Module Organization

MCP tools are organized into toolsets with sub-modules:
```
tools/
  redis/          # ~125 tools: server, keys, structures, json, search, diagnostics, bulk
  cloud/          # ~147 tools: subscriptions, account, networking, fixed
  enterprise/     # ~91 tools: cluster, databases, rbac, observability, proxy, services
  profile.rs      # 8 tools: profile management
```

CLI commands mirror this structure:
```
cli/
  cloud.rs        # Cloud subcommands
  enterprise.rs   # Enterprise subcommands
  mod.rs          # Top-level command tree
```

## Adding a Command

Enterprise command groups (`database`, `cluster`, `node`, `crdb`, `logs`, `module`, `rbac`) share
one dispatch shape. Follow it when adding a group or a subcommand; do not introduce a new pattern.

The group is split across three places:

1. **The clap enum.** A `#[derive(Subcommand)]` enum holding the subcommands, their `#[arg]`
   attributes, aliases and `after_help` examples.
2. **The dispatch module** (`commands/enterprise/<name>.rs`): one free function that matches on the
   enum and calls a handler per variant. No API calls, no output formatting.
3. **The implementation module** (`commands/enterprise/<name>_impl.rs`): one function per
   subcommand, holding the API call, the table types and the output formatting.

### Checklist

- [ ] **Declare the enum.** Placement varies by group and both existing choices are fine:
      `cli/enterprise.rs` as `Enterprise<Name>Commands` (used by `database`, `cluster`, `node`,
      `crdb`), or the group's own `commands/enterprise/<name>.rs` as `<Name>Commands` (used by
      `logs`, `module`). Prefer `cli/enterprise.rs` for a group whose variants carry many flags,
      so the CLI surface stays readable in one file.
- [ ] **Add the variant** to `EnterpriseCommands` in `cli/enterprise.rs` wrapping the new enum.
- [ ] **Write the dispatch function** in `commands/enterprise/<name>.rs` with the signature shown
      below. Take `&ConnectionManager`; never build one from a `&Config`. `conn_mgr.config` is
      public if a handler needs the profile list. `CliResult` is `crate::error::Result`.
- [ ] **Write the handlers** in `commands/enterprise/<name>_impl.rs`, one per subcommand, each
      taking `conn_mgr`/`profile_name` plus its own arguments and returning `CliResult<()>`.
- [ ] **Register both modules** in `commands/enterprise/mod.rs` (`pub mod <name>;` and
      `pub mod <name>_impl;`).
- [ ] **Wire the dispatch** in the `EnterpriseCommands` match in `main.rs`, passing
      `conn_mgr, profile, cmd, output, query`.
- [ ] **Run the checks**: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`,
      `cargo test --workspace`.

### Dispatch signature

```rust
pub async fn handle_<name>_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &<Name>Commands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()>
```

### Handler body

```rust
let client = conn_mgr.create_enterprise_client(profile_name).await?;
let response = client.get_raw("/v1/...").await.map_err(RedisCtlError::from)?;

let data = handle_output(response, output_format, query)?;
if matches!(resolve_auto(output_format), OutputFormat::Table) {
    print_<name>_table(&data)?;
} else {
    print_formatted_output(data, output_format)?;
}
Ok(())
```

`handle_output`, `print_formatted_output` and `resolve_auto` come from `super::utils::*`, which
re-exports them from `crate::output`.

Two variations exist in the tree and are not worth churning: `logs` keeps its dispatch function in
`logs_impl.rs` rather than `logs.rs`, and a few older groups still pass their command enum by value
(`cmd.clone()`) at the `main.rs` call site. New groups should take the enum by reference.

## Conventions

### Commits

Follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat:` new feature (minor version bump)
- `fix:` bug fix (patch bump)
- `docs:` documentation only
- `refactor:` no behavior change
- `test:` test additions or fixes
- `chore:` maintenance

Scope by crate when relevant: `feat(mcp):`, `fix(core):`, `docs(cli):`.

### Code Style

- Rust edition 2024, MSRV 1.90+
- No `unwrap()` in production code -- use proper error handling
- Prefer `anyhow` for CLI errors, `tower_mcp::Error` for MCP tool errors
- Tool descriptions are the primary documentation for MCP tools -- keep them accurate and concise
- Use long-only `--force` for destructive confirmation bypasses; add new destructive command paths
  to the command-tree test inventory in `crates/redisctl/src/main.rs`

### PRs

- Use feature branches (`feat/`, `fix/`, `docs/`)
- Open Draft PRs early
- Squash and merge to main
- Versions are bumped automatically by release-plz based on commit types

## Dependencies

API client crates are external git dependencies on the `main` branch:
- `redis-cloud` from `github.com/redis-developer/redis-cloud-rs`
- `redis-enterprise` from `github.com/redis-developer/redis-enterprise-rs`

These are published separately and versioned independently.

## MCP Skills

Skills are workflow templates in `crates/redisctl-mcp/skills/` following the [agentskills.io spec](https://agentskills.io/specification):

```
skills/
  index-advisor/SKILL.md      # Recommend search index schemas
  query-tuning/SKILL.md       # Optimize queries with FT.EXPLAIN/FT.PROFILE
  index-ab-test/SKILL.md      # Compare multiple index configurations
```

Each SKILL.md has YAML frontmatter (`name`, `description`) and a markdown body with step-by-step instructions that reference MCP tools.

## License

MIT OR Apache-2.0
