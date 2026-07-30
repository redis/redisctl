#![cfg(feature = "enterprise")]
//! Integration tests for Redis Enterprise MCP tools using mock server

mod support;

use std::sync::Arc;

use redis_enterprise::testing::{
    AlertFixture, ClusterFixture, DatabaseFixture, LicenseFixture, MockEnterpriseServer,
    NodeFixture, UserFixture,
};
use serde_json::json;
use tower_mcp::Tool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use redisctl_mcp::state::AppState;
use redisctl_mcp::tools::enterprise;
use support::{state_full, state_write};

/// Build an AppState with a full-tier policy so destructive tools are permitted.
fn full_policy_state(client: redis_enterprise::EnterpriseClient) -> Arc<AppState> {
    state_full(AppState::with_enterprise_client(client))
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
// Cluster Tests
// ============================================================================

#[tokio::test]
async fn test_get_cluster() {
    let server = MockEnterpriseServer::start().await;

    let cluster = ClusterFixture::new("production-cluster")
        .nodes(vec![1, 2, 3])
        .build();

    server.mock_cluster_info(cluster).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_cluster(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["name"], "production-cluster");
}

#[tokio::test]
async fn test_get_cluster_stats() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/cluster/stats/last"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "avg_latency": 0.5,
            "total_req": 10000,
            "egress_bytes": 1024000,
            "ingress_bytes": 512000
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_cluster_stats(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("avg_latency").is_some() || result.get("total_req").is_some());
}

// ============================================================================
// License Tests
// ============================================================================

#[tokio::test]
async fn test_get_license() {
    let server = MockEnterpriseServer::start().await;

    let license = LicenseFixture::new().shards_limit(100).build();

    server.mock_license(license).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_license(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["expired"], false);
    assert_eq!(result["shards_limit"], 100);
}

#[tokio::test]
async fn test_get_license_expired() {
    let server = MockEnterpriseServer::start().await;

    let license = LicenseFixture::expired().build();

    server.mock_license(license).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_license(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["expired"], true);
}

#[tokio::test]
async fn test_get_license_usage() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/license/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "shards_limit": 100,
            "shards_used": 45,
            "nodes_limit": 10,
            "nodes_used": 3,
            "ram_limit": 107374182400_i64,
            "ram_used": 34359738368_i64
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_license_usage(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["shards_limit"], 100);
    assert_eq!(result["shards_used"], 45);
}

// ============================================================================
// Logs Tests
// ============================================================================

#[tokio::test]
async fn test_list_logs() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "time": "2024-01-15T10:30:00Z",
                "type": "bdb_created"
            },
            {
                "time": "2024-01-15T10:25:00Z",
                "type": "node_joined"
            },
            {
                "time": "2024-01-15T10:20:00Z",
                "type": "cluster_config_updated"
            }
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_logs(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 3);
    let logs = result["logs"].as_array().unwrap();
    assert_eq!(logs.len(), 3);
    assert_eq!(logs[0]["type"], "bdb_created");
    assert_eq!(logs[1]["type"], "node_joined");
}

#[tokio::test]
async fn test_list_logs_with_params() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "time": "2024-01-15T10:30:00Z",
                "type": "bdb_created"
            }
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_logs(state);

    let result = call_tool_json(
        &tool,
        json!({
            "start_time": "2024-01-15T10:00:00Z",
            "end_time": "2024-01-15T11:00:00Z",
            "order": "desc",
            "limit": 10
        }),
    )
    .await;

    assert_eq!(result["count"], 1);
    let logs = result["logs"].as_array().unwrap();
    assert_eq!(logs.len(), 1);
}

// ============================================================================
// Database Tests
// ============================================================================

#[tokio::test]
async fn test_list_enterprise_databases() {
    let server = MockEnterpriseServer::start().await;

    let db1 = DatabaseFixture::new(1, "cache-primary")
        .memory_size(2 * 1024 * 1024 * 1024)
        .build();

    let db2 = DatabaseFixture::new(2, "sessions")
        .memory_size(1024 * 1024 * 1024)
        .build();

    server.mock_databases_list(vec![db1, db2]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_databases(state);

    let result = call_tool_json(&tool, json!({})).await;

    let databases = result["databases"].as_array().expect("expected array");
    assert_eq!(databases.len(), 2);
    assert!(databases.iter().any(|db| db["name"] == "cache-primary"));
    assert!(databases.iter().any(|db| db["name"] == "sessions"));
}

#[tokio::test]
async fn test_list_enterprise_databases_with_filter() {
    let server = MockEnterpriseServer::start().await;

    let db1 = DatabaseFixture::new(1, "cache-primary").build();
    let db2 = DatabaseFixture::new(2, "sessions").build();
    let db3 = DatabaseFixture::new(3, "cache-replica").build();

    server.mock_databases_list(vec![db1, db2, db3]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_databases(state);

    let result = call_tool_json(&tool, json!({"name_filter": "cache"})).await;

    let databases = result["databases"].as_array().expect("expected array");
    assert_eq!(databases.len(), 2);
    assert!(databases.iter().any(|db| db["name"] == "cache-primary"));
    assert!(databases.iter().any(|db| db["name"] == "cache-replica"));
    // sessions should be filtered out
    assert!(!databases.iter().any(|db| db["name"] == "sessions"));
}

#[tokio::test]
async fn test_get_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    let database = DatabaseFixture::new(1, "cache-primary")
        .memory_size(2 * 1024 * 1024 * 1024)
        .build();

    server.mock_database_get(1, database).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_database(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["name"], "cache-primary");
}

// ============================================================================
// Node Tests
// ============================================================================

#[tokio::test]
async fn test_list_nodes() {
    let server = MockEnterpriseServer::start().await;

    let node1 = NodeFixture::new(1, "10.0.0.1").cores(8).build();
    let node2 = NodeFixture::new(2, "10.0.0.2").cores(8).build();
    let node3 = NodeFixture::new(3, "10.0.0.3").cores(4).build();

    server.mock_nodes_list(vec![node1, node2, node3]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_nodes(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 3);
    let nodes = result["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0]["addr"], "10.0.0.1");
    assert_eq!(nodes[1]["addr"], "10.0.0.2");
    assert_eq!(nodes[2]["addr"], "10.0.0.3");
}

#[tokio::test]
async fn test_get_node() {
    let server = MockEnterpriseServer::start().await;

    let node = NodeFixture::new(1, "10.0.0.1").cores(8).build();

    server.mock_node_get(1, node).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_node(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["addr"], "10.0.0.1");
}

// ============================================================================
// User Tests
// ============================================================================

#[tokio::test]
async fn test_list_enterprise_users() {
    let server = MockEnterpriseServer::start().await;

    let user1 = UserFixture::new(1, "admin@example.com")
        .name("Admin User")
        .build();

    let user2 = UserFixture::new(2, "dev@example.com")
        .name("Developer")
        .build();

    server.mock_users_list(vec![user1, user2]).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_users(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let users = result["users"].as_array().unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0]["email"], "admin@example.com");
    assert_eq!(users[1]["email"], "dev@example.com");
}

#[tokio::test]
async fn test_get_enterprise_user() {
    let server = MockEnterpriseServer::start().await;

    let user = UserFixture::new(1, "admin@example.com")
        .name("Admin User")
        .build();

    server.mock_user_get(1, user).await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_user(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["email"], "admin@example.com");
}

// ============================================================================
// Alert and Stats Tests
// ============================================================================

#[tokio::test]
async fn test_list_alerts() {
    let server = MockEnterpriseServer::start().await;

    let alert1 = AlertFixture::new("alert-1", "high_memory_usage")
        .severity("WARNING")
        .description("Memory usage above 80%")
        .build();

    let alert2 = AlertFixture::new("alert-2", "node_cpu_critical")
        .severity("CRITICAL")
        .entity_type("node")
        .entity_uid("1")
        .build();

    // The `mock_alerts_list` helper was removed upstream; the list_alerts tool
    // now reads the cluster-wide alerts route directly.
    Mock::given(method("GET"))
        .and(path("/v1/alerts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([alert1, alert2])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_alerts(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let alerts = result["alerts"].as_array().unwrap();
    assert_eq!(alerts.len(), 2);
    assert_eq!(alerts[0]["name"], "high_memory_usage");
    assert_eq!(alerts[1]["name"], "node_cpu_critical");
}

#[tokio::test]
async fn test_get_database_stats() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/bdbs/1/stats/last"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "avg_latency": 0.3,
            "total_req": 5000,
            "used_memory": 1024000
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_database_stats(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert!(result.get("avg_latency").is_some() || result.get("total_req").is_some());
}

// ============================================================================
// Aggregate Stats Tests
// ============================================================================

#[tokio::test]
async fn test_get_all_nodes_stats() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/nodes/stats/last"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "stats": [
                {
                    "uid": 1,
                    "intervals": [{"time": "2024-01-15T10:30:00Z", "metrics": {"cpu_usage": 45.2}}]
                },
                {
                    "uid": 2,
                    "intervals": [{"time": "2024-01-15T10:30:00Z", "metrics": {"cpu_usage": 32.1}}]
                }
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_all_nodes_stats(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("stats").is_some());
    let stats = result["stats"].as_array().unwrap();
    assert_eq!(stats.len(), 2);
}

#[tokio::test]
async fn test_get_all_databases_stats() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/bdbs/stats/last"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "stats": [
                {
                    "uid": 1,
                    "intervals": [{"time": "2024-01-15T10:30:00Z", "metrics": {"avg_latency": 0.5}}]
                },
                {
                    "uid": 2,
                    "intervals": [{"time": "2024-01-15T10:30:00Z", "metrics": {"avg_latency": 0.3}}]
                }
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_all_databases_stats(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("stats").is_some());
    let stats = result["stats"].as_array().unwrap();
    assert_eq!(stats.len(), 2);
}

#[tokio::test]
async fn test_get_shard_stats() {
    let server = MockEnterpriseServer::start().await;

    // The documented Enterprise REST API path is `/v1/shards/stats/{uid}`,
    // not `/v1/shards/{uid}/stats`. The handler was corrected to match
    // the spec in redis-enterprise-rs#71; this test mounts the correct
    // path so the tool round-trips against a realistic mock.
    Mock::given(method("GET"))
        .and(path("/v1/shards/stats/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "intervals": [
                {"time": "2024-01-15T10:30:00Z", "metrics": {"used_memory": 512000}}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_shard_stats(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert!(result.get("intervals").is_some());
}

#[tokio::test]
async fn test_get_all_shards_stats() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/shards/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "stats": [
                {
                    "uid": 1,
                    "intervals": [{"time": "2024-01-15T10:30:00Z", "metrics": {"used_memory": 512000}}]
                },
                {
                    "uid": 2,
                    "intervals": [{"time": "2024-01-15T10:30:00Z", "metrics": {"used_memory": 256000}}]
                }
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_all_shards_stats(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("stats").is_some());
    let stats = result["stats"].as_array().unwrap();
    assert_eq!(stats.len(), 2);
}

// ============================================================================
// Historical Stats Tests
// ============================================================================

#[tokio::test]
async fn test_get_cluster_stats_historical() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/cluster/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "intervals": [
                {"time": "2024-01-15T10:00:00Z", "metrics": {"cpu_usage": 40.5}},
                {"time": "2024-01-15T10:05:00Z", "metrics": {"cpu_usage": 42.3}},
                {"time": "2024-01-15T10:10:00Z", "metrics": {"cpu_usage": 38.1}}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_cluster_stats(state);

    let result = call_tool_json(
        &tool,
        json!({
            "interval": "5min",
            "start_time": "2024-01-15T10:00:00Z",
            "end_time": "2024-01-15T10:15:00Z"
        }),
    )
    .await;

    assert!(result.get("intervals").is_some());
    let intervals = result["intervals"].as_array().unwrap();
    assert_eq!(intervals.len(), 3);
}

#[tokio::test]
async fn test_get_database_stats_historical() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/bdbs/1/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "intervals": [
                {"time": "2024-01-15T10:00:00Z", "metrics": {"avg_latency": 0.5}},
                {"time": "2024-01-15T10:05:00Z", "metrics": {"avg_latency": 0.6}}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_database_stats(state);

    let result = call_tool_json(
        &tool,
        json!({
            "uid": 1,
            "interval": "5min"
        }),
    )
    .await;

    assert!(result.get("intervals").is_some());
    let intervals = result["intervals"].as_array().unwrap();
    assert_eq!(intervals.len(), 2);
}

#[tokio::test]
async fn test_get_node_stats_historical() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/nodes/1/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "intervals": [
                {"time": "2024-01-15T10:00:00Z", "metrics": {"cpu_usage": 45.0}},
                {"time": "2024-01-15T11:00:00Z", "metrics": {"cpu_usage": 50.0}}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_node_stats(state);

    let result = call_tool_json(
        &tool,
        json!({
            "uid": 1,
            "interval": "1hour",
            "start_time": "2024-01-15T10:00:00Z"
        }),
    )
    .await;

    assert!(result.get("intervals").is_some());
    let intervals = result["intervals"].as_array().unwrap();
    assert_eq!(intervals.len(), 2);
}

// ============================================================================
// Debug Info Tests
// ============================================================================

#[tokio::test]
async fn test_list_debug_info_tasks() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/debuginfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "task_id": "debug-123",
                "status": "completed",
                "progress": 100.0,
                "download_url": "https://example.com/download/debug-123.tar.gz"
            },
            {
                "task_id": "debug-456",
                "status": "running",
                "progress": 45.0
            }
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_debug_info_tasks(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let tasks = result["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["task_id"], "debug-123");
    assert_eq!(tasks[0]["status"], "completed");
    assert_eq!(tasks[1]["task_id"], "debug-456");
    assert_eq!(tasks[1]["status"], "running");
}

#[tokio::test]
async fn test_get_debug_info_status() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/debuginfo/debug-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "task_id": "debug-123",
            "status": "completed",
            "progress": 100.0,
            "download_url": "https://example.com/download/debug-123.tar.gz"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_debug_info_status(state);

    let result = call_tool_json(&tool, json!({"task_id": "debug-123"})).await;

    assert_eq!(result["task_id"], "debug-123");
    assert_eq!(result["status"], "completed");
    assert_eq!(result["progress"], 100.0);
    assert!(result.get("download_url").is_some());
}

// ============================================================================
// Module Tests
// ============================================================================

#[tokio::test]
async fn test_list_modules() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/modules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "uid": "redisjson-2.6.0",
                "module_name": "ReJSON",
                "semantic_version": "2.6.0",
                "description": "Native JSON support for Redis",
                "capabilities": ["JSON"],
                "is_bundled": true
            },
            {
                "uid": "redisearch-2.8.0",
                "module_name": "ft",
                "semantic_version": "2.8.0",
                "description": "Full-text search and secondary indexing",
                "capabilities": ["SEARCH"],
                "is_bundled": true
            }
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_modules(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let modules = result["modules"].as_array().unwrap();
    assert_eq!(modules.len(), 2);
    assert_eq!(modules[0]["uid"], "redisjson-2.6.0");
    assert_eq!(modules[0]["module_name"], "ReJSON");
    assert_eq!(modules[1]["uid"], "redisearch-2.8.0");
}

#[tokio::test]
async fn test_get_module() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/modules/redisjson-2.6.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": "redisjson-2.6.0",
            "module_name": "ReJSON",
            "semantic_version": "2.6.0",
            "description": "Native JSON support for Redis",
            "author": "Redis Ltd.",
            "license": "Redis Source Available License",
            "capabilities": ["JSON"],
            "min_redis_version": "7.0.0",
            "is_bundled": true
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_module(state);

    let result = call_tool_json(&tool, json!({"uid": "redisjson-2.6.0"})).await;

    assert_eq!(result["uid"], "redisjson-2.6.0");
    assert_eq!(result["module_name"], "ReJSON");
    assert_eq!(result["semantic_version"], "2.6.0");
    assert_eq!(result["author"], "Redis Ltd.");
}

// ============================================================================
// Proxy Tests
// ============================================================================

#[tokio::test]
async fn test_list_enterprise_proxies() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/proxies"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": 1, "status": "active", "port": 12000},
            {"uid": 2, "status": "active", "port": 12001}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_proxies(state);

    let result = call_tool_json(&tool, json!({})).await;

    let proxies = result["proxies"]
        .as_array()
        .expect("expected proxies array");
    assert_eq!(proxies.len(), 2);
    assert_eq!(proxies[0]["uid"], 1);
}

#[tokio::test]
async fn test_get_enterprise_proxy() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/proxies/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 1,
            "status": "active",
            "port": 12000,
            "max_connections": 1000
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_proxy(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["status"], "active");
}

#[tokio::test]
async fn test_get_enterprise_proxy_stats() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/proxies/1/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 1,
            "intervals": [
                {"interval": "1sec", "timestamps": [1705314600], "values": [5000]}
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_proxy_stats(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert!(result.get("intervals").is_some());
}

#[tokio::test]
async fn test_update_enterprise_proxy() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/proxies/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 1,
            "status": "active",
            "max_connections": 2000
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_proxy(state);

    let result = call_tool_json(
        &tool,
        json!({
            "uid": 1,
            "updates": {"max_connections": 2000}
        }),
    )
    .await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["max_connections"], 2000);
}

// ============================================================================
// Services Tests
// ============================================================================

#[tokio::test]
async fn test_list_enterprise_services() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/local/services"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cm_server": {"service_id": "cm_server", "name": "cm_server", "enabled": true},
            "mdns_server": {"service_id": "mdns_server", "name": "mdns_server", "enabled": true}
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_services(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("cm_server").is_some());
    assert!(result.get("mdns_server").is_some());
}

#[tokio::test]
async fn test_get_enterprise_service() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/local/services"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cm_server": {"service_id": "cm_server", "name": "cm_server", "enabled": true}
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_service(state);

    let result = call_tool_json(&tool, json!({"service_id": "cm_server"})).await;

    assert_eq!(result["service_id"], "cm_server");
    assert_eq!(result["enabled"], true);
}

#[tokio::test]
async fn test_get_enterprise_service_status() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/local/services"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cm_server": {"service_id": "cm_server", "name": "cm_server", "enabled": true}
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_service_status(state);

    let result = call_tool_json(&tool, json!({"service_id": "cm_server"})).await;

    assert_eq!(result["service_id"], "cm_server");
    assert_eq!(result["enabled"], true);
}

#[tokio::test]
async fn test_update_enterprise_service() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/services/cm_server"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "service_id": "cm_server",
            "enabled": false
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_service(state);

    let result = call_tool_json(
        &tool,
        json!({
            "service_id": "cm_server",
            "config": {"enabled": false}
        }),
    )
    .await;

    assert_eq!(result["service_id"], "cm_server");
    assert_eq!(result["enabled"], false);
}

#[tokio::test]
async fn test_start_enterprise_service() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/services/cm_server/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ok"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::start_service(state);

    let result = call_tool_json(&tool, json!({"service_id": "cm_server"})).await;

    assert_eq!(result["status"], "ok");
}

#[tokio::test]
async fn test_stop_enterprise_service() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/services/cm_server/stop"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ok"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::stop_service(state);

    let result = call_tool_json(&tool, json!({"service_id": "cm_server"})).await;

    assert_eq!(result["status"], "ok");
}

#[tokio::test]
async fn test_restart_enterprise_service() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/services/cm_server/restart"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ok"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::restart_service(state);

    let result = call_tool_json(&tool, json!({"service_id": "cm_server"})).await;

    assert_eq!(result["status"], "ok");
}

// ============================================================================
// Database Read Tools (endpoints + alerts)
// ============================================================================

#[tokio::test]
async fn test_get_database_endpoints() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/bdbs/1/endpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "uid": "1:1",
                "addr": ["10.0.0.1"],
                "port": 12000,
                "dns_name": "redis-12000.cluster.local",
                "addr_type": "external"
            }
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_database_endpoints(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    let endpoints = result["endpoints"]
        .as_array()
        .expect("expected endpoints array");
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0]["port"], 12000);
    assert_eq!(endpoints[0]["dns_name"], "redis-12000.cluster.local");
}

#[tokio::test]
async fn test_list_database_alerts() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/bdbs/1/alerts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "uid": "alert-1",
                "name": "bdb_high_latency",
                "severity": "WARNING",
                "state": "true",
                "entity_type": "bdb",
                "entity_uid": "1"
            }
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_database_alerts(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    let alerts = result["alerts"].as_array().expect("expected alerts array");
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["name"], "bdb_high_latency");
    assert_eq!(alerts[0]["severity"], "WARNING");
}

// ============================================================================
// Database Write Tools
// ============================================================================

#[tokio::test]
async fn test_create_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/bdbs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 42,
            "name": "new-db",
            "status": "active"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = state_write(AppState::with_enterprise_client(client));
    let tool = enterprise::create_enterprise_database(state);

    let result = call_tool_json(
        &tool,
        json!({"name": "new-db", "memory_size": "1073741824"}),
    )
    .await;

    assert_eq!(result["uid"], 42);
    assert_eq!(result["name"], "new-db");
}

#[tokio::test]
async fn test_create_enterprise_database_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let client = server.client();
    // Default test policy is read-only.
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::create_enterprise_database(state);

    let result = tool
        .call(json!({"name": "new-db", "memory_size": 1073741824u64}))
        .await;

    assert!(
        result.is_error,
        "create should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_update_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/bdbs/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 1,
            "name": "cache-primary",
            "memory_size": 2147483648u64
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_enterprise_database(state);

    let result = call_tool_json(
        &tool,
        json!({"uid": 1, "updates": {"memory_size": 2147483648u64}}),
    )
    .await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["memory_size"], 2147483648u64);
}

#[tokio::test]
async fn test_backup_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/bdbs/1/actions/backup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "backup-action-1"
        })))
        .mount(server.inner())
        .await;

    // The Layer 2 workflow polls the action until it reports completed.
    Mock::given(method("GET"))
        .and(path("/v1/actions/backup-action-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "backup-action-1",
            "name": "backup",
            "status": "completed",
            "progress": "100"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::backup_enterprise_database(state);

    let result = call_tool_json(&tool, json!({"bdb_uid": 1, "timeout_seconds": 5})).await;

    assert_eq!(result["bdb_uid"], 1);
    assert_eq!(result["message"], "Backup completed successfully");
}

#[tokio::test]
async fn test_import_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/bdbs/1/actions/import"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "import-action-1",
            "status": "queued"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/actions/import-action-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "import-action-1",
            "name": "import",
            "status": "completed",
            "progress": "100"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::import_enterprise_database(state);

    let result = call_tool_json(
        &tool,
        json!({
            "bdb_uid": 1,
            "import_location": "s3://bucket/dump.rdb",
            "timeout_seconds": 5
        }),
    )
    .await;

    assert_eq!(result["bdb_uid"], 1);
    assert_eq!(result["import_location"], "s3://bucket/dump.rdb");
    assert_eq!(result["message"], "Import completed successfully");
}

#[tokio::test]
async fn test_export_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/bdbs/1/actions/export"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "export-action-1",
            "status": "queued"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::export_enterprise_database(state);

    let result = call_tool_json(
        &tool,
        json!({"uid": 1, "export_location": "s3://bucket/export.rdb"}),
    )
    .await;

    assert_eq!(result["action_uid"], "export-action-1");
}

#[tokio::test]
async fn test_restore_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/bdbs/1/actions/restore"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "restore-action-1"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::restore_enterprise_database(state);

    let result = call_tool_json(&tool, json!({"uid": 1, "backup_uid": "backup-99"})).await;

    assert_eq!(result["action_uid"], "restore-action-1");
}

#[tokio::test]
async fn test_upgrade_enterprise_database_redis() {
    let server = MockEnterpriseServer::start().await;

    // The client posts the upgrade request to the path-segment-style action.
    Mock::given(method("POST"))
        .and(path("/v1/bdbs/1/upgrade"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "upgrade-action-1"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::upgrade_enterprise_database_redis(state);

    let result = call_tool_json(&tool, json!({"uid": 1, "redis_version": "7.2"})).await;

    assert_eq!(result["action_uid"], "upgrade-action-1");
}

// ============================================================================
// Database Destructive Tools (delete + flush)
// ============================================================================

#[tokio::test]
async fn test_delete_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v1/bdbs/1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server.inner())
        .await;

    let state = full_policy_state(server.client());
    let tool = enterprise::delete_enterprise_database(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["message"], "Database deleted successfully");
}

#[tokio::test]
async fn test_delete_enterprise_database_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    // Default test policy is read-only; destructive tools require full tier.
    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::delete_enterprise_database(state);

    let result = tool.call(json!({"uid": 1})).await;

    assert!(
        result.is_error,
        "delete should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_flush_enterprise_database() {
    let server = MockEnterpriseServer::start().await;

    // The client issues PUT /v1/bdbs/{uid}/flush (path-segment action),
    // then the Layer 2 workflow polls the action to completion.
    Mock::given(method("PUT"))
        .and(path("/v1/bdbs/1/flush"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "flush-action-1"
        })))
        .mount(server.inner())
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/actions/flush-action-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "flush-action-1",
            "name": "flush",
            "status": "completed",
            "progress": "100"
        })))
        .mount(server.inner())
        .await;

    let state = full_policy_state(server.client());
    let tool = enterprise::flush_enterprise_database(state);

    let result = call_tool_json(&tool, json!({"bdb_uid": 1, "timeout_seconds": 5})).await;

    assert_eq!(result["bdb_uid"], 1);
    assert_eq!(result["message"], "Database flushed successfully");
}

#[tokio::test]
async fn test_flush_enterprise_database_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::flush_enterprise_database(state);

    let result = tool.call(json!({"bdb_uid": 1})).await;

    assert!(
        result.is_error,
        "flush should be blocked under read-only policy"
    );
}

// ============================================================================
// CRDB (Active-Active) Tools
// ============================================================================

#[tokio::test]
async fn test_list_enterprise_crdbs() {
    let server = MockEnterpriseServer::start().await;

    // The CRDB list endpoint wraps the array under a `crdbs` key.
    Mock::given(method("GET"))
        .and(path("/v1/crdbs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crdbs": [
                {
                    "guid": "crdb-guid-1",
                    "name": "global-cache",
                    "status": "active",
                    "memory_size": 1073741824u64,
                    "instances": []
                }
            ]
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_enterprise_crdbs(state);

    let result = call_tool_json(&tool, json!({})).await;

    let crdbs = result["crdbs"].as_array().expect("expected crdbs array");
    assert_eq!(crdbs.len(), 1);
    assert_eq!(crdbs[0]["name"], "global-cache");
    assert_eq!(crdbs[0]["guid"], "crdb-guid-1");
}

#[tokio::test]
async fn test_get_enterprise_crdb() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/crdbs/crdb-guid-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "guid": "crdb-guid-1",
            "name": "global-cache",
            "status": "active",
            "memory_size": 1073741824u64,
            "instances": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_enterprise_crdb(state);

    let result = call_tool_json(&tool, json!({"guid": "crdb-guid-1"})).await;

    assert_eq!(result["guid"], "crdb-guid-1");
    assert_eq!(result["name"], "global-cache");
}

#[tokio::test]
async fn test_get_enterprise_crdb_tasks() {
    let server = MockEnterpriseServer::start().await;

    // CRDB tasks live under the per-CRDB route, not /v1/crdbs/tasks.
    Mock::given(method("GET"))
        .and(path("/v1/crdbs/crdb-guid-1/tasks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "task-1", "status": "completed"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_enterprise_crdb_tasks(state);

    let result = call_tool_json(&tool, json!({"guid": "crdb-guid-1"})).await;

    let tasks = result.as_array().expect("expected tasks array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], "task-1");
}

#[tokio::test]
async fn test_create_enterprise_crdb() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/crdbs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "guid": "crdb-guid-new",
            "name": "new-aa-db",
            "status": "active"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::create_enterprise_crdb(state);

    let result = call_tool_json(
        &tool,
        json!({"request": {"name": "new-aa-db", "memory_size": 1073741824u64}}),
    )
    .await;

    assert_eq!(result["guid"], "crdb-guid-new");
    assert_eq!(result["name"], "new-aa-db");
}

#[tokio::test]
async fn test_update_enterprise_crdb() {
    let server = MockEnterpriseServer::start().await;

    // The client issues PATCH /v1/crdbs/{guid} for updates.
    Mock::given(method("PATCH"))
        .and(path("/v1/crdbs/crdb-guid-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "guid": "crdb-guid-1",
            "name": "global-cache",
            "status": "active",
            "memory_size": 2147483648u64,
            "instances": []
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_enterprise_crdb(state);

    let result = call_tool_json(
        &tool,
        json!({"guid": "crdb-guid-1", "updates": {"memory_size": 2147483648u64}}),
    )
    .await;

    assert_eq!(result["guid"], "crdb-guid-1");
    assert_eq!(result["memory_size"], 2147483648u64);
}

#[tokio::test]
async fn test_delete_enterprise_crdb() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v1/crdbs/crdb-guid-1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server.inner())
        .await;

    let state = full_policy_state(server.client());
    let tool = enterprise::delete_enterprise_crdb(state);

    let result = call_tool_json(&tool, json!({"guid": "crdb-guid-1"})).await;

    assert_eq!(result["guid"], "crdb-guid-1");
    assert_eq!(result["message"], "CRDB deleted successfully");
}

#[tokio::test]
async fn test_delete_enterprise_crdb_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::delete_enterprise_crdb(state);

    let result = tool.call(json!({"guid": "crdb-guid-1"})).await;

    assert!(
        result.is_error,
        "delete CRDB should be blocked under read-only policy"
    );
}

// ============================================================================
// Cluster Write Tools
// ============================================================================
//
// NOTE: several paths differ from the #989 issue text; these tests follow the
// actual `redis-enterprise` handler source:
//   - update_enterprise_cluster_certificates -> PUT /v1/cluster/update_cert
//   - validate_enterprise_license            -> POST /v1/license/validate
//   - get_enterprise_cluster_services        -> GET /v1/cluster/services_configuration
//   - maintenance mode tools                 -> PUT /v1/cluster (block_cluster_changes)
//   - update_enterprise_license              -> PUT /v1/license then GET /v1/license

#[tokio::test]
async fn test_update_enterprise_cluster() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/cluster"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "renamed-cluster",
            "email_alerts": true
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_cluster(state);

    let result = call_tool_json(
        &tool,
        json!({"updates": {"name": "renamed-cluster", "email_alerts": true}}),
    )
    .await;

    assert_eq!(result["name"], "renamed-cluster");
    assert_eq!(result["email_alerts"], true);
}

#[tokio::test]
async fn test_update_enterprise_cluster_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::update_cluster(state);

    let result = tool.call(json!({"updates": {"name": "x"}})).await;

    assert!(
        result.is_error,
        "update cluster should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_get_enterprise_cluster_policy() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/cluster/policy"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "default_shards_placement": "dense",
            "rack_aware": false,
            "default_provisioned_redis_version": "7.2"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_cluster_policy(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["default_shards_placement"], "dense");
    assert_eq!(result["rack_aware"], false);
}

#[tokio::test]
async fn test_update_enterprise_cluster_policy() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/cluster/policy"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rack_aware": true,
            "default_shards_placement": "sparse"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_cluster_policy(state);

    let result = call_tool_json(
        &tool,
        json!({"policy": {"rack_aware": true, "default_shards_placement": "sparse"}}),
    )
    .await;

    assert_eq!(result["rack_aware"], true);
    assert_eq!(result["default_shards_placement"], "sparse");
}

#[tokio::test]
async fn test_enable_enterprise_maintenance_mode() {
    let server = MockEnterpriseServer::start().await;

    // The tool toggles block_cluster_changes via PUT /v1/cluster.
    Mock::given(method("PUT"))
        .and(path("/v1/cluster"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "block_cluster_changes": true
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::enable_maintenance_mode(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["message"], "Maintenance mode enabled");
    assert_eq!(result["result"]["block_cluster_changes"], true);
}

#[tokio::test]
async fn test_disable_enterprise_maintenance_mode() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/cluster"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "block_cluster_changes": false
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::disable_maintenance_mode(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["message"], "Maintenance mode disabled");
    assert_eq!(result["result"]["block_cluster_changes"], false);
}

#[tokio::test]
async fn test_get_enterprise_cluster_certificates() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/cluster/certificates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "proxy_cert": "-----BEGIN CERTIFICATE-----proxy-----END CERTIFICATE-----",
            "syncer_cert": "-----BEGIN CERTIFICATE-----syncer-----END CERTIFICATE-----"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_cluster_certificates(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("proxy_cert").is_some());
    assert!(result.get("syncer_cert").is_some());
}

#[tokio::test]
async fn test_rotate_enterprise_cluster_certificates() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/cluster/certificates/rotate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::rotate_cluster_certificates(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["message"], "Certificate rotation initiated");
}

#[tokio::test]
async fn test_update_enterprise_cluster_certificates() {
    let server = MockEnterpriseServer::start().await;

    // The handler PUTs to /v1/cluster/update_cert (not /v1/cluster/certificates).
    Mock::given(method("PUT"))
        .and(path("/v1/cluster/update_cert"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_cluster_certificates(state);

    let result = call_tool_json(
        &tool,
        json!({
            "name": "proxy",
            "certificate": "-----BEGIN CERTIFICATE-----...-----END CERTIFICATE-----",
            "key": "-----BEGIN PRIVATE KEY-----...-----END PRIVATE KEY-----"
        }),
    )
    .await;

    assert_eq!(result["message"], "Certificate updated successfully");
    assert_eq!(result["name"], "proxy");
}

#[tokio::test]
async fn test_get_enterprise_cluster_services() {
    let server = MockEnterpriseServer::start().await;

    // get_enterprise_cluster_services reads the services_configuration route.
    Mock::given(method("GET"))
        .and(path("/v1/cluster/services_configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "cm_server": {"operating_mode": "enabled"},
            "mdns_server": {"operating_mode": "enabled"}
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_enterprise_cluster_services(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("cm_server").is_some());
    assert!(result.get("mdns_server").is_some());
}

// ============================================================================
// License Write Tools
// ============================================================================

#[tokio::test]
async fn test_update_enterprise_license() {
    let server = MockEnterpriseServer::start().await;

    // update() PUTs the new key (200/empty body tolerated) then GETs the
    // installed license to return it.
    Mock::given(method("PUT"))
        .and(path("/v1/license"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server.inner())
        .await;

    let license = LicenseFixture::new().shards_limit(250).build();
    server.mock_license(license).await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_license(state);

    let result = call_tool_json(&tool, json!({"license_key": "NEW-LICENSE-KEY"})).await;

    assert_eq!(result["expired"], false);
    assert_eq!(result["shards_limit"], 250);
}

#[tokio::test]
async fn test_update_enterprise_license_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::update_license(state);

    let result = tool.call(json!({"license_key": "NEW-LICENSE-KEY"})).await;

    assert!(
        result.is_error,
        "update license should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_validate_enterprise_license() {
    let server = MockEnterpriseServer::start().await;

    // validate() POSTs the candidate key to /v1/license/validate.
    Mock::given(method("POST"))
        .and(path("/v1/license/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "expired": false,
            "type": "commercial",
            "shards_limit": 100
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::validate_license(state);

    let result = call_tool_json(&tool, json!({"license_key": "SOME-LICENSE-KEY"})).await;

    assert_eq!(result["expired"], false);
    assert_eq!(result["type"], "commercial");
}

// ============================================================================
// Node Write Tools
// ============================================================================

#[tokio::test]
async fn test_enable_enterprise_node_maintenance() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/nodes/1/actions/maintenance_on"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "maint-on-1",
            "description": "Enabling maintenance mode"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::enable_node_maintenance(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["message"], "Node maintenance mode enabled");
    assert_eq!(result["node_uid"], 1);
    assert_eq!(result["action_uid"], "maint-on-1");
}

#[tokio::test]
async fn test_disable_enterprise_node_maintenance() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/nodes/1/actions/maintenance_off"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "maint-off-1",
            "description": "Disabling maintenance mode"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::disable_node_maintenance(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["message"], "Node maintenance mode disabled");
    assert_eq!(result["node_uid"], 1);
    assert_eq!(result["action_uid"], "maint-off-1");
}

#[tokio::test]
async fn test_rebalance_enterprise_node() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/nodes/1/actions/rebalance"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "rebalance-1",
            "description": "Rebalancing shards"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::rebalance_node(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["message"], "Node rebalance initiated");
    assert_eq!(result["node_uid"], 1);
    assert_eq!(result["action_uid"], "rebalance-1");
}

#[tokio::test]
async fn test_drain_enterprise_node() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/nodes/1/actions/drain"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "action_uid": "drain-1",
            "description": "Draining shards"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::drain_node(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["message"], "Node drain initiated");
    assert_eq!(result["node_uid"], 1);
    assert_eq!(result["action_uid"], "drain-1");
}

#[tokio::test]
async fn test_update_enterprise_node() {
    let server = MockEnterpriseServer::start().await;

    let node = NodeFixture::new(1, "10.0.0.1").cores(8).build();

    Mock::given(method("PUT"))
        .and(path("/v1/nodes/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(node))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_enterprise_node(state);

    let result = call_tool_json(
        &tool,
        json!({"uid": 1, "updates": {"external_addr": ["10.0.0.99"]}}),
    )
    .await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["addr"], "10.0.0.1");
}

#[tokio::test]
async fn test_update_enterprise_node_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::update_enterprise_node(state);

    let result = tool.call(json!({"uid": 1, "updates": {}})).await;

    assert!(
        result.is_error,
        "update node should be blocked under read-only policy"
    );
}

// ============================================================================
// Node Destructive Tools
// ============================================================================

#[tokio::test]
async fn test_remove_enterprise_node() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v1/nodes/1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server.inner())
        .await;

    let state = full_policy_state(server.client());
    let tool = enterprise::remove_enterprise_node(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["message"], "Node removed successfully");
}

#[tokio::test]
async fn test_remove_enterprise_node_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    // Default test policy is read-only; destructive tools require full tier.
    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::remove_enterprise_node(state);

    let result = tool.call(json!({"uid": 1})).await;

    assert!(
        result.is_error,
        "remove node should be blocked under read-only policy"
    );
}

// ============================================================================
// RBAC: User Write/Destructive Tools
// ============================================================================
//
// NOTE: several paths differ from the #992 issue text; these tests follow the
// actual `redis-enterprise` handler source:
//   - get_enterprise_user_permissions -> GET  /v1/users/permissions
//   - get_enterprise_builtin_roles    -> GET  /v1/roles/builtin
//   - LDAP config tools               -> GET/PUT /v1/cluster/ldap
//   - validate_enterprise_acl is a read_only tool (POST /v1/redis_acls/validate)

#[tokio::test]
async fn test_create_enterprise_user() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 5,
            "email": "new@example.com",
            "name": "New User",
            "role": "db_member"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::create_enterprise_user(state);

    let result = call_tool_json(
        &tool,
        json!({
            "email": "new@example.com",
            "password": "s3cr3t!",
            "role": "db_member",
            "name": "New User"
        }),
    )
    .await;

    assert_eq!(result["uid"], 5);
    assert_eq!(result["email"], "new@example.com");
}

#[tokio::test]
async fn test_create_enterprise_user_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::create_enterprise_user(state);

    let result = tool
        .call(json!({
            "email": "new@example.com",
            "password": "s3cr3t!",
            "role": "db_member"
        }))
        .await;

    assert!(
        result.is_error,
        "create user should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_update_enterprise_user() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/users/5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 5,
            "email": "new@example.com",
            "name": "Renamed User",
            "role": "db_viewer"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_enterprise_user(state);

    let result = call_tool_json(
        &tool,
        json!({"uid": 5, "name": "Renamed User", "role": "db_viewer"}),
    )
    .await;

    assert_eq!(result["uid"], 5);
    assert_eq!(result["name"], "Renamed User");
    assert_eq!(result["role"], "db_viewer");
}

#[tokio::test]
async fn test_delete_enterprise_user() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v1/users/5"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server.inner())
        .await;

    let state = full_policy_state(server.client());
    let tool = enterprise::delete_enterprise_user(state);

    let result = call_tool_json(&tool, json!({"uid": 5})).await;

    assert_eq!(result["uid"], 5);
    assert_eq!(result["message"], "User deleted successfully");
}

#[tokio::test]
async fn test_delete_enterprise_user_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    // Default test policy is read-only; destructive tools require full tier.
    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::delete_enterprise_user(state);

    let result = tool.call(json!({"uid": 5})).await;

    assert!(
        result.is_error,
        "delete user should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_get_enterprise_user_permissions() {
    let server = MockEnterpriseServer::start().await;

    // The handler reads GET /v1/users/permissions (not /v1/users/{uid}/permissions).
    Mock::given(method("GET"))
        .and(path("/v1/users/permissions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "view_cluster_info", "description": "View cluster information"},
            {"name": "update_bdb", "description": "Update database configuration"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_enterprise_user_permissions(state);

    let result = call_tool_json(&tool, json!({})).await;

    let permissions = result.as_array().expect("expected permissions array");
    assert_eq!(permissions.len(), 2);
    assert_eq!(permissions[0]["name"], "view_cluster_info");
}

// ============================================================================
// RBAC: Role Tools
// ============================================================================

#[tokio::test]
async fn test_list_enterprise_roles() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/roles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": 1, "name": "Admin", "management": "admin"},
            {"uid": 2, "name": "Developer", "management": "db_member"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_roles(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let roles = result["roles"].as_array().expect("expected roles array");
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0]["name"], "Admin");
    assert_eq!(roles[1]["name"], "Developer");
}

#[tokio::test]
async fn test_get_enterprise_role() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/roles/2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 2,
            "name": "Developer",
            "management": "db_member",
            "data_access": "redis_acl"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_role(state);

    let result = call_tool_json(&tool, json!({"uid": 2})).await;

    assert_eq!(result["uid"], 2);
    assert_eq!(result["name"], "Developer");
    assert_eq!(result["management"], "db_member");
}

#[tokio::test]
async fn test_create_enterprise_role() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/roles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 3,
            "name": "ReadOnly",
            "management": "db_viewer"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::create_enterprise_role(state);

    let result = call_tool_json(
        &tool,
        json!({"name": "ReadOnly", "management": "db_viewer"}),
    )
    .await;

    assert_eq!(result["uid"], 3);
    assert_eq!(result["name"], "ReadOnly");
}

#[tokio::test]
async fn test_create_enterprise_role_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::create_enterprise_role(state);

    let result = tool.call(json!({"name": "ReadOnly"})).await;

    assert!(
        result.is_error,
        "create role should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_update_enterprise_role() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/roles/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 3,
            "name": "ReadOnlyRenamed",
            "management": "cluster_viewer"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_enterprise_role(state);

    let result = call_tool_json(
        &tool,
        json!({"uid": 3, "name": "ReadOnlyRenamed", "management": "cluster_viewer"}),
    )
    .await;

    assert_eq!(result["uid"], 3);
    assert_eq!(result["name"], "ReadOnlyRenamed");
}

#[tokio::test]
async fn test_delete_enterprise_role() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v1/roles/3"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server.inner())
        .await;

    let state = full_policy_state(server.client());
    let tool = enterprise::delete_enterprise_role(state);

    let result = call_tool_json(&tool, json!({"uid": 3})).await;

    assert_eq!(result["uid"], 3);
    assert_eq!(result["message"], "Role deleted successfully");
}

#[tokio::test]
async fn test_delete_enterprise_role_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    // Default test policy is read-only; destructive tools require full tier.
    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::delete_enterprise_role(state);

    let result = tool.call(json!({"uid": 3})).await;

    assert!(
        result.is_error,
        "delete role should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_get_enterprise_builtin_roles() {
    let server = MockEnterpriseServer::start().await;

    // The handler reads GET /v1/roles/builtin (not /v1/roles/built_in_roles).
    Mock::given(method("GET"))
        .and(path("/v1/roles/builtin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": 1, "name": "admin", "management": "admin"},
            {"uid": 2, "name": "db_viewer", "management": "db_viewer"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_enterprise_builtin_roles(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let roles = result["roles"].as_array().expect("expected roles array");
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0]["name"], "admin");
}

// ============================================================================
// RBAC: Redis ACL Tools
// ============================================================================

#[tokio::test]
async fn test_list_enterprise_acls() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/redis_acls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": 1, "name": "full-access", "acl": "+@all ~*"},
            {"uid": 2, "name": "read-only", "acl": "+@read ~*"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_redis_acls(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let acls = result["acls"].as_array().expect("expected acls array");
    assert_eq!(acls.len(), 2);
    assert_eq!(acls[0]["name"], "full-access");
    assert_eq!(acls[1]["name"], "read-only");
}

#[tokio::test]
async fn test_get_enterprise_acl() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/redis_acls/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 1,
            "name": "full-access",
            "acl": "+@all ~*"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_redis_acl(state);

    let result = call_tool_json(&tool, json!({"uid": 1})).await;

    assert_eq!(result["uid"], 1);
    assert_eq!(result["name"], "full-access");
    assert_eq!(result["acl"], "+@all ~*");
}

#[tokio::test]
async fn test_create_enterprise_acl() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/redis_acls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 3,
            "name": "cache-only",
            "acl": "+get +set ~cache:*"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::create_enterprise_acl(state);

    let result = call_tool_json(
        &tool,
        json!({"name": "cache-only", "acl": "+get +set ~cache:*"}),
    )
    .await;

    assert_eq!(result["uid"], 3);
    assert_eq!(result["name"], "cache-only");
}

#[tokio::test]
async fn test_create_enterprise_acl_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::create_enterprise_acl(state);

    let result = tool
        .call(json!({"name": "cache-only", "acl": "+get +set ~cache:*"}))
        .await;

    assert!(
        result.is_error,
        "create ACL should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_update_enterprise_acl() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/redis_acls/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 3,
            "name": "cache-only",
            "acl": "+@read ~cache:*"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_enterprise_acl(state);

    let result = call_tool_json(
        &tool,
        json!({"uid": 3, "name": "cache-only", "acl": "+@read ~cache:*"}),
    )
    .await;

    assert_eq!(result["uid"], 3);
    assert_eq!(result["acl"], "+@read ~cache:*");
}

#[tokio::test]
async fn test_delete_enterprise_acl() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/v1/redis_acls/3"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server.inner())
        .await;

    let state = full_policy_state(server.client());
    let tool = enterprise::delete_enterprise_acl(state);

    let result = call_tool_json(&tool, json!({"uid": 3})).await;

    assert_eq!(result["uid"], 3);
    assert_eq!(result["message"], "ACL deleted successfully");
}

#[tokio::test]
async fn test_delete_enterprise_acl_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    // Default test policy is read-only; destructive tools require full tier.
    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::delete_enterprise_acl(state);

    let result = tool.call(json!({"uid": 3})).await;

    assert!(
        result.is_error,
        "delete ACL should be blocked under read-only policy"
    );
}

#[tokio::test]
async fn test_validate_enterprise_acl() {
    let server = MockEnterpriseServer::start().await;

    // validate() POSTs the candidate rule to /v1/redis_acls/validate.
    Mock::given(method("POST"))
        .and(path("/v1/redis_acls/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "valid": true,
            "message": "ACL is valid"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::validate_enterprise_acl(state);

    let result = call_tool_json(&tool, json!({"name": "candidate", "acl": "+@all ~*"})).await;

    assert_eq!(result["valid"], true);
    assert_eq!(result["message"], "ACL is valid");
}

// ============================================================================
// RBAC: LDAP Tools
// ============================================================================

#[tokio::test]
async fn test_get_enterprise_ldap_config() {
    let server = MockEnterpriseServer::start().await;

    // The handler reads GET /v1/cluster/ldap (not /v1/ldap).
    Mock::given(method("GET"))
        .and(path("/v1/cluster/ldap"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "enabled": true,
            "bind_dn": "cn=admin,dc=example,dc=com",
            "authentication_query_suffix": "ou=users,dc=example,dc=com"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_enterprise_ldap_config(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["enabled"], true);
    assert_eq!(result["bind_dn"], "cn=admin,dc=example,dc=com");
}

#[tokio::test]
async fn test_update_enterprise_ldap_config() {
    let server = MockEnterpriseServer::start().await;

    // The handler writes PUT /v1/cluster/ldap.
    Mock::given(method("PUT"))
        .and(path("/v1/cluster/ldap"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "enabled": true,
            "bind_dn": "cn=svc,dc=example,dc=com",
            "cache_refresh_interval": 300
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::update_enterprise_ldap_config(state);

    let result = call_tool_json(
        &tool,
        json!({
            "config": {
                "enabled": true,
                "bind_dn": "cn=svc,dc=example,dc=com",
                "cache_refresh_interval": 300
            }
        }),
    )
    .await;

    assert_eq!(result["enabled"], true);
    assert_eq!(result["bind_dn"], "cn=svc,dc=example,dc=com");
}

#[tokio::test]
async fn test_update_enterprise_ldap_config_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::update_enterprise_ldap_config(state);

    let result = tool.call(json!({"config": {"enabled": false}})).await;

    assert!(
        result.is_error,
        "update LDAP config should be blocked under read-only policy"
    );
}

// ============================================================================
// Shard Tests
// ============================================================================

#[tokio::test]
async fn test_list_shards() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/shards"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": "1", "bdb_uid": 1, "node_uid": "1", "role": "master", "status": "active"},
            {"uid": "2", "bdb_uid": 1, "node_uid": "2", "role": "slave", "status": "active"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_shards(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert_eq!(result["count"], 2);
    let shards = result["shards"].as_array().unwrap();
    assert_eq!(shards.len(), 2);
    assert_eq!(shards[0]["uid"], "1");
}

#[tokio::test]
async fn test_get_shard() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/shards/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": "1",
            "bdb_uid": 1,
            "node_uid": "1",
            "role": "master",
            "status": "active"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::get_shard(state);

    let result = call_tool_json(&tool, json!({"uid": "1"})).await;

    assert_eq!(result["uid"], "1");
    assert_eq!(result["role"], "master");
}

#[tokio::test]
async fn test_list_shards_by_database() {
    let server = MockEnterpriseServer::start().await;

    // ShardHandler::list_by_database() uses a path segment: GET /v1/bdbs/{bdb_uid}/shards
    Mock::given(method("GET"))
        .and(path("/v1/bdbs/1/shards"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": "1", "bdb_uid": 1, "node_uid": "1", "role": "master", "status": "active"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_shards_by_database(state);

    let result = call_tool_json(&tool, json!({"bdb_uid": 1})).await;

    assert_eq!(result["count"], 1);
    let shards = result["shards"].as_array().unwrap();
    assert_eq!(shards[0]["bdb_uid"], 1);
}

#[tokio::test]
async fn test_list_shards_by_node() {
    let server = MockEnterpriseServer::start().await;

    // ShardHandler::list_by_node() uses a path segment: GET /v1/nodes/{node_uid}/shards
    Mock::given(method("GET"))
        .and(path("/v1/nodes/2/shards"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": "2", "bdb_uid": 1, "node_uid": "2", "role": "slave", "status": "active"}
        ])))
        .mount(server.inner())
        .await;

    let client = server.client();
    let state = Arc::new(AppState::with_enterprise_client(client));
    let tool = enterprise::list_shards_by_node(state);

    let result = call_tool_json(&tool, json!({"node_uid": 2})).await;

    assert_eq!(result["count"], 1);
    let shards = result["shards"].as_array().unwrap();
    assert_eq!(shards[0]["node_uid"], "2");
}

// ============================================================================
// Alert Write Tests
// ============================================================================

#[tokio::test]
async fn test_acknowledge_enterprise_alert() {
    let server = MockEnterpriseServer::start().await;

    // acknowledge_enterprise_alert uses client.delete(), so the HTTP method is DELETE.
    Mock::given(method("DELETE"))
        .and(path("/v1/alerts/high_memory_usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::acknowledge_enterprise_alert(state);

    let result = call_tool_json(&tool, json!({"alert_uid": "high_memory_usage"})).await;

    assert!(result.get("message").is_some());
    assert_eq!(result["alert_uid"], "high_memory_usage");
}

#[tokio::test]
async fn test_acknowledge_enterprise_alert_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::acknowledge_enterprise_alert(state);

    let result = tool.call(json!({"alert_uid": "some_alert"})).await;

    assert!(
        result.is_error,
        "acknowledge alert should be blocked under read-only policy"
    );
}

// ============================================================================
// Debug Info Write Tests
// ============================================================================

#[tokio::test]
async fn test_create_debug_info() {
    let server = MockEnterpriseServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/debuginfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "task_id": "debug-789",
            "status": "queued"
        })))
        .mount(server.inner())
        .await;

    let client = server.client();
    let mut state = AppState::with_enterprise_client(client);
    state.policy = AppState::test_write_policy();
    let state = Arc::new(state);
    let tool = enterprise::create_debug_info(state);

    let result = call_tool_json(&tool, json!({})).await;

    assert!(result.get("task_id").is_some());
    assert_eq!(result["task_id"], "debug-789");
    assert_eq!(result["status"], "queued");
}

#[tokio::test]
async fn test_create_debug_info_blocked_in_read_only() {
    let server = MockEnterpriseServer::start().await;

    let state = Arc::new(AppState::with_enterprise_client(server.client()));
    let tool = enterprise::create_debug_info(state);

    let result = tool.call(json!({})).await;

    assert!(
        result.is_error,
        "create debug info should be blocked under read-only policy"
    );
}
