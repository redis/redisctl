//! Database, CRDB, and database alert tools

use std::time::Duration;

use redis_enterprise::alerts::AlertHandler;
use redis_enterprise::bdb::{CreateDatabaseRequest, DatabaseHandler, DatabaseUpgradeRequest};
use redis_enterprise::crdb::CrdbHandler;
use redis_enterprise::stats::{StatsHandler, StatsQuery};
use redisctl_core::enterprise::{
    backup_database_and_wait, flush_database_and_wait, import_database_and_wait,
};
use serde_json::Value;
use tower_mcp::{CallToolResult, ResultExt};

use crate::serde_helpers;
use crate::tools::macros::{enterprise_tool, mcp_module};

mcp_module! {
    list_databases => "list_enterprise_databases",
    get_database => "get_enterprise_database",
    get_database_stats => "get_database_stats",
    list_database_alerts => "list_database_alerts",
    backup_enterprise_database => "backup_enterprise_database",
    import_enterprise_database => "import_enterprise_database",
    create_enterprise_database => "create_enterprise_database",
    update_enterprise_database => "update_enterprise_database",
    delete_enterprise_database => "delete_enterprise_database",
    flush_enterprise_database => "flush_enterprise_database",
    export_enterprise_database => "export_enterprise_database",
    restore_enterprise_database => "restore_enterprise_database",
    upgrade_enterprise_database_redis => "upgrade_enterprise_database_redis",
    list_enterprise_crdbs => "list_enterprise_crdbs",
    get_enterprise_crdb => "get_enterprise_crdb",
    get_enterprise_crdb_tasks => "get_enterprise_crdb_tasks",
    create_enterprise_crdb => "create_enterprise_crdb",
    update_enterprise_crdb => "update_enterprise_crdb",
    delete_enterprise_crdb => "delete_enterprise_crdb",
}

// ============================================================================
// Database Read Operations
// ============================================================================

enterprise_tool!(read_only, list_databases, "list_enterprise_databases",
    "List all databases. Supports filtering by name and status.",
    {
        /// Optional filter by database name (case-insensitive substring match)
        #[serde(default)]
        pub name_filter: Option<String>,
        /// Optional filter by database status (e.g., "active", "pending", "creation-failed")
        #[serde(default)]
        pub status_filter: Option<String>,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let databases = handler
            .list()
            .await
            .tool_context("Failed to list databases")?;

        // Apply name filter
        let filtered: Vec<_> = databases
            .into_iter()
            .filter(|db| {
                if let Some(filter) = &input.name_filter {
                    db.name.to_lowercase().contains(&filter.to_lowercase())
                } else {
                    true
                }
            })
            .filter(|db| {
                if let Some(filter) = &input.status_filter {
                    db.status
                        .as_ref()
                        .map(|s| s.to_lowercase() == filter.to_lowercase())
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .collect();

        CallToolResult::from_list("databases", &filtered)
    }
);

enterprise_tool!(read_only, get_database, "get_enterprise_database",
    "Get database details by UID.",
    {
        /// Database UID
        pub uid: u32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let database = handler
            .get(input.uid)
            .await
            .tool_context("Failed to get database")?;

        CallToolResult::from_serialize(&database)
    }
);

enterprise_tool!(read_only, get_database_stats, "get_database_stats",
    "Get statistics for a specific database. Optionally specify interval and time range \
     for historical data.",
    {
        /// Database UID
        pub uid: u32,
        /// Time interval for aggregation: "1sec", "10sec", "5min", "15min", "1hour", "12hour", "1week"
        #[serde(default)]
        pub interval: Option<String>,
        /// Start time for historical query (ISO 8601 format, e.g., "2024-01-15T10:00:00Z")
        #[serde(default)]
        pub start_time: Option<String>,
        /// End time for historical query (ISO 8601 format)
        #[serde(default)]
        pub end_time: Option<String>,
    } => |client, input| {
        let handler = StatsHandler::new(client);

        if input.interval.is_some()
            || input.start_time.is_some()
            || input.end_time.is_some()
        {
            let query = StatsQuery {
                interval: input.interval,
                stime: input.start_time,
                etime: input.end_time,
                metrics: None,
            };
            let stats = handler
                .database(input.uid, Some(query))
                .await
                .tool_context("Failed to get database stats")?;
            CallToolResult::from_serialize(&stats)
        } else {
            let stats = handler
                .database_last(input.uid)
                .await
                .tool_context("Failed to get database stats")?;
            CallToolResult::from_serialize(&stats)
        }
    }
);

enterprise_tool!(read_only, list_database_alerts, "list_database_alerts",
    "List all alerts for a specific database.",
    {
        /// Database UID
        pub uid: u32,
    } => |client, input| {
        let handler = AlertHandler::new(client);
        let alerts = handler
            .list_by_database(input.uid)
            .await
            .tool_context("Failed to list database alerts")?;

        CallToolResult::from_list("alerts", &alerts)
    }
);

// ============================================================================
// Database Write Operations
// ============================================================================

fn default_enterprise_timeout() -> u64 {
    600
}

enterprise_tool!(write, backup_enterprise_database, "backup_enterprise_database",
    "Trigger a database backup and wait for completion.",
    {
        /// Database UID to backup
        pub bdb_uid: u32,
        /// Timeout in seconds (default: 600)
        #[serde(default = "default_enterprise_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Use Layer 2 workflow
        backup_database_and_wait(
            &client,
            input.bdb_uid,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to backup database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Backup completed successfully",
            "bdb_uid": input.bdb_uid
        }))
    }
);

enterprise_tool!(write, import_enterprise_database, "import_enterprise_database",
    "Import data into a database from an external source and wait for completion. \
     WARNING: If flush is true, existing data will be deleted before import.",
    {
        /// Database UID to import into
        pub bdb_uid: u32,
        /// Import location (file path or URL)
        pub import_location: String,
        /// Whether to flush the database before import (default: false)
        #[serde(default)]
        pub flush: bool,
        /// Timeout in seconds (default: 600)
        #[serde(default = "default_enterprise_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Use Layer 2 workflow
        import_database_and_wait(
            &client,
            input.bdb_uid,
            &input.import_location,
            input.flush,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to import database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Import completed successfully",
            "bdb_uid": input.bdb_uid,
            "import_location": input.import_location
        }))
    }
);

enterprise_tool!(write, create_enterprise_database, "create_enterprise_database",
    "Create a new database on the Enterprise cluster. \
     Prerequisites: 1) get_cluster -- verify the cluster is healthy and has capacity. \
     2) get_enterprise_cluster_policy -- check the default Redis version and rack-awareness settings. \
     3) list_enterprise_databases -- check name uniqueness and review existing databases.",
    {
        /// Database name
        pub name: String,
        /// Memory size in **bytes** (not GB — unlike Cloud tools). Example: 1073741824 = 1 GB.
        /// IMPORTANT: passing a value in GB (e.g., 1.0) will create a 1-byte database.
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub memory_size: Option<u64>,
        /// Redis engine version in MAJOR.MINOR form (for example, "7.2")
        pub redis_version: Option<String>,
        /// Port number (optional, cluster will assign if not specified)
        pub port: Option<u16>,
        /// Enable replication for high availability
        #[serde(default)]
        pub replication: Option<bool>,
        /// Persistence mode: "disabled", "aof", "snapshot", "aof_and_snapshot"
        pub persistence: Option<String>,
        /// Eviction policy: "noeviction", "allkeys-lru", "volatile-lru", etc.
        pub eviction_policy: Option<String>,
        /// Enable sharding (clustering)
        #[serde(default)]
        pub sharding: Option<bool>,
        /// Number of shards (if sharding is enabled)
        pub shards_count: Option<u32>,
    } => |client, input| {
        // Build the request using struct construction (all Option fields have defaults)
        let request = CreateDatabaseRequest {
            name: input.name.clone(),
            memory_size: input.memory_size,
            redis_version: input.redis_version.clone(),
            port: input.port,
            replication: input.replication,
            persistence: input.persistence.clone(),
            eviction_policy: input.eviction_policy.clone(),
            sharding: input.sharding,
            shards_count: input.shards_count,
            shard_count: None,
            proxy_policy: None,
            rack_aware: None,
            module_list: None,
            crdt: None,
            authentication_redis_pass: None,
        };

        let handler = DatabaseHandler::new(client);
        let database = handler
            .create(request)
            .await
            .tool_context("Failed to create database")?;

        CallToolResult::from_serialize(&database)
    }
);

enterprise_tool!(write, update_enterprise_database, "update_enterprise_database",
    "Update database configuration. Pass fields to update as JSON.",
    {
        /// Database UID to update
        pub uid: u32,
        /// JSON object with fields to update (e.g., {"memory_size": 2147483648, "replication": true})
        pub updates: Value,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let database = handler
            .update(input.uid, input.updates)
            .await
            .tool_context("Failed to update database")?;

        CallToolResult::from_serialize(&database)
    }
);

enterprise_tool!(destructive, delete_enterprise_database, "delete_enterprise_database",
    "DANGEROUS: Delete a database and all its data.",
    {
        /// Database UID to delete
        pub uid: u32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        handler
            .delete(input.uid)
            .await
            .tool_context("Failed to delete database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Database deleted successfully",
            "uid": input.uid
        }))
    }
);

enterprise_tool!(destructive, flush_enterprise_database, "flush_enterprise_database",
    "DANGEROUS: Flush all data from a database.",
    {
        /// Database UID to flush
        pub bdb_uid: u32,
        /// Timeout in seconds (default: 600)
        #[serde(default = "default_enterprise_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Use Layer 2 workflow
        flush_database_and_wait(
            &client,
            input.bdb_uid,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to flush database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Database flushed successfully",
            "bdb_uid": input.bdb_uid
        }))
    }
);

enterprise_tool!(write, export_enterprise_database, "export_enterprise_database",
    "Export a database to a specified location (e.g., S3, FTP).",
    {
        /// Database UID to export
        pub uid: u32,
        /// Export location (e.g., S3 URL or FTP path)
        pub export_location: String,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let response = handler
            .export(input.uid, &input.export_location)
            .await
            .tool_context("Failed to export database")?;

        CallToolResult::from_serialize(&response)
    }
);

enterprise_tool!(write, restore_enterprise_database, "restore_enterprise_database",
    "Restore a database from a backup.",
    {
        /// Database UID to restore
        pub uid: u32,
        /// Optional backup UID to restore from (uses latest if not specified)
        #[serde(default)]
        pub backup_uid: Option<String>,
    } => |client, input| {
        // `DatabaseHandler::restore` was removed upstream; POST the restore
        // action directly (BDB.RESTORE).
        let body = match input.backup_uid.as_deref() {
            Some(backup_id) => serde_json::json!({ "backup_uid": backup_id }),
            None => serde_json::json!({}),
        };
        let response: redis_enterprise::bdb::DatabaseActionResponse = client
            .post(&format!("/v1/bdbs/{}/actions/restore", input.uid), &body)
            .await
            .tool_context("Failed to restore database")?;

        CallToolResult::from_serialize(&response)
    }
);

enterprise_tool!(write, upgrade_enterprise_database_redis, "upgrade_enterprise_database_redis",
    "Upgrade the Redis version of a database.",
    {
        /// Database UID to upgrade
        pub uid: u32,
        /// Target Redis version (defaults to latest if not specified)
        #[serde(default)]
        pub redis_version: Option<String>,
        /// Restart shards even if no version change
        #[serde(default)]
        pub force_restart: Option<bool>,
        /// Allow data loss in non-replicated, non-persistent databases
        #[serde(default)]
        pub may_discard_data: Option<bool>,
    } => |client, input| {
        let request = DatabaseUpgradeRequest {
            redis_version: input.redis_version,
            force_restart: input.force_restart,
            may_discard_data: input.may_discard_data,
            ..Default::default()
        };

        let handler = DatabaseHandler::new(client);
        let response = handler
            .upgrade_redis_version(input.uid, request)
            .await
            .tool_context("Failed to upgrade database Redis version")?;

        CallToolResult::from_serialize(&response)
    }
);

// ============================================================================
// CRDB (Active-Active) tools
// ============================================================================

enterprise_tool!(read_only, list_enterprise_crdbs, "list_enterprise_crdbs",
    "List all Active-Active (CRDB) databases.",
    {} => |client, _input| {
        let handler = CrdbHandler::new(client);
        let crdbs = handler.list().await.tool_context("Failed to list CRDBs")?;

        CallToolResult::from_list("crdbs", &crdbs)
    }
);

enterprise_tool!(read_only, get_enterprise_crdb, "get_enterprise_crdb",
    "Get details of a specific Active-Active (CRDB) database by GUID.",
    {
        /// CRDB GUID (globally unique identifier)
        pub guid: String,
    } => |client, input| {
        let handler = CrdbHandler::new(client);
        let crdb = handler
            .get(&input.guid)
            .await
            .tool_context("Failed to get CRDB")?;

        CallToolResult::from_serialize(&crdb)
    }
);

enterprise_tool!(read_only, get_enterprise_crdb_tasks, "get_enterprise_crdb_tasks",
    "Get tasks for a specific Active-Active (CRDB) database.",
    {
        /// CRDB GUID (globally unique identifier)
        pub guid: String,
    } => |client, input| {
        let handler = CrdbHandler::new(client);
        let tasks = handler
            .tasks(&input.guid)
            .await
            .tool_context("Failed to get CRDB tasks")?;

        CallToolResult::from_serialize(&tasks)
    }
);

enterprise_tool!(write, create_enterprise_crdb, "create_enterprise_crdb",
    "Create a new Active-Active (CRDB) database. Pass full configuration as JSON.",
    {
        /// Full CRDB configuration as JSON (name, memory_size, instances, etc.)
        pub request: Value,
    } => |client, input| {
        let crdb: Value = client
            .post("/v1/crdbs", &input.request)
            .await
            .tool_context("Failed to create CRDB")?;

        CallToolResult::from_serialize(&crdb)
    }
);

enterprise_tool!(write, update_enterprise_crdb, "update_enterprise_crdb",
    "Update an Active-Active (CRDB) database. Pass fields to update as JSON.",
    {
        /// CRDB GUID (globally unique identifier)
        pub guid: String,
        /// JSON object with fields to update
        pub updates: Value,
    } => |client, input| {
        let handler = CrdbHandler::new(client);
        let crdb = handler
            .update(&input.guid, input.updates)
            .await
            .tool_context("Failed to update CRDB")?;

        CallToolResult::from_serialize(&crdb)
    }
);

enterprise_tool!(destructive, delete_enterprise_crdb, "delete_enterprise_crdb",
    "DANGEROUS: Delete an Active-Active (CRDB) database across all participating clusters.",
    {
        /// CRDB GUID (globally unique identifier) to delete
        pub guid: String,
    } => |client, input| {
        let handler = CrdbHandler::new(client);
        handler
            .delete(&input.guid)
            .await
            .tool_context("Failed to delete CRDB")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "CRDB deleted successfully",
            "guid": input.guid
        }))
    }
);
