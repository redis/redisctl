//! Data structure Redis tools (hgetall, lrange, smembers, zrange, xinfo_stream, xrange, xlen,
//! pubsub_channels, pubsub_numsub, hset, hdel, lpush, rpush, lpop, rpop, sadd, srem, zadd,
//! zrem, xadd, xtrim, hget, hmget, hlen, hexists, hkeys, hvals, hincrby, hexpire, scard,
//! sismember, sunion, sinter, sdiff, zcard, zscore, zrank, zcount, zrangebyscore,
//! zremrangebyscore, llen, lindex)

use std::collections::HashMap;

use tower_mcp::{CallToolResult, ResultExt};

use crate::serde_helpers;
use crate::tools::macros::{database_tool, mcp_module};

/// A score-member pair for sorted set operations
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
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
                "(empty list or key '{}' not found)",
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
            cmd.arg("COUNT").arg(count);
        }

        let result: redis::Value = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XRANGE failed")?;

        // Format stream entries
        let formatted = match &result {
            redis::Value::Array(entries) if entries.is_empty() => {
                format!("(empty stream or key '{}' not found)", input.key)
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
        let mut cmd = redis::cmd("ZRANGEBYSCORE");
        cmd.arg(&input.key).arg(&input.min).arg(&input.max);

        if input.withscores {
            cmd.arg("WITHSCORES");
        }

        if let (Some(offset), Some(count)) = (input.offset, input.count) {
            cmd.arg("LIMIT").arg(offset).arg(count);
        }

        if input.withscores {
            let result: Vec<(String, f64)> = cmd
                .query_async(&mut conn)
                .await
                .tool_context("ZRANGEBYSCORE failed")?;

            if result.is_empty() {
                return Ok(CallToolResult::text(format!(
                    "No members in '{}' with scores in [{}, {}]",
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
                    "No members in '{}' with scores in [{}, {}]",
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

        let entry_id: String = cmd
            .query_async(&mut conn)
            .await
            .tool_context("XADD failed")?;

        Ok(CallToolResult::text(format!(
            "OK - added entry {} to stream '{}'",
            entry_id, input.key
        )))
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
     0 = condition not met, -2 = field does not exist.",
    {
        /// Hash key
        pub key: String,
        /// TTL in seconds
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub seconds: i64,
        /// Fields to set TTL on
        pub fields: Vec<String>,
        /// Optional condition: NX, XX, GT, or LT
        #[serde(default)]
        pub condition: Option<String>,
    } => |conn, input| {
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
