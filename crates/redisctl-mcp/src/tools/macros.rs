//! Declarative macros for MCP tool definitions.
//!
//! These macros eliminate per-tool boilerplate for the three platform patterns:
//!
//! - [`database_tool!`] — Redis direct connections (url + profile -> connection)
//! - [`cloud_tool!`] — Redis Cloud API (profile -> client)
//! - [`enterprise_tool!`] — Redis Enterprise API (profile -> client)
//! - [`mcp_module!`] — Auto-generates `TOOL_NAMES` and `router()` from a tool list

/// Define an MCP tool for direct Redis database operations.
///
/// Generates the input struct (with `url` and `profile` fields injected), the
/// builder function, connection acquisition, and permission guard.
///
/// # Safety tiers
///
/// - `read_only` — `.read_only_safe()`, no permission guard
/// - `write` — `.non_destructive()`, checks `state.is_write_allowed()`
/// - `destructive` — `.destructive()`, checks `state.is_destructive_allowed()`
///
/// # Example
///
/// ```ignore
/// database_tool!(read_only, ping, "redis_ping",
///     "Test connectivity by sending a PING command",
///     {} => |conn, _input| {
///         let response: String = redis::cmd("PING")
///             .query_async(&mut conn).await
///             .tool_context("PING failed")?;
///         Ok(CallToolResult::text(format!("Connected: {}", response)))
///     }
/// );
/// ```
#[cfg(feature = "database")]
macro_rules! database_tool {
    // --- Permission guard dispatch ---

    (@guard no_guard $state:ident) => {};

    (@guard write_guard $state:ident) => {
        if !$state.is_write_allowed() {
            return Err(tower_mcp::Error::tool(
                "Write operations not allowed in read-only mode",
            ));
        }
    };

    (@guard destructive_guard $state:ident) => {
        if !$state.is_destructive_allowed() {
            return Err(tower_mcp::Error::tool(
                "Destructive operations require policy tier 'full'",
            ));
        }
    };

    // --- Main implementation ---

    (@impl $safety_method:ident, $guard:ident, $fn_name:ident, $tool_name:literal, $description:expr,
     { $($(#[$field_meta:meta])* pub $field_name:ident : $field_type:ty),* $(,)? }
     => |$conn:ident, $input:ident| $body:block
    ) => {
        pastey::paste! {
            #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
            pub struct [<$fn_name:camel Input>] {
                /// Optional Redis URL (overrides profile, uses configured URL if not provided)
                #[serde(default)]
                pub url: Option<String>,
                /// Optional profile name to resolve connection from (uses default profile if not set)
                #[serde(default)]
                pub profile: Option<String>,
                $(
                    $(#[$field_meta])*
                    pub $field_name: $field_type,
                )*
            }

            pub fn $fn_name(state: std::sync::Arc<crate::state::AppState>) -> tower_mcp::Tool {
                tower_mcp::ToolBuilder::new($tool_name)
                    .description($description)
                    .$safety_method()
                    .extractor_handler(
                        state,
                        |tower_mcp::extract::State(state): tower_mcp::extract::State<std::sync::Arc<crate::state::AppState>>,
                         tower_mcp::extract::Json(mut $input): tower_mcp::extract::Json<[<$fn_name:camel Input>]>| async move {
                            database_tool!(@guard $guard state);
                            #[allow(unused_mut)]
                            let mut $conn = super::get_connection(
                                $input.url.take(), $input.profile.as_deref(), &state
                            ).await?;
                            #[allow(unused_variables)]
                            let state = &state;
                            $body
                        },
                    )
                    .build()
            }
        }
    };

    // --- Stateful variant: exposes state as a user-provided identifier ---

    (@impl_stateful $safety_method:ident, $guard:ident, $fn_name:ident, $tool_name:literal, $description:expr,
     { $($(#[$field_meta:meta])* pub $field_name:ident : $field_type:ty),* $(,)? }
     => |$state:ident, $conn:ident, $input:ident| $body:block
    ) => {
        pastey::paste! {
            #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
            pub struct [<$fn_name:camel Input>] {
                /// Optional Redis URL (overrides profile, uses configured URL if not provided)
                #[serde(default)]
                pub url: Option<String>,
                /// Optional profile name to resolve connection from (uses default profile if not set)
                #[serde(default)]
                pub profile: Option<String>,
                $(
                    $(#[$field_meta])*
                    pub $field_name: $field_type,
                )*
            }

            pub fn $fn_name(state: std::sync::Arc<crate::state::AppState>) -> tower_mcp::Tool {
                tower_mcp::ToolBuilder::new($tool_name)
                    .description($description)
                    .$safety_method()
                    .extractor_handler(
                        state,
                        |tower_mcp::extract::State($state): tower_mcp::extract::State<std::sync::Arc<crate::state::AppState>>,
                         tower_mcp::extract::Json(mut $input): tower_mcp::extract::Json<[<$fn_name:camel Input>]>| async move {
                            database_tool!(@guard $guard $state);
                            #[allow(unused_mut)]
                            let mut $conn = super::get_connection(
                                $input.url.take(), $input.profile.as_deref(), &$state
                            ).await?;
                            let $state = &$state;
                            $body
                        },
                    )
                    .build()
            }
        }
    };

    // --- Public entry points ---

    (read_only, $($rest:tt)*) => {
        database_tool!(@impl read_only_safe, no_guard, $($rest)*);
    };
    (write, $($rest:tt)*) => {
        database_tool!(@impl non_destructive, write_guard, $($rest)*);
    };
    (destructive, $($rest:tt)*) => {
        database_tool!(@impl destructive, destructive_guard, $($rest)*);
    };

    // Stateful variants — handler receives |state, conn, input| instead of |conn, input|
    (read_only_stateful, $($rest:tt)*) => {
        database_tool!(@impl_stateful read_only_safe, no_guard, $($rest)*);
    };
    (write_stateful, $($rest:tt)*) => {
        database_tool!(@impl_stateful non_destructive, write_guard, $($rest)*);
    };
    (destructive_stateful, $($rest:tt)*) => {
        database_tool!(@impl_stateful destructive, destructive_guard, $($rest)*);
    };
}
#[cfg(feature = "database")]
pub(crate) use database_tool;

/// Define an MCP tool for Redis Cloud API operations.
///
/// Generates the input struct (with `profile` field injected), the builder
/// function, client acquisition via `cloud_client_for_profile()`, and
/// permission guard.
///
/// The handler body receives `client` (the Cloud API client) and `input`.
///
/// # Example
///
/// ```ignore
/// cloud_tool!(read_only, list_subscriptions, "list_subscriptions",
///     "List all subscriptions.",
///     {} => |client, _input| {
///         let handler = SubscriptionHandler::new(client);
///         let result = handler.get_all_subscriptions().await
///             .tool_context("Failed to list subscriptions")?;
///         CallToolResult::from_serialize(&result)
///     }
/// );
/// ```
#[allow(unused_macros)]
macro_rules! cloud_tool {
    (@guard no_guard $state:ident) => {};

    (@guard write_guard $state:ident) => {
        if !$state.is_write_allowed() {
            return Err(tower_mcp::Error::tool(
                "Write operations not allowed in read-only mode",
            ));
        }
    };

    (@guard destructive_guard $state:ident) => {
        if !$state.is_destructive_allowed() {
            return Err(tower_mcp::Error::tool(
                "Destructive operations require policy tier 'full'",
            ));
        }
    };

    (@impl $safety_method:ident, $guard:ident, $fn_name:ident, $tool_name:literal, $description:expr,
     { $($(#[$field_meta:meta])* pub $field_name:ident : $field_type:ty),* $(,)? }
     => |$client:ident, $input:ident| $body:block
    ) => {
        pastey::paste! {
            #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
            pub struct [<$fn_name:camel Input>] {
                /// Profile name for multi-account support. If not specified, uses the first configured profile or default.
                #[serde(default)]
                pub profile: Option<String>,
                $(
                    $(#[$field_meta])*
                    pub $field_name: $field_type,
                )*
            }

            pub fn $fn_name(state: std::sync::Arc<crate::state::AppState>) -> tower_mcp::Tool {
                tower_mcp::ToolBuilder::new($tool_name)
                    .description($description)
                    .$safety_method()
                    .extractor_handler(
                        state,
                        |tower_mcp::extract::State(state): tower_mcp::extract::State<std::sync::Arc<crate::state::AppState>>,
                         tower_mcp::extract::Json($input): tower_mcp::extract::Json<[<$fn_name:camel Input>]>| async move {
                            cloud_tool!(@guard $guard state);
                            let $client = state
                                .cloud_client_for_profile($input.profile.as_deref())
                                .await
                                .map_err(|e| crate::tools::credential_error("cloud", e))?;
                            #[allow(unused_variables)]
                            let state = &state;
                            $body
                        },
                    )
                    .build()
            }
        }
    };

    (read_only, $($rest:tt)*) => {
        cloud_tool!(@impl read_only_safe, no_guard, $($rest)*);
    };
    (write, $($rest:tt)*) => {
        cloud_tool!(@impl non_destructive, write_guard, $($rest)*);
    };
    (destructive, $($rest:tt)*) => {
        cloud_tool!(@impl destructive, destructive_guard, $($rest)*);
    };
}
#[allow(unused_imports)]
pub(crate) use cloud_tool;

/// Define an MCP tool for Redis Enterprise API operations.
///
/// Generates the input struct (with `profile` field injected), the builder
/// function, client acquisition via `enterprise_client_for_profile()`, and
/// permission guard.
///
/// The handler body receives `client` (the Enterprise API client) and `input`.
///
/// # Example
///
/// ```ignore
/// enterprise_tool!(read_only, list_enterprise_databases, "list_enterprise_databases",
///     "List all databases.",
///     {} => |client, _input| {
///         let handler = DatabaseHandler::new(client);
///         let databases = handler.list().await
///             .tool_context("Failed to list databases")?;
///         CallToolResult::from_list("databases", &databases)
///     }
/// );
/// ```
#[allow(unused_macros)]
macro_rules! enterprise_tool {
    (@guard no_guard $state:ident) => {};

    (@guard write_guard $state:ident) => {
        if !$state.is_write_allowed() {
            return Err(tower_mcp::Error::tool(
                "Write operations not allowed in read-only mode",
            ));
        }
    };

    (@guard destructive_guard $state:ident) => {
        if !$state.is_destructive_allowed() {
            return Err(tower_mcp::Error::tool(
                "Destructive operations require policy tier 'full'",
            ));
        }
    };

    (@impl $safety_method:ident, $guard:ident, $fn_name:ident, $tool_name:literal, $description:expr,
     { $($(#[$field_meta:meta])* pub $field_name:ident : $field_type:ty),* $(,)? }
     => |$client:ident, $input:ident| $body:block
    ) => {
        pastey::paste! {
            #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
            pub struct [<$fn_name:camel Input>] {
                /// Profile name for multi-cluster support. If not specified, uses the first configured profile or default.
                #[serde(default)]
                pub profile: Option<String>,
                $(
                    $(#[$field_meta])*
                    pub $field_name: $field_type,
                )*
            }

            pub fn $fn_name(state: std::sync::Arc<crate::state::AppState>) -> tower_mcp::Tool {
                tower_mcp::ToolBuilder::new($tool_name)
                    .description($description)
                    .$safety_method()
                    .extractor_handler(
                        state,
                        |tower_mcp::extract::State(state): tower_mcp::extract::State<std::sync::Arc<crate::state::AppState>>,
                         tower_mcp::extract::Json($input): tower_mcp::extract::Json<[<$fn_name:camel Input>]>| async move {
                            enterprise_tool!(@guard $guard state);
                            let $client = state
                                .enterprise_client_for_profile($input.profile.as_deref())
                                .await
                                .map_err(|e| crate::tools::credential_error("enterprise", e))?;
                            #[allow(unused_variables)]
                            let state = &state;
                            $body
                        },
                    )
                    .build()
            }
        }
    };

    (read_only, $($rest:tt)*) => {
        enterprise_tool!(@impl read_only_safe, no_guard, $($rest)*);
    };
    (write, $($rest:tt)*) => {
        enterprise_tool!(@impl non_destructive, write_guard, $($rest)*);
    };
    (destructive, $($rest:tt)*) => {
        enterprise_tool!(@impl destructive, destructive_guard, $($rest)*);
    };
}
#[allow(unused_imports)]
pub(crate) use enterprise_tool;

/// Generate `TOOL_NAMES` constant and `router()` function from a tool list.
///
/// # Example
///
/// ```ignore
/// mcp_module! {
///     ping => "redis_ping",
///     info => "redis_info",
///     dbsize => "redis_dbsize",
/// }
/// ```
///
/// Expands to:
///
/// ```ignore
/// pub(super) const TOOL_NAMES: &[&str] = &["redis_ping", "redis_info", "redis_dbsize"];
///
/// pub fn router(state: Arc<AppState>) -> McpRouter {
///     McpRouter::new()
///         .tool(ping(state.clone()))
///         .tool(info(state.clone()))
///         .tool(dbsize(state.clone()))
/// }
/// ```
#[cfg(any(feature = "cloud", feature = "enterprise", feature = "database"))]
macro_rules! mcp_module {
    { $( $fn_name:ident => $tool_name:literal ),* $(,)? } => {
        pub(super) const TOOL_NAMES: &[&str] = &[$($tool_name),*];

        pub fn router(state: std::sync::Arc<crate::state::AppState>) -> tower_mcp::McpRouter {
            tower_mcp::McpRouter::new()
            $(
                .tool($fn_name(state.clone()))
            )*
        }
    };
}
#[cfg(any(feature = "cloud", feature = "enterprise", feature = "database"))]
pub(crate) use mcp_module;
