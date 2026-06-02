#![cfg(feature = "cloud")]
//! Integration tests for Redis Cloud MCP tools using mock server

use std::sync::Arc;

use redis_cloud::testing::{
    AccountFixture, DatabaseFixture, MockCloudServer, SubscriptionFixture, TaskFixture, UserFixture,
};
use serde_json::json;
use tower_mcp::Tool;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

// Import the tools and state from the MCP crate
use redisctl_mcp::state::AppState;
use redisctl_mcp::tools::cloud;

/// Helper to call a tool and get text result
async fn call_tool_text(tool: &Tool, input: serde_json::Value) -> String {
    let result = tool.call(input).await;
    result
        .content
        .first()
        .and_then(|c: &tower_mcp::Content| c.as_text())
        .unwrap_or_default()
        .to_string()
}

/// Helper to call a tool and get JSON result
async fn call_tool_json(tool: &Tool, input: serde_json::Value) -> serde_json::Value {
    let text = call_tool_text(tool, input).await;
    serde_json::from_str(&text).expect("valid JSON response")
}

// ============================================================================
// Subscription Tests
// ============================================================================

#[tokio::test]
async fn test_list_subscriptions() {
    let server = MockCloudServer::start().await;

    let sub1 = SubscriptionFixture::new(123, "Production")
        .status("active")
        .cloud_provider("AWS")
        .region("us-east-1")
        .build();

    let sub2 = SubscriptionFixture::new(456, "Development")
        .status("active")
        .cloud_provider("GCP")
        .region("us-central1")
        .build();

    server.mock_subscriptions_list(vec![sub1, sub2]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_subscriptions(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("subscriptions").is_some());
    let subscriptions = result["subscriptions"].as_array().unwrap();
    assert_eq!(subscriptions.len(), 2);
    assert_eq!(subscriptions[0]["name"], "Production");
    assert_eq!(subscriptions[1]["name"], "Development");
}

#[tokio::test]
async fn test_list_subscriptions_empty() {
    let server = MockCloudServer::start().await;
    server.mock_subscriptions_list(vec![]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_subscriptions(state);

    let result = call_tool_json(&tool, json!({})).await;

    let subscriptions = result["subscriptions"].as_array().unwrap();
    assert_eq!(subscriptions.len(), 0);
}

#[tokio::test]
async fn test_get_subscription() {
    let server = MockCloudServer::start().await;

    let subscription = SubscriptionFixture::new(123, "Production")
        .status("active")
        .payment_method_type("credit-card")
        .memory_storage("ram")
        .cloud_provider("AWS")
        .region("us-east-1")
        .build();

    server.mock_subscription_get(123, subscription).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_subscription(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 123})).await;

    assert_eq!(result["id"], 123);
    assert_eq!(result["name"], "Production");
    assert_eq!(result["status"], "active");
}

// ============================================================================
// Database Tests
// ============================================================================

#[tokio::test]
async fn test_list_databases() {
    let server = MockCloudServer::start().await;

    let db1 = DatabaseFixture::new(1001, "cache-primary")
        .memory_limit_in_gb(2.0)
        .protocol("redis")
        .replication(true)
        .public_endpoint("redis-1001.c1.us-east-1.ec2.cloud.redislabs.com:12001")
        .build();

    let db2 = DatabaseFixture::new(1002, "cache-replica")
        .memory_limit_in_gb(1.0)
        .protocol("redis")
        .replication(false)
        .build();

    // Use the convenience method - now returns correct nested structure
    server.mock_databases_list(123, vec![db1, db2]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_databases(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 123})).await;

    // The response includes subscription wrapper
    let subscriptions = result["subscription"].as_array().unwrap();
    assert_eq!(subscriptions.len(), 1);
    let databases = subscriptions[0]["databases"].as_array().unwrap();
    assert_eq!(databases.len(), 2);
    assert_eq!(databases[0]["name"], "cache-primary");
    assert_eq!(databases[1]["name"], "cache-replica");
}

#[tokio::test]
async fn test_get_database() {
    let server = MockCloudServer::start().await;

    let database = DatabaseFixture::new(1001, "cache-primary")
        .memory_limit_in_gb(2.0)
        .protocol("redis")
        .replication(true)
        .data_persistence("aof-every-1-second")
        .throughput("operations-per-second", 25000)
        .public_endpoint("redis-1001.c1.us-east-1.ec2.cloud.redislabs.com:12001")
        .build();

    server.mock_database_get(123, 1001, database).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_database(state);

    let result = call_tool_json(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001
        }),
    )
    .await;

    assert_eq!(result["databaseId"], 1001);
    assert_eq!(result["name"], "cache-primary");
    assert_eq!(result["memoryLimitInGb"], 2.0);
    assert_eq!(result["protocol"], "redis");
    assert_eq!(result["replication"], true);
}

// ============================================================================
// Account Tests
// ============================================================================

#[tokio::test]
async fn test_get_account() {
    let server = MockCloudServer::start().await;

    let account = AccountFixture::new(12345, "My Organization")
        .marketplace_status("active")
        .created_timestamp("2024-01-15T10:30:00Z")
        .build();

    server.mock_account(account).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_account(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("account").is_some());
    assert_eq!(result["account"]["id"], 12345);
    assert_eq!(result["account"]["name"], "My Organization");
}

// ============================================================================
// Task Tests
// ============================================================================

#[tokio::test]
async fn test_list_tasks() {
    let server = MockCloudServer::start().await;

    let task1 = TaskFixture::completed("task-001", 123)
        .command_type("subscriptionCreateRequest")
        .description("Create subscription")
        .build();

    let task2 = TaskFixture::new("task-002")
        .command_type("databaseCreateRequest")
        .status("processing-in-progress")
        .description("Create database")
        .build();

    // Use convenience method - now returns direct array
    server.mock_tasks_list(vec![task1, task2]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_tasks(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let tasks = result["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["taskId"], "task-001");
    assert_eq!(tasks[0]["status"], "processing-completed");
    assert_eq!(tasks[1]["taskId"], "task-002");
    assert_eq!(tasks[1]["status"], "processing-in-progress");
}

#[tokio::test]
async fn test_get_task() {
    let server = MockCloudServer::start().await;

    let task = TaskFixture::completed("task-001", 123)
        .command_type("subscriptionCreateRequest")
        .description("Create subscription completed successfully")
        .build();

    server.mock_task_get("task-001", task).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_task(state);

    let result = call_tool_json(&tool, json!({"task_id": "task-001"})).await;

    assert_eq!(result["taskId"], "task-001");
    assert_eq!(result["status"], "processing-completed");
    assert_eq!(result["response"]["resourceId"], 123);
}

#[tokio::test]
async fn test_get_task_failed() {
    let server = MockCloudServer::start().await;

    let task = TaskFixture::failed("task-002", "Insufficient credits")
        .command_type("subscriptionCreateRequest")
        .build();

    server.mock_task_get("task-002", task).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_task(state);

    let result = call_tool_json(&tool, json!({"task_id": "task-002"})).await;

    assert_eq!(result["taskId"], "task-002");
    assert_eq!(result["status"], "processing-error");
    assert_eq!(result["response"]["error"], "Insufficient credits");
}

// ============================================================================
// User Tests
// ============================================================================

#[tokio::test]
async fn test_list_account_users() {
    let server = MockCloudServer::start().await;

    let user1 = UserFixture::new(1, "admin@example.com")
        .name("Admin User")
        .role("owner")
        .build();

    let user2 = UserFixture::new(2, "dev@example.com")
        .name("Developer")
        .role("member")
        .build();

    server.mock_users_list(vec![user1, user2]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_account_users(state);

    let result = call_tool_json(&tool, json!({})).await;

    let users = result["users"].as_array().unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0]["email"], "admin@example.com");
    assert_eq!(users[0]["role"], "owner");
    assert_eq!(users[1]["email"], "dev@example.com");
    assert_eq!(users[1]["role"], "member");
}

// ============================================================================
// Regions and Modules Tests
// ============================================================================

#[tokio::test]
async fn test_get_regions() {
    let server = MockCloudServer::start().await;

    server
        .mock_regions(vec![
            json!({"name": "us-east-1", "provider": "AWS"}),
            json!({"name": "us-west-2", "provider": "AWS"}),
            json!({"name": "us-central1", "provider": "GCP"}),
        ])
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_regions(state);

    let result = call_tool_json(&tool, json!({})).await;

    let regions = result["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 3);
}

#[tokio::test]
async fn test_get_modules() {
    let server = MockCloudServer::start().await;

    server
        .mock_database_modules(vec![
            json!({"name": "RedisJSON", "description": "JSON support"}),
            json!({"name": "RediSearch", "description": "Full-text search"}),
            json!({"name": "RedisTimeSeries", "description": "Time series data"}),
        ])
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_modules(state);

    let result = call_tool_json(&tool, json!({})).await;

    let modules = result["modules"].as_array().unwrap();
    assert_eq!(modules.len(), 3);
    assert_eq!(modules[0]["name"], "RedisJSON");
}

// ============================================================================
// Logs Tests
// ============================================================================

#[tokio::test]
async fn test_get_system_logs() {
    let server = MockCloudServer::start().await;

    server
        .mock_path(
            "GET",
            "/logs",
            ResponseTemplate::new(200).set_body_json(json!({
                "entries": [
                    {
                        "id": 1,
                        "time": "2024-01-15T10:30:00Z",
                        "originator": "admin@example.com",
                        "apiKeyName": "default-api-key",
                        "resource": "subscription",
                        "resourceId": 123,
                        "action": "create-subscription"
                    },
                    {
                        "id": 2,
                        "time": "2024-01-15T10:25:00Z",
                        "originator": "admin@example.com",
                        "apiKeyName": "default-api-key",
                        "resource": "database",
                        "resourceId": 456,
                        "action": "update-database"
                    }
                ]
            })),
        )
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_system_logs(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("entries").is_some());
    let entries = result["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn test_get_session_logs() {
    let server = MockCloudServer::start().await;

    server
        .mock_path(
            "GET",
            "/session-logs",
            ResponseTemplate::new(200).set_body_json(json!({
                "entries": [
                    {
                        "id": "550e8400-e29b-41d4-a716-446655440001",
                        "time": "2024-01-15T10:30:00Z",
                        "user": "admin@example.com",
                        "action": "login"
                    },
                    {
                        "id": "550e8400-e29b-41d4-a716-446655440002",
                        "time": "2024-01-15T09:00:00Z",
                        "user": "dev@example.com",
                        "action": "logout"
                    }
                ]
            })),
        )
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_session_logs(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("entries").is_some());
    let entries = result["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
}

// ============================================================================
// Networking Tests
// ============================================================================

#[tokio::test]
async fn test_create_aa_vpc_peering_destination_region() {
    let server = MockCloudServer::start().await;

    // The mock only matches when the request body carries both the source and
    // destination regions, so a successful call proves `destination_region`
    // was wired through to the upstream request.
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/regions/peerings"))
        .and(body_partial_json(json!({
            "sourceRegion": "us-east-1",
            "destinationRegion": "us-west-2"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-aa-001",
            "commandType": "CREATE_AA_VPC_PEERING",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client_write(client));
    let tool = cloud::create_aa_vpc_peering(state);

    let result = call_tool_json(
        &tool,
        json!({
            "subscription_id": 123,
            "provider": "AWS",
            "aws_region": "us-east-1",
            "destination_region": "us-west-2",
            "aws_account_id": "123456789012",
            "vpc_id": "vpc-abcdef01"
        }),
    )
    .await;

    assert_eq!(result["taskId"], "task-aa-001");
}
