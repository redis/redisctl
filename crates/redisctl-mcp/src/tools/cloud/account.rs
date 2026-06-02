//! Account, ACL, task, cloud account (BYOC), and user tools for Redis Cloud

use redis_cloud::acl::{
    AclRedisRuleCreateRequest, AclRedisRuleUpdateRequest, AclRoleCreateRequest,
    AclRoleDatabaseSpec, AclRoleRedisRuleSpec, AclRoleUpdateRequest, AclUserCreateRequest,
    AclUserUpdateRequest,
};
use redis_cloud::cloud_accounts::{CloudAccountCreateRequest, CloudAccountUpdateRequest};
use redis_cloud::types::TaskStatus;
use redis_cloud::users::AccountUserUpdateRequest;
use redis_cloud::{
    AccountHandler, AclHandler, CloudAccountHandler, CostReportCreateRequest, CostReportHandler,
    TaskHandler, UserHandler,
};
use tower_mcp::{CallToolResult, ResultExt};

use crate::tools::macros::{cloud_tool, mcp_module};

/// Database specification for ACL role assignment
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DatabaseSpec {
    /// Subscription ID for the database
    pub subscription_id: i32,
    /// Database ID
    pub database_id: i32,
    /// Optional list of regions for Active-Active databases
    #[serde(default)]
    pub regions: Option<Vec<String>>,
}

/// Redis rule specification for role creation/update
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RedisRuleSpec {
    /// Redis ACL rule name. Use list_redis_rules to get available rules.
    pub rule_name: String,
    /// List of databases where this rule applies
    pub databases: Vec<DatabaseSpec>,
}

fn default_task_timeout() -> u64 {
    300
}

fn default_task_interval() -> u64 {
    5
}

mcp_module! {
    get_account => "get_account",
    get_system_logs => "get_system_logs",
    get_session_logs => "get_session_logs",
    get_regions => "get_regions",
    get_modules => "get_modules",
    list_account_users => "list_account_users",
    get_account_user => "get_account_user",
    update_account_user => "update_account_user",
    delete_account_user => "delete_account_user",
    list_acl_users => "list_acl_users",
    get_acl_user => "get_acl_user",
    list_acl_roles => "list_acl_roles",
    list_redis_rules => "list_redis_rules",
    create_acl_user => "create_acl_user",
    update_acl_user => "update_acl_user",
    delete_acl_user => "delete_acl_user",
    create_acl_role => "create_acl_role",
    update_acl_role => "update_acl_role",
    delete_acl_role => "delete_acl_role",
    create_redis_rule => "create_redis_rule",
    update_redis_rule => "update_redis_rule",
    delete_redis_rule => "delete_redis_rule",
    generate_cost_report => "generate_cost_report",
    download_cost_report => "download_cost_report",
    list_payment_methods => "list_payment_methods",
    list_tasks => "list_tasks",
    get_task => "get_task",
    wait_for_cloud_task => "wait_for_cloud_task",
    list_cloud_accounts => "list_cloud_accounts",
    get_cloud_account => "get_cloud_account",
    create_cloud_account => "create_cloud_account",
    update_cloud_account => "update_cloud_account",
    delete_cloud_account => "delete_cloud_account",
}

// ============================================================================
// Account tools
// ============================================================================

cloud_tool!(read_only, get_account, "get_account",
    "Get current account information.",
    {} => |client, _input| {
        let handler = AccountHandler::new(client);
        let account = handler
            .get_current_account()
            .await
            .tool_context("Failed to get account")?;

        CallToolResult::from_serialize(&account)
    }
);

cloud_tool!(read_only, get_system_logs, "get_system_logs",
    "Get system audit logs.",
    {
        /// Number of entries to skip (for pagination)
        #[serde(default)]
        pub offset: Option<i32>,
        /// Maximum number of entries to return
        #[serde(default)]
        pub limit: Option<i32>,
    } => |client, input| {
        let handler = AccountHandler::new(client);
        let logs = handler
            .get_account_system_logs(input.offset, input.limit)
            .await
            .tool_context("Failed to get system logs")?;

        CallToolResult::from_serialize(&logs)
    }
);

cloud_tool!(read_only, get_session_logs, "get_session_logs",
    "Get session activity logs.",
    {
        /// Number of entries to skip (for pagination)
        #[serde(default)]
        pub offset: Option<i32>,
        /// Maximum number of entries to return
        #[serde(default)]
        pub limit: Option<i32>,
    } => |client, input| {
        let handler = AccountHandler::new(client);
        let logs = handler
            .get_account_session_logs(input.offset, input.limit)
            .await
            .tool_context("Failed to get session logs")?;

        CallToolResult::from_serialize(&logs)
    }
);

cloud_tool!(read_only, get_regions, "get_regions",
    "Get supported cloud regions. Optionally filter by provider.",
    {
        /// Optional cloud provider filter (e.g., "AWS", "GCP", "Azure")
        #[serde(default)]
        pub provider: Option<String>,
    } => |client, input| {
        let handler = AccountHandler::new(client);
        let regions = handler
            .get_supported_regions(input.provider)
            .await
            .tool_context("Failed to get regions")?;

        CallToolResult::from_serialize(&regions)
    }
);

cloud_tool!(read_only, get_modules, "get_modules",
    "Get supported database modules.",
    {} => |client, _input| {
        let handler = AccountHandler::new(client);
        let modules = handler
            .get_supported_database_modules()
            .await
            .tool_context("Failed to get modules")?;

        CallToolResult::from_serialize(&modules)
    }
);

// ============================================================================
// Account Users tools
// ============================================================================

cloud_tool!(read_only, list_account_users, "list_account_users",
    "List all account users (team members with console access).",
    {} => |client, _input| {
        let handler = UserHandler::new(client);
        let users = handler
            .get_all_users()
            .await
            .tool_context("Failed to list users")?;

        CallToolResult::from_serialize(&users)
    }
);

cloud_tool!(read_only, get_account_user, "get_account_user",
    "Get an account user by ID.",
    {
        /// Account user ID
        pub user_id: i32,
    } => |client, input| {
        let handler = UserHandler::new(client);
        let user = handler
            .get_user_by_id(input.user_id)
            .await
            .tool_context("Failed to get account user")?;

        CallToolResult::from_serialize(&user)
    }
);

cloud_tool!(write, update_account_user, "update_account_user",
    "Update an account user's name or role.",
    {
        /// Account user ID to update
        pub user_id: i32,
        /// Updated name for the user
        pub name: String,
        /// Updated role (e.g., "owner", "member", "viewer")
        #[serde(default)]
        pub role: Option<String>,
    } => |client, input| {
        let handler = UserHandler::new(client);
        let request = AccountUserUpdateRequest {
            user_id: None,
            name: input.name,
            role: input.role,
            command_type: None,
        };
        let result = handler
            .update_user(input.user_id, &request)
            .await
            .tool_context("Failed to update account user")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_account_user, "delete_account_user",
    "DANGEROUS: Delete an account user. The user will lose all access.",
    {
        /// Account user ID to delete
        pub user_id: i32,
    } => |client, input| {
        let handler = UserHandler::new(client);
        let result = handler
            .delete_user_by_id(input.user_id)
            .await
            .tool_context("Failed to delete account user")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// ACL tools (database-level access control)
// ============================================================================

cloud_tool!(read_only, list_acl_users, "list_acl_users",
    "List all ACL users.",
    {} => |client, _input| {
        let handler = AclHandler::new(client);
        let users = handler
            .get_all_acl_users()
            .await
            .tool_context("Failed to list ACL users")?;

        CallToolResult::from_serialize(&users)
    }
);

cloud_tool!(read_only, get_acl_user, "get_acl_user",
    "Get an ACL user by ID.",
    {
        /// ACL user ID
        pub user_id: i32,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let user = handler
            .get_user_by_id(input.user_id)
            .await
            .tool_context("Failed to get ACL user")?;

        CallToolResult::from_serialize(&user)
    }
);

cloud_tool!(read_only, list_acl_roles, "list_acl_roles",
    "List all ACL roles.",
    {} => |client, _input| {
        let handler = AclHandler::new(client);
        let roles = handler
            .get_roles()
            .await
            .tool_context("Failed to list ACL roles")?;

        CallToolResult::from_serialize(&roles)
    }
);

cloud_tool!(read_only, list_redis_rules, "list_redis_rules",
    "List all Redis ACL rules.",
    {} => |client, _input| {
        let handler = AclHandler::new(client);
        let rules = handler
            .get_all_redis_rules()
            .await
            .tool_context("Failed to list Redis rules")?;

        CallToolResult::from_serialize(&rules)
    }
);

// ============================================================================
// ACL write operations (require write permission)
// ============================================================================

cloud_tool!(write, create_acl_user, "create_acl_user",
    "Create a new ACL user with a database access role.",
    {
        /// Access control user name
        pub name: String,
        /// Name of the database access role to assign. Use list_acl_roles to get available roles.
        pub role: String,
        /// Database password for the user
        pub password: String,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let request = AclUserCreateRequest {
            name: input.name,
            role: input.role,
            password: input.password,
            command_type: None,
        };
        let result = handler
            .create_user(&request)
            .await
            .tool_context("Failed to create ACL user")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_acl_user, "update_acl_user",
    "Update an ACL user's role or password.",
    {
        /// ACL user ID to update
        pub user_id: i32,
        /// New database access role name (optional)
        #[serde(default)]
        pub role: Option<String>,
        /// New database password (optional)
        #[serde(default)]
        pub password: Option<String>,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let request = AclUserUpdateRequest {
            user_id: None,
            role: input.role,
            password: input.password,
            command_type: None,
        };
        let result = handler
            .update_acl_user(input.user_id, &request)
            .await
            .tool_context("Failed to update ACL user")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_acl_user, "delete_acl_user",
    "DANGEROUS: Delete an ACL user. Active sessions will be terminated.",
    {
        /// ACL user ID to delete
        pub user_id: i32,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let result = handler
            .delete_user(input.user_id)
            .await
            .tool_context("Failed to delete ACL user")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_acl_role, "create_acl_role",
    "Create a new ACL role with Redis rules and database associations.",
    {
        /// Database access role name
        pub name: String,
        /// List of Redis ACL rules to assign to this role
        pub redis_rules: Vec<RedisRuleSpec>,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let request = AclRoleCreateRequest {
            name: input.name,
            redis_rules: input
                .redis_rules
                .into_iter()
                .map(|r| AclRoleRedisRuleSpec {
                    rule_name: r.rule_name,
                    databases: r
                        .databases
                        .into_iter()
                        .map(|d| AclRoleDatabaseSpec {
                            subscription_id: d.subscription_id,
                            database_id: d.database_id,
                            regions: d.regions,
                        })
                        .collect(),
                })
                .collect(),
            command_type: None,
        };
        let result = handler
            .create_role(&request)
            .await
            .tool_context("Failed to create ACL role")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_acl_role, "update_acl_role",
    "Update an ACL role's name or Redis rule assignments.",
    {
        /// ACL role ID to update
        pub role_id: i32,
        /// New role name (optional)
        #[serde(default)]
        pub name: Option<String>,
        /// New list of Redis ACL rules (optional)
        #[serde(default)]
        pub redis_rules: Option<Vec<RedisRuleSpec>>,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let request = AclRoleUpdateRequest {
            name: input.name,
            redis_rules: input.redis_rules.map(|rules| {
                rules
                    .into_iter()
                    .map(|r| AclRoleRedisRuleSpec {
                        rule_name: r.rule_name,
                        databases: r
                            .databases
                            .into_iter()
                            .map(|d| AclRoleDatabaseSpec {
                                subscription_id: d.subscription_id,
                                database_id: d.database_id,
                                regions: d.regions,
                            })
                            .collect(),
                    })
                    .collect()
            }),
            role_id: None,
            command_type: None,
        };
        let result = handler
            .update_role(input.role_id, &request)
            .await
            .tool_context("Failed to update ACL role")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_acl_role, "delete_acl_role",
    "DANGEROUS: Delete an ACL role. Assigned users will lose their permissions.",
    {
        /// ACL role ID to delete
        pub role_id: i32,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let result = handler
            .delete_acl_role(input.role_id)
            .await
            .tool_context("Failed to delete ACL role")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_redis_rule, "create_redis_rule",
    "Create a new Redis ACL rule defining command permissions.",
    {
        /// Redis ACL rule name
        pub name: String,
        /// Redis ACL rule pattern (e.g., "+@all ~*" or "+@read ~cache:*")
        pub redis_rule: String,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let request = AclRedisRuleCreateRequest {
            name: input.name,
            redis_rule: input.redis_rule,
            command_type: None,
        };
        let result = handler
            .create_redis_rule(&request)
            .await
            .tool_context("Failed to create Redis rule")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_redis_rule, "update_redis_rule",
    "Update a Redis ACL rule's name or pattern.",
    {
        /// Redis ACL rule ID to update
        pub rule_id: i32,
        /// New rule name
        pub name: String,
        /// New Redis ACL rule pattern
        pub redis_rule: String,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let request = AclRedisRuleUpdateRequest {
            redis_rule_id: None,
            name: input.name,
            redis_rule: input.redis_rule,
            command_type: None,
        };
        let result = handler
            .update_redis_rule(input.rule_id, &request)
            .await
            .tool_context("Failed to update Redis rule")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_redis_rule, "delete_redis_rule",
    "DANGEROUS: Delete a Redis ACL rule. Roles using it will lose those permissions.",
    {
        /// Redis ACL rule ID to delete
        pub rule_id: i32,
    } => |client, input| {
        let handler = AclHandler::new(client);
        let result = handler
            .delete_redis_rule(input.rule_id)
            .await
            .tool_context("Failed to delete Redis rule")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Cost report tools
// ============================================================================

cloud_tool!(write, generate_cost_report, "generate_cost_report",
    "Generate a FOCUS cost report for the specified date range.",
    {
        /// Start date in YYYY-MM-DD format
        pub start_date: String,
        /// End date in YYYY-MM-DD format (max 40 days from start_date)
        pub end_date: String,
        /// Output format: "csv" or "json" (default: "csv")
        #[serde(default)]
        pub format: Option<String>,
        /// Filter by subscription IDs
        #[serde(default)]
        pub subscription_ids: Option<Vec<i32>>,
        /// Filter by database IDs
        #[serde(default)]
        pub database_ids: Option<Vec<i32>>,
        /// Filter by subscription type: "pro" or "essentials"
        #[serde(default)]
        pub subscription_type: Option<String>,
        /// Filter by cloud regions
        #[serde(default)]
        pub regions: Option<Vec<String>>,
    } => |client, input| {
        use redis_cloud::{CostReportFormat, SubscriptionType};

        let handler = CostReportHandler::new(client);
        let format = input.format.as_deref().map(|f| match f {
            "json" => CostReportFormat::Json,
            _ => CostReportFormat::Csv,
        });
        let subscription_type = input.subscription_type.as_deref().map(|t| match t {
            "essentials" => SubscriptionType::Essentials,
            _ => SubscriptionType::Pro,
        });
        let request = CostReportCreateRequest {
            start_date: input.start_date,
            end_date: input.end_date,
            format,
            subscription_ids: input.subscription_ids,
            database_ids: input.database_ids,
            subscription_type,
            regions: input.regions,
            tags: None,
        };
        let result = handler
            .generate_cost_report(request)
            .await
            .tool_context("Failed to generate cost report")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, download_cost_report, "download_cost_report",
    "Download a previously generated cost report by ID.",
    {
        /// Cost report ID from a completed generation task
        pub cost_report_id: String,
    } => |client, input| {
        let handler = CostReportHandler::new(client);
        let bytes = handler
            .download_cost_report(&input.cost_report_id)
            .await
            .tool_context("Failed to download cost report")?;

        let content = String::from_utf8(bytes).unwrap_or_else(|e| {
            format!("<binary data, {} bytes>", e.into_bytes().len())
        });
        CallToolResult::from_serialize(&content)
    }
);

// ============================================================================
// Payment method tools
// ============================================================================

cloud_tool!(read_only, list_payment_methods, "list_payment_methods",
    "List all payment methods.",
    {} => |client, _input| {
        let handler = AccountHandler::new(client);
        let methods = handler
            .get_account_payment_methods()
            .await
            .tool_context("Failed to list payment methods")?;

        CallToolResult::from_serialize(&methods)
    }
);

// ============================================================================
// Task tools
// ============================================================================

cloud_tool!(read_only, list_tasks, "list_tasks",
    "List all async tasks.",
    {} => |client, _input| {
        let handler = TaskHandler::new(client);
        let tasks = handler
            .get_all_tasks()
            .await
            .tool_context("Failed to list tasks")?;

        CallToolResult::from_list("tasks", &tasks)
    }
);

cloud_tool!(read_only, get_task, "get_task",
    "Get task status by ID.",
    {
        /// Task ID
        pub task_id: String,
    } => |client, input| {
        let handler = TaskHandler::new(client);
        let task = handler
            .get_task_by_id(input.task_id)
            .await
            .tool_context("Failed to get task")?;

        CallToolResult::from_serialize(&task)
    }
);

cloud_tool!(read_only, wait_for_cloud_task, "wait_for_cloud_task",
    "Poll an async task until it reaches a terminal state. Useful for multi-step workflows.",
    {
        /// Task ID to wait for
        pub task_id: String,
        /// Maximum time to wait in seconds (default: 300)
        #[serde(default = "default_task_timeout")]
        pub timeout_seconds: u64,
        /// Polling interval in seconds (default: 5)
        #[serde(default = "default_task_interval")]
        pub interval_seconds: u64,
    } => |client, input| {
        let handler = TaskHandler::new(client);
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(input.timeout_seconds);
        let interval = std::time::Duration::from_secs(input.interval_seconds);

        loop {
            let task = handler
                .get_task_by_id(input.task_id.clone())
                .await
                .tool_context("Failed to get task status")?;

            // Check for terminal states. `TaskStatus` models only the two
            // terminal states explicitly; everything else is still in flight.
            if matches!(
                task.status,
                Some(TaskStatus::ProcessingCompleted) | Some(TaskStatus::ProcessingError)
            ) {
                return CallToolResult::from_serialize(&task);
            }

            // Check timeout
            if tokio::time::Instant::now() >= deadline {
                return CallToolResult::from_serialize(&serde_json::json!({
                    "timeout": true,
                    "message": format!(
                        "Task {} did not complete within {} seconds",
                        input.task_id, input.timeout_seconds
                    ),
                    "last_status": task,
                }));
            }

            tokio::time::sleep(interval).await;
        }
    }
);

// ============================================================================
// Cloud Account (BYOC) tools
// ============================================================================

cloud_tool!(read_only, list_cloud_accounts, "list_cloud_accounts",
    "List all cloud provider accounts (BYOC).",
    {} => |client, _input| {
        let handler = CloudAccountHandler::new(client);
        let result = handler
            .get_cloud_accounts()
            .await
            .tool_context("Failed to list cloud accounts")?;

        let accounts = result.cloud_accounts.unwrap_or_default();
        CallToolResult::from_list("cloud_accounts", &accounts)
    }
);

cloud_tool!(read_only, get_cloud_account, "get_cloud_account",
    "Get a cloud provider account (BYOC) by ID.",
    {
        /// Cloud account ID
        pub cloud_account_id: i32,
    } => |client, input| {
        let handler = CloudAccountHandler::new(client);
        let account = handler
            .get_cloud_account_by_id(input.cloud_account_id)
            .await
            .tool_context("Failed to get cloud account")?;

        CallToolResult::from_serialize(&account)
    }
);

cloud_tool!(write, create_cloud_account, "create_cloud_account",
    "Create a new cloud provider account (BYOC).",
    {
        /// Cloud account display name
        pub name: String,
        /// Cloud provider (e.g., "AWS", "GCP", "Azure"). Defaults to "AWS" if not specified.
        #[serde(default)]
        pub provider: Option<String>,
        /// Cloud provider access key
        pub access_key_id: String,
        /// Cloud provider secret key
        pub access_secret_key: String,
        /// Cloud provider management console username
        pub console_username: String,
        /// Cloud provider management console password
        pub console_password: String,
        /// Cloud provider management console login URL
        pub sign_in_login_url: String,
    } => |client, input| {
        let handler = CloudAccountHandler::new(client);
        let request = CloudAccountCreateRequest {
            name: input.name,
            provider: input.provider,
            access_key_id: input.access_key_id,
            access_secret_key: input.access_secret_key,
            console_username: input.console_username,
            console_password: input.console_password,
            sign_in_login_url: input.sign_in_login_url,
            command_type: None,
        };
        let result = handler
            .create_cloud_account(&request)
            .await
            .tool_context("Failed to create cloud account")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_cloud_account, "update_cloud_account",
    "Update a cloud provider account (BYOC) configuration.",
    {
        /// Cloud account ID to update
        pub cloud_account_id: i32,
        /// New display name (optional)
        #[serde(default)]
        pub name: Option<String>,
        /// Cloud provider access key
        pub access_key_id: String,
        /// Cloud provider secret key
        pub access_secret_key: String,
        /// Cloud provider management console username
        pub console_username: String,
        /// Cloud provider management console password
        pub console_password: String,
        /// Cloud provider management console login URL (optional)
        #[serde(default)]
        pub sign_in_login_url: Option<String>,
    } => |client, input| {
        let handler = CloudAccountHandler::new(client);
        let request = CloudAccountUpdateRequest {
            name: input.name,
            cloud_account_id: None,
            access_key_id: input.access_key_id,
            access_secret_key: input.access_secret_key,
            console_username: input.console_username,
            console_password: input.console_password,
            sign_in_login_url: input.sign_in_login_url,
            command_type: None,
        };
        let result = handler
            .update_cloud_account(input.cloud_account_id, &request)
            .await
            .tool_context("Failed to update cloud account")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_cloud_account, "delete_cloud_account",
    "DANGEROUS: Delete a cloud provider account (BYOC).",
    {
        /// Cloud account ID to delete
        pub cloud_account_id: i32,
    } => |client, input| {
        let handler = CloudAccountHandler::new(client);
        let result = handler
            .delete_cloud_account(input.cloud_account_id)
            .await
            .tool_context("Failed to delete cloud account")?;

        CallToolResult::from_serialize(&result)
    }
);
