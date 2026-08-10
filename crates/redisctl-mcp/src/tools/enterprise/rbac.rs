//! User, role, ACL, and LDAP tools

use redis_enterprise::ldap_mappings::LdapMappingHandler;
use redis_enterprise::redis_acls::{CreateRedisAclRequest, RedisAclHandler};
use redis_enterprise::roles::{CreateRoleRequest, RolesHandler};
use redis_enterprise::users::{CreateUserRequest, UpdateUserRequest, UserHandler};
use serde_json::Value;
use tower_mcp::{CallToolResult, ResultExt};

use crate::tools::macros::{enterprise_tool, mcp_module};

mcp_module! {
    list_users => "list_enterprise_users",
    get_user => "get_enterprise_user",
    create_enterprise_user => "create_enterprise_user",
    update_enterprise_user => "update_enterprise_user",
    delete_enterprise_user => "delete_enterprise_user",
    get_enterprise_user_permissions => "get_enterprise_user_permissions",
    list_roles => "list_enterprise_roles",
    get_role => "get_enterprise_role",
    create_enterprise_role => "create_enterprise_role",
    update_enterprise_role => "update_enterprise_role",
    delete_enterprise_role => "delete_enterprise_role",
    list_redis_acls => "list_enterprise_acls",
    get_redis_acl => "get_enterprise_acl",
    create_enterprise_acl => "create_enterprise_acl",
    update_enterprise_acl => "update_enterprise_acl",
    delete_enterprise_acl => "delete_enterprise_acl",
    validate_enterprise_acl => "validate_enterprise_acl",
    get_enterprise_ldap_config => "get_enterprise_ldap_config",
    update_enterprise_ldap_config => "update_enterprise_ldap_config",
}

// ============================================================================
// User tools
// ============================================================================

enterprise_tool!(read_only, list_users, "list_enterprise_users",
    "List all users.",
    {} => |client, _input| {
        let handler = UserHandler::new(client);
        let users = handler.list().await.tool_context("Failed to list users")?;

        CallToolResult::from_list("users", &users)
    }
);

enterprise_tool!(read_only, get_user, "get_enterprise_user",
    "Get user details by UID.",
    {
        /// User UID
        pub uid: u32,
    } => |client, input| {
        let handler = UserHandler::new(client);
        let user = handler
            .get(input.uid)
            .await
            .tool_context("Failed to get user")?;

        CallToolResult::from_serialize(&user)
    }
);

enterprise_tool!(write, create_enterprise_user, "create_enterprise_user",
    "Create a new user. \
     Prerequisites: 1) list_enterprise_roles -- identify roles to assign. \
     2) list_enterprise_users -- check for existing users to avoid duplicates.",
    {
        /// User email address (used as login)
        pub email: String,
        /// User password
        pub password: String,
        /// Role name: "admin", "cluster_member", "cluster_viewer", "db_member", "db_viewer", or "none"
        pub role: String,
        /// Display name
        #[serde(default)]
        pub name: Option<String>,
        /// Whether the user receives email alerts
        #[serde(default)]
        pub email_alerts: Option<bool>,
        /// Role UIDs to assign (for custom role-based access)
        #[serde(default)]
        pub role_uids: Option<Vec<u32>>,
    } => |client, input| {
        let request = CreateUserRequest {
            email: input.email,
            password: input.password,
            // `role` is now Option<String> in redis-enterprise so RBAC-only
            // clusters can pass role_uids without a role. The MCP tool
            // still treats role as required at the schema level.
            role: Some(input.role),
            name: input.name,
            email_alerts: input.email_alerts,
            bdbs_email_alerts: None,
            role_uids: input.role_uids,
            auth_method: None,
        };

        let handler = UserHandler::new(client);
        let user = handler
            .create(request)
            .await
            .tool_context("Failed to create user")?;

        CallToolResult::from_serialize(&user)
    }
);

enterprise_tool!(write, update_enterprise_user, "update_enterprise_user",
    "Update an existing user. Only specified fields will be modified.",
    {
        /// User UID to update
        pub uid: u32,
        /// New password
        #[serde(default)]
        pub password: Option<String>,
        /// New role: "admin", "cluster_member", "cluster_viewer", "db_member", "db_viewer", or "none"
        #[serde(default)]
        pub role: Option<String>,
        /// New email address
        #[serde(default)]
        pub email: Option<String>,
        /// New display name
        #[serde(default)]
        pub name: Option<String>,
        /// Whether the user receives email alerts
        #[serde(default)]
        pub email_alerts: Option<bool>,
        /// Role UIDs to assign (for custom role-based access)
        #[serde(default)]
        pub role_uids: Option<Vec<u32>>,
    } => |client, input| {
        let request = UpdateUserRequest {
            password: input.password,
            role: input.role,
            email: input.email,
            name: input.name,
            email_alerts: input.email_alerts,
            bdbs_email_alerts: None,
            role_uids: input.role_uids,
            auth_method: None,
        };

        let handler = UserHandler::new(client);
        let user = handler
            .update(input.uid, request)
            .await
            .tool_context("Failed to update user")?;

        CallToolResult::from_serialize(&user)
    }
);

enterprise_tool!(destructive, delete_enterprise_user, "delete_enterprise_user",
    "DANGEROUS: Delete a user. Active sessions will be terminated.",
    {
        /// User UID to delete
        pub uid: u32,
    } => |client, input| {
        let handler = UserHandler::new(client);
        handler
            .delete(input.uid)
            .await
            .tool_context("Failed to delete user")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "User deleted successfully",
            "uid": input.uid
        }))
    }
);

// ============================================================================
// User Permissions
// ============================================================================

enterprise_tool!(read_only, get_enterprise_user_permissions, "get_enterprise_user_permissions",
    "Get all available permission types for user management.",
    {} => |client, _input| {
        let handler = UserHandler::new(client);
        let permissions = handler
            .permissions()
            .await
            .tool_context("Failed to get user permissions")?;

        CallToolResult::from_serialize(&permissions)
    }
);

// ============================================================================
// Role tools
// ============================================================================

enterprise_tool!(read_only, list_roles, "list_enterprise_roles",
    "List all roles.",
    {} => |client, _input| {
        let handler = RolesHandler::new(client);
        let roles = handler.list().await.tool_context("Failed to list roles")?;

        CallToolResult::from_list("roles", &roles)
    }
);

enterprise_tool!(read_only, get_role, "get_enterprise_role",
    "Get role details by UID, including permissions and assignments.",
    {
        /// Role UID
        pub uid: u32,
    } => |client, input| {
        let handler = RolesHandler::new(client);
        let role = handler
            .get(input.uid)
            .await
            .tool_context("Failed to get role")?;

        CallToolResult::from_serialize(&role)
    }
);

enterprise_tool!(write, create_enterprise_role, "create_enterprise_role",
    "Create a new role. Use list_enterprise_roles to review existing roles and \
     list_enterprise_acls to identify Redis ACLs before creating it.",
    {
        /// Role name
        pub name: String,
        /// Management permission level: "admin", "db_member", "db_viewer", "cluster_member", "cluster_viewer", or "none"
        #[serde(default)]
        pub management: Option<String>,
        /// Data access permission level: "redis_acl" or "none"
        #[serde(default)]
        pub data_access: Option<String>,
    } => |client, input| {
        let request = CreateRoleRequest {
            name: input.name,
            management: input.management,
            data_access: input.data_access,
            bdb_roles: None,
            cluster_roles: None,
        };

        let handler = RolesHandler::new(client);
        let role = handler
            .create(request)
            .await
            .tool_context("Failed to create role")?;

        CallToolResult::from_serialize(&role)
    }
);

enterprise_tool!(write, update_enterprise_role, "update_enterprise_role",
    "Update an existing role.",
    {
        /// Role UID to update
        pub uid: u32,
        /// Role name
        pub name: String,
        /// Management permission level: "admin", "db_member", "db_viewer", "cluster_member", "cluster_viewer", or "none"
        #[serde(default)]
        pub management: Option<String>,
        /// Data access permission level: "redis_acl" or "none"
        #[serde(default)]
        pub data_access: Option<String>,
    } => |client, input| {
        let request = CreateRoleRequest {
            name: input.name,
            management: input.management,
            data_access: input.data_access,
            bdb_roles: None,
            cluster_roles: None,
        };

        let handler = RolesHandler::new(client);
        let role = handler
            .update(input.uid, request)
            .await
            .tool_context("Failed to update role")?;

        CallToolResult::from_serialize(&role)
    }
);

enterprise_tool!(destructive, delete_enterprise_role, "delete_enterprise_role",
    "DANGEROUS: Delete a role. Users assigned to it will lose their permissions.",
    {
        /// Role UID to delete
        pub uid: u32,
    } => |client, input| {
        let handler = RolesHandler::new(client);
        handler
            .delete(input.uid)
            .await
            .tool_context("Failed to delete role")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "Role deleted successfully",
            "uid": input.uid
        }))
    }
);

// ============================================================================
// Redis ACL tools
// ============================================================================

enterprise_tool!(read_only, list_redis_acls, "list_enterprise_acls",
    "List all Redis ACLs.",
    {} => |client, _input| {
        let handler = RedisAclHandler::new(client);
        let acls = handler.list().await.tool_context("Failed to list ACLs")?;

        CallToolResult::from_list("acls", &acls)
    }
);

enterprise_tool!(read_only, get_redis_acl, "get_enterprise_acl",
    "Get Redis ACL details by UID, including rule string and associated databases.",
    {
        /// ACL UID
        pub uid: u32,
    } => |client, input| {
        let handler = RedisAclHandler::new(client);
        let acl = handler
            .get(input.uid)
            .await
            .tool_context("Failed to get ACL")?;

        CallToolResult::from_serialize(&acl)
    }
);

enterprise_tool!(write, create_enterprise_acl, "create_enterprise_acl",
    "Create a new Redis ACL using Redis ACL syntax (e.g., \"+@all ~*\"). \
     Prerequisites: 1) list_enterprise_acls -- review existing ACLs to avoid duplicates. \
     2) validate_enterprise_acl -- validate ACL syntax before creation.",
    {
        /// ACL name
        pub name: String,
        /// ACL rule string (e.g., "+@all ~*" or "+get +set ~cache:*")
        pub acl: String,
        /// Description of the ACL
        #[serde(default)]
        pub description: Option<String>,
    } => |client, input| {
        let request = CreateRedisAclRequest {
            name: input.name,
            acl: input.acl,
            description: input.description,
        };

        let handler = RedisAclHandler::new(client);
        let acl = handler
            .create(request)
            .await
            .tool_context("Failed to create ACL")?;

        CallToolResult::from_serialize(&acl)
    }
);

enterprise_tool!(write, update_enterprise_acl, "update_enterprise_acl",
    "Update an existing Redis ACL.",
    {
        /// ACL UID to update
        pub uid: u32,
        /// ACL name
        pub name: String,
        /// ACL rule string (e.g., "+@all ~*" or "+get +set ~cache:*")
        pub acl: String,
        /// Description of the ACL
        #[serde(default)]
        pub description: Option<String>,
    } => |client, input| {
        let request = CreateRedisAclRequest {
            name: input.name,
            acl: input.acl,
            description: input.description,
        };

        let handler = RedisAclHandler::new(client);
        let acl = handler
            .update(input.uid, request)
            .await
            .tool_context("Failed to update ACL")?;

        CallToolResult::from_serialize(&acl)
    }
);

enterprise_tool!(destructive, delete_enterprise_acl, "delete_enterprise_acl",
    "DANGEROUS: Delete a Redis ACL. Databases using it will lose those access controls.",
    {
        /// ACL UID to delete
        pub uid: u32,
    } => |client, input| {
        let handler = RedisAclHandler::new(client);
        handler
            .delete(input.uid)
            .await
            .tool_context("Failed to delete ACL")?;

        CallToolResult::from_serialize(&serde_json::json!({
            "message": "ACL deleted successfully",
            "uid": input.uid
        }))
    }
);

// ============================================================================
// ACL Validation
// ============================================================================

enterprise_tool!(read_only, validate_enterprise_acl, "validate_enterprise_acl",
    "Validate a Redis ACL rule before creating it.",
    {
        /// ACL name
        pub name: String,
        /// ACL rule string (e.g., "+@all ~*" or "+get +set ~cache:*")
        pub acl: String,
        /// Description of the ACL
        #[serde(default)]
        pub description: Option<String>,
    } => |client, input| {
        let request = CreateRedisAclRequest {
            name: input.name,
            acl: input.acl,
            description: input.description,
        };

        let handler = RedisAclHandler::new(client);
        let result = handler
            .validate(request)
            .await
            .tool_context("Failed to validate ACL")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// LDAP tools
// ============================================================================

enterprise_tool!(read_only, get_enterprise_ldap_config, "get_enterprise_ldap_config",
    "Get the LDAP configuration including server settings, bind DN, and query suffixes.",
    {} => |client, _input| {
        let handler = LdapMappingHandler::new(client);
        let config = handler
            .get_config()
            .await
            .tool_context("Failed to get LDAP config")?;

        CallToolResult::from_serialize(&config)
    }
);

enterprise_tool!(write, update_enterprise_ldap_config, "update_enterprise_ldap_config",
    "Update the LDAP configuration. Pass LDAP settings as JSON.",
    {
        /// LDAP configuration as a JSON object. Fields: enabled (bool), servers (array of {host, port, use_tls, starttls}),
        /// cache_refresh_interval, authentication_query_suffix, authorization_query_suffix, bind_dn, bind_pass
        pub config: Value,
    } => |client, input| {
        let config = serde_json::from_value(input.config)
            .tool_context("Invalid LDAP config")?;

        let handler = LdapMappingHandler::new(client);
        let result = handler
            .update_config(config)
            .await
            .tool_context("Failed to update LDAP config")?;

        CallToolResult::from_serialize(&result)
    }
);
