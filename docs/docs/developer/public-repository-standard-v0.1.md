# Public Repository Standard v0.1

**Status:** Candidate for discussion, October 6, 2026. This is not approved
organization policy or a statement that any repository is compliant.
Josh Rotenberg accepted the two policy targets below on October 6; the complete
checklist and actual repository adoption remain separate.

This checklist proposes a common, evidence-based baseline for public repositories.
The initial cohort is [redisctl](https://github.com/redis/redisctl),
[redis-cloud-rs](https://github.com/redis-developer/redis-cloud-rs),
[redis-enterprise-rs](https://github.com/redis-developer/redis-enterprise-rs) and
[redis-database-mcp-rs](https://github.com/redis-developer/redis-database-mcp-rs).
Redisctl hosts the first reference and its [adoption record](redisctl-adoption.md);
that location does not confer authority over other repositories or organizations.
Each repository applies the common checklist and only the profiles it ships.

## How to assess adoption

Record an audited source revision and dated settings observations. Classify every
applicable item as **verified**, **partial**, **missing**, **unknown** or
**not applicable**:

- **Verified:** the complete requirement has supporting, revision-specific evidence.
- **Partial:** some evidence exists, but named gaps remain.
- **Missing:** an observed requirement is absent or contradicted by evidence.
- **Unknown:** evidence was not obtained; absence of access is not proof of absence.
- **Not applicable:** explain why the requirement does not apply to this product.

A file's presence is not proof its instructions work. A configured workflow is not
proof a released artifact installs. A test that was ignored or exited early is not
an exercised journey. Link evidence rather than relying on a green badge or an
undated test count. Distinguish source guarantees, host policy and upstream behavior.

Accepted exceptions need a reason, follow-up, named human owner and review date.
Until approved, record them as gaps or pending decisions, not accepted exceptions.
Reassess affected items after source, settings, support or release changes.

## Common checklist

These are proposed adoption requirements, not authorization to change settings.

| ID | Observable outcome | Evidence to retain |
| --- | --- | --- |
| PUB-01 | Users can identify purpose, maturity, supported use and support boundaries. | README and working documentation/support destinations agree with the supported product matrix. |
| PUB-02 | A consumer outside the maintainer checkout can install and complete a first useful example through each advertised channel without private dependencies. | Identified artifact or immutable source revision, clean consumer environment, commands, results and prerequisites. |
| PUB-03 | Actual shipped packages have correct licenses and metadata. | Package contents, license declarations/files, source/documentation links and required bundled guidance checked at the release revision. |
| PUB-04 | Contributors can find human ownership and reproduce relevant checks. | Named maintainer/team and responsibilities, bug/PR guidance, local commands and verified effective repository/inherited community files. |
| PUB-05 | Vulnerabilities have a maintained private reporting route. | Accessible private destination, assigned human triage owner and documented expectations that the owner has accepted. |
| PUB-06 | Mandatory validation failures, cancellations and unexpected skips cannot produce a passing aggregate. | Required/advisory job inventory and controlled success/failure/cancellation/skip evidence for the aggregate at the reviewed revision. |
| PUB-07 | Relevant PRs consistently produce stable, unambiguous intended gate names. | Event/path coverage, check names and representative source, Cargo and documentation PR results; expected exclusions are explicit. |
| PUB-08 | Validation uses least-privilege permissions and does not require publishing secrets for public contributors. | Explicit read permissions for ordinary validation; write/OIDC permissions limited to justified jobs; reviewed fork execution paths. |
| PUB-09 | Dependency and workflow inputs have an update and finding-disposition process. | Reviewed immutable Action/shared-workflow refs, update configuration, dependency audits and owned dispositions for findings. |
| PUB-10 | Independent review applies to the actual proposed change. | Reviewed revision, validation scope, findings and disposition; separately record the selected human/GitHub approval policy. |
| PUB-11 | Distribution and compatibility promises match demonstrated release behavior. | Advertised channels/targets, versioning contract, artifact and upgrade results, release owner and rollback/critical-fix guidance. |
| PUB-12 | Failures and adoption drift have ongoing ownership. | Human triage ownership for scheduled failures and public reports, plus review triggers/dates for gaps and exceptions. |

## Product profiles

Record profile-level evidence as well as PUB-01 through PUB-12. Product matrices,
coverage floors and release channels belong to each repository's supported contract;
this candidate does not invent uniform floors, platforms or support promises.

| Profile | Additional observable outcomes |
| --- | --- |
| Rust library | Declared MSRV and stable compiler are exercised; supported feature combinations, examples/doctests and strict rustdoc pass; actual package contents and a separate consuming project are tested; API/SemVer checks match the maturity and compatibility policy. |
| CLI | Command, output/exit and configuration contracts, credential handling and destructive-operation controls have tests; advertised install channels, targets and upgrade journeys have artifact evidence. |
| MCP library/server | Tool/schema/resource contracts, advertised protocol/transports and embedding have evidence; authorization, resource and lifecycle limits are documented and tested; library guarantees are distinguished from host policy. |
| Python binding | Built wheels/sdist install and work on the supported matrix; root Rust changes trigger relevant binding checks; release-time version synchronization or independent versioning is explicit. |
| Management API client | API contract/coverage and server-family support have evidence; live, costly or destructive validation has separate authorization, fixtures and cleanup; contributors retain useful credential-free checks. |

A Rust library does not acquire CLI installer requirements merely by adopting this
standard. Versions remain independent across repositories. Applicable profiles
describe a repository's own shipped interfaces, not every transitive dependency.

## Adoption record format

| Field | Content |
| --- | --- |
| Identity | Repository, audited source revision, standard version, applicable profiles and observation date. |
| Ownership | Human maintainer/team and explicit CI, vulnerability, release and drift responsibilities; unresolved assignments remain pending. |
| Support | Compiler/runtime versions, platforms, feature sets, release channels and compatibility policy; separate advertised from exercised. |
| Gates | Exact required/advisory check names, expected skips, source behavior and effective ruleset evidence with dated provenance. |
| Assessment | Status and evidence for all common items and applicable profiles, including concrete gaps and linked follow-ups. |
| Exceptions | Reason, human approval/owner, follow-up and review date, or an explicit pending decision. |
| Review | Reviewed revision, reviewer evidence, findings/disposition and remaining human decisions. |

Settings are time-specific observations, not immutable source. Record which API or
public ruleset was inspected and when. An inaccessible setting remains unknown.
Classic branch protection and rulesets are separate; a failed lookup of one does
not establish the absence of protection. Effective inherited guidance must be
checked rather than inferred from a missing local file. See GitHub's
[community-file guidance](https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/creating-a-default-community-health-file).

## Accepted targets and pending adoption details

| Decision | Accepted target or pending choice | Still required |
| --- | --- | --- |
| Review model | **Accepted:** required meaningful CI, independent agent review and Josh Rotenberg's merge decision for this rollout. | Propose and authorize exact settings separately; reconcile the existing required human approval without silently weakening rules or using a bypass. |
| Security reporting | **Accepted:** GitHub private vulnerability reporting with a named human triage owner. | Name the owner, authorize and verify settings adoption, and agree realistic response expectations before advertising a maintained route. No owner is assigned here. |
| Shared ownership/home | **Pending:** this versioned redisctl reference is the initial home during evaluation. | Name the shared standard's human owner and permanent home, and designate each repository's adoption owner. |

Acceptance of a target is not adoption of settings, approval of the entire
candidate, compliance evidence or merge/release authorization. Existing repository
requirements remain in force until an exact settings change is separately approved
and verified. The security target does not enable reporting or establish a response
promise before ownership and adoption are resolved.

Agent review is evidence, not automatically an eligible GitHub approval. Review
requirements follow the configured policy and reviewer permissions; see
[GitHub review requirements](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).
Private vulnerability reporting supplies a private submission path when enabled;
it does not assign a triage owner. See
[GitHub reporting configuration](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository).

## Incremental adoption

1. Review this candidate and an honest per-repository record.
2. Prepare narrowly scoped source fixes for demonstrated gaps. Correct aggregate
   behavior before proposing new ruleset enforcement. Preserve advisory platforms.
3. Resolve the named security owner and standard ownership/home; obtain approval
   for the exact settings changes needed to adopt the accepted targets.
4. Apply authorized changes, verify their effect and record adopted revisions and
   outstanding gaps or approved exceptions.
5. Plan merges and releases using their own acceptance criteria and authorization.

For tracked agent implementation, check issue/file overlap, create a dedicated
branch, push an empty kickoff commit and open a scoped draft before edits. Keep
work visible through validation and use “leaves #N open” for retained issues.
For ordinary implementation, follow the repository's readiness rules; policy
proposals can intentionally remain draft after documentation validation. None of
these rules grants merge, publication, provisioning or settings-change authority.

The candidate is not a blanket new prerequisite for unrelated existing PRs. Scope
relevant defects to the affected change and schedule other adoption gaps separately.
MCP migration, setup/lifecycle work and release packaging retain their existing
acceptance criteria. Source changes do not establish that settings were changed.
