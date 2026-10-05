# Database MCP Migration Contract

!!! warning "Design for an opt-in pilot, not current runtime behavior"
    This document proposes the host contract for
    [#1171](https://github.com/redis/redisctl/issues/1171) and the validation plan for
    [#1172](https://github.com/redis/redisctl/issues/1172), under migration epic
    [#1170](https://github.com/redis/redisctl/issues/1170). The released 0.12.1
    server still uses the legacy database backend. This document adds no new
    runtime flags, dependency, or tool schema and does not authorize a cutover.

## Decision: one database target per process

The library-backed database router binds to one fixed executor for the lifetime
of the MCP process. redisctl chooses that target at startup. Database calls
cannot switch it through tool arguments, profile edits, or shared mutable state.
Separate targets require separate MCP server instances.

Cloud and Enterprise remain redisctl-owned toolsets. Their existing per-call
profile selection is unchanged. A combined server can expose those toolsets
alongside one database target; a Cloud API profile is not a database target.

## Ownership boundary

| Responsibility | Owner |
| --- | --- |
| Database tool names/schemas, command semantics, structured results | `redis-mcp` |
| Access/effect classification, capability checks, output budgets and command timeouts | `redis-mcp` |
| Redis protocol, reconnection and Cluster connection setup | `redis-tower`, through `redis-mcp` |
| Profile and credential resolution, fixed startup target and connection policy | redisctl |
| Cloud/Enterprise/app toolsets, visibility, policy and combined transport | redisctl |
| Audit integration and process lifecycle | redisctl, using public library lifecycle APIs |

redisctl must not copy library schemas or response normalization into parallel
handlers. Its composition layer binds credentials and maps host policy/audit
behavior onto the library router. It must not recreate the legacy backend as a
production `redis-rs` executor behind the new schemas.

## Startup resolution

These rules apply only when the opt-in backend loads database tools. A server
without library-backed database tools does not require a database target.

1. Resolve CLI/environment values normally: explicit `--database-url` overrides
   `REDIS_URL`; explicit `--profile` values override `REDISCTL_PROFILE`. Rust
   embedders provide the equivalent values through the builder, rather than
   implicitly reading CLI environment variables.
2. Validate explicitly selected profile names against the startup configuration.
   An unknown profile or unreadable/invalid configuration needed for profile
   resolution is an error, not an empty configuration fallback. Deduplicate
   repeated names. Count only selected **database** profiles as target candidates;
   selected Cloud/Enterprise profiles do not enter this count.
3. Apply the following matrix. A present but empty/malformed URL is an error,
   not a reason to fall back to a profile.

| Direct URL | Selected database profiles | Outcome |
| --- | --- | --- |
| Present | None | Use the direct URL |
| Absent | Exactly one | Resolve that database profile |
| Present | One or more | Reject conflicting target sources |
| Absent | More than one | Reject ambiguous database profiles |
| Absent | None | Reject missing target |

There is no implicit target from `default_database`, the only configured
database profile, the first profile in a list, or localhost. Explicit selection
keeps the pilot deterministic even when local configuration changes. Multiple
configured but **unselected** database profiles are not ambiguous. An inherited
`REDIS_URL` conflicting with a selected database profile must produce a safe
instruction to remove the conflicting source, never echo the URL.

Validate target syntax and resolve required credentials at startup, but connect
lazily on the first database operation. Initialization and catalog discovery
must not require network access. A first-connect failure remains a bounded,
redacted operation error, not an implicit switch to another target.

### Connection policy

- `--cluster` chooses the executor mode at startup; it is not a per-call field.
  The first pilot can use a single configured seed. Multiple-seed configuration
  is separate scope, not a new implicit CLI contract.
- Preserve authentication, TLS verification and the selected logical database.
  Reject a nonzero logical database in Cluster mode instead of ignoring it.
- Never downgrade TLS, retry without credentials, or enable insecure mode to
  recover from a connection error. Plaintext development connections remain an
  explicit configuration choice.
- Client naming must eventually apply after authentication/protocol/database
  setup and before traffic on every physical connection, including reconnects
  and new/replacement Cluster nodes. A one-time `CLIENT SETNAME` does not satisfy
  this contract. Until the upstream replay gate passes, the pilot must document
  its limitation and must not claim production naming parity.
- Profile changes do not retarget a running router. Restart the MCP process to
  resolve new configuration or credentials. In-process credential refresh is
  separate work and must preserve fixed target identity.

### Credential boundary

The migrated tool schemas contain neither `url` nor `profile` target fields.
Credentials and connection URLs stay in host configuration and executor state;
they must not enter model-facing schemas, tool results, errors, tracing, audit
records, or instruction/resource text. A safe target summary may identify its
source, profile name and standalone/Cluster mode, but must not print the URL.

Sanitize parse, credential-resolution and connection errors as well as success
paths. In particular, do not forward raw TOML source snippets, expanded
environment values, or upstream errors containing authentication material.
Tests use unique fake secrets to prove this boundary on failure paths.

## Multiple-target deployment

Use clearly named configurations in the MCP client, each launching its own
process. For example, these existing launch options illustrate the proposed
selection pattern without introducing a future backend-selector flag:

```json
{
  "mcpServers": {
    "redis-dev": {
      "command": "/absolute/path/to/redisctl-mcp",
      "args": ["--profile", "dev-db", "--tools", "database,app"]
    },
    "redis-staging": {
      "command": "/absolute/path/to/redisctl-mcp",
      "args": ["--profile", "staging-db", "--tools", "database,app"]
    }
  }
}
```

Profiles are configured through trusted local credential flows, not by putting
secrets in the client launch JSON. In HTTP mode, all sessions sharing a router
share its fixed database target. HTTP transport does not supply built-in client
authentication; keep loopback binding or use a trusted authenticated TLS gateway.

## Pilot scope and compatibility

The pilot is an additive compile-time feature plus an explicit runtime backend
selection, with final option names settled in its implementation PR. Neither
building the feature nor changing a dependency makes it the default.

Only read-only Keyspace/Strings operations are composed initially. No duplicate
tool registrations are permitted: the opted-in pilot replaces those database
families for that server, rather than merging matching legacy names. Unmigrated
database families are absent from the pilot catalog, not silently served by a
second database backend. Cloud, Enterprise, app tools, transport and skill
behavior remain unchanged. The legacy catalog stays the default and rollback
requires only deselecting the pilot backend.

Removing accepted `url`/`profile` fields is a breaking catalog change, even
though the fields are optional. New structured outputs and changed return
behavior also require explicit compatibility review. Keep a separate reviewed
pilot catalog; do not replace the legacy fixture to hide differences. At the
declared 1.0 boundary, users of per-call target selection migrate to one startup
target per process. A default cutover before that boundary requires a separate,
explicit compatibility decision, not an incidental dependency update.

## Pilot acceptance matrix

Each row requires recorded test evidence in the implementation PR. This table
is a plan, not a claim that migration tests already exist or pass.

| Area | Required evidence |
| --- | --- |
| Default path | Default builds/runtime retain the legacy catalog; feature alone does not opt in; selector without compiled support fails clearly |
| Target resolver | CLI/environment precedence, explicit URL, named profile, missing/unknown/wrong-type profile, duplicate name, conflicting sources, multiple selected databases, and no implicit default |
| Offline startup | Initialization and discovery succeed for a valid target with Redis offline; no connection until first database operation |
| Standalone parity | Compare `redis_scan`, `redis_type`, `redis_get`, missing keys, TTL, Unicode and supported byte handling against one shared live fixture |
| TLS/authentication | Verified TLS and correct credentials succeed; wrong credentials, untrusted CA, malformed URL and refused connection fail safely without fallback |
| Cluster | Representative reads across slots and MOVED/topology behavior; Cluster database-index rejection; document library scan/fan-out differences |
| Policy/visibility | Tier, global/per-toolset allow/deny, disabled database toolset and presets are enforced on discovery and direct invocation; explicit allow cannot expose writes outside the read-only pilot |
| Isolation | Concurrent requests/sessions remain on the fixed target; changing a profile cannot move an already-built router |
| Catalog/results | Reviewed name/schema/annotation/output diff; no `url`/`profile` fields; no duplicate registration; normalize only test comparisons, not production responses |
| Audit/secrets | Representative success, denial, timeout and parser/auth/TLS failures preserve host auditing without fake secret/URL leakage |
| Budgets/timeouts | Connect and command failure paths terminate within documented limits; large replies respect library output budgets |
| Lifecycle/rollback | Exercise process stop and disposal; record unresolved upstream lifecycle limitations; deselection restores the legacy backend |
| Build/release | fmt, all-target/all-feature Clippy, workspace/integration tests, minimal-feature builds, strict rustdoc and clean-consumer checks against the pinned Git revision; registry packaging before release |

## Development checkpoint and dependency gates

The target contract remains proposed until human review of
[#1175](https://github.com/redis/redisctl/pull/1175). The isolated opt-in
[pilot #1176](https://github.com/redis/redisctl/pull/1176) implements it for
validation, not as an accepted default compatibility change.

The library's
[published redis-tower dependency update #124](https://github.com/redis-developer/redis-database-mcp-rs/pull/124)
is merged. The pilot pins reviewed library revision
`8eff8240679d38157ee08b9d5e5eaeb656ce7550`
([#126](https://github.com/redis-developer/redis-database-mcp-rs/pull/126)) for
reproducible committed builds. Local path overrides are allowed for coordinated
development, but must not appear as machine-specific dependencies in committed
manifests or lockfiles.

Skills from #1174 are now embedded in main. Their resources and prompts remain
host-owned; the pilot's `redisctl://skills` index must describe the actual
composed, policy-visible catalog. Missing library families must not be advertised
as available merely because a bundled workflow refers to them.

### Source-access prerequisite

The pinned library repository is INTERNAL. Public CI cannot currently fetch it.
An approved read-access mechanism, owner and reference are required; this work
does not provision credentials, change visibility, reuse release tokens or
expose secrets to untrusted forks. Verify uncached resolution before every
affected Cargo entry point, not only compile jobs:

- `ci.yml`: Clippy/core checks, unit/integration/live tests, platform builds and
  coverage.
- `docs.yml`: default embedding API and all-feature workspace rustdoc.
- `cargo-deny.yml`: both dependency-graph check groups.
- `release.yml`: dist plan metadata, local/global builds and any host metadata
  resolution; `release-plz.yml`: version/package/publish resolution.

The lockfile-based `cargo audit` in `security.yml` is distinct from these graph
checks. A trusted-CI read mechanism would not solve anonymous default-feature or
public-fork builds: Cargo still resolves optional Git dependencies when their
feature is disabled. Public source consumption requires a separate agreed
distribution/access path. A warm cache is not clean-consumer evidence.

Publication is not a pilot implementation gate. Before shipping a release that
includes this dependency, discuss publication, switch to the agreed published
crate version, and verify registry packaging and clean-consumer resolution. The
library server has its own library-first release ordering; its binary is not a
redisctl dependency.

Before making the new backend default:

- Complete the pilot, each family's parity/catalog gates, and compatibility
  migration treatment; delete duplicate handlers only in reviewed slices.
- Adopt and verify the library's
  [connection setup #128](https://github.com/redis-developer/redis-database-mcp-rs/pull/128).
  Upstream replay support in
  [redis-tower #736](https://github.com/joshrotenberg/redis-tower/issues/736)
  is complete, but not yet consumed by the immutable pilot pin.
- Verify host-usable, deterministic executor shutdown, tracked in
  [redis-database-mcp-rs #125](https://github.com/redis-developer/redis-database-mcp-rs/issues/125).
- Record live standalone, TLS/authentication, Cluster, rollback and release
  evidence. Do not equate a passing compile or initial library release with
  production readiness.

The design document alone leaves #1171 open: implementation contract tests and
the library-side consumer tracker
[#76](https://github.com/redis-developer/redis-database-mcp-rs/issues/76) still need
the accepted host contract and evidence. The pilot and parent epic also remain
open until their respective acceptance criteria are met.
