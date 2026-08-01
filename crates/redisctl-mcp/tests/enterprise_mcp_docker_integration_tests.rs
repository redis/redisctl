#![cfg(feature = "enterprise")]
//! Docker-backed Enterprise MCP live-integration tests.
//!
//! Requires the local Enterprise demo cluster:
//!   docker compose -f docker/docker-compose.enterprise-demo.yml up -d
//!   docker compose -f docker/docker-compose.enterprise-demo.yml logs -f redis-enterprise-init  # wait for ready
//!
//! Run with:
//!   cargo test --test enterprise_mcp_docker_integration_tests --features enterprise -- --ignored
//!
//! Env vars (set by docker/docker-compose.enterprise-demo.yml):
//!   REDIS_ENTERPRISE_URL=https://localhost:9443
//!   REDIS_ENTERPRISE_USER=admin@redis.local
//!   REDIS_ENTERPRISE_PASSWORD=Redis123!
//!   REDIS_ENTERPRISE_INSECURE=true

mod support;

use serde_json::json;
use serial_test::serial;
use tower_mcp::Tool;

use redisctl_mcp::tools::enterprise;
use support::{docker_available, enterprise_state, enterprise_state_full};

/// Call a tool and return its first text content as parsed JSON.
async fn call_json_result(
    tool: &Tool,
    input: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let result = tool.call(input).await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .unwrap_or_default()
        .to_string();

    if result.is_error {
        return Err(format!("tool returned an error: {text}"));
    }

    serde_json::from_str(&text).map_err(|error| format!("tool returned invalid JSON: {error}"))
}

/// Call a tool and return its first text content as a parsed JSON Value.
async fn call_json(tool: &Tool, input: serde_json::Value) -> serde_json::Value {
    call_json_result(tool, input)
        .await
        .unwrap_or_else(|error| panic!("{error}"))
}

fn unique_suffix() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{timestamp}", std::process::id())
}

// ============================================================================
// Cluster submodule (read paths)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_cluster() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::get_cluster(state);
    let result = call_json(&tool, json!({})).await;
    assert!(
        result["name"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    );
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_nodes() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_nodes(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["count"].as_u64().unwrap_or(0) >= 1);
    assert!(result["nodes"].is_array());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_cluster_stats() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::get_cluster_stats(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_license() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::get_license(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["expired"].is_boolean());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_cluster_policy() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::get_cluster_policy(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
}

// ============================================================================
// Databases submodule (read paths)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_enterprise_databases() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_databases(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["databases"].is_array());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_database_endpoints() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let list_tool = enterprise::list_databases(state.clone());
    let list = call_json(&list_tool, json!({})).await;
    let Some(uid) = list["databases"]
        .as_array()
        .and_then(|dbs| dbs.first())
        .and_then(|db| db["uid"].as_u64())
    else {
        eprintln!("Skipping: no databases available to query endpoints");
        return;
    };

    let tool = enterprise::get_database_endpoints(state);
    let result = call_json(&tool, json!({ "uid": uid })).await;
    assert!(result.is_object() || result.is_array());
}

// ============================================================================
// Observability submodule (read paths)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_logs() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_logs(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["logs"].is_array());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_alerts() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_alerts(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
    assert!(result.get("alerts").is_some());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_all_nodes_stats() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::get_all_nodes_stats(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_all_databases_stats() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::get_all_databases_stats(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_shards() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_shards(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_shard_and_list_shards_by_database() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let database_tool = enterprise::list_databases(state.clone());
    let databases = call_json(&database_tool, json!({})).await;
    let Some(database_uid) = databases["databases"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|database| database["uid"].as_u64())
    else {
        eprintln!("Skipping: no databases available to query shards");
        return;
    };

    let list_tool = enterprise::list_shards_by_database(state.clone());
    let shards = call_json(&list_tool, json!({ "bdb_uid": database_uid })).await;
    let shard_items = shards["shards"]
        .as_array()
        .expect("database shard list should be an array");
    let Some(shard_uid) = shard_items.first().and_then(|shard| {
        shard["uid"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| shard["uid"].as_u64().map(|uid| uid.to_string()))
    }) else {
        eprintln!("Skipping: database has no shards to query");
        return;
    };

    let get_tool = enterprise::get_shard(state);
    let shard = call_json(&get_tool, json!({ "uid": shard_uid })).await;
    assert!(shard.is_object());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_modules() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_modules(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["modules"].is_array());
}

// ============================================================================
// RBAC submodule (read paths)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_enterprise_users() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_users(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["users"].is_array());
    assert!(result["count"].as_u64().unwrap_or(0) >= 1);
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_enterprise_roles() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_roles(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["roles"].is_array());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_enterprise_acls() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_redis_acls(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result["acls"].is_array() || result.is_object());
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_get_enterprise_builtin_roles() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::get_enterprise_builtin_roles(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object() || result.is_array());
}

// ============================================================================
// Proxy submodule (read paths)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_proxies() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_proxies(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
}

// ============================================================================
// Services submodule (read paths)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
async fn test_live_list_services() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let tool = enterprise::list_services(state);
    let result = call_json(&tool, json!({})).await;
    assert!(result.is_object());
}

// ============================================================================
// Write paths (with cleanup)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
#[serial]
async fn test_live_create_update_and_delete_enterprise_database() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state_full();
    let name = format!("test-mcp-db-{}", unique_suffix());

    // Create a minimal 100MB database.
    let create_tool = enterprise::create_enterprise_database(state.clone());
    let created = call_json_result(
        &create_tool,
        json!({ "name": name, "memory_size": "104857600" }),
    )
    .await
    .expect("database creation should succeed");
    let uid = created["uid"]
        .as_u64()
        .expect("database creation should return a uid");

    // Exercise a bounded update on the resource created by this test.
    let update_tool = enterprise::update_enterprise_database(state.clone());
    let updated = call_json_result(
        &update_tool,
        json!({ "uid": uid, "updates": { "memory_size": 125829120u64 } }),
    )
    .await;

    // Always attempt cleanup.
    let delete_tool = enterprise::delete_enterprise_database(state);
    let deleted = call_json_result(&delete_tool, json!({ "uid": uid })).await;

    let updated = updated.expect("database update should succeed");
    let deleted = deleted.expect("database cleanup should succeed");
    assert_eq!(created["uid"], uid);
    assert_eq!(created["name"], name);
    assert_eq!(updated["uid"], uid);
    assert_eq!(deleted["uid"], uid);
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
#[serial]
async fn test_live_create_and_delete_enterprise_role() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state_full();
    let name = format!("test-mcp-role-{}", unique_suffix());

    let create_tool = enterprise::create_enterprise_role(state.clone());
    let created = call_json_result(
        &create_tool,
        json!({ "name": name, "management": "db_viewer" }),
    )
    .await
    .expect("role creation should succeed");
    let uid = created["uid"]
        .as_u64()
        .expect("role creation should return a uid");

    // Delete before asserting the response body so cleanup is still attempted
    // if a live API version changes a non-identity response field.
    let delete_tool = enterprise::delete_enterprise_role(state);
    let deleted = call_json_result(&delete_tool, json!({ "uid": uid })).await;

    let deleted = deleted.expect("role cleanup should succeed");
    assert_eq!(created["uid"], uid);
    assert_eq!(created["name"], name);
    assert_eq!(deleted["uid"], uid);
}

#[tokio::test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
#[serial]
async fn test_live_create_and_delete_enterprise_user() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state_full();
    let email = format!("test-mcp-{}@example.com", unique_suffix());

    let create_tool = enterprise::create_enterprise_user(state.clone());
    let created = call_json_result(
        &create_tool,
        json!({
            "email": email,
            "password": "McpTest123!",
            "role": "db_viewer",
            "name": "redisctl MCP integration test"
        }),
    )
    .await
    .expect("user creation should succeed");
    let uid = created["uid"]
        .as_u64()
        .expect("user creation should return a uid");

    // Delete before asserting the response body so cleanup is still attempted
    // if a live API version changes a non-identity response field.
    let delete_tool = enterprise::delete_enterprise_user(state);
    let deleted = call_json_result(&delete_tool, json!({ "uid": uid })).await;

    let deleted = deleted.expect("user cleanup should succeed");
    assert_eq!(created["uid"], uid);
    assert_eq!(created["email"], email);
    assert_eq!(deleted["uid"], uid);
}
