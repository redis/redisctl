#![cfg(feature = "enterprise")]
//! Docker-backed Enterprise MCP live-integration tests.
//!
//! Requires the local Enterprise demo cluster:
//!   docker compose up -d
//!   docker compose logs -f redis-enterprise-init  # wait for ready
//!
//! Run with:
//!   cargo test --test enterprise_mcp_docker_integration_tests --features enterprise -- --ignored
//!
//! Env vars (set by docker-compose.yml):
//!   REDIS_ENTERPRISE_URL=https://localhost:9443
//!   REDIS_ENTERPRISE_USER=admin@redis.local
//!   REDIS_ENTERPRISE_PASSWORD=Redis123!
//!   REDIS_ENTERPRISE_INSECURE=true

mod support;

use serde_json::json;
use tower_mcp::Tool;

use redisctl_mcp::tools::enterprise;
use support::{docker_available, enterprise_state};

/// Call a tool and return its first text content as a parsed JSON Value.
async fn call_json(tool: &Tool, input: serde_json::Value) -> serde_json::Value {
    let result = tool.call(input).await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .unwrap_or_default()
        .to_string();
    serde_json::from_str(&text).expect("tool returned valid JSON")
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
async fn test_live_create_and_delete_enterprise_database() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }
    let state = enterprise_state();

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("test-mcp-{}", ts);

    // Create a minimal 100MB database.
    let create_tool = enterprise::create_enterprise_database(state.clone());
    let created = call_json(
        &create_tool,
        json!({ "name": name, "memory_size": 104857600u64 }),
    )
    .await;

    let Some(uid) = created["uid"].as_u64() else {
        eprintln!("Skipping cleanup: database creation did not return a uid: {created}");
        return;
    };

    // Always attempt cleanup.
    let delete_tool = enterprise::delete_enterprise_database(state);
    let deleted = call_json(&delete_tool, json!({ "uid": uid })).await;
    assert!(deleted.is_object());
}
