# Testing redisctl

This document describes how redisctl is tested: the local test tiers, what each one needs,
which suites are gated behind Docker, and what CI enforces on a pull request.

## Quick reference

The three commands that mirror the fast CI gate:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Everything those commands run is hermetic: no Docker, no credentials, no network. The live
suites described below are `#[ignore]`d and are skipped unless you ask for them with
`-- --ignored`.

## Prerequisites

- Rust stable. The workspace MSRV is 1.90 (`rust-version` in the root `Cargo.toml`).
- Docker, only for the live suites in [Tier 4](#tier-4-live-docker-suites).

## Test tiers

### Tier 1: unit tests

In-crate `#[cfg(test)]` modules next to the code they cover, across all three crates
(`redisctl`, `redisctl-core`, `redisctl-mcp`). These cover config parsing, credential
resolution, error classification, output formatting, progress reporting, and MCP tool
registration.

```bash
cargo test --package redisctl-core --lib --all-features
cargo test --package redisctl --bins --all-features
cargo test --package redisctl-mcp --lib --bins --all-features
```

The target flags differ per package on purpose. `redisctl` is bin-only and has no lib
target, so `--lib` errors there; `redisctl-core` has no bin. `--bins` matters for
`redisctl-mcp` because the MCP safety-tier registration tests live in `main.rs`.

### Tier 2: integration tests

Files under `crates/*/tests/`, compiled as separate test binaries:

| File | Covers |
| --- | --- |
| `crates/redisctl/tests/cli_basic_tests.rs` | argument parsing, help text, command surface |
| `crates/redisctl/tests/cli_integration_mocked_tests.rs` | full command runs against mock HTTP servers |
| `crates/redisctl/tests/cli_profile_tests.rs` | profile resolution, env overrides, config files |
| `crates/redisctl/tests/cloud_output_test.rs` | output-format rendering |
| `crates/redisctl-core/tests/config_edge_cases.rs` | configuration edge cases |
| `crates/redisctl-core/tests/workflow_tests.rs` | async workflow behavior |

```bash
cargo test --workspace --test '*' --all-features
```

### Tier 3: MCP request-shape tests

`crates/redisctl-mcp/tests/cloud_tools.rs` and `enterprise_tools.rs` drive the MCP tools
against [wiremock](https://docs.rs/wiremock) servers. They assert on the exact request a
tool produces (path, query string, JSON body) and on how it parses the response, so a
breaking change in an upstream API client surfaces as a failing assertion rather than a
runtime surprise. They need no Docker and run as part of Tier 2's command.

```bash
cargo test --package redisctl-mcp --test cloud_tools --all-features
cargo test --package redisctl-mcp --test enterprise_tools --all-features
```

### Tier 4: live Docker suites

These are marked `#[ignore]` and never run in pull-request CI. They need a Docker daemon and,
for the Enterprise suites, a running demo cluster.

#### Redis and Redis Stack

Container lifecycle is managed by `docker-wrapper`, which starts and removes containers
itself. A running Docker daemon is the only prerequisite.

```bash
cargo test -p redisctl-mcp --test redis_tools --all-features -- --ignored --nocapture
cargo test -p redisctl-mcp --test redis_stack_tools --all-features -- --ignored --nocapture
cargo test -p redisctl --test docker_wrapper_tests --all-features -- --ignored --nocapture
```

Set `REUSE_CONTAINERS=1` to keep a container alive between runs, which is much faster when
iterating:

```bash
REUSE_CONTAINERS=1 cargo test -p redisctl-mcp --test redis_tools --all-features -- --ignored --nocapture
```

#### Redis Enterprise

Start the demo cluster first and wait for the init container to finish provisioning:

```bash
docker compose -f docker/docker-compose.enterprise-demo.yml up -d
docker compose -f docker/docker-compose.enterprise-demo.yml logs -f redis-enterprise-init
```

The compose file provisions the cluster and sets the credentials the tests expect:

| Variable | Value |
| --- | --- |
| `REDIS_ENTERPRISE_URL` | `https://localhost:9443` |
| `REDIS_ENTERPRISE_USER` | `admin@redis.local` |
| `REDIS_ENTERPRISE_PASSWORD` | `Redis123!` |
| `REDIS_ENTERPRISE_INSECURE` | `true` |

Then run the suites:

```bash
# MCP tools against the live cluster
cargo test -p redisctl-mcp --features enterprise \
  --test enterprise_mcp_docker_integration_tests \
  -- --ignored --test-threads=1

# CLI commands against the live cluster
cargo test -p redisctl --test enterprise_docker_integration_tests \
  --all-features -- --ignored
```

`--test-threads=1` matters for the MCP suite: several tests mutate shared cluster state and
are serialized deliberately.

Tear down when you are done:

```bash
docker compose -f docker/docker-compose.enterprise-demo.yml down -v
```

Both Enterprise suites probe the cluster before doing anything and return early with a
`Skipping: Docker Redis Enterprise not available` message on stderr if it does not respond.
An `--ignored` run against a stopped cluster therefore reports passes with skip messages
rather than a wall of failures. Use `--nocapture` if you want to see those messages.

## Feature flags

`redisctl-mcp` gates code behind `cloud`, `enterprise`, `database`, `http`, and
`secure-storage`, all on by default. Tests are gated to match: `redis_tools.rs` and
`redis_stack_tools.rs` are `#![cfg(feature = "database")]`, and
`enterprise_mcp_docker_integration_tests.rs` is `#![cfg(feature = "enterprise")]`. A test
file whose feature is off compiles to an empty binary and silently passes, so use
`--all-features` when you want to be sure a suite actually ran.

## What CI enforces

`.github/workflows/ci.yml` runs on every pull request, including documentation-only and
stacked ones, so the `CI Status` check is always emitted.

| Job | Command | Required |
| --- | --- | --- |
| Quick Checks | `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings` | yes |
| Unit Tests (per package) | `cargo test --package <pkg> <targets> --all-features` | yes |
| Integration Tests | `cargo test --workspace --test '*' --all-features` | yes |
| Build (ubuntu) | `cargo build --release --bin redisctl` plus the full test run | yes |
| Build (macOS, Windows) | `cargo build --release --bin redisctl` | no, `continue-on-error` |
| Code Coverage | `cargo tarpaulin`, uploaded to Codecov | `main` only |

`CI Status` is the aggregate gate. It requires Quick Checks, all Unit Tests, and Integration
Tests to succeed; non-Linux build failures do not block it.

Three more workflows run alongside:

- `docs.yml` -- markdownlint, a strict MkDocs build, a link check, and `cargo doc --workspace
  --no-deps --all-features`. Only the MkDocs build is fatal; the lint and link checks are
  `continue-on-error`.
- `cargo-deny.yml` -- license and advisory checks, also weekly on a schedule.
- `security.yml` -- `cargo audit`, also nightly on a schedule.

No CI job runs the Tier 4 suites. Validate those locally before changing anything that talks
to a live cluster.

## Adding tests

- Put a unit test next to the code it covers; put a test that runs a whole command or tool in
  `crates/<crate>/tests/`.
- Prefer a wiremock-backed test over a live one. Reach for a live Docker test only when the
  behavior cannot be represented faithfully by a mock, such as server-side state transitions
  or genuine async task polling.
- Mark anything that needs Docker `#[ignore = "Requires Docker"]` (or a more specific reason)
  so it stays out of pull-request CI.
- When you fix a polling, workflow, or output-format bug, add the regression test in the same
  pull request.

See [CONTRIBUTING.md](CONTRIBUTING.md) for branch, commit, and pull-request conventions.
