# Experimental database library pilot

The opt-in backend proves composition with `redis-mcp` without replacing
redisctl's production database backend. Legacy remains the build/runtime default.
This is development work for #1172, not a production migration or release promise.

## Try it

Building from this branch currently requires access to the INTERNAL
`redis-developer/redis-database-mcp-rs` repository. The manifest pins revision
`8eff8240679d38157ee08b9d5e5eaeb656ce7550`; it does not require a crates.io
publication to develop or review the pilot. See the access gate below before
assuming a public checkout or fork can build it.

```bash
cargo build -p redisctl-mcp --features database-mcp-pilot
./target/debug/redisctl-mcp --database-backend redis-mcp --tools database,app \
  --database-url redis://127.0.0.1:6379

# Alternatively: explicitly select exactly one database profile.
./target/debug/redisctl-mcp --database-backend redis-mcp --tools database,app \
  --profile local-db
```

`--database-url` overrides `REDIS_URL`; explicit `--profile` selection overrides
`REDISCTL_PROFILE`. A URL and selected database profile conflict. Repeating the
same profile is harmless, but multiple distinct database profiles, an unknown
selected profile, or no explicit database target fail startup. Config defaults,
the first configured profile, and localhost are never fallback targets. Cloud
and Enterprise profiles may coexist and retain their existing routing.

URLs are validated offline by the library; connections are lazy and reused.
`--cluster` selects Cluster at startup and rejects nonzero database indexes.
TLS verification is preserved; unsupported URL options are rejected rather
than silently ignored. The embedding builder accepts explicit inputs only—it
does not read CLI environment variables as hidden target sources.

Only bare `database` selection is supported for the pilot. Legacy database
submodule selectors are rejected. The library owns the read-only Keyspace and
Strings catalog; no legacy database handlers are mixed into it. Host policy,
tool visibility, audit middleware, non-database tools, and transport remain
redisctl-owned. Global/per-toolset allow rules cannot add writes or raw commands.

## Known limitations and release gates

- **Cluster keyspace-wide calls are guarded.** The underlying transport's SCAN
  is node-local/default-primary; it cannot guarantee full-cluster enumeration.
  The pilot supplies the known deployment to the library, which rejects SCAN,
  DBSIZE, and RANDOMKEY on Cluster with `DEPLOYMENT_UNAVAILABLE`. These tools
  remain listed with stable errors; no single-node result claims full-cluster
  completeness. Cluster-wide keyspace semantics are a default-cutover gate.
- Reconnect/Cluster-safe `CLIENT SETNAME` is not implemented by this pilot. An
  explicit warning is emitted; production users needing naming stay on legacy.
  Upstream replay support in
  [redis-tower #736](https://github.com/joshrotenberg/redis-tower/issues/736)
  is complete; adoption through the library's
  [setup PR #128](https://github.com/redis-developer/redis-database-mcp-rs/pull/128)
  remains outside this immutable pilot pin and is a default-cutover gate.
- Deterministic library shutdown/lifecycle remains a cutover gate tracked in
  redis-database-mcp-rs #125. The pilot does not introduce session families.
- The immutable Git dependency is intentional for coordinated development.
  Before publishing a dependent redisctl release, switch to the real published
  library and rerun clean-room packaging. Publication is not required to review
  this pilot, but this branch is not a release-ready package manifest.
- Embedded skills/resources and prompts remain available. The
  `redisctl://skills` index reports the tools actually exposed by this backend
  after host policy and visibility filtering. Read it before using a workflow:
  many workflows reference tools absent from the pilot. Setup guidance must
  not enable writes or silently switch backends to satisfy those workflows.

Rollback is `--database-backend legacy` (or omit the selector); disabling the
Cargo feature removes the experimental runtime backend, not Cargo's need to
resolve its optional Git dependency. Selecting an uncompiled pilot fails
explicitly rather than silently falling back.

### Source-access gate

Public CI currently cannot fetch the internal repository. An approved read
mechanism, its owner, and an existing reference are needed before access wiring;
this branch does not provision credentials, reuse release tokens, change
repository visibility, or expose secrets to untrusted fork jobs.

Access must work before uncached Cargo resolution in all affected entry points:

| Workflow | Cargo-resolving entry points |
| --- | --- |
| `ci.yml` | Clippy/core checks, package unit tests, integration/live tests, platform builds/tests, coverage |
| `docs.yml` | Default embedding API rustdoc and all-feature workspace rustdoc |
| `cargo-deny.yml` | Both advisory and bans/licenses/sources dependency-graph checks |
| `release.yml` | `dist plan` metadata, local/global builds and any host step resolving workspace metadata |
| `release-plz.yml` | Workspace/version resolution and eventual package/publish validation |

`security.yml` runs `cargo audit` against the lockfile; that is distinct from
the Cargo-resolving graph checks above. A cache hit is not proof of clean fetch
access. CI-only credentials would unblock trusted jobs, but would **not** make
anonymous default-feature builds or public fork builds resolve this optional
Git dependency. Public source consumption needs a separate distribution/access
decision. Registry packaging remains a later release gate; no release or
default cutover is authorized by local pilot success.

## Catalog and result differences

The legacy baseline `mcp-catalog-v1.json` is untouched. The separate
`mcp-pilot-catalog-v1.json` freezes all 16 pilot tool names, input/output schemas,
and safety annotations. All pilot tools omit per-call `url`/`profile`, declare
read-only/non-destructive annotations, and return JSON `structuredContent`
plus its text representation instead of legacy human-readable text.

The table records every pilot tool. Input fields not mentioned retain their
names; the full snapshots capture validation, defaults, and required fields.
`key_encoding` selects UTF-8 (default) or base64 where supported.

| Tool | Additional input difference | Structured result fields |
| --- | --- | --- |
| `redis_dbsize` | None | `key_count` |
| `redis_digest` | New tool: `key`, `key_encoding` | `exists`, `digest`, `algorithm` |
| `redis_dump` | Adds `key_encoding`, bounded `max_bytes` | `key`, `key_encoding`, `exists`, `payload_base64`, `payload_bytes` |
| `redis_exists` | Bounded `keys` | `requested`, `existing`, `all_exist` |
| `redis_get` | Adds `key_encoding` | `key`, `key_encoding`, `exists`, `value`, `encoding` |
| `redis_getrange` | Adds `key_encoding` | `key`, `key_encoding`, `start`, `end`, `bytes`, `value`, `encoding` |
| `redis_memory_usage` | None | `key`, `exists`, `bytes` |
| `redis_mget` | Bounded `keys` | `values`, `count` |
| `redis_object_inspect` | New tool: `key`, `key_encoding`, `operation` | `key`, `key_encoding`, `operation`, `exists`, `encoding`, `value` |
| `redis_ping` | None | `response`, `latency_ms` |
| `redis_randomkey` | None | `key`, `encoding` |
| `redis_scan` | Removes whole-scan `limit`; adds page `cursor`, bounded `count` | `cursor`, `keys`, `count`, `page` |
| `redis_sort` | New read-only tool: `key`, `key_encoding`, `by`, `get`, `offset`, `count`, `order`, `alpha` | `key`, `key_encoding`, `source_exists`, `offset`, `requested_count`, `get_pattern_count`, `returned`, `values` |
| `redis_strlen` | None | `key`, `length_bytes` |
| `redis_ttl` | Adds `key_encoding` | `key`, `key_encoding`, `ttl_seconds`, `exists`, `persistent` |
| `redis_type` | Adds `key_encoding` | `key`, `key_encoding`, `key_type` |

GET preserves binary values with base64 and missing values with `exists: false`
and null fields. SCAN returns one bounded page rather than aggregating an
entire scan. Output budgets may reject a response; they do not return an
apparently complete truncated result. Tools outside this table are absent,
including all writes, destructive operations, and raw commands.

Unknown input fields are rejected: per-call targets cannot silently rebind the
process. Callers must use the declared JSON types; legacy numeric-string
coercion is not a compatibility promise for library inputs. The public builder
can set pilot deadlines and encoded-output/entry budgets; their defaults remain
library-owned (30 seconds, 256 KiB, and 1,000 collection entries).

## Validation and ownership

`tests/database_pilot.rs` covers catalog isolation, default rollback, host
policy/visibility, disabled toolsets, missing target/invalid limits, bounded
first connection failures, and credential-safe errors. Resolver unit tests
cover explicit profiles, deduplication, unknown/ambiguous/conflicting sources,
management-profile coexistence, offline URL validation, and Cluster DB rules.

Live tests own disposable standalone and three-primary Cluster processes:
legacy GET comparison, binary/missing values, TYPE/TTL/SCAN, authentication,
client reuse, output budgets, deadlines/cancellation recovery, Cluster slot
routing and the keyspace deployment guard, untrusted/trusted TLS, and clean stdio JSON. The
TLS child gets only its temporary CA; no user trust store is changed.

```bash
# Offline contracts
cargo test -p redisctl-mcp --all-features --lib --test database_pilot

# Live fixture tests: requires redis-server, redis-cli, and openssl, not Docker
cargo test -p redisctl-mcp --all-features --test database_pilot \
  -- --ignored --test-threads=1
```

CI runs both contracts and live fixtures. Snapshot updates require explicit
review using `UPDATE_MCP_PILOT_CATALOG=1`; never rewrite the legacy snapshot to
make an experimental schema change appear backward compatible.
