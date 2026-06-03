//! Cluster, license, node, maintenance, and certificate tools

use redis_enterprise::cluster::ClusterHandler;
use redis_enterprise::license::{LicenseHandler, LicenseUpdateRequest};
use redis_enterprise::nodes::NodeHandler;
use redis_enterprise::stats::{StatsHandler, StatsQuery};
use serde_json::Value;
use tower_mcp::{CallToolResult, ResultExt};

use crate::tools::macros::{enterprise_tool, mcp_module};

mcp_module! {
    get_cluster => "get_cluster",
    get_cluster_stats => "get_cluster_stats",
    update_cluster => "update_enterprise_cluster",
    get_cluster_policy => "get_enterprise_cluster_policy",
    update_cluster_policy => "update_enterprise_cluster_policy",
    enable_maintenance_mode => "enable_enterprise_maintenance_mode",
    disable_maintenance_mode => "disable_enterprise_maintenance_mode",
    get_cluster_certificates => "get_enterprise_cluster_certificates",
    rotate_cluster_certificates => "rotate_enterprise_cluster_certificates",
    update_cluster_certificates => "update_enterprise_cluster_certificates",
    get_license => "get_license",
    get_license_usage => "get_license_usage",
    update_license => "update_enterprise_license",
    validate_license => "validate_enterprise_license",
    list_nodes => "list_nodes",
    get_node => "get_node",
    get_node_stats => "get_node_stats",
    enable_node_maintenance => "enable_enterprise_node_maintenance",
    disable_node_maintenance => "disable_enterprise_node_maintenance",
    rebalance_node => "rebalance_enterprise_node",
    drain_node => "drain_enterprise_node",
    update_enterprise_node => "update_enterprise_node",
    remove_enterprise_node => "remove_enterprise_node",
    get_enterprise_cluster_services => "get_enterprise_cluster_services",
}

// ============================================================================
// Cluster tools
// ============================================================================

enterprise_tool!(read_only, get_cluster, "get_cluster",
    "Get cluster information including name, version, and configuration",
    {} => |client, _input| {
        let handler = ClusterHandler::new(client);
        let cluster = handler
            .info()
            .await
            .tool_context("Failed to get cluster info")?;

        CallToolResult::from_serialize(&cluster)
    }
);

enterprise_tool!(read_only, get_cluster_stats, "get_cluster_stats",
    "Get cluster-level statistics. Optionally specify interval and time range \
     for historical data.",
    {
        /// Time interval for aggregation: "1sec", "10sec", "5min", "15min", "1hour", "12hour", "1week"
        #[serde(default)]
        pub interval: Option<String>,
        /// Start time for historical query (ISO 8601 format, e.g., "2024-01-15T10:00:00Z")
        #[serde(default)]
        pub start_time: Option<String>,
        /// End time for historical query (ISO 8601 format)
        #[serde(default)]
        pub end_time: Option<String>,
    } => |client, input| {
        let handler = StatsHandler::new(client);

        if input.interval.is_some()
            || input.start_time.is_some()
            || input.end_time.is_some()
        {
            let query = StatsQuery {
                interval: input.interval,
                stime: input.start_time,
                etime: input.end_time,
                metrics: None,
            };
            let stats = handler
                .cluster(Some(query))
                .await
                .tool_context("Failed to get cluster stats")?;
            CallToolResult::from_serialize(&stats)
        } else {
            let stats = handler
                .cluster_last()
                .await
                .tool_context("Failed to get cluster stats")?;
            CallToolResult::from_serialize(&stats)
        }
    }
);

enterprise_tool!(write, update_cluster, "update_enterprise_cluster",
    "Update cluster configuration settings. Typed fields cover the most common changes; \
     use `extra` for any additional fields not listed here.",
    {
        /// Cluster display name
        #[serde(default)]
        pub name: Option<String>,
        /// Enable or disable email alerts for the cluster
        #[serde(default)]
        pub email_alerts: Option<bool>,
        /// Alert settings as a JSON object
        #[serde(default)]
        pub alert_settings: Option<Value>,
        /// Block cluster configuration changes (maintenance mode)
        #[serde(default)]
        pub block_cluster_changes: Option<bool>,
        /// Additional advanced cluster fields
        #[serde(flatten)]
        pub extra: Option<serde_json::Map<String, Value>>,
    } => |client, input| {
        let mut body = serde_json::Map::new();
        if let Some(name) = input.name {
            body.insert("name".to_string(), serde_json::json!(name));
        }
        if let Some(email_alerts) = input.email_alerts {
            body.insert("email_alerts".to_string(), serde_json::json!(email_alerts));
        }
        if let Some(alert_settings) = input.alert_settings {
            body.insert("alert_settings".to_string(), alert_settings);
        }
        if let Some(block_cluster_changes) = input.block_cluster_changes {
            body.insert("block_cluster_changes".to_string(), serde_json::json!(block_cluster_changes));
        }
        if let Some(extra) = input.extra {
            body.extend(extra);
        }
        let handler = ClusterHandler::new(client);
        let result = handler
            .update(Value::Object(body))
            .await
            .tool_context("Failed to update cluster")?;

        CallToolResult::from_serialize(&result)
    }
);

enterprise_tool!(read_only, get_cluster_policy, "get_enterprise_cluster_policy",
    "Get cluster policy settings including default shards placement, \
     rack awareness, and default Redis version.",
    {} => |client, _input| {
        let handler = ClusterHandler::new(client);
        let policy = handler
            .policy()
            .await
            .tool_context("Failed to get cluster policy")?;

        CallToolResult::from_serialize(&policy)
    }
);

enterprise_tool!(write, update_cluster_policy, "update_enterprise_cluster_policy",
    "Update cluster policy settings. Typed fields cover the most common changes; \
     use `extra` for any additional fields not listed here.",
    {
        /// Default shards placement: "dense" or "sparse"
        #[serde(default)]
        pub default_shards_placement: Option<String>,
        /// Enable rack-aware shard placement
        #[serde(default)]
        pub rack_aware: Option<bool>,
        /// Default Redis version for new databases (e.g., "7.2")
        #[serde(default)]
        pub default_provisioned_redis_version: Option<String>,
        /// Redis upgrade policy: "major" or "latest"
        #[serde(default)]
        pub redis_upgrade_policy: Option<String>,
        /// Default proxy policy for non-sharded databases
        #[serde(default)]
        pub default_non_sharded_proxy_policy: Option<String>,
        /// Default proxy policy for sharded databases
        #[serde(default)]
        pub default_sharded_proxy_policy: Option<String>,
        /// Additional advanced policy fields
        #[serde(flatten)]
        pub extra: Option<serde_json::Map<String, Value>>,
    } => |client, input| {
        let mut body = serde_json::Map::new();
        if let Some(default_shards_placement) = input.default_shards_placement {
            body.insert("default_shards_placement".to_string(), serde_json::json!(default_shards_placement));
        }
        if let Some(rack_aware) = input.rack_aware {
            body.insert("rack_aware".to_string(), serde_json::json!(rack_aware));
        }
        if let Some(default_provisioned_redis_version) = input.default_provisioned_redis_version {
            body.insert("default_provisioned_redis_version".to_string(), serde_json::json!(default_provisioned_redis_version));
        }
        if let Some(redis_upgrade_policy) = input.redis_upgrade_policy {
            body.insert("redis_upgrade_policy".to_string(), serde_json::json!(redis_upgrade_policy));
        }
        if let Some(default_non_sharded_proxy_policy) = input.default_non_sharded_proxy_policy {
            body.insert("default_non_sharded_proxy_policy".to_string(), serde_json::json!(default_non_sharded_proxy_policy));
        }
        if let Some(default_sharded_proxy_policy) = input.default_sharded_proxy_policy {
            body.insert("default_sharded_proxy_policy".to_string(), serde_json::json!(default_sharded_proxy_policy));
        }
        if let Some(extra) = input.extra {
            body.extend(extra);
        }
        let handler = ClusterHandler::new(client);
        let result = handler
            .policy_update(Value::Object(body))
            .await
            .tool_context("Failed to update cluster policy")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Maintenance Mode Operations
// ============================================================================

enterprise_tool!(write, enable_maintenance_mode, "enable_enterprise_maintenance_mode",
    "Enable maintenance mode. Configuration changes will be blocked \
     until maintenance mode is disabled.",
    {} => |client, _input| {
        let handler = ClusterHandler::new(client);
        let result = handler
            .update(serde_json::json!({"block_cluster_changes": true}))
            .await
            .tool_context("Failed to enable maintenance mode")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Maintenance mode enabled",
            "result": result
        }))
    }
);

enterprise_tool!(write, disable_maintenance_mode, "disable_enterprise_maintenance_mode",
    "Disable maintenance mode and re-enable configuration changes.",
    {} => |client, _input| {
        let handler = ClusterHandler::new(client);
        let result = handler
            .update(serde_json::json!({"block_cluster_changes": false}))
            .await
            .tool_context("Failed to disable maintenance mode")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Maintenance mode disabled",
            "result": result
        }))
    }
);

// ============================================================================
// Certificate Operations
// ============================================================================

enterprise_tool!(read_only, get_cluster_certificates, "get_enterprise_cluster_certificates",
    "Get all configured certificates (proxy, syncer, API).",
    {} => |client, _input| {
        let handler = ClusterHandler::new(client);
        let certificates = handler
            .certificates()
            .await
            .tool_context("Failed to get certificates")?;

        CallToolResult::from_serialize(&certificates)
    }
);

enterprise_tool!(write, rotate_cluster_certificates, "rotate_enterprise_cluster_certificates",
    "Rotate all certificates, generating new ones to replace existing.",
    {} => |client, _input| {
        let handler = ClusterHandler::new(client);
        let result = handler
            .certificates_rotate()
            .await
            .tool_context("Failed to rotate certificates")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Certificate rotation initiated",
            "result": result
        }))
    }
);

enterprise_tool!(write, update_cluster_certificates, "update_enterprise_cluster_certificates",
    "Update a specific certificate. Provide the certificate name (proxy, syncer, api), \
     PEM-encoded certificate, and PEM-encoded private key.",
    {
        /// Certificate name (e.g., "proxy", "syncer", "api")
        pub name: String,
        /// PEM-encoded certificate content
        pub certificate: String,
        /// PEM-encoded private key content
        pub key: String,
    } => |client, input| {
        let handler = ClusterHandler::new(client);
        let body = serde_json::json!({
            "name": &input.name,
            "certificate": input.certificate,
            "key": input.key
        });
        let result = handler
            .update_cert(body)
            .await
            .tool_context("Failed to update certificate")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Certificate updated successfully",
            "name": input.name,
            "result": result
        }))
    }
);

// ============================================================================
// License tools
// ============================================================================

enterprise_tool!(read_only, get_license, "get_license",
    "Get license information including type, expiration, and enabled features",
    {} => |client, _input| {
        let handler = LicenseHandler::new(client);
        let license = handler.get().await.tool_context("Failed to get license")?;

        CallToolResult::from_serialize(&license)
    }
);

enterprise_tool!(read_only, get_license_usage, "get_license_usage",
    "Get license utilization statistics including shards, nodes, and RAM usage against limits",
    {} => |client, _input| {
        let handler = LicenseHandler::new(client);
        let usage = handler
            .usage()
            .await
            .tool_context("Failed to get license usage")?;

        CallToolResult::from_serialize(&usage)
    }
);

enterprise_tool!(write, update_license, "update_enterprise_license",
    "Apply a new license key to the cluster.",
    {
        /// The license key string to install
        pub license_key: String,
    } => |client, input| {
        let handler = LicenseHandler::new(client);
        let request = LicenseUpdateRequest {
            license: input.license_key,
        };
        let license = handler
            .update(request)
            .await
            .tool_context("Failed to update license")?;

        CallToolResult::from_serialize(&license)
    }
);

enterprise_tool!(read_only, validate_license, "validate_enterprise_license",
    "Validate a license key without applying it (dry-run).",
    {
        /// The license key string to validate
        pub license_key: String,
    } => |client, input| {
        let handler = LicenseHandler::new(client);
        let license = handler
            .validate(&input.license_key)
            .await
            .tool_context("License validation failed")?;

        CallToolResult::from_serialize(&license)
    }
);

// ============================================================================
// Node tools
// ============================================================================

enterprise_tool!(read_only, list_nodes, "list_nodes",
    "List all nodes.",
    {} => |client, _input| {
        let handler = NodeHandler::new(client);
        let nodes = handler.list().await.tool_context("Failed to list nodes")?;

        CallToolResult::from_list("nodes", &nodes)
    }
);

enterprise_tool!(read_only, get_node, "get_node",
    "Get detailed information about a specific node by UID.",
    {
        /// Node UID
        pub uid: u32,
    } => |client, input| {
        let handler = NodeHandler::new(client);
        let node = handler
            .get(input.uid)
            .await
            .tool_context("Failed to get node")?;

        CallToolResult::from_serialize(&node)
    }
);

enterprise_tool!(read_only, get_node_stats, "get_node_stats",
    "Get statistics for a specific node. Optionally specify interval and time range \
     for historical data.",
    {
        /// Node UID
        pub uid: u32,
        /// Time interval for aggregation: "1sec", "10sec", "5min", "15min", "1hour", "12hour", "1week"
        #[serde(default)]
        pub interval: Option<String>,
        /// Start time for historical query (ISO 8601 format, e.g., "2024-01-15T10:00:00Z")
        #[serde(default)]
        pub start_time: Option<String>,
        /// End time for historical query (ISO 8601 format)
        #[serde(default)]
        pub end_time: Option<String>,
    } => |client, input| {
        let handler = StatsHandler::new(client);

        if input.interval.is_some()
            || input.start_time.is_some()
            || input.end_time.is_some()
        {
            let query = StatsQuery {
                interval: input.interval,
                stime: input.start_time,
                etime: input.end_time,
                metrics: None,
            };
            let stats = handler
                .node(input.uid, Some(query))
                .await
                .tool_context("Failed to get node stats")?;
            CallToolResult::from_serialize(&stats)
        } else {
            let stats = handler
                .node_last(input.uid)
                .await
                .tool_context("Failed to get node stats")?;
            CallToolResult::from_serialize(&stats)
        }
    }
);

// ============================================================================
// Node Action Operations
// ============================================================================

enterprise_tool!(write, enable_node_maintenance, "enable_enterprise_node_maintenance",
    "Enable maintenance mode on a specific node. Shards will be migrated off first.",
    {
        /// Node UID
        pub uid: u32,
    } => |client, input| {
        let handler = NodeHandler::new(client);
        let result = handler
            .execute_action(input.uid, "maintenance_on")
            .await
            .tool_context("Failed to enable node maintenance")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Node maintenance mode enabled",
            "node_uid": input.uid,
            "action_uid": result.action_uid,
            "description": result.description
        }))
    }
);

enterprise_tool!(write, disable_node_maintenance, "disable_enterprise_node_maintenance",
    "Disable maintenance mode on a specific node. The node will accept shards again.",
    {
        /// Node UID
        pub uid: u32,
    } => |client, input| {
        let handler = NodeHandler::new(client);
        let result = handler
            .execute_action(input.uid, "maintenance_off")
            .await
            .tool_context("Failed to disable node maintenance")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Node maintenance mode disabled",
            "node_uid": input.uid,
            "action_uid": result.action_uid,
            "description": result.description
        }))
    }
);

enterprise_tool!(write, rebalance_node, "rebalance_enterprise_node",
    "Rebalance shards on a specific node for optimal distribution.",
    {
        /// Node UID
        pub uid: u32,
    } => |client, input| {
        let handler = NodeHandler::new(client);
        let result = handler
            .execute_action(input.uid, "rebalance")
            .await
            .tool_context("Failed to rebalance node")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Node rebalance initiated",
            "node_uid": input.uid,
            "action_uid": result.action_uid,
            "description": result.description
        }))
    }
);

enterprise_tool!(write, drain_node, "drain_enterprise_node",
    "Drain all shards from a specific node, migrating them to other nodes.",
    {
        /// Node UID
        pub uid: u32,
    } => |client, input| {
        let handler = NodeHandler::new(client);
        let result = handler
            .execute_action(input.uid, "drain")
            .await
            .tool_context("Failed to drain node")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Node drain initiated",
            "node_uid": input.uid,
            "action_uid": result.action_uid,
            "description": result.description
        }))
    }
);

// ============================================================================
// Node Update/Remove Operations
// ============================================================================

enterprise_tool!(write, update_enterprise_node, "update_enterprise_node",
    "Update a node's configuration. Pass fields to update as JSON.",
    {
        /// Node UID
        pub uid: u32,
        /// JSON object with node settings to update
        pub updates: Value,
    } => |client, input| {
        let handler = NodeHandler::new(client);
        let node = handler
            .update(input.uid, input.updates)
            .await
            .tool_context("Failed to update node")?;

        CallToolResult::from_serialize(&node)
    }
);

enterprise_tool!(destructive, remove_enterprise_node, "remove_enterprise_node",
    "DANGEROUS: Remove a node. All shards must be drained first.",
    {
        /// Node UID
        pub uid: u32,
    } => |client, input| {
        let handler = NodeHandler::new(client);
        handler
            .remove(input.uid)
            .await
            .tool_context("Failed to remove node")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Node removed successfully",
            "uid": input.uid
        }))
    }
);

// ============================================================================
// Cluster Services
// ============================================================================

enterprise_tool!(read_only, get_enterprise_cluster_services, "get_enterprise_cluster_services",
    "Get the list of cluster services.",
    {} => |client, _input| {
        let handler = ClusterHandler::new(client);
        let services = handler
            .services_configuration()
            .await
            .tool_context("Failed to get cluster services")?;

        CallToolResult::from_serialize(&services)
    }
);
