# Compatibility and Support Policy

- **Policy version:** 1.0
- **Applies to:** redisctl 1.x releases
- **Status:** The contract takes effect with redisctl 1.0.0. Until then, 0.x releases may make
  breaking changes called out in their release notes.

redisctl 1.0 is a compatibility commitment for deliberately supported interfaces. It is not a
claim that every Redis Cloud or Redis Enterprise endpoint, every Redis command, or every possible
MCP workflow is implemented.

## Support levels

Every externally visible surface is classified as one of the following:

| Level | Meaning |
| --- | --- |
| **Stable** | Covered by the 1.x compatibility rules in this document. Breaking changes require a major release, except for the limited safety exceptions below. |
| **Preview** | Available for evaluation. It may change in a minor release, with the change documented in release notes and migration guidance when practical. |
| **Internal** | An implementation detail. It may change without notice and must not be treated as a supported integration point. |

An interface is stable only when it is documented as stable. Being visible in source code, emitted
by an upstream API, or reachable through a hidden command does not make it stable.

## Supported surfaces

| Surface | 1.0 level | Compatibility commitment |
| --- | --- | --- |
| Documented, non-hidden `redisctl` commands and flags | Stable | Command paths, option names, accepted documented inputs, defaults that affect automation, and documented behavior follow the CLI rules below. |
| Configuration and profiles | Stable | The versioned 1.0 schema, credential references, and documented precedence rules remain readable throughout 1.x. |
| JSON/YAML success output owned by redisctl | Stable | Documented redisctl-owned fields and value meanings follow the machine-output rules below. |
| Structured error envelope and process exit codes | Stable | Existing string codes, numeric exit codes, stream placement, and documented meanings are not reused or changed incompatibly in 1.x. |
| Human-readable table and diagnostic wording | Stable | Output remains usable by people, but spacing, colors, headings, tips, and prose are not scripting contracts. |
| `redisctl-mcp` stdio transport | Stable | Startup flags, safe defaults, and the supported MCP catalog follow the MCP rules below. |
| MCP HTTP transport | Preview | HTTP remains unauthenticated and intended for loopback or a separately secured proxy. Its deployment contract may change before promotion to stable. |
| MCP tool catalog enabled by a documented feature set | Stable | Tool names, required inputs, output contracts owned by redisctl, annotations, and safety tiers follow the MCP rules below. |
| Bundled Agent Skills | Preview | Skill names and workflow text may evolve as clients and tool coverage mature. Skill metadata and tool references must still validate. |
| Raw Cloud or Enterprise API access | Preview | redisctl preserves the documented raw request mechanism, but endpoint availability and payloads are controlled by the upstream service. |
| Documented public APIs of `redisctl-core` and `redisctl-mcp` | Stable | The intentionally supported Rust API and feature combinations follow Rust SemVer after the 1.0 baseline is recorded. |
| Hidden commands, undocumented flags, test-support features, and repository layout | Internal | These may change without a compatibility period. |

Support for a product area is described separately from interface stability. The
[support matrix](https://github.com/redis/redisctl/issues/1085) records which Redis products, API
operations, versions, and release targets have validation evidence.

## CLI compatibility

For documented, non-hidden commands, the following are breaking changes:

- removing or renaming a command, option, or documented alias;
- making an optional argument required;
- rejecting a previously documented input;
- changing a default in a way that can alter automation or side effects;
- changing a read operation into a write operation, or weakening a confirmation requirement;
- changing documented stdout/stderr placement or success/failure exit behavior.

The following are normally additive changes:

- adding a command or option;
- accepting a new input value;
- adding an opt-in behavior whose default preserves existing behavior;
- improving human-readable text or table layout.

Shell completions and `--help` reflect the current command catalog. Their exact whitespace and
ordering are not stable.

The hidden `--yes` and `-y` compatibility aliases that existed during 0.x are removed at the 1.0
boundary. Destructive commands use the documented, long-only `--force` flag. After 1.0, a
documented compatibility alias is covered like any other stable CLI option; a hidden development
command is internal until it is deliberately published. The removal is tracked by
[issue #1095](https://github.com/redis/redisctl/issues/1095).

## Configuration compatibility

Profiles are persistent user data, not an implementation detail. The 1.0 configuration work must:

- assign an explicit schema version;
- continue to read the documented final 0.x profile format or return a precise migration error;
- migrate without silently losing unknown fields or credentials;
- perform any rewrite atomically and provide a recovery path;
- reject configurations from an unsupported newer schema with an actionable error rather than
  guessing.

Adding an optional field with a backward-compatible default is additive. Removing or renaming a
field, changing its meaning, changing precedence, or requiring an existing installation to rewrite
manually is breaking unless handled by a compatible migration.

The schema and migration mechanism are tracked by
[issue #1084](https://github.com/redis/redisctl/issues/1084).

## Machine-readable output and exit codes

Automation should use JSON or YAML output and documented process exit codes. Table output and
human diagnostic prose are not stable parsing formats.

redisctl guarantees the following for stable commands:

- successful machine output is written to stdout;
- structured failures are written to stderr;
- the error envelope retains `error.code`, `error.exit_code`, `error.message`, and `error.tips`;
- existing string error codes and numeric exit codes keep their documented meaning;
- a newly classified failure receives a new code rather than changing the meaning of an old one;
- YAML represents the same data model as JSON.

Adding an optional field is additive. Removing a field, changing its type or meaning, or adding a
new required wrapper is breaking.

Some commands intentionally expose Cloud or Enterprise response objects. Fields owned by the
upstream API follow that API's lifecycle and can change independently of redisctl. redisctl-owned
envelopes, normalized fields, and documented projections remain covered. Raw API output is always
upstream-controlled. Scripts that require a narrow contract should use a documented JMESPath
projection and account for upstream field availability.

## MCP compatibility and safety

The stable MCP contract covers the stdio server, default read-only policy, documented feature-gated
toolsets, tool names, input schemas, redisctl-owned output contracts, and safety annotations.

Within 1.x:

- removing or renaming a stable tool is breaking;
- adding a required input or rejecting a previously valid documented input is breaking;
- adding a tool or optional input is additive;
- moving a tool to a less restrictive safety tier is prohibited;
- making a tool more restrictive is a compatibility change and requires explicit security
  rationale, release-note visibility, and migration guidance;
- disabling or bypassing policy evaluation is a security defect, not a compatibility option.

Feature-gated tools are covered when their documented feature is enabled. A tool from a feature
that was not built cannot be assumed to exist. Upstream-controlled payload fields have the same
exception described for machine-readable CLI output.

The [MCP Compatibility and Catalog](../mcp/compatibility.md) page documents the canonical catalog,
CI enforcement, intentional update workflow, and tool deprecation convention.

## Rust crate compatibility

The workspace remains lockstep-versioned for the 1.x series. `redisctl`, `redisctl-core`, and
`redisctl-mcp` share one version so release notes, binary behavior, profile compatibility, and MCP
behavior refer to the same product baseline.

The documented public Rust APIs of `redisctl-core` and `redisctl-mcp` are supported library
surfaces. Before 1.0, accidental exports must be removed or explicitly classified. Beginning with
the recorded 1.0 baseline:

- Rust SemVer applies to supported public items and documented feature combinations;
- adding public items or optional features is normally additive;
- removing an item, changing a public signature, or removing a documented feature requires a major
  release;
- `test-support`, private modules, binaries' internal modules, and items explicitly documented as
  unstable are not supported library API.

The public API inventory and SemVer gate are tracked by
[issue #1088](https://github.com/redis/redisctl/issues/1088).

## Deprecation and removal

A stable interface scheduled for removal must:

1. be marked deprecated in the relevant help or reference documentation;
2. name its replacement and provide migration guidance;
3. remain available for at least one minor release and 90 days, whichever is longer; and
4. be removed only in a major release.

Preview interfaces do not receive the major-release guarantee, but incompatible changes must be
called out in release notes. Internal interfaces receive no deprecation period.

## Release and support policy

- Stable releases use Semantic Versioning. Release candidates such as `1.0.0-rc.1` are for
  validation and are not production support baselines.
- The latest 1.x minor line receives normal bug and security fixes.
- A previous minor line is not supported after the next minor is published; users must upgrade to
  receive fixes. Maintainers may issue an exceptional critical-security backport, but this is not a
  support commitment.
- Patch releases do not intentionally break stable interfaces.
- `main` is the development branch; no long-lived release branch is promised unless one is
  announced for a specific maintenance need.
- Security reports and response expectations follow [SECURITY.md](https://github.com/redis/redisctl/blob/main/SECURITY.md).

Release ownership, artifact validation, rollback, and critical-fix procedures are covered by the
[release-candidate checklist](https://github.com/redis/redisctl/issues/1087).

## Exceptional changes

A critical vulnerability, unsafe default, legal requirement, or upstream service shutdown may make
strict compatibility more harmful than a narrow breaking fix. In that case maintainers will:

- choose the smallest safe change;
- document the exception and affected versions prominently;
- provide migration or mitigation guidance when possible; and
- avoid reusing an existing command, field, tool, or error code with a different meaning.

## Reviewing future changes

Every change to a user-visible interface should answer:

1. Which support level and surface does this affect?
2. Is the change additive, breaking, or a safety correction under this policy?
3. What automated contract test or recorded exception detects the change?
4. Are release notes, migration guidance, and the support matrix affected?

The policy itself may be clarified additively during 1.x. Weakening a stable 1.x compatibility
commitment requires the same visibility and release treatment as changing the affected interface.
