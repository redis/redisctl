//! Subscription and database tools for Redis Cloud

use std::time::Duration;

use redis_cloud::databases::DatabaseCreateRequest;
use redis_cloud::flexible::{DatabaseHandler, SubscriptionHandler};
use redisctl_core::cloud::{
    backup_database_and_wait, create_database_and_wait, delete_database_and_wait,
    delete_subscription_and_wait, flush_database_and_wait, import_database_and_wait,
    update_database_and_wait,
};
use tower_mcp::{CallToolResult, ResultExt};

use crate::tools::macros::{cloud_tool, mcp_module};

// ============================================================================
// Helper functions for serde defaults
// ============================================================================

fn default_replication() -> bool {
    true
}

fn default_protocol() -> String {
    "redis".to_string()
}

fn default_timeout() -> u64 {
    600
}

fn default_import_timeout() -> u64 {
    1800 // Imports can take longer
}

fn default_flush_timeout() -> u64 {
    300
}

fn default_cloud_account_id() -> i32 {
    1 // Default internal account
}

fn default_subscription_timeout() -> u64 {
    1800 // Subscriptions can take a while
}

// ============================================================================
// Helper structs used as field types in tool inputs
// ============================================================================

/// Input for a maintenance window specification
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MaintenanceWindowInput {
    /// Start hour (0-23 UTC)
    pub start_hour: i32,
    /// Duration in hours
    pub duration_in_hours: i32,
    /// Days of the week (e.g., ["Monday", "Wednesday", "Friday"])
    pub days: Vec<String>,
}

/// Input for a region to delete from an Active-Active subscription
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ActiveActiveRegionToDeleteInput {
    /// Region name to delete (e.g., "us-east-1")
    pub region: String,
}

/// Input for a tag key-value pair
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TagInput {
    /// Tag key
    pub key: String,
    /// Tag value
    pub value: String,
}

// ============================================================================
// Module router
// ============================================================================

mcp_module! {
    list_subscriptions => "list_subscriptions",
    get_subscription => "get_subscription",
    list_databases => "list_databases",
    get_database => "get_database",
    get_backup_status => "get_backup_status",
    get_slow_log => "get_slow_log",
    get_tags => "get_database_tags",
    get_database_certificate => "get_database_certificate",
    create_database => "create_database",
    update_database => "update_database",
    delete_database => "delete_database",
    backup_database => "backup_database",
    import_database => "import_database",
    delete_subscription => "delete_subscription",
    flush_database => "flush_database",
    flush_crdb_database => "flush_crdb_database",
    create_subscription => "create_subscription",
    update_subscription => "update_subscription",
    get_subscription_pricing => "get_subscription_pricing",
    get_redis_versions => "get_redis_versions",
    get_subscription_cidr_allowlist => "get_subscription_cidr_allowlist",
    update_subscription_cidr_allowlist => "update_subscription_cidr_allowlist",
    get_subscription_maintenance_windows => "get_subscription_maintenance_windows",
    update_subscription_maintenance_windows => "update_subscription_maintenance_windows",
    get_active_active_regions => "get_active_active_regions",
    add_active_active_region => "add_active_active_region",
    delete_active_active_regions => "delete_active_active_regions",
    get_available_database_versions => "get_available_database_versions",
    upgrade_database_redis_version => "upgrade_database_redis_version",
    get_database_upgrade_status => "get_database_upgrade_status",
    get_database_import_status => "get_database_import_status",
    create_database_tag => "create_database_tag",
    update_database_tag => "update_database_tag",
    delete_database_tag => "delete_database_tag",
    update_database_tags => "update_database_tags",
    update_crdb_local_properties => "update_crdb_local_properties",
}

// ============================================================================
// Read-only tools
// ============================================================================

cloud_tool!(read_only, list_subscriptions, "list_subscriptions",
    "List all Pro (Flexible) subscriptions. Use list_fixed_subscriptions for Essentials/Fixed subscriptions.",
    {} => |client, _input| {
        let handler = SubscriptionHandler::new(client);
        let account_subs = handler
            .get_all_subscriptions()
            .await
            .tool_context("Failed to list subscriptions")?;

        CallToolResult::from_serialize(&account_subs)
    }
);

cloud_tool!(read_only, get_subscription, "get_subscription",
    "Get Pro subscription details by ID. Use get_fixed_subscription for Essentials/Fixed subscriptions.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = SubscriptionHandler::new(client);
        let subscription = handler
            .get_subscription_by_id(input.subscription_id)
            .await
            .tool_context("Failed to get subscription")?;

        CallToolResult::from_serialize(&subscription)
    }
);

cloud_tool!(read_only, list_databases, "list_databases",
    "List all databases in a Pro (Flexible) subscription. Use list_fixed_databases for Essentials/Fixed subscriptions.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let databases = handler
            .get_subscription_databases(input.subscription_id, None, None)
            .await
            .tool_context("Failed to list databases")?;

        CallToolResult::from_serialize(&databases)
    }
);

cloud_tool!(read_only, get_database, "get_database",
    "Get database details by ID.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let database = handler
            .get_subscription_database_by_id(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get database")?;

        CallToolResult::from_serialize(&database)
    }
);

cloud_tool!(read_only, get_backup_status, "get_backup_status",
    "Get backup status and history for a database. The region_name parameter is required for Active-Active databases.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Optional region name for Active-Active databases
        #[serde(default)]
        pub region_name: Option<String>,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let status = handler
            .get_database_backup_status(
                input.subscription_id,
                input.database_id,
                input.region_name,
            )
            .await
            .tool_context("Failed to get backup status")?;

        CallToolResult::from_serialize(&status)
    }
);

cloud_tool!(read_only, get_slow_log, "get_slow_log",
    "Get slow log entries for a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Optional region name for Active-Active databases
        #[serde(default)]
        pub region_name: Option<String>,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let log = handler
            .get_slow_log(input.subscription_id, input.database_id, input.region_name)
            .await
            .tool_context("Failed to get slow log")?;

        CallToolResult::from_serialize(&log)
    }
);

cloud_tool!(read_only, get_tags, "get_database_tags",
    "Get tags for a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let tags = handler
            .get_tags(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get tags")?;

        CallToolResult::from_serialize(&tags)
    }
);

cloud_tool!(read_only, get_database_certificate, "get_database_certificate",
    "Get the TLS/SSL certificate for a database in PEM format.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let cert = handler
            .get_subscription_database_certificate(
                input.subscription_id,
                input.database_id,
            )
            .await
            .tool_context("Failed to get certificate")?;

        CallToolResult::from_serialize(&cert)
    }
);

cloud_tool!(read_only, get_subscription_pricing, "get_subscription_pricing",
    "Get pricing details for a subscription.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = SubscriptionHandler::new(client);
        let pricing = handler
            .get_subscription_pricing(input.subscription_id)
            .await
            .tool_context("Failed to get subscription pricing")?;

        CallToolResult::from_serialize(&pricing)
    }
);

cloud_tool!(read_only, get_redis_versions, "get_redis_versions",
    "Get available Redis versions. Optionally filter by subscription ID.",
    {
        /// Optional subscription ID to filter versions
        #[serde(default)]
        pub subscription_id: Option<i32>,
    } => |client, input| {
        let handler = SubscriptionHandler::new(client);
        let versions = handler
            .get_redis_versions(input.subscription_id)
            .await
            .tool_context("Failed to get Redis versions")?;

        CallToolResult::from_serialize(&versions)
    }
);

cloud_tool!(read_only, get_subscription_cidr_allowlist, "get_subscription_cidr_allowlist",
    "Get the CIDR allowlist for a subscription.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = SubscriptionHandler::new(client);
        let allowlist = handler
            .get_cidr_allowlist(input.subscription_id)
            .await
            .tool_context("Failed to get subscription CIDR allowlist")?;

        CallToolResult::from_serialize(&allowlist)
    }
);

cloud_tool!(read_only, get_subscription_maintenance_windows, "get_subscription_maintenance_windows",
    "Get maintenance windows for a subscription.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = SubscriptionHandler::new(client);
        let windows = handler
            .get_subscription_maintenance_windows(input.subscription_id)
            .await
            .tool_context("Failed to get subscription maintenance windows")?;

        CallToolResult::from_serialize(&windows)
    }
);

cloud_tool!(read_only, get_active_active_regions, "get_active_active_regions",
    "Get regions from an Active-Active subscription.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = SubscriptionHandler::new(client);
        let regions = handler
            .get_regions_from_active_active_subscription(input.subscription_id)
            .await
            .tool_context("Failed to get Active-Active regions")?;

        CallToolResult::from_serialize(&regions)
    }
);

cloud_tool!(read_only, get_available_database_versions, "get_available_database_versions",
    "Get available target Redis versions for upgrading a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let versions = handler
            .get_available_target_versions(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get available database versions")?;

        CallToolResult::from_serialize(&versions)
    }
);

cloud_tool!(read_only, get_database_upgrade_status, "get_database_upgrade_status",
    "Get the Redis version upgrade status for a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let status = handler
            .get_database_redis_version_upgrade_status(
                input.subscription_id,
                input.database_id,
            )
            .await
            .tool_context("Failed to get database upgrade status")?;

        CallToolResult::from_serialize(&status)
    }
);

cloud_tool!(read_only, get_database_import_status, "get_database_import_status",
    "Get the import status for a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let status = handler
            .get_database_import_status(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get database import status")?;

        CallToolResult::from_serialize(&status)
    }
);

// ============================================================================
// Write tools (require write permission)
// ============================================================================

cloud_tool!(write, create_database, "create_database",
    "Create a new database in a Pro subscription and wait for it to be ready. \
     Prerequisites: 1) get_subscription -- verify the target subscription exists and is active. \
     2) get_modules -- validate desired modules. \
     3) get_redis_versions -- pick a supported Redis version.",
    {
        /// Subscription ID to create the database in
        pub subscription_id: i32,
        /// Database name
        pub name: String,
        /// Memory limit in GB (e.g., 1.0, 2.5, 10.0)
        pub memory_limit_in_gb: f64,
        /// Enable replication for high availability (default: true)
        #[serde(default = "default_replication")]
        pub replication: bool,
        /// Protocol: "redis" (RESP2), "stack" (RESP2 with modules), or "memcached"
        #[serde(default = "default_protocol")]
        pub protocol: String,
        /// Data persistence: "none", "aof-every-1-second", "aof-every-write", "snapshot-every-1-hour", etc.
        #[serde(default)]
        pub data_persistence: Option<String>,
        /// Timeout in seconds to wait for database creation (default: 600)
        #[serde(default = "default_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Build the request using Layer 1's TypedBuilder
        let request = match (input.protocol.as_str(), input.data_persistence.as_ref()) {
            ("redis", None) => DatabaseCreateRequest::builder()
                .name(&input.name)
                .memory_limit_in_gb(input.memory_limit_in_gb)
                .replication(input.replication)
                .build(),
            ("redis", Some(persistence)) => DatabaseCreateRequest::builder()
                .name(&input.name)
                .memory_limit_in_gb(input.memory_limit_in_gb)
                .replication(input.replication)
                .data_persistence(persistence)
                .build(),
            (protocol, None) => DatabaseCreateRequest::builder()
                .name(&input.name)
                .memory_limit_in_gb(input.memory_limit_in_gb)
                .replication(input.replication)
                .protocol(protocol)
                .build(),
            (protocol, Some(persistence)) => DatabaseCreateRequest::builder()
                .name(&input.name)
                .memory_limit_in_gb(input.memory_limit_in_gb)
                .replication(input.replication)
                .protocol(protocol)
                .data_persistence(persistence)
                .build(),
        };

        // Use Layer 2 workflow - no progress callback needed for MCP
        let database = create_database_and_wait(
            &client,
            input.subscription_id,
            &request,
            Duration::from_secs(input.timeout_seconds),
            None, // MCP doesn't need progress callbacks
        )
        .await
        .tool_context("Failed to create database")?;

        CallToolResult::from_serialize(&database)
    }
);

cloud_tool!(write, update_database, "update_database",
    "Update a database configuration. \
     Valid values for data_persistence: \"none\", \"aof-every-1-second\", \"aof-every-write\", \
     \"snapshot-every-1-hour\", \"snapshot-every-6-hours\", \"snapshot-every-12-hours\". \
     Valid values for data_eviction_policy: \"noeviction\", \"allkeys-lru\", \"volatile-lru\", \
     \"allkeys-lfu\", \"volatile-lfu\", \"allkeys-random\", \"volatile-random\", \"volatile-ttl\".",
    {
        /// Subscription ID containing the database
        pub subscription_id: i32,
        /// Database ID to update
        pub database_id: i32,
        /// New database name (optional)
        #[serde(default)]
        pub name: Option<String>,
        /// New memory limit in GB (optional)
        #[serde(default)]
        pub memory_limit_in_gb: Option<f64>,
        /// Change replication setting (optional)
        #[serde(default)]
        pub replication: Option<bool>,
        /// Change data persistence (optional)
        #[serde(default)]
        pub data_persistence: Option<String>,
        /// Change eviction policy (optional)
        #[serde(default)]
        pub data_eviction_policy: Option<String>,
        /// Timeout in seconds (default: 600)
        #[serde(default = "default_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        use redis_cloud::databases::DatabaseUpdateRequest;

        // Build the update request
        let mut request = DatabaseUpdateRequest::builder().build();
        request.name = input.name.clone();
        request.memory_limit_in_gb = input.memory_limit_in_gb;
        request.replication = input.replication;
        request.data_persistence = input.data_persistence.clone();
        request.data_eviction_policy = input.data_eviction_policy.clone();

        // Validate at least one field is set
        if request.name.is_none()
            && request.memory_limit_in_gb.is_none()
            && request.replication.is_none()
            && request.data_persistence.is_none()
            && request.data_eviction_policy.is_none()
        {
            return Err(tower_mcp::Error::tool(
                "At least one update field is required",
            ));
        }

        // Use Layer 2 workflow
        let database = update_database_and_wait(
            &client,
            input.subscription_id,
            input.database_id,
            &request,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to update database")?;

        CallToolResult::from_serialize(&database)
    }
);

cloud_tool!(write, backup_database, "backup_database",
    "Trigger a manual backup of a database.",
    {
        /// Subscription ID containing the database
        pub subscription_id: i32,
        /// Database ID to backup
        pub database_id: i32,
        /// Region name (required for Active-Active databases)
        #[serde(default)]
        pub region_name: Option<String>,
        /// Timeout in seconds (default: 600)
        #[serde(default = "default_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Use Layer 2 workflow
        backup_database_and_wait(
            &client,
            input.subscription_id,
            input.database_id,
            input.region_name.as_deref(),
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to backup database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Backup completed successfully",
            "subscription_id": input.subscription_id,
            "database_id": input.database_id
        }))
    }
);

cloud_tool!(write, import_database, "import_database",
    "Import data into a database from an external source. WARNING: This will overwrite existing data.",
    {
        /// Subscription ID containing the database
        pub subscription_id: i32,
        /// Database ID to import into
        pub database_id: i32,
        /// Source type: "http", "redis", "ftp", "aws-s3", "azure-blob-storage", "google-blob-storage"
        pub source_type: String,
        /// URI to import from
        pub import_from_uri: String,
        /// Timeout in seconds (default: 1800 for imports)
        #[serde(default = "default_import_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        use redis_cloud::databases::DatabaseImportRequest;

        // Build the import request
        let request = DatabaseImportRequest::builder()
            .source_type(&input.source_type)
            .import_from_uri(vec![input.import_from_uri.clone()])
            .build();

        // Use Layer 2 workflow
        import_database_and_wait(
            &client,
            input.subscription_id,
            input.database_id,
            &request,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to import database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Import completed successfully",
            "subscription_id": input.subscription_id,
            "database_id": input.database_id
        }))
    }
);

cloud_tool!(write, create_subscription, "create_subscription",
    "Create a new Pro subscription with an initial database. \
     Prerequisites: 1) list_payment_methods -- verify a payment method exists. \
     2) get_regions -- validate the target cloud provider and region. \
     3) get_modules -- confirm desired database modules are available. \
     4) get_redis_versions -- pick a supported Redis version.",
    {
        /// Subscription name
        pub name: String,
        /// Cloud provider: "AWS", "GCP", or "Azure"
        pub cloud_provider: String,
        /// Cloud region (e.g., "us-east-1" for AWS, "us-central1" for GCP)
        pub region: String,
        /// Cloud account ID (use list_cloud_accounts to find available accounts, or use 1 for internal account)
        #[serde(default = "default_cloud_account_id")]
        pub cloud_account_id: i32,
        /// Database name for the initial database
        pub database_name: String,
        /// Memory limit in GB for the initial database
        pub memory_limit_in_gb: f64,
        /// Database protocol: "redis" (default), "stack", or "memcached"
        #[serde(default = "default_protocol")]
        pub protocol: String,
        /// Enable replication for high availability (default: true)
        #[serde(default = "default_replication")]
        pub replication: bool,
        /// Timeout in seconds (default: 1800 - subscriptions take longer)
        #[serde(default = "default_subscription_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        use redis_cloud::flexible::subscriptions::{
            SubscriptionCreateRequest, SubscriptionDatabaseSpec, SubscriptionRegionSpec,
            SubscriptionSpec,
        };
        use redisctl_core::cloud::create_subscription_and_wait;

        // Build the subscription request
        let request = SubscriptionCreateRequest::builder()
            .name(&input.name)
            .cloud_providers(vec![SubscriptionSpec {
                provider: Some(input.cloud_provider.clone()),
                cloud_account_id: Some(input.cloud_account_id),
                regions: vec![SubscriptionRegionSpec {
                    region: input.region.clone(),
                    multiple_availability_zones: None,
                    preferred_availability_zones: None,
                    networking: None,
                }],
            }])
            .databases(vec![SubscriptionDatabaseSpec {
                name: input.database_name.clone(),
                protocol: input.protocol.clone(),
                memory_limit_in_gb: Some(input.memory_limit_in_gb),
                dataset_size_in_gb: None,
                support_oss_cluster_api: None,
                data_persistence: None,
                replication: Some(input.replication),
                throughput_measurement: None,
                local_throughput_measurement: None,
                modules: None,
                quantity: None,
                average_item_size_in_bytes: None,
                resp_version: None,
                redis_version: None,
                sharding_type: None,
                query_performance_factor: None,
            }])
            .build();

        // Use Layer 2 workflow
        let subscription = create_subscription_and_wait(
            &client,
            &request,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to create subscription")?;

        CallToolResult::from_serialize(&subscription)
    }
);

cloud_tool!(write, update_subscription, "update_subscription",
    "Update a Pro subscription's name or payment method. \
     At least one of name, payment_method_id, or payment_method must be provided. \
     Use list_payment_methods to find valid payment method IDs.",
    {
        /// Subscription ID to update
        pub subscription_id: i32,
        /// New subscription name (optional)
        #[serde(default)]
        pub name: Option<String>,
        /// Payment method ID to use for this subscription (optional). \
        /// Required when payment_method is "credit-card". \
        /// Use list_payment_methods to get valid IDs.
        #[serde(default)]
        pub payment_method_id: Option<i32>,
        /// Payment method: "credit-card" or "marketplace" (optional)
        #[serde(default)]
        pub payment_method: Option<String>,
    } => |client, input| {
        use redis_cloud::flexible::subscriptions::SubscriptionUpdateRequest;
        use redis_cloud::types::TaskStateUpdate;

        if input.name.is_none() && input.payment_method_id.is_none() && input.payment_method.is_none() {
            return Err(tower_mcp::Error::tool(
                "At least one of name, payment_method_id, or payment_method must be provided",
            ));
        }

        let request = SubscriptionUpdateRequest {
            name: input.name.clone(),
            payment_method_id: input.payment_method_id,
            payment_method: input.payment_method.clone(),
            subscription_id: None,
            command_type: None,
        };

        let result: TaskStateUpdate = client
            .put(&format!("/subscriptions/{}", input.subscription_id), &request)
            .await
            .map_err(|e| tower_mcp::Error::tool(format!("Failed to update subscription: {e}")))?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_subscription_cidr_allowlist, "update_subscription_cidr_allowlist",
    "Update the CIDR allowlist for a subscription.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// List of CIDR IP ranges to allow (e.g., ["192.168.1.0/24", "10.0.0.0/8"])
        #[serde(default)]
        pub cidr_ips: Option<Vec<String>>,
        /// List of security group IDs to allow (AWS only)
        #[serde(default)]
        pub security_group_ids: Option<Vec<String>>,
    } => |client, input| {
        use redis_cloud::flexible::subscriptions::CidrAllowlistUpdateRequest;

        let request = CidrAllowlistUpdateRequest {
            subscription_id: None,
            cidr_ips: input.cidr_ips.clone(),
            security_group_ids: input.security_group_ids.clone(),
            command_type: None,
        };

        let handler = SubscriptionHandler::new(client);
        let result = handler
            .update_subscription_cidr_allowlist(input.subscription_id, &request)
            .await
            .tool_context("Failed to update subscription CIDR allowlist")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_subscription_maintenance_windows, "update_subscription_maintenance_windows",
    "Update maintenance windows for a subscription.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Maintenance mode: "manual" or "automatic"
        pub mode: String,
        /// Maintenance windows (required when mode is "manual")
        #[serde(default)]
        pub windows: Option<Vec<MaintenanceWindowInput>>,
    } => |client, input| {
        use redis_cloud::flexible::subscriptions::{
            MaintenanceWindowSpec, SubscriptionMaintenanceWindowsSpec,
        };

        let windows = input.windows.map(|ws| {
            ws.into_iter()
                .map(|w| MaintenanceWindowSpec {
                    start_hour: w.start_hour,
                    duration_in_hours: w.duration_in_hours,
                    days: w.days,
                })
                .collect()
        });

        let request = SubscriptionMaintenanceWindowsSpec {
            mode: input.mode.clone(),
            windows,
        };

        let handler = SubscriptionHandler::new(client);
        let result = handler
            .update_subscription_maintenance_windows(input.subscription_id, &request)
            .await
            .tool_context("Failed to update subscription maintenance windows")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, add_active_active_region, "add_active_active_region",
    "Add a new region to an Active-Active subscription.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Deployment CIDR for the new region (e.g., "10.0.0.0/24")
        pub deployment_cidr: String,
        /// Region name (e.g., "us-east-1")
        #[serde(default)]
        pub region: Option<String>,
        /// VPC ID for the region
        #[serde(default)]
        pub vpc_id: Option<String>,
        /// Whether to perform a dry run without making changes
        #[serde(default)]
        pub dry_run: Option<bool>,
        /// RESP version for the region
        #[serde(default)]
        pub resp_version: Option<String>,
    } => |client, input| {
        use redis_cloud::flexible::subscriptions::ActiveActiveRegionCreateRequest;

        let request = ActiveActiveRegionCreateRequest {
            subscription_id: None,
            region: input.region.clone(),
            vpc_id: input.vpc_id.clone(),
            deployment_cidr: input.deployment_cidr.clone(),
            dry_run: input.dry_run,
            databases: None,
            resp_version: input.resp_version.clone(),
            customer_managed_key_resource_name: None,
            command_type: None,
        };

        let handler = SubscriptionHandler::new(client);
        let result = handler
            .add_new_region_to_active_active_subscription(
                input.subscription_id,
                &request,
            )
            .await
            .tool_context("Failed to add Active-Active region")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, upgrade_database_redis_version, "upgrade_database_redis_version",
    "Upgrade the Redis version of a database. \
     Use get_available_database_versions to find valid target versions.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Target Redis version to upgrade to (e.g., "7.2")
        pub target_redis_version: String,
    } => |client, input| {
        use redis_cloud::databases::DatabaseUpgradeRedisVersionRequest;

        let request = DatabaseUpgradeRedisVersionRequest {
            database_id: None,
            subscription_id: None,
            target_redis_version: input.target_redis_version.clone(),
            command_type: None,
        };

        let handler = DatabaseHandler::new(client);
        let result = handler
            .upgrade_database_redis_version(
                input.subscription_id,
                input.database_id,
                &request,
            )
            .await
            .tool_context("Failed to upgrade database Redis version")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_database_tag, "create_database_tag",
    "Create a tag on a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tag key
        pub key: String,
        /// Tag value
        pub value: String,
    } => |client, input| {
        use redis_cloud::databases::DatabaseTagCreateRequest;

        let request = DatabaseTagCreateRequest {
            key: input.key.clone(),
            value: input.value.clone(),
            subscription_id: None,
            database_id: None,
            command_type: None,
        };

        let handler = DatabaseHandler::new(client);
        let tag = handler
            .create_tag(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to create database tag")?;

        CallToolResult::from_serialize(&tag)
    }
);

cloud_tool!(write, update_database_tag, "update_database_tag",
    "Update a tag on a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tag key to update
        pub tag_key: String,
        /// New tag value
        pub value: String,
    } => |client, input| {
        use redis_cloud::databases::DatabaseTagUpdateRequest;

        let request = DatabaseTagUpdateRequest {
            subscription_id: None,
            database_id: None,
            key: None,
            value: input.value.clone(),
            command_type: None,
        };

        let handler = DatabaseHandler::new(client);
        let tag = handler
            .update_tag(
                input.subscription_id,
                input.database_id,
                input.tag_key.clone(),
                &request,
            )
            .await
            .tool_context("Failed to update database tag")?;

        CallToolResult::from_serialize(&tag)
    }
);

cloud_tool!(write, update_database_tags, "update_database_tags",
    "Update all tags on a database (replaces existing tags).",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tags to set on the database (replaces all existing tags)
        pub tags: Vec<TagInput>,
    } => |client, input| {
        use redis_cloud::databases::{DatabaseTagsUpdateRequest, Tag};

        let tags = input
            .tags
            .into_iter()
            .map(|t| Tag {
                key: t.key,
                value: t.value,
                command_type: None,
            })
            .collect();

        let request = DatabaseTagsUpdateRequest {
            subscription_id: None,
            database_id: None,
            tags,
            command_type: None,
        };

        let handler = DatabaseHandler::new(client);
        let result = handler
            .update_tags(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to update database tags")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_crdb_local_properties, "update_crdb_local_properties",
    "Update local properties of an Active-Active (CRDB) database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Updated database name
        #[serde(default)]
        pub name: Option<String>,
        /// Whether to perform a dry run without making changes
        #[serde(default)]
        pub dry_run: Option<bool>,
        /// Total memory limit in GB including replication overhead
        #[serde(default)]
        pub memory_limit_in_gb: Option<f64>,
        /// Maximum dataset size in GB
        #[serde(default)]
        pub dataset_size_in_gb: Option<f64>,
        /// Enable OSS Cluster API support
        #[serde(default)]
        pub support_oss_cluster_api: Option<bool>,
        /// Use external endpoint for OSS Cluster API
        #[serde(default)]
        pub use_external_endpoint_for_oss_cluster_api: Option<bool>,
        /// Enable TLS for connections
        #[serde(default)]
        pub enable_tls: Option<bool>,
        /// Global data persistence setting for all regions
        #[serde(default)]
        pub global_data_persistence: Option<String>,
        /// Global password for all regions
        #[serde(default)]
        pub global_password: Option<String>,
        /// Global source IP allowlist for all regions
        #[serde(default)]
        pub global_source_ip: Option<Vec<String>>,
        /// Data eviction policy
        #[serde(default)]
        pub data_eviction_policy: Option<String>,
    } => |client, input| {
        use redis_cloud::databases::CrdbUpdatePropertiesRequest;

        let request = CrdbUpdatePropertiesRequest {
            subscription_id: None,
            database_id: None,
            name: input.name.clone(),
            dry_run: input.dry_run,
            memory_limit_in_gb: input.memory_limit_in_gb,
            dataset_size_in_gb: input.dataset_size_in_gb,
            support_oss_cluster_api: input.support_oss_cluster_api,
            use_external_endpoint_for_oss_cluster_api: input
                .use_external_endpoint_for_oss_cluster_api,
            client_ssl_certificate: None,
            client_tls_certificates: None,
            enable_tls: input.enable_tls,
            global_data_persistence: input.global_data_persistence.clone(),
            global_password: input.global_password.clone(),
            global_source_ip: input.global_source_ip.clone(),
            global_alerts: None,
            regions: None,
            data_eviction_policy: input.data_eviction_policy.clone(),
            command_type: None,
        };

        let handler = DatabaseHandler::new(client);
        let result = handler
            .update_crdb_local_properties(
                input.subscription_id,
                input.database_id,
                &request,
            )
            .await
            .tool_context("Failed to update CRDB local properties")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Destructive tools (require destructive permission)
// ============================================================================

cloud_tool!(destructive, delete_database, "delete_database",
    "DANGEROUS: Permanently delete a database and all its data. This operation is irreversible. Recommended: run backup_database first.",
    {
        /// Subscription ID containing the database
        pub subscription_id: i32,
        /// Database ID to delete
        pub database_id: i32,
        /// Timeout in seconds (default: 600)
        #[serde(default = "default_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Use Layer 2 workflow
        delete_database_and_wait(
            &client,
            input.subscription_id,
            input.database_id,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to delete database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Database deleted successfully",
            "subscription_id": input.subscription_id,
            "database_id": input.database_id
        }))
    }
);

cloud_tool!(destructive, delete_subscription, "delete_subscription",
    "DANGEROUS: Permanently delete a subscription. This operation is irreversible. All databases must be deleted first (use delete_database for each database in the subscription).",
    {
        /// Subscription ID to delete
        pub subscription_id: i32,
        /// Timeout in seconds (default: 600)
        #[serde(default = "default_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Use Layer 2 workflow
        delete_subscription_and_wait(
            &client,
            input.subscription_id,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to delete subscription")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Subscription deleted successfully",
            "subscription_id": input.subscription_id
        }))
    }
);

cloud_tool!(destructive, flush_database, "flush_database",
    "DANGEROUS: Permanently remove all data from a database. This operation is irreversible — all keys are deleted and cannot be recovered. Recommended: run backup_database first.",
    {
        /// Subscription ID containing the database
        pub subscription_id: i32,
        /// Database ID to flush
        pub database_id: i32,
        /// Timeout in seconds (default: 300)
        #[serde(default = "default_flush_timeout")]
        pub timeout_seconds: u64,
    } => |client, input| {
        // Use Layer 2 workflow
        flush_database_and_wait(
            &client,
            input.subscription_id,
            input.database_id,
            Duration::from_secs(input.timeout_seconds),
            None,
        )
        .await
        .tool_context("Failed to flush database")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Database flushed successfully",
            "subscription_id": input.subscription_id,
            "database_id": input.database_id
        }))
    }
);

cloud_tool!(destructive, flush_crdb_database, "flush_crdb_database",
    "DANGEROUS: Permanently remove all data from an Active-Active (CRDB) database across ALL regions. \
     This operation is irreversible — all keys are deleted globally and cannot be recovered. \
     Use this instead of flush_database for Active-Active databases. \
     Recommended: back up all regions before running.",
    {
        /// Subscription ID containing the database
        pub subscription_id: i32,
        /// Database ID to flush
        pub database_id: i32,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let request = redis_cloud::databases::CrdbFlushRequest {
            subscription_id: None,
            database_id: None,
            command_type: None,
        };
        let result = handler
            .flush_crdb(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to flush CRDB database")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_active_active_regions, "delete_active_active_regions",
    "DANGEROUS: Remove regions from an Active-Active subscription. May cause data loss in removed regions.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Regions to delete
        pub regions: Vec<ActiveActiveRegionToDeleteInput>,
        /// Whether to perform a dry run without making changes
        #[serde(default)]
        pub dry_run: Option<bool>,
    } => |client, input| {
        use redis_cloud::flexible::subscriptions::{
            ActiveActiveRegionDeleteRequest, ActiveActiveRegionToDelete,
        };

        let regions = input
            .regions
            .into_iter()
            .map(|r| ActiveActiveRegionToDelete {
                region: Some(r.region),
            })
            .collect();

        let request = ActiveActiveRegionDeleteRequest {
            subscription_id: None,
            regions: Some(regions),
            dry_run: input.dry_run,
            command_type: None,
        };

        let handler = SubscriptionHandler::new(client);
        let result = handler
            .delete_regions_from_active_active_subscription(input.subscription_id, &request)
            .await
            .tool_context("Failed to delete Active-Active regions")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_database_tag, "delete_database_tag",
    "DANGEROUS: Delete a tag from a database.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tag key to delete
        pub tag_key: String,
    } => |client, input| {
        let handler = DatabaseHandler::new(client);
        let result = handler
            .delete_tag(input.subscription_id, input.database_id, input.tag_key.clone())
            .await
            .tool_context("Failed to delete database tag")?;

        CallToolResult::from_serialize(&result)
    }
);
