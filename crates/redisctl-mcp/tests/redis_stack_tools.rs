#![cfg(feature = "database")]
//! Integration tests for Redis Stack MCP tools (JSON, Search, Bulk, Aliases).
//!
//! Requires a running Docker daemon. All tests are #[ignore] by default.
//!
//! Run with:
//! ```bash
//! cargo test -p redisctl-mcp --test redis_stack_tools --all-features -- --ignored --nocapture
//! ```
//!
//! For faster iteration with container reuse:
//! ```bash
//! REUSE_CONTAINERS=1 cargo test -p redisctl-mcp --test redis_stack_tools --all-features -- --ignored --nocapture
//! ```

mod support;

use std::sync::Arc;
use std::time::Duration;

use docker_wrapper::template::redis::RedisTemplate;
use docker_wrapper::testing::ContainerGuardBuilder;
use serde_json::json;
use serial_test::serial;
use tokio::sync::OnceCell;
use tower_mcp::Tool;

use redisctl_mcp::state::AppState;
use redisctl_mcp::tools::redis;
use support::{database_state_full, database_state_readonly, database_state_write};

// ============================================================================
// Test infrastructure
// ============================================================================

static REDIS_STACK_GUARD: OnceCell<RedisStackTestContext> = OnceCell::const_new();

struct RedisStackTestContext {
    _guard: docker_wrapper::testing::ContainerGuard<RedisTemplate>,
    port: u16,
}

unsafe impl Send for RedisStackTestContext {}
unsafe impl Sync for RedisStackTestContext {}

async fn get_redis_stack() -> anyhow::Result<&'static RedisStackTestContext> {
    REDIS_STACK_GUARD
        .get_or_try_init(|| async {
            let reuse = std::env::var("REUSE_CONTAINERS").is_ok();
            let template = RedisTemplate::new("redisctl-mcp-stack-test")
                .with_redis_stack()
                .port(16381);

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

            Ok(RedisStackTestContext {
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
    database_state_readonly(redis_url(port))
}

fn make_rw_state(port: u16) -> Arc<AppState> {
    database_state_write(redis_url(port))
}

fn make_full_state(port: u16) -> Arc<AppState> {
    database_state_full(redis_url(port))
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
// JSON tools (RedisJSON)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_json_tools() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let state = make_full_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "js_doc:").await;

    // redis_json_set -- set a JSON document
    let text = call_tool_text(
        &redis::json_set(state.clone()),
        json!({
            "key": "js_doc:1",
            "path": "$",
            "value": "{\"name\":\"alice\",\"age\":30,\"scores\":[1,2,3],\"active\":true}"
        }),
    )
    .await;
    assert!(text.contains("OK"), "json_set: {}", text);

    // redis_json_get -- get root
    let text = call_tool_text(&redis::json_get(state.clone()), json!({"key": "js_doc:1"})).await;
    assert!(text.contains("alice"), "json_get: {}", text);

    // redis_json_mget -- multi-key get at $.name (js_doc:2 missing)
    let text = call_tool_text(
        &redis::json_mget(state.clone()),
        json!({"keys": ["js_doc:1", "js_doc:2"], "path": "$.name"}),
    )
    .await;
    assert!(text.contains("alice"), "json_mget alice: {}", text);
    assert!(text.contains("nil"), "json_mget nil: {}", text);

    // redis_json_type -- root type
    let text = call_tool_text(
        &redis::json_type(state.clone()),
        json!({"key": "js_doc:1", "path": "$"}),
    )
    .await;
    assert!(text.contains("object"), "json_type: {}", text);

    // redis_json_strlen -- $.name length (alice = 5)
    let text = call_tool_text(
        &redis::json_strlen(state.clone()),
        json!({"key": "js_doc:1", "path": "$.name"}),
    )
    .await;
    assert!(text.contains("5"), "json_strlen: {}", text);

    // redis_json_objkeys -- root object keys
    let text = call_tool_text(
        &redis::json_objkeys(state.clone()),
        json!({"key": "js_doc:1", "path": "$"}),
    )
    .await;
    assert!(text.contains("name"), "json_objkeys name: {}", text);
    assert!(text.contains("scores"), "json_objkeys scores: {}", text);

    // redis_json_objlen -- root object length (4)
    let text = call_tool_text(
        &redis::json_objlen(state.clone()),
        json!({"key": "js_doc:1", "path": "$"}),
    )
    .await;
    assert!(text.contains("4"), "json_objlen: {}", text);

    // redis_json_arrlen -- $.scores length (3)
    let text = call_tool_text(
        &redis::json_arrlen(state.clone()),
        json!({"key": "js_doc:1", "path": "$.scores"}),
    )
    .await;
    assert!(text.contains("3"), "json_arrlen: {}", text);

    // redis_json_numincrby -- increment $.age by 1 (30 -> 31)
    let text = call_tool_text(
        &redis::json_numincrby(state.clone()),
        json!({"key": "js_doc:1", "path": "$.age", "value": 1.0}),
    )
    .await;
    assert!(text.contains("31"), "json_numincrby: {}", text);

    // redis_json_arrappend -- append 4 to $.scores (length 4)
    let text = call_tool_text(
        &redis::json_arrappend(state.clone()),
        json!({"key": "js_doc:1", "path": "$.scores", "values": ["4"]}),
    )
    .await;
    assert!(text.contains("4"), "json_arrappend: {}", text);

    // redis_json_toggle -- toggle $.active (true -> false)
    let text = call_tool_text(
        &redis::json_toggle(state.clone()),
        json!({"key": "js_doc:1", "path": "$.active"}),
    )
    .await;
    assert!(
        text.contains("false") || text.contains("0"),
        "json_toggle: {}",
        text
    );

    // redis_json_del -- delete $.active
    let text = call_tool_text(
        &redis::json_del(state.clone()),
        json!({"key": "js_doc:1", "path": "$.active"}),
    )
    .await;
    assert!(text.contains("1"), "json_del: {}", text);

    // redis_json_get after del -- active should be gone
    let text = call_tool_text(&redis::json_get(state.clone()), json!({"key": "js_doc:1"})).await;
    assert!(!text.contains("active"), "json_get after del: {}", text);

    // redis_json_clear -- clear $.scores (array -> [])
    let text = call_tool_text(
        &redis::json_clear(state.clone()),
        json!({"key": "js_doc:1", "path": "$.scores"}),
    )
    .await;
    assert!(text.contains("Cleared"), "json_clear: {}", text);

    cleanup(&mut conn, "js_doc:").await;
}

// ============================================================================
// Search tools (RediSearch)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_search_tools() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let full_state = make_full_state(ctx.port);
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "ft_doc:").await;
    // Drop a stale index from a previous run, if any.
    let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
        .arg("ft_test_idx")
        .query_async::<()>(&mut conn)
        .await;

    // Pre-populate JSON documents to index.
    for (key, doc) in [
        (
            "ft_doc:1",
            "{\"title\":\"Redis Search Guide\",\"tags\":\"redis,search\",\"score\":9.5}",
        ),
        (
            "ft_doc:2",
            "{\"title\":\"Redis JSON Tutorial\",\"tags\":\"redis,json\",\"score\":8.0}",
        ),
        (
            "ft_doc:3",
            "{\"title\":\"Vector Search with Redis\",\"tags\":\"redis,vector\",\"score\":9.0}",
        ),
    ] {
        let _: () = ::redis::cmd("JSON.SET")
            .arg(key)
            .arg("$")
            .arg(doc)
            .query_async(&mut conn)
            .await
            .unwrap();
    }

    // redis_ft_create -- create a JSON index
    let text = call_tool_text(
        &redis::ft_create(full_state.clone()),
        json!({
            "index": "ft_test_idx",
            "on": "JSON",
            "prefixes": ["ft_doc:"],
            "schema": [
                {"name": "$.title", "alias": "title", "field_type": "TEXT"},
                {"name": "$.tags", "alias": "tags", "field_type": "TAG"},
                {"name": "$.score", "alias": "score", "field_type": "NUMERIC", "sortable": true}
            ]
        }),
    )
    .await;
    assert!(
        text.contains("Created") || text.contains("OK"),
        "ft_create: {}",
        text
    );

    // Give the indexer a moment to settle.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // redis_ft_list -- index appears
    let text = call_tool_text(&redis::ft_list(state.clone()), json!({})).await;
    assert!(text.contains("ft_test_idx"), "ft_list: {}", text);

    // redis_ft_info -- info on index
    let text = call_tool_text(
        &redis::ft_info(state.clone()),
        json!({"index": "ft_test_idx"}),
    )
    .await;
    assert!(text.contains("ft_test_idx"), "ft_info: {}", text);

    // redis_ft_search -- full-text search for "redis"
    let text = call_tool_text(
        &redis::ft_search(state.clone()),
        json!({"index": "ft_test_idx", "query": "redis"}),
    )
    .await;
    assert!(
        text.contains("Total results") && !text.contains("Total results: 0"),
        "ft_search redis: {}",
        text
    );

    // redis_ft_search -- TAG search for json
    let text = call_tool_text(
        &redis::ft_search(state.clone()),
        json!({"index": "ft_test_idx", "query": "@tags:{json}"}),
    )
    .await;
    assert!(text.contains("ft_doc:2"), "ft_search tags: {}", text);

    // redis_ft_aggregate -- count all documents
    let text = call_tool_text(
        &redis::ft_aggregate(state.clone()),
        json!({
            "index": "ft_test_idx",
            "query": "*",
            "raw_args": ["GROUPBY", "0", "REDUCE", "COUNT", "0", "AS", "total"]
        }),
    )
    .await;
    assert!(
        text.contains("total") || text.contains("Total results"),
        "ft_aggregate: {}",
        text
    );

    // redis_ft_explain -- explain a query
    let text = call_tool_text(
        &redis::ft_explain(state.clone()),
        json!({"index": "ft_test_idx", "query": "redis"}),
    )
    .await;
    assert!(!text.is_empty(), "ft_explain empty");
    assert!(text.contains("redis"), "ft_explain: {}", text);

    // redis_ft_tagvals -- distinct tag values
    let text = call_tool_text(
        &redis::ft_tagvals(state.clone()),
        json!({"index": "ft_test_idx", "field": "tags"}),
    )
    .await;
    assert!(text.contains("redis"), "ft_tagvals: {}", text);

    // redis_ft_alter -- add a new numeric field
    let result = redis::ft_alter(full_state.clone())
        .call(json!({
            "index": "ft_test_idx",
            "field": {"name": "$.score", "alias": "score2", "field_type": "NUMERIC"}
        }))
        .await;
    assert!(!result.is_error, "ft_alter should not error: {:?}", result);

    // redis_ft_dropindex -- drop the index
    let text = call_tool_text(
        &redis::ft_dropindex(full_state.clone()),
        json!({"index": "ft_test_idx"}),
    )
    .await;
    assert!(text.contains("Dropped"), "ft_dropindex: {}", text);

    cleanup(&mut conn, "ft_doc:").await;
}

// ============================================================================
// Bulk / seed tools
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_bulk_seed_tools() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let state = make_rw_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "bulk_").await;
    cleanup(&mut conn, "seed_user:").await;
    cleanup(&mut conn, "seed_str:").await;
    cleanup(&mut conn, "seed_list:").await;
    cleanup(&mut conn, "seed_set:").await;
    cleanup(&mut conn, "seed_zs:").await;
    cleanup(&mut conn, "seed_json:").await;

    // redis_bulk_load -- 3 SET commands
    let text = call_tool_text(
        &redis::bulk_load(state.clone()),
        json!({"commands": [
            {"args": ["SET", "bulk_k1", "v1"]},
            {"args": ["SET", "bulk_k2", "v2"]},
            {"args": ["SET", "bulk_k3", "v3"]}
        ]}),
    )
    .await;
    assert!(text.contains("3"), "bulk_load SET: {}", text);

    // Verify bulk_k1
    let text = call_tool_text(&redis::get(state.clone()), json!({"key": "bulk_k1"})).await;
    assert!(text.contains("v1"), "get bulk_k1: {}", text);

    // redis_seed -- 5 hash records
    let text = call_tool_text(
        &redis::seed(state.clone()),
        json!({
            "data_type": "hash",
            "key_pattern": "seed_user:{i}",
            "count": 5,
            "field_values": [
                {"name": "id", "value": "{i}"},
                {"name": "username", "value": "user_{i}"}
            ]
        }),
    )
    .await;
    assert!(text.contains("5"), "seed: {}", text);

    // Verify seed_user:1 username
    let text = call_tool_text(
        &redis::hget(state.clone()),
        json!({"key": "seed_user:1", "field": "username"}),
    )
    .await;
    assert!(text.contains("user_1"), "hget seed_user:1: {}", text);

    // redis_bulk_load -- JSON.SET commands
    let text = call_tool_text(
        &redis::bulk_load(state.clone()),
        json!({"commands": [
            {"args": ["JSON.SET", "bulk_json:1", "$", "{\"x\":1}"]},
            {"args": ["JSON.SET", "bulk_json:2", "$", "{\"x\":2}"]}
        ]}),
    )
    .await;
    assert!(text.contains("2"), "bulk_load JSON.SET: {}", text);

    // Verify bulk_json:1 exists
    let text = call_tool_text(
        &redis::json_get(state.clone()),
        json!({"key": "bulk_json:1"}),
    )
    .await;
    assert!(text.contains("1"), "json_get bulk_json:1: {}", text);

    // redis_seed -- string type with value_pattern + ttl
    let text = call_tool_text(
        &redis::seed(state.clone()),
        json!({
            "data_type": "string",
            "key_pattern": "seed_str:{i}",
            "count": 3,
            "value_pattern": "val_{i}",
            "ttl": 3600
        }),
    )
    .await;
    assert!(text.contains("3"), "seed string: {}", text);

    let value: String = ::redis::cmd("GET")
        .arg("seed_str:0")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(value, "val_0", "seed string value");
    let ttl: i64 = ::redis::cmd("TTL")
        .arg("seed_str:0")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(ttl > 0, "seed string ttl should be positive: {}", ttl);

    // redis_seed -- list type with member_pattern
    let text = call_tool_text(
        &redis::seed(state.clone()),
        json!({
            "data_type": "list",
            "key_pattern": "seed_list:{i}",
            "count": 3,
            "member_pattern": "item_{i}"
        }),
    )
    .await;
    assert!(text.contains("3"), "seed list: {}", text);

    let members: Vec<String> = ::redis::cmd("LRANGE")
        .arg("seed_list:1")
        .arg(0)
        .arg(-1)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(members, vec!["item_1".to_string()], "seed list members");

    // redis_seed -- set type with member_pattern
    let text = call_tool_text(
        &redis::seed(state.clone()),
        json!({
            "data_type": "set",
            "key_pattern": "seed_set:{i}",
            "count": 3,
            "member_pattern": "item_{i}"
        }),
    )
    .await;
    assert!(text.contains("3"), "seed set: {}", text);

    let members: Vec<String> = ::redis::cmd("SMEMBERS")
        .arg("seed_set:2")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(members, vec!["item_2".to_string()], "seed set members");

    // redis_seed -- sorted_set type with member_pattern
    let text = call_tool_text(
        &redis::seed(state.clone()),
        json!({
            "data_type": "sorted_set",
            "key_pattern": "seed_zs:{i}",
            "count": 2,
            "member_pattern": "member_{i}"
        }),
    )
    .await;
    assert!(text.contains("2"), "seed sorted_set: {}", text);

    let members: Vec<String> = ::redis::cmd("ZRANGE")
        .arg("seed_zs:0")
        .arg(0)
        .arg(-1)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        members,
        vec!["member_0".to_string()],
        "seed sorted_set members"
    );

    // redis_seed -- json type with value_pattern
    let text = call_tool_text(
        &redis::seed(state.clone()),
        json!({
            "data_type": "json",
            "key_pattern": "seed_json:{i}",
            "count": 2,
            "value_pattern": "{\"id\":{i}}"
        }),
    )
    .await;
    assert!(text.contains("2"), "seed json: {}", text);

    let doc = call_tool_text(
        &redis::json_get(state.clone()),
        json!({"key": "seed_json:1"}),
    )
    .await;
    assert!(doc.contains("1"), "seed json get seed_json:1: {}", doc);

    // Error path: hash type without field_values
    let result = redis::seed(state.clone())
        .call(json!({
            "data_type": "hash",
            "key_pattern": "seed_err:{i}",
            "count": 1
        }))
        .await;
    assert!(
        result.is_error,
        "seed hash without field_values should error"
    );

    // Error path: invalid data_type
    let result = redis::seed(state.clone())
        .call(json!({
            "data_type": "bogus",
            "key_pattern": "seed_err:{i}",
            "count": 1
        }))
        .await;
    assert!(result.is_error, "seed with invalid data_type should error");

    cleanup(&mut conn, "bulk_").await;
    cleanup(&mut conn, "seed_user:").await;
    cleanup(&mut conn, "seed_str:").await;
    cleanup(&mut conn, "seed_list:").await;
    cleanup(&mut conn, "seed_set:").await;
    cleanup(&mut conn, "seed_zs:").await;
    cleanup(&mut conn, "seed_json:").await;
}

// ============================================================================
// Alias tools (session-scoped, in-memory state)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
#[serial]
async fn test_alias_tools() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let state = make_rw_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "alias_doc:").await;

    // redis_alias_list -- empty state
    let text = call_tool_text(&redis::alias_list(state.clone()), json!({})).await;
    assert!(text.contains("No aliases"), "alias_list empty: {}", text);

    // redis_alias_set -- save a simple alias
    let text = call_tool_text(
        &redis::alias_set(state.clone()),
        json!({"name": "ping-check", "commands": [{"args": ["PING"]}]}),
    )
    .await;
    assert!(text.contains("saved"), "alias_set ping-check: {}", text);

    // redis_alias_list -- ping-check appears
    let text = call_tool_text(&redis::alias_list(state.clone()), json!({})).await;
    assert!(
        text.contains("ping-check"),
        "alias_list ping-check: {}",
        text
    );

    // redis_alias_run -- run ping-check
    let text = call_tool_text(
        &redis::alias_run(state.clone()),
        json!({"name": "ping-check"}),
    )
    .await;
    assert!(text.contains("PONG"), "alias_run ping-check: {}", text);

    // redis_alias_set -- save a JSON round-trip alias
    let text = call_tool_text(
        &redis::alias_set(state.clone()),
        json!({"name": "json-roundtrip", "commands": [
            {"args": ["JSON.SET", "alias_doc:1", "$", "{\"v\":42}"]},
            {"args": ["JSON.GET", "alias_doc:1", "$"]}
        ]}),
    )
    .await;
    assert!(
        text.contains("2 command"),
        "alias_set json-roundtrip: {}",
        text
    );

    // redis_alias_run -- run json-roundtrip
    let text = call_tool_text(
        &redis::alias_run(state.clone()),
        json!({"name": "json-roundtrip"}),
    )
    .await;
    assert!(text.contains("42"), "alias_run json-roundtrip: {}", text);

    // redis_alias_delete -- delete ping-check
    let text = call_tool_text(
        &redis::alias_delete(state.clone()),
        json!({"name": "ping-check"}),
    )
    .await;
    assert!(
        text.contains("Deleted"),
        "alias_delete ping-check: {}",
        text
    );

    // redis_alias_list -- only json-roundtrip remains
    let text = call_tool_text(&redis::alias_list(state.clone()), json!({})).await;
    assert!(
        text.contains("json-roundtrip"),
        "alias_list remaining: {}",
        text
    );
    assert!(
        !text.contains("ping-check"),
        "alias_list ping-check gone: {}",
        text
    );

    // redis_alias_delete -- delete nonexistent
    let text = call_tool_text(&redis::alias_delete(state.clone()), json!({"name": "nope"})).await;
    assert!(text.contains("not found"), "alias_delete nope: {}", text);

    cleanup(&mut conn, "alias_doc:").await;
}

// ============================================================================
// Search: profile, synonyms, dictionaries, aliases (RediSearch)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_ft_profile() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let full_state = make_full_state(ctx.port);
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "ftpro_doc:").await;
    let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
        .arg("ftpro_idx")
        .query_async::<()>(&mut conn)
        .await;

    // Index a single document.
    let _: () = ::redis::cmd("JSON.SET")
        .arg("ftpro_doc:1")
        .arg("$")
        .arg("{\"title\":\"Redis Search Profiling Guide\"}")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_ft_create -- minimal TEXT index on the JSON title.
    let text = call_tool_text(
        &redis::ft_create(full_state.clone()),
        json!({
            "index": "ftpro_idx",
            "on": "JSON",
            "prefixes": ["ftpro_doc:"],
            "schema": [
                {"name": "$.title", "alias": "title", "field_type": "TEXT"}
            ]
        }),
    )
    .await;
    assert!(
        text.contains("Created") || text.contains("OK"),
        "ft_create: {}",
        text
    );

    // Give the indexer a moment to settle before profiling.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // redis_ft_profile -- profile a SEARCH query.
    let text = call_tool_text(
        &redis::ft_profile(state.clone()),
        json!({"index": "ftpro_idx", "command": "SEARCH", "query": "redis"}),
    )
    .await;
    assert!(
        text.contains("Profile for SEARCH"),
        "ft_profile header: {}",
        text
    );
    // The result/profile sections are rendered as "[0]:" and "[1]:" markers.
    assert!(
        text.contains("[0]:"),
        "ft_profile results section: {}",
        text
    );
    assert!(
        text.contains("[1]:"),
        "ft_profile profile section: {}",
        text
    );

    let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
        .arg("ftpro_idx")
        .query_async::<()>(&mut conn)
        .await;
    cleanup(&mut conn, "ftpro_doc:").await;
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_ft_synonym_and_dict() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let full_state = make_full_state(ctx.port);
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "ftsyn_doc:").await;
    let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
        .arg("ftsyn_idx")
        .query_async::<()>(&mut conn)
        .await;
    // Clear any leftover dictionary terms from a previous run.
    let _: Result<i64, _> = ::redis::cmd("FT.DICTDEL")
        .arg("ftsyn_dict")
        .arg("foo")
        .arg("bar")
        .query_async(&mut conn)
        .await;

    // redis_ft_create -- minimal TEXT index (synonyms are index-scoped).
    let text = call_tool_text(
        &redis::ft_create(full_state.clone()),
        json!({
            "index": "ftsyn_idx",
            "on": "JSON",
            "prefixes": ["ftsyn_doc:"],
            "schema": [
                {"name": "$.title", "alias": "title", "field_type": "TEXT"}
            ]
        }),
    )
    .await;
    assert!(
        text.contains("Created") || text.contains("OK"),
        "ft_create: {}",
        text
    );

    // redis_ft_synupdate -- add a synonym group.
    let text = call_tool_text(
        &redis::ft_synupdate(full_state.clone()),
        json!({
            "index": "ftsyn_idx",
            "group_id": "speed_group",
            "terms": ["fast", "quick", "speedy"]
        }),
    )
    .await;
    assert!(
        text.contains("Updated synonym group"),
        "ft_synupdate: {}",
        text
    );

    // redis_ft_syndump -- the group terms appear.
    let text = call_tool_text(
        &redis::ft_syndump(state.clone()),
        json!({"index": "ftsyn_idx"}),
    )
    .await;
    assert!(
        text.contains("speed_group") || text.contains("fast"),
        "ft_syndump: {}",
        text
    );

    // redis_ft_dictadd -- add dictionary terms.
    let text = call_tool_text(
        &redis::ft_dictadd(full_state.clone()),
        json!({"dict": "ftsyn_dict", "terms": ["foo", "bar"]}),
    )
    .await;
    assert!(text.contains("Added"), "ft_dictadd: {}", text);

    // redis_ft_dictdump -- both terms present.
    let text = call_tool_text(
        &redis::ft_dictdump(state.clone()),
        json!({"dict": "ftsyn_dict"}),
    )
    .await;
    assert!(text.contains("foo"), "ft_dictdump foo: {}", text);

    // redis_ft_dictdel -- remove "foo".
    let text = call_tool_text(
        &redis::ft_dictdel(full_state.clone()),
        json!({"dict": "ftsyn_dict", "terms": ["foo"]}),
    )
    .await;
    assert!(text.contains("Removed"), "ft_dictdel: {}", text);

    // redis_ft_dictdump -- "bar" remains, "foo" is gone.
    let text = call_tool_text(
        &redis::ft_dictdump(state.clone()),
        json!({"dict": "ftsyn_dict"}),
    )
    .await;
    assert!(text.contains("bar"), "ft_dictdump bar: {}", text);
    assert!(!text.contains("foo"), "ft_dictdump foo gone: {}", text);

    // Clean up the dictionary and index.
    let _: Result<i64, _> = ::redis::cmd("FT.DICTDEL")
        .arg("ftsyn_dict")
        .arg("bar")
        .query_async(&mut conn)
        .await;
    let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
        .arg("ftsyn_idx")
        .query_async::<()>(&mut conn)
        .await;
    cleanup(&mut conn, "ftsyn_doc:").await;
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_ft_alias_management() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let full_state = make_full_state(ctx.port);
    let state = make_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "ftalias_doc:").await;
    for idx in ["ft_alias_src_idx", "ft_alias_dst_idx"] {
        let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
            .arg(idx)
            .query_async::<()>(&mut conn)
            .await;
    }
    // Best-effort drop of a stale alias from a previous run.
    let _: Result<(), _> = ::redis::cmd("FT.ALIASDEL")
        .arg("ft_alias_test")
        .query_async::<()>(&mut conn)
        .await;

    // Index a document so searches via the alias return a hit.
    let _: () = ::redis::cmd("JSON.SET")
        .arg("ftalias_doc:1")
        .arg("$")
        .arg("{\"title\":\"Redis Alias Guide\"}")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_ft_create -- source index.
    let schema = json!([
        {"name": "$.title", "alias": "title", "field_type": "TEXT"}
    ]);
    let text = call_tool_text(
        &redis::ft_create(full_state.clone()),
        json!({
            "index": "ft_alias_src_idx",
            "on": "JSON",
            "prefixes": ["ftalias_doc:"],
            "schema": schema.clone()
        }),
    )
    .await;
    assert!(
        text.contains("Created") || text.contains("OK"),
        "ft_create src: {}",
        text
    );

    tokio::time::sleep(Duration::from_millis(150)).await;

    // redis_ft_aliasadd -- point alias at the source index.
    let text = call_tool_text(
        &redis::ft_aliasadd(full_state.clone()),
        json!({"alias": "ft_alias_test", "index": "ft_alias_src_idx"}),
    )
    .await;
    assert!(text.contains("Added alias"), "ft_aliasadd: {}", text);

    // redis_ft_search -- search via the alias should resolve to the source index.
    let text = call_tool_text(
        &redis::ft_search(state.clone()),
        json!({"index": "ft_alias_test", "query": "redis"}),
    )
    .await;
    assert!(
        text.contains("Total results") && !text.contains("Total results: 0"),
        "ft_search via alias: {}",
        text
    );

    // redis_ft_create -- destination index (same schema).
    let text = call_tool_text(
        &redis::ft_create(full_state.clone()),
        json!({
            "index": "ft_alias_dst_idx",
            "on": "JSON",
            "prefixes": ["ftalias_doc:"],
            "schema": schema
        }),
    )
    .await;
    assert!(
        text.contains("Created") || text.contains("OK"),
        "ft_create dst: {}",
        text
    );

    // redis_ft_aliasupdate -- repoint the alias.
    let text = call_tool_text(
        &redis::ft_aliasupdate(full_state.clone()),
        json!({"alias": "ft_alias_test", "index": "ft_alias_dst_idx"}),
    )
    .await;
    assert!(text.contains("Updated alias"), "ft_aliasupdate: {}", text);

    // redis_ft_aliasdel -- remove the alias.
    let text = call_tool_text(
        &redis::ft_aliasdel(full_state.clone()),
        json!({"alias": "ft_alias_test"}),
    )
    .await;
    assert!(text.contains("Deleted alias"), "ft_aliasdel: {}", text);

    for idx in ["ft_alias_src_idx", "ft_alias_dst_idx"] {
        let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
            .arg(idx)
            .query_async::<()>(&mut conn)
            .await;
    }
    cleanup(&mut conn, "ftalias_doc:").await;
}

// ============================================================================
// JSON array mutation tools (RedisJSON)
// ============================================================================

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_json_array_tools() {
    let ctx = get_redis_stack()
        .await
        .expect("Failed to get Redis Stack container");
    let state = make_full_state(ctx.port);
    let mut conn = get_conn(ctx.port).await;

    cleanup(&mut conn, "jarr_doc:").await;

    // Seed a document with an array at $.nums.
    let _: () = ::redis::cmd("JSON.SET")
        .arg("jarr_doc:1")
        .arg("$")
        .arg("{\"nums\":[10,20,30,40,50]}")
        .query_async(&mut conn)
        .await
        .unwrap();

    // redis_json_arrinsert -- insert 99 at the front -> new length 6.
    let text = call_tool_text(
        &redis::json_arrinsert(state.clone()),
        json!({"key": "jarr_doc:1", "path": "$.nums", "index": 0, "values": ["99"]}),
    )
    .await;
    assert!(text.contains("6"), "json_arrinsert: {}", text);

    // redis_json_arrpop -- pop the last element. Uses a legacy path (".nums")
    // because the tool deserializes the reply into a scalar String; a "$" path
    // returns a wrapped array that would not deserialize.
    let text = call_tool_text(
        &redis::json_arrpop(state.clone()),
        json!({"key": "jarr_doc:1", "path": ".nums"}),
    )
    .await;
    assert!(text.contains("Popped:"), "json_arrpop: {}", text);
    assert!(text.contains("50"), "json_arrpop value: {}", text);

    // redis_json_arrtrim -- trim to indices 0..=1 -> new length 2.
    let text = call_tool_text(
        &redis::json_arrtrim(state.clone()),
        json!({"key": "jarr_doc:1", "path": "$.nums", "start": 0, "stop": 1}),
    )
    .await;
    assert!(text.contains("Trimmed array"), "json_arrtrim: {}", text);
    assert!(text.contains("2"), "json_arrtrim length: {}", text);

    cleanup(&mut conn, "jarr_doc:").await;
}
