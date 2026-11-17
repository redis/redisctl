//! Data structure Redis tools (hgetall, lrange, smembers, zrange, xinfo_stream, xrange, xlen,
//! pubsub_channels, pubsub_numsub, hset, hdel, lpush, rpush, lpop, rpop, sadd, srem, zadd,
//! zrem, xadd, xtrim, hget, hmget, hlen, hexists, hkeys, hvals, hincrby, hexpire, scard,
//! sismember, sunion, sinter, sdiff, zcard, zscore, zrank, zcount, zrangebyscore,
//! zremrangebyscore, llen, lindex, lrem, lset, ltrim, xinfo_groups, xinfo_consumers, xpending,
//! xgroup_create, xreadgroup, xack, xdel, xgroup_destroy, publish)

use std::collections::HashMap;

use tower_mcp::CallToolResult;

use super::RedisResultExt;

use crate::serde_helpers;
use crate::tools::macros::{database_tool, mcp_module};

/// A score-member pair for sorted set operations
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoreMember {
    /// Score value
    #[serde(deserialize_with = "serde_helpers::string_or_f64::deserialize")]
    pub score: f64,
    /// Member value
    pub member: String,
}

fn default_stop() -> i64 {
    -1
}

fn default_xrange_start() -> String {
    "-".to_string()
}

fn default_xrange_end() -> String {
    "+".to_string()
}

fn default_xgroup_create_id() -> String {
    "$".to_string()
}

fn default_xreadgroup_id() -> String {
    ">".to_string()
}

/// Default page size for `redis_xreadgroup` when `count` is omitted.
const DEFAULT_XREADGROUP_COUNT: usize = 10;

/// Key/value pairs from a RESP3 map or a RESP2 flat `[k, v, k, v, ...]` array.
fn field_pairs(value: &redis::Value) -> Option<Vec<(String, &redis::Value)>> {
    match value {
        redis::Value::Map(pairs) => Some(
            pairs
                .iter()
                .map(|(k, v)| (super::format_value(k), v))
                .collect(),
        ),
        redis::Value::Array(items) if items.len() % 2 == 0 => Some(
            items
                .chunks(2)
                .map(|pair| (super::format_value(&pair[0]), &pair[1]))
                .collect(),
        ),
        _ => None,
    }
}

/// One `k=v` line per XINFO GROUPS / XINFO CONSUMERS record.
fn format_info_records(records: &redis::Value) -> Vec<String> {
    let items: &[redis::Value] = match records {
        redis::Value::Array(items) | redis::Value::Set(items) => items,
        _ => return vec![super::format_value(records)],
    };
    items
        .iter()
        .map(|record| match field_pairs(record) {
            Some(pairs) => pairs
                .iter()
                .map(|(k, v)| format!("{}={}", k, super::format_value(v)))
                .collect::<Vec<_>>()
                .join(" "),
            None => super::format_value(record),
        })
        .collect()
}

/// Format one stream entry `[id, [field, value, ...]]`. A nil field list means the
/// entry was deleted after delivery but is still in the pending entries list.
fn format_stream_entry(entry: &redis::Value) -> String {
    let redis::Value::Array(parts) = entry else {
        return super::format_value(entry);
    };
    let (Some(id), Some(fields)) = (parts.first(), parts.get(1)) else {
        return super::format_value(entry);
    };
    let id = super::format_value(id);
    if matches!(fields, redis::Value::Nil) {
        return format!("{id}  (deleted)");
    }
    match field_pairs(fields) {
        Some(pairs) => format!(
            "{id}  {}",
            pairs
                .iter()
                .map(|(k, v)| format!("{}={}", k, super::format_value(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => format!("{id}  {}", super::format_value(fields)),
    }
}

/// Stream batches from an XREADGROUP reply: RESP2 `[[stream, entries], ...]` or a
/// RESP3 map of stream to entries. Nil (nothing to read) yields no batches.
fn stream_batches(reply: &redis::Value) -> Vec<(String, Vec<&redis::Value>)> {
    fn entries_of(value: &redis::Value) -> Vec<&redis::Value> {
        match value {
            redis::Value::Array(entries) => entries.iter().collect(),
            _ => Vec::new(),
        }
    }
    match reply {
        redis::Value::Map(pairs) => pairs
            .iter()
            .map(|(name, entries)| (super::format_value(name), entries_of(entries)))
            .collect(),
        redis::Value::Array(batches) => batches
            .iter()
            .filter_map(|batch| match batch {
                redis::Value::Array(parts) if parts.len() == 2 => {
                    Some((super::format_value(&parts[0]), entries_of(&parts[1])))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

mcp_module! {
    hgetall => "redis_hgetall",
    lrange => "redis_lrange",
    smembers => "redis_smembers",
    zrange => "redis_zrange",
    xinfo_stream => "redis_xinfo_stream",
    xrange => "redis_xrange",
    xlen => "redis_xlen",
    pubsub_channels => "redis_pubsub_channels",
    pubsub_numsub => "redis_pubsub_numsub",
    hset => "redis_hset",
    hdel => "redis_hdel",
    lpush => "redis_lpush",
    rpush => "redis_rpush",
    lpop => "redis_lpop",
    rpop => "redis_rpop",
    sadd => "redis_sadd",
    srem => "redis_srem",
    zadd => "redis_zadd",
    zrem => "redis_zrem",
    xadd => "redis_xadd",
    xtrim => "redis_xtrim",
    hget => "redis_hget",
    hmget => "redis_hmget",
    hlen => "redis_hlen",
    hexists => "redis_hexists",
    hkeys => "redis_hkeys",
    hvals => "redis_hvals",
    hincrby => "redis_hincrby",
    hexpire => "redis_hexpire",
    scard => "redis_scard",
    sismember => "redis_sismember",
    sunion => "redis_sunion",
    sinter => "redis_sinter",
    sdiff => "redis_sdiff",
    zcard => "redis_zcard",
    zscore => "redis_zscore",
    zrank => "redis_zrank",
    zcount => "redis_zcount",
    zrangebyscore => "redis_zrangebyscore",
    zremrangebyscore => "redis_zremrangebyscore",
    llen => "redis_llen",
    lindex => "redis_lindex",
    lrem => "redis_lrem",
    lset => "redis_lset",
    ltrim => "redis_ltrim",
    xinfo_groups => "redis_xinfo_groups",
    xinfo_consumers => "redis_xinfo_consumers",
    xpending => "redis_xpending",
    xgroup_create => "redis_xgroup_create",
    xreadgroup => "redis_xreadgroup",
    xack => "redis_xack",
    xdel => "redis_xdel",
    xgroup_destroy => "redis_xgroup_destroy",
    publish => "redis_publish",
}

// --- Read tools ---

database_tool!(read_only, hgetall, "redis_hgetall",
    "Get all fields and values of a hash.",
    {
        /// Hash key to get
        pub key: String,
    } => |conn, input| {
        let result: Vec<(String, String)> = redis::cmd("HGETALL")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("HGETALL failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text(format!(
                "(empty hash or key '{}' not found)",
                input.key
            )));
        }

        let output = result
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult::text(format!(
            "Hash '{}' ({} fields):\n{}",
            input.key,
            result.len(),
            output
        )))
    }
);

database_tool!(read_only, lrange, "redis_lrange",
    "Get a range of elements from a list (start=0, stop=-1 for all).",
    {
        /// List key
        pub key: String,
        /// Start index (0-based)
        #[serde(default, deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub start: i64,
        /// Stop index (-1 for all)
        #[serde(default = "default_stop", deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub stop: i64,
    } => |conn, input| {
        let result: Vec<String> = redis::cmd("LRANGE")
            .arg(&input.key)
            .arg(input.start)
            .arg(input.stop)
            .query_async(&mut conn)
            .await
            .tool_context("LRANGE failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text(format!(
                "No list elements returned for '{}' in range [{}, {}]",
                input.key, input.start, input.stop
            )));
        }

        let output = result
            .iter()
            .enumerate()
            .map(|(i, v)| format!("{}: {}", i, v))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult::text(format!(
            "List '{}' ({} elements):\n{}",
            input.key,
            result.len(),
            output
        )))
    }
);

database_tool!(read_only, smembers, "redis_smembers",
    "Get all members of a set.",
    {
        /// Set key
        pub key: String,
    } => |conn, input| {
        let result: Vec<String> = redis::cmd("SMEMBERS")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("SMEMBERS failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text(format!(
                "(empty set or key '{}' not found)",
                input.key
            )));
        }

        Ok(CallToolResult::text(format!(
            "Set '{}' ({} members):\n{}",
            input.key,
            result.len(),
            result.join("\n")
        )))
    }
);

database_tool!(read_only, zrange, "redis_zrange",
    "Get a range of members from a sorted set by index.",
    {
        /// Sorted set key
        pub key: String,
        /// Start index (0-based)
        #[serde(default, deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub start: i64,
        /// Stop index (-1 for all)
        #[serde(default = "default_stop", deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub stop: i64,
        /// Include scores in output
        #[serde(default)]
        pub withscores: bool,
        /// Reverse the order (highest to lowest)
        #[serde(default)]
        pub rev: bool,
    } => |conn, input| {
        if input.withscores {
            let mut cmd = redis::cmd("ZRANGE");
            cmd.arg(&input.key)
                .arg(input.start)
                .arg(input.stop);
            if input.rev {
                cmd.arg("REV");
            }
            cmd.arg("WITHSCORES");
            let result: Vec<(String, f64)> = cmd
                .query_async(&mut conn)
                .await
                .tool_context("ZRANGE failed")?;

            if result.is_empty() {
                return Ok(CallToolResult::text(format!(
                    "(empty sorted set or key '{}' not found)",
                    input.key
                )));
            }

            let output = result
                .iter()
                .enumerate()
                .map(|(i, (member, score))| format!("{}: {} (score: {})", i, member, score))
                .collect::<Vec<_>>()
                .join("\n");

            Ok(CallToolResult::text(format!(
                "Sorted set '{}' ({} members):\n{}",
                input.key,
                result.len(),
                output
            )))
        } else {
            let mut cmd = redis::cmd("ZRANGE");
            cmd.arg(&input.key)
                .arg(input.start)
                .arg(input.stop);
            if input.rev {
                cmd.arg("REV");
            }
            let result: Vec<String> = cmd
                .query_async(&mut conn)
                .await
                .tool_context("ZRANGE failed")?;

            if result.is_empty() {
                return Ok(CallToolResult::text(format!(
                    "(empty sorted set or key '{}' not found)",
                    input.key
                )));
            }

            let output = result
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{}: {}", i, v))
                .collect::<Vec<_>>()
                .join("\n");

            Ok(CallToolResult::text(format!(
                "Sorted set '{}' ({} members):\n{}",
                input.key,
                result.len(),
                output
            )))
        }
    }
);

database_tool!(read_only, xinfo_stream, "redis_xinfo_stream",
    "Get stream metadata including length, consumer groups, and entry details (XINFO STREAM).",
    {
        /// Stream key to inspect
        pub key: String,
    } => |conn, input| {
        let result: redis::Value = redis::cmd("XINFO")
            .arg("STREAM")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("XINFO STREAM failed")?;

        Ok(CallToolResult::text(format!(
            "Stream '{}':\n{}",
            input.key,
            super::format_value(&result)
        )))
    }
);

database_tool!(read_only, xrange, "redis_xrange",
    "Get stream entries in a range. Use \"-\" to \"+\" for all entries.",
    {
        /// Stream key
        pub key: String,
        /// Start ID (default: "-" for beginning)
        #[serde(default = "default_xrange_start")]
        pub start: String,
        /// End ID (default: "+" for end)
        #[serde(default = "default_xrange_end")]
        pub end: String,
        /// Maximum number of entries to return
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_usize::deserialize")]
        pub count: Option<usize>,
    } => |conn, input| {
        let mut cmd = redis::cmd("XRANGE");
        cmd.arg(&input.key).arg(&input.start).arg(&input.end);

        if let Some(count) = input.count {
            // XRANGE COUNT 0 returns nothing, reported as "No stream entries returned".
            super::require_positive_param("count", count).map_err(tower_mcp::Error::tool)?;
            cmd.arg("COUNT").arg(count);
        }

        let result: redis::Value = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XRANGE failed")?;

        // Format stream entries
        let formatted = match &result {
            redis::Value::Array(entries) if entries.is_empty() => {
                format!(
                    "No stream entries returned for '{}' in range [{}, {}]",
                    input.key, input.start, input.end
                )
            }
            redis::Value::Array(entries) => {
                let mut output =
                    format!("Stream '{}' ({} entries):\n", input.key, entries.len());
                for entry in entries {
                    output.push_str(&super::format_value(entry));
                    output.push('\n');
                }
                output
            }
            _ => super::format_value(&result),
        };

        Ok(CallToolResult::text(formatted))
    }
);

database_tool!(read_only, xlen, "redis_xlen",
    "Get the number of entries in a stream.",
    {
        /// Stream key
        pub key: String,
    } => |conn, input| {
        let len: i64 = redis::cmd("XLEN")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("XLEN failed")?;

        Ok(CallToolResult::text(format!(
            "Stream '{}': {} entries",
            input.key, len
        )))
    }
);

database_tool!(read_only, pubsub_channels, "redis_pubsub_channels",
    "List active pub/sub channels, optionally filtered by pattern.",
    {
        /// Optional glob-style pattern to filter channels
        #[serde(default)]
        pub pattern: Option<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("PUBSUB");
        cmd.arg("CHANNELS");

        if let Some(ref pattern) = input.pattern {
            cmd.arg(pattern);
        }

        let channels: Vec<String> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("PUBSUB CHANNELS failed")?;

        if channels.is_empty() {
            return Ok(CallToolResult::text("No active pub/sub channels"));
        }

        Ok(CallToolResult::text(format!(
            "Active channels ({}):\n{}",
            channels.len(),
            channels.join("\n")
        )))
    }
);

database_tool!(read_only, pubsub_numsub, "redis_pubsub_numsub",
    "Get subscriber counts for pub/sub channels.",
    {
        /// Channel names to get subscriber counts for (omit for all)
        #[serde(default)]
        pub channels: Option<Vec<String>>,
    } => |conn, input| {
        let mut cmd = redis::cmd("PUBSUB");
        cmd.arg("NUMSUB");

        if let Some(ref channels) = input.channels {
            for channel in channels {
                cmd.arg(channel);
            }
        }

        // PUBSUB NUMSUB returns alternating channel name + count
        let result: Vec<redis::Value> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("PUBSUB NUMSUB failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text("No subscriber information available"));
        }

        let mut output = String::from("Channel subscriber counts:\n");
        for pair in result.chunks(2) {
            if pair.len() == 2 {
                let channel = super::format_value(&pair[0]);
                let count = super::format_value(&pair[1]);
                output.push_str(&format!("  {}: {} subscribers\n", channel, count));
            }
        }

        Ok(CallToolResult::text(output))
    }
);

// --- P1 Hash read tools ---

database_tool!(read_only, hget, "redis_hget",
    "Get the value of a single field in a hash.",
    {
        /// Hash key
        pub key: String,
        /// Field to get
        pub field: String,
    } => |conn, input| {
        let value: Option<String> = redis::cmd("HGET")
            .arg(&input.key)
            .arg(&input.field)
            .query_async(&mut conn)
            .await
            .tool_context("HGET failed")?;

        match value {
            Some(v) => Ok(CallToolResult::text(v)),
            None => Ok(CallToolResult::text(format!(
                "(nil) - field '{}' not found in '{}'",
                input.field, input.key
            ))),
        }
    }
);

database_tool!(read_only, hmget, "redis_hmget",
    "Get the values of multiple fields in a hash.",
    {
        /// Hash key
        pub key: String,
        /// Fields to get
        pub fields: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("HMGET");
        cmd.arg(&input.key);
        for field in &input.fields {
            cmd.arg(field);
        }

        let values: Vec<redis::Value> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("HMGET failed")?;

        let output = input
            .fields
            .iter()
            .zip(values.iter())
            .map(|(f, v)| format!("{}: {}", f, super::format_value(v)))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, hlen, "redis_hlen",
    "Get the number of fields in a hash.",
    {
        /// Hash key
        pub key: String,
    } => |conn, input| {
        let count: i64 = redis::cmd("HLEN")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("HLEN failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {} fields",
            input.key, count
        )))
    }
);

database_tool!(read_only, hexists, "redis_hexists",
    "Check if a field exists in a hash.",
    {
        /// Hash key
        pub key: String,
        /// Field to check
        pub field: String,
    } => |conn, input| {
        let exists: bool = redis::cmd("HEXISTS")
            .arg(&input.key)
            .arg(&input.field)
            .query_async(&mut conn)
            .await
            .tool_context("HEXISTS failed")?;

        Ok(CallToolResult::text(format!(
            "{}.{}: {}",
            input.key,
            input.field,
            if exists { "exists" } else { "does not exist" }
        )))
    }
);

database_tool!(read_only, hkeys, "redis_hkeys",
    "Get all field names in a hash.",
    {
        /// Hash key
        pub key: String,
    } => |conn, input| {
        let fields: Vec<String> = redis::cmd("HKEYS")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("HKEYS failed")?;

        if fields.is_empty() {
            return Ok(CallToolResult::text(format!(
                "(empty hash or key '{}' not found)",
                input.key
            )));
        }

        Ok(CallToolResult::text(format!(
            "Hash '{}' ({} fields):\n{}",
            input.key,
            fields.len(),
            fields.join("\n")
        )))
    }
);

database_tool!(read_only, hvals, "redis_hvals",
    "Get all values in a hash.",
    {
        /// Hash key
        pub key: String,
    } => |conn, input| {
        let values: Vec<String> = redis::cmd("HVALS")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("HVALS failed")?;

        if values.is_empty() {
            return Ok(CallToolResult::text(format!(
                "(empty hash or key '{}' not found)",
                input.key
            )));
        }

        Ok(CallToolResult::text(format!(
            "Hash '{}' ({} values):\n{}",
            input.key,
            values.len(),
            values.join("\n")
        )))
    }
);

// --- P1 Set read tools ---

database_tool!(read_only, scard, "redis_scard",
    "Get the number of members in a set (cardinality).",
    {
        /// Set key
        pub key: String,
    } => |conn, input| {
        let count: i64 = redis::cmd("SCARD")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("SCARD failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {} members",
            input.key, count
        )))
    }
);

database_tool!(read_only, sismember, "redis_sismember",
    "Check if a value is a member of a set.",
    {
        /// Set key
        pub key: String,
        /// Member to check
        pub member: String,
    } => |conn, input| {
        let is_member: bool = redis::cmd("SISMEMBER")
            .arg(&input.key)
            .arg(&input.member)
            .query_async(&mut conn)
            .await
            .tool_context("SISMEMBER failed")?;

        Ok(CallToolResult::text(format!(
            "'{}' {} a member of '{}'",
            input.member,
            if is_member { "is" } else { "is not" },
            input.key
        )))
    }
);

database_tool!(read_only, sunion, "redis_sunion",
    "Return the union of multiple sets.",
    {
        /// Set keys to compute union of
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("SUNION");
        for key in &input.keys {
            cmd.arg(key);
        }

        let members: Vec<String> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("SUNION failed")?;

        if members.is_empty() {
            return Ok(CallToolResult::text("(empty set)"));
        }

        Ok(CallToolResult::text(format!(
            "Union ({} members):\n{}",
            members.len(),
            members.join("\n")
        )))
    }
);

database_tool!(read_only, sinter, "redis_sinter",
    "Return the intersection of multiple sets.",
    {
        /// Set keys to compute intersection of
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("SINTER");
        for key in &input.keys {
            cmd.arg(key);
        }

        let members: Vec<String> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("SINTER failed")?;

        if members.is_empty() {
            return Ok(CallToolResult::text("(empty set)"));
        }

        Ok(CallToolResult::text(format!(
            "Intersection ({} members):\n{}",
            members.len(),
            members.join("\n")
        )))
    }
);

database_tool!(read_only, sdiff, "redis_sdiff",
    "Return the difference between the first set and all subsequent sets.",
    {
        /// Set keys (first set minus all subsequent sets)
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("SDIFF");
        for key in &input.keys {
            cmd.arg(key);
        }

        let members: Vec<String> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("SDIFF failed")?;

        if members.is_empty() {
            return Ok(CallToolResult::text("(empty set)"));
        }

        Ok(CallToolResult::text(format!(
            "Difference ({} members):\n{}",
            members.len(),
            members.join("\n")
        )))
    }
);

// --- P1 Sorted Set read tools ---

database_tool!(read_only, zcard, "redis_zcard",
    "Get the number of members in a sorted set (cardinality).",
    {
        /// Sorted set key
        pub key: String,
    } => |conn, input| {
        let count: i64 = redis::cmd("ZCARD")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("ZCARD failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {} members",
            input.key, count
        )))
    }
);

database_tool!(read_only, zscore, "redis_zscore",
    "Get the score of a member in a sorted set.",
    {
        /// Sorted set key
        pub key: String,
        /// Member to get score for
        pub member: String,
    } => |conn, input| {
        let score: Option<f64> = redis::cmd("ZSCORE")
            .arg(&input.key)
            .arg(&input.member)
            .query_async(&mut conn)
            .await
            .tool_context("ZSCORE failed")?;

        match score {
            Some(s) => Ok(CallToolResult::text(format!(
                "{}.{}: {}",
                input.key, input.member, s
            ))),
            None => Ok(CallToolResult::text(format!(
                "(nil) - '{}' not found in '{}'",
                input.member, input.key
            ))),
        }
    }
);

database_tool!(read_only, zrank, "redis_zrank",
    "Get the rank (0-based index) of a member in a sorted set, ordered low to high.",
    {
        /// Sorted set key
        pub key: String,
        /// Member to get rank for
        pub member: String,
    } => |conn, input| {
        let rank: Option<i64> = redis::cmd("ZRANK")
            .arg(&input.key)
            .arg(&input.member)
            .query_async(&mut conn)
            .await
            .tool_context("ZRANK failed")?;

        match rank {
            Some(r) => Ok(CallToolResult::text(format!(
                "{}.{}: rank {}",
                input.key, input.member, r
            ))),
            None => Ok(CallToolResult::text(format!(
                "(nil) - '{}' not found in '{}'",
                input.member, input.key
            ))),
        }
    }
);

database_tool!(read_only, zcount, "redis_zcount",
    "Count members in a sorted set with scores between min and max (inclusive). Use \"-inf\"/\"+inf\" for unbounded.",
    {
        /// Sorted set key
        pub key: String,
        /// Minimum score (use "-inf" for no lower bound)
        pub min: String,
        /// Maximum score (use "+inf" for no upper bound)
        pub max: String,
    } => |conn, input| {
        let count: i64 = redis::cmd("ZCOUNT")
            .arg(&input.key)
            .arg(&input.min)
            .arg(&input.max)
            .query_async(&mut conn)
            .await
            .tool_context("ZCOUNT failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {} members in score range [{}, {}]",
            input.key, count, input.min, input.max
        )))
    }
);

database_tool!(read_only, zrangebyscore, "redis_zrangebyscore",
    "Get members from a sorted set with scores in the given range. Use \"-inf\"/\"+inf\" for unbounded.",
    {
        /// Sorted set key
        pub key: String,
        /// Minimum score (use "-inf" for no lower bound)
        pub min: String,
        /// Maximum score (use "+inf" for no upper bound)
        pub max: String,
        /// Include scores in output
        #[serde(default)]
        pub withscores: bool,
        /// Offset for pagination (requires count)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_i64::deserialize")]
        pub offset: Option<i64>,
        /// Maximum number of results (requires offset)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_i64::deserialize")]
        pub count: Option<i64>,
    } => |conn, input| {
        if input.offset.is_some() != input.count.is_some() {
            return Err(tower_mcp::Error::tool(
                "offset and count must be provided together for pagination",
            ));
        }

        let mut cmd = redis::cmd("ZRANGEBYSCORE");
        cmd.arg(&input.key).arg(&input.min).arg(&input.max);

        if input.withscores {
            cmd.arg("WITHSCORES");
        }

        if let (Some(offset), Some(count)) = (input.offset, input.count) {
            cmd.arg("LIMIT").arg(offset).arg(count);
        }

        let empty_prefix = if input.offset.is_some() {
            "No members returned from"
        } else {
            "No members in"
        };

        if input.withscores {
            let result: Vec<(String, f64)> = cmd
                .query_async(&mut conn)
                .await
                .tool_context("ZRANGEBYSCORE failed")?;

            if result.is_empty() {
                return Ok(CallToolResult::text(format!(
                    "{empty_prefix} '{}' with scores in [{}, {}]",
                    input.key, input.min, input.max
                )));
            }

            let output = result
                .iter()
                .map(|(member, score)| format!("{} (score: {})", member, score))
                .collect::<Vec<_>>()
                .join("\n");

            Ok(CallToolResult::text(format!(
                "'{}' ({} members in [{}, {}]):\n{}",
                input.key,
                result.len(),
                input.min,
                input.max,
                output
            )))
        } else {
            let result: Vec<String> = cmd
                .query_async(&mut conn)
                .await
                .tool_context("ZRANGEBYSCORE failed")?;

            if result.is_empty() {
                return Ok(CallToolResult::text(format!(
                    "{empty_prefix} '{}' with scores in [{}, {}]",
                    input.key, input.min, input.max
                )));
            }

            Ok(CallToolResult::text(format!(
                "'{}' ({} members in [{}, {}]):\n{}",
                input.key,
                result.len(),
                input.min,
                input.max,
                result.join("\n")
            )))
        }
    }
);

database_tool!(write, zremrangebyscore, "redis_zremrangebyscore",
    "Remove all members from a sorted set with scores between min and max (inclusive). \
     Use \"-inf\"/\"+inf\" for unbounded. Returns the number of removed members.\n\n\
     Tip: use with timestamp scores to implement sliding-window cleanup \
     (e.g. ZREMRANGEBYSCORE key -inf {now - window}).",
    {
        /// Sorted set key
        pub key: String,
        /// Minimum score (use "-inf" for no lower bound)
        pub min: String,
        /// Maximum score (use "+inf" for no upper bound)
        pub max: String,
    } => |conn, input| {
        let removed: i64 = redis::cmd("ZREMRANGEBYSCORE")
            .arg(&input.key)
            .arg(&input.min)
            .arg(&input.max)
            .query_async(&mut conn)
            .await
            .tool_context("ZREMRANGEBYSCORE failed")?;

        Ok(CallToolResult::text(format!(
            "Removed {} member(s) from '{}' with scores in [{}, {}]",
            removed, input.key, input.min, input.max
        )))
    }
);

// --- P1 List read tools ---

database_tool!(read_only, llen, "redis_llen",
    "Get the length of a list.",
    {
        /// List key
        pub key: String,
    } => |conn, input| {
        let length: i64 = redis::cmd("LLEN")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("LLEN failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {} elements",
            input.key, length
        )))
    }
);

database_tool!(read_only, lindex, "redis_lindex",
    "Get an element from a list by its index (0-based, negative counts from end).",
    {
        /// List key
        pub key: String,
        /// Index (0-based, negative counts from end)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub index: i64,
    } => |conn, input| {
        let value: Option<String> = redis::cmd("LINDEX")
            .arg(&input.key)
            .arg(input.index)
            .query_async(&mut conn)
            .await
            .tool_context("LINDEX failed")?;

        match value {
            Some(v) => Ok(CallToolResult::text(v)),
            None => Ok(CallToolResult::text(format!(
                "(nil) - index {} out of range or key '{}' not found",
                input.index, input.key
            ))),
        }
    }
);

// --- Write tools ---

database_tool!(write, hset, "redis_hset",
    "Set one or more field-value pairs in a hash. Creates the hash if needed.\n\n\
     Architectural note: a single hash with per-field TTL (via HEXPIRE) can replace \
     sorted-set-based membership tracking, eliminating the need for cleanup scanners.",
    {
        /// Hash key
        pub key: String,
        /// Field-value pairs to set
        pub fields: HashMap<String, String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("HSET");
        cmd.arg(&input.key);
        for (field, value) in &input.fields {
            cmd.arg(field).arg(value);
        }

        let added: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("HSET failed")?;

        Ok(CallToolResult::text(format!(
            "OK - {} field(s) added to hash '{}' ({} field(s) set total)",
            added,
            input.key,
            input.fields.len()
        )))
    }
);

database_tool!(write, hdel, "redis_hdel",
    "Delete one or more fields from a hash.",
    {
        /// Hash key
        pub key: String,
        /// Fields to delete
        pub fields: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("HDEL");
        cmd.arg(&input.key);
        for field in &input.fields {
            cmd.arg(field);
        }

        let removed: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("HDEL failed")?;

        Ok(CallToolResult::text(format!(
            "Deleted {} of {} field(s) from hash '{}'",
            removed,
            input.fields.len(),
            input.key
        )))
    }
);

database_tool!(write, lpush, "redis_lpush",
    "Push elements to the head (left) of a list. Creates the list if needed.",
    {
        /// List key
        pub key: String,
        /// Elements to push to the head of the list
        pub elements: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("LPUSH");
        cmd.arg(&input.key);
        for elem in &input.elements {
            cmd.arg(elem);
        }

        let length: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("LPUSH failed")?;

        Ok(CallToolResult::text(format!(
            "OK - pushed {} element(s) to '{}', new length: {}",
            input.elements.len(),
            input.key,
            length
        )))
    }
);

database_tool!(write, rpush, "redis_rpush",
    "Push elements to the tail (right) of a list. Creates the list if needed.",
    {
        /// List key
        pub key: String,
        /// Elements to push to the tail of the list
        pub elements: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("RPUSH");
        cmd.arg(&input.key);
        for elem in &input.elements {
            cmd.arg(elem);
        }

        let length: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("RPUSH failed")?;

        Ok(CallToolResult::text(format!(
            "OK - pushed {} element(s) to '{}', new length: {}",
            input.elements.len(),
            input.key,
            length
        )))
    }
);

database_tool!(write, lpop, "redis_lpop",
    "Pop elements from the head (left) of a list.",
    {
        /// List key
        pub key: String,
        /// Number of elements to pop (default: 1)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub count: Option<u64>,
    } => |conn, input| {
        let mut cmd = redis::cmd("LPOP");
        cmd.arg(&input.key);
        if let Some(count) = input.count {
            cmd.arg(count);
        }

        let result: redis::Value = cmd
            .query_async(&mut conn)
            .await
            .tool_context("LPOP failed")?;

        Ok(CallToolResult::text(format!(
            "LPOP '{}': {}",
            input.key,
            super::format_value(&result)
        )))
    }
);

database_tool!(write, rpop, "redis_rpop",
    "Pop elements from the tail (right) of a list.",
    {
        /// List key
        pub key: String,
        /// Number of elements to pop (default: 1)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub count: Option<u64>,
    } => |conn, input| {
        let mut cmd = redis::cmd("RPOP");
        cmd.arg(&input.key);
        if let Some(count) = input.count {
            cmd.arg(count);
        }

        let result: redis::Value = cmd
            .query_async(&mut conn)
            .await
            .tool_context("RPOP failed")?;

        Ok(CallToolResult::text(format!(
            "RPOP '{}': {}",
            input.key,
            super::format_value(&result)
        )))
    }
);

database_tool!(write, sadd, "redis_sadd",
    "Add one or more members to a set. Creates the set if needed.",
    {
        /// Set key
        pub key: String,
        /// Members to add to the set
        pub members: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("SADD");
        cmd.arg(&input.key);
        for member in &input.members {
            cmd.arg(member);
        }

        let added: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("SADD failed")?;

        Ok(CallToolResult::text(format!(
            "OK - added {} of {} member(s) to set '{}'",
            added,
            input.members.len(),
            input.key
        )))
    }
);

database_tool!(write, srem, "redis_srem",
    "Remove one or more members from a set.",
    {
        /// Set key
        pub key: String,
        /// Members to remove from the set
        pub members: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("SREM");
        cmd.arg(&input.key);
        for member in &input.members {
            cmd.arg(member);
        }

        let removed: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("SREM failed")?;

        Ok(CallToolResult::text(format!(
            "Removed {} of {} member(s) from set '{}'",
            removed,
            input.members.len(),
            input.key
        )))
    }
);

database_tool!(write, zadd, "redis_zadd",
    "Add members with scores to a sorted set. Creates the set if needed. \
     Supports NX, XX, GT, LT, and CH flags.\n\n\
     Architectural note: score-based ordering enables leaderboard, time-series, and priority \
     queue patterns. For Active-Active (CRDB), sorted sets have high CRDT cost — consider \
     hashes or strings (cheap LWW) if cross-region replication is a factor.",
    {
        /// Sorted set key
        pub key: String,
        /// Score-member pairs to add
        pub members: Vec<ScoreMember>,
        /// Only add new elements, do not update existing ones
        #[serde(default)]
        pub nx: bool,
        /// Only update existing elements, do not add new ones
        #[serde(default)]
        pub xx: bool,
        /// Only update elements whose new score is greater than current score
        #[serde(default)]
        pub gt: bool,
        /// Only update elements whose new score is less than current score
        #[serde(default)]
        pub lt: bool,
        /// Return the number of elements changed (added + updated) instead of only added
        #[serde(default)]
        pub ch: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("ZADD");
        cmd.arg(&input.key);

        if input.nx {
            cmd.arg("NX");
        }
        if input.xx {
            cmd.arg("XX");
        }
        if input.gt {
            cmd.arg("GT");
        }
        if input.lt {
            cmd.arg("LT");
        }
        if input.ch {
            cmd.arg("CH");
        }

        for sm in &input.members {
            cmd.arg(sm.score).arg(&sm.member);
        }

        let count: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("ZADD failed")?;

        let verb = if input.ch { "changed" } else { "added" };
        Ok(CallToolResult::text(format!(
            "OK - {} {} member(s) in sorted set '{}'",
            count, verb, input.key
        )))
    }
);

database_tool!(write, zrem, "redis_zrem",
    "Remove one or more members from a sorted set.",
    {
        /// Sorted set key
        pub key: String,
        /// Members to remove
        pub members: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("ZREM");
        cmd.arg(&input.key);
        for member in &input.members {
            cmd.arg(member);
        }

        let removed: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("ZREM failed")?;

        Ok(CallToolResult::text(format!(
            "Removed {} of {} member(s) from sorted set '{}'",
            removed,
            input.members.len(),
            input.key
        )))
    }
);

database_tool!(write, xadd, "redis_xadd",
    "Append an entry to a stream. Supports NOMKSTREAM, MAXLEN, and MINID trimming.",
    {
        /// Stream key
        pub key: String,
        /// Entry ID (default: "*" for auto-generated)
        #[serde(default)]
        pub id: Option<String>,
        /// Field-value pairs for the stream entry
        pub fields: HashMap<String, String>,
        /// Do not create stream if it does not exist
        #[serde(default)]
        pub nomkstream: bool,
        /// Cap stream to a maximum length
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub maxlen: Option<u64>,
        /// Cap stream entries older than this ID
        #[serde(default)]
        pub minid: Option<String>,
        /// Use approximate trimming (~) for better performance
        #[serde(default)]
        pub approximate: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("XADD");
        cmd.arg(&input.key);

        if input.nomkstream {
            cmd.arg("NOMKSTREAM");
        }

        if let Some(maxlen) = input.maxlen {
            cmd.arg("MAXLEN");
            if input.approximate {
                cmd.arg("~");
            }
            cmd.arg(maxlen);
        } else if let Some(ref minid) = input.minid {
            cmd.arg("MINID");
            if input.approximate {
                cmd.arg("~");
            }
            cmd.arg(minid);
        }

        let id = input.id.as_deref().unwrap_or("*");
        cmd.arg(id);

        for (field, value) in &input.fields {
            cmd.arg(field).arg(value);
        }

        // XADD returns nil (not an error) when NOMKSTREAM is set and the stream
        // does not exist.
        let entry_id: Option<String> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XADD failed")?;

        match entry_id {
            Some(entry_id) => Ok(CallToolResult::text(format!(
                "OK - added entry {} to stream '{}'",
                entry_id, input.key
            ))),
            None => Ok(CallToolResult::text(format!(
                "No entry added: stream '{}' does not exist and nomkstream was set",
                input.key
            ))),
        }
    }
);

database_tool!(write, xtrim, "redis_xtrim",
    "Trim a stream by length (MAXLEN) or minimum ID (MINID). \
     Use approximate=true for better performance.",
    {
        /// Stream key
        pub key: String,
        /// Trimming strategy: "MAXLEN" or "MINID"
        pub strategy: String,
        /// Threshold value (count for MAXLEN, ID for MINID)
        pub threshold: String,
        /// Use approximate trimming (~) for better performance
        #[serde(default)]
        pub approximate: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("XTRIM");
        cmd.arg(&input.key);
        cmd.arg(&input.strategy);

        if input.approximate {
            cmd.arg("~");
        }
        cmd.arg(&input.threshold);

        let trimmed: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XTRIM failed")?;

        Ok(CallToolResult::text(format!(
            "OK - trimmed {} entries from stream '{}'",
            trimmed, input.key
        )))
    }
);

database_tool!(write, hincrby, "redis_hincrby",
    "Increment the integer value of a hash field by the given amount.",
    {
        /// Hash key
        pub key: String,
        /// Field to increment
        pub field: String,
        /// Increment value (can be negative)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub increment: i64,
    } => |conn, input| {
        let value: i64 = redis::cmd("HINCRBY")
            .arg(&input.key)
            .arg(&input.field)
            .arg(input.increment)
            .query_async(&mut conn)
            .await
            .tool_context("HINCRBY failed")?;

        Ok(CallToolResult::text(format!(
            "{}.{}: {}",
            input.key, input.field, value
        )))
    }
);

database_tool!(write, hexpire, "redis_hexpire",
    "Set a TTL (in seconds) on one or more hash fields (Redis 7.4+). \
     Each field expires independently; when a field expires it is automatically deleted.\n\n\
     Optional condition:\n\
     - NX: set expiry only if the field has no expiry\n\
     - XX: set expiry only if the field already has an expiry\n\
     - GT: set expiry only if new TTL is greater than current TTL\n\
     - LT: set expiry only if new TTL is less than current TTL\n\n\
     Per-field result codes: 2 = field already expired/deleted, 1 = expiry set, \
     0 = condition not met, -2 = field does not exist.\n\n\
     TTL must be positive; 0 or less is refused because Redis would delete the fields immediately.",
    {
        /// Hash key
        pub key: String,
        /// TTL in seconds (must be positive)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub seconds: i64,
        /// Fields to set TTL on
        pub fields: Vec<String>,
        /// Optional condition: NX, XX, GT, or LT
        #[serde(default)]
        pub condition: Option<String>,
    } => |conn, input| {
        super::require_positive_ttl(
            input.seconds,
            "the fields",
            "Use redis_hdel to delete the fields.",
        )
        .map_err(tower_mcp::Error::tool)?;


        if let Some(ref cond) = input.condition {
            let cond_upper = cond.to_uppercase();
            if !matches!(cond_upper.as_str(), "NX" | "XX" | "GT" | "LT") {
                return Err(tower_mcp::Error::tool(format!(
                    "invalid condition '{}': must be NX, XX, GT, or LT",
                    cond
                )));
            }
        }

        let mut cmd = redis::cmd("HEXPIRE");
        cmd.arg(&input.key).arg(input.seconds);

        if let Some(ref cond) = input.condition {
            cmd.arg(cond.to_uppercase());
        }

        cmd.arg("FIELDS").arg(input.fields.len());
        for field in &input.fields {
            cmd.arg(field);
        }

        let results: Vec<i64> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("HEXPIRE failed")?;

        let summary: Vec<String> = input
            .fields
            .iter()
            .zip(results.iter())
            .map(|(field, &code)| {
                let status = match code {
                    2 => "field already expired/deleted",
                    1 => "expiry set",
                    0 => "condition not met",
                    -2 => "field not found",
                    _ => "unknown result",
                };
                format!("  {}: {}", field, status)
            })
            .collect();

        Ok(CallToolResult::text(format!(
            "HEXPIRE '{}' ({}s) on {} field(s):\n{}",
            input.key,
            input.seconds,
            input.fields.len(),
            summary.join("\n")
        )))
    }
);

// --- List write tools ---

database_tool!(write, lrem, "redis_lrem",
    "Remove occurrences of an element from a list (LREM). count > 0 removes the first \
     count matches from the head, count < 0 removes the last |count| matches from the \
     tail, and count = 0 removes every match.",
    {
        /// List key
        pub key: String,
        /// Occurrences to remove: positive from the head, negative from the tail, 0 for all matches
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub count: i64,
        /// List element to remove
        pub value: String,
    } => |conn, input| {
        let removed: i64 = redis::cmd("LREM")
            .arg(&input.key)
            .arg(input.count)
            .arg(&input.value)
            .query_async(&mut conn)
            .await
            .tool_context("LREM failed")?;

        if removed == 0 {
            return Ok(CallToolResult::text(format!(
                "No elements removed: '{}' has no element equal to '{}' (or the key does not exist)",
                input.key, input.value
            )));
        }

        Ok(CallToolResult::text(format!(
            "OK - removed {} occurrence(s) of '{}' from '{}'",
            removed, input.value, input.key
        )))
    }
);

database_tool!(write, lset, "redis_lset",
    "Replace the list element at an index (LSET). Index is 0-based; negative counts from \
     the tail. Fails when the key does not exist or the index is out of range.",
    {
        /// List key
        pub key: String,
        /// Index to replace (0-based, negative counts from the tail)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub index: i64,
        /// Replacement value
        pub value: String,
    } => |conn, input| {
        let _: () = redis::cmd("LSET")
            .arg(&input.key)
            .arg(input.index)
            .arg(&input.value)
            .query_async(&mut conn)
            .await
            .tool_context("LSET failed")?;

        Ok(CallToolResult::text(format!(
            "OK - set index {} of '{}'",
            input.index, input.key
        )))
    }
);

database_tool!(write, ltrim, "redis_ltrim",
    "Trim a list to the inclusive range [start, stop] (LTRIM). Elements outside the range \
     are removed. Indexes are 0-based; negative counts from the tail. Use start=0, stop=N-1 \
     to keep the first N elements (capped list).",
    {
        /// List key
        pub key: String,
        /// Inclusive start index (negative counts from the tail)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub start: i64,
        /// Inclusive stop index (negative counts from the tail)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub stop: i64,
    } => |conn, input| {
        let _: () = redis::cmd("LTRIM")
            .arg(&input.key)
            .arg(input.start)
            .arg(input.stop)
            .query_async(&mut conn)
            .await
            .tool_context("LTRIM failed")?;

        let length: i64 = redis::cmd("LLEN")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("LLEN after LTRIM failed")?;

        Ok(CallToolResult::text(format!(
            "OK - trimmed '{}' to [{}, {}], length now {}",
            input.key, input.start, input.stop, length
        )))
    }
);

// --- Stream consumer-group tools ---

database_tool!(read_only, xinfo_groups, "redis_xinfo_groups",
    "List the consumer groups of a stream with their consumers, pending count, \
     last-delivered ID, entries read and lag (XINFO GROUPS).",
    {
        /// Stream key
        pub key: String,
    } => |conn, input| {
        let result: redis::Value = redis::cmd("XINFO")
            .arg("GROUPS")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("XINFO GROUPS failed")?;

        let groups = format_info_records(&result);
        if groups.is_empty() {
            return Ok(CallToolResult::text(format!(
                "Stream '{}' has no consumer groups",
                input.key
            )));
        }

        Ok(CallToolResult::text(format!(
            "Stream '{}' consumer groups ({}):\n  {}",
            input.key,
            groups.len(),
            groups.join("\n  ")
        )))
    }
);

database_tool!(read_only, xinfo_consumers, "redis_xinfo_consumers",
    "List the consumers of a stream consumer group with their pending count and idle \
     time in milliseconds (XINFO CONSUMERS).",
    {
        /// Stream key
        pub key: String,
        /// Consumer group name
        pub group: String,
    } => |conn, input| {
        let result: redis::Value = redis::cmd("XINFO")
            .arg("CONSUMERS")
            .arg(&input.key)
            .arg(&input.group)
            .query_async(&mut conn)
            .await
            .tool_context("XINFO CONSUMERS failed")?;

        let consumers = format_info_records(&result);
        if consumers.is_empty() {
            return Ok(CallToolResult::text(format!(
                "Group '{}' on stream '{}' has no consumers",
                input.group, input.key
            )));
        }

        Ok(CallToolResult::text(format!(
            "Group '{}' on stream '{}' consumers ({}):\n  {}",
            input.group,
            input.key,
            consumers.len(),
            consumers.join("\n  ")
        )))
    }
);

database_tool!(read_only, xpending, "redis_xpending",
    "Inspect a consumer group's pending entries list (XPENDING). Without count, returns \
     the summary: total pending, ID range and per-consumer counts. With count, returns up \
     to count entries in [start, end] with consumer, idle time (ms) and delivery count, \
     optionally filtered by consumer and minimum idle time.",
    {
        /// Stream key
        pub key: String,
        /// Consumer group name
        pub group: String,
        /// Start ID for the detailed form (default: "-")
        #[serde(default = "default_xrange_start")]
        pub start: String,
        /// End ID for the detailed form (default: "+")
        #[serde(default = "default_xrange_end")]
        pub end: String,
        /// Number of entries to return; selects the detailed form
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_usize::deserialize")]
        pub count: Option<usize>,
        /// Only entries owned by this consumer (detailed form only)
        #[serde(default)]
        pub consumer: Option<String>,
        /// Only entries idle for at least this many milliseconds (detailed form only)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub min_idle_ms: Option<u64>,
    } => |conn, input| {
        let Some(count) = input.count else {
            if input.consumer.is_some() || input.min_idle_ms.is_some() {
                return Err(tower_mcp::Error::tool(
                    "consumer and min_idle_ms apply to the detailed form; pass count as well",
                ));
            }
            let summary: redis::Value = redis::cmd("XPENDING")
                .arg(&input.key)
                .arg(&input.group)
                .query_async(&mut conn)
                .await
                .tool_context("XPENDING failed")?;

            let redis::Value::Array(parts) = &summary else {
                return Ok(CallToolResult::text(super::format_value(&summary)));
            };
            let total = parts.first().map(super::format_value).unwrap_or_default();
            if total == "0" {
                return Ok(CallToolResult::text(format!(
                    "Group '{}' on stream '{}' has no pending entries",
                    input.group, input.key
                )));
            }
            let smallest = parts.get(1).map(super::format_value).unwrap_or_default();
            let greatest = parts.get(2).map(super::format_value).unwrap_or_default();
            let mut output = format!(
                "Group '{}' on stream '{}': {} pending, IDs {} to {}",
                input.group, input.key, total, smallest, greatest
            );
            if let Some(redis::Value::Array(consumers)) = parts.get(3) {
                for consumer in consumers {
                    if let redis::Value::Array(pair) = consumer
                        && pair.len() == 2
                    {
                        output.push_str(&format!(
                            "\n  {}: {} pending",
                            super::format_value(&pair[0]),
                            super::format_value(&pair[1])
                        ));
                    }
                }
            }
            return Ok(CallToolResult::text(output));
        };

        super::require_positive_param("count", count).map_err(tower_mcp::Error::tool)?;
        let mut cmd = redis::cmd("XPENDING");
        cmd.arg(&input.key).arg(&input.group);
        if let Some(min_idle_ms) = input.min_idle_ms {
            cmd.arg("IDLE").arg(min_idle_ms);
        }
        cmd.arg(&input.start).arg(&input.end).arg(count);
        if let Some(ref consumer) = input.consumer {
            cmd.arg(consumer);
        }

        let entries: Vec<(String, String, u64, u64)> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XPENDING failed")?;

        if entries.is_empty() {
            return Ok(CallToolResult::text(format!(
                "No pending entries in [{}, {}] for group '{}' on stream '{}'",
                input.start, input.end, input.group, input.key
            )));
        }

        let lines = entries
            .iter()
            .map(|(id, consumer, idle_ms, deliveries)| {
                format!(
                    "  {id}  consumer={consumer} idle_ms={idle_ms} deliveries={deliveries}"
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(CallToolResult::text(format!(
            "Group '{}' on stream '{}' pending entries ({}):\n{}",
            input.group,
            input.key,
            entries.len(),
            lines
        )))
    }
);

database_tool!(write, xgroup_create, "redis_xgroup_create",
    "Create a consumer group on a stream (XGROUP CREATE). id sets where the group \
     starts: \"$\" (default) delivers only entries added after creation, \"0\" delivers \
     the whole stream. mkstream creates an empty stream if the key does not exist.",
    {
        /// Stream key
        pub key: String,
        /// Consumer group name
        pub group: String,
        /// Last-delivered ID for the new group: "$" (default) for new entries only, "0" for the full history
        #[serde(default = "default_xgroup_create_id")]
        pub id: String,
        /// Create the stream if it does not exist
        #[serde(default)]
        pub mkstream: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("XGROUP");
        cmd.arg("CREATE").arg(&input.key).arg(&input.group).arg(&input.id);
        if input.mkstream {
            cmd.arg("MKSTREAM");
        }

        let _: () = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XGROUP CREATE failed")?;

        Ok(CallToolResult::text(format!(
            "OK - created group '{}' on stream '{}' starting after ID {}",
            input.group, input.key, input.id
        )))
    }
);

database_tool!(write, xreadgroup, "redis_xreadgroup",
    "Read entries from a stream as a consumer in a consumer group (XREADGROUP, \
     non-blocking). id \">\" (default) reads entries never delivered to this group and \
     adds them to the consumer's pending list; an explicit ID such as \"0\" re-reads this \
     consumer's own pending entries. Acknowledge processed entries with redis_xack. \
     noack skips the pending list.",
    {
        /// Consumer group name
        pub group: String,
        /// Consumer name (created on first read)
        pub consumer: String,
        /// Stream key
        pub key: String,
        /// ">" (default) for new entries, or an ID to re-read this consumer's pending entries after it
        #[serde(default = "default_xreadgroup_id")]
        pub id: String,
        /// Maximum entries to return (default: 10)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_usize::deserialize")]
        pub count: Option<usize>,
        /// Do not add delivered entries to the pending entries list
        #[serde(default)]
        pub noack: bool,
    } => |conn, input| {
        let count = input.count.unwrap_or(DEFAULT_XREADGROUP_COUNT);
        super::require_positive_param("count", count).map_err(tower_mcp::Error::tool)?;

        let mut cmd = redis::cmd("XREADGROUP");
        cmd.arg("GROUP")
            .arg(&input.group)
            .arg(&input.consumer)
            .arg("COUNT")
            .arg(count);
        if input.noack {
            cmd.arg("NOACK");
        }
        cmd.arg("STREAMS").arg(&input.key).arg(&input.id);

        let reply: redis::Value = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XREADGROUP failed")?;

        let entries: Vec<&redis::Value> = stream_batches(&reply)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .collect();
        if entries.is_empty() {
            let what = if input.id == ">" {
                "no new entries"
            } else {
                "no pending entries after that ID"
            };
            return Ok(CallToolResult::text(format!(
                "Consumer '{}' in group '{}' on stream '{}': {}",
                input.consumer, input.group, input.key, what
            )));
        }

        let lines = entries
            .iter()
            .map(|entry| format!("  {}", format_stream_entry(entry)))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(CallToolResult::text(format!(
            "Consumer '{}' in group '{}' read {} entr{} from stream '{}':\n{}",
            input.consumer,
            input.group,
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" },
            input.key,
            lines
        )))
    }
);

database_tool!(write, xack, "redis_xack",
    "Acknowledge processed entries for a consumer group, removing them from its pending \
     entries list (XACK).",
    {
        /// Stream key
        pub key: String,
        /// Consumer group name
        pub group: String,
        /// Entry IDs to acknowledge
        pub ids: Vec<String>,
    } => |conn, input| {
        if input.ids.is_empty() {
            return Err(tower_mcp::Error::tool("ids must contain at least one entry ID"));
        }
        let mut cmd = redis::cmd("XACK");
        cmd.arg(&input.key).arg(&input.group);
        for id in &input.ids {
            cmd.arg(id);
        }

        let acknowledged: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XACK failed")?;

        if acknowledged == 0 {
            return Ok(CallToolResult::text(format!(
                "No entries acknowledged: none of the {} ID(s) are pending for group '{}' on stream '{}'",
                input.ids.len(),
                input.group,
                input.key
            )));
        }

        Ok(CallToolResult::text(format!(
            "OK - acknowledged {} of {} ID(s) for group '{}' on stream '{}'",
            acknowledged,
            input.ids.len(),
            input.group,
            input.key
        )))
    }
);

database_tool!(write, xdel, "redis_xdel",
    "Delete entries from a stream by ID (XDEL). Consumer groups keep deleted IDs in \
     their pending lists until acknowledged.",
    {
        /// Stream key
        pub key: String,
        /// Entry IDs to delete
        pub ids: Vec<String>,
    } => |conn, input| {
        if input.ids.is_empty() {
            return Err(tower_mcp::Error::tool("ids must contain at least one entry ID"));
        }
        let mut cmd = redis::cmd("XDEL");
        cmd.arg(&input.key);
        for id in &input.ids {
            cmd.arg(id);
        }

        let deleted: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XDEL failed")?;

        if deleted == 0 {
            return Ok(CallToolResult::text(format!(
                "No entries deleted: none of the {} ID(s) exist in stream '{}'",
                input.ids.len(),
                input.key
            )));
        }

        Ok(CallToolResult::text(format!(
            "OK - deleted {} of {} ID(s) from stream '{}'",
            deleted,
            input.ids.len(),
            input.key
        )))
    }
);

database_tool!(destructive, xgroup_destroy, "redis_xgroup_destroy",
    "DANGEROUS: Destroy a consumer group (XGROUP DESTROY). Its consumers and pending \
     entries list are discarded; stream entries are kept.",
    {
        /// Stream key
        pub key: String,
        /// Consumer group name
        pub group: String,
    } => |conn, input| {
        let destroyed: i64 = redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(&input.key)
            .arg(&input.group)
            .query_async(&mut conn)
            .await
            .tool_context("XGROUP DESTROY failed")?;

        if destroyed == 0 {
            return Ok(CallToolResult::text(format!(
                "No group destroyed: stream '{}' has no group '{}'",
                input.key, input.group
            )));
        }

        Ok(CallToolResult::text(format!(
            "OK - destroyed group '{}' on stream '{}'",
            input.group, input.key
        )))
    }
);

// --- Pub/sub write tools ---

database_tool!(write, publish, "redis_publish",
    "Publish a message to a pub/sub channel (PUBLISH). Returns the number of subscribers \
     that received it. In Redis Cluster the count covers only subscribers connected to \
     the node that handled the publish. Delivery is fire-and-forget: subscribers that \
     are not connected miss the message.",
    {
        /// Channel name
        pub channel: String,
        /// Message payload
        pub message: String,
    } => |conn, input| {
        let receivers: i64 = redis::cmd("PUBLISH")
            .arg(&input.channel)
            .arg(&input.message)
            .query_async(&mut conn)
            .await
            .tool_context("PUBLISH failed")?;

        Ok(CallToolResult::text(format!(
            "OK - published to '{}', received by {} subscriber(s)",
            input.channel, receivers
        )))
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    use redis::Value;

    fn bulk(s: &str) -> Value {
        Value::BulkString(s.as_bytes().to_vec())
    }

    fn entry(id: &str, fields: &[(&str, &str)]) -> Value {
        Value::Array(vec![
            bulk(id),
            Value::Array(
                fields
                    .iter()
                    .flat_map(|(f, v)| [bulk(f), bulk(v)])
                    .collect(),
            ),
        ])
    }

    #[test]
    fn field_pairs_reads_resp2_and_resp3() {
        let resp2 = Value::Array(vec![
            bulk("name"),
            bulk("g1"),
            bulk("pending"),
            Value::Int(2),
        ]);
        let resp3 = Value::Map(vec![
            (bulk("name"), bulk("g1")),
            (bulk("pending"), Value::Int(2)),
        ]);
        for value in [resp2, resp3] {
            let pairs = field_pairs(&value).expect("pairs");
            let rendered: Vec<_> = pairs
                .iter()
                .map(|(k, v)| format!("{k}={}", super::super::format_value(v)))
                .collect();
            assert_eq!(rendered, ["name=g1", "pending=2"]);
        }
        assert!(field_pairs(&Value::Array(vec![bulk("odd")])).is_none());
    }

    #[test]
    fn format_info_records_renders_one_line_per_group() {
        let groups = Value::Array(vec![
            Value::Array(vec![
                bulk("name"),
                bulk("g1"),
                bulk("consumers"),
                Value::Int(1),
            ]),
            Value::Map(vec![
                (bulk("name"), bulk("g2")),
                (bulk("consumers"), Value::Int(0)),
            ]),
        ]);
        assert_eq!(
            format_info_records(&groups),
            ["name=g1 consumers=1", "name=g2 consumers=0"]
        );
        assert!(format_info_records(&Value::Array(vec![])).is_empty());
    }

    #[test]
    fn format_stream_entry_marks_deleted_entries() {
        assert_eq!(
            format_stream_entry(&entry("1-0", &[("a", "1"), ("b", "2")])),
            "1-0  a=1, b=2"
        );
        assert_eq!(
            format_stream_entry(&Value::Array(vec![bulk("2-0"), Value::Nil])),
            "2-0  (deleted)"
        );
    }

    #[test]
    fn stream_batches_reads_resp2_resp3_and_nil() {
        let resp2 = Value::Array(vec![Value::Array(vec![
            bulk("s"),
            Value::Array(vec![entry("1-0", &[("a", "1")])]),
        ])]);
        let resp3 = Value::Map(vec![(
            bulk("s"),
            Value::Array(vec![entry("1-0", &[("a", "1")])]),
        )]);
        for reply in [resp2, resp3] {
            let batches = stream_batches(&reply);
            assert_eq!(batches.len(), 1);
            assert_eq!(batches[0].0, "s");
            assert_eq!(batches[0].1.len(), 1);
        }
        assert!(stream_batches(&Value::Nil).is_empty());
    }
}
