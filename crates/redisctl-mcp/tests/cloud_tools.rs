#![cfg(feature = "cloud")]
//! Integration tests for Redis Cloud MCP tools using mock server

use std::sync::Arc;

use redis_cloud::testing::{
    AccountFixture, DatabaseFixture, Mock, MockCloudServer, SubscriptionFixture, TaskFixture,
    UserFixture, method, path, query_param,
};
use serde_json::json;
use tower_mcp::Tool;
use wiremock::ResponseTemplate;
// NOTE: `redis_cloud::testing` re-exports wiremock's `body_json`, but in wiremock
// 0.6 that matcher is an *exact* whole-body comparison (`BodyExactMatcher`). The
// request-contract tests below only want to assert the key fields a tool is
// supposed to send while tolerating server-populated / skipped fields, so they use
// `body_partial_json`, which does a genuine partial (subset) match.
use wiremock::matchers::body_partial_json;

// Import the tools and state from the MCP crate
use redisctl_mcp::policy::{Policy, PolicyConfig, SafetyTier};
use redisctl_mcp::state::AppState;
use redisctl_mcp::tools::cloud;

/// Create an AppState with full-tier policy for testing write/destructive tools.
#[cfg(feature = "cloud")]
fn full_policy_state(client: redis_cloud::CloudClient) -> Arc<AppState> {
    let mut state = AppState::with_cloud_client(client);
    state.policy = Arc::new(Policy::new(
        PolicyConfig {
            tier: SafetyTier::Full,
            ..Default::default()
        },
        std::collections::HashMap::new(),
        "test-full".to_string(),
    ));
    Arc::new(state)
}

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

// ============================================================================
// Section 1: Subscriptions — strict request shapes
// ============================================================================

#[tokio::test]
async fn test_update_subscription_cidr_allowlist_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/cidr"))
        .and(body_partial_json(json!({
            "cidrIps": ["10.0.0.0/8"]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-cidr-update",
            "commandType": "updateSubscriptionCidrAllowlist",
            "status": "processing-in-progress",
            "description": "Task in progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_subscription_cidr_allowlist(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "cidr_ips": ["10.0.0.0/8"]
        }),
    )
    .await;

    // If the mock matched (correct body shape), we get the task response.
    // If it didn't match (wrong body), wiremock returns 404 and the tool errors.
    assert!(
        result.contains("task-cidr-update") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

/// KEY REGRESSION TEST: DELETE /subscriptions/{id}/regions sends a body.
/// A loose mock (method+path only) would accept any DELETE — including one
/// that silently drops the body. The body_partial_json matcher here ensures
/// the tool actually serializes the regions list.
#[tokio::test]
async fn test_delete_active_active_regions_bodyful_delete() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/regions"))
        .and(body_partial_json(json!({
            "regions": [{"region": "us-east-1"}]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-regions",
            "commandType": "deleteActiveActiveRegions",
            "status": "processing-in-progress",
            "description": "Task in progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_active_active_regions(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "regions": [{"region": "us-east-1"}]
        }),
    )
    .await;

    assert!(
        result.contains("task-delete-regions") || result.contains("taskId"),
        "Expected task response — bodyful DELETE body was not sent correctly: {result}"
    );
}

#[tokio::test]
async fn test_get_redis_versions_with_subscription_id_query_param() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/redis-versions"))
        .and(query_param("subscriptionId", "123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "redisVersions": [
                {"version": "7.2", "default": true},
                {"version": "7.0", "default": false}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_redis_versions(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    // If the query_param matcher matched, we get the versions back.
    // If the URL didn't include subscriptionId=123, wiremock would return 404.
    assert!(
        result.contains("7.2") || result.contains("redisVersions"),
        "Expected versions response with query param match, got: {result}"
    );
}

// ============================================================================
// Section 2: Networking — strict request shapes
// ============================================================================

#[tokio::test]
async fn test_get_vpc_peering_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/peerings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": 123,
            "peerings": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_vpc_peering(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET peerings should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_vpc_peering_request_shape() {
    let server = MockCloudServer::start().await;

    // The VpcPeeringCreateRequest serializes:
    //   aws_region -> "region" (explicit rename)
    //   aws_account_id -> "awsAccountId" (camelCase)
    //   vpc_id -> "vpcId" (camelCase)
    //   vpc_cidr -> "vpcCidr" (camelCase)
    //   provider -> "provider"
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/peerings"))
        .and(body_partial_json(json!({
            "provider": "AWS",
            "region": "us-east-1",
            "awsAccountId": "123456789012",
            "vpcId": "vpc-abc123"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-peering",
            "commandType": "createSubscriptionPeering",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_vpc_peering(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "provider": "AWS",
            "vpc_id": "vpc-abc123",
            "aws_region": "us-east-1",
            "aws_account_id": "123456789012",
            "vpc_cidr": "10.0.0.0/16"
        }),
    )
    .await;

    assert!(
        result.contains("task-create-peering") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_vpc_peering_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/peerings/456"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-peering",
            "commandType": "deleteSubscriptionPeering",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_vpc_peering(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "peering_id": 456
        }),
    )
    .await;

    assert!(
        result.contains("task-delete-peering") || result.contains("taskId"),
        "Expected task response for DELETE peering, got: {result}"
    );
}

// ============================================================================
// Section 3: Fixed subscriptions — strict request shapes
// ============================================================================

#[tokio::test]
async fn test_create_fixed_subscription_request_shape() {
    let server = MockCloudServer::start().await;

    // FixedSubscriptionCreateRequest: name -> "name", plan_id -> "planId" (camelCase)
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions"))
        .and(body_partial_json(json!({
            "name": "my-essentials",
            "planId": 42
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-fixed-sub",
            "commandType": "createFixedSubscription",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_fixed_subscription(state);

    let result = call_tool_text(
        &tool,
        json!({
            "name": "my-essentials",
            "plan_id": 42
        }),
    )
    .await;

    assert!(
        result.contains("task-create-fixed-sub") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

#[tokio::test]
async fn test_update_fixed_subscription_request_shape() {
    let server = MockCloudServer::start().await;

    // FixedSubscriptionUpdateRequest: name -> "name" (single-word, camelCase is identity)
    Mock::given(method("PUT"))
        .and(path("/fixed/subscriptions/789"))
        .and(body_partial_json(json!({
            "name": "updated-name"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-fixed-sub",
            "commandType": "updateFixedSubscription",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_fixed_subscription(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 789,
            "name": "updated-name"
        }),
    )
    .await;

    assert!(
        result.contains("task-update-fixed-sub") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_fixed_subscription_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/fixed/subscriptions/789"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-fixed-sub",
            "commandType": "deleteFixedSubscription",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_fixed_subscription(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 789})).await;

    assert!(
        result.contains("task-delete-fixed-sub") || result.contains("taskId"),
        "Expected task response for DELETE fixed subscription, got: {result}"
    );
}

// ============================================================================
// Section 4: Account / ACL — strict request shapes
// ============================================================================

#[tokio::test]
async fn test_create_acl_user_request_shape() {
    let server = MockCloudServer::start().await;

    // AclUserCreateRequest: name -> "name", role -> "role", password -> "password"
    Mock::given(method("POST"))
        .and(path("/acl/users"))
        .and(body_partial_json(json!({
            "name": "test-user",
            "role": "some-role"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-acl-user",
            "commandType": "createAclUser",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_acl_user(state);

    let result = call_tool_text(
        &tool,
        json!({
            "name": "test-user",
            "role": "some-role",
            "password": "s3cr3t"
        }),
    )
    .await;

    assert!(
        result.contains("task-create-acl-user") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

#[tokio::test]
async fn test_create_redis_rule_request_shape() {
    let server = MockCloudServer::start().await;

    // AclRedisRuleCreateRequest: name -> "name", redis_rule -> "redisRule" (camelCase)
    Mock::given(method("POST"))
        .and(path("/acl/redisRules"))
        .and(body_partial_json(json!({
            "name": "my-rule",
            "redisRule": "+@read"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-redis-rule",
            "commandType": "createRedisRule",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_redis_rule(state);

    let result = call_tool_text(
        &tool,
        json!({
            "name": "my-rule",
            "redis_rule": "+@read"
        }),
    )
    .await;

    assert!(
        result.contains("task-create-redis-rule") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_acl_role_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/acl/roles/99"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-acl-role",
            "commandType": "deleteAclRole",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_acl_role(state);

    let result = call_tool_text(&tool, json!({"role_id": 99})).await;

    assert!(
        result.contains("task-delete-acl-role") || result.contains("taskId"),
        "Expected task response for DELETE ACL role, got: {result}"
    );
}

// ============================================================================
// Section 5: Subscription write tools — strict request shapes
// ============================================================================

#[tokio::test]
async fn test_update_subscription_request_shape() {
    let server = MockCloudServer::start().await;

    // update_subscription sends an empty BaseSubscriptionUpdateRequest body via PUT.
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-sub",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_subscription(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        result.contains("task-update-sub") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

#[tokio::test]
async fn test_get_subscription_cidr_allowlist_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/cidr"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cidrIps": ["10.0.0.0/8"],
            "securityGroupIds": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_subscription_cidr_allowlist(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET cidr response, got: {result}"
    );
}

#[tokio::test]
async fn test_get_subscription_maintenance_windows_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/maintenance-windows"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mode": "automatic",
            "timeZone": "UTC",
            "windows": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_subscription_maintenance_windows(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 123})).await;

    assert_eq!(result["mode"], "automatic");
}

#[tokio::test]
async fn test_update_subscription_maintenance_windows_request_shape() {
    let server = MockCloudServer::start().await;

    // Verify the request body contains the "mode" field.
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/maintenance-windows"))
        .and(body_partial_json(json!({
            "mode": "automatic"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-mw",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_subscription_maintenance_windows(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "mode": "automatic"
        }),
    )
    .await;

    assert!(
        result.contains("task-update-mw") || result.contains("taskId"),
        "Expected task response, got: {result}"
    );
}

#[tokio::test]
async fn test_get_active_active_regions_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/regions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptionId": 123
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_active_active_regions(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET regions response, got: {result}"
    );
}

#[tokio::test]
async fn test_add_active_active_region_request_shape() {
    let server = MockCloudServer::start().await;

    // Verify the POST body carries deploymentCidr (camelCase from serde rename_all = "camelCase").
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/regions"))
        .and(body_partial_json(json!({
            "deploymentCidr": "10.1.0.0/24"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-add-region",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::add_active_active_region(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "deployment_cidr": "10.1.0.0/24"
        }),
    )
    .await;

    assert!(
        result.contains("task-add-region") || result.contains("taskId"),
        "Expected task response for add region, got: {result}"
    );
}

#[tokio::test]
async fn test_get_subscription_pricing_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/pricing"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "pricing": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_subscription_pricing(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET pricing response, got: {result}"
    );
}

// ============================================================================
// Section 6: Database tag tools — strict request shapes
// ============================================================================

#[tokio::test]
async fn test_create_database_tag_request_shape() {
    let server = MockCloudServer::start().await;

    // DatabaseTagCreateRequest serializes key and value directly (no rename — they're already
    // single lower-case words, camelCase is identity for them).
    // create_tag returns CloudTag (not TaskStateUpdate).
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/databases/1001/tags"))
        .and(body_partial_json(json!({
            "key": "env",
            "value": "prod"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "key": "env",
            "value": "prod"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_database_tag(state);

    let result = call_tool_json(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "key": "env",
            "value": "prod"
        }),
    )
    .await;

    assert_eq!(
        result["key"], "env",
        "Expected tag key in response, got: {result}"
    );
}

#[tokio::test]
async fn test_update_database_tag_request_shape() {
    let server = MockCloudServer::start().await;

    // PUT /subscriptions/{sub}/databases/{db}/tags/{key} — body: {"value": "..."}
    // update_tag returns CloudTag (not TaskStateUpdate).
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/databases/1001/tags/env"))
        .and(body_partial_json(json!({
            "value": "staging"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": "env",
            "value": "staging"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_database_tag(state);

    let result = call_tool_json(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "tag_key": "env",
            "value": "staging"
        }),
    )
    .await;

    assert_eq!(
        result["value"], "staging",
        "Expected updated tag value in response, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_database_tag_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/databases/1001/tags/env"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-tag",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_database_tag(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "tag_key": "env"
        }),
    )
    .await;

    assert!(
        result.contains("task-delete-tag") || result.contains("taskId"),
        "Expected task response for DELETE tag, got: {result}"
    );
}

#[tokio::test]
async fn test_update_database_tags_request_shape() {
    let server = MockCloudServer::start().await;

    // PUT /subscriptions/{sub}/databases/{db}/tags — body: {"tags": [{key, value}]}
    // DatabaseTagsUpdateRequest serializes as camelCase, but "tags" is already camelCase-safe.
    // update_tags returns CloudTags (an HATEOAS envelope, not TaskStateUpdate).
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/databases/1001/tags"))
        .and(body_partial_json(json!({
            "tags": [{"key": "env", "value": "prod"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accountId": 12345
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_database_tags(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "tags": [{"key": "env", "value": "prod"}]
        }),
    )
    .await;

    // If the mock matched (correct body shape), we get a CloudTags response.
    // If it didn't match (wrong tags body), wiremock falls through and the tool errors.
    assert!(
        !result.contains("Failed"),
        "Expected successful update tags response — body_partial_json matcher was not satisfied: {result}"
    );
}

// ============================================================================
// Section 7: Database upgrade and status tools — strict request shapes
// ============================================================================

#[tokio::test]
async fn test_upgrade_database_redis_version_request_shape() {
    let server = MockCloudServer::start().await;

    // DatabaseUpgradeRedisVersionRequest serializes targetRedisVersion (camelCase).
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/databases/1001/upgrade"))
        .and(body_partial_json(json!({
            "targetRedisVersion": "7.4"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-upgrade-version",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::upgrade_database_redis_version(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "target_redis_version": "7.4"
        }),
    )
    .await;

    assert!(
        result.contains("task-upgrade-version") || result.contains("taskId"),
        "Expected task response for upgrade, got: {result}"
    );
}

#[tokio::test]
async fn test_get_database_upgrade_status_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/1001/upgrade"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "upgradeStatus": "completed",
            "targetRedisVersion": "7.4"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_database_upgrade_status(state);

    let result = call_tool_json(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001
        }),
    )
    .await;

    assert_eq!(result["upgradeStatus"], "completed");
}

#[tokio::test]
async fn test_get_database_import_status_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/1001/import"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "completed",
            "importedRdb": true
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_database_import_status(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "Expected successful import status response, got: {result}"
    );
}

#[tokio::test]
async fn test_get_available_database_versions_request_shape() {
    let server = MockCloudServer::start().await;

    // get_available_target_versions calls:
    // GET /subscriptions/{sub}/databases/{db}/available-target-versions
    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/databases/1001/available-target-versions",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "versions": ["7.2", "7.4"]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_available_database_versions(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "Expected successful available versions response, got: {result}"
    );
}

#[tokio::test]
async fn test_get_database_certificate_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/1001/certificate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "publicCertificatePemString": "-----BEGIN CERTIFICATE-----\nMIIBx...\n-----END CERTIFICATE-----"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_database_certificate(state);

    let result = call_tool_json(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001
        }),
    )
    .await;

    assert!(
        result.get("publicCertificatePemString").is_some(),
        "Expected certificate PEM in response, got: {result}"
    );
}
