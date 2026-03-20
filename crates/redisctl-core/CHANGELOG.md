# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0](https://github.com/redis-developer/redisctl/compare/redisctl-core-v0.10.1...redisctl-core-v0.11.0) - 2026-03-20

### Fixed

- *(auth)* support Redis Cloud secret env var alias ([#913](https://github.com/redis-developer/redisctl/pull/913))

## [0.2.0](https://github.com/redis-developer/redisctl/compare/redisctl-core-v0.1.0...redisctl-core-v0.2.0) - 2026-02-28

### Added

- *(cli)* add profile tags for organizing many profiles ([#692](https://github.com/redis-developer/redisctl/pull/692)) ([#705](https://github.com/redis-developer/redisctl/pull/705))

### Other

- add edge case tests for profile config loading ([#696](https://github.com/redis-developer/redisctl/pull/696)) ([#699](https://github.com/redis-developer/redisctl/pull/699))
- add repository and homepage metadata to redisctl-core ([#685](https://github.com/redis-developer/redisctl/pull/685))

## [0.1.0](https://github.com/redis-developer/redisctl/releases/tag/redisctl-core-v0.1.0) - 2026-02-25

### Added

- *(cli)* cargo-style diagnostic error formatting ([#671](https://github.com/redis-developer/redisctl/pull/671))
- *(cli)* infer platform from profile — make cloud/enterprise prefix optional ([#668](https://github.com/redis-developer/redisctl/pull/668))
- *(mcp)* add Cloud database flush operation ([#633](https://github.com/redis-developer/redisctl/pull/633))
- *(mcp)* add Enterprise database write operations ([#632](https://github.com/redis-developer/redisctl/pull/632))
- [**breaking**] implement Layer 2 architecture in redisctl-core ([#630](https://github.com/redis-developer/redisctl/pull/630))

### Other

- consolidate workspace dependencies ([#640](https://github.com/redis-developer/redisctl/pull/640))
