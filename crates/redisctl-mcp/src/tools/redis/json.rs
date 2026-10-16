//! RedisJSON module tools (JSON.GET, JSON.SET, JSON.DEL, JSON.MGET, JSON.TYPE, etc.)

use tower_mcp::CallToolResult;

use super::RedisResultExt;

use super::format_value;
use crate::serde_helpers;
use crate::tools::macros::{database_tool, mcp_module};

mcp_module! {
    json_get => "redis_json_get",
    json_mget => "redis_json_mget",
    json_type => "redis_json_type",
    json_arrlen => "redis_json_arrlen",
    json_objkeys => "redis_json_objkeys",
    json_objlen => "redis_json_objlen",
    json_strlen => "redis_json_strlen",
    json_set => "redis_json_set",
    json_numincrby => "redis_json_numincrby",
    json_arrappend => "redis_json_arrappend",
    json_arrinsert => "redis_json_arrinsert",
    json_toggle => "redis_json_toggle",
    json_del => "redis_json_del",
    json_clear => "redis_json_clear",
    json_arrpop => "redis_json_arrpop",
    json_arrtrim => "redis_json_arrtrim"
}

fn default_root_path() -> String {
    "$".to_string()
}

fn default_paths() -> Vec<String> {
    vec!["$".to_string()]
}

// ---------------------------------------------------------------------------
// Read-only tools
// ---------------------------------------------------------------------------

database_tool!(read_only, json_get, "redis_json_get",
    "Get JSON value(s) at one or more paths. Returns the JSON string. Requires the RedisJSON module.",
    {
        /// Key to get
        pub key: String,
        /// JSONPath expression(s) (default: "$" for root)
        #[serde(default = "default_paths")]
        pub paths: Vec<String>,
    } => |conn, input| {
        let mut cmd = redis::cmd("JSON.GET");
        cmd.arg(&input.key);
        for path in &input.paths {
            cmd.arg(path);
        }

        let value: String = cmd
            .query_async(&mut conn)
            .await
            .tool_context("JSON.GET failed")?;

        Ok(CallToolResult::text(value))
    }
);

database_tool!(read_only, json_mget, "redis_json_mget",
    "Get JSON values from multiple keys at a path. Requires the RedisJSON module.",
    {
        /// Keys to get
        pub keys: Vec<String>,
        /// JSONPath expression (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        if input.keys.is_empty() {
            return Err(tower_mcp::Error::tool("keys must not be empty"));
        }
        let mut cmd = redis::cmd("JSON.MGET");
        for key in &input.keys {
            cmd.arg(key);
        }
        cmd.arg(&input.path);

        let values: Vec<redis::Value> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("JSON.MGET failed")?;

        let output = input.keys.iter().zip(values.iter())
            .map(|(key, val)| format!("{}: {}", key, format_value(val)))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, json_type, "redis_json_type",
    "Get the JSON type at a path (string, number, boolean, object, array, null). Requires the RedisJSON module.",
    {
        /// Key to check
        pub key: String,
        /// JSONPath expression (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let value: redis::Value = redis::cmd("JSON.TYPE")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.TYPE failed")?;

        Ok(CallToolResult::text(format_value(&value)))
    }
);

database_tool!(read_only, json_arrlen, "redis_json_arrlen",
    "Get the length of a JSON array. Requires the RedisJSON module.",
    {
        /// Key containing the array
        pub key: String,
        /// JSONPath to the array (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let value: redis::Value = redis::cmd("JSON.ARRLEN")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.ARRLEN failed")?;

        Ok(CallToolResult::text(format_value(&value)))
    }
);

database_tool!(read_only, json_objkeys, "redis_json_objkeys",
    "Get the keys of a JSON object. Requires the RedisJSON module.",
    {
        /// Key containing the object
        pub key: String,
        /// JSONPath to the object (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let value: redis::Value = redis::cmd("JSON.OBJKEYS")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.OBJKEYS failed")?;

        Ok(CallToolResult::text(format_value(&value)))
    }
);

database_tool!(read_only, json_objlen, "redis_json_objlen",
    "Get the number of keys in a JSON object. Requires the RedisJSON module.",
    {
        /// Key containing the object
        pub key: String,
        /// JSONPath to the object (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let value: redis::Value = redis::cmd("JSON.OBJLEN")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.OBJLEN failed")?;

        Ok(CallToolResult::text(format_value(&value)))
    }
);

database_tool!(read_only, json_strlen, "redis_json_strlen",
    "Get the length of a JSON string value. Requires the RedisJSON module.",
    {
        /// Key containing the string
        pub key: String,
        /// JSONPath to the string (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let value: redis::Value = redis::cmd("JSON.STRLEN")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.STRLEN failed")?;

        Ok(CallToolResult::text(format_value(&value)))
    }
);

// ---------------------------------------------------------------------------
// Write tools (non-destructive)
// ---------------------------------------------------------------------------

database_tool!(write, json_set, "redis_json_set",
    "Set a JSON value at a path. Creates the key if it does not exist. Value must be valid JSON. Requires the RedisJSON module.\n\n\
     Architectural note: the NX flag enables conditional-write patterns — idempotent creation, \
     conflict-free seeding, and distinguishing new records from updates (e.g. new user joins \
     vs heartbeat refreshes).",
    {
        /// Key to set
        pub key: String,
        /// JSONPath expression (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
        /// JSON value to set (as a JSON string, e.g. "\"hello\"", "42", "{\"a\":1}")
        pub value: String,
        /// Only set if the key/path does not already exist
        #[serde(default)]
        pub nx: bool,
        /// Only set if the key/path already exists
        #[serde(default)]
        pub xx: bool,
    } => |conn, input| {
        if input.nx && input.xx {
            return Err(tower_mcp::Error::tool(
                "Cannot set both nx and xx: NX (only set if not exists) and XX (only set if exists) are mutually exclusive",
            ));
        }

        let mut cmd = redis::cmd("JSON.SET");
        cmd.arg(&input.key).arg(&input.path).arg(&input.value);
        if input.nx {
            cmd.arg("NX");
        }
        if input.xx {
            cmd.arg("XX");
        }

        let result: Option<String> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("JSON.SET failed")?;

        match result {
            Some(_) => Ok(CallToolResult::text(format!(
                "OK - set '{}' at path '{}'", input.key, input.path
            ))),
            None => Ok(CallToolResult::text(
                "Not set (NX/XX condition not met)".to_string()
            )),
        }
    }
);

database_tool!(write, json_numincrby, "redis_json_numincrby",
    "Increment a JSON number value by the given amount. Requires the RedisJSON module.",
    {
        /// Key containing the number
        pub key: String,
        /// JSONPath to the number
        #[serde(default = "default_root_path")]
        pub path: String,
        /// Amount to increment by (can be negative)
        pub value: f64,
    } => |conn, input| {
        let result: String = redis::cmd("JSON.NUMINCRBY")
            .arg(&input.key)
            .arg(&input.path)
            .arg(input.value)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.NUMINCRBY failed")?;

        Ok(CallToolResult::text(format!(
            "New value at '{}': {}", input.path, result
        )))
    }
);

database_tool!(write, json_arrappend, "redis_json_arrappend",
    "Append one or more JSON values to an array. Values must be valid JSON strings. Requires the RedisJSON module.",
    {
        /// Key containing the array
        pub key: String,
        /// JSONPath to the array
        #[serde(default = "default_root_path")]
        pub path: String,
        /// JSON values to append (e.g. ["\"item\"", "42", "{\"a\":1}"])
        pub values: Vec<String>,
    } => |conn, input| {
        if input.values.is_empty() {
            return Err(tower_mcp::Error::tool("values must not be empty"));
        }
        let mut cmd = redis::cmd("JSON.ARRAPPEND");
        cmd.arg(&input.key).arg(&input.path);
        for v in &input.values {
            cmd.arg(v);
        }

        let result: redis::Value = cmd
            .query_async(&mut conn)
            .await
            .tool_context("JSON.ARRAPPEND failed")?;

        Ok(CallToolResult::text(format!(
            "New array length: {}", format_value(&result)
        )))
    }
);

database_tool!(write, json_arrinsert, "redis_json_arrinsert",
    "Insert one or more JSON values into an array at the given index. Requires the RedisJSON module.",
    {
        /// Key containing the array
        pub key: String,
        /// JSONPath to the array
        #[serde(default = "default_root_path")]
        pub path: String,
        /// Index to insert at (0-based, negative counts from end)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub index: i64,
        /// JSON values to insert
        pub values: Vec<String>,
    } => |conn, input| {
        if input.values.is_empty() {
            return Err(tower_mcp::Error::tool("values must not be empty"));
        }
        let mut cmd = redis::cmd("JSON.ARRINSERT");
        cmd.arg(&input.key).arg(&input.path).arg(input.index);
        for v in &input.values {
            cmd.arg(v);
        }

        let result: redis::Value = cmd
            .query_async(&mut conn)
            .await
            .tool_context("JSON.ARRINSERT failed")?;

        Ok(CallToolResult::text(format!(
            "New array length: {}", format_value(&result)
        )))
    }
);

database_tool!(write, json_toggle, "redis_json_toggle",
    "Toggle a JSON boolean value (true becomes false, false becomes true). Requires the RedisJSON module.",
    {
        /// Key containing the boolean
        pub key: String,
        /// JSONPath to the boolean
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let result: redis::Value = redis::cmd("JSON.TOGGLE")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.TOGGLE failed")?;

        Ok(CallToolResult::text(format!(
            "Toggled '{}' at '{}': {}", input.key, input.path, format_value(&result)
        )))
    }
);

// ---------------------------------------------------------------------------
// Destructive tools
// ---------------------------------------------------------------------------

database_tool!(destructive, json_del, "redis_json_del",
    "DANGEROUS: Delete a JSON value at a path. If path is root, deletes the entire key. Requires the RedisJSON module.",
    {
        /// Key to delete from
        pub key: String,
        /// JSONPath to delete (default: "$" deletes entire document)
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let deleted: i64 = redis::cmd("JSON.DEL")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.DEL failed")?;

        Ok(CallToolResult::text(format!(
            "Deleted {} value(s) at path '{}' in '{}'", deleted, input.path, input.key
        )))
    }
);

database_tool!(destructive, json_clear, "redis_json_clear",
    "DANGEROUS: Clear container values (arrays/objects become empty, numbers become 0). Requires the RedisJSON module.",
    {
        /// Key to clear
        pub key: String,
        /// JSONPath to clear (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
    } => |conn, input| {
        let cleared: i64 = redis::cmd("JSON.CLEAR")
            .arg(&input.key)
            .arg(&input.path)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.CLEAR failed")?;

        Ok(CallToolResult::text(format!(
            "Cleared {} value(s) at path '{}' in '{}'", cleared, input.path, input.key
        )))
    }
);

database_tool!(destructive, json_arrpop, "redis_json_arrpop",
    "DANGEROUS: Remove and return an element from a JSON array. Requires the RedisJSON module.",
    {
        /// Key containing the array
        pub key: String,
        /// JSONPath to the array (default: "$")
        #[serde(default = "default_root_path")]
        pub path: String,
        /// Index to pop (-1 for last element, default)
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_i64::deserialize")]
        pub index: Option<i64>,
    } => |conn, input| {
        let mut cmd = redis::cmd("JSON.ARRPOP");
        cmd.arg(&input.key).arg(&input.path);
        if let Some(idx) = input.index {
            cmd.arg(idx);
        }

        let result: String = cmd
            .query_async(&mut conn)
            .await
            .tool_context("JSON.ARRPOP failed")?;

        Ok(CallToolResult::text(format!("Popped: {}", result)))
    }
);

database_tool!(destructive, json_arrtrim, "redis_json_arrtrim",
    "DANGEROUS: Trim a JSON array to the specified inclusive range. Requires the RedisJSON module.",
    {
        /// Key containing the array
        pub key: String,
        /// JSONPath to the array
        #[serde(default = "default_root_path")]
        pub path: String,
        /// Start index (inclusive)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub start: i64,
        /// Stop index (inclusive)
        #[serde(deserialize_with = "serde_helpers::string_or_i64::deserialize")]
        pub stop: i64,
    } => |conn, input| {
        let result: redis::Value = redis::cmd("JSON.ARRTRIM")
            .arg(&input.key)
            .arg(&input.path)
            .arg(input.start)
            .arg(input.stop)
            .query_async(&mut conn)
            .await
            .tool_context("JSON.ARRTRIM failed")?;

        Ok(CallToolResult::text(format!(
            "Trimmed array at '{}': new length {}", input.path, format_value(&result)
        )))
    }
);
