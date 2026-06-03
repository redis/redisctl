# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.10.1...redisctl-mcp-v0.11.0) - 2026-03-20

### Added

- *(mcp)* cluster-aware connections + client_name ([#906](https://github.com/redis-developer/redisctl/pull/906))

### Fixed

- *(auth)* support Redis Cloud secret env var alias ([#913](https://github.com/redis-developer/redisctl/pull/913))
- *(mcp)* preserve raw tool denies in synthesized policy ([#912](https://github.com/redis-developer/redisctl/pull/912))

## [0.10.1](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.10.0...redisctl-mcp-v0.10.1) - 2026-03-19

### Other

- docs + feat(mcp): .mcp.json setup, tool hints, and 5 new skills ([#902](https://github.com/redis-developer/redisctl/pull/902))

## [0.10.0](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.9.1...redisctl-mcp-v0.10.0) - 2026-03-17

### Added

- *(mcp)* add session-scoped command aliases ([#891](https://github.com/redis-developer/redisctl/pull/891))
- *(mcp)* add if_exists option to redis_ft_create ([#889](https://github.com/redis-developer/redisctl/pull/889))
- *(mcp)* improve bulk_load for JSON seeding + collect_results flag ([#888](https://github.com/redis-developer/redisctl/pull/888))
- *(mcp)* upgrade tower-mcp to 0.8.2 with dynamic prompt skills ([#845](https://github.com/redis-developer/redisctl/pull/845))
- *(mcp)* add RediSearch skill prompts for index optimization workflows ([#843](https://github.com/redis-developer/redisctl/pull/843))

### Fixed

- *(mcp)* place NOSTEM before WEIGHT in FT.CREATE field args ([#879](https://github.com/redis-developer/redisctl/pull/879))
- *(mcp)* serde coercion for numeric params + bulk load/seed tools ([#875](https://github.com/redis-developer/redisctl/pull/875))
- *(mcp)* bail when --oauth is used (not yet implemented) ([#860](https://github.com/redis-developer/redisctl/pull/860))
- *(mcp)* validate enum-like string params in RediSearch tools ([#864](https://github.com/redis-developer/redisctl/pull/864))
- *(mcp)* harden YAML frontmatter parser and add tests ([#868](https://github.com/redis-developer/redisctl/pull/868))
- *(mcp)* validate nx/xx mutual exclusivity in json_set ([#863](https://github.com/redis-developer/redisctl/pull/863))
- *(mcp)* validate non-empty collections in RediSearch tools ([#865](https://github.com/redis-developer/redisctl/pull/865))
- *(mcp)* validate non-empty collections in RedisJSON tools ([#862](https://github.com/redis-developer/redisctl/pull/862))
- *(mcp)* reclassify ft_aliasdel and ft_dictdel as write tier ([#870](https://github.com/redis-developer/redisctl/pull/870))
- *(mcp)* set permissive default on app tracing filter when audit enabled ([#859](https://github.com/redis-developer/redisctl/pull/859))

### Other

- *(mcp)* improve tool descriptions for search and bulk/seed tools ([#880](https://github.com/redis-developer/redisctl/pull/880))
- *(mcp)* extract K/V pair formatting helper in search tools ([#871](https://github.com/redis-developer/redisctl/pull/871))
- *(mcp)* clarify FT.ALTER limitations in query-tuning skill ([#861](https://github.com/redis-developer/redisctl/pull/861))
- *(mcp)* remove unnecessary HashMap clones in main ([#866](https://github.com/redis-developer/redisctl/pull/866))
- *(mcp)* clean up misleading _name param and blanket allow(dead_code) ([#867](https://github.com/redis-developer/redisctl/pull/867))
- *(mcp)* unify static and dynamic prompt registration ([#869](https://github.com/redis-developer/redisctl/pull/869))

## [0.9.1](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.9.0...redisctl-mcp-v0.9.1) - 2026-03-07

### Added

- *(mcp)* add RedisJSON and RediSearch module tools ([#840](https://github.com/redis-developer/redisctl/pull/840))

### Other

- Database Quickstart and database-first messaging ([#842](https://github.com/redis-developer/redisctl/pull/842))

## [0.9.0](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.8.3...redisctl-mcp-v0.9.0) - 2026-03-06

### Added

- *(mcp)* add Docker Compose demo and upgrade tower-mcp to 0.8.1 ([#831](https://github.com/redis-developer/redisctl/pull/831))

### Fixed

- *(mcp)* accept --read-only=false and add REV to redis_zrange ([#827](https://github.com/redis-developer/redisctl/pull/827))

### Other

- *(mcp)* upgrade tower-mcp to 0.8.0 ([#830](https://github.com/redis-developer/redisctl/pull/830))

## [0.8.3](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.8.2...redisctl-mcp-v0.8.3) - 2026-03-06

### Other

- update Cargo.toml dependencies

## [0.5.0](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.4.0...redisctl-mcp-v0.5.0) - 2026-03-04

### Added

- *(mcp)* add sub-module syntax for --tools CLI flag ([#783](https://github.com/redis-developer/redisctl/pull/783)) ([#790](https://github.com/redis-developer/redisctl/pull/790))
- *(mcp)* add `enabled = false` per toolset in policy file ([#782](https://github.com/redis-developer/redisctl/pull/782)) ([#788](https://github.com/redis-developer/redisctl/pull/788))
- *(mcp)* add tool visibility presets for manageable tool surfaces ([#786](https://github.com/redis-developer/redisctl/pull/786)) ([#787](https://github.com/redis-developer/redisctl/pull/787))
- *(mcp)* add raw API passthrough tools ([#785](https://github.com/redis-developer/redisctl/pull/785))
- *(mcp)* add 27 Enterprise tools for coverage parity (#769, #770, #771, #772) ([#779](https://github.com/redis-developer/redisctl/pull/779))
- *(mcp)* close Cloud tool coverage gaps ([#773](https://github.com/redis-developer/redisctl/pull/773)) ([#778](https://github.com/redis-developer/redisctl/pull/778))
- *(mcp)* add audit logging for tool invocations ([#767](https://github.com/redis-developer/redisctl/pull/767)) ([#777](https://github.com/redis-developer/redisctl/pull/777))
- *(mcp)* add granular allow/deny policy configuration ([#776](https://github.com/redis-developer/redisctl/pull/776))

### Other

- *(mcp)* optimize tool descriptions for token efficiency ([#784](https://github.com/redis-developer/redisctl/pull/784)) ([#792](https://github.com/redis-developer/redisctl/pull/792))

## [0.4.0](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.3.0...redisctl-mcp-v0.4.0) - 2026-02-28

### Added

- *(mcp)* migrate to tower-mcp 0.7.0 ([#748](https://github.com/redis-developer/redisctl/pull/748))
- *(mcp)* add safety annotations, normalize instructions, add verification tests ([#610](https://github.com/redis-developer/redisctl/pull/610)) ([#746](https://github.com/redis-developer/redisctl/pull/746))
- *(mcp)* add 4 composed Redis diagnostic tools ([#737](https://github.com/redis-developer/redisctl/pull/737)) ([#744](https://github.com/redis-developer/redisctl/pull/744))
- *(mcp)* add 18 write-gated Redis data management tools ([#743](https://github.com/redis-developer/redisctl/pull/743))
- *(mcp)* add profile-based connection support for Redis database tools ([#742](https://github.com/redis-developer/redisctl/pull/742))
- *(mcp)* add 14 Redis read tools for streams, pub/sub, diagnostics, ACL, modules ([#741](https://github.com/redis-developer/redisctl/pull/741))
- *(mcp)* add 27 Fixed/Essentials tier MCP tools ([#734](https://github.com/redis-developer/redisctl/pull/734))
- *(mcp)* add 5 cloud accounts (BYOC) MCP tools ([#733](https://github.com/redis-developer/redisctl/pull/733))
- *(mcp)* add 51 cloud networking MCP tools for VPC, TGW, PSC, PrivateLink ([#732](https://github.com/redis-developer/redisctl/pull/732))
- *(mcp)* add 19 cloud subscription and database MCP tools ([#731](https://github.com/redis-developer/redisctl/pull/731))
- *(mcp)* add ACL write, cost report, and payment method cloud tools ([#730](https://github.com/redis-developer/redisctl/pull/730))
- *(mcp)* add 18 enterprise tools for node actions, RBAC, CRDB, and LDAP ([#715](https://github.com/redis-developer/redisctl/pull/715))
- *(cli)* add profile tags for organizing many profiles ([#692](https://github.com/redis-developer/redisctl/pull/692)) ([#705](https://github.com/redis-developer/redisctl/pull/705))
- *(mcp)* surface credential issues clearly in tool error responses ([#695](https://github.com/redis-developer/redisctl/pull/695)) ([#704](https://github.com/redis-developer/redisctl/pull/704))
- *(mcp)* add profile_create tool for creating profiles via MCP ([#646](https://github.com/redis-developer/redisctl/pull/646)) ([#703](https://github.com/redis-developer/redisctl/pull/703))
- *(cli)* add --connect flag to profile validate for connectivity testing ([#688](https://github.com/redis-developer/redisctl/pull/688))

### Other

- *(mcp)* split redis.rs into server, keys, and structures submodules ([#740](https://github.com/redis-developer/redisctl/pull/740))
- *(mcp)* split enterprise.rs and cloud.rs into domain submodules ([#717](https://github.com/redis-developer/redisctl/pull/717))

## [0.3.0](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.2.0...redisctl-mcp-v0.3.0) - 2026-02-25

### Added

- *(mcp)* auto-detect toolsets from config profiles ([#667](https://github.com/redis-developer/redisctl/pull/667))
- *(mcp)* modular router with feature flags and runtime toolset selection ([#656](https://github.com/redis-developer/redisctl/pull/656))
- *(mcp)* default to read-only mode ([#655](https://github.com/redis-developer/redisctl/pull/655))
- *(mcp)* add multi-profile support for Cloud tools ([#654](https://github.com/redis-developer/redisctl/pull/654))
- *(mcp)* add multi-profile support for Enterprise tools ([#651](https://github.com/redis-developer/redisctl/pull/651)) ([#652](https://github.com/redis-developer/redisctl/pull/652))
- *(mcp)* add create_subscription tool for Cloud ([#643](https://github.com/redis-developer/redisctl/pull/643))
- *(mcp)* add Enterprise license, cluster, and certificate management tools ([#636](https://github.com/redis-developer/redisctl/pull/636))
- *(mcp)* add Enterprise Redis ACL tools ([#635](https://github.com/redis-developer/redisctl/pull/635))
- *(mcp)* add Cloud certificate and Enterprise roles tools ([#634](https://github.com/redis-developer/redisctl/pull/634))
- *(mcp)* add Cloud database flush operation ([#633](https://github.com/redis-developer/redisctl/pull/633))
- *(mcp)* add Enterprise database write operations ([#632](https://github.com/redis-developer/redisctl/pull/632))
- [**breaking**] implement Layer 2 architecture in redisctl-core ([#630](https://github.com/redis-developer/redisctl/pull/630))
- add custom CA certificate support for Kubernetes deployments ([#624](https://github.com/redis-developer/redisctl/pull/624))
- *(mcp)* upgrade tower-mcp to 0.3.4 ([#622](https://github.com/redis-developer/redisctl/pull/622))
- add filtering support and new Redis diagnostic tools ([#621](https://github.com/redis-developer/redisctl/pull/621))
- add individual getter tools for Cloud and Enterprise resources ([#620](https://github.com/redis-developer/redisctl/pull/620))
- add MCP resources and prompts to redisctl-mcp ([#619](https://github.com/redis-developer/redisctl/pull/619))
- *(mcp)* add read-only tool filter using CapabilityFilter ([#618](https://github.com/redis-developer/redisctl/pull/618))
- *(mcp)* add historical stats, Cloud logs, debug info, and modules tools ([#617](https://github.com/redis-developer/redisctl/pull/617))
- *(mcp)* add Enterprise logs and aggregate stats tools ([#616](https://github.com/redis-developer/redisctl/pull/616))
- *(mcp)* add Enterprise license tools ([#615](https://github.com/redis-developer/redisctl/pull/615))
- *(mcp)* add mock testing support for cloud and enterprise tools ([#611](https://github.com/redis-developer/redisctl/pull/611))
- *(mcp)* add profile management tools ([#609](https://github.com/redis-developer/redisctl/pull/609))

### Fixed

- pre-release cleanup — cfg-gate warnings and stale doc versions ([#676](https://github.com/redis-developer/redisctl/pull/676))
- *(mcp)* wrap array results in JSON objects for structuredContent compliance ([#653](https://github.com/redis-developer/redisctl/pull/653))
- *(mcp)* normalize 'default' profile to use configured default ([#608](https://github.com/redis-developer/redisctl/pull/608))

### Other

- *(mcp)* bump tower-mcp to 0.5.0 ([#658](https://github.com/redis-developer/redisctl/pull/658))
- consolidate workspace dependencies ([#640](https://github.com/redis-developer/redisctl/pull/640))
- upgrade tower-mcp to 0.2.3 and use from_serialize() ([#607](https://github.com/redis-developer/redisctl/pull/607))

## [0.1.2](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.1.1...redisctl-mcp-v0.1.2) - 2026-01-23

### Added

- *(mcp)* add --database-url CLI option for direct Redis connections ([#574](https://github.com/redis-developer/redisctl/pull/574))
- *(mcp)* add database tools for direct Redis connections ([#572](https://github.com/redis-developer/redisctl/pull/572))

### Other

- add Python bindings documentation and update CHANGELOGs ([#581](https://github.com/redis-developer/redisctl/pull/581))

## [0.1.1](https://github.com/redis-developer/redisctl/compare/redisctl-mcp-v0.1.0...redisctl-mcp-v0.1.1) - 2026-01-14

### Added

- *(mcp)* add Private Link, Transit Gateway, BDB Groups, OCSP, and Suffixes tools ([#561](https://github.com/redis-developer/redisctl/pull/561))
- *(mcp)* add VPC Peering, Cloud Accounts, and CRDB Tasks tools ([#560](https://github.com/redis-developer/redisctl/pull/560))
- *(mcp)* add 25 new tools for enterprise and cloud operations ([#559](https://github.com/redis-developer/redisctl/pull/559))

### Other

- *(mcp)* add readme to redisctl-mcp crate ([#534](https://github.com/redis-developer/redisctl/pull/534))
- *(redisctl)* release v0.7.4 ([#517](https://github.com/redis-developer/redisctl/pull/517))

## [0.1.0](https://github.com/redis-developer/redisctl/releases/tag/redisctl-mcp-v0.1.0) - 2026-01-12

### Added

- add MCP server for AI integration ([#531](https://github.com/redis-developer/redisctl/pull/531))
