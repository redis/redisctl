# MCP Compatibility and Catalog

The stable redisctl 1.x MCP contract is the tool surface exposed by the stdio server for a
documented feature set. The all-features catalog is recorded in
[`mcp-catalog-v1.json`](https://github.com/redis/redisctl/blob/main/crates/redisctl-mcp/tests/fixtures/mcp-catalog-v1.json)
and checked in CI.

The current all-features baseline contains 375 tools:

| Toolset | Tools |
| --- | ---: |
| Cloud | 148 |
| Enterprise | 85 |
| Database | 132 |
| App/profile | 8 |
| System | 2 |

Runtime discovery remains authoritative for a particular installation. Feature flags, `--tools`,
visibility presets, and policy tiers can expose a supported subset of this catalog. Call
`list_available_tools` to inspect that resolved subset.

## Rust embedding API

The supported Rust embedding entry point is `McpServerBuilder`. It constructs the same router used
by the `redisctl-mcp` binary, including:

- the tool-to-toolset mapping used by per-toolset policy rules;
- safety-tier and allow/deny filtering;
- visibility presets and the two system tools;
- built-in and dynamically loaded prompts; and
- server instructions and shared application state.

This is an intentional security boundary: creating state with a policy is not sufficient unless
the router also installs that policy as a capability filter. The binary and library now share one
construction path so those behaviors cannot drift. A toolset disabled by policy remains disabled
even when a caller or CLI invocation selects it explicitly.

Generated input structs and individual tool-constructor functions are implementation details, not
supported Rust APIs. They are hidden in normal builds. The `test-support` feature exposes them only
for redisctl's own integration tests and carries no 1.x compatibility promise. The MCP names,
schemas, annotations, and tiers produced by those internals remain protected by the catalog
contract described on this page.

## What the catalog records

Each entry records:

- the tool name, toolset, and submodule;
- its normalized input JSON Schema;
- its structured `outputSchema`, or `null` when the tool does not declare one;
- all MCP safety annotations with protocol defaults made explicit; and
- the minimum redisctl safety tier derived from those annotations.

Schema titles, descriptions, examples, and the repeated `$schema` declaration are excluded from
the fixture. They are explanatory text rather than callable input constraints. Defaults, types,
formats, accepted variants, required fields, and other validation constraints remain in the
contract.

Most current tools return MCP text content and do not declare a structured `outputSchema`.
`outputSchema: null` records that fact; it does not turn prose or upstream-controlled JSON into a
new structured guarantee. When redisctl owns and publishes a structured output schema, that schema
becomes part of the catalog contract.

## Compatibility rules

The contract test classifies changes before maintainers update the fixture:

| Change | Classification |
| --- | --- |
| Add a tool or optional input | Additive; requires catalog and documentation review |
| Remove or rename a tool or accepted input | Breaking |
| Add a required input or a stricter input constraint | Breaking |
| Add an accepted schema variant or relax a constraint | Additive; requires review |
| Remove or change an existing structured output schema | Breaking |
| Move a tool to a less restrictive safety tier | Safety regression; prohibited |
| Make a tool more restrictive | Compatibility change requiring security rationale and migration guidance |
| Change idempotence or open-world annotations | Reviewed as a behavioral contract change |

Both additive and breaking differences fail the snapshot test. This prevents a new tool or field
from entering the stable catalog merely because it compiles. The failure explains the detected
classification so the pull request can apply the appropriate SemVer, documentation, and migration
treatment.

## Reviewing an intentional catalog change

1. Run the contract test without updating the fixture and inspect its classification:

   ```bash
   cargo test -p redisctl-mcp --all-features mcp_catalog_matches_1_0_baseline
   ```

2. Apply the compatibility policy. For an incompatible 1.x change, retain the old tool or input and
   use the deprecation process unless a documented safety exception applies.
3. For an accepted additive change, or while deliberately preparing the pre-1.0 baseline, refresh
   the fixture:

   ```bash
   UPDATE_MCP_CATALOG=1 cargo test -p redisctl-mcp --all-features \
     mcp_catalog_matches_1_0_baseline
   ```

4. Review the fixture diff. Update the tools reference, release notes, policy examples, bundled
   skills, and migration guidance as applicable.
5. Run the test again without `UPDATE_MCP_CATALOG` before committing.

The fixture is generated through an in-process MCP client and `tools/list`, so it tests the router
surface a client actually discovers rather than only checking handwritten name constants.

## Tool deprecation

A stable tool or input scheduled for replacement must:

1. remain available under its existing name and behavior;
2. identify the replacement in its tool description and reference documentation;
3. be called out in release notes with migration guidance;
4. remain available for at least one minor release and 90 days, whichever is longer; and
5. be removed only in a major release.

An optional replacement input may be added in a minor release. Making that input required still
waits for a major release. Preview and internal tools follow the broader
[Compatibility and Support Policy](../reference/compatibility.md).

## Transports and deployment modes

- **Stdio is stable for 1.0.** Startup behavior, safe defaults, and catalog discovery follow the
  stable contract.
- **HTTP is preview.** It has no built-in client authentication. Keep it on loopback or place it
  behind a trusted gateway providing authentication, authorization, and TLS.
- **Feature-gated builds are supported subsets.** A build without `cloud`, `enterprise`, or
  `database` does not advertise that toolset.
- **Raw API tools are preview passthroughs.** Their presence and redisctl-owned input contract are
  recorded, but upstream endpoint behavior and response fields follow the upstream service.

## Bundled Agent Skills

Bundled Agent Skills remain preview, but CI validates that every `SKILL.md`:

- has parseable `name` and `description` frontmatter;
- uses a unique name matching its directory;
- contains a non-empty workflow; and
- references tools present in the compiled all-features catalog.

This prevents a skill from shipping with a stale tool call while leaving broader skill catalog and
workflow development independent of the 1.0 MCP contract.
