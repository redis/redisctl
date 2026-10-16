//! Server-level Redis tools (ping, info, dbsize, client_list, cluster_info, slowlog,
//! config_get, memory_stats, latency_history, acl_list, acl_whoami, module_list,
//! config_set, flushdb)

use tower_mcp::CallToolResult;

use super::RedisResultExt;

use crate::serde_helpers;
use crate::tools::macros::{database_tool, mcp_module};

mcp_module! {
    ping => "redis_ping",
    info => "redis_info",
    dbsize => "redis_dbsize",
    client_list => "redis_client_list",
    cluster_info => "redis_cluster_info",
    slowlog => "redis_slowlog",
    config_get => "redis_config_get",
    memory_stats => "redis_memory_stats",
    latency_history => "redis_latency_history",
    acl_list => "redis_acl_list",
    acl_whoami => "redis_acl_whoami",
    module_list => "redis_module_list",
    config_set => "redis_config_set",
    flushdb => "redis_flushdb",
}

database_tool!(read_only, ping, "redis_ping",
    "Test connectivity by sending a PING command",
    {} => |conn, _input| {
        let response: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .tool_context("PING failed")?;

        Ok(CallToolResult::text(format!(
            "Connected successfully. Response: {}",
            response
        )))
    }
);

database_tool!(read_only, info, "redis_info",
    "Get server information and statistics (INFO command).",
    {
        /// Optional section to retrieve (e.g., "server", "memory", "stats")
        #[serde(default)]
        pub section: Option<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("INFO");
        if let Some(section) = &input.section {
            cmd.arg(section);
        }

        let info: String = cmd
            .query_async(&mut conn)
            .await
            .tool_context("INFO failed")?;

        Ok(CallToolResult::text(info))
    }
);

database_tool!(read_only, dbsize, "redis_dbsize",
    "Get the number of keys in the current database.",
    {} => |conn, _input| {
        let size: i64 = redis::cmd("DBSIZE")
            .query_async(&mut conn)
            .await
            .tool_context("DBSIZE failed")?;

        Ok(CallToolResult::text(format!(
            "Database contains {} keys",
            size
        )))
    }
);

database_tool!(read_only, client_list, "redis_client_list",
    "List client connections (CLIENT LIST).",
    {} => |conn, _input| {
        let clients: String = redis::cmd("CLIENT")
            .arg("LIST")
            .query_async(&mut conn)
            .await
            .tool_context("CLIENT LIST failed")?;

        let count = clients.lines().count();
        Ok(CallToolResult::text(format!(
            "{} connected client(s):\n\n{}",
            count, clients
        )))
    }
);

database_tool!(read_only, cluster_info, "redis_cluster_info",
    "Get cluster information (only works on cluster-enabled instances).",
    {} => |conn, _input| {
        let info: String = redis::cmd("CLUSTER")
            .arg("INFO")
            .query_async(&mut conn)
            .await
            .tool_context("CLUSTER INFO failed")?;

        Ok(CallToolResult::text(info))
    }
);

fn default_slowlog_count() -> usize {
    10
}

database_tool!(read_only, slowlog, "redis_slowlog",
    "Get slow query log entries for identifying performance issues.",
    {
        /// Number of entries to return (default: 10)
        #[serde(default = "default_slowlog_count", deserialize_with = "serde_helpers::string_or_usize::deserialize")]
        pub count: usize,
    } => |conn, input| {
        // SLOWLOG GET returns nested arrays
        let entries: Vec<Vec<redis::Value>> = redis::cmd("SLOWLOG")
            .arg("GET")
            .arg(input.count)
            .query_async(&mut conn)
            .await
            .tool_context("SLOWLOG GET failed")?;

        if entries.is_empty() {
            return Ok(CallToolResult::text("No slow queries recorded"));
        }

        let mut output = format!("Slow log ({} entries):\n\n", entries.len());

        for entry in entries {
            // Each entry is: [id, timestamp, duration_us, command_args, ...]
            if entry.len() >= 4 {
                let id = super::format_value(&entry[0]);
                let duration_us = super::format_value(&entry[2]);
                let command = if let redis::Value::Array(args) = &entry[3] {
                    args.iter()
                        .map(super::format_value)
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    super::format_value(&entry[3])
                };

                output.push_str(&format!("#{} - {} us: {}\n", id, duration_us, command));
            }
        }

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, config_get, "redis_config_get",
    "Get configuration parameter values (CONFIG GET). \
     Supports glob-style patterns.",
    {
        /// Configuration parameter pattern (e.g. "maxmemory", "save", "*")
        pub parameter: String,
    } => |conn, input| {
        let result: Vec<(String, String)> = redis::cmd("CONFIG")
            .arg("GET")
            .arg(&input.parameter)
            .query_async(&mut conn)
            .await
            .tool_context("CONFIG GET failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text(format!(
                "No configuration parameters matching '{}'",
                input.parameter
            )));
        }

        let output = result
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult::text(format!(
            "Configuration ({} parameter(s)):\n{}",
            result.len(),
            output
        )))
    }
);

database_tool!(read_only, memory_stats, "redis_memory_stats",
    "Get memory usage breakdown by category (MEMORY STATS).",
    {} => |conn, _input| {
        let result: redis::Value = redis::cmd("MEMORY")
            .arg("STATS")
            .query_async(&mut conn)
            .await
            .tool_context("MEMORY STATS failed")?;

        Ok(CallToolResult::text(super::format_value(&result)))
    }
);

database_tool!(read_only, latency_history, "redis_latency_history",
    "Get latency history for a specific event (LATENCY HISTORY). \
     May return empty if latency monitoring is not enabled \
     (CONFIG SET latency-monitor-threshold <ms>).",
    {
        /// Latency event name (e.g. "command", "fast-command")
        pub event: String,
    } => |conn, input| {
        let result: Vec<Vec<redis::Value>> = redis::cmd("LATENCY")
            .arg("HISTORY")
            .arg(&input.event)
            .query_async(&mut conn)
            .await
            .tool_context("LATENCY HISTORY failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text(format!(
                "No latency history for event '{}'. \
                 Latency monitoring may not be enabled \
                 (CONFIG SET latency-monitor-threshold <ms>).",
                input.event
            )));
        }

        let mut output = format!(
            "Latency history for '{}' ({} entries):\n\n",
            input.event,
            result.len()
        );

        for entry in &result {
            if entry.len() >= 2 {
                let timestamp = super::format_value(&entry[0]);
                let latency_ms = super::format_value(&entry[1]);
                output.push_str(&format!("  {} - {} ms\n", timestamp, latency_ms));
            }
        }

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, acl_list, "redis_acl_list",
    "List all ACL rules (ACL LIST).",
    {} => |conn, _input| {
        let rules: Vec<String> = redis::cmd("ACL")
            .arg("LIST")
            .query_async(&mut conn)
            .await
            .tool_context("ACL LIST failed")?;

        if rules.is_empty() {
            return Ok(CallToolResult::text("No ACL rules configured"));
        }

        Ok(CallToolResult::text(format!(
            "ACL rules ({}):\n{}",
            rules.len(),
            rules.join("\n")
        )))
    }
);

database_tool!(read_only, acl_whoami, "redis_acl_whoami",
    "Get the current authenticated username (ACL WHOAMI).",
    {} => |conn, _input| {
        let username: String = redis::cmd("ACL")
            .arg("WHOAMI")
            .query_async(&mut conn)
            .await
            .tool_context("ACL WHOAMI failed")?;

        Ok(CallToolResult::text(format!("Current user: {}", username)))
    }
);

database_tool!(read_only, module_list, "redis_module_list",
    "List loaded modules with names and versions (MODULE LIST).",
    {} => |conn, _input| {
        let result: redis::Value = redis::cmd("MODULE")
            .arg("LIST")
            .query_async(&mut conn)
            .await
            .tool_context("MODULE LIST failed")?;

        let formatted = super::format_value(&result);
        if formatted == "[]" {
            return Ok(CallToolResult::text("No modules loaded"));
        }

        Ok(CallToolResult::text(format!(
            "Loaded modules:\n{}",
            formatted
        )))
    }
);

// --- Write tools ---

database_tool!(write, config_set, "redis_config_set",
    "Set a configuration parameter at runtime (CONFIG SET). \
     Changes may not persist unless CONFIG REWRITE is called.",
    {
        /// Configuration parameter name
        pub parameter: String,
        /// Configuration parameter value
        pub value: String,
    } => |conn, input| {
        let _: () = redis::cmd("CONFIG")
            .arg("SET")
            .arg(&input.parameter)
            .arg(&input.value)
            .query_async(&mut conn)
            .await
            .tool_context("CONFIG SET failed")?;

        Ok(CallToolResult::text(format!(
            "OK - set {} = {}",
            input.parameter, input.value
        )))
    }
);

database_tool!(destructive, flushdb, "redis_flushdb",
    "DANGEROUS: Delete all keys in the current database. \
     Set async_flush=true for non-blocking operation.",
    {
        /// Use asynchronous flush (non-blocking, default: false)
        #[serde(default)]
        pub async_flush: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("FLUSHDB");
        if input.async_flush {
            cmd.arg("ASYNC");
        }

        let _: () = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FLUSHDB failed")?;

        let mode = if input.async_flush { " (async)" } else { "" };
        Ok(CallToolResult::text(format!(
            "OK - database flushed{}",
            mode
        )))
    }
);
