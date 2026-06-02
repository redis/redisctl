#![cfg(feature = "enterprise")]
//! Integration tests for Redis Enterprise MCP tools using mock server

use std::sync::Arc;

use redis_enterprise::testing::{
    AlertFixture, ClusterFixture, DatabaseFixture, LicenseFixture, MockEnterpriseServer,
    NodeFixture, UserFixture,
};
use serde_json::json;
use tower_mcp::Tool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

// Import the tools and state from the MCP crate
use redisctl_mcp::state::AppState;
use redisctl_mcp::tools::enterprise;

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
