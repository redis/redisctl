#![cfg(all(feature = "database", feature = "test-support"))]
//! Live JSON and seed reporting regressions. Requires Docker and the pinned image:
//! ```text
//! docker pull redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8
//! cargo test -p redisctl-mcp --all-features --test redis_noop_live -- --ignored --nocapture
//! ```
//! One disposable container uses a random loopback-only port. Missing Docker,
//! image, or JSON support fails the test; fixture failures never count as skips.

mod support;

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ::redis::aio::MultiplexedConnection;
use redisctl_mcp::AppState;
use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tower_mcp::{CallToolResult, Tool};

const IMAGE: &str =
    "redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8";

struct Fixture {
    id: Option<String>,
    port: u16,
}

fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("Docker must be installed for the live regression test");
    assert!(output.status.success(), "Docker fixture command failed");
    String::from_utf8(output.stdout)
        .expect("Docker fixture output must be UTF-8")
        .trim()
        .to_owned()
}

impl Fixture {
    async fn start() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("redisctl-mcp-noop-{}-{nonce}", std::process::id());
        let id = docker(&[
            "run",
            "--detach",
            "--rm",
            "--pull=never",
            "--name",
            &name,
            "--publish",
            "127.0.0.1::6379",
            IMAGE,
        ]);
        // Install cleanup before resolving the port or checking readiness.
        let mut fixture = Self {
            id: Some(id),
            port: 0,
        };
        let binding = docker(&["port", fixture.id.as_deref().unwrap(), "6379/tcp"]);
        fixture.port = binding
            .strip_prefix("127.0.0.1:")
            .expect("fixture must bind only to loopback")
            .parse()
            .expect("fixture must have an allocated host port");
        let client = ::redis::Client::open(fixture.url("resp2")).unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await
                && ::redis::cmd("PING")
                    .query_async::<String>(&mut conn)
                    .await
                    .is_ok()
            {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "Redis fixture did not become ready"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let image_id = docker(&[
            "inspect",
            fixture.id.as_deref().unwrap(),
            "--format",
            "{{.Image}}",
        ]);
        eprintln!(
            "fixture image={IMAGE}; image_id={image_id}; container={name}; port={}",
            fixture.port
        );
        fixture
    }

    fn url(&self, protocol: &str) -> String {
        format!("redis://127.0.0.1:{}/?protocol={protocol}", self.port)
    }

    fn remove(&mut self) {
        docker(&["rm", "--force", self.id.as_deref().unwrap()]);
        self.id = None;
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(id) = &self.id {
            let _ = Command::new("docker").args(["rm", "--force", id]).output();
        }
    }
}

fn text(result: &CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0].as_text().expect("tool must return text")
}

async fn call(tool: Tool, input: Value) -> CallToolResult {
    tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .expect("live tool call must finish")
}

async fn document(conn: &mut MultiplexedConnection, key: &str) -> String {
    ::redis::cmd("JSON.GET")
        .arg(key)
        .query_async(conn)
        .await
        .unwrap()
}

async fn set_document(conn: &mut MultiplexedConnection, key: &str, value: &Value) -> String {
    let _: () = ::redis::cmd("JSON.SET")
        .arg(key)
        .arg("$")
        .arg(value.to_string())
        .query_async(conn)
        .await
        .expect("pinned Redis fixture must support JSON.SET");
    let stored = document(conn, key).await;
    assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), *value);
    stored
}

async fn toggle_cases(state: &Arc<AppState>, conn: &mut MultiplexedConnection, key: &str) {
    let initial = json!({"n":7,"text":"x","nil":null,"object":{"n":2},"values":[true,7,false,"x"]});
    for path in ["$.n", "$.text", "$.nil", "$.object", "$.missing"] {
        let before = set_document(conn, key, &initial).await;
        let result = call(
            redis::json_toggle(state.clone()),
            json!({"key":key,"path":path}),
        )
        .await;
        assert!(!result.is_error, "no-op is not a Redis error: {result:?}");
        assert_eq!(
            text(&result),
            format!("Nothing toggled: no boolean values matched '{key}' at '{path}'")
        );
        assert_eq!(
            document(conn, key).await,
            before,
            "toggle must preserve {path}"
        );
        eprintln!("toggle no-op {path}: {}", text(&result));
    }
    for (path, expected_reply, expected_values) in [
        ("$.values[0]", "[0]", json!([false, 7, false, "x"])),
        ("$.values[2]", "[1]", json!([true, 7, true, "x"])),
        (
            "$.values[*]",
            "[0, (nil), 1, (nil)]",
            json!([false, 7, true, "x"]),
        ),
    ] {
        set_document(conn, key, &initial).await;
        let result = call(
            redis::json_toggle(state.clone()),
            json!({"key":key,"path":path}),
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            format!("Toggled '{key}' at '{path}': {expected_reply}")
        );
        let mut expected = initial.clone();
        expected["values"] = expected_values;
        assert_eq!(
            document(conn, key).await,
            expected.to_string(),
            "toggle persisted state for {path}"
        );
        eprintln!("toggle control {path}: {}", text(&result));
    }
}

async fn increment_cases(
    state: &Arc<AppState>,
    conn: &mut MultiplexedConnection,
    key: &str,
    protocol: &str,
) {
    let initial = json!({"n":3,"nonnumeric":[null,"x",true],"mixed":[null,3,"x",0]});
    // Verify the real server's reply shapes independently of the tool decoder.
    for (path, resp2, resp3) in [
        ("$.missing", "[]", vec![]),
        (
            "$.nonnumeric[*]",
            "[null,null,null]",
            vec![::redis::Value::Nil; 3],
        ),
        ("$.n", "[5.0]", vec![::redis::Value::Double(5.0)]),
        (
            "$.mixed[*]",
            "[null,5.0,null,2.0]",
            vec![
                ::redis::Value::Nil,
                ::redis::Value::Double(5.0),
                ::redis::Value::Nil,
                ::redis::Value::Double(2.0),
            ],
        ),
    ] {
        set_document(conn, key, &initial).await;
        let reply: ::redis::Value = ::redis::cmd("JSON.NUMINCRBY")
            .arg(key)
            .arg(path)
            .arg(2.0)
            .query_async(conn)
            .await
            .unwrap();
        let expected = if protocol == "resp2" {
            ::redis::Value::BulkString(resp2.as_bytes().to_vec())
        } else {
            ::redis::Value::Array(resp3)
        };
        assert_eq!(reply, expected, "live {protocol} reply shape for {path}");
        eprintln!("increment wire {protocol} {path}: {reply:?}");
    }
    set_document(conn, key, &initial).await;
    let legacy_reply: ::redis::Value = ::redis::cmd("JSON.NUMINCRBY")
        .arg(key)
        .arg(".n")
        .arg(2.0)
        .query_async(conn)
        .await
        .unwrap();
    assert_eq!(
        legacy_reply,
        if protocol == "resp2" {
            ::redis::Value::BulkString(b"5.0".to_vec())
        } else {
            ::redis::Value::Array(vec![::redis::Value::Double(5.0)])
        }
    );
    eprintln!("increment wire {protocol} .n: {legacy_reply:?}");
    for (path, reason) in [
        ("$.missing", "no values"),
        ("$.nonnumeric[*]", "no numeric values"),
        (".missing", "no values"),
        (".nonnumeric[*]", "no numeric values"),
    ] {
        let before = set_document(conn, key, &initial).await;
        let result = call(
            redis::json_numincrby(state.clone()),
            json!({"key":key,"path":path,"value":2}),
        )
        .await;
        if protocol == "resp2" && !path.starts_with('$') {
            assert!(
                result.is_error,
                "legacy RESP2 failures must remain errors: {result:?}"
            );
            assert!(
                text(&result).contains("JSON.NUMINCRBY failed"),
                "{result:?}"
            );
        } else {
            assert!(!result.is_error, "no-op is not a Redis error: {result:?}");
            assert_eq!(
                text(&result),
                format!("Nothing incremented: path '{path}' matched {reason}")
            );
        }
        assert_eq!(
            document(conn, key).await,
            before,
            "increment must preserve {path}"
        );
        eprintln!("increment no-op {path}: {}", text(&result));
    }
    for (path, expected_reply, expected) in [
        (
            "$.n",
            "[5.0]",
            json!({"n":5.0,"nonnumeric":[null,"x",true],"mixed":[null,3,"x",0]}),
        ),
        (
            "$.mixed[*]",
            "[null,5.0,null,2.0]",
            json!({"n":3,"nonnumeric":[null,"x",true],"mixed":[null,5.0,"x",2.0]}),
        ),
        (
            ".n",
            if protocol == "resp2" { "5.0" } else { "[5.0]" },
            json!({"n":5.0,"nonnumeric":[null,"x",true],"mixed":[null,3,"x",0]}),
        ),
        (
            ".mixed[*]",
            if protocol == "resp2" {
                "2.0"
            } else {
                "[null,5.0,null,2.0]"
            },
            json!({"n":3,"nonnumeric":[null,"x",true],"mixed":[null,5.0,"x",2.0]}),
        ),
    ] {
        set_document(conn, key, &initial).await;
        let result = call(
            redis::json_numincrby(state.clone()),
            json!({"key":key,"path":path,"value":2}),
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            format!("New value at '{path}': {expected_reply}")
        );
        assert_eq!(
            document(conn, key).await,
            expected.to_string(),
            "increment persisted state for {path}"
        );
        eprintln!("increment control {path}: {}", text(&result));
    }
}

async fn error_cases(state: &Arc<AppState>, conn: &mut MultiplexedConnection, key: &str) {
    let _: () = ::redis::cmd("SET")
        .arg(key)
        .arg("unchanged")
        .query_async(conn)
        .await
        .unwrap();
    for (tool, input, message) in [
        (
            redis::json_toggle(state.clone()),
            json!({"key":key,"path":"$.n"}),
            "JSON.TOGGLE failed",
        ),
        (
            redis::json_numincrby(state.clone()),
            json!({"key":key,"path":"$.n","value":2}),
            "JSON.NUMINCRBY failed",
        ),
        (
            redis::seed(state.clone()),
            json!({"data_type":"hash","key_pattern":key,"count":5,"field_values":[{"name":"f","value":"v{i}"}]}),
            "Seed pipeline failed",
        ),
    ] {
        let result = call(tool, input).await;
        assert!(
            result.is_error,
            "server errors must remain errors: {result:?}"
        );
        assert!(text(&result).contains(message), "{result:?}");
        assert!(!text(&result).contains("Seed complete"), "{result:?}");
        let stored: String = ::redis::cmd("GET")
            .arg(key)
            .query_async(conn)
            .await
            .unwrap();
        assert_eq!(stored, "unchanged");
        eprintln!("preserved server error: {}", text(&result));
    }
}

async fn seed_cases(state: &Arc<AppState>, conn: &mut MultiplexedConnection, prefix: &str) {
    for (pattern, expected_keys, constant) in [
        (
            format!("{prefix}:constant"),
            vec![format!("{prefix}:constant")],
            true,
        ),
        (
            format!("{prefix}:indexed:{{i}}"),
            (0..5).map(|i| format!("{prefix}:indexed:{i}")).collect(),
            false,
        ),
    ] {
        let before: usize = ::redis::cmd("DBSIZE").query_async(conn).await.unwrap();
        let result = call(redis::seed(state.clone()), json!({"data_type":"string","key_pattern":pattern,"count":5,"value_pattern":"v{i}","batch_size":2})).await;
        assert!(!result.is_error, "{result:?}");
        let output = text(&result);
        assert!(
            output.starts_with("Seed complete: 5 string iterations completed (5 commands)"),
            "{output}"
        );
        assert!(!output.contains("items seeded"), "{output}");
        assert_eq!(
            output.contains("all iterations target one key"),
            constant,
            "{output}"
        );
        let after: usize = ::redis::cmd("DBSIZE").query_async(conn).await.unwrap();
        assert_eq!(after - before, expected_keys.len());
        let mut actual: Vec<String> = ::redis::cmd("KEYS")
            .arg(if constant {
                pattern.clone()
            } else {
                format!("{prefix}:indexed:*")
            })
            .query_async(conn)
            .await
            .unwrap();
        actual.sort();
        assert_eq!(actual, expected_keys, "distinct persisted keys");
        for (i, key) in actual.iter().enumerate() {
            let stored: String = ::redis::cmd("GET")
                .arg(key)
                .query_async(conn)
                .await
                .unwrap();
            assert_eq!(stored, format!("v{}", if constant { 4 } else { i }));
        }
        eprintln!(
            "seed distinct_keys={}; {}",
            actual.len(),
            output.lines().next().unwrap()
        );
    }
}

#[tokio::test]
#[ignore = "Requires Docker and the pinned Redis 8.6.7 image"]
async fn json_and_seed_reports_match_persisted_state_resp2_and_resp3() {
    let mut fixture = Fixture::start().await;
    for protocol in ["resp2", "resp3"] {
        let url = fixture.url(protocol);
        let mut conn = ::redis::Client::open(url.clone())
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();
        let info: String = ::redis::cmd("INFO")
            .arg("server")
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(
            info.lines().any(|line| line == "redis_version:8.6.7"),
            "unexpected Redis version"
        );
        let modules: String = ::redis::cmd("INFO")
            .arg("modules")
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(
            modules.contains("name=ReJSON,"),
            "pinned fixture must load RedisJSON"
        );
        let client_info: String = ::redis::cmd("CLIENT")
            .arg("INFO")
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(client_info.split_whitespace().any(|field| field
            == if protocol == "resp2" {
                "resp=2"
            } else {
                "resp=3"
            }));
        let module_versions = modules
            .lines()
            .filter(|line| line.starts_with("module:"))
            .collect::<Vec<_>>()
            .join("; ");
        eprintln!("protocol={protocol}; redis_version=8.6.7; {module_versions}");
        let state = support::database_state_write(url);
        let prefix = format!("noop-live:{protocol}");
        toggle_cases(&state, &mut conn, &format!("{prefix}:toggle")).await;
        error_cases(&state, &mut conn, &format!("{prefix}:wrongtype")).await;
        seed_cases(&state, &mut conn, &format!("{prefix}:seed")).await;
        increment_cases(&state, &mut conn, &format!("{prefix}:increment"), protocol).await;
    }
    fixture.remove();
}
