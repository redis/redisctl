# Tools Reference

The redisctl MCP server exposes **392 tools** across 4 toolsets and 2 system tools for managing Redis Cloud, Redis Enterprise, and direct database operations.

Tools are organized into **toolsets** (Cloud, Enterprise, Database, App) and further into **sub-modules** that can be selectively loaded with the [`--tools` flag](configuration.md#the-tools-flag).

Tools that modify state require `--read-only=false` or an appropriate [safety tier](configuration.md#safety-tiers). Database tools require a `--database-url` connection.

!!! tip "Runtime Discovery"
    Use the `list_available_tools` system tool at runtime to see exactly which tools are active in your current configuration, grouped by toolset. This is the most accurate way to discover available tools.

## System Tools (2 tools)

These tools are always available regardless of `--tools` selection or visibility presets.

| Tool | Description |
|------|-------------|
| `list_available_tools` | List all available tools grouped by toolset, showing active vs. hidden |
| `show_policy` | Show the active safety tier, per-toolset overrides, and allow/deny lists |

## Cloud Toolset (151 tools)

Redis Cloud management tools. Select with `--tools cloud` or target specific sub-modules.

### `cloud:subscriptions` (36 tools)

Manages flexible subscriptions and their databases -- creation, configuration, backup/import, tagging, CIDR allowlists, maintenance windows, Active-Active regions, and version upgrades.

| Representative Tools | Description |
|---------------------|-------------|
| `list_subscriptions` | List all Pro subscriptions |
| `get_subscription` | Get subscription details |
| `list_databases` | List databases in a subscription |
| `get_database` | Get database details |
| `create_database` | Create a new database *(write)* |
| `update_database` | Update database configuration *(write)* |
| `get_backup_status` | Get database backup status |
| `get_database_tags` | Get database tags |

### `cloud:account` (35 tools)

Account management -- users, ACL users/roles/rules, cloud provider accounts, payment methods, cost reports, and task tracking.

| Representative Tools | Description |
|---------------------|-------------|
| `get_account` | Get account information |
| `get_system_logs` | Get system event logs |
| `get_regions` | Get available regions by provider |
| `get_modules` | List available database modules |
| `list_account_users` | List account users |
| `list_acl_users` | List ACL users |
| `generate_cost_report` | Generate cost reports |
| `list_tasks` | List recent async tasks |

### `cloud:networking` (52 tools)

Network connectivity -- VPC peering, Transit Gateway, Private Service Connect (PSC), and AWS PrivateLink for both standard and Active-Active subscriptions.

| Representative Tools | Description |
|---------------------|-------------|
| `get_vpc_peering` | Get VPC peering details |
| `create_vpc_peering` | Create VPC peering *(write)* |
| `get_aa_vpc_peering` | Get Active-Active VPC peering |
| `get_tgw_attachments` | Get Transit Gateway attachments |
| `create_tgw_attachment` | Create Transit Gateway attachment *(write)* |
| `get_psc_service` | Get PSC configuration |
| `get_private_link` | Get PrivateLink configuration |
| `create_private_link` | Create PrivateLink *(write)* |

### `cloud:fixed` (27 tools)

Essentials/Fixed tier subscriptions and databases -- plans, backup, import, tagging, and version upgrades.

| Representative Tools | Description |
|---------------------|-------------|
| `list_fixed_subscriptions` | List Essentials subscriptions |
| `get_fixed_subscription` | Get Essentials subscription details |
| `create_fixed_subscription` | Create Essentials subscription *(write)* |
| `list_fixed_plans` | List available Essentials plans |
| `list_fixed_databases` | List databases in subscription |
| `get_fixed_database` | Get Essentials database details |
| `create_fixed_database` | Create Essentials database *(write)* |
| `get_fixed_database_backup_status` | Get backup status |

### `cloud:raw` (1 tool)

| Tool | Description |
|------|-------------|
| `cloud_raw_api` | Execute arbitrary Redis Cloud REST API requests |

## Enterprise Toolset (92 tools)

Redis Enterprise cluster management tools. Select with `--tools enterprise` or target specific sub-modules.

### `enterprise:cluster` (24 tools)

Cluster-level configuration -- license management, cluster policies, maintenance mode, TLS certificates, services, and node lifecycle.

| Representative Tools | Description |
|---------------------|-------------|
| `get_cluster` | Get cluster information |
| `get_license` | Get license information |
| `update_enterprise_license` | Update cluster license *(write)* |
| `get_enterprise_cluster_policy` | Get cluster policy settings |
| `update_enterprise_cluster_policy` | Update cluster policy *(write)* |
| `enable_enterprise_maintenance_mode` | Enable maintenance mode *(write)* |
| `list_nodes` | List all cluster nodes |
| `get_node` | Get node details |

### `enterprise:databases` (20 tools)

Database and Active-Active CRDB management -- CRUD, backup/import/export/restore, stats, endpoints, alerts, and version upgrades.

| Representative Tools | Description |
|---------------------|-------------|
| `list_enterprise_databases` | List all databases |
| `get_enterprise_database` | Get database details |
| `get_database_stats` | Get database statistics |
| `get_database_endpoints` | Get database endpoints |
| `create_enterprise_database` | Create a database *(write)* |
| `update_enterprise_database` | Update database config *(write)* |
| `backup_enterprise_database` | Trigger backup *(write)* |
| `list_enterprise_crdbs` | List Active-Active databases |

### `enterprise:rbac` (20 tools)

Role-based access control -- users, roles, ACL rules, built-in roles, and LDAP configuration.

| Representative Tools | Description |
|---------------------|-------------|
| `list_enterprise_users` | List all users |
| `get_enterprise_user` | Get user details |
| `create_enterprise_user` | Create a user *(write)* |
| `list_enterprise_roles` | List all roles |
| `get_enterprise_role` | Get role details |
| `create_enterprise_role` | Create a role *(write)* |
| `list_enterprise_acls` | List ACL rules |
| `get_enterprise_ldap_config` | Get LDAP configuration |

### `enterprise:observability` (16 tools)

Monitoring -- alerts, audit logs, aggregate stats for nodes/databases/shards, debug info, and module listing.

| Representative Tools | Description |
|---------------------|-------------|
| `list_alerts` | List active alerts |
| `acknowledge_enterprise_alert` | Acknowledge an alert *(write)* |
| `list_logs` | Get cluster event logs |
| `get_all_nodes_stats` | Get aggregate node statistics |
| `get_all_databases_stats` | Get aggregate database statistics |
| `list_shards` | List all shards |
| `get_shard_stats` | Get shard statistics |
| `list_modules` | List available modules |

### `enterprise:proxy` (4 tools)

Proxy management -- list, inspect, stats, and configuration updates.

| Tool | Description |
|------|-------------|
| `list_enterprise_proxies` | List all proxy instances |
| `get_enterprise_proxy` | Get proxy details |
| `get_enterprise_proxy_stats` | Get proxy statistics |
| `update_enterprise_proxy` | Update proxy configuration *(write)* |

### `enterprise:services` (7 tools)

System service lifecycle -- list, inspect, start, stop, restart, and status checks.

| Tool | Description |
|------|-------------|
| `list_enterprise_services` | List system services |
| `get_enterprise_service` | Get service details |
| `get_enterprise_service_status` | Get service status |
| `update_enterprise_service` | Update service configuration *(write)* |
| `start_enterprise_service` | Start a service *(write)* |
| `stop_enterprise_service` | Stop a service *(write)* |
| `restart_enterprise_service` | Restart a service *(write)* |

### `enterprise:raw` (1 tool)

| Tool | Description |
|------|-------------|
| `enterprise_raw_api` | Execute arbitrary Redis Enterprise REST API requests |

## Database Toolset (139 tools)

Direct Redis database operations. Requires `--database-url` connection. Select with `--tools database` or target specific sub-modules.

### `database:server` (14 tools)

Server-level operations -- connectivity, server info, client listing, slow log, memory stats, latency, ACL inspection, and config management.

| Representative Tools | Description |
|---------------------|-------------|
| `redis_ping` | Ping the server |
| `redis_info` | Get server information |
| `redis_dbsize` | Get number of keys |
| `redis_client_list` | List connected clients |
| `redis_slowlog` | Get slow log entries |
| `redis_memory_stats` | Get memory statistics |
| `redis_config_get` | Get config values |
| `redis_config_set` | Set config values *(write)* |

### `database:keys` (33 tools)

Key-space operations -- listing, scanning, get/set, type inspection, TTL, existence checks, memory usage, key mutation, multi-key operations, atomic counters, and string manipulation.

| Representative Tools | Description |
|---------------------|-------------|
| `redis_keys` | List keys matching a pattern |
| `redis_scan` | Scan keys with cursor |
| `redis_get` | Get string value |
| `redis_set` | Set string value *(write)* |
| `redis_mget` | Get multiple key values |
| `redis_mset` | Set multiple key-value pairs *(write)* |
| `redis_type` | Get key type |
| `redis_ttl` | Get key TTL |
| `redis_del` | Delete keys *(write)* |
| `redis_unlink` | Async-delete keys *(write)* |
| `redis_expire` | Set key expiration *(write)* |
| `redis_incr` | Increment integer value *(write)* |
| `redis_decr` | Decrement integer value *(write)* |

### `database:structures` (41 tools)

Data structure operations -- hashes, lists, sets, sorted sets, streams, pub/sub inspection, set algebra, and granular field/member accessors.

| Representative Tools | Description |
|---------------------|-------------|
| `redis_hgetall` | Get all hash fields |
| `redis_hget` | Get a single hash field |
| `redis_hmget` | Get multiple hash fields |
| `redis_hlen` | Get hash field count |
| `redis_lrange` | Get list range |
| `redis_llen` | Get list length |
| `redis_lindex` | Get list element by index |
| `redis_smembers` | Get all set members |
| `redis_scard` | Get set cardinality |
| `redis_sismember` | Check set membership |
| `redis_sunion` | Set union |
| `redis_sinter` | Set intersection |
| `redis_sdiff` | Set difference |
| `redis_zrange` | Get sorted set range |
| `redis_zscore` | Get member score |
| `redis_zrank` | Get member rank |
| `redis_zcount` | Count members by score range |
| `redis_zrangebyscore` | Get members by score range |
| `redis_xinfo_stream` | Get stream info |
| `redis_xrange` | Get stream entries by ID range |
| `redis_xlen` | Get stream length |
| `redis_pubsub_channels` | List active pub/sub channels |

### `database:diagnostics` (5 tools)

Higher-level diagnostic tools that aggregate information from multiple Redis commands.

| Tool | Description |
|------|-------------|
| `redis_health_check` | Comprehensive health check |
| `redis_key_summary` | Key distribution summary |
| `redis_hotkeys` | Hot key detection |
| `redis_connection_summary` | Connection pool summary |
| `redis_memory_doctor` | Memory usage analysis and recommendations |

### `database:json` (16 tools)

JSON document operations using RedisJSON -- get, set, merge, delete, increment, array/object manipulation, and path-based queries.

| Representative Tools | Description |
|---------------------|-------------|
| `redis_json_get` | Get JSON value at path |
| `redis_json_set` | Set JSON value at path *(write)* |
| `redis_json_del` | Delete JSON value at path *(write)* |
| `redis_json_mget` | Get JSON values from multiple keys |
| `redis_json_type` | Get JSON type at path |
| `redis_json_arrlen` | Get JSON array length |
| `redis_json_objkeys` | Get JSON object keys |
| `redis_json_numincrby` | Increment numeric JSON value *(write)* |

### `database:search` (19 tools)

Full-text and vector search via RediSearch -- index management, FT.SEARCH, FT.AGGREGATE, suggest, explain, profile, and cursor-based result iteration.

| Representative Tools | Description |
|---------------------|-------------|
| `redis_ft_create` | Create a search index *(write)* |
| `redis_ft_search` | Execute full-text search |
| `redis_ft_aggregate` | Execute aggregation pipeline |
| `redis_ft_info` | Get index information |
| `redis_ft_list` | List all search indexes |
| `redis_ft_explain` | Explain query execution plan |
| `redis_ft_profile` | Profile query execution |
| `redis_ft_sugadd` | Add suggestion to index *(write)* |

### `database:aliases` (4 tools)

Session-scoped command aliases -- save, list, run, and delete named command sequences for repeatable workflows.

| Tool | Description |
|------|-------------|
| `redis_alias_set` | Save a named command alias *(write)* |
| `redis_alias_list` | List all saved aliases |
| `redis_alias_run` | Execute a saved alias |
| `redis_alias_delete` | Delete a saved alias *(write)* |

### `database:bulk` (6 tools)

Bulk and batch operations -- multi-key get/delete, TTL management, key migration, and pattern-based bulk operations.

| Representative Tools | Description |
|---------------------|-------------|
| `redis_bulk_get` | Get multiple keys by pattern |
| `redis_bulk_delete` | Delete keys by pattern *(write)* |
| `redis_bulk_expire` | Set TTL on multiple keys *(write)* |
| `redis_bulk_copy` | Copy keys to another database *(write)* |
| `redis_pipeline_exec` | Execute a batch of commands *(write)* |
| `redis_bulk_ttl_report` | Report TTL distribution |

### `database:raw` (1 tool)

| Tool | Description |
|------|-------------|
| `redis_command` | Execute arbitrary Redis commands |

## App Toolset (8 tools)

Profile and configuration management tools. Always compiled in; no sub-modules.

| Tool | Description |
|------|-------------|
| `profile_list` | List all configured profiles |
| `profile_show` | Show profile details |
| `profile_path` | Show config file path |
| `profile_validate` | Validate profile credentials |
| `profile_set_default_cloud` | Set default Cloud profile |
| `profile_set_default_enterprise` | Set default Enterprise profile |
| `profile_delete` | Delete a profile *(write)* |
| `profile_create` | Create a new profile *(write)* |

## Summary

| Toolset | Sub-modules | Tools |
|---------|-------------|-------|
| Cloud | `subscriptions` (36), `account` (35), `networking` (52), `fixed` (27), `raw` (1) | **151** |
| Enterprise | `cluster` (24), `databases` (20), `rbac` (20), `observability` (16), `proxy` (4), `services` (7), `raw` (1) | **92** |
| Database | `server` (14), `keys` (33), `structures` (41), `diagnostics` (5), `json` (16), `search` (19), `aliases` (4), `bulk` (6), `raw` (1) | **139** |
| App | *(flat)* | **8** |
| System | *(always on)* | **2** |
| **Total** | | **392** |

## Example Tool Usage

### Working with Data

> **You**: Add a user to my users hash with name "Alice" and email "alice@example.com"
>
> **AI**: *uses `redis_hgetall` to check existing fields, then `redis_set` or hash mutation tools*
>
> Added 2 fields to hash `user:123`:
> - name: Alice
> - email: alice@example.com

### Leaderboard Operations

> **You**: Show me the top 5 players on the leaderboard
>
> **AI**: *uses `redis_zrange` with reverse ordering, start=0, stop=4*
>
> | Rank | Player | Score |
> |------|--------|-------|
> | 1 | alice | 15000 |
> | 2 | bob | 12500 |
> | 3 | charlie | 11000 |
> | 4 | diana | 9500 |
> | 5 | eve | 8000 |

### Full-Text Search

> **You**: Search for products containing "wireless headphones" under $100
>
> **AI**: *uses database tools with RediSearch commands*
>
> Found 3 products:
> - Sony WH-1000XM4 - $89.99
> - JBL Tune 500BT - $49.99
> - Anker Soundcore - $39.99

### Infrastructure Management

> **You**: What's the cluster health?
>
> **AI**: *uses `get_cluster`, `get_all_nodes_stats`, `list_enterprise_databases`*
>
> **Cluster**: docker-cluster (v8.0.2-17)
>
> | Metric | Value |
> |--------|-------|
> | CPU Utilization | 4% |
> | Memory Available | 7.81 GB |
> | Nodes | 1 active |
> | Databases | 1 active |
> | Alerts | None |
