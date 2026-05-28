//! Composed Redis diagnostic tools (health_check, key_summary, hotkeys, connection_summary)

use std::collections::HashMap;

use tower_mcp::{CallToolResult, ResultExt};

use crate::serde_helpers;
use crate::tools::macros::{database_tool, mcp_module};

mcp_module! {
    health_check => "redis_health_check",
    key_summary => "redis_key_summary",
    hotkeys => "redis_hotkeys",
    connection_summary => "redis_connection_summary",
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a Redis INFO response into a key-value map.
///
/// The INFO format has `# Section` headers and `key:value` lines.
fn parse_info(info: &str) -> HashMap<String, String> {
    info.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(2, ':');
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect()
}

/// Parse a Redis CLIENT LIST response into a vector of field maps.
///
/// Each line contains space-separated `key=value` pairs.
fn parse_client_list(clients: &str) -> Vec<HashMap<String, String>> {
    clients
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.split_whitespace()
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .collect()
        })
        .collect()
}

/// Format a byte count into a human-readable string.
fn format_bytes(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} bytes", bytes)
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

database_tool!(read_only, health_check, "redis_health_check",
    "Comprehensive health check combining PING, INFO, and DBSIZE into a single summary \
     covering connectivity, version, uptime, memory, ops rate, and key count.",
    {} => |conn, _input| {
        // PING
        let ping_response: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .tool_context("PING failed")?;

        // INFO (server + memory + stats combined)
        let info_text: String = redis::cmd("INFO")
            .query_async(&mut conn)
            .await
            .tool_context("INFO failed")?;

        let info = parse_info(&info_text);

        // DBSIZE
        let db_size: i64 = redis::cmd("DBSIZE")
            .query_async(&mut conn)
            .await
            .tool_context("DBSIZE failed")?;

        // Extract fields with fallbacks
        let version = info
            .get("redis_version")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let uptime_seconds = info
            .get("uptime_in_seconds")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let uptime_days = info
            .get("uptime_in_days")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let used_memory_human = info
            .get("used_memory_human")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let maxmemory = info
            .get("maxmemory")
            .cloned()
            .unwrap_or_else(|| "0".to_string());
        let maxmemory_human = info
            .get("maxmemory_human")
            .cloned()
            .unwrap_or_else(|| "unlimited".to_string());
        let frag_ratio = info
            .get("mem_fragmentation_ratio")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let ops_per_sec = info
            .get("instantaneous_ops_per_sec")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let total_commands = info
            .get("total_commands_processed")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let connected_clients = info
            .get("connected_clients")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let maxmemory_display = if maxmemory == "0" {
            "unlimited".to_string()
        } else {
            maxmemory_human
        };

        let output = format!(
            "Redis Health Check\n\
             ==================\n\
             \n\
             Connectivity: {}\n\
             Version: {}\n\
             Uptime: {} seconds ({} days)\n\
             \n\
             Memory:\n\
             - Used: {}\n\
             - Max: {}\n\
             - Fragmentation ratio: {}\n\
             \n\
             Stats:\n\
             - Ops/sec: {}\n\
             - Total commands processed: {}\n\
             - Connected clients: {}\n\
             \n\
             Keys: {}",
            ping_response,
            version,
            uptime_seconds,
            uptime_days,
            used_memory_human,
            maxmemory_display,
            frag_ratio,
            ops_per_sec,
            total_commands,
            connected_clients,
            db_size,
        );

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, key_summary, "redis_key_summary",
    "Get metadata summary for a key combining TYPE, TTL, MEMORY USAGE, and OBJECT ENCODING.",
    {
        /// Key to inspect
        pub key: String,
    } => |conn, input| {
        // TYPE
        let key_type: String = redis::cmd("TYPE")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("TYPE failed")?;

        if key_type == "none" {
            return Ok(CallToolResult::text(format!(
                "Key '{}' does not exist",
                input.key
            )));
        }

        // TTL
        let ttl: i64 = redis::cmd("TTL")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("TTL failed")?;

        let ttl_display = match ttl {
            -2 => "key does not exist".to_string(),
            -1 => "no expiry".to_string(),
            _ => format!("{} seconds", ttl),
        };

        // MEMORY USAGE (may fail for some key types or Redis versions)
        let memory_display = match redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(&input.key)
            .query_async::<Option<i64>>(&mut conn)
            .await
        {
            Ok(Some(bytes)) => format_bytes(bytes),
            Ok(None) => "unknown".to_string(),
            Err(_) => "unavailable".to_string(),
        };

        // OBJECT ENCODING (may fail for some key types)
        let encoding_display = match redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(&input.key)
            .query_async::<Option<String>>(&mut conn)
            .await
        {
            Ok(Some(enc)) => enc,
            Ok(None) => "unknown".to_string(),
            Err(_) => "unavailable".to_string(),
        };

        let output = format!(
            "Key Summary: {}\n\
             =============={}\n\
             \n\
             Type: {}\n\
             TTL: {}\n\
             Memory: {}\n\
             Encoding: {}",
            input.key,
            "=".repeat(input.key.len()),
            key_type,
            ttl_display,
            memory_display,
            encoding_display,
        );

        Ok(CallToolResult::text(output))
    }
);

/// Maximum allowed sample size to prevent runaway scans.
const MAX_SAMPLE_SIZE: usize = 10_000;

/// Number of top keys to return by memory usage.
const TOP_N: usize = 20;

database_tool!(read_only, hotkeys, "redis_hotkeys",
    "Sample keys to find the largest by memory and show type distribution. \
     Capped at sample_size (default 1000, max 10000) to limit impact.",
    {
        /// Key pattern to match (default: "*")
        #[serde(default)]
        pub pattern: Option<String>,
        /// Maximum number of keys to sample (default: 1000, max: 10000)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_usize::deserialize")]
        pub sample_size: Option<usize>,
    } => |conn, input| {
        let pattern = input.pattern.as_deref().unwrap_or("*");
        let sample_size = input.sample_size.unwrap_or(1000).min(MAX_SAMPLE_SIZE);

        // SCAN to collect keys
        let mut cursor: u64 = 0;
        let mut scanned_keys: Vec<String> = Vec::new();

        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .tool_context("SCAN failed")?;

            scanned_keys.extend(keys);
            cursor = new_cursor;

            if cursor == 0 || scanned_keys.len() >= sample_size {
                break;
            }
        }

        scanned_keys.truncate(sample_size);

        if scanned_keys.is_empty() {
            return Ok(CallToolResult::text(format!(
                "No keys found matching pattern '{}'",
                pattern
            )));
        }

        // Collect TYPE and MEMORY USAGE for each key
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        let mut key_sizes: Vec<(String, i64, String)> = Vec::new();
        let mut total_memory: i64 = 0;

        for key in &scanned_keys {
            // TYPE
            let key_type: String =
                match redis::cmd("TYPE").arg(key).query_async(&mut conn).await {
                    Ok(t) => t,
                    Err(_) => continue,
                };

            *type_counts.entry(key_type.clone()).or_insert(0) += 1;

            // MEMORY USAGE -- may return None or fail
            let mem_bytes: Option<i64> = redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(key)
                .query_async(&mut conn)
                .await
                .unwrap_or_default();

            if let Some(bytes) = mem_bytes {
                total_memory += bytes;
                key_sizes.push((key.clone(), bytes, key_type));
            }
        }

        // Sort by memory descending and take top N
        key_sizes.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        key_sizes.truncate(TOP_N);

        // Build output
        let mut output = format!(
            "Redis Hotkeys Analysis\n\
             ======================\n\
             \n\
             Keys scanned: {}\n\
             Total memory sampled: {}\n\
             \n\
             Type Distribution:\n",
            scanned_keys.len(),
            format_bytes(total_memory),
        );

        let mut type_list: Vec<_> = type_counts.iter().collect();
        type_list.sort_by(|a, b| b.1.cmp(a.1));
        for (t, count) in &type_list {
            output.push_str(&format!("  {}: {}\n", t, count));
        }

        output.push_str(&format!(
            "\nTop {} Keys by Memory:\n",
            key_sizes.len().min(TOP_N)
        ));

        for (i, (key, bytes, key_type)) in key_sizes.iter().enumerate() {
            output.push_str(&format!(
                "  {}. {} ({}) - {}\n",
                i + 1,
                key,
                key_type,
                format_bytes(*bytes),
            ));
        }

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, connection_summary, "redis_connection_summary",
    "Analyze client connections: totals, top IPs, idle/blocked counts, and oldest connection.",
    {} => |conn, _input| {
        // CLIENT LIST
        let client_list_raw: String = redis::cmd("CLIENT")
            .arg("LIST")
            .query_async(&mut conn)
            .await
            .tool_context("CLIENT LIST failed")?;

        // INFO clients section
        let info_text: String = redis::cmd("INFO")
            .arg("clients")
            .query_async(&mut conn)
            .await
            .tool_context("INFO clients failed")?;

        let info = parse_info(&info_text);
        let clients = parse_client_list(&client_list_raw);

        let total = clients.len();

        // Connections by source IP
        let mut ip_counts: HashMap<String, usize> = HashMap::new();
        for c in &clients {
            if let Some(addr) = c.get("addr") {
                // addr is "ip:port" -- extract just IP
                let ip = addr
                    .rsplit_once(':')
                    .map(|(ip, _)| ip.to_string())
                    .unwrap_or_else(|| addr.clone());
                *ip_counts.entry(ip).or_insert(0) += 1;
            }
        }
        let mut ip_list: Vec<_> = ip_counts.into_iter().collect();
        ip_list.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        ip_list.truncate(10);

        // Idle connections (idle > 60s)
        let idle_count = clients
            .iter()
            .filter(|c| {
                c.get("idle")
                    .and_then(|v| v.parse::<u64>().ok())
                    .is_some_and(|idle| idle > 60)
            })
            .count();

        // Blocked clients from INFO
        let blocked_clients = info
            .get("blocked_clients")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        // Oldest connection age
        let oldest_age = clients
            .iter()
            .filter_map(|c| c.get("age").and_then(|v| v.parse::<u64>().ok()))
            .max();

        let oldest_display = match oldest_age {
            Some(age) => {
                let days = age / 86400;
                let hours = (age % 86400) / 3600;
                let minutes = (age % 3600) / 60;
                let secs = age % 60;
                if days > 0 {
                    format!("{}d {}h {}m {}s ({} seconds)", days, hours, minutes, secs, age)
                } else if hours > 0 {
                    format!("{}h {}m {}s ({} seconds)", hours, minutes, secs, age)
                } else if minutes > 0 {
                    format!("{}m {}s ({} seconds)", minutes, secs, age)
                } else {
                    format!("{} seconds", age)
                }
            }
            None => "unknown".to_string(),
        };

        // Build output
        let mut output = format!(
            "Redis Connection Summary\n\
             ========================\n\
             \n\
             Total connections: {}\n\
             Blocked clients: {}\n\
             Idle connections (>60s): {}\n\
             Oldest connection: {}\n\
             \n\
             Connections by IP (top 10):\n",
            total, blocked_clients, idle_count, oldest_display,
        );

        for (ip, count) in &ip_list {
            output.push_str(&format!("  {}: {}\n", ip, count));
        }

        Ok(CallToolResult::text(output))
    }
);
