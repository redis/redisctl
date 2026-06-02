//! Cluster service management tools for Redis Enterprise

use serde_json::Value;
use tower_mcp::{CallToolResult, ResultExt};

use crate::tools::macros::{enterprise_tool, mcp_module};

mcp_module! {
    list_services => "list_enterprise_services",
    get_service => "get_enterprise_service",
    get_service_status => "get_enterprise_service_status",
    update_service => "update_enterprise_service",
    start_service => "start_enterprise_service",
    stop_service => "stop_enterprise_service",
    restart_service => "restart_enterprise_service",
}

/// Look up a single service entry in the `/v1/local/services` map (keyed by
/// service name), returning a helpful error listing available names if absent.
fn lookup_service(services: &Value, service_id: &str) -> Result<Value, tower_mcp::Error> {
    let map = services.as_object().ok_or_else(|| {
        tower_mcp::Error::tool("Unexpected response format from /v1/local/services".to_string())
    })?;
    map.get(service_id).cloned().ok_or_else(|| {
        let available: Vec<&str> = map.keys().map(String::as_str).collect();
        tower_mcp::Error::tool(format!(
            "Service '{}' not found. Available services: {}",
            service_id,
            available.join(", ")
        ))
    })
}

enterprise_tool!(read_only, list_services, "list_enterprise_services",
    "List all cluster services.",
    {} => |client, _input| {
        // `ServicesHandler` was removed upstream; the cluster service map is
        // served at /v1/local/services, keyed by service name.
        let services: Value = client
            .get("/v1/local/services")
            .await
            .tool_context("Failed to list services")?;

        CallToolResult::from_serialize(&services)
    }
);

enterprise_tool!(read_only, get_service, "get_enterprise_service",
    "Get service details by ID.",
    {
        /// Service ID (e.g., "cm_server", "mdns_server", "stats_archiver")
        pub service_id: String,
    } => |client, input| {
        let services: Value = client
            .get("/v1/local/services")
            .await
            .tool_context("Failed to get service")?;
        let service = lookup_service(&services, &input.service_id)?;

        CallToolResult::from_serialize(&service)
    }
);

enterprise_tool!(read_only, get_service_status, "get_enterprise_service_status",
    "Get service status including per-node status.",
    {
        /// Service ID
        pub service_id: String,
    } => |client, input| {
        let services: Value = client
            .get("/v1/local/services")
            .await
            .tool_context("Failed to get service status")?;
        let service = lookup_service(&services, &input.service_id)?;

        CallToolResult::from_serialize(&service)
    }
);

enterprise_tool!(write, update_service, "update_enterprise_service",
    "Update a service's configuration. Pass fields as JSON.",
    {
        /// Service ID to update
        pub service_id: String,
        /// Updated service configuration as JSON (e.g., enabled, config, node_uids)
        pub config: Value,
    } => |client, input| {
        let result: Value = client
            .put_raw(&format!("/v1/services/{}", input.service_id), input.config)
            .await
            .tool_context("Failed to update service")?;

        CallToolResult::from_serialize(&result)
    }
);

enterprise_tool!(write, start_service, "start_enterprise_service",
    "Start a stopped service.",
    {
        /// Service ID
        pub service_id: String,
    } => |client, input| {
        let result: Value = client
            .post_raw(
                &format!("/v1/services/{}/start", input.service_id),
                serde_json::json!({}),
            )
            .await
            .tool_context("Failed to start service")?;

        CallToolResult::from_serialize(&result)
    }
);

enterprise_tool!(write, stop_service, "stop_enterprise_service",
    "Stop a running service.",
    {
        /// Service ID
        pub service_id: String,
    } => |client, input| {
        let result: Value = client
            .post_raw(
                &format!("/v1/services/{}/stop", input.service_id),
                serde_json::json!({}),
            )
            .await
            .tool_context("Failed to stop service")?;

        CallToolResult::from_serialize(&result)
    }
);

enterprise_tool!(write, restart_service, "restart_enterprise_service",
    "Restart a service (stop then start).",
    {
        /// Service ID
        pub service_id: String,
    } => |client, input| {
        let result: Value = client
            .post_raw(
                &format!("/v1/services/{}/restart", input.service_id),
                serde_json::json!({}),
            )
            .await
            .tool_context("Failed to restart service")?;

        CallToolResult::from_serialize(&result)
    }
);
