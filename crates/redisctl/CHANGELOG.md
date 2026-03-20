# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0](https://github.com/redis-developer/redisctl/compare/redisctl-v0.10.1...redisctl-v0.11.0) - 2026-03-20

### Fixed

- *(auth)* support Redis Cloud secret env var alias ([#913](https://github.com/redis-developer/redisctl/pull/913))

### Other

- *(readme)* fix MCP policy example ([#915](https://github.com/redis-developer/redisctl/pull/915))

## [0.10.1](https://github.com/redis-developer/redisctl/compare/redisctl-v0.10.0...redisctl-v0.10.1) - 2026-03-19

### Fixed

- get-modules uses /v1/bdbs/{id} module_list instead of 404 endpoint ([#900](https://github.com/redis-developer/redisctl/pull/900))

## [0.10.0](https://github.com/redis-developer/redisctl/compare/redisctl-v0.9.1...redisctl-v0.10.0) - 2026-03-17

### Other

- update Cargo.toml dependencies

## [0.9.1](https://github.com/redis-developer/redisctl/compare/redisctl-v0.9.0...redisctl-v0.9.1) - 2026-03-07

### Other

- update Cargo.toml dependencies

## [0.9.0](https://github.com/redis-developer/redisctl/compare/redisctl-v0.8.3...redisctl-v0.9.0) - 2026-03-06

### Other

- update Cargo.toml dependencies

## [0.8.3](https://github.com/redis-developer/redisctl/compare/redisctl-v0.8.2...redisctl-v0.8.3) - 2026-03-06

### Other

- clarify Docker env var forwarding for password in README ([#819](https://github.com/redis-developer/redisctl/pull/819))
- rewrite README with elevator pitch and streamlined structure ([#817](https://github.com/redis-developer/redisctl/pull/817)) ([#818](https://github.com/redis-developer/redisctl/pull/818))
- sync all workspace crate versions to 0.8.2 ([#816](https://github.com/redis-developer/redisctl/pull/816))

## [0.8.2](https://github.com/redis-developer/redisctl/compare/redisctl-v0.8.1...redisctl-v0.8.2) - 2026-03-04

### Added

- *(enterprise)* add curated table formatters for list/get commands ([#761](https://github.com/redis-developer/redisctl/pull/761)) ([#763](https://github.com/redis-developer/redisctl/pull/763))
- *(cli)* add dynamic shell completions and ValueHint annotations ([#758](https://github.com/redis-developer/redisctl/pull/758))

### Fixed

- *(output)* standardize table styles and add Cloud ACL formatters ([#764](https://github.com/redis-developer/redisctl/pull/764))

### Other

- *(output)* unify OutputFormat and standardize table library ([#749](https://github.com/redis-developer/redisctl/pull/749)) ([#760](https://github.com/redis-developer/redisctl/pull/760))

## [0.8.1](https://github.com/redis-developer/redisctl/compare/redisctl-v0.8.0...redisctl-v0.8.1) - 2026-02-28

### Added

- *(cli)* add table output and brief summary for enterprise status ([#714](https://github.com/redis-developer/redisctl/pull/714))
- *(cli)* add cluster health verification commands ([#626](https://github.com/redis-developer/redisctl/pull/626)) ([#713](https://github.com/redis-developer/redisctl/pull/713))
- *(cli)* add profile tags for organizing many profiles ([#692](https://github.com/redis-developer/redisctl/pull/692)) ([#705](https://github.com/redis-developer/redisctl/pull/705))
- *(cli)* preserve profile settings on credential update ([#694](https://github.com/redis-developer/redisctl/pull/694)) ([#702](https://github.com/redis-developer/redisctl/pull/702))
- *(cli)* add profile current command for shell prompt integration ([#693](https://github.com/redis-developer/redisctl/pull/693)) ([#701](https://github.com/redis-developer/redisctl/pull/701))
- *(cli)* add interactive profile init wizard ([#690](https://github.com/redis-developer/redisctl/pull/690)) ([#698](https://github.com/redis-developer/redisctl/pull/698))
- *(cli)* improve profile help and discoverability ([#663](https://github.com/redis-developer/redisctl/pull/663)) ([#689](https://github.com/redis-developer/redisctl/pull/689))
- *(cli)* add --connect flag to profile validate for connectivity testing ([#688](https://github.com/redis-developer/redisctl/pull/688))

### Fixed

- *(cli)* improve error messages for credential and connection failures ([#691](https://github.com/redis-developer/redisctl/pull/691)) ([#700](https://github.com/redis-developer/redisctl/pull/700))

## [0.8.0](https://github.com/redis-developer/redisctl/compare/redisctl-v0.7.7...redisctl-v0.8.0) - 2026-02-25

### Added

- *(cli)* support name@version syntax for --module flag ([#675](https://github.com/redis-developer/redisctl/pull/675))
- *(cli)* group `profile list` output by deployment type ([#674](https://github.com/redis-developer/redisctl/pull/674))
- *(cli)* cargo-style diagnostic error formatting ([#671](https://github.com/redis-developer/redisctl/pull/671))
- *(cli)* infer platform from profile — make cloud/enterprise prefix optional ([#668](https://github.com/redis-developer/redisctl/pull/668))
- *(mcp)* add Enterprise license, cluster, and certificate management tools ([#636](https://github.com/redis-developer/redisctl/pull/636))
- [**breaking**] implement Layer 2 architecture in redisctl-core ([#630](https://github.com/redis-developer/redisctl/pull/630))
- add 'db open' command to spawn redis-cli with profile credentials ([#627](https://github.com/redis-developer/redisctl/pull/627))
- add custom CA certificate support for Kubernetes deployments ([#624](https://github.com/redis-developer/redisctl/pull/624))
- [**breaking**] rewrite redisctl-mcp using tower-mcp framework ([#597](https://github.com/redis-developer/redisctl/pull/597))
- update to redis-enterprise 0.8 ([#600](https://github.com/redis-developer/redisctl/pull/600))
- update to redis-cloud 0.9 ([#599](https://github.com/redis-developer/redisctl/pull/599))
- add one-shot cost-report export command ([#595](https://github.com/redis-developer/redisctl/pull/595))

### Fixed

- handle rate limits (429) and processing-completed state in task polling ([#587](https://github.com/redis-developer/redisctl/pull/587))

### Other

- update examples for prefix-free CLI commands ([#673](https://github.com/redis-developer/redisctl/pull/673))
- document Docker as zero-install MCP option ([#647](https://github.com/redis-developer/redisctl/pull/647)) ([#659](https://github.com/redis-developer/redisctl/pull/659))
- consolidate workspace dependencies ([#640](https://github.com/redis-developer/redisctl/pull/640))
- [**breaking**] extract redis-cloud and redis-enterprise to standalone repos ([#596](https://github.com/redis-developer/redisctl/pull/596))

## [0.7.7](https://github.com/redis-developer/redisctl/compare/redisctl-v0.7.6...redisctl-v0.7.7) - 2026-01-23

### Other

- update Cargo.toml dependencies

## [0.7.6](https://github.com/redis-developer/redisctl/compare/redisctl-v0.7.5...redisctl-v0.7.6) - 2026-01-23

### Added

- Add Python bindings via PyO3 ([#578](https://github.com/redis-developer/redisctl/pull/578))
- *(mcp)* add --database-url CLI option for direct Redis connections ([#574](https://github.com/redis-developer/redisctl/pull/574))
- *(mcp)* add database tools for direct Redis connections ([#572](https://github.com/redis-developer/redisctl/pull/572))
- *(config)* add database profile type for direct Redis connections ([#566](https://github.com/redis-developer/redisctl/pull/566))

### Other

- add Python bindings documentation and update CHANGELOGs ([#581](https://github.com/redis-developer/redisctl/pull/581))
- add assert_cmd tests for MCP commands ([#570](https://github.com/redis-developer/redisctl/pull/570))

### Added

- Add Python bindings via PyO3 for `redis-cloud` and `redis-enterprise` libraries ([#578](https://github.com/redis-developer/redisctl/pull/578))
  - `CloudClient` with async and sync methods for subscriptions and databases
  - `EnterpriseClient` with async and sync methods for cluster, databases, nodes, and users
  - Environment variable support via `from_env()` factory methods
  - Raw API access for unsupported endpoints
  - Available on PyPI: `pip install redisctl`

## [0.7.5](https://github.com/redis-developer/redisctl/compare/redisctl-v0.7.4...redisctl-v0.7.5) - 2026-01-14

### Added

- *(enterprise)* add first-class params for all remaining commands ([#558](https://github.com/redis-developer/redisctl/pull/558))
- *(enterprise)* add first-class params for job-scheduler, bdb-group, suffix, migration ([#556](https://github.com/redis-developer/redisctl/pull/556))
- *(enterprise)* add first-class params for LDAP mapping create/update ([#554](https://github.com/redis-developer/redisctl/pull/554))
- *(cli)* add first-class params for enterprise CRDB update ([#553](https://github.com/redis-developer/redisctl/pull/553))
- *(cli)* add first-class params for enterprise cluster update ([#552](https://github.com/redis-developer/redisctl/pull/552))
- *(cli)* add first-class params for enterprise node update ([#551](https://github.com/redis-developer/redisctl/pull/551))
- *(cli)* add first-class params for enterprise ACL create/update ([#550](https://github.com/redis-developer/redisctl/pull/550))
- *(cli)* add first-class params for enterprise role create/update ([#549](https://github.com/redis-developer/redisctl/pull/549))
- *(cli)* add first-class params for enterprise user create/update ([#548](https://github.com/redis-developer/redisctl/pull/548))
- *(cli)* add first-class params for enterprise database update ([#547](https://github.com/redis-developer/redisctl/pull/547))
- *(cli)* add first-class params for database update-aa-regions ([#546](https://github.com/redis-developer/redisctl/pull/546))
- *(cloud)* add first-class CLI params for provider-account commands ([#545](https://github.com/redis-developer/redisctl/pull/545))
- *(cloud)* add first-class CLI params for fixed-subscription commands ([#544](https://github.com/redis-developer/redisctl/pull/544))
- *(cloud)* add first-class CLI params for fixed-database commands ([#543](https://github.com/redis-developer/redisctl/pull/543))
- *(cloud)* add first-class params to database commands ([#542](https://github.com/redis-developer/redisctl/pull/542))
- *(cloud)* add first-class params to subscription commands ([#541](https://github.com/redis-developer/redisctl/pull/541))
- *(cloud)* add first-class CLI params for all connectivity commands ([#540](https://github.com/redis-developer/redisctl/pull/540))

## [0.7.4](https://github.com/redis-developer/redisctl/compare/redisctl-v0.7.3...redisctl-v0.7.4) - 2026-01-12

### Added

- add MCP server for AI integration ([#531](https://github.com/redis-developer/redisctl/pull/531))

### Other

- add Enterprise CLI Docker integration tests ([#523](https://github.com/redis-developer/redisctl/pull/523))

## [0.7.3](https://github.com/redis-developer/redisctl/compare/redisctl-v0.7.2...redisctl-v0.7.3) - 2025-12-17

### Added

- add module workflow tools (validate, inspect, package) ([#513](https://github.com/redis-developer/redisctl/pull/513))
- add module name lookup for module get and database create ([#512](https://github.com/redis-developer/redisctl/pull/512))

### Fixed

- support JMESPath backtick string literals and improve module upload error ([#511](https://github.com/redis-developer/redisctl/pull/511))
- correct repository URLs broken by PR #500 ([#506](https://github.com/redis-developer/redisctl/pull/506))

### Other

- update documentation URLs to new hosting location ([#509](https://github.com/redis-developer/redisctl/pull/509))
- release ([#503](https://github.com/redis-developer/redisctl/pull/503))

## [0.7.2](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.7.1...redisctl-v0.7.2) - 2025-12-13

### Added

- upgrade jmespath_extensions to 0.6 with full feature set ([#496](https://github.com/joshrotenberg/redisctl/pull/496))

## [0.7.1](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.7.0...redisctl-v0.7.1) - 2025-12-09

### Added

- add cross-platform pager support for Windows ([#491](https://github.com/joshrotenberg/redisctl/pull/491))
- *(cloud)* add delete endpoint for PrivateLink ([#487](https://github.com/joshrotenberg/redisctl/pull/487))
- *(cloud)* add upgrade endpoints for Essentials databases ([#488](https://github.com/joshrotenberg/redisctl/pull/488))
- *(cloud)* add available-versions command for Essentials databases ([#485](https://github.com/joshrotenberg/redisctl/pull/485))
- *(cloud)* add update-aa-regions command for Active-Active databases ([#486](https://github.com/joshrotenberg/redisctl/pull/486))
- *(cloud)* add update single tag endpoint for Pro databases ([#489](https://github.com/joshrotenberg/redisctl/pull/489))

## [0.7.0](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.6.6...redisctl-v0.7.0) - 2025-12-09

### Added

- *(cli)* integrate jmespath-extensions for enhanced query capabilities ([#482](https://github.com/joshrotenberg/redisctl/pull/482))
- *(cli)* add tower-resilience integration framework ([#459](https://github.com/joshrotenberg/redisctl/pull/459))
- *(cloud)* add task list, database flush, and available-versions commands ([#477](https://github.com/joshrotenberg/redisctl/pull/477))
- *(cloud)* add cost-report API support (Beta) ([#479](https://github.com/joshrotenberg/redisctl/pull/479))
- add user agent header to HTTP requests ([#473](https://github.com/joshrotenberg/redisctl/pull/473))
- *(enterprise)* add database watch command for real-time status monitoring ([#458](https://github.com/joshrotenberg/redisctl/pull/458))
- *(enterprise)* improve stats streaming UX with Ctrl+C handling ([#457](https://github.com/joshrotenberg/redisctl/pull/457))
- *(redis-enterprise)* add stats streaming with --follow flag ([#455](https://github.com/joshrotenberg/redisctl/pull/455))
- add first-class parameters to major create commands ([#449](https://github.com/joshrotenberg/redisctl/pull/449))
- add database upgrade command for Redis version upgrades ([#442](https://github.com/joshrotenberg/redisctl/pull/442))
- [**breaking**] improve CLI help text accuracy and add comprehensive test coverage ([#444](https://github.com/joshrotenberg/redisctl/pull/444))
- add payment-method commands to CLI ([#439](https://github.com/joshrotenberg/redisctl/pull/439))
- make --config-file take precedence over environment variables ([#438](https://github.com/joshrotenberg/redisctl/pull/438))

### Fixed

- upgrade indicatif to 0.18 to resolve RUSTSEC-2025-0119 ([#474](https://github.com/joshrotenberg/redisctl/pull/474))
- *(release)* improve Homebrew formula auto-update ([#433](https://github.com/joshrotenberg/redisctl/pull/433))

### Other

- *(redisctl)* add async_utils unit tests ([#472](https://github.com/joshrotenberg/redisctl/pull/472))
- split cli.rs into cloud.rs and enterprise.rs modules ([#454](https://github.com/joshrotenberg/redisctl/pull/454))
- update presentation materials with first-class parameters feature ([#450](https://github.com/joshrotenberg/redisctl/pull/450))
- add comprehensive CLI test coverage  ([#448](https://github.com/joshrotenberg/redisctl/pull/448))
- add comprehensive CLI tests with assert_cmd ([#435](https://github.com/joshrotenberg/redisctl/pull/435))

## [0.6.6](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.6.5...redisctl-v0.6.6) - 2025-10-29

### Added

- add --config-file flag for alternate configuration file ([#430](https://github.com/joshrotenberg/redisctl/pull/430))
- *(cli)* add AWS PrivateLink human-friendly commands ([#407](https://github.com/joshrotenberg/redisctl/pull/407))
- Add streaming logs support with --follow flag (Issue #70) ([#404](https://github.com/joshrotenberg/redisctl/pull/404))
- Add improved error messages with actionable suggestions (Issue #259) ([#401](https://github.com/joshrotenberg/redisctl/pull/401))

### Fixed

- handle processing-error state in async operations ([#431](https://github.com/joshrotenberg/redisctl/pull/431))

### Other

- add comprehensive presentation outline and rladmin comparison ([#415](https://github.com/joshrotenberg/redisctl/pull/415))
- Extract config/profile management to library crate ([#410](https://github.com/joshrotenberg/redisctl/pull/410))
- rewrite README for presentation readiness ([#408](https://github.com/joshrotenberg/redisctl/pull/408))
- extract profile commands from main.rs to dedicated module ([#403](https://github.com/joshrotenberg/redisctl/pull/403))

## [0.6.5](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.6.4...redisctl-v0.6.5) - 2025-10-07

### Added

- *(enterprise)* implement local node commands and expose shard commands

### Fixed

- add JSON output support to profile and version commands ([#394](https://github.com/joshrotenberg/redisctl/pull/394))

## [0.6.4](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.6.3...redisctl-v0.6.4) - 2025-10-07

### Fixed

- remove unused variable warning on Windows builds

## [0.6.3](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.6.2...redisctl-v0.6.3) - 2025-10-07

### Added

- add comprehensive Files.com API key management with secure storage
- add support package upload feature with files-sdk 0.3.1
- add support package optimization

### Fixed

- *(secure-storage)* enable platform-native keyring backends

### Other

- add support package optimization and upload documentation
- Merge pull request #371 from joshrotenberg/feat/homebrew-auto-update
- add Homebrew installation instructions

## [0.6.1](https://github.com/joshrotenberg/redisctl/compare/redisctl-v0.6.0...redisctl-v0.6.1) - 2025-09-16

### Fixed

- improve profile resolution for explicit cloud/enterprise commands ([#353](https://github.com/joshrotenberg/redisctl/pull/353))