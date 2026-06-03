//! Fixed/Essentials tier subscription and database tools for Redis Cloud

use redis_cloud::fixed::databases::{
    DatabaseTagCreateRequest, DatabaseTagUpdateRequest, DatabaseTagsUpdateRequest,
    FixedDatabaseBackupRequest, FixedDatabaseCreateRequest, FixedDatabaseHandler,
    FixedDatabaseImportRequest, FixedDatabaseUpdateRequest, Tag,
};
use redis_cloud::fixed::subscriptions::{
    FixedSubscriptionCreateRequest, FixedSubscriptionHandler, FixedSubscriptionUpdateRequest,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{CallToolResult, ResultExt};

use crate::tools::macros::{cloud_tool, mcp_module};

/// Input for a tag key-value pair
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FixedTagInput {
    /// Tag key
    pub key: String,
    /// Tag value
    pub value: String,
}

mcp_module! {
    list_fixed_subscriptions => "list_fixed_subscriptions",
    get_fixed_subscription => "get_fixed_subscription",
    create_fixed_subscription => "create_fixed_subscription",
    update_fixed_subscription => "update_fixed_subscription",
    delete_fixed_subscription => "delete_fixed_subscription",
    list_fixed_plans => "list_fixed_plans",
    get_fixed_plans_by_subscription => "get_fixed_plans_by_subscription",
    get_fixed_plan => "get_fixed_plan",
    get_fixed_redis_versions => "get_fixed_redis_versions",
    list_fixed_databases => "list_fixed_databases",
    get_fixed_database => "get_fixed_database",
    create_fixed_database => "create_fixed_database",
    update_fixed_database => "update_fixed_database",
    delete_fixed_database => "delete_fixed_database",
    get_fixed_database_backup_status => "get_fixed_database_backup_status",
    backup_fixed_database => "backup_fixed_database",
    get_fixed_database_import_status => "get_fixed_database_import_status",
    import_fixed_database => "import_fixed_database",
    get_fixed_database_slow_log => "get_fixed_database_slow_log",
    get_fixed_database_tags => "get_fixed_database_tags",
    create_fixed_database_tag => "create_fixed_database_tag",
    update_fixed_database_tag => "update_fixed_database_tag",
    delete_fixed_database_tag => "delete_fixed_database_tag",
    update_fixed_database_tags => "update_fixed_database_tags",
    get_fixed_database_upgrade_versions => "get_fixed_database_upgrade_versions",
    get_fixed_database_upgrade_status => "get_fixed_database_upgrade_status",
    upgrade_fixed_database_redis_version => "upgrade_fixed_database_redis_version",
}

// ============================================================================
// Fixed/Essentials Subscription tools
// ============================================================================

cloud_tool!(read_only, list_fixed_subscriptions, "list_fixed_subscriptions",
    "List all Fixed/Essentials tier subscriptions (pre-configured plans). \
     Use list_subscriptions for Pro subscriptions.",
    {} => |client, _input| {
        let handler = FixedSubscriptionHandler::new(client);
        let subscriptions = handler
            .list()
            .await
            .tool_context("Failed to list fixed subscriptions")?;

        CallToolResult::from_serialize(&subscriptions)
    }
);

cloud_tool!(read_only, get_fixed_subscription, "get_fixed_subscription",
    "Get details for a Fixed/Essentials subscription by ID. \
     Use get_subscription for Pro subscriptions.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = FixedSubscriptionHandler::new(client);
        let subscription = handler
            .get_by_id(input.subscription_id)
            .await
            .tool_context("Failed to get fixed subscription")?;

        CallToolResult::from_serialize(&subscription)
    }
);

cloud_tool!(write, create_fixed_subscription, "create_fixed_subscription",
    "Create a new Fixed/Essentials subscription. \
     Prerequisites: 1) list_fixed_plans -- choose a plan by size, region, and price. \
     2) list_payment_methods -- verify a payment method exists.",
    {
        /// New subscription name
        pub name: String,
        /// Essentials plan ID (use list_fixed_plans to find available plans)
        pub plan_id: i32,
        /// Payment method: "credit-card" or "marketplace"
        #[serde(default)]
        pub payment_method: Option<String>,
        /// Payment method ID (required when payment_method is "credit-card")
        #[serde(default)]
        pub payment_method_id: Option<i32>,
    } => |client, input| {
        let request = FixedSubscriptionCreateRequest {
            name: input.name,
            plan_id: input.plan_id,
            payment_method: input.payment_method,
            payment_method_id: input.payment_method_id,
            command_type: None,
        };

        let handler = FixedSubscriptionHandler::new(client);
        let result = handler
            .create(&request)
            .await
            .tool_context("Failed to create fixed subscription")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_fixed_subscription, "update_fixed_subscription",
    "Update a Fixed/Essentials subscription.",
    {
        /// Fixed subscription ID to update
        pub subscription_id: i32,
        /// Updated subscription name
        #[serde(default)]
        pub name: Option<String>,
        /// New plan ID
        #[serde(default)]
        pub plan_id: Option<i32>,
        /// Payment method: "credit-card" or "marketplace"
        #[serde(default)]
        pub payment_method: Option<String>,
        /// Payment method ID
        #[serde(default)]
        pub payment_method_id: Option<i32>,
    } => |client, input| {
        let request = FixedSubscriptionUpdateRequest {
            subscription_id: None,
            name: input.name,
            plan_id: input.plan_id,
            payment_method: input.payment_method,
            payment_method_id: input.payment_method_id,
            command_type: None,
        };

        let handler = FixedSubscriptionHandler::new(client);
        let result = handler
            .update(input.subscription_id, &request)
            .await
            .tool_context("Failed to update fixed subscription")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_fixed_subscription, "delete_fixed_subscription",
    "DANGEROUS: Delete a Fixed/Essentials subscription. \
     All databases must be deleted first.",
    {
        /// Fixed subscription ID to delete
        pub subscription_id: i32,
    } => |client, input| {
        let handler = FixedSubscriptionHandler::new(client);
        let result = handler
            .delete_by_id(input.subscription_id)
            .await
            .tool_context("Failed to delete fixed subscription")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, list_fixed_plans, "list_fixed_plans",
    "List available Fixed/Essentials plans by size, region, and price. \
     Call this before create_fixed_subscription to discover available plan IDs.",
    {
        /// Cloud provider filter (e.g., "AWS", "GCP", "Azure")
        #[serde(default)]
        pub provider: Option<String>,
        /// Filter by Redis Flex plans
        #[serde(default)]
        pub redis_flex: Option<bool>,
    } => |client, input| {
        let handler = FixedSubscriptionHandler::new(client);
        let plans = handler
            .list_plans(input.provider, input.redis_flex)
            .await
            .tool_context("Failed to list fixed plans")?;

        CallToolResult::from_serialize(&plans)
    }
);

cloud_tool!(read_only, get_fixed_plans_by_subscription, "get_fixed_plans_by_subscription",
    "Get compatible plans for a subscription.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = FixedSubscriptionHandler::new(client);
        let plans = handler
            .get_plans_by_subscription_id(input.subscription_id)
            .await
            .tool_context("Failed to get fixed plans by subscription")?;

        CallToolResult::from_serialize(&plans)
    }
);

cloud_tool!(read_only, get_fixed_plan, "get_fixed_plan",
    "Get plan details by ID.",
    {
        /// Plan ID
        pub plan_id: i32,
    } => |client, input| {
        let handler = FixedSubscriptionHandler::new(client);
        let plan = handler
            .get_plan_by_id(input.plan_id)
            .await
            .tool_context("Failed to get fixed plan")?;

        CallToolResult::from_serialize(&plan)
    }
);

cloud_tool!(read_only, get_fixed_redis_versions, "get_fixed_redis_versions",
    "Get available Redis versions for a subscription.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = FixedSubscriptionHandler::new(client);
        let versions = handler
            .get_redis_versions(input.subscription_id)
            .await
            .tool_context("Failed to get fixed Redis versions")?;

        CallToolResult::from_serialize(&versions)
    }
);

// ============================================================================
// Fixed/Essentials Database tools
// ============================================================================

cloud_tool!(read_only, list_fixed_databases, "list_fixed_databases",
    "List databases in a Fixed/Essentials subscription. \
     Use list_databases for Pro subscriptions.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Number of entries to skip (for pagination)
        #[serde(default)]
        pub offset: Option<i32>,
        /// Maximum number of entries to return
        #[serde(default)]
        pub limit: Option<i32>,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let databases = handler
            .list(input.subscription_id, input.offset, input.limit)
            .await
            .tool_context("Failed to list fixed databases")?;

        CallToolResult::from_serialize(&databases)
    }
);

cloud_tool!(read_only, get_fixed_database, "get_fixed_database",
    "Get details for a database in a Fixed/Essentials subscription by ID. \
     Use get_database for Pro subscriptions.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let database = handler
            .get_by_id(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get fixed database")?;

        CallToolResult::from_serialize(&database)
    }
);

cloud_tool!(write, create_fixed_database, "create_fixed_database",
    "Create a database in a Fixed/Essentials subscription. \
     Prerequisites: 1) get_fixed_subscription -- verify the subscription exists and is active. \
     2) get_fixed_plans_by_subscription -- check compatible plans. \
     3) get_fixed_redis_versions -- pick a supported Redis version.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database name (max 40 chars, letters, digits, and hyphens only)
        pub name: String,
        /// Database protocol: "stack" (default) or "redis" (for Redis Flex)
        #[serde(default)]
        pub protocol: Option<String>,
        /// Total memory in GB including replication overhead (Pay-as-you-go only)
        #[serde(default)]
        pub memory_limit_in_gb: Option<f64>,
        /// Maximum dataset size in GB (Pay-as-you-go only)
        #[serde(default)]
        pub dataset_size_in_gb: Option<f64>,
        /// Support Redis OSS Cluster API (Pay-as-you-go only)
        #[serde(default)]
        pub support_oss_cluster_api: Option<bool>,
        /// Redis database version
        #[serde(default)]
        pub redis_version: Option<String>,
        /// Data persistence mode (e.g., "none", "aof-every-1-second", "snapshot-every-1-hour")
        #[serde(default)]
        pub data_persistence: Option<String>,
        /// Data eviction policy
        #[serde(default)]
        pub data_eviction_policy: Option<String>,
        /// Enable replication for high availability
        #[serde(default)]
        pub replication: Option<bool>,
        /// Enable TLS for connections
        #[serde(default)]
        pub enable_tls: Option<bool>,
        /// Database password (random generated if not set)
        #[serde(default)]
        pub password: Option<String>,
        /// List of source IP addresses or subnet masks to allow
        #[serde(default)]
        pub source_ips: Option<Vec<String>>,
    } => |client, input| {
        let request = FixedDatabaseCreateRequest {
            subscription_id: None,
            name: input.name,
            protocol: input.protocol,
            memory_limit_in_gb: input.memory_limit_in_gb,
            dataset_size_in_gb: input.dataset_size_in_gb,
            support_oss_cluster_api: input.support_oss_cluster_api,
            redis_version: input.redis_version,
            resp_version: None,
            use_external_endpoint_for_oss_cluster_api: None,
            enable_database_clustering: None,
            number_of_shards: None,
            data_persistence: input.data_persistence,
            data_eviction_policy: input.data_eviction_policy,
            replication: input.replication,
            periodic_backup_path: None,
            source_ips: input.source_ips,
            regex_rules: None,
            replica_of: None,
            replica: None,
            client_ssl_certificate: None,
            client_tls_certificates: None,
            enable_tls: input.enable_tls,
            password: input.password,
            alerts: None,
            modules: None,
            command_type: None,
        };

        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .create(input.subscription_id, &request)
            .await
            .tool_context("Failed to create fixed database")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_fixed_database, "update_fixed_database",
    "Update a database in a Fixed/Essentials subscription.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID to update
        pub database_id: i32,
        /// Updated database name
        #[serde(default)]
        pub name: Option<String>,
        /// Total memory in GB including replication overhead (Pay-as-you-go only)
        #[serde(default)]
        pub memory_limit_in_gb: Option<f64>,
        /// Data persistence mode
        #[serde(default)]
        pub data_persistence: Option<String>,
        /// Data eviction policy
        #[serde(default)]
        pub data_eviction_policy: Option<String>,
        /// Enable or disable replication
        #[serde(default)]
        pub replication: Option<bool>,
        /// Enable or disable TLS
        #[serde(default)]
        pub enable_tls: Option<bool>,
        /// Updated database password
        #[serde(default)]
        pub password: Option<String>,
        /// List of source IP addresses or subnet masks to allow
        #[serde(default)]
        pub source_ips: Option<Vec<String>>,
    } => |client, input| {
        let request = FixedDatabaseUpdateRequest {
            subscription_id: None,
            database_id: None,
            name: input.name,
            memory_limit_in_gb: input.memory_limit_in_gb,
            dataset_size_in_gb: None,
            support_oss_cluster_api: None,
            resp_version: None,
            use_external_endpoint_for_oss_cluster_api: None,
            enable_database_clustering: None,
            number_of_shards: None,
            data_persistence: input.data_persistence,
            data_eviction_policy: input.data_eviction_policy,
            replication: input.replication,
            periodic_backup_path: None,
            source_ips: input.source_ips,
            replica_of: None,
            replica: None,
            regex_rules: None,
            client_ssl_certificate: None,
            client_tls_certificates: None,
            enable_tls: input.enable_tls,
            password: input.password,
            enable_default_user: None,
            alerts: None,
            command_type: None,
        };

        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .update(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to update fixed database")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_fixed_database, "delete_fixed_database",
    "DANGEROUS: Delete a Fixed/Essentials database and all its data.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID to delete
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .delete_by_id(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to delete fixed database")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Fixed/Essentials Database operations tools
// ============================================================================

cloud_tool!(read_only, get_fixed_database_backup_status, "get_fixed_database_backup_status",
    "Get latest backup status for a Fixed/Essentials database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let status = handler
            .get_backup_status(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get fixed database backup status")?;

        CallToolResult::from_serialize(&status)
    }
);

cloud_tool!(write, backup_fixed_database, "backup_fixed_database",
    "Trigger a manual backup of a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Custom backup path (overrides the configured periodicBackupPath)
        #[serde(default)]
        pub adhoc_backup_path: Option<String>,
    } => |client, input| {
        let request = FixedDatabaseBackupRequest {
            subscription_id: None,
            database_id: None,
            adhoc_backup_path: input.adhoc_backup_path,
            command_type: None,
        };

        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .backup(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to backup fixed database")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_fixed_database_import_status, "get_fixed_database_import_status",
    "Get latest import status for a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let status = handler
            .get_import_status(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get fixed database import status")?;

        CallToolResult::from_serialize(&status)
    }
);

cloud_tool!(write, import_fixed_database, "import_fixed_database",
    "Import data into a database from an external source. \
     WARNING: This will overwrite existing data.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Source type: "http", "redis", "ftp", "aws-s3", "azure-blob-storage", "google-blob-storage"
        pub source_type: String,
        /// One or more URIs to import from
        pub import_from_uri: Vec<String>,
    } => |client, input| {
        let request = FixedDatabaseImportRequest {
            subscription_id: None,
            database_id: None,
            source_type: input.source_type,
            import_from_uri: input.import_from_uri,
            command_type: None,
        };

        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .import(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to import fixed database")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_fixed_database_slow_log, "get_fixed_database_slow_log",
    "Get slow log entries for a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let log = handler
            .get_slow_log(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get fixed database slow log")?;

        CallToolResult::from_serialize(&log)
    }
);

// ============================================================================
// Fixed/Essentials Database tag tools
// ============================================================================

cloud_tool!(read_only, get_fixed_database_tags, "get_fixed_database_tags",
    "Get tags for a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let tags = handler
            .get_tags(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get fixed database tags")?;

        CallToolResult::from_serialize(&tags)
    }
);

cloud_tool!(write, create_fixed_database_tag, "create_fixed_database_tag",
    "Create a tag on a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tag key
        pub key: String,
        /// Tag value
        pub value: String,
    } => |client, input| {
        let request = DatabaseTagCreateRequest {
            key: input.key,
            value: input.value,
            subscription_id: None,
            database_id: None,
            command_type: None,
        };

        let handler = FixedDatabaseHandler::new(client);
        let tag = handler
            .create_tag(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to create fixed database tag")?;

        CallToolResult::from_serialize(&tag)
    }
);

cloud_tool!(write, update_fixed_database_tag, "update_fixed_database_tag",
    "Update a tag value on a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tag key to update
        pub tag_key: String,
        /// New tag value
        pub value: String,
    } => |client, input| {
        let request = DatabaseTagUpdateRequest {
            subscription_id: None,
            database_id: None,
            key: None,
            value: input.value,
            command_type: None,
        };

        let handler = FixedDatabaseHandler::new(client);
        let tag = handler
            .update_tag(
                input.subscription_id,
                input.database_id,
                input.tag_key,
                &request,
            )
            .await
            .tool_context("Failed to update fixed database tag")?;

        CallToolResult::from_serialize(&tag)
    }
);

cloud_tool!(destructive, delete_fixed_database_tag, "delete_fixed_database_tag",
    "DANGEROUS: Delete a tag from a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tag key to delete
        pub tag_key: String,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .delete_tag(input.subscription_id, input.database_id, input.tag_key)
            .await
            .tool_context("Failed to delete fixed database tag")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_fixed_database_tags, "update_fixed_database_tags",
    "Update all tags on a database (replaces existing tags).",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Tags to set on the database (replaces all existing tags)
        pub tags: Vec<FixedTagInput>,
    } => |client, input| {
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

        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .update_tags(input.subscription_id, input.database_id, &request)
            .await
            .tool_context("Failed to update fixed database tags")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Fixed/Essentials Database upgrade tools
// ============================================================================

cloud_tool!(read_only, get_fixed_database_upgrade_versions, "get_fixed_database_upgrade_versions",
    "Get available upgrade target Redis versions for a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let versions = handler
            .get_available_target_versions(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get fixed database upgrade versions")?;

        CallToolResult::from_serialize(&versions)
    }
);

cloud_tool!(read_only, get_fixed_database_upgrade_status, "get_fixed_database_upgrade_status",
    "Get latest Redis version upgrade status for a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let status = handler
            .get_upgrade_status(input.subscription_id, input.database_id)
            .await
            .tool_context("Failed to get fixed database upgrade status")?;

        CallToolResult::from_serialize(&status)
    }
);

cloud_tool!(write, upgrade_fixed_database_redis_version, "upgrade_fixed_database_redis_version",
    "Upgrade the Redis version of a database.",
    {
        /// Fixed subscription ID
        pub subscription_id: i32,
        /// Database ID
        pub database_id: i32,
        /// Target Redis version to upgrade to (use get_fixed_database_upgrade_versions to see available versions)
        pub target_version: String,
    } => |client, input| {
        let handler = FixedDatabaseHandler::new(client);
        let result = handler
            .upgrade_redis_version(
                input.subscription_id,
                input.database_id,
                &input.target_version,
            )
            .await
            .tool_context("Failed to upgrade fixed database Redis version")?;

        CallToolResult::from_serialize(&result)
    }
);
