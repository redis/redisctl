//! Direct Redis database tools

mod aliases;
mod bulk;
pub(crate) mod command_safety;
mod connection;
mod diagnostics;
mod json;
mod keys;
mod raw;
mod search;
mod server;
mod structures;

pub(crate) use connection::RedisConnection;

#[allow(unused_imports)]
pub use aliases::*;
#[allow(unused_imports)]
pub use bulk::*;
#[allow(unused_imports)]
pub use diagnostics::*;
#[allow(unused_imports)]
pub use json::*;
#[allow(unused_imports)]
pub use keys::*;
#[allow(unused_imports)]
pub use raw::*;
#[allow(unused_imports)]
pub use search::*;
#[allow(unused_imports)]
pub use server::*;
#[allow(unused_imports)]
pub use structures::*;

use std::sync::Arc;

use tower_mcp::{McpRouter, ToolError};

use super::SubModule;
use crate::state::AppState;

/// Sub-modules within the Database (Redis) toolset, each with its own tool names and router.
pub const SUB_MODULES: &[SubModule] = &[
    SubModule {
        name: "server",
        tool_names: server::TOOL_NAMES,
    },
    SubModule {
        name: "keys",
        tool_names: keys::TOOL_NAMES,
    },
    SubModule {
        name: "structures",
        tool_names: structures::TOOL_NAMES,
    },
    SubModule {
        name: "diagnostics",
        tool_names: diagnostics::TOOL_NAMES,
    },
    SubModule {
        name: "json",
        tool_names: json::TOOL_NAMES,
    },
    SubModule {
        name: "search",
        tool_names: search::TOOL_NAMES,
    },
    SubModule {
        name: "bulk",
        tool_names: bulk::TOOL_NAMES,
    },
    SubModule {
        name: "raw",
        tool_names: raw::TOOL_NAMES,
    },
    SubModule {
        name: "aliases",
        tool_names: aliases::TOOL_NAMES,
    },
];

/// Get all Database tool names as owned strings.
pub fn tool_names() -> Vec<String> {
    SUB_MODULES
        .iter()
        .flat_map(|sm| sm.tool_names.iter().map(|s| (*s).to_string()))
        .collect()
}

/// Get tool names for a specific sub-module by name.
pub fn sub_tool_names(name: &str) -> Option<&'static [&'static str]> {
    SUB_MODULES
        .iter()
        .find(|sm| sm.name == name)
        .map(|sm| sm.tool_names)
}

/// Build an MCP sub-router for a specific sub-module by name.
pub fn sub_router(name: &str, state: Arc<AppState>) -> Option<McpRouter> {
    match name {
        "server" => Some(server::router(state)),
        "keys" => Some(keys::router(state)),
        "structures" => Some(structures::router(state)),
        "diagnostics" => Some(diagnostics::router(state)),
        "json" => Some(json::router(state)),
        "search" => Some(search::router(state)),
        "bulk" => Some(bulk::router(state)),
        "raw" => Some(raw::router(state)),
        "aliases" => Some(aliases::router(state)),
        _ => None,
    }
}

/// Resolve a Redis URL from the provided inputs.
///
/// Resolution order:
/// 1. If `url` is provided, use it directly (backward compatible)
/// 2. If `profile` is provided, resolve via profile system
/// 3. Fall back to `state.database_url`
pub(crate) fn resolve_redis_url(
    url: Option<String>,
    profile: Option<&str>,
    state: &AppState,
) -> Result<String, ToolError> {
    if let Some(url) = url {
        return Ok(url);
    }
    if let Some(profile_name) = profile {
        return state
            .database_url_for_profile(Some(profile_name))
            .map_err(|e| {
                ToolError::new(format!(
                    "Failed to resolve database profile '{}': {}",
                    profile_name, e
                ))
            });
    }
    // Try default profile resolution (no explicit profile name)
    if state.database_url.is_none()
        && let Ok(url) = state.database_url_for_profile(None)
    {
        return Ok(url);
    }
    state.database_url.clone().ok_or_else(|| {
        ToolError::new(
            "No Redis URL provided, no profile configured, and no default database URL set",
        )
    })
}

/// Resolve a Redis URL and return a cached connection (standalone or cluster).
///
/// This is the main entry point for tool handlers. It resolves the URL from
/// the input parameters, then returns a pooled connection (creating one if needed).
pub(crate) async fn get_connection(
    url: Option<String>,
    profile: Option<&str>,
    state: &AppState,
) -> Result<RedisConnection, ToolError> {
    let url = resolve_redis_url(url, profile, state)?;
    state
        .redis_connection_for_url(&url)
        .await
        .map_err(|e| ToolError::new(format!("Connection failed: {}", e)))
}

/// Helper to format Redis values for display
pub(crate) fn format_value(v: &redis::Value) -> String {
    match v {
        redis::Value::Nil => "(nil)".to_string(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Array(arr) => format!(
            "[{}]",
            arr.iter().map(format_value).collect::<Vec<_>>().join(", ")
        ),
        _ => format!("{:?}", v),
    }
}

/// Render a Redis error the way the server sent it.
///
/// `redis-rs` splits every error reply on the first space and treats the first
/// word as an error code. That is right for `ERR ...` and `WRONGTYPE ...`, but
/// module errors are often plain sentences ("No such index", "Syntax error at
/// offset 8"), so its `Display` output becomes "No: such index". Rebuilding
/// `code detail` restores the original text for both cases. Errors without a
/// server code (I/O, type conversion, client-side) keep their `Display` form.
pub(crate) fn describe_redis_error(err: &redis::RedisError) -> String {
    match (err.code(), err.detail()) {
        (Some(code), Some(detail)) => format!("{code} {detail}"),
        (Some(code), None) => code.to_string(),
        _ => err.to_string(),
    }
}

/// `tool_context` for Redis results that formats the server error faithfully.
///
/// Shadows `tower_mcp::ResultExt::tool_context` inside the Redis tool modules so
/// every handler gets the readable form without per-call changes.
pub(crate) trait RedisResultExt<T> {
    fn tool_context(self, context: impl Into<String>) -> Result<T, tower_mcp::Error>;
}

impl<T> RedisResultExt<T> for Result<T, redis::RedisError> {
    fn tool_context(self, context: impl Into<String>) -> Result<T, tower_mcp::Error> {
        self.map_err(|err| {
            tower_mcp::Error::Tool(ToolError::new(format!(
                "{}: {}",
                context.into(),
                describe_redis_error(&err)
            )))
        })
    }
}

/// Build an MCP sub-router containing all Redis database tools
pub fn router(state: Arc<AppState>) -> McpRouter {
    McpRouter::new()
        .merge(server::router(state.clone()))
        .merge(keys::router(state.clone()))
        .merge(structures::router(state.clone()))
        .merge(diagnostics::router(state.clone()))
        .merge(json::router(state.clone()))
        .merge(search::router(state.clone()))
        .merge(bulk::router(state.clone()))
        .merge(raw::router(state.clone()))
        .merge(aliases::router(state))
}

#[cfg(test)]
mod error_format_tests {
    use super::describe_redis_error;

    /// Build a `RedisError` exactly as the client would from a raw `-...` reply line.
    fn server_error(line: &str) -> redis::RedisError {
        let raw = format!("-{line}\r\n");
        redis::parse_redis_value(raw.as_bytes())
            .expect("error line parses as a value")
            .extract_error()
            .expect_err("error reply becomes an error")
    }

    #[test]
    fn module_errors_keep_their_first_word() {
        for line in [
            "No such index idx:nope",
            "Syntax error at offset 8 near abc",
            "Not a tag field",
            "Index already exists",
            "idx:nope: no such index",
            "Existing key has wrong Redis type",
        ] {
            let err = server_error(line);
            assert_ne!(
                err.to_string(),
                line,
                "precondition: redis-rs mangles {line:?}"
            );
            assert_eq!(describe_redis_error(&err), line);
        }
    }

    #[test]
    fn core_errors_are_the_original_server_text() {
        let err = server_error("ERR value is not an integer or out of range");
        assert_eq!(
            describe_redis_error(&err),
            "ERR value is not an integer or out of range"
        );

        let err = server_error("WRONGTYPE Operation against a key holding the wrong kind of value");
        assert_eq!(
            describe_redis_error(&err),
            "WRONGTYPE Operation against a key holding the wrong kind of value"
        );

        let err = server_error("NOAUTH Authentication required.");
        assert_eq!(
            describe_redis_error(&err),
            "NOAUTH Authentication required."
        );
    }

    #[test]
    fn client_side_errors_fall_back_to_display() {
        let err = redis::RedisError::from((
            redis::ErrorKind::TypeError,
            "Response was of incompatible type",
            "expected string".to_string(),
        ));
        assert_eq!(describe_redis_error(&err), err.to_string());

        let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = redis::RedisError::from(io);
        assert_eq!(describe_redis_error(&err), err.to_string());
    }
}
