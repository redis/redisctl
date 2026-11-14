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

        Ok(CallToolResult::text(if info.is_empty() {
            "(no INFO data returned)".to_string()
        } else {
            info
        }))
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
     Supports glob-style patterns. Authentication and TLS key passphrases are redacted.",
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

        let sensitive_fields = [
            "requirepass", "masterauth", "tls-key-file-pass", "tls-client-key-file-pass",
        ].map(str::to_string);
        let output = result
            .iter()
            .map(|(k, v)| {
                // Match each returned name, not the requested glob pattern.
                let redacted = crate::audit::redact_response_value(
                    &serde_json::json!({(k): v}), &sensitive_fields,
                );
                let safe_value = redacted.get(k).and_then(serde_json::Value::as_str)
                    .unwrap_or("[REDACTED]");
                format!("{}: {}", k, safe_value)
            })
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

        let mut output = super::format_value(&result);
        output.push_str(super::MEMORY_INTERPRETATION_NOTE);
        Ok(CallToolResult::text(output))
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
     Changes may not persist unless CONFIG REWRITE is called. \
     Security-sensitive parameters (authentication, persistence paths, \
     bind/TLS, replication, command-surface switches) are refused at every tier.",
    {
        /// Configuration parameter name
        pub parameter: String,
        /// Configuration parameter value
        pub value: String,
    } => |conn, input| {
        super::command_safety::check_config_parameter(&input.parameter)
            .map_err(tower_mcp::Error::tool)?;

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, tcp::OwnedReadHalf};

    use crate::state::AppState;

    async fn read_command(reader: &mut BufReader<OwnedReadHalf>) -> Vec<String> {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
        assert!(line.starts_with('*'));
        let count: usize = line[1..].trim_end().parse().unwrap();
        let mut command = Vec::with_capacity(count);
        for _ in 0..count {
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            assert!(line.starts_with('$'));
            let length: usize = line[1..].trim_end().parse().unwrap();
            let mut bytes = vec![0; length + 2];
            reader.read_exact(&mut bytes).await.unwrap();
            assert_eq!(&bytes[length..], b"\r\n");
            bytes.truncate(length);
            command.push(String::from_utf8(bytes).unwrap());
        }
        command
    }

    async fn call_config_get(
        parameter: &str,
        values: &[(&str, &str)],
    ) -> tower_mcp::CallToolResult {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let expected = parameter.to_string();
        let mut reply = format!("*{}\r\n", values.len() * 2);
        for (name, value) in values {
            for part in [name, value] {
                reply.push_str(&format!("${}\r\n{}\r\n", part.len(), part));
            }
        }
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (read, mut write) = socket.into_split();
            let mut reader = BufReader::new(read);
            for field in ["LIB-NAME", "LIB-VER"] {
                let command = read_command(&mut reader).await;
                assert_eq!(command.len(), 4);
                assert_eq!(&command[..3], &["CLIENT", "SETINFO", field]);
                write.write_all(b"+OK\r\n").await.unwrap();
            }
            assert_eq!(
                read_command(&mut reader).await,
                ["CONFIG", "GET", &expected]
            );
            write.write_all(reply.as_bytes()).await.unwrap();
            let mut byte = [0];
            match reader.read(&mut byte).await {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) => {}
                other => panic!("unexpected extra command or close error: {other:?}"),
            }
        });
        // No default config, real credentials or live client lookup.
        let mut state = AppState::without_config_for_tests(AppState::test_policy());
        state.database_url = Some(format!("redis://{addr}"));
        let state = Arc::new(state);
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            super::config_get(state.clone())
                .call(json!({"parameter": parameter}))
                .await
        })
        .await
        .expect("synthetic CONFIG GET must complete");
        drop(state);
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("fixture connection must close")
            .expect("fixture assertions must pass");
        assert!(
            !result.is_error,
            "synthetic tool read must succeed: {result:?}"
        );
        result
    }

    #[tokio::test]
    async fn read_response_redaction_covers_direct_and_wildcard_config_get() {
        let sensitive = [
            "requirepass",
            "masterauth",
            "tls-key-file-pass",
            "tls-client-key-file-pass",
        ];
        for name in sensitive {
            let result = call_config_get(name, &[(name, "synthetic-config-marker")]).await;
            assert!(
                !serde_json::to_string(&result)
                    .unwrap()
                    .contains("synthetic-config-marker")
            );
            assert_eq!(
                result.content[0].as_text(),
                Some(format!("Configuration (1 parameter(s)):\n{name}: [REDACTED]").as_str())
            );
        }
        let result = call_config_get(
            "*",
            &[
                ("masterauth", "synthetic-master-marker"),
                ("requirepass", "synthetic-require-marker"),
                ("tls-key-file-pass", "synthetic-tls-marker"),
                ("TLS-CLIENT-KEY-FILE-PASS", "synthetic-client-marker"),
                ("maxmemory", "1024"),
                ("masteruser", "public-user"),
            ],
        )
        .await;
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("synthetic-")
        );
        assert_eq!(
            result.content[0].as_text(),
            Some(concat!(
                "Configuration (6 parameter(s)):\nmasterauth: [REDACTED]\nrequirepass: [REDACTED]\n",
                "tls-key-file-pass: [REDACTED]\nTLS-CLIENT-KEY-FILE-PASS: [REDACTED]\n",
                "maxmemory: 1024\nmasteruser: public-user"
            ))
        );
    }

    #[tokio::test]
    async fn read_response_redaction_preserves_empty_config_values_and_no_matches() {
        let result =
            call_config_get("*pass*", &[("requirepass", ""), ("tls-key-file-pass", "")]).await;
        assert_eq!(
            result.content[0].as_text(),
            Some("Configuration (2 parameter(s)):\nrequirepass: \ntls-key-file-pass: ")
        );
        let result = call_config_get("synthetic-no-match", &[]).await;
        assert_eq!(
            result.content[0].as_text(),
            Some("No configuration parameters matching 'synthetic-no-match'")
        );
    }
}
