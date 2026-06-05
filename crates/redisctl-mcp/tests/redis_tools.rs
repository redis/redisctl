#![cfg(feature = "database")]
//! Integration tests for Redis database MCP tools using docker-wrapper.
//!
//! Each test uses a unique key prefix to avoid interference when running concurrently.
//! The shared Redis container is created once via `OnceCell`.
//!
//! Run with:
//! ```bash
//! cargo test -p redisctl-mcp --test redis_tools --all-features -- --ignored --nocapture
//! ```
//!
//! For faster iteration with container reuse:
//! ```bash
//! REUSE_CONTAINERS=1 cargo test -p redisctl-mcp --test redis_tools --all-features -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use docker_wrapper::template::redis::RedisTemplate;
use docker_wrapper::testing::ContainerGuardBuilder;
use serde_json::json;
use tokio::sync::OnceCell;
use tower_mcp::Tool;

use redisctl_mcp::policy::{Policy, PolicyConfig, SafetyTier};
use redisctl_mcp::state::AppState;
use redisctl_mcp::tools::redis;

// ============================================================================
// Test infrastructure
// ============================================================================

static REDIS_GUARD: OnceCell<RedisTestContext> = OnceCell::const_new();

struct RedisTestContext {
    _guard: docker_wrapper::testing::ContainerGuard<RedisTemplate>,
    port: u16,
}

unsafe impl Send for RedisTestContext {}
unsafe impl Sync for RedisTestContext {}

async fn get_redis() -> anyhow::Result<&'static RedisTestContext> {
    REDIS_GUARD
        .get_or_try_init(|| async {
            let reuse = std::env::var("REUSE_CONTAINERS").is_ok();
            let template = RedisTemplate::new("redisctl-mcp-dw-test").port(16380);

            let guard = ContainerGuardBuilder::new(template)
                .stop_on_drop(!reuse)
                .remove_on_drop(!reuse)
                .reuse_if_running(reuse)
                .keep_on_panic(true)
                .capture_logs(true)
                .wait_for_ready(true)
                .stop_timeout(Duration::from_secs(10))
                .start()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to start container: {}", e))?;

            let port = guard
                .host_port(6379)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get port: {}", e))?;

            Ok(RedisTestContext {
                _guard: guard,
                port,
            })
        })
        .await
}

fn redis_url(port: u16) -> String {
    format!("redis://localhost:{}", port)
}

fn make_state(port: u16) -> Arc<AppState> {
    let policy = Arc::new(Policy::new(
        PolicyConfig::default(),
        HashMap::new(),
        "test".to_string(),
    ));
    Arc::new(
        AppState::new(
            redisctl_mcp::state::CredentialSource::Profiles(vec![]),
            policy,
            Some(redis_url(port)),
            false,
            None,
        )
        .unwrap(),
    )
}

fn make_rw_state(port: u16) -> Arc<AppState> {
    let policy = Arc::new(Policy::new(
        PolicyConfig {
            tier: SafetyTier::ReadWrite,
            ..Default::default()
        },
        HashMap::new(),
        "test".to_string(),
    ));
    Arc::new(
        AppState::new(
            redisctl_mcp::state::CredentialSource::Profiles(vec![]),
            policy,
            Some(redis_url(port)),
            false,
            None,
        )
        .unwrap(),
    )
}

fn make_full_state(port: u16) -> Arc<AppState> {
    let policy = Arc::new(Policy::new(
        PolicyConfig {
            tier: SafetyTier::Full,
            ..Default::default()
        },
        HashMap::new(),
        "test".to_string(),
    ));
    Arc::new(
        AppState::new(
            redisctl_mcp::state::CredentialSource::Profiles(vec![]),
            policy,
            Some(redis_url(port)),
            false,
            None,
        )
        .unwrap(),
    )
}

async fn call_tool_text(tool: &Tool, input: serde_json::Value) -> String {
    let result = tool.call(input).await;
    result
        .content
        .first()
        .and_then(|c: &tower_mcp::Content| c.as_text())
        .unwrap_or_default()
        .to_string()
}

async fn get_conn(port: u16) -> ::redis::aio::MultiplexedConnection {
    let client = ::redis::Client::open(redis_url(port)).unwrap();
    client.get_multiplexed_async_connection().await.unwrap()
}

/// Clean up keys matching a prefix (for test isolation)
async fn cleanup(conn: &mut ::redis::aio::MultiplexedConnection, prefix: &str) {
    let keys: Vec<String> = ::redis::cmd("KEYS")
        .arg(format!("{}*", prefix))
        .query_async(conn)
        .await
        .unwrap_or_default();
    if !keys.is_empty() {
        let mut cmd = ::redis::cmd("DEL");
        for k in &keys {
            cmd.arg(k);
        }
        let _: () = cmd.query_async(conn).await.unwrap_or_default();
    }
}

// ============================================================================
// Server tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_server_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);

    // redis_ping
    let text = call_tool_text(&redis::ping(state.clone()), json!({})).await;
    assert!(text.contains("PONG"), "ping: {}", text);

    // redis_info
    let text = call_tool_text(&redis::info(state.clone()), json!({})).await;
    assert!(text.contains("redis_version"), "info: {}", text);

    // redis_info with section
    let text = call_tool_text(&redis::info(state.clone()), json!({"section": "server"})).await;
    assert!(text.contains("redis_version"), "info server: {}", text);

    // redis_dbsize (just verify it returns a number, other tests may add keys)
    let text = call_tool_text(&redis::dbsize(state.clone()), json!({})).await;
    assert!(text.contains("keys"), "dbsize: {}", text);

    // redis_client_list
    let text = call_tool_text(&redis::client_list(state.clone()), json!({})).await;
    assert!(text.contains("connected client"), "client_list: {}", text);

    // redis_slowlog
    let text = call_tool_text(&redis::slowlog(state.clone()), json!({})).await;
    assert!(
        text.contains("Slow log") || text.contains("No slow queries"),
        "slowlog: {}",
        text
    );

    // redis_config_get
    let text = call_tool_text(
        &redis::config_get(state.clone()),
        json!({"parameter": "maxmemory"}),
    )
    .await;
    assert!(text.contains("maxmemory"), "config_get: {}", text);

    // redis_memory_stats
    let text = call_tool_text(&redis::memory_stats(state.clone()), json!({})).await;
    assert!(!text.is_empty(), "memory_stats returned empty");

    // redis_latency_history
    let text = call_tool_text(
        &redis::latency_history(state.clone()),
        json!({"event": "command"}),
    )
    .await;
    assert!(
        text.contains("Latency history") || text.contains("No latency history"),
        "latency_history: {}",
        text
    );

    // redis_acl_list
    let text = call_tool_text(&redis::acl_list(state.clone()), json!({})).await;
    assert!(
        text.contains("ACL rules") || text.contains("default"),
        "acl_list: {}",
        text
    );

    // redis_acl_whoami
    let text = call_tool_text(&redis::acl_whoami(state.clone()), json!({})).await;
    assert!(text.contains("default"), "acl_whoami: {}", text);

    // redis_module_list
    let text = call_tool_text(&redis::module_list(state.clone()), json!({})).await;
    assert!(
        text.contains("modules") || text.contains("No modules"),
        "module_list: {}",
        text
    );

    // redis_cluster_info (standalone -- may return cluster_enabled:0 or error)
    let text = call_tool_text(&redis::cluster_info(state.clone()), json!({})).await;
    assert!(
        text.contains("cluster_enabled") || text.contains("error") || text.is_empty(),
        "cluster_info: {}",
        text
    );
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_server_config_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_rw_state(ctx.port);

    let text = call_tool_text(
        &redis::config_set(state.clone()),
        json!({"parameter": "hz", "value": "15"}),
    )
    .await;
    assert!(text.contains("OK"), "config_set: {}", text);

    let text = call_tool_text(
        &redis::config_get(state.clone()),
        json!({"parameter": "hz"}),
    )
    .await;
    assert!(text.contains("15"), "config_get verify: {}", text);

    // Reset
    let _ = call_tool_text(
        &redis::config_set(state.clone()),
        json!({"parameter": "hz", "value": "10"}),
    )
    .await;
}

// ============================================================================
// Key read tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_key_read_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "kr_"; // key prefix

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("SET")
        .arg(format!("{p}key1"))
        .arg("value1")
        .query_async(&mut conn)
        .await
        .unwrap();
    let _: () = ::redis::cmd("SET")
        .arg(format!("{p}key2"))
        .arg("value2")
        .query_async(&mut conn)
        .await
        .unwrap();
    let _: () = ::redis::cmd("EXPIRE")
        .arg(format!("{p}key2"))
        .arg(3600)
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_keys
    let text = call_tool_text(
        &redis::keys(state.clone()),
        json!({"pattern": format!("{p}*")}),
    )
    .await;
    assert!(text.contains(&format!("{p}key1")), "keys: {}", text);

    // redis_scan
    let text = call_tool_text(
        &redis::scan(state.clone()),
        json!({"pattern": format!("{p}*")}),
    )
    .await;
    assert!(text.contains(&format!("{p}key1")), "scan: {}", text);

    // redis_get
    let text = call_tool_text(
        &redis::get(state.clone()),
        json!({"key": format!("{p}key1")}),
    )
    .await;
    assert!(text.contains("value1"), "get: {}", text);

    // redis_get non-existent
    let text = call_tool_text(
        &redis::get(state.clone()),
        json!({"key": format!("{p}nonexistent")}),
    )
    .await;
    assert!(text.contains("nil"), "get nil: {}", text);

    // redis_type
    let text = call_tool_text(
        &redis::key_type(state.clone()),
        json!({"key": format!("{p}key1")}),
    )
    .await;
    assert!(text.contains("string"), "type: {}", text);

    // redis_ttl
    let text = call_tool_text(
        &redis::ttl(state.clone()),
        json!({"key": format!("{p}key1")}),
    )
    .await;
    assert!(text.contains("no expiry"), "ttl no expiry: {}", text);

    let text = call_tool_text(
        &redis::ttl(state.clone()),
        json!({"key": format!("{p}key2")}),
    )
    .await;
    assert!(
        text.contains("seconds remaining"),
        "ttl with expiry: {}",
        text
    );

    // redis_exists
    let text = call_tool_text(
        &redis::exists(state.clone()),
        json!({"keys": [format!("{p}key1"), format!("{p}key2"), format!("{p}none")]}),
    )
    .await;
    assert!(text.contains("2 of 3"), "exists: {}", text);

    // redis_memory_usage
    let text = call_tool_text(
        &redis::memory_usage(state.clone()),
        json!({"key": format!("{p}key1")}),
    )
    .await;
    assert!(text.contains("bytes"), "memory_usage: {}", text);

    // redis_object_encoding
    let text = call_tool_text(
        &redis::object_encoding(state.clone()),
        json!({"key": format!("{p}key1")}),
    )
    .await;
    assert!(
        text.contains("embstr") || text.contains("raw"),
        "object_encoding: {}",
        text
    );

    // redis_object_idletime
    let text = call_tool_text(
        &redis::object_idletime(state.clone()),
        json!({"key": format!("{p}key1")}),
    )
    .await;
    assert!(text.contains("idle"), "object_idletime: {}", text);

    // redis_object_help
    let text = call_tool_text(&redis::object_help(state.clone()), json!({})).await;
    assert!(text.contains("OBJECT"), "object_help: {}", text);

    // redis_mget
    let text = call_tool_text(
        &redis::mget(state.clone()),
        json!({"keys": [format!("{p}key1"), format!("{p}key2"), format!("{p}none")]}),
    )
    .await;
    assert!(text.contains("value1"), "mget: {}", text);
    assert!(text.contains("value2"), "mget: {}", text);
    assert!(text.contains("nil"), "mget nil: {}", text);

    // redis_strlen
    let text = call_tool_text(
        &redis::strlen(state.clone()),
        json!({"key": format!("{p}key1")}),
    )
    .await;
    assert!(text.contains("6"), "strlen: {}", text);

    // redis_getrange
    let text = call_tool_text(
        &redis::getrange(state.clone()),
        json!({"key": format!("{p}key1"), "start": 0, "end": 2}),
    )
    .await;
    assert!(text.contains("val"), "getrange: {}", text);

    // redis_randomkey (just verify it doesn't error)
    let text = call_tool_text(&redis::randomkey(state.clone()), json!({})).await;
    assert!(!text.is_empty(), "randomkey: {}", text);

    // redis_touch
    let text = call_tool_text(
        &redis::touch(state.clone()),
        json!({"keys": [format!("{p}key1"), format!("{p}key2")]}),
    )
    .await;
    assert!(text.contains("2"), "touch: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Key write tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_key_write_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_rw_state(ctx.port);
    let ro_state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "kw_";

    cleanup(&mut conn, p).await;

    // redis_set
    let text = call_tool_text(
        &redis::set(state.clone()),
        json!({"key": format!("{p}k1"), "value": "v1"}),
    )
    .await;
    assert!(text.contains("OK"), "set: {}", text);

    // Verify
    let text = call_tool_text(
        &redis::get(ro_state.clone()),
        json!({"key": format!("{p}k1")}),
    )
    .await;
    assert!(text.contains("v1"), "get after set: {}", text);

    // redis_set with EX
    let text = call_tool_text(
        &redis::set(state.clone()),
        json!({"key": format!("{p}k_ex"), "value": "v", "ex": 3600}),
    )
    .await;
    assert!(text.contains("OK"), "set ex: {}", text);

    // redis_setnx (should succeed)
    let text = call_tool_text(
        &redis::setnx(state.clone()),
        json!({"key": format!("{p}k_nx"), "value": "first"}),
    )
    .await;
    assert!(text.contains("OK"), "setnx: {}", text);

    // redis_setnx (should fail -- key exists)
    let text = call_tool_text(
        &redis::setnx(state.clone()),
        json!({"key": format!("{p}k_nx"), "value": "second"}),
    )
    .await;
    assert!(text.contains("already exists"), "setnx exists: {}", text);

    // redis_expire
    let text = call_tool_text(
        &redis::expire(state.clone()),
        json!({"key": format!("{p}k1"), "seconds": 300}),
    )
    .await;
    assert!(text.contains("OK"), "expire: {}", text);

    // redis_persist
    let text = call_tool_text(
        &redis::persist(state.clone()),
        json!({"key": format!("{p}k1")}),
    )
    .await;
    assert!(text.contains("OK"), "persist: {}", text);

    // redis_rename
    let text = call_tool_text(
        &redis::rename(state.clone()),
        json!({"key": format!("{p}k1"), "newkey": format!("{p}k1_ren")}),
    )
    .await;
    assert!(text.contains("OK"), "rename: {}", text);

    // redis_mset
    let text = call_tool_text(
        &redis::mset(state.clone()),
        json!({"entries": [
            {"key": format!("{p}mk1"), "value": "mv1"},
            {"key": format!("{p}mk2"), "value": "mv2"}
        ]}),
    )
    .await;
    assert!(text.contains("OK"), "mset: {}", text);

    // redis_copy
    let text = call_tool_text(
        &redis::copy(state.clone()),
        json!({"source": format!("{p}mk1"), "destination": format!("{p}mk1_cp")}),
    )
    .await;
    assert!(text.contains("OK"), "copy: {}", text);

    // redis_incr
    let _ = call_tool_text(
        &redis::set(state.clone()),
        json!({"key": format!("{p}ctr"), "value": "10"}),
    )
    .await;
    let text = call_tool_text(
        &redis::incr(state.clone()),
        json!({"key": format!("{p}ctr")}),
    )
    .await;
    assert!(text.contains("11"), "incr: {}", text);

    // redis_decr
    let text = call_tool_text(
        &redis::decr(state.clone()),
        json!({"key": format!("{p}ctr")}),
    )
    .await;
    assert!(text.contains("10"), "decr: {}", text);

    // redis_append
    let text = call_tool_text(
        &redis::append(state.clone()),
        json!({"key": format!("{p}mk1"), "value": "_app"}),
    )
    .await;
    assert!(text.contains("length"), "append: {}", text);

    // redis_setrange
    let _ = call_tool_text(
        &redis::set(state.clone()),
        json!({"key": format!("{p}sr"), "value": "Hello World"}),
    )
    .await;
    let text = call_tool_text(
        &redis::setrange(state.clone()),
        json!({"key": format!("{p}sr"), "offset": 6, "value": "Redis"}),
    )
    .await;
    assert!(text.contains("11"), "setrange: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Key destructive tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_key_destructive_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_full_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "kd_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("MSET")
        .arg(format!("{p}d1"))
        .arg("v1")
        .arg(format!("{p}d2"))
        .arg("v2")
        .arg(format!("{p}u1"))
        .arg("v3")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_del
    let text = call_tool_text(
        &redis::del(state.clone()),
        json!({"keys": [format!("{p}d1"), format!("{p}d2")]}),
    )
    .await;
    assert!(text.contains("Deleted 2"), "del: {}", text);

    // redis_unlink
    let text = call_tool_text(
        &redis::unlink(state.clone()),
        json!({"keys": [format!("{p}u1")]}),
    )
    .await;
    assert!(text.contains("Unlinked 1"), "unlink: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Dump / Restore
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_dump_restore() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let ro_state = make_state(ctx.port);
    let rw_state = make_rw_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "dr_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("SET")
        .arg(format!("{p}src"))
        .arg("hello_dump")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_dump
    let text = call_tool_text(
        &redis::dump(ro_state.clone()),
        json!({"key": format!("{p}src")}),
    )
    .await;
    assert!(text.contains("bytes"), "dump: {}", text);

    // Extract hex from dump output (last line)
    let hex = text.lines().last().unwrap_or("").trim();
    assert!(!hex.is_empty(), "dump hex empty");

    // redis_restore into a new key
    let text = call_tool_text(
        &redis::restore(rw_state.clone()),
        json!({"key": format!("{p}dst"), "ttl_ms": 0, "serialized_value": hex}),
    )
    .await;
    assert!(text.contains("OK"), "restore: {}", text);

    // Verify
    let text = call_tool_text(
        &redis::get(ro_state.clone()),
        json!({"key": format!("{p}dst")}),
    )
    .await;
    assert!(text.contains("hello_dump"), "restored value: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Hash tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_hash_read_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "hr_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("HSET")
        .arg(format!("{p}h"))
        .arg("f1")
        .arg("v1")
        .arg("f2")
        .arg("v2")
        .arg("f3")
        .arg("v3")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_hgetall
    let text = call_tool_text(
        &redis::hgetall(state.clone()),
        json!({"key": format!("{p}h")}),
    )
    .await;
    assert!(text.contains("f1"), "hgetall: {}", text);
    assert!(text.contains("v1"), "hgetall: {}", text);
    assert!(text.contains("3 fields"), "hgetall: {}", text);

    // redis_hget
    let text = call_tool_text(
        &redis::hget(state.clone()),
        json!({"key": format!("{p}h"), "field": "f1"}),
    )
    .await;
    assert!(text.contains("v1"), "hget: {}", text);

    // redis_hmget
    let text = call_tool_text(
        &redis::hmget(state.clone()),
        json!({"key": format!("{p}h"), "fields": ["f1", "f2", "nope"]}),
    )
    .await;
    assert!(text.contains("v1"), "hmget: {}", text);
    assert!(text.contains("v2"), "hmget: {}", text);
    assert!(text.contains("nil"), "hmget nil: {}", text);

    // redis_hlen
    let text = call_tool_text(&redis::hlen(state.clone()), json!({"key": format!("{p}h")})).await;
    assert!(text.contains("3"), "hlen: {}", text);

    // redis_hexists
    let text = call_tool_text(
        &redis::hexists(state.clone()),
        json!({"key": format!("{p}h"), "field": "f1"}),
    )
    .await;
    assert!(text.contains("exists"), "hexists: {}", text);

    // redis_hkeys
    let text = call_tool_text(
        &redis::hkeys(state.clone()),
        json!({"key": format!("{p}h")}),
    )
    .await;
    assert!(text.contains("f1"), "hkeys: {}", text);
    assert!(text.contains("f2"), "hkeys: {}", text);

    // redis_hvals
    let text = call_tool_text(
        &redis::hvals(state.clone()),
        json!({"key": format!("{p}h")}),
    )
    .await;
    assert!(text.contains("v1"), "hvals: {}", text);
    assert!(text.contains("v2"), "hvals: {}", text);

    cleanup(&mut conn, p).await;
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_hash_write_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_rw_state(ctx.port);
    let ro_state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "hw_";

    cleanup(&mut conn, p).await;

    // redis_hset
    let text = call_tool_text(
        &redis::hset(state.clone()),
        json!({"key": format!("{p}h"), "fields": {"a": "1", "b": "2"}}),
    )
    .await;
    assert!(text.contains("2 field(s) added"), "hset: {}", text);

    // redis_hdel
    let text = call_tool_text(
        &redis::hdel(state.clone()),
        json!({"key": format!("{p}h"), "fields": ["a"]}),
    )
    .await;
    assert!(text.contains("Deleted 1"), "hdel: {}", text);

    // redis_hincrby
    let _ = call_tool_text(
        &redis::hset(state.clone()),
        json!({"key": format!("{p}h"), "fields": {"counter": "10"}}),
    )
    .await;
    let text = call_tool_text(
        &redis::hincrby(state.clone()),
        json!({"key": format!("{p}h"), "field": "counter", "increment": 5}),
    )
    .await;
    assert!(text.contains("15"), "hincrby: {}", text);

    // Verify
    let text = call_tool_text(
        &redis::hgetall(ro_state.clone()),
        json!({"key": format!("{p}h")}),
    )
    .await;
    assert!(text.contains("counter"), "hgetall verify: {}", text);
    assert!(text.contains("15"), "hgetall verify: {}", text);

    // redis_hexpire -- set a 300s TTL on field "counter" (requires Redis 7.4+)
    let text = call_tool_text(
        &redis::hexpire(state.clone()),
        json!({"key": format!("{p}h"), "fields": ["counter"], "seconds": 300}),
    )
    .await;
    assert!(text.contains("expiry set"), "hexpire: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// List tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_list_read_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "lr_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("RPUSH")
        .arg(format!("{p}l"))
        .arg("a")
        .arg("b")
        .arg("c")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_lrange
    let text = call_tool_text(
        &redis::lrange(state.clone()),
        json!({"key": format!("{p}l")}),
    )
    .await;
    assert!(text.contains("a"), "lrange: {}", text);
    assert!(text.contains("b"), "lrange: {}", text);
    assert!(text.contains("3 elements"), "lrange: {}", text);

    // redis_llen
    let text = call_tool_text(&redis::llen(state.clone()), json!({"key": format!("{p}l")})).await;
    assert!(text.contains("3"), "llen: {}", text);

    // redis_lindex
    let text = call_tool_text(
        &redis::lindex(state.clone()),
        json!({"key": format!("{p}l"), "index": 1}),
    )
    .await;
    assert!(text.contains("b"), "lindex: {}", text);

    cleanup(&mut conn, p).await;
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_list_write_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_rw_state(ctx.port);
    let ro_state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "lw_";

    cleanup(&mut conn, p).await;

    // redis_lpush
    let text = call_tool_text(
        &redis::lpush(state.clone()),
        json!({"key": format!("{p}l"), "elements": ["b", "a"]}),
    )
    .await;
    assert!(text.contains("pushed 2"), "lpush: {}", text);

    // redis_rpush
    let text = call_tool_text(
        &redis::rpush(state.clone()),
        json!({"key": format!("{p}l"), "elements": ["c"]}),
    )
    .await;
    assert!(text.contains("pushed 1"), "rpush: {}", text);

    // Verify
    let text = call_tool_text(
        &redis::lrange(ro_state.clone()),
        json!({"key": format!("{p}l")}),
    )
    .await;
    assert!(text.contains("3 elements"), "lrange verify: {}", text);

    // redis_lpop
    let text = call_tool_text(&redis::lpop(state.clone()), json!({"key": format!("{p}l")})).await;
    assert!(text.contains("LPOP"), "lpop: {}", text);

    // redis_rpop
    let text = call_tool_text(&redis::rpop(state.clone()), json!({"key": format!("{p}l")})).await;
    assert!(text.contains("RPOP"), "rpop: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Set tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_set_read_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "sr_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("SADD")
        .arg(format!("{p}s1"))
        .arg("a")
        .arg("b")
        .arg("c")
        .query_async(&mut conn)
        .await
        .unwrap();
    let _: () = ::redis::cmd("SADD")
        .arg(format!("{p}s2"))
        .arg("b")
        .arg("c")
        .arg("d")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_smembers
    let text = call_tool_text(
        &redis::smembers(state.clone()),
        json!({"key": format!("{p}s1")}),
    )
    .await;
    assert!(text.contains("3 members"), "smembers: {}", text);

    // redis_scard
    let text = call_tool_text(
        &redis::scard(state.clone()),
        json!({"key": format!("{p}s1")}),
    )
    .await;
    assert!(text.contains("3"), "scard: {}", text);

    // redis_sismember
    let text = call_tool_text(
        &redis::sismember(state.clone()),
        json!({"key": format!("{p}s1"), "member": "a"}),
    )
    .await;
    assert!(text.contains("member"), "sismember: {}", text);

    // redis_sunion
    let text = call_tool_text(
        &redis::sunion(state.clone()),
        json!({"keys": [format!("{p}s1"), format!("{p}s2")]}),
    )
    .await;
    assert!(text.contains("4"), "sunion: {}", text);

    // redis_sinter
    let text = call_tool_text(
        &redis::sinter(state.clone()),
        json!({"keys": [format!("{p}s1"), format!("{p}s2")]}),
    )
    .await;
    assert!(text.contains("2"), "sinter: {}", text);

    // redis_sdiff
    let text = call_tool_text(
        &redis::sdiff(state.clone()),
        json!({"keys": [format!("{p}s1"), format!("{p}s2")]}),
    )
    .await;
    assert!(text.contains("a"), "sdiff: {}", text);

    cleanup(&mut conn, p).await;
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_set_write_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_rw_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "sw_";

    cleanup(&mut conn, p).await;

    // redis_sadd
    let text = call_tool_text(
        &redis::sadd(state.clone()),
        json!({"key": format!("{p}s"), "members": ["x", "y", "z"]}),
    )
    .await;
    assert!(text.contains("added 3"), "sadd: {}", text);

    // redis_srem
    let text = call_tool_text(
        &redis::srem(state.clone()),
        json!({"key": format!("{p}s"), "members": ["x"]}),
    )
    .await;
    assert!(text.contains("Removed 1"), "srem: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Sorted set tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_sorted_set_read_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "zr_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("ZADD")
        .arg(format!("{p}z"))
        .arg(1.0)
        .arg("alice")
        .arg(2.0)
        .arg("bob")
        .arg(3.0)
        .arg("charlie")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_zrange
    let text = call_tool_text(
        &redis::zrange(state.clone()),
        json!({"key": format!("{p}z")}),
    )
    .await;
    assert!(text.contains("alice"), "zrange: {}", text);
    assert!(text.contains("3 members"), "zrange: {}", text);

    // redis_zrange with scores
    let text = call_tool_text(
        &redis::zrange(state.clone()),
        json!({"key": format!("{p}z"), "withscores": true}),
    )
    .await;
    assert!(text.contains("score"), "zrange withscores: {}", text);

    // redis_zcard
    let text = call_tool_text(
        &redis::zcard(state.clone()),
        json!({"key": format!("{p}z")}),
    )
    .await;
    assert!(text.contains("3"), "zcard: {}", text);

    // redis_zscore
    let text = call_tool_text(
        &redis::zscore(state.clone()),
        json!({"key": format!("{p}z"), "member": "bob"}),
    )
    .await;
    assert!(text.contains("2"), "zscore: {}", text);

    // redis_zrank
    let text = call_tool_text(
        &redis::zrank(state.clone()),
        json!({"key": format!("{p}z"), "member": "alice"}),
    )
    .await;
    assert!(text.contains("0"), "zrank: {}", text);

    // redis_zcount
    let text = call_tool_text(
        &redis::zcount(state.clone()),
        json!({"key": format!("{p}z"), "min": "1", "max": "2"}),
    )
    .await;
    assert!(text.contains("2"), "zcount: {}", text);

    // redis_zrangebyscore
    let text = call_tool_text(
        &redis::zrangebyscore(state.clone()),
        json!({"key": format!("{p}z"), "min": "1", "max": "2"}),
    )
    .await;
    assert!(text.contains("alice"), "zrangebyscore: {}", text);
    assert!(text.contains("bob"), "zrangebyscore: {}", text);

    cleanup(&mut conn, p).await;
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_sorted_set_write_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_rw_state(ctx.port);
    let ro_state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "zw_";

    cleanup(&mut conn, p).await;

    // redis_zadd
    let text = call_tool_text(
        &redis::zadd(state.clone()),
        json!({"key": format!("{p}z"), "members": [{"score": 1.0, "member": "x"}, {"score": 2.0, "member": "y"}]}),
    )
    .await;
    assert!(text.contains("2"), "zadd: {}", text);

    // redis_zrem
    let text = call_tool_text(
        &redis::zrem(state.clone()),
        json!({"key": format!("{p}z"), "members": ["x"]}),
    )
    .await;
    assert!(text.contains("Removed 1"), "zrem: {}", text);

    // Clear the key and re-seed three members for zremrangebyscore coverage
    let _: () = ::redis::cmd("DEL")
        .arg(format!("{p}z"))
        .query_async(&mut conn)
        .await
        .unwrap();
    let text = call_tool_text(
        &redis::zadd(state.clone()),
        json!({"key": format!("{p}z"), "members": [
            {"score": 1.0, "member": "alice"},
            {"score": 2.0, "member": "bob"},
            {"score": 3.0, "member": "charlie"}
        ]}),
    )
    .await;
    assert!(text.contains("3"), "zadd reseed: {}", text);

    // redis_zremrangebyscore -- remove bob (score 2) only
    let text = call_tool_text(
        &redis::zremrangebyscore(state.clone()),
        json!({"key": format!("{p}z"), "min": "2", "max": "2"}),
    )
    .await;
    assert!(text.contains("Removed 1"), "zremrangebyscore: {}", text);

    // Verify alice and charlie remain, bob is gone
    let text = call_tool_text(
        &redis::zrange(ro_state.clone()),
        json!({"key": format!("{p}z")}),
    )
    .await;
    assert!(
        text.contains("alice") && text.contains("charlie"),
        "zrange after remove: {}",
        text
    );
    assert!(!text.contains("bob"), "bob should be gone: {}", text);

    // Unbounded: remove all remaining members with -inf/+inf
    let text = call_tool_text(
        &redis::zremrangebyscore(state.clone()),
        json!({"key": format!("{p}z"), "min": "-inf", "max": "+inf"}),
    )
    .await;
    assert!(text.contains("Removed 2"), "zremrangebyscore all: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Stream tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_stream_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let rw_state = make_rw_state(ctx.port);
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "st_";

    cleanup(&mut conn, p).await;

    // redis_xadd
    let text = call_tool_text(
        &redis::xadd(rw_state.clone()),
        json!({"key": format!("{p}s"), "fields": {"name": "alice", "action": "login"}}),
    )
    .await;
    assert!(
        text.contains("OK") || text.contains("Added") || text.contains("added"),
        "xadd: {}",
        text
    );

    // Add another entry
    let _ = call_tool_text(
        &redis::xadd(rw_state.clone()),
        json!({"key": format!("{p}s"), "fields": {"name": "bob", "action": "logout"}}),
    )
    .await;

    // redis_xlen
    let text = call_tool_text(&redis::xlen(state.clone()), json!({"key": format!("{p}s")})).await;
    assert!(text.contains("2"), "xlen: {}", text);

    // redis_xinfo_stream
    let text = call_tool_text(
        &redis::xinfo_stream(state.clone()),
        json!({"key": format!("{p}s")}),
    )
    .await;
    assert!(text.contains(&format!("{p}s")), "xinfo_stream: {}", text);

    // redis_xrange
    let text = call_tool_text(
        &redis::xrange(state.clone()),
        json!({"key": format!("{p}s")}),
    )
    .await;
    assert!(text.contains("2 entries"), "xrange: {}", text);

    // redis_xtrim
    let text = call_tool_text(
        &redis::xtrim(rw_state.clone()),
        json!({"key": format!("{p}s"), "strategy": "MAXLEN", "threshold": "1"}),
    )
    .await;
    assert!(
        text.contains("Trimmed") || text.contains("trimmed"),
        "xtrim: {}",
        text
    );

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Pub/Sub tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_pubsub_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);

    // redis_pubsub_channels (no active channels expected)
    let text = call_tool_text(&redis::pubsub_channels(state.clone()), json!({})).await;
    assert!(
        text.contains("No active") || text.contains("channels"),
        "pubsub_channels: {}",
        text
    );

    // redis_pubsub_numsub
    let text = call_tool_text(
        &redis::pubsub_numsub(state.clone()),
        json!({"channels": ["ps_test_channel"]}),
    )
    .await;
    assert!(
        text.contains("subscriber") || text.contains("0"),
        "pubsub_numsub: {}",
        text
    );
}

// ============================================================================
// Diagnostic tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_diagnostics_tools() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "diag_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("SET")
        .arg(format!("{p}k"))
        .arg("hello")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_health_check
    let text = call_tool_text(&redis::health_check(state.clone()), json!({})).await;
    assert!(text.contains("Health Check"), "health_check: {}", text);
    assert!(text.contains("PONG"), "health_check ping: {}", text);
    assert!(text.contains("Version"), "health_check version: {}", text);

    // redis_key_summary
    let text = call_tool_text(
        &redis::key_summary(state.clone()),
        json!({"key": format!("{p}k")}),
    )
    .await;
    assert!(text.contains("Key Summary"), "key_summary: {}", text);
    assert!(text.contains("string"), "key_summary type: {}", text);
    assert!(text.contains("no expiry"), "key_summary ttl: {}", text);

    // redis_key_summary for non-existent key
    let text = call_tool_text(
        &redis::key_summary(state.clone()),
        json!({"key": format!("{p}nonexistent")}),
    )
    .await;
    assert!(text.contains("does not exist"), "key_summary nil: {}", text);

    // redis_hotkeys
    let text = call_tool_text(
        &redis::hotkeys(state.clone()),
        json!({"pattern": format!("{p}*")}),
    )
    .await;
    assert!(
        text.contains("Hotkeys") || text.contains(&format!("{p}k")),
        "hotkeys: {}",
        text
    );

    // redis_connection_summary
    let text = call_tool_text(&redis::connection_summary(state.clone()), json!({})).await;
    assert!(
        text.contains("Connection Summary"),
        "connection_summary: {}",
        text
    );
    assert!(
        text.contains("Total connections"),
        "connection_summary total: {}",
        text
    );

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Raw command tool
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_raw_command() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_full_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "raw_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("SET")
        .arg(format!("{p}k"))
        .arg("rawval")
        .query_async(&mut conn)
        .await
        .unwrap();

    // Allowed command
    let text = call_tool_text(
        &redis::redis_command(state.clone()),
        json!({"command": "GET", "args": [format!("{p}k")]}),
    )
    .await;
    assert!(text.contains("rawval"), "raw GET: {}", text);

    // Blocked command (FLUSHALL)
    let result = redis::redis_command(state.clone())
        .call(json!({"command": "FLUSHALL", "args": []}))
        .await;
    assert!(result.is_error, "FLUSHALL should be blocked");

    // Blocked subcommand (CONFIG SET)
    let result = redis::redis_command(state.clone())
        .call(json!({"command": "CONFIG", "args": ["SET", "hz", "10"]}))
        .await;
    assert!(result.is_error, "CONFIG SET should be blocked via raw");

    // Dry run
    let text = call_tool_text(
        &redis::redis_command(state.clone()),
        json!({"command": "INFO", "args": [], "dry_run": true}),
    )
    .await;
    assert!(text.contains("dry_run"), "dry_run: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Policy enforcement
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_policy_enforcement() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let ro_state = make_state(ctx.port);
    let rw_state = make_rw_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;
    let p = "pol_";

    cleanup(&mut conn, p).await;

    let _: () = ::redis::cmd("SET")
        .arg(format!("{p}k"))
        .arg("polval")
        .query_async(&mut conn)
        .await
        .unwrap();

    // Write tool should fail with read-only policy
    let result = redis::set(ro_state.clone())
        .call(json!({"key": format!("{p}k"), "value": "new"}))
        .await;
    assert!(result.is_error, "set should fail in read-only");

    // Destructive tool should fail with read-write policy
    let result = redis::del(rw_state.clone())
        .call(json!({"keys": [format!("{p}k")]}))
        .await;
    assert!(result.is_error, "del should fail in read-write");

    // Destructive tool (flushdb) should fail with read-write policy
    let result = redis::flushdb(rw_state.clone()).call(json!({})).await;
    assert!(result.is_error, "flushdb should fail in read-write");

    // redis_command should fail without full tier
    let result = redis::redis_command(rw_state.clone())
        .call(json!({"command": "PING", "args": []}))
        .await;
    assert!(result.is_error, "redis_command should fail in read-write");

    // Verify original data unchanged
    let text = call_tool_text(
        &redis::get(ro_state.clone()),
        json!({"key": format!("{p}k")}),
    )
    .await;
    assert!(text.contains("polval"), "data unchanged: {}", text);

    cleanup(&mut conn, p).await;
}

// ============================================================================
// Flushdb tool (with full tier)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_flushdb_tool() {
    let ctx = get_redis().await.expect("Failed to get Redis container");
    let state = make_full_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    // Use a dedicated DB (SELECT 1) to avoid interfering with other tests on DB 0.
    // But since tool handlers use their own connections via URL, we pass db=1 in URL.
    let url_db1 = format!("redis://localhost:{}/1", ctx.port);
    let policy = Arc::new(Policy::new(
        PolicyConfig {
            tier: SafetyTier::Full,
            ..Default::default()
        },
        HashMap::new(),
        "test".to_string(),
    ));
    let db1_state = Arc::new(
        AppState::new(
            redisctl_mcp::state::CredentialSource::Profiles(vec![]),
            policy,
            Some(url_db1.clone()),
            false,
            None,
        )
        .unwrap(),
    );

    // Switch our direct connection to DB 1 too
    let _: () = ::redis::cmd("SELECT")
        .arg(1)
        .query_async(&mut conn)
        .await
        .unwrap();

    let _: () = ::redis::cmd("SET")
        .arg("flush_test")
        .arg("v")
        .query_async(&mut conn)
        .await
        .unwrap();

    let text = call_tool_text(&redis::flushdb(db1_state.clone()), json!({})).await;
    assert!(text.contains("OK"), "flushdb: {}", text);

    // Verify empty in DB 1
    let text = call_tool_text(&redis::dbsize(db1_state.clone()), json!({})).await;
    assert!(text.contains("0 keys"), "dbsize after flush: {}", text);

    // Switch back to DB 0 for other tests' connections
    let _: () = ::redis::cmd("SELECT")
        .arg(0)
        .query_async(&mut conn)
        .await
        .unwrap();

    // Verify DB 0 has its own state (make_full_state uses DB 0)
    let text = call_tool_text(&redis::dbsize(state.clone()), json!({})).await;
    assert!(text.contains("keys"), "db0 still has keys: {}", text);
}
