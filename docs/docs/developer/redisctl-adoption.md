# Redisctl Public Repository Adoption Record

**Assessment:** Partial against [candidate standard v0.1](public-repository-standard-v0.1.md),
October 6, 2026. This is a baseline, not a compliance declaration or a new GA gate.

## Identity and evidence boundary

- Repository: [redis/redisctl](https://github.com/redis/redisctl).
- Audited main revision: [`1453352a88017612fda48f2e54b286100a64bd88`](https://github.com/redis/redisctl/tree/1453352a88017612fda48f2e54b286100a64bd88).
- Workspace version: **0.12.1**, Rust edition **2024**, declared MSRV **1.90**.
- Applicable profiles: **Rust library** (`redisctl-core`, `redisctl-mcp`),
  **CLI** (`redisctl`), **MCP server/embedding library** (`redisctl-mcp`).
- Python binding and management API client profiles are **not applicable to this
  repository's own packages**. The separately versioned Cloud/Enterprise SDKs
  ship those interfaces; their adoption records belong in their repositories.
- Evidence: source inspection at the immutable main revision above and read-only
  GitHub REST observations on October 6, 2026.
  No settings changes, consumer installs or release-artifact tests were performed
  for this documentation assessment.

Source links below point to that main revision. Existing tests/workflows are
evidence of implemented checks, not proof that every release journey has run.
Open PRs are not part of this baseline, even if their own checks pass.

## Ownership, support and distribution

[Workspace metadata](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/Cargo.toml)
names Josh Rotenberg as author. That is identity evidence, not assignment of all
CI, vulnerability, release or shared-standard responsibilities. Those explicit
human assignments and the permanent standard home remain pending.

Public bug reports use the [issue tracker](https://github.com/redis/redisctl/issues);
user/contributor guidance is in the [README](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/README.md),
[CONTRIBUTING.md](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/CONTRIBUTING.md)
and [documentation site](https://redis-field-engineering.github.io/redisctl-docs/).
Canonical support/contact and tested product/API coverage remain part of
[#1085](https://github.com/redis/redisctl/issues/1085). No response-time or
service-support commitment is established by this record.

| Surface/channel | Advertised or configured baseline | Evidence still needed |
| --- | --- | --- |
| Rust crates | Three workspace packages, lockstep versioning; registry dependencies on redis-cloud 0.11 and redis-enterprise 0.10. | Actual package contents and clean external consumers for the supported features at the release revision. |
| Cargo/Homebrew | README advertises Cargo install and `redis/homebrew-tap`. | Identified package/formula versions, installation and upgrade smoke tests. |
| GitHub archives/installers | Cargo dist config targets Apple ARM64/x86_64, Linux x86_64 GNU/musl and Windows x86_64 MSVC; shell/PowerShell installers. | Actual asset availability/startup evidence. Installation docs also advertise Linux ARM64 GNU, absent from this target list, and show an old 0.11.1 version example. |
| Container | Docker workflow configures GHCR image `redis/redisctl` for linux/amd64 and linux/arm64. | Published digest/multiarch availability and startup/journey results; container ARM64 does not prove an ARM64 archive exists. |
| MCP | Default legacy database backend, read-only policy, stdio and preview HTTP; embedding and skill/catalog contracts exist. | Complete transport/lifecycle and packaged-client journeys; do not infer network authentication from HTTP availability. |

References: [installation](../getting-started/installation.md),
[Docker workflow](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/workflows/docker.yml),
[dist configuration](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/Cargo.toml),
[compatibility policy](../reference/compatibility.md) and
[MCP catalog/transport policy](../mcp/compatibility.md).
The 1.x compatibility commitment begins at 1.0, not at this 0.12.1 baseline;
0.x changes may break interfaces with release-note visibility. The 1.0 support
matrix and artifact/upgrade acceptance remain in #1085 and
[#1087](https://github.com/redis/redisctl/issues/1087).

Release source uses release-plz for release PRs/crates publication, cargo-dist for
tagged archives/installers and separate Docker/Homebrew workflows. Contributor
guidance still describes an obsolete manual release path. Workflow-declared OIDC
is not verification of external registry configuration or successful publication.

## Observed gate behavior

“Required” has two meanings here: intended validation in source/project guidance,
and a GitHub ruleset-required check. They are not interchangeable.

| Exact check name | Source intent and availability | Enforcement/gap at the audited revision |
| --- | --- | --- |
| `Quick Checks` | All PRs; formatting, strict all-target/all-feature Clippy and core default-feature check. | Evaluated by `CI Status`. |
| `Unit Tests - redisctl-core`, `Unit Tests - redisctl`, `Unit Tests - redisctl-mcp` | All PRs after Quick Checks; package-specific targets, all features. | Matrix result evaluated by `CI Status`. |
| `Integration Tests` | All PRs after Quick Checks; workspace integration and three explicitly selected ignored Docker command-safety tests. | Evaluated by `CI Status`; broader ignored Enterprise/live suites are not implied. |
| `Build - ubuntu-latest` | Intended required; release-mode CLI build and all-feature workspace tests. | `CI Status` depends on the build matrix but does not evaluate its result. A green aggregate alone does not prove this required job succeeded. |
| `Build - macos-latest`, `Build - windows-latest` | Advisory via `continue-on-error`. | Preserve advisory status; a truthful Linux gate must not accidentally make these mandatory. |
| `CI Status` | All PRs, including docs-only/stacked PRs; `always()` aggregate. | Observed sole ruleset-required status. Explicitly enforces Quick Checks, unit and integration, not platform-build outcomes. |
| `Build Documentation` | Strict MkDocs; docs workflow has Markdown/docs/workflow path filters. | Evaluated by `Documentation Status`. |
| `Validate Rust Docs` | Strict default MCP and all-feature workspace rustdoc. | Not evaluated for failure by `Documentation Status`; Rust/Cargo-only PRs can miss this workflow entirely. |
| `Markdown Lint`, `Check Links` | Advisory (`continue-on-error`) when docs workflow runs. | A passing aggregate does not certify absence of lint/link findings. |
| `Documentation Status` | `always()` after docs jobs when workflow is triggered. | Only MkDocs success is enforced; not observed as ruleset-required. |
| `License and Security Check (advisories)`, `License and Security Check (bans licenses sources)` | Cargo/deny/workflow-path PRs and weekly schedule. | Separate cargo-deny results; not observed as ruleset-required. |
| `Security Audit` | Cargo/security-workflow-path PRs and nightly schedule; cargo-audit. | Separate result; not an MSRV build, despite installing Rust 1.90. |
| `plan` (Release workflow) | All PRs; dist metadata planning. | No release artifacts are built/published in plan-only PR mode. |

Expected exclusions: coverage is main-only; documentation deployment is main-push
only; cargo-deny/audit can be absent on unrelated path-only PRs; release artifact,
host and announcement jobs are skipped in PR plan mode. An unexpected skip in a
mandatory gate must not be treated as validated behavior.

Source evidence: [CI](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/workflows/ci.yml),
[documentation](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/workflows/docs.yml),
[cargo-deny](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/workflows/cargo-deny.yml),
[security audit](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/workflows/security.yml),
[release](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/workflows/release.yml).
[TESTING.md](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/TESTING.md)
has stale claims that no Tier 4 tests run in PR CI. Follow workflow source until
that guidance is reconciled. No gate behavior is changed by this record.

## Settings observations and limits

Read-only GitHub API observations on October 6, 2026:

- `GET /repos/redis/redisctl/rules/branches/main` and ruleset `8009354`:
  active default-branch rules require a PR, one approving review and strict
  `CI Status`; deletion/non-fast-forward rules exist. Admin repository role has
  an always-bypass entry. Stale-review dismissal and last-push approval are off.
  See the [public ruleset](https://github.com/redis/redisctl/rules/8009354).
- `GET /repos/redis/redisctl/actions/permissions/workflow`: default token is read;
  Actions approval permission is enabled. Ordinary validation workflows omit
  explicit read permissions. Release workflow source declares broader permissions;
  permission minimization remains a follow-up, not a settings change here.
- `GET /repos/redis/redisctl/private-vulnerability-reporting`: disabled.
  `GET /repos/redis/redisctl` reports Dependabot security updates, secret scanning
  and push protection disabled. This does not prove the absence of an uninspected
  company security service or other external monitoring.
- `GET /repos/redis/redisctl/community/profile`: effective README, contributing,
  code-of-conduct, license and PR-template files resolve to this repository.
  This is presence evidence, not validation of the instructions or all support links.

These observations can change without a source commit. Refresh before settings
implementation; API access failures are unknown evidence, not disabled settings.
No secret values, private reports or customer environments were inspected.

## Common checklist assessment

| ID | Status | Evidence and remaining gap |
| --- | --- | --- |
| PUB-01 | Partial | Task-oriented README/docs and compatibility policy exist. Absolute “Full API Coverage” claims, canonical support/contact and tested product support still require #1085. |
| PUB-02 | Partial | Install/quick-start instructions exist. This assessment did not execute clean consumer installs; archive/ARM64 and stale example gaps above remain. |
| PUB-03 | Partial | MIT/Apache license files and workspace/package metadata exist. Actual shipped contents were not inspected here. |
| PUB-04 | Partial | Effective contributor/community files and local check commands exist. Author metadata is present; responsibility assignments, stale manual-release guidance, closed good-first-issue examples and migration-specific PR-template phases need follow-up. AGENTS.md still describes registry SDK dependencies as Git dependencies. |
| PUB-05 | Missing | SECURITY.md requests private reports but provides no actionable private destination (“Email … via GitHub”). Private reporting is disabled; route/triage owner and existing response-time promises need human decisions. |
| PUB-06 | Partial | Quick/unit/integration and MkDocs outcomes are enforced by their aggregates. Required Linux and strict rustdoc failures are omitted; controlled negative aggregate cases have not been demonstrated. |
| PUB-07 | Partial | `CI Status` runs on every PR. Docs path filtering excludes Rust/Cargo-only changes despite rustdoc requirements; other path-filtered checks are explicitly separate. |
| PUB-08 | Partial | Default token is read and Docker publishing permissions are job-scoped. Explicit validation permissions and broader release-workflow permission scoping/fork evidence are incomplete. |
| PUB-09 | Partial | Action SHA pins, weekly Actions Dependabot and cargo-deny/audit exist. Dependabot does not cover Cargo; assigned finding triage/update dispositions remain incomplete. |
| PUB-10 | Partial | PR guidance and one-approval ruleset exist. Shared human review model and revision-specific review records for adoption remain pending; agent reports are not human approval. |
| PUB-11 | Partial | Lockstep versioning, 1.x policy, changelogs and release workflows exist. Clean install/upgrade/artifact evidence, release ownership and rollback procedures remain #1087. |
| PUB-12 | Partial | Scheduled audits and tracked backlog exist. Permanent drift/security/CI triage owners and accepted exception review dates are not assigned by this assessment. |

Evidence for licensing/guidance: [license files](https://github.com/redis/redisctl/tree/1453352a88017612fda48f2e54b286100a64bd88),
[SECURITY.md](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/SECURITY.md),
[Dependabot](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/dependabot.yml)
and [PR template](https://github.com/redis/redisctl/blob/1453352a88017612fda48f2e54b286100a64bd88/.github/pull_request_template.md).

## Profile assessment

| Profile | Status | Present evidence and remaining acceptance |
| --- | --- | --- |
| Rust library | Partial | Stable/all-feature CI, core default check and strict default MCP/all-feature rustdoc exist. Dedicated MSRV build, maintained supported-feature matrix, package/consumer validation and full intentional API/SemVer baseline remain incomplete; #1088 owns the API work. |
| CLI | Partial | Parsing, mock command, configuration/output and destructive-control tests exist. Advertised channel/target/upgrade smoke evidence and final supported product matrix remain #1087/#1085. |
| MCP server/library | Partial | Catalog/schema/safety, embedding, policy, skills/resources and stdio tests exist. HTTP is preview and unauthenticated; complete lifecycle/resource-limit and packaged transport journeys are not established here. Database migration has its own acceptance. |
| Python binding | Not applicable | Redisctl ships no Python binding; the SDK repositories own those packages and release matrices. |
| Management API client | Not applicable | Redisctl consumes the separately shipped SDKs. Host CLI/MCP request-shape and journey evidence still matters, but SDK route/spec/server-family audit belongs to those libraries. |

Test/source inventory: [workspace tests](https://github.com/redis/redisctl/tree/1453352a88017612fda48f2e54b286100a64bd88/crates),
[MCP contract](../mcp/compatibility.md), [skills](../mcp/skills.md) and
[migration contract](../mcp/database-migration.md). Test presence is not a fresh
live run or exhaustive API/lifecycle coverage claim.

## Follow-ups and decisions

[#1181](https://github.com/redis/redisctl/issues/1181) tracks this candidate and
baseline documentation. It leaves [#628](https://github.com/redis/redisctl/issues/628),
[#1088](https://github.com/redis/redisctl/issues/1088), #1087 and #1085 open:
testing infrastructure, API stabilization, artifact/upgrade validation and support
coverage retain their existing scopes. Truthful CI/docs aggregates are a separate
proposed implementation slice, not implemented or certified here.

Human decisions are still required for the shared review model, private-reporting
destination and triage owner, and permanent standard/repository responsibility
assignments. No exception is approved by this document. Pending owners/review dates
must be filled by the adopting maintainer before calling a gap an accepted exception.

At this observation, opt-in migration [#1176](https://github.com/redis/redisctl/pull/1176),
Unicode profile fix [#1180](https://github.com/redis/redisctl/pull/1180) and release
[#1178](https://github.com/redis/redisctl/pull/1178) are separate open PRs, not audited
main. The pilot's public immutable library source resolves its former source-access
gate; it does not change the baseline's legacy default, publish a registry package
or complete the broader migration. No pin/default/credential/release decision is
made by this record.

This baseline came from a read-only October 6 inventory, then was reconciled with
the source and APIs listed above. Editorial review, reviewed head and disposition
belong in the documentation PR's evidence. Refresh this record after accepted
source/settings changes or support/release-policy decisions; passing this docs PR
alone does not promote any checklist item to verified.
