//! Key-level Redis tools (keys, scan, get, key_type, ttl, exists, memory_usage, object_encoding,
//! object_freq, object_idletime, object_help, set, del, expire, rename, mget, mset, persist,
//! unlink, copy, dump, restore, randomkey, touch, incr, decr, append, strlen, getrange, setrange,
//! setnx)

use tower_mcp::CallToolResult;

use super::RedisResultExt;

use super::format_value;
use crate::serde_helpers;
use crate::tools::macros::{database_tool, mcp_module};

mcp_module! {
    keys => "redis_keys",
    scan => "redis_scan",
    get => "redis_get",
    key_type => "redis_type",
    ttl => "redis_ttl",
    exists => "redis_exists",
    memory_usage => "redis_memory_usage",
    object_encoding => "redis_object_encoding",
    object_freq => "redis_object_freq",
    object_idletime => "redis_object_idletime",
    object_help => "redis_object_help",
    set => "redis_set",
    del => "redis_del",
    expire => "redis_expire",
    rename => "redis_rename",
    mget => "redis_mget",
    mset => "redis_mset",
    persist => "redis_persist",
    unlink => "redis_unlink",
    copy => "redis_copy",
    dump => "redis_dump",
    restore => "redis_restore",
    randomkey => "redis_randomkey",
    touch => "redis_touch",
    incr => "redis_incr",
    decr => "redis_decr",
    append => "redis_append",
    strlen => "redis_strlen",
    getrange => "redis_getrange",
    setrange => "redis_setrange",
    setnx => "redis_setnx",
}

fn default_pattern() -> String {
    "*".to_string()
}

fn default_limit() -> usize {
    100
}

database_tool!(read_only, keys, "redis_keys",
    "List keys matching a pattern using SCAN (production-safe, non-blocking).",
    {
        /// Key pattern to match (default: "*")
        #[serde(default = "default_pattern")]
        pub pattern: String,
        /// Maximum number of keys to return (default: 100)
        #[serde(default = "default_limit", deserialize_with = "serde_helpers::string_or_usize::deserialize")]
        pub limit: usize,
    } => |conn, input| {
        // Use SCAN to safely iterate keys
        let mut cursor: u64 = 0;
        let mut all_keys: Vec<String> = Vec::new();

        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&input.pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .tool_context("SCAN failed")?;

            all_keys.extend(keys);
            cursor = new_cursor;

            if cursor == 0 || all_keys.len() >= input.limit {
                break;
            }
        }

        // Truncate to limit
        all_keys.truncate(input.limit);

        let output = if all_keys.is_empty() {
            format!("No keys found matching pattern '{}'", input.pattern)
        } else {
            format!(
                "Found {} key(s) matching '{}'\n\n{}",
                all_keys.len(),
                input.pattern,
                all_keys.join("\n")
            )
        };

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, scan, "redis_scan",
    "Scan keys with optional type filter. Prefer over redis_keys when filtering by type.",
    {
        /// Key pattern to match (default: "*")
        #[serde(default = "default_pattern")]
        pub pattern: String,
        /// Filter by key type (e.g., "string", "list", "set", "zset", "hash", "stream")
        #[serde(default)]
        pub key_type: Option<String>,
        /// Maximum number of keys to return (default: 100)
        #[serde(default = "default_limit", deserialize_with = "serde_helpers::string_or_usize::deserialize")]
        pub limit: usize,
    } => |conn, input| {
        let mut cursor: u64 = 0;
        let mut all_keys: Vec<String> = Vec::new();

        loop {
            let mut cmd = redis::cmd("SCAN");
            cmd.arg(cursor)
                .arg("MATCH")
                .arg(&input.pattern)
                .arg("COUNT")
                .arg(100);

            // Add TYPE filter if specified
            if let Some(ref key_type) = input.key_type {
                cmd.arg("TYPE").arg(key_type);
            }

            let (new_cursor, keys): (u64, Vec<String>) = cmd
                .query_async(&mut conn)
                .await
                .tool_context("SCAN failed")?;

            all_keys.extend(keys);
            cursor = new_cursor;

            if cursor == 0 || all_keys.len() >= input.limit {
                break;
            }
        }

        all_keys.truncate(input.limit);

        let type_info = input
            .key_type
            .as_ref()
            .map(|t| format!(" of type '{}'", t))
            .unwrap_or_default();

        let output = if all_keys.is_empty() {
            format!(
                "No keys{} found matching pattern '{}'",
                type_info, input.pattern
            )
        } else {
            format!(
                "Found {} key(s){} matching '{}'\n\n{}",
                all_keys.len(),
                type_info,
                input.pattern,
                all_keys.join("\n")
            )
        };

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, get, "redis_get",
    "Get the value of a key.",
    {
        /// Key to get
        pub key: String,
    } => |conn, input| {
        let value: Option<String> = redis::cmd("GET")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("GET failed")?;

        match value {
            Some(v) => Ok(CallToolResult::text(v)),
            None => Ok(CallToolResult::text(format!(
                "(nil) - key '{}' not found",
                input.key
            ))),
        }
    }
);

database_tool!(read_only, key_type, "redis_type",
    "Get the data type of a key.",
    {
        /// Key to check type
        pub key: String,
    } => |conn, input| {
        let key_type: String = redis::cmd("TYPE")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("TYPE failed")?;

        Ok(CallToolResult::text(format!("{}: {}", input.key, key_type)))
    }
);

database_tool!(read_only, ttl, "redis_ttl",
    "Get the TTL of a key in seconds (-1 = no expiry, -2 = missing).",
    {
        /// Key to check TTL
        pub key: String,
    } => |conn, input| {
        let ttl: i64 = redis::cmd("TTL")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("TTL failed")?;

        let message = match ttl {
            -2 => format!("{}: key does not exist", input.key),
            -1 => format!("{}: no expiry set", input.key),
            _ => format!("{}: {} seconds remaining", input.key, ttl),
        };

        Ok(CallToolResult::text(message))
    }
);

database_tool!(read_only, exists, "redis_exists",
    "Check if one or more keys exist.",
    {
        /// Keys to check existence
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("EXISTS");
        for key in &input.keys {
            cmd.arg(key);
        }

        let count: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("EXISTS failed")?;

        Ok(CallToolResult::text(format!(
            "{} of {} key(s) exist",
            count,
            input.keys.len()
        )))
    }
);

database_tool!(read_only, memory_usage, "redis_memory_usage",
    "Get memory usage of a key in bytes (MEMORY USAGE).",
    {
        /// Key to check memory usage
        pub key: String,
    } => |conn, input| {
        let bytes: Option<i64> = redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("MEMORY USAGE failed")?;

        match bytes {
            Some(b) => Ok(CallToolResult::text(format!("{}: {} bytes", input.key, b))),
            None => Ok(CallToolResult::text(format!(
                "{}: key does not exist",
                input.key
            ))),
        }
    }
);

database_tool!(read_only, object_encoding, "redis_object_encoding",
    "Get the internal encoding of a key. Useful for understanding memory usage patterns.",
    {
        /// Key to check encoding
        pub key: String,
    } => |conn, input| {
        let encoding: Option<String> = redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("OBJECT ENCODING failed")?;

        match encoding {
            Some(enc) => Ok(CallToolResult::text(format!("{}: {}", input.key, enc))),
            None => Ok(CallToolResult::text(format!(
                "{}: key does not exist",
                input.key
            ))),
        }
    }
);

database_tool!(read_only, object_freq, "redis_object_freq",
    "Get the LFU access frequency counter for a key. \
     Only works with allkeys-lfu or volatile-lfu eviction policy.",
    {
        /// Key to get LFU access frequency for
        pub key: String,
    } => |conn, input| {
        let freq: i64 = redis::cmd("OBJECT")
            .arg("FREQ")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("OBJECT FREQ failed")?;

        Ok(CallToolResult::text(format!(
            "{}: LFU frequency counter = {}",
            input.key, freq
        )))
    }
);

database_tool!(read_only, object_idletime, "redis_object_idletime",
    "Get idle time of a key in seconds since last access.",
    {
        /// Key to get idle time for
        pub key: String,
    } => |conn, input| {
        let idle: i64 = redis::cmd("OBJECT")
            .arg("IDLETIME")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("OBJECT IDLETIME failed")?;

        Ok(CallToolResult::text(format!(
            "{}: idle for {} seconds",
            input.key, idle
        )))
    }
);

database_tool!(read_only, object_help, "redis_object_help",
    "Get available OBJECT subcommands.",
    {} => |conn, _input| {
        let result: Vec<String> = redis::cmd("OBJECT")
            .arg("HELP")
            .query_async(&mut conn)
            .await
            .tool_context("OBJECT HELP failed")?;

        Ok(CallToolResult::text(format!(
            "OBJECT subcommands:\n{}",
            result.join("\n")
        )))
    }
);

// --- Write tools ---

database_tool!(write, set, "redis_set",
    "Set a key to a string value with optional expiry and conditional flags (NX/XX).",
    {
        /// Key to set
        pub key: String,
        /// Value to set
        pub value: String,
        /// Expire time in seconds
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub ex: Option<u64>,
        /// Expire time in milliseconds
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub px: Option<u64>,
        /// Only set if key does not already exist
        #[serde(default)]
        pub nx: bool,
        /// Only set if key already exists
        #[serde(default)]
        pub xx: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("SET");
        cmd.arg(&input.key).arg(&input.value);

        if let Some(ex) = input.ex {
            cmd.arg("EX").arg(ex);
        }
        if let Some(px) = input.px {
            cmd.arg("PX").arg(px);
        }
        if input.nx {
            cmd.arg("NX");
        }
        if input.xx {
            cmd.arg("XX");
        }

        let result: Option<String> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("SET failed")?;

        match result {
            Some(_) => Ok(CallToolResult::text(format!(
                "OK - set '{}' successfully",
                input.key
            ))),
            None => Ok(CallToolResult::text(format!(
                "Key '{}' not set (condition not met: {})",
                input.key,
                if input.nx {
                    "NX - key already exists"
                } else {
                    "XX - key does not exist"
                }
            ))),
        }
    }
);

database_tool!(destructive, del, "redis_del",
    "DANGEROUS: Delete one or more keys.",
    {
        /// Keys to delete
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("DEL");
        for key in &input.keys {
            cmd.arg(key);
        }

        let count: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("DEL failed")?;

        Ok(CallToolResult::text(format!(
            "Deleted {} of {} key(s)",
            count,
            input.keys.len()
        )))
    }
);

database_tool!(write, expire, "redis_expire",
    "Set a timeout on a key in seconds. Key auto-deletes after expiry.",
    {
        /// Key to set expiry on
        pub key: String,
        /// TTL in seconds
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub seconds: i64,
    } => |conn, input| {
        let result: bool = redis::cmd("EXPIRE")
            .arg(&input.key)
            .arg(input.seconds)
            .query_async(&mut conn)
            .await
            .tool_context("EXPIRE failed")?;

        if result {
            Ok(CallToolResult::text(format!(
                "OK - TTL set to {} seconds on '{}'",
                input.seconds, input.key
            )))
        } else {
            Ok(CallToolResult::text(format!(
                "Key '{}' does not exist or timeout could not be set",
                input.key
            )))
        }
    }
);

database_tool!(write, rename, "redis_rename",
    "Rename a key. Overwrites the destination key if it exists.",
    {
        /// Current key name
        pub key: String,
        /// New key name
        pub newkey: String,
    } => |conn, input| {
        let _: () = redis::cmd("RENAME")
            .arg(&input.key)
            .arg(&input.newkey)
            .query_async(&mut conn)
            .await
            .tool_context("RENAME failed")?;

        Ok(CallToolResult::text(format!(
            "OK - renamed '{}' to '{}'",
            input.key, input.newkey
        )))
    }
);

database_tool!(read_only, mget, "redis_mget",
    "Get the values of multiple keys in a single call.",
    {
        /// Keys to get
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("MGET");
        for key in &input.keys {
            cmd.arg(key);
        }

        let values: Vec<redis::Value> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("MGET failed")?;

        let output = input
            .keys
            .iter()
            .zip(values.iter())
            .map(|(k, v)| format!("{}: {}", k, format_value(v)))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult::text(output))
    }
);

/// A key-value pair for MSET
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeyValuePair {
    /// Key name
    pub key: String,
    /// Value to set
    pub value: String,
}

database_tool!(write, mset, "redis_mset",
    "Set multiple key-value pairs in a single atomic call.",
    {
        /// Key-value pairs to set
        pub entries: Vec<KeyValuePair>,
    } => |conn, input| {
        let mut cmd = redis::cmd("MSET");
        for entry in &input.entries {
            cmd.arg(&entry.key).arg(&entry.value);
        }

        let _: () = cmd
            .query_async(&mut conn)
            .await
            .tool_context("MSET failed")?;

        Ok(CallToolResult::text(format!(
            "OK - set {} key(s)",
            input.entries.len()
        )))
    }
);

database_tool!(write, persist, "redis_persist",
    "Remove the expiry from a key, making it persistent.",
    {
        /// Key to remove expiry from
        pub key: String,
    } => |conn, input| {
        let result: bool = redis::cmd("PERSIST")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("PERSIST failed")?;

        if result {
            Ok(CallToolResult::text(format!(
                "OK - expiry removed from '{}'",
                input.key
            )))
        } else {
            Ok(CallToolResult::text(format!(
                "Key '{}' does not exist or has no expiry",
                input.key
            )))
        }
    }
);

database_tool!(destructive, unlink, "redis_unlink",
    "DANGEROUS: Asynchronously delete one or more keys (non-blocking version of DEL).",
    {
        /// Keys to unlink (async delete)
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("UNLINK");
        for key in &input.keys {
            cmd.arg(key);
        }

        let count: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("UNLINK failed")?;

        Ok(CallToolResult::text(format!(
            "Unlinked {} of {} key(s)",
            count,
            input.keys.len()
        )))
    }
);

database_tool!(write, copy, "redis_copy",
    "Copy a key to a new key. Use replace=true to overwrite the destination.",
    {
        /// Source key
        pub source: String,
        /// Destination key
        pub destination: String,
        /// Replace destination key if it already exists
        #[serde(default)]
        pub replace: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("COPY");
        cmd.arg(&input.source).arg(&input.destination);
        if input.replace {
            cmd.arg("REPLACE");
        }

        let result: bool = cmd
            .query_async(&mut conn)
            .await
            .tool_context("COPY failed")?;

        if result {
            Ok(CallToolResult::text(format!(
                "OK - copied '{}' to '{}'",
                input.source, input.destination
            )))
        } else {
            Ok(CallToolResult::text(format!(
                "COPY failed: destination '{}' already exists (use replace=true to overwrite)",
                input.destination
            )))
        }
    }
);

database_tool!(read_only, dump, "redis_dump",
    "Serialize a key's value using Redis internal format. Returns hex-encoded bytes \
     for use with RESTORE.",
    {
        /// Key to dump
        pub key: String,
    } => |conn, input| {
        let value: redis::Value = redis::cmd("DUMP")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("DUMP failed")?;

        match value {
            redis::Value::BulkString(bytes) => {
                let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                Ok(CallToolResult::text(format!(
                    "{}: {} bytes\n{}",
                    input.key,
                    bytes.len(),
                    hex
                )))
            }
            redis::Value::Nil => Ok(CallToolResult::text(format!(
                "(nil) - key '{}' not found",
                input.key
            ))),
            _ => Ok(CallToolResult::text(format_value(&value))),
        }
    }
);

database_tool!(write, restore, "redis_restore",
    "Restore a key from a serialized value (from DUMP). \
     The serialized_value must be hex-encoded.",
    {
        /// Key to restore
        pub key: String,
        /// TTL in milliseconds (0 = no expiry)
        #[serde(deserialize_with = "serde_helpers::string_or_u64::deserialize")]
        pub ttl_ms: u64,
        /// Hex-encoded serialized value from DUMP
        pub serialized_value: String,
    } => |conn, input| {
        // Decode hex string to bytes
        let bytes: Result<Vec<u8>, _> = (0..input.serialized_value.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(
                    &input.serialized_value[i..i.min(input.serialized_value.len()) + 2],
                    16,
                )
            })
            .collect();

        let bytes =
            bytes.map_err(|_| tower_mcp::Error::tool("Invalid hex string in serialized_value"))?;

        let _: () = redis::cmd("RESTORE")
            .arg(&input.key)
            .arg(input.ttl_ms)
            .arg(bytes.as_slice())
            .query_async(&mut conn)
            .await
            .tool_context("RESTORE failed")?;

        Ok(CallToolResult::text(format!(
            "OK - restored key '{}'",
            input.key
        )))
    }
);

database_tool!(read_only, randomkey, "redis_randomkey",
    "Return a random key from the database.",
    {} => |conn, _input| {
        let key: Option<String> = redis::cmd("RANDOMKEY")
            .query_async(&mut conn)
            .await
            .tool_context("RANDOMKEY failed")?;

        match key {
            Some(k) => Ok(CallToolResult::text(k)),
            None => Ok(CallToolResult::text("(empty) - database has no keys")),
        }
    }
);

database_tool!(read_only, touch, "redis_touch",
    "Update the last access time of one or more keys without modifying them.",
    {
        /// Keys to touch (update last access time)
        pub keys: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("TOUCH");
        for key in &input.keys {
            cmd.arg(key);
        }

        let count: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("TOUCH failed")?;

        Ok(CallToolResult::text(format!(
            "Touched {} of {} key(s)",
            count,
            input.keys.len()
        )))
    }
);

// --- P1 String Operations ---

database_tool!(write, incr, "redis_incr",
    "Increment the integer value of a key by 1. Creates the key with value 1 if it does not exist.",
    {
        /// Key to increment
        pub key: String,
    } => |conn, input| {
        let value: i64 = redis::cmd("INCR")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("INCR failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {}",
            input.key, value
        )))
    }
);

database_tool!(write, decr, "redis_decr",
    "Decrement the integer value of a key by 1. Creates the key with value -1 if it does not exist.",
    {
        /// Key to decrement
        pub key: String,
    } => |conn, input| {
        let value: i64 = redis::cmd("DECR")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("DECR failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {}",
            input.key, value
        )))
    }
);

database_tool!(write, append, "redis_append",
    "Append a value to a key. Creates the key if it does not exist. Returns the new string length.",
    {
        /// Key to append to
        pub key: String,
        /// Value to append
        pub value: String,
    } => |conn, input| {
        let length: i64 = redis::cmd("APPEND")
            .arg(&input.key)
            .arg(&input.value)
            .query_async(&mut conn)
            .await
            .tool_context("APPEND failed")?;

        Ok(CallToolResult::text(format!(
            "OK - '{}' new length: {}",
            input.key, length
        )))
    }
);

database_tool!(read_only, strlen, "redis_strlen",
    "Get the length of the string value stored at a key.",
    {
        /// Key to get string length of
        pub key: String,
    } => |conn, input| {
        let length: i64 = redis::cmd("STRLEN")
            .arg(&input.key)
            .query_async(&mut conn)
            .await
            .tool_context("STRLEN failed")?;

        Ok(CallToolResult::text(format!(
            "{}: {} bytes",
            input.key, length
        )))
    }
);

database_tool!(read_only, getrange, "redis_getrange",
    "Get a substring of the string value at a key by start and end offsets (inclusive).",
    {
        /// Key to get substring from
        pub key: String,
        /// Start offset (0-based, negative counts from end)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub start: i64,
        /// End offset (inclusive, negative counts from end)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub end: i64,
    } => |conn, input| {
        let value: String = redis::cmd("GETRANGE")
            .arg(&input.key)
            .arg(input.start)
            .arg(input.end)
            .query_async(&mut conn)
            .await
            .tool_context("GETRANGE failed")?;

        Ok(CallToolResult::text(value))
    }
);

database_tool!(write, setrange, "redis_setrange",
    "Overwrite part of a string value at the given byte offset. Returns the new string length.",
    {
        /// Key to overwrite substring in
        pub key: String,
        /// Byte offset to start overwriting at
        #[serde(deserialize_with = "serde_helpers::string_or_u64::deserialize")]
        pub offset: u64,
        /// Value to write at the offset
        pub value: String,
    } => |conn, input| {
        let length: i64 = redis::cmd("SETRANGE")
            .arg(&input.key)
            .arg(input.offset)
            .arg(&input.value)
            .query_async(&mut conn)
            .await
            .tool_context("SETRANGE failed")?;

        Ok(CallToolResult::text(format!(
            "OK - '{}' new length: {}",
            input.key, length
        )))
    }
);

database_tool!(write, setnx, "redis_setnx",
    "Set a key only if it does not already exist. Returns whether the key was set.",
    {
        /// Key to set
        pub key: String,
        /// Value to set
        pub value: String,
    } => |conn, input| {
        let was_set: bool = redis::cmd("SETNX")
            .arg(&input.key)
            .arg(&input.value)
            .query_async(&mut conn)
            .await
            .tool_context("SETNX failed")?;

        if was_set {
            Ok(CallToolResult::text(format!(
                "OK - set '{}' (key was new)",
                input.key
            )))
        } else {
            Ok(CallToolResult::text(format!(
                "Key '{}' already exists, not set",
                input.key
            )))
        }
    }
);
