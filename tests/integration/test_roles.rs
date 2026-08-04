//! Integration tests for Redis Enterprise role and ACL commands
//!
//! This module demonstrates role-based access control (RBAC) management in Redis Enterprise.
//! Roles define what users can do within the cluster and which databases they can access.
//!
//! # Supported Operations
//!
//! - **List roles** - Get all roles (custom and built-in)
//! - **Get role info** - Retrieve details about a specific role
//! - **Create role** - Define new roles with specific permissions
//! - **Update role** - Modify existing role permissions
//! - **Delete role** - Remove custom roles
//! - **List role users** - See which users are assigned to a role
//!
//! # Permission Levels
//!
//! ## Management Permissions
//! - `none` - No management access
//! - `db_viewer` - Read-only access to database info
//! - `db_member` - Limited database management
//! - `cluster_viewer` - Read-only cluster access
//! - `cluster_member` - Limited cluster management
//! - `admin` - Full administrative access
//!
//! ## Data Access Permissions
//! - `full` - Read and write access to all data
//! - `read_only` - Read-only access to data
//! - `custom` - Custom ACL rules per database
//!
//! # Usage Examples
//!
//! ## List all roles
//! ```bash
//! redis-enterprise role list
//! redis-enterprise role list --builtin  # Include built-in roles
//! ```
//!
//! ## Create a new role
//! ```bash
//! redis-enterprise role create --name "app_reader" --management db_viewer --data-access read_only
//! redis-enterprise role create --name "complex_role" --from-json role_config.json
//! ```
//!
//! ## Update role permissions
//! ```bash
//! redis-enterprise role update 123 --management db_member
//! redis-enterprise role update 123 --from-json updated_config.json
//! ```
//!
//! ## Get role information
//! ```bash
//! redis-enterprise role get 123
//! redis-enterprise role users 123  # List users with this role
//! ```

use redis_enterprise_cli::handlers::handle_role_command;
use redis_enterprise_cli::commands::RoleCommands;
use serde_json::json;
use wiremock::{
    matchers::{method, path, body_json},
    Mock, ResponseTemplate,
};

mod common;
use common::{setup_mock_server, create_temp_json_file};

#[tokio::test]
async fn test_role_list() {
    let (mock_server, client) = setup_mock_server().await;

    let mock_response = json!([
        {
            "uid": 1,
            "name": "admin",
            "management": "admin",
            "data_access": "full"
        },
        {
            "uid": 2,
            "name": "db_viewer",
            "management": "db_viewer",
            "data_access": "read_only"
        }
    ]);

    Mock::given(method("GET"))
        .and(path("/v1/roles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::List { builtin: false };
    let result = handle_role_command(&client, command).await.unwrap();

    let roles = result.as_array().unwrap();
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0]["name"], "admin");
    assert_eq!(roles[1]["name"], "db_viewer");
}

#[tokio::test]
async fn test_role_get() {
    let (mock_server, client) = setup_mock_server().await;

    let mock_response = json!({
        "uid": 1,
        "name": "test_role",
        "management": "db_member",
        "data_access": "full",
        "bdb_roles": [
            {
                "bdb_uid": 1,
                "role": "owner",
                "redis_acl_uid": 1
            }
        ],
        "cluster_roles": ["DB Member"]
    });

    Mock::given(method("GET"))
        .and(path("/v1/roles/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::Get { uid: 1 };
    let result = handle_role_command(&client, command).await.unwrap();

    assert_eq!(result["uid"], 1);
    assert_eq!(result["name"], "test_role");
    assert_eq!(result["management"], "db_member");
    assert_eq!(result["data_access"], "full");
    assert_eq!(result["bdb_roles"].as_array().unwrap().len(), 1);
    assert_eq!(result["cluster_roles"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_role_get_not_found() {
    let (mock_server, client) = setup_mock_server().await;

    Mock::given(method("GET"))
        .and(path("/v1/roles/999"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": "Role not found"
        })))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::Get { uid: 999 };
    let result = handle_role_command(&client, command).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("404"));
}

#[tokio::test]
async fn test_role_create_with_args() {
    let (mock_server, client) = setup_mock_server().await;

    let expected_request = json!({
        "name": "new_role",
        "management": "db_viewer",
        "data_access": "read_only"
    });

    let mock_response = json!({
        "uid": 2,
        "name": "new_role",
        "management": "db_viewer",
        "data_access": "read_only"
    });

    Mock::given(method("POST"))
        .and(path("/v1/roles"))
        .and(body_json(&expected_request))
        .respond_with(ResponseTemplate::new(201).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::Create {
        name: "new_role".to_string(),
        management: Some("db_viewer".to_string()),
        data_access: Some("read_only".to_string()),
        from_json: None,
    };
    let result = handle_role_command(&client, command).await.unwrap();

    assert_eq!(result["uid"], 2);
    assert_eq!(result["name"], "new_role");
    assert_eq!(result["management"], "db_viewer");
    assert_eq!(result["data_access"], "read_only");
}

/// Test creating a role from JSON configuration file
/// 
/// This test demonstrates:
/// - Complex role configuration with database-specific permissions
/// - Using JSON files for roles with multiple database assignments
/// - Custom data access patterns with specific database roles
#[tokio::test]
async fn test_role_create_from_json() {
    let (mock_server, client) = setup_mock_server().await;

    // Create comprehensive role configuration
    let role_data = json!({
        "name": "database_admin",
        "management": "db_member",
        "data_access": "custom",
        "bdb_roles": [
            {
                "bdb_uid": 1,
                "role": "owner",
                "redis_acl_uid": 10
            },
            {
                "bdb_uid": 2,
                "role": "viewer"
            }
        ],
        "cluster_roles": ["DB Member"]
    });
    
    let temp_file = create_temp_json_file(role_data.clone());

    let mock_response = json!({
        "uid": 3,
        "name": "database_admin",
        "management": "db_member",
        "data_access": "custom",
        "bdb_roles": [
            {
                "bdb_uid": 1,
                "role": "owner",
                "redis_acl_uid": 10
            },
            {
                "bdb_uid": 2,
                "role": "viewer"
            }
        ],
        "cluster_roles": ["DB Member"]
    });

    Mock::given(method("POST"))
        .and(path("/v1/roles"))
        .and(body_json(&role_data))
        .respond_with(ResponseTemplate::new(201).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::Create {
        name: "ignored".to_string(), // CLI args ignored when using from_json
        management: None,
        data_access: None,
        from_json: Some(temp_file.path().to_path_buf()),
    };
    let result = handle_role_command(&client, command).await.unwrap();

    assert_eq!(result["name"], "database_admin");
    assert_eq!(result["management"], "db_member");
    assert_eq!(result["data_access"], "custom");
    assert_eq!(result["bdb_roles"].as_array().unwrap().len(), 2);
    assert_eq!(result["cluster_roles"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_role_update() {
    let (mock_server, client) = setup_mock_server().await;

    // Mock the GET request to fetch existing role
    let existing_role = json!({
        "uid": 1,
        "name": "existing_role",
        "management": "db_viewer",
        "data_access": "read_only",
        "bdb_roles": [],
        "cluster_roles": []
    });

    Mock::given(method("GET"))
        .and(path("/v1/roles/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&existing_role))
        .mount(&mock_server)
        .await;

    // Mock the PUT request
    let expected_request = json!({
        "name": "updated_role",
        "management": "db_member",
        "data_access": "read_only",
        "bdb_roles": [],
        "cluster_roles": []
    });

    let mock_response = json!({
        "uid": 1,
        "name": "updated_role",
        "management": "db_member",
        "data_access": "read_only"
    });

    Mock::given(method("PUT"))
        .and(path("/v1/roles/1"))
        .and(body_json(&expected_request))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::Update {
        uid: 1,
        name: Some("updated_role".to_string()),
        management: Some("db_member".to_string()),
        data_access: None, // Should keep existing value
        from_json: None,
    };
    let result = handle_role_command(&client, command).await.unwrap();

    assert_eq!(result["name"], "updated_role");
    assert_eq!(result["management"], "db_member");
}

#[tokio::test]
async fn test_role_delete_success() {
    let (mock_server, client) = setup_mock_server().await;

    Mock::given(method("DELETE"))
        .and(path("/v1/roles/1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::Delete {
        uid: 1,
        yes: true, // Skip confirmation
    };
    let result = handle_role_command(&client, command).await.unwrap();

    assert_eq!(result["deleted"], true);
    assert_eq!(result["uid"], 1);
}

#[tokio::test]
async fn test_role_users() {
    let (mock_server, client) = setup_mock_server().await;

    let mock_response = json!([1, 2, 3]);

    Mock::given(method("GET"))
        .and(path("/v1/roles/1/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = RoleCommands::Users { uid: 1 };
    let result = handle_role_command(&client, command).await.unwrap();

    let user_uids = result.as_array().unwrap();
    assert_eq!(user_uids.len(), 3);
    assert_eq!(user_uids[0], 1);
    assert_eq!(user_uids[1], 2);
    assert_eq!(user_uids[2], 3);
}

#[tokio::test]
async fn test_role_create_invalid_json_file() {
    let (_, client) = setup_mock_server().await;

    let command = RoleCommands::Create {
        name: "test".to_string(),
        management: None,
        data_access: None,
        from_json: Some("/nonexistent/file.json".into()),
    };
    let result = handle_role_command(&client, command).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No such file"));
}
