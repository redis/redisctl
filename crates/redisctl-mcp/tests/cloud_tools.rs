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
// Section 3B: Fixed plan listing — response-content assertions (#1027)
//
// redis-cloud <0.11 silently dropped the `plans` array from FixedSubscriptionsPlans
// responses. 0.11 populates it. These tests feed a response WITH plans and assert
// the tool surfaces them.
// ============================================================================

#[tokio::test]
async fn test_list_fixed_plans_populated() {
    // Verify list_fixed_plans surfaces the `plans` array from the response.
    // In redis-cloud <0.11 the plans array was silently dropped; 0.11 populates it.
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/plans"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plans": [
                {
                    "id": 1,
                    "name": "30MB",
                    "size": 30,
                    "sizeMeasurementUnit": "MB",
                    "provider": "AWS",
                    "region": "us-east-1",
                    "price": 7,
                    "priceCurrency": "USD",
                    "pricePeriod": "Month"
                },
                {
                    "id": 2,
                    "name": "250MB",
                    "size": 250,
                    "sizeMeasurementUnit": "MB",
                    "provider": "AWS",
                    "region": "us-east-1",
                    "price": 30,
                    "priceCurrency": "USD",
                    "pricePeriod": "Month"
                }
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_fixed_plans(state);

    let result = call_tool_json(&tool, json!({})).await;

    let plans = result["plans"]
        .as_array()
        .expect("plans array must be present in list_fixed_plans response");
    assert_eq!(plans.len(), 2, "Expected 2 plans, got: {result}");
    assert_eq!(plans[0]["id"], 1, "Expected first plan id=1, got: {result}");
    assert_eq!(
        plans[0]["name"], "30MB",
        "Expected first plan name=30MB, got: {result}"
    );
    assert_eq!(
        plans[1]["id"], 2,
        "Expected second plan id=2, got: {result}"
    );
}

#[tokio::test]
async fn test_get_fixed_plans_by_subscription_populated() {
    // Verify get_fixed_plans_by_subscription surfaces the `plans` array.
    // In redis-cloud <0.11 the plans array was silently dropped; 0.11 populates it.
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/plans/subscriptions/789"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plans": [
                {
                    "id": 3,
                    "name": "1GB",
                    "size": 1,
                    "sizeMeasurementUnit": "GB",
                    "provider": "GCP",
                    "region": "us-central1",
                    "price": 98,
                    "priceCurrency": "USD",
                    "pricePeriod": "Month"
                }
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_plans_by_subscription(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 789})).await;

    let plans = result["plans"]
        .as_array()
        .expect("plans array must be present in get_fixed_plans_by_subscription response");
    assert_eq!(plans.len(), 1, "Expected 1 plan, got: {result}");
    assert_eq!(plans[0]["id"], 3, "Expected plan id=3, got: {result}");
    assert_eq!(
        plans[0]["name"], "1GB",
        "Expected plan name=1GB, got: {result}"
    );
}

// ============================================================================
// Section 3C: Fixed subscription / plan reads — request shapes (#997)
//
// The fixed-plan catalog and subscription read tools had no coverage. Each test
// pins the exact method + URL path the tool issues against the redis-cloud client
// so a path regression in the handler surfaces as a mock miss ("Failed" in the
// tool output) rather than a silent wrong call.
// ============================================================================

#[tokio::test]
async fn test_list_fixed_subscriptions_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [
                {"id": 101, "name": "essentials-a", "status": "active"}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_fixed_subscriptions(state);

    let result = call_tool_json(&tool, json!({})).await;

    let subs = result["subscriptions"]
        .as_array()
        .expect("subscriptions array must be present in list_fixed_subscriptions response");
    assert_eq!(subs.len(), 1, "Expected 1 subscription, got: {result}");
    assert_eq!(subs[0]["id"], 101, "Expected sub id=101, got: {result}");
}

#[tokio::test]
async fn test_get_fixed_subscription_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/789"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 789,
            "name": "my-essentials",
            "status": "active"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_subscription(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 789})).await;

    assert_eq!(result["id"], 789, "Expected sub id=789, got: {result}");
    assert_eq!(
        result["name"], "my-essentials",
        "Expected sub name, got: {result}"
    );
}

#[tokio::test]
async fn test_get_fixed_plan_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/plans/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42,
            "name": "250MB",
            "size": 250,
            "sizeMeasurementUnit": "MB",
            "provider": "AWS",
            "region": "us-east-1"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_plan(state);

    let result = call_tool_json(&tool, json!({"plan_id": 42})).await;

    assert_eq!(result["id"], 42, "Expected plan id=42, got: {result}");
    assert_eq!(result["name"], "250MB", "Expected plan name, got: {result}");
}

#[tokio::test]
async fn test_get_fixed_redis_versions_request_shape() {
    let server = MockCloudServer::start().await;

    // Handler passes the subscription as a `subscriptionId` query param.
    Mock::given(method("GET"))
        .and(path("/fixed/redis-versions"))
        .and(query_param("subscriptionId", "789"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "redisVersions": [
                {"version": "7.2", "isDefault": true},
                {"version": "7.4", "isDefault": false}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_redis_versions(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 789})).await;

    let versions = result["redisVersions"]
        .as_array()
        .expect("redisVersions array must be present, got: {result}");
    assert_eq!(versions.len(), 2, "Expected 2 versions, got: {result}");
}

// ============================================================================
// Section 3D: Fixed database lifecycle — request shapes (#997)
// ============================================================================

#[tokio::test]
async fn test_list_fixed_databases_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/123/databases"))
        // The Essentials list response wraps databases under a single
        // `subscription` object (Pro uses an array).
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": {
                "subscriptionId": 123,
                "numberOfDatabases": 1,
                "databases": [
                    {"databaseId": 1, "name": "demo-db"}
                ]
            }
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_fixed_databases(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET /fixed/subscriptions/123/databases should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_fixed_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/123/databases/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "databaseId": 1,
            "name": "demo-db",
            "status": "active"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert_eq!(
        result["databaseId"], 1,
        "Expected databaseId=1, got: {result}"
    );
    assert_eq!(result["name"], "demo-db", "Expected db name, got: {result}");
}

#[tokio::test]
async fn test_create_fixed_database_request_shape() {
    let server = MockCloudServer::start().await;

    // FixedDatabaseCreateRequest is camelCase; `name` is the only required field.
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions/123/databases"))
        .and(body_partial_json(json!({"name": "demo-db"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-fixed-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_fixed_database(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "name": "demo-db"
        }),
    )
    .await;

    assert!(
        result.contains("task-create-fixed-db") || result.contains("taskId"),
        "Expected task response for create_fixed_database, got: {result}"
    );
}

#[tokio::test]
async fn test_update_fixed_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/fixed/subscriptions/123/databases/1"))
        .and(body_partial_json(json!({"name": "renamed-db"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-fixed-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_fixed_database(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1,
            "name": "renamed-db"
        }),
    )
    .await;

    assert!(
        result.contains("task-update-fixed-db") || result.contains("taskId"),
        "Expected task response for update_fixed_database, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_fixed_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/fixed/subscriptions/123/databases/1"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-fixed-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_fixed_database(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        result.contains("task-delete-fixed-db") || result.contains("taskId"),
        "Expected task response for DELETE fixed database, got: {result}"
    );
}

// ============================================================================
// Section 3E: Fixed database operations — request shapes (#997)
// ============================================================================

#[tokio::test]
async fn test_get_fixed_database_backup_status_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/123/databases/1/backup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "task-backup-status",
            "status": "processing-completed"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database_backup_status(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET .../backup should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_backup_fixed_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions/123/databases/1/backup"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-backup-fixed-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::backup_fixed_database(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        result.contains("task-backup-fixed-db") || result.contains("taskId"),
        "Expected task response for backup_fixed_database, got: {result}"
    );
}

#[tokio::test]
async fn test_get_fixed_database_import_status_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/123/databases/1/import"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "task-import-status",
            "status": "processing-completed"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database_import_status(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET .../import should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_import_fixed_database_request_shape() {
    let server = MockCloudServer::start().await;

    // FixedDatabaseImportRequest is camelCase: source_type -> "sourceType",
    // import_from_uri -> "importFromUri".
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions/123/databases/1/import"))
        .and(body_partial_json(json!({
            "sourceType": "http",
            "importFromUri": ["https://example.com/dump.rdb"]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-import-fixed-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::import_fixed_database(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1,
            "source_type": "http",
            "import_from_uri": ["https://example.com/dump.rdb"]
        }),
    )
    .await;

    assert!(
        result.contains("task-import-fixed-db") || result.contains("taskId"),
        "Expected task response for import_fixed_database, got: {result}"
    );
}

#[tokio::test]
async fn test_get_fixed_database_slow_log_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/123/databases/1/slow-log"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "entries": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database_slow_log(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET .../slow-log should have matched, got: {result}"
    );
}

// ============================================================================
// Section 3F: Fixed database tags — request shapes (#997)
// ============================================================================

#[tokio::test]
async fn test_get_fixed_database_tags_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/123/databases/1/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tags": [
                {"key": "env", "value": "prod"}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database_tags(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET .../tags should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_fixed_database_tag_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions/123/databases/1/tags"))
        .and(body_partial_json(json!({"key": "env", "value": "prod"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": "env",
            "value": "prod"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_fixed_database_tag(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1,
            "key": "env",
            "value": "prod"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST .../tags should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_update_fixed_database_tag_request_shape() {
    let server = MockCloudServer::start().await;

    // update_tag targets the tag key in the path; body carries the new value.
    Mock::given(method("PUT"))
        .and(path("/fixed/subscriptions/123/databases/1/tags/env"))
        .and(body_partial_json(json!({"value": "staging"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": "env",
            "value": "staging"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_fixed_database_tag(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1,
            "tag_key": "env",
            "value": "staging"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT .../tags/env should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_fixed_database_tag_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/fixed/subscriptions/123/databases/1/tags/env"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_fixed_database_tag(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1,
            "tag_key": "env"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "DELETE .../tags/env should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_update_fixed_database_tags_request_shape() {
    let server = MockCloudServer::start().await;

    // update_tags replaces the whole tag set with a PUT to .../tags.
    Mock::given(method("PUT"))
        .and(path("/fixed/subscriptions/123/databases/1/tags"))
        .and(body_partial_json(json!({
            "tags": [{"key": "env", "value": "prod"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tags": [{"key": "env", "value": "prod"}]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_fixed_database_tags(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1,
            "tags": [{"key": "env", "value": "prod"}]
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT .../tags (replace) should have matched, got: {result}"
    );
}

// ============================================================================
// Section 3G: Fixed database Redis-version upgrade — request shapes (#997)
// ============================================================================

#[tokio::test]
async fn test_get_fixed_database_upgrade_versions_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/fixed/subscriptions/123/databases/1/available-target-versions",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "targetVersions": ["7.4"]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database_upgrade_versions(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET .../available-target-versions should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_fixed_database_upgrade_status_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/123/databases/1/upgrade"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "processing-completed"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database_upgrade_status(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET .../upgrade (status) should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_upgrade_fixed_database_redis_version_request_shape() {
    let server = MockCloudServer::start().await;

    // The handler builds the body as {"targetVersion": <version>} and POSTs it.
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions/123/databases/1/upgrade"))
        .and(body_partial_json(json!({"targetVersion": "7.4"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-upgrade-fixed-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::upgrade_fixed_database_redis_version(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1,
            "target_version": "7.4"
        }),
    )
    .await;

    assert!(
        result.contains("task-upgrade-fixed-db") || result.contains("taskId"),
        "Expected task response for upgrade_fixed_database_redis_version, got: {result}"
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

    // update_subscription requires at least one of name, payment_method_id, or payment_method.
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123"))
        .and(body_partial_json(json!({"name": "Updated Subscription"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-sub",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_subscription(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "name": "Updated Subscription"
        }),
    )
    .await;

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
// Section 6B: Database tag response-content assertions (#1027)
//
// redis-cloud <0.11 silently dropped the `tags` array from CloudTags responses.
// 0.11 populates it. These tests feed a response WITH tags and assert the tool
// surfaces them (Pro and Essentials).
// ============================================================================

#[tokio::test]
async fn test_get_database_tags_populated() {
    // Verify get_tags (Pro) surfaces the `tags` array with actual tag data.
    // In redis-cloud <0.11 the tags array was silently dropped; 0.11 populates it.
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/1001/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tags": [
                {"key": "env", "value": "prod"},
                {"key": "tier", "value": "standard"}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_tags(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 123, "database_id": 1001})).await;

    let tags = result["tags"]
        .as_array()
        .expect("tags array must be present in get_tags response");
    assert_eq!(tags.len(), 2, "Expected 2 tags, got: {result}");
    assert_eq!(
        tags[0]["key"], "env",
        "Expected first tag key=env, got: {result}"
    );
    assert_eq!(
        tags[0]["value"], "prod",
        "Expected first tag value=prod, got: {result}"
    );
    assert_eq!(
        tags[1]["key"], "tier",
        "Expected second tag key=tier, got: {result}"
    );
}

#[tokio::test]
async fn test_get_fixed_database_tags_populated() {
    // Verify get_fixed_database_tags (Essentials) surfaces the `tags` array.
    // Endpoint: GET /fixed/subscriptions/{subId}/databases/{dbId}/tags
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/789/databases/2001/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tags": [
                {"key": "app", "value": "web"},
                {"key": "owner", "value": "platform"}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_fixed_database_tags(state);

    let result = call_tool_json(&tool, json!({"subscription_id": 789, "database_id": 2001})).await;

    let tags = result["tags"]
        .as_array()
        .expect("tags array must be present in get_fixed_database_tags response");
    assert_eq!(tags.len(), 2, "Expected 2 tags, got: {result}");
    assert_eq!(
        tags[0]["key"], "app",
        "Expected first tag key=app, got: {result}"
    );
    assert_eq!(
        tags[1]["key"], "owner",
        "Expected second tag key=owner, got: {result}"
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
            "publicCertificatePEMString": "-----BEGIN CERTIFICATE-----\nMIIBx...\n-----END CERTIFICATE-----"
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
        result.get("publicCertificatePEMString").is_some(),
        "Expected certificate PEM in response, got: {result}"
    );
}

// ============================================================================
// Section 4: Networking breadth — request shapes for every tool family
//
// These tests exercise at least one tool per networking primitive (VPC peering,
// Transit Gateway, PSC, PrivateLink) in both Pro and Active-Active variants.
// Each test mounts a method+path (and, where a body matters, a body_partial_json)
// matcher. If the tool builds the wrong URL or drops the body, wiremock returns
// 404 and the tool surfaces "Failed", which the assertions catch.
// ============================================================================

/// Standard 202 task response used by write/destructive networking tools.
fn net_task_body() -> serde_json::Value {
    json!({"taskId": "task-net-test", "status": "processing-in-progress"})
}

// ===========================================================================
// Section A: VPC Peering — remaining Pro + all Active-Active
// ===========================================================================

#[tokio::test]
async fn test_update_vpc_peering_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/peerings/456"))
        .and(body_partial_json(json!({"vpcCidr": "10.0.0.0/16"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_vpc_peering(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "peering_id": 456,
            "vpc_cidr": "10.0.0.0/16"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT peerings/456 with vpcCidr should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_aa_vpc_peering_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/regions/peerings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_vpc_peering(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET regions/peerings should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_update_aa_vpc_peering_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/regions/peerings/456"))
        .and(body_partial_json(json!({"vpcCidr": "10.1.0.0/16"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_aa_vpc_peering(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "peering_id": 456,
            "vpc_cidr": "10.1.0.0/16"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT regions/peerings/456 with vpcCidr should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_aa_vpc_peering_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/regions/peerings/456"))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_aa_vpc_peering(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "peering_id": 456})).await;

    assert!(
        !result.contains("Failed"),
        "DELETE regions/peerings/456 should have matched, got: {result}"
    );
}

// ===========================================================================
// Section B: Transit Gateway (Pro)
// ===========================================================================

#[tokio::test]
async fn test_get_tgw_attachments_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/transitGateways"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_tgw_attachments(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET transitGateways should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_tgw_invitations_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/transitGateways/invitations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_tgw_invitations(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET transitGateways/invitations should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_tgw_attachment_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/subscriptions/123/transitGateways/tgw-abc/attachment",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_tgw_attachment(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "tgw_id": "tgw-abc"})).await;

    assert!(
        !result.contains("Failed"),
        "POST transitGateways/tgw-abc/attachment should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_update_tgw_attachment_cidrs_request_shape() {
    let server = MockCloudServer::start().await;

    // TgwAttachmentRequest serializes to camelCase: awsAccountId, tgwId, cidrs.
    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/transitGateways/attach-1/attachment",
        ))
        .and(body_partial_json(json!({"cidrs": ["10.0.0.0/24"]})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_tgw_attachment_cidrs(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "attachment_id": "attach-1",
            "cidrs": ["10.0.0.0/24"]
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT transitGateways/attach-1/attachment with cidrs should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_tgw_attachment_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/subscriptions/123/transitGateways/attach-1/attachment",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_tgw_attachment(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "attachment_id": "attach-1"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "DELETE transitGateways/attach-1/attachment should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_accept_tgw_invitation_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/transitGateways/invitations/inv-1/accept",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::accept_tgw_invitation(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "invitation_id": "inv-1"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT transitGateways/invitations/inv-1/accept should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_reject_tgw_invitation_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/transitGateways/invitations/inv-1/reject",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::reject_tgw_invitation(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "invitation_id": "inv-1"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT transitGateways/invitations/inv-1/reject should have matched, got: {result}"
    );
}

// ===========================================================================
// Section C: Transit Gateway (Active-Active)
// ===========================================================================

#[tokio::test]
async fn test_get_aa_tgw_attachments_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/regions/1/transitGateways"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_tgw_attachments(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "region_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET regions/1/transitGateways should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_aa_tgw_invitations_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/regions/1/transitGateways/invitations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_tgw_invitations(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "region_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET regions/1/transitGateways/invitations should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_aa_tgw_attachment_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/subscriptions/123/regions/1/transitGateways/tgw-abc/attachment",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_aa_tgw_attachment(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "region_id": 1, "tgw_id": "tgw-abc"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST regions/1/transitGateways/tgw-abc/attachment should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_update_aa_tgw_attachment_cidrs_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/regions/1/transitGateways/attach-1/attachment",
        ))
        .and(body_partial_json(json!({"cidrs": ["10.2.0.0/24"]})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_aa_tgw_attachment_cidrs(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "region_id": 1,
            "attachment_id": "attach-1",
            "cidrs": ["10.2.0.0/24"]
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT regions/1/transitGateways/attach-1/attachment with cidrs should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_aa_tgw_attachment_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/subscriptions/123/regions/1/transitGateways/attach-1/attachment",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_aa_tgw_attachment(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "region_id": 1, "attachment_id": "attach-1"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "DELETE regions/1/transitGateways/attach-1/attachment should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_accept_aa_tgw_invitation_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/regions/1/transitGateways/invitations/inv-1/accept",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::accept_aa_tgw_invitation(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "region_id": 1, "invitation_id": "inv-1"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT regions/1/transitGateways/invitations/inv-1/accept should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_reject_aa_tgw_invitation_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/regions/1/transitGateways/invitations/inv-1/reject",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::reject_aa_tgw_invitation(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "region_id": 1, "invitation_id": "inv-1"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT regions/1/transitGateways/invitations/inv-1/reject should have matched, got: {result}"
    );
}

// ===========================================================================
// Section D: Private Service Connect (Pro)
// ===========================================================================

#[tokio::test]
async fn test_get_psc_service_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/private-service-connect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_psc_service(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET private-service-connect should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_psc_service_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/private-service-connect"))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_psc_service(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "POST private-service-connect should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_psc_service_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/private-service-connect"))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_psc_service(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "DELETE private-service-connect should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_psc_endpoints_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/private-service-connect/10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_psc_endpoints(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "psc_service_id": 10})).await;

    assert!(
        !result.contains("Failed"),
        "GET private-service-connect/10 should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_psc_endpoint_request_shape() {
    let server = MockCloudServer::start().await;

    // PscEndpointUpdateRequest serializes gcp_project_id -> "gcpProjectId".
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/private-service-connect/10"))
        .and(body_partial_json(json!({"gcpProjectId": "my-gcp-project"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_psc_endpoint(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "psc_service_id": 10,
            "endpoint_id": 20,
            "gcp_project_id": "my-gcp-project"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST private-service-connect/10 with gcpProjectId should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_update_psc_endpoint_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/private-service-connect/10/endpoints/20",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_psc_endpoint(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "psc_service_id": 10,
            "endpoint_id": 20,
            "gcp_project_id": "my-gcp-project"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT private-service-connect/10/endpoints/20 should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_psc_endpoint_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/subscriptions/123/private-service-connect/10/endpoints/20",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_psc_endpoint(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "psc_service_id": 10, "endpoint_id": 20}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "DELETE private-service-connect/10/endpoints/20 should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_psc_creation_script_request_shape() {
    let server = MockCloudServer::start().await;

    // The handler returns Result<String>; the client deserializes a JSON string.
    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/private-service-connect/10/endpoints/20/creationScripts",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!("#!/bin/bash\necho create-endpoint")),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_psc_creation_script(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "psc_service_id": 10, "endpoint_id": 20}),
    )
    .await;

    assert!(
        !result.contains("Failed") && result.contains("create-endpoint"),
        "GET creationScripts should have returned script text, got: {result}"
    );
}

#[tokio::test]
async fn test_get_psc_deletion_script_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/private-service-connect/10/endpoints/20/deletionScripts",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!("#!/bin/bash\necho delete-endpoint")),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_psc_deletion_script(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "psc_service_id": 10, "endpoint_id": 20}),
    )
    .await;

    assert!(
        !result.contains("Failed") && result.contains("delete-endpoint"),
        "GET deletionScripts should have returned script text, got: {result}"
    );
}

// ===========================================================================
// Section E: Private Service Connect (Active-Active)
// ===========================================================================

#[tokio::test]
async fn test_get_aa_psc_service_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/regions/1/private-service-connect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_psc_service(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "region_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET regions/1/private-service-connect should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_aa_psc_service_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/regions/1/private-service-connect"))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_aa_psc_service(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "region_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "POST regions/1/private-service-connect should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_aa_psc_service_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/regions/1/private-service-connect"))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_aa_psc_service(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "region_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "DELETE regions/1/private-service-connect should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_aa_psc_endpoints_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/regions/1/private-service-connect/10",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_psc_endpoints(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "region_id": 1, "psc_service_id": 10}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "GET regions/1/private-service-connect/10 should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_aa_psc_endpoint_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/subscriptions/123/regions/1/private-service-connect/10",
        ))
        .and(body_partial_json(json!({"gcpProjectId": "my-gcp-project"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_aa_psc_endpoint(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "region_id": 1,
            "psc_service_id": 10,
            "endpoint_id": 20,
            "gcp_project_id": "my-gcp-project"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST regions/1/private-service-connect/10 with gcpProjectId should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_update_aa_psc_endpoint_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path(
            "/subscriptions/123/regions/1/private-service-connect/10/endpoints/20",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_aa_psc_endpoint(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "region_id": 1,
            "psc_service_id": 10,
            "endpoint_id": 20,
            "gcp_project_id": "my-gcp-project"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "PUT regions/1/private-service-connect/10/endpoints/20 should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_aa_psc_endpoint_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/subscriptions/123/regions/1/private-service-connect/10/endpoints/20",
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_aa_psc_endpoint(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "region_id": 1,
            "psc_service_id": 10,
            "endpoint_id": 20
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "DELETE regions/1/private-service-connect/10/endpoints/20 should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_aa_psc_creation_script_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/regions/1/private-service-connect/10/endpoints/20/creationScripts",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!("#!/bin/bash\necho aa-create")),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_psc_creation_script(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "region_id": 1,
            "psc_service_id": 10,
            "endpoint_id": 20
        }),
    )
    .await;

    assert!(
        !result.contains("Failed") && result.contains("aa-create"),
        "GET AA creationScripts should have returned script text, got: {result}"
    );
}

#[tokio::test]
async fn test_get_aa_psc_deletion_script_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/regions/1/private-service-connect/10/endpoints/20/deletionScripts",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!("#!/bin/bash\necho aa-delete")),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_psc_deletion_script(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "region_id": 1,
            "psc_service_id": 10,
            "endpoint_id": 20
        }),
    )
    .await;

    assert!(
        !result.contains("Failed") && result.contains("aa-delete"),
        "GET AA deletionScripts should have returned script text, got: {result}"
    );
}

// ===========================================================================
// Section F: PrivateLink (Pro)
// ===========================================================================

#[tokio::test]
async fn test_get_private_link_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/private-link"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_private_link(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET private-link should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_private_link_request_shape() {
    let server = MockCloudServer::start().await;

    // PrivateLinkCreateRequest serializes share_name -> "shareName",
    // principal_type -> "type" with snake_case PrincipalType values.
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/private-link"))
        .and(body_partial_json(json!({
            "shareName": "my-share",
            "principal": "123456789012",
            "type": "aws_account"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_private_link(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "share_name": "my-share",
            "principal": "123456789012",
            "principal_type": "aws_account"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST private-link with shareName/principal/type should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_private_link_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/private-link"))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_private_link(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "DELETE private-link should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_add_private_link_principals_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/private-link/principals"))
        .and(body_partial_json(json!({"principal": "123456789012"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::add_private_link_principals(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "principal": "123456789012"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST private-link/principals should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_remove_private_link_principals_request_shape() {
    let server = MockCloudServer::start().await;

    // remove_principals uses a bodyful DELETE; the body carries the principal.
    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/private-link/principals"))
        .and(body_partial_json(json!({"principal": "123456789012"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::remove_private_link_principals(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "principal": "123456789012"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "DELETE private-link/principals with body should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_private_link_endpoint_script_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/private-link/endpoint-script"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_private_link_endpoint_script(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123})).await;

    assert!(
        !result.contains("Failed"),
        "GET private-link/endpoint-script should have matched, got: {result}"
    );
}

// ===========================================================================
// Section G: PrivateLink (Active-Active)
// ===========================================================================

#[tokio::test]
async fn test_get_aa_private_link_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/regions/1/private-link"))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_private_link(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "region_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET regions/1/private-link should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_create_aa_private_link_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/regions/1/private-link"))
        .and(body_partial_json(json!({
            "shareName": "my-share",
            "principal": "123456789012",
            "type": "aws_account"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_aa_private_link(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "region_id": 1,
            "share_name": "my-share",
            "principal": "123456789012",
            "principal_type": "aws_account"
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST regions/1/private-link should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_add_aa_private_link_principals_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/regions/1/private-link/principals"))
        .and(body_partial_json(json!({"principal": "123456789012"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::add_aa_private_link_principals(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "region_id": 1, "principal": "123456789012"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "POST regions/1/private-link/principals should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_remove_aa_private_link_principals_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/regions/1/private-link/principals"))
        .and(body_partial_json(json!({"principal": "123456789012"})))
        .respond_with(ResponseTemplate::new(202).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::remove_aa_private_link_principals(state);

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "region_id": 1, "principal": "123456789012"}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "DELETE regions/1/private-link/principals with body should have matched, got: {result}"
    );
}

#[tokio::test]
async fn test_get_aa_private_link_endpoint_script_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/subscriptions/123/regions/1/private-link/endpoint-script",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(net_task_body()))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_aa_private_link_endpoint_script(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "region_id": 1})).await;

    assert!(
        !result.contains("Failed"),
        "GET regions/1/private-link/endpoint-script should have matched, got: {result}"
    );
}

// ============================================================================
// Section 5: Subscription write / destructive / read-only tools — request shapes
//
// Covers the 13 cloud subscription tools that previously had zero test coverage
// (see #991): create/update/backup/import database, create subscription,
// update CRDB local properties (write); delete subscription/database,
// flush database, flush CRDB database (destructive); and get backup status,
// slow log, and tags (read-only).
//
// Several write/destructive tools run the Layer-2 `*_and_wait` workflow: they
// fire the mutating call (returns a task), then poll `GET /tasks/{id}` until the
// task reaches a terminal state, then (for create/update) fetch the resource.
// The task mock returns `processing-completed` on the *first* poll, so the
// workflow never sleeps. Destructive tools additionally assert the
// `destructive_hint` annotation carried by the tool builder.
// ============================================================================

/// A task body that completes immediately, optionally pointing at a resource.
fn completed_task(task_id: &str, resource_id: Option<i64>) -> serde_json::Value {
    let mut body = json!({
        "taskId": task_id,
        "status": "processing-completed",
    });
    if let Some(id) = resource_id {
        body["response"] = json!({ "resourceId": id });
    }
    body
}

#[tokio::test]
async fn test_create_subscription_request_shape() {
    let server = MockCloudServer::start().await;

    // POST /subscriptions carries the nested cloudProviders + databases shape.
    Mock::given(method("POST"))
        .and(path("/subscriptions"))
        .and(body_partial_json(json!({
            "name": "demo-sub",
            "cloudProviders": [{"provider": "AWS", "regions": [{"region": "us-east-1"}]}],
            "databases": [{"name": "demo-db", "memoryLimitInGb": 1.0}]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-sub",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    // Task completes on first poll, pointing at the new subscription.
    Mock::given(method("GET"))
        .and(path("/tasks/task-create-sub"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-create-sub", Some(555))),
        )
        .mount(server.inner())
        .await;

    // Step 3: the workflow fetches the created subscription.
    let subscription = SubscriptionFixture::new(555, "demo-sub")
        .status("active")
        .build();
    server.mock_subscription_get(555, subscription).await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_subscription(state);

    let result = call_tool_text(
        &tool,
        json!({
            "name": "demo-sub",
            "cloud_provider": "AWS",
            "region": "us-east-1",
            "database_name": "demo-db",
            "memory_limit_in_gb": 1.0,
            "timeout_seconds": 5
        }),
    )
    .await;

    assert!(
        !result.contains("Failed") && (result.contains("555") || result.contains("demo-sub")),
        "Expected created subscription, got: {result}"
    );
}

#[tokio::test]
async fn test_create_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/databases"))
        .and(body_partial_json(json!({
            "name": "demo-db",
            "memoryLimitInGb": 1.0
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-create-db"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-create-db", Some(1001))),
        )
        .mount(server.inner())
        .await;

    let database = DatabaseFixture::new(1001, "demo-db").build();
    server.mock_database_get(123, 1001, database).await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_database(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "name": "demo-db",
            "memory_limit_in_gb": 1.0,
            "timeout_seconds": 5
        }),
    )
    .await;

    assert!(
        !result.contains("Failed") && (result.contains("demo-db") || result.contains("1001")),
        "Expected created database, got: {result}"
    );
}

#[tokio::test]
async fn test_update_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/databases/1001"))
        .and(body_partial_json(json!({ "memoryLimitInGb": 2.0 })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-update-db"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-update-db", Some(1001))),
        )
        .mount(server.inner())
        .await;

    let database = DatabaseFixture::new(1001, "demo-db")
        .memory_limit_in_gb(2.0)
        .build();
    server.mock_database_get(123, 1001, database).await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_database(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "memory_limit_in_gb": 2.0,
            "timeout_seconds": 5
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "Expected updated database, got: {result}"
    );
}

#[tokio::test]
async fn test_backup_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/databases/1001/backup"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-backup-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-backup-db"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-backup-db", None)),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::backup_database(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "timeout_seconds": 5
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "Expected successful backup, got: {result}"
    );
}

#[tokio::test]
async fn test_import_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/123/databases/1001/import"))
        .and(body_partial_json(json!({
            "sourceType": "http",
            "importFromUri": ["https://example.com/dump.rdb"]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-import-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-import-db"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-import-db", None)),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::import_database(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "source_type": "http",
            "import_from_uri": "https://example.com/dump.rdb",
            "timeout_seconds": 5
        }),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "Expected successful import, got: {result}"
    );
}

#[tokio::test]
async fn test_update_crdb_local_properties_request_shape() {
    let server = MockCloudServer::start().await;

    // PUT /subscriptions/{s}/databases/{d}/regions — returns a task directly
    // (no _and_wait poll for this tool).
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/databases/1001/regions"))
        .and(body_partial_json(json!({ "name": "aa-db" })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-crdb-update",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_crdb_local_properties(state);

    let result = call_tool_text(
        &tool,
        json!({
            "subscription_id": 123,
            "database_id": 1001,
            "name": "aa-db"
        }),
    )
    .await;

    assert!(
        result.contains("task-crdb-update") || result.contains("taskId"),
        "Expected task response for CRDB update, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_subscription_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-sub",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-delete-sub"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-delete-sub", None)),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_subscription(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "delete_subscription must carry the destructive annotation"
    );

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "timeout_seconds": 5})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful delete_subscription, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_database_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/123/databases/1001"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-delete-db"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-delete-db", None)),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_database(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "delete_database must carry the destructive annotation"
    );

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "database_id": 1001, "timeout_seconds": 5}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "Expected successful delete_database, got: {result}"
    );
}

#[tokio::test]
async fn test_flush_database_request_shape() {
    let server = MockCloudServer::start().await;

    // Standard flush is a PUT with an empty body to .../flush.
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/databases/1001/flush"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-flush-db",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-flush-db"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completed_task("task-flush-db", None)),
        )
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::flush_database(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "flush_database must carry the destructive annotation"
    );

    let result = call_tool_text(
        &tool,
        json!({"subscription_id": 123, "database_id": 1001, "timeout_seconds": 5}),
    )
    .await;

    assert!(
        !result.contains("Failed"),
        "Expected successful flush_database, got: {result}"
    );
}

#[tokio::test]
async fn test_flush_crdb_database_request_shape() {
    let server = MockCloudServer::start().await;

    // CRDB flush is a PUT to the same .../flush path, returning a task directly.
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/databases/1001/flush"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-flush-crdb",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::flush_crdb_database(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "flush_crdb_database must carry the destructive annotation"
    );

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1001})).await;

    assert!(
        result.contains("task-flush-crdb") || result.contains("taskId"),
        "Expected task response for flush_crdb_database, got: {result}"
    );
}

#[tokio::test]
async fn test_get_backup_status_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/1001/backup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "task-backup-status",
            "status": "processing-completed"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_backup_status(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1001})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET backup status, got: {result}"
    );
}

#[tokio::test]
async fn test_get_slow_log_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/1001/slow-log"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "entries": [] })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_slow_log(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1001})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET slow log, got: {result}"
    );
}

#[tokio::test]
async fn test_get_database_tags_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/1001/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tags": [] })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_tags(state);

    let result = call_tool_text(&tool, json!({"subscription_id": 123, "database_id": 1001})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET database tags, got: {result}"
    );
}

// ============================================================================
// Section 8: Account write / destructive / read-only tools -- request shapes
//
// Covers the 22 cloud account tools with zero test coverage (see #994).
// ============================================================================

#[tokio::test]
async fn test_update_acl_role_request_shape() {
    let server = MockCloudServer::start().await;

    // AclRoleUpdateRequest: redis_rules -> "redisRules", rule_name -> "ruleName"
    Mock::given(method("PUT"))
        .and(path("/acl/roles/42"))
        .and(body_partial_json(json!({
            "name": "readonly-role",
            "redisRules": [{
                "ruleName": "+@read",
                "databases": [{"subscriptionId": 1, "databaseId": 2}]
            }]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-role",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_acl_role(state);

    let result = call_tool_text(
        &tool,
        json!({
            "role_id": 42,
            "name": "readonly-role",
            "redis_rules": [{
                "rule_name": "+@read",
                "databases": [{"subscription_id": 1, "database_id": 2}]
            }]
        }),
    )
    .await;

    assert!(
        result.contains("task-update-role") || result.contains("taskId"),
        "Expected task response for update_acl_role, got: {result}"
    );
}

#[tokio::test]
async fn test_create_acl_role_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("POST"))
        .and(path("/acl/roles"))
        .and(body_partial_json(json!({
            "name": "dev-role",
            "redisRules": [{
                "ruleName": "+@all",
                "databases": [{"subscriptionId": 10, "databaseId": 20}]
            }]
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-role",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_acl_role(state);

    let result = call_tool_text(
        &tool,
        json!({
            "name": "dev-role",
            "redis_rules": [{
                "rule_name": "+@all",
                "databases": [{"subscription_id": 10, "database_id": 20}]
            }]
        }),
    )
    .await;

    assert!(
        result.contains("task-create-role") || result.contains("taskId"),
        "Expected task response for create_acl_role, got: {result}"
    );
}

#[tokio::test]
async fn test_update_redis_rule_request_shape() {
    let server = MockCloudServer::start().await;

    // AclRedisRuleUpdateRequest: redis_rule -> "redisRule" (camelCase)
    Mock::given(method("PUT"))
        .and(path("/acl/redisRules/5"))
        .and(body_partial_json(json!({
            "name": "read-only-rule",
            "redisRule": "+@read ~*"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-redis-rule",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_redis_rule(state);

    let result = call_tool_text(
        &tool,
        json!({
            "rule_id": 5,
            "name": "read-only-rule",
            "redis_rule": "+@read ~*"
        }),
    )
    .await;

    assert!(
        result.contains("task-update-redis-rule") || result.contains("taskId"),
        "Expected task response for update_redis_rule, got: {result}"
    );
}

#[tokio::test]
async fn test_update_acl_user_request_shape() {
    let server = MockCloudServer::start().await;

    // AclUserUpdateRequest: role -> "role", password -> "password"
    Mock::given(method("PUT"))
        .and(path("/acl/users/3"))
        .and(body_partial_json(json!({
            "role": "dev-role"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-acl-user",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_acl_user(state);

    let result = call_tool_text(
        &tool,
        json!({
            "user_id": 3,
            "role": "dev-role"
        }),
    )
    .await;

    assert!(
        result.contains("task-update-acl-user") || result.contains("taskId"),
        "Expected task response for update_acl_user, got: {result}"
    );
}

#[tokio::test]
async fn test_update_account_user_request_shape() {
    let server = MockCloudServer::start().await;

    // AccountUserUpdateRequest: name -> "name", role -> "role"
    Mock::given(method("PUT"))
        .and(path("/users/1"))
        .and(body_partial_json(json!({
            "name": "Alice Updated",
            "role": "member"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-account-user",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_account_user(state);

    let result = call_tool_text(
        &tool,
        json!({
            "user_id": 1,
            "name": "Alice Updated",
            "role": "member"
        }),
    )
    .await;

    assert!(
        result.contains("task-update-account-user") || result.contains("taskId"),
        "Expected task response for update_account_user, got: {result}"
    );
}

#[tokio::test]
async fn test_generate_cost_report_request_shape() {
    let server = MockCloudServer::start().await;

    // CostReportCreateRequest: start_date -> "startDate", end_date -> "endDate" (camelCase)
    Mock::given(method("POST"))
        .and(path("/cost-report"))
        .and(body_partial_json(json!({
            "startDate": "2024-01-01",
            "endDate": "2024-01-31"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-cost-report",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::generate_cost_report(state);

    let result = call_tool_text(
        &tool,
        json!({
            "start_date": "2024-01-01",
            "end_date": "2024-01-31"
        }),
    )
    .await;

    assert!(
        result.contains("task-cost-report") || result.contains("taskId"),
        "Expected task response -- startDate/endDate must serialize as camelCase, got: {result}"
    );
}

#[tokio::test]
async fn test_create_cloud_account_request_shape() {
    let server = MockCloudServer::start().await;

    // CloudAccountCreateRequest: all fields are camelCase
    Mock::given(method("POST"))
        .and(path("/cloud-accounts"))
        .and(body_partial_json(json!({
            "name": "my-aws-account",
            "accessKeyId": "AKIAIOSFODNN7EXAMPLE",
            "consoleUsername": "admin",
            "signInLoginUrl": "https://console.aws.amazon.com"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-create-cloud-account",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::create_cloud_account(state);

    let result = call_tool_text(
        &tool,
        json!({
            "name": "my-aws-account",
            "access_key_id": "AKIAIOSFODNN7EXAMPLE",
            "access_secret_key": "wJalrXUtnFEMI/bPxRfiCYEXAMPLEKEY",
            "console_username": "admin",
            "console_password": "s3cr3t",
            "sign_in_login_url": "https://console.aws.amazon.com"
        }),
    )
    .await;

    assert!(
        result.contains("task-create-cloud-account") || result.contains("taskId"),
        "Expected task response for create_cloud_account, got: {result}"
    );
}

#[tokio::test]
async fn test_update_cloud_account_request_shape() {
    let server = MockCloudServer::start().await;

    // CloudAccountUpdateRequest: camelCase field names
    Mock::given(method("PUT"))
        .and(path("/cloud-accounts/10"))
        .and(body_partial_json(json!({
            "accessKeyId": "AKIAIOSFODNN7UPDATED",
            "consoleUsername": "admin"
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-update-cloud-account",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::update_cloud_account(state);

    let result = call_tool_text(
        &tool,
        json!({
            "cloud_account_id": 10,
            "access_key_id": "AKIAIOSFODNN7UPDATED",
            "access_secret_key": "wJalrXUtnFEMI/bPxRfiCYUPDATEDKEY",
            "console_username": "admin",
            "console_password": "newpassword"
        }),
    )
    .await;

    assert!(
        result.contains("task-update-cloud-account") || result.contains("taskId"),
        "Expected task response for update_cloud_account, got: {result}"
    );
}

// ---------------------------------------------------------------------------
// Section 8B: Destructive tools -- path+method + destructive_hint annotation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_account_user_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/users/1"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-account-user",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_account_user(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "delete_account_user must carry the destructive annotation"
    );

    let result = call_tool_text(&tool, json!({"user_id": 1})).await;

    assert!(
        result.contains("task-delete-account-user") || result.contains("taskId"),
        "Expected task response for delete_account_user, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_acl_user_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/acl/users/3"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-acl-user",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_acl_user(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "delete_acl_user must carry the destructive annotation"
    );

    let result = call_tool_text(&tool, json!({"user_id": 3})).await;

    assert!(
        result.contains("task-delete-acl-user") || result.contains("taskId"),
        "Expected task response for delete_acl_user, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_redis_rule_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/acl/redisRules/5"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-redis-rule",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_redis_rule(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "delete_redis_rule must carry the destructive annotation"
    );

    let result = call_tool_text(&tool, json!({"rule_id": 5})).await;

    assert!(
        result.contains("task-delete-redis-rule") || result.contains("taskId"),
        "Expected task response for delete_redis_rule, got: {result}"
    );
}

#[tokio::test]
async fn test_delete_cloud_account_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/cloud-accounts/10"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-delete-cloud-account",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = full_policy_state(client);
    let tool = cloud::delete_cloud_account(state);

    assert!(
        tool.annotations
            .as_ref()
            .is_some_and(|a| a.destructive_hint),
        "delete_cloud_account must carry the destructive annotation"
    );

    let result = call_tool_text(&tool, json!({"cloud_account_id": 10})).await;

    assert!(
        result.contains("task-delete-cloud-account") || result.contains("taskId"),
        "Expected task response for delete_cloud_account, got: {result}"
    );
}

// ---------------------------------------------------------------------------
// Section 8C: Read-only tools -- correct URL construction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_account_user_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/users/5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5,
            "name": "Alice",
            "email": "alice@example.com",
            "role": "owner"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_account_user(state);

    let result = call_tool_text(&tool, json!({"user_id": 5})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /users/5, got: {result}"
    );
}

#[tokio::test]
async fn test_list_acl_users_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/acl/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accountId": 12345,
            "users": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_acl_users(state);

    let result = call_tool_text(&tool, json!({})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /acl/users, got: {result}"
    );
}

#[tokio::test]
async fn test_get_acl_user_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/acl/users/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 3,
            "name": "db-user",
            "role": "readonly-role",
            "status": "active"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_acl_user(state);

    let result = call_tool_text(&tool, json!({"user_id": 3})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /acl/users/3, got: {result}"
    );
}

#[tokio::test]
async fn test_list_acl_roles_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/acl/roles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accountId": 12345,
            "roles": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_acl_roles(state);

    let result = call_tool_text(&tool, json!({})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /acl/roles, got: {result}"
    );
}

#[tokio::test]
async fn test_list_redis_rules_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/acl/redisRules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accountId": 12345,
            "redisRules": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_redis_rules(state);

    let result = call_tool_text(&tool, json!({})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /acl/redisRules, got: {result}"
    );
}

#[tokio::test]
async fn test_download_cost_report_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/cost-report/rpt-001"))
        .respond_with(ResponseTemplate::new(200).set_body_string("date,cost\n2024-01-01,100.00"))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::download_cost_report(state);

    let result = call_tool_text(&tool, json!({"cost_report_id": "rpt-001"})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /reports/rpt-001, got: {result}"
    );
}

#[tokio::test]
async fn test_list_payment_methods_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/payment-methods"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "paymentMethods": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_payment_methods(state);

    let result = call_tool_text(&tool, json!({})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /payment-methods, got: {result}"
    );
}

#[tokio::test]
async fn test_list_cloud_accounts_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/cloud-accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cloudAccounts": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::list_cloud_accounts(state);

    let result = call_tool_text(&tool, json!({})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /cloud-accounts, got: {result}"
    );
}

#[tokio::test]
async fn test_get_cloud_account_request_shape() {
    let server = MockCloudServer::start().await;

    Mock::given(method("GET"))
        .and(path("/cloud-accounts/10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 10,
            "name": "my-aws-account",
            "status": "active",
            "provider": "AWS"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::get_cloud_account(state);

    let result = call_tool_text(&tool, json!({"cloud_account_id": 10})).await;

    assert!(
        !result.contains("Failed"),
        "Expected successful GET /cloud-accounts/10, got: {result}"
    );
}

// ---------------------------------------------------------------------------
// Section 8D: Polling helper -- wait_for_cloud_task timeout path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_wait_for_cloud_task_timeout() {
    let server = MockCloudServer::start().await;

    // Task always returns in-progress -- exercises the timeout path.
    Mock::given(method("GET"))
        .and(path("/tasks/wait-task-001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "wait-task-001",
            "status": "processing-in-progress"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_cloud_client(client));
    let tool = cloud::wait_for_cloud_task(state);

    let result = call_tool_text(
        &tool,
        json!({
            "task_id": "wait-task-001",
            "timeout_seconds": 1,
            "interval_seconds": 1
        }),
    )
    .await;

    assert!(
        result.contains("timeout"),
        "Expected timeout response from wait_for_cloud_task, got: {result}"
    );
}
