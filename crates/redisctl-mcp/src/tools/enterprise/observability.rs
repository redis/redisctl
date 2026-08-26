//! Alerts, logs, aggregate stats, shards, debug info, and module tools

use redis_enterprise::logs::{LogsHandler, LogsQuery};
use tower_mcp::{CallToolResult, ResultExt};

use crate::tools::macros::{enterprise_tool, mcp_module};

mcp_module! {
    list_alerts => "list_alerts",
    acknowledge_enterprise_alert => "acknowledge_enterprise_alert",
    list_logs => "list_logs",
    get_all_nodes_stats => "get_all_nodes_stats",
    get_all_databases_stats => "get_all_databases_stats",
    get_shard_stats => "get_shard_stats",
    get_all_shards_stats => "get_all_shards_stats",
    list_shards => "list_shards",
    get_shard => "get_shard",
    list_shards_by_database => "list_shards_by_database",
    list_shards_by_node => "list_shards_by_node",
    list_modules => "list_modules",
    get_module => "get_module",
}

// ============================================================================
// Alert tools
// ============================================================================

enterprise_tool!(read_only, list_alerts, "list_alerts",
    "List all active alerts.",
    {} => |client, _input| {
        // The `AlertHandler::list` convenience method was removed upstream; call
        // the cluster-wide alerts route directly to preserve this tool.
        let alerts: Vec<redis_enterprise::alerts::Alert> = client
            .get("/v1/alerts")
            .await
            .tool_context("Failed to list alerts")?;
        CallToolResult::from_list("alerts", &alerts)
    }
);

enterprise_tool!(write, acknowledge_enterprise_alert, "acknowledge_enterprise_alert",
    "Acknowledge (clear) a specific alert by ID.",
    {
        /// Alert UID to acknowledge
        pub alert_uid: String,
    } => |client, input| {
        // The `AlertHandler::clear` convenience method was removed upstream;
        // delete the alert directly to preserve this tool.
        client
            .delete(&format!("/v1/alerts/{}", input.alert_uid))
            .await
            .tool_context("Failed to acknowledge alert")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Alert acknowledged successfully",
            "alert_uid": input.alert_uid
        }))
    }
);

// ============================================================================
// Logs tools
// ============================================================================

enterprise_tool!(read_only, list_logs, "list_logs",
    "List cluster event logs. Supports filtering by time range and pagination.",
    {
        /// Start time - only return events after this time (ISO 8601 format, e.g., "2024-01-15T10:00:00Z")
        #[serde(default)]
        pub start_time: Option<String>,
        /// End time - only return events before this time (ISO 8601 format)
        #[serde(default)]
        pub end_time: Option<String>,
        /// Sort order: "asc" (oldest first) or "desc" (newest first, default)
        #[serde(default)]
        pub order: Option<String>,
        /// Maximum number of log entries to return
        #[serde(default)]
        pub limit: Option<u32>,
        /// Number of entries to skip (for pagination)
        #[serde(default)]
        pub offset: Option<u32>,
    } => |client, input| {
        let query = if input.start_time.is_some()
            || input.end_time.is_some()
            || input.order.is_some()
            || input.limit.is_some()
            || input.offset.is_some()
        {
            Some(LogsQuery {
                stime: input.start_time,
                etime: input.end_time,
                order: input.order,
                limit: input.limit,
                offset: input.offset,
            })
        } else {
            None
        };

        let handler = LogsHandler::new(client);
        let logs = handler
            .list(query)
            .await
            .tool_context("Failed to list logs")?;

        CallToolResult::from_list("logs", &logs)
    }
);

// ============================================================================
// Aggregate Stats tools
// ============================================================================

enterprise_tool!(read_only, get_all_nodes_stats, "get_all_nodes_stats",
    "Get current statistics for all nodes including CPU, memory, and network metrics.",
    {} => |client, _input| {
        let handler = redis_enterprise::stats::StatsHandler::new(client);
        let stats = handler
            .nodes_last()
            .await
            .tool_context("Failed to get all nodes stats")?;

        CallToolResult::from_serialize(&stats)
    }
);

enterprise_tool!(read_only, get_all_databases_stats, "get_all_databases_stats",
    "Get current statistics for all databases including latency, throughput, and memory usage.",
    {} => |client, _input| {
        let handler = redis_enterprise::stats::StatsHandler::new(client);
        let stats = handler
            .databases_last()
            .await
            .tool_context("Failed to get all databases stats")?;

        CallToolResult::from_serialize(&stats)
    }
);

enterprise_tool!(read_only, get_shard_stats, "get_shard_stats",
    "Get current statistics for a specific shard.",
    {
        /// Shard UID
        pub uid: u32,
    } => |client, input| {
        let handler = redis_enterprise::stats::StatsHandler::new(client);
        let stats = handler
            .shard(input.uid, None)
            .await
            .tool_context("Failed to get shard stats")?;

        CallToolResult::from_serialize(&stats)
    }
);

enterprise_tool!(read_only, get_all_shards_stats, "get_all_shards_stats",
    "Get current statistics for all shards.",
    {} => |client, _input| {
        let handler = redis_enterprise::stats::StatsHandler::new(client);
        let stats = handler
            .shards(None)
            .await
            .tool_context("Failed to get all shards stats")?;

        CallToolResult::from_serialize(&stats)
    }
);

// ============================================================================
// Shard tools
// ============================================================================

enterprise_tool!(read_only, list_shards, "list_shards",
    "List all shards. Optionally filter by database UID.",
    {
        /// Optional database UID to filter by
        #[serde(default)]
        pub database_uid: Option<u32>,
    } => |client, input| {
        let handler = redis_enterprise::shards::ShardHandler::new(client);
        let shards = if let Some(db_uid) = input.database_uid {
            handler
                .list_by_database(db_uid)
                .await
                .tool_context("Failed to list shards")?
        } else {
            handler.list().await.tool_context("Failed to list shards")?
        };

        CallToolResult::from_list("shards", &shards)
    }
);

enterprise_tool!(read_only, get_shard, "get_shard",
    "Get shard details including role (master/replica), status, and assigned node.",
    {
        /// Shard UID (e.g., "1" or "2")
        pub uid: String,
    } => |client, input| {
        let handler = redis_enterprise::shards::ShardHandler::new(client);
        let shard = handler
            .get(&input.uid)
            .await
            .tool_context("Failed to get shard")?;

        CallToolResult::from_serialize(&shard)
    }
);

enterprise_tool!(read_only, list_shards_by_database, "list_shards_by_database",
    "List all shards for a specific database.",
    {
        /// Database UID to list shards for
        pub bdb_uid: u32,
    } => |client, input| {
        let handler = redis_enterprise::shards::ShardHandler::new(client);
        let shards = handler
            .list_by_database(input.bdb_uid)
            .await
            .tool_context("Failed to list shards by database")?;

        CallToolResult::from_list("shards", &shards)
    }
);

enterprise_tool!(read_only, list_shards_by_node, "list_shards_by_node",
    "List all shards on a specific node.",
    {
        /// Node UID to list shards for
        pub node_uid: u32,
    } => |client, input| {
        let handler = redis_enterprise::shards::ShardHandler::new(client);
        let shards = handler
            .list_by_node(input.node_uid)
            .await
            .tool_context("Failed to list shards by node")?;

        CallToolResult::from_list("shards", &shards)
    }
);

// ============================================================================
// Module tools
// ============================================================================

enterprise_tool!(read_only, list_modules, "list_modules",
    "List all installed Redis modules.",
    {} => |client, _input| {
        let handler = redis_enterprise::modules::ModuleHandler::new(client);
        let modules = handler
            .list()
            .await
            .tool_context("Failed to list modules")?;

        CallToolResult::from_list("modules", &modules)
    }
);

enterprise_tool!(read_only, get_module, "get_module",
    "Get details of a specific Redis module by UID.",
    {
        /// Module UID
        pub uid: String,
    } => |client, input| {
        let handler = redis_enterprise::modules::ModuleHandler::new(client);
        let module = handler
            .get(&input.uid)
            .await
            .tool_context("Failed to get module")?;

        CallToolResult::from_serialize(&module)
    }
);
