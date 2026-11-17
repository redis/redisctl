#![cfg(all(feature = "database", feature = "test-support"))]
//! Live tests for the list, stream consumer-group, publish and vector tools.
//!
//! ```text
//! docker pull redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8
//! cargo test -p redisctl-mcp --all-features --test db_command_gap_tools -- --include-ignored --nocapture
//! ```
//!
//! Set `REDISCTL_GAP_TOOLS_URL` (for example `redis://127.0.0.1:16390`) to run against
//! an existing Redis 8 server with the Query Engine instead of starting the pinned image.
//! Docker, image, startup or module failures fail the test, never skip it.

mod support;

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ::redis::aio::MultiplexedConnection;
use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tower_mcp::{CallToolResult, Tool};

const IMAGE: &str =
    "redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8";
const EXTERNAL_URL_ENV: &str = "REDISCTL_GAP_TOOLS_URL";

struct Fixture {
    container: Option<String>,
    base_url: String,
}

fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("Docker is required for the live test");
    assert!(output.status.success(), "Docker fixture command failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

impl Fixture {
    async fn start() -> Self {
        let fixture = match std::env::var(EXTERNAL_URL_ENV) {
            Ok(url) => Self {
                container: None,
                base_url: url.trim_end_matches('/').to_owned(),
            },
            Err(_) => {
                let nonce = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let name = format!("redisctl-mcp-gap-tools-{}-{nonce}", std::process::id());
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
                let port = docker(&["port", &id, "6379/tcp"]);
                let port = port
                    .strip_prefix("127.0.0.1:")
                    .expect("fixture must bind only to loopback")
                    .to_owned();
                eprintln!("fixture image={IMAGE}; container={name}; port={port}");
                Self {
                    container: Some(id),
                    base_url: format!("redis://127.0.0.1:{port}"),
                }
            }
        };
        let client = ::redis::Client::open(fixture.url("resp2")).unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(mut conn) = client.get_multiplexed_async_connection().await
                    && ::redis::cmd("PING")
                        .query_async::<String>(&mut conn)
                        .await
                        .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("Redis fixture must become ready");
        fixture
    }

    fn url(&self, protocol: &str) -> String {
        format!("{}/?protocol={protocol}", self.base_url)
    }

    async fn setup_conn(&self) -> MultiplexedConnection {
        ::redis::Client::open(self.url("resp2"))
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(id) = &self.container {
            let _ = Command::new("docker").args(["rm", "--force", id]).output();
        }
    }
}

fn text(result: &CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|c| c.as_text())
        .unwrap_or_default()
        .to_owned()
}

async fn ok(tool: Tool, input: Value) -> String {
    let result = tool.call(input).await;
    let body = text(&result);
    assert!(!result.is_error, "tool failed: {body}");
    body
}

async fn err(tool: Tool, input: Value) -> String {
    let result = tool.call(input).await;
    let body = text(&result);
    assert!(result.is_error, "tool should fail: {body}");
    body
}

async fn delete_prefix(conn: &mut MultiplexedConnection, prefix: &str) {
    let keys: Vec<String> = ::redis::cmd("KEYS")
        .arg(format!("{prefix}*"))
        .query_async(conn)
        .await
        .unwrap();
    for key in keys {
        let _: i64 = ::redis::cmd("DEL")
            .arg(key)
            .query_async(conn)
            .await
            .unwrap();
    }
}

async fn check_lists(url: &str, conn: &mut MultiplexedConnection, p: &str) {
    let rw = support::database_state_write(url.to_owned());
    let ro = support::database_state_readonly(url.to_owned());
    let key = format!("{p}list");
    let _: i64 = ::redis::cmd("RPUSH")
        .arg(&key)
        .arg(&["a", "b", "a", "c", "a"])
        .query_async(conn)
        .await
        .unwrap();

    let out = ok(
        redis::lrem(rw.clone()),
        json!({"key": key, "count": 2, "value": "a"}),
    )
    .await;
    assert!(out.contains("removed 2 occurrence(s) of 'a'"), "{out}");
    let out = ok(
        redis::lrem(rw.clone()),
        json!({"key": key, "count": "0", "value": "zz"}),
    )
    .await;
    assert!(out.starts_with("No elements removed"), "{out}");

    let out = ok(
        redis::lset(rw.clone()),
        json!({"key": key, "index": -1, "value": "z"}),
    )
    .await;
    assert!(out.contains("set index -1"), "{out}");
    let out = err(
        redis::lset(rw.clone()),
        json!({"key": key, "index": 9, "value": "z"}),
    )
    .await;
    assert!(out.contains("LSET failed"), "{out}");

    let out = ok(
        redis::ltrim(rw.clone()),
        json!({"key": key, "start": 0, "stop": 1}),
    )
    .await;
    assert!(out.contains("length now 2"), "{out}");
    let items: Vec<String> = ::redis::cmd("LRANGE")
        .arg(&key)
        .arg(0)
        .arg(-1)
        .query_async(conn)
        .await
        .unwrap();
    assert_eq!(items, ["b", "c"]);

    err(
        redis::lrem(ro),
        json!({"key": key, "count": 0, "value": "b"}),
    )
    .await;
}

async fn check_streams(url: &str, conn: &mut MultiplexedConnection, p: &str) {
    let rw = support::database_state_write(url.to_owned());
    let ro = support::database_state_readonly(url.to_owned());
    let full = support::database_state_full(url.to_owned());
    let key = format!("{p}stream");
    let mut ids = Vec::new();
    for n in 0..3 {
        let id: String = ::redis::cmd("XADD")
            .arg(&key)
            .arg("*")
            .arg("n")
            .arg(n)
            .query_async(conn)
            .await
            .unwrap();
        ids.push(id);
    }

    let out = ok(redis::xinfo_groups(ro.clone()), json!({"key": key})).await;
    assert!(out.contains("has no consumer groups"), "{out}");

    let out = ok(
        redis::xgroup_create(rw.clone()),
        json!({"key": key, "group": "g", "id": "0"}),
    )
    .await;
    assert!(out.contains("created group 'g'"), "{out}");
    err(
        redis::xgroup_create(rw.clone()),
        json!({"key": key, "group": "g"}),
    )
    .await;
    let out = ok(
        redis::xgroup_create(rw.clone()),
        json!({"key": format!("{p}made"), "group": "g", "mkstream": true}),
    )
    .await;
    assert!(out.contains("created group 'g'"), "{out}");

    let out = ok(
        redis::xreadgroup(rw.clone()),
        json!({"key": key, "group": "g", "consumer": "c1", "count": 2}),
    )
    .await;
    assert!(out.contains("read 2 entries"), "{out}");
    assert!(out.contains(&format!("{}  n=0", ids[0])), "{out}");

    let out = ok(redis::xinfo_groups(ro.clone()), json!({"key": key})).await;
    assert!(out.contains("name=g") && out.contains("pending=2"), "{out}");
    let out = ok(
        redis::xinfo_consumers(ro.clone()),
        json!({"key": key, "group": "g"}),
    )
    .await;
    assert!(
        out.contains("name=c1") && out.contains("pending=2"),
        "{out}"
    );

    let out = ok(
        redis::xpending(ro.clone()),
        json!({"key": key, "group": "g"}),
    )
    .await;
    assert!(out.contains("2 pending"), "{out}");
    assert!(out.contains("c1: 2 pending"), "{out}");
    let out = ok(
        redis::xpending(ro.clone()),
        json!({"key": key, "group": "g", "count": 10, "consumer": "c1"}),
    )
    .await;
    assert!(out.contains("pending entries (2)"), "{out}");
    assert!(
        out.contains("consumer=c1") && out.contains("deliveries=1"),
        "{out}"
    );
    err(
        redis::xpending(ro.clone()),
        json!({"key": key, "group": "g", "consumer": "c1"}),
    )
    .await;

    let out = ok(
        redis::xack(rw.clone()),
        json!({"key": key, "group": "g", "ids": [ids[0]]}),
    )
    .await;
    assert!(out.contains("acknowledged 1 of 1"), "{out}");
    let out = ok(
        redis::xack(rw.clone()),
        json!({"key": key, "group": "g", "ids": [ids[0]]}),
    )
    .await;
    assert!(out.starts_with("No entries acknowledged"), "{out}");

    let out = ok(
        redis::xdel(rw.clone()),
        json!({"key": key, "ids": [ids[1]]}),
    )
    .await;
    assert!(out.contains("deleted 1 of 1"), "{out}");
    let out = ok(
        redis::xreadgroup(rw.clone()),
        json!({"key": key, "group": "g", "consumer": "c1", "id": "0"}),
    )
    .await;
    assert!(out.contains(&format!("{}  (deleted)", ids[1])), "{out}");
    let out = ok(
        redis::xreadgroup(rw.clone()),
        json!({"key": key, "group": "g", "consumer": "c2"}),
    )
    .await;
    assert!(out.contains("read 1 entry"), "{out}");
    let out = ok(
        redis::xreadgroup(rw.clone()),
        json!({"key": key, "group": "g", "consumer": "c2"}),
    )
    .await;
    assert!(out.contains("no new entries"), "{out}");

    err(
        redis::xreadgroup(ro.clone()),
        json!({"key": key, "group": "g", "consumer": "c3"}),
    )
    .await;
    err(redis::xgroup_destroy(rw), json!({"key": key, "group": "g"})).await;
    let out = ok(
        redis::xgroup_destroy(full.clone()),
        json!({"key": key, "group": "g"}),
    )
    .await;
    assert!(out.contains("destroyed group 'g'"), "{out}");
    let out = ok(
        redis::xgroup_destroy(full),
        json!({"key": key, "group": "g"}),
    )
    .await;
    assert!(out.starts_with("No group destroyed"), "{out}");
}

async fn check_publish(url: &str, fixture: &Fixture, p: &str) {
    let rw = support::database_state_write(url.to_owned());
    let ro = support::database_state_readonly(url.to_owned());
    let channel = format!("{p}channel");

    let out = ok(
        redis::publish(rw.clone()),
        json!({"channel": channel, "message": "hi"}),
    )
    .await;
    assert!(out.contains("received by 0 subscriber(s)"), "{out}");

    let mut pubsub = ::redis::Client::open(fixture.url("resp2"))
        .unwrap()
        .get_async_pubsub()
        .await
        .unwrap();
    pubsub.subscribe(&channel).await.unwrap();
    let out = ok(
        redis::publish(rw),
        json!({"channel": channel, "message": "hello"}),
    )
    .await;
    assert!(out.contains("received by 1 subscriber(s)"), "{out}");
    drop(pubsub);

    err(
        redis::publish(ro),
        json!({"channel": channel, "message": "x"}),
    )
    .await;
}

async fn check_vectors(url: &str, conn: &mut MultiplexedConnection, p: &str) {
    let rw = support::database_state_write(url.to_owned());
    let ro = support::database_state_readonly(url.to_owned());
    let index = format!("{p}idx");
    // An external server may still hold the index from an interrupted run.
    let _: Result<(), _> = ::redis::cmd("FT.DROPINDEX")
        .arg(&index)
        .query_async(conn)
        .await;
    let _: () = ::redis::cmd("FT.CREATE")
        .arg(&index)
        .arg(&["ON", "HASH", "PREFIX", "1"])
        .arg(format!("{p}doc:"))
        .arg(&["SCHEMA", "genre", "TAG", "embedding", "VECTOR", "FLAT", "6"])
        .arg(&["TYPE", "FLOAT32", "DIM", "3", "DISTANCE_METRIC", "L2"])
        .query_async(conn)
        .await
        .expect("fixture must support FT.CREATE with VECTOR fields");

    let docs = [
        ("a", "jazz", json!([1.0, 0.0, 0.0])),
        ("b", "rock", json!([0.0, 1.0, 0.0])),
        ("c", "jazz", json!([0.9, 0.1, 0.0])),
    ];
    for (id, genre, vector) in &docs {
        let key = format!("{p}doc:{id}");
        let _: i64 = ::redis::cmd("HSET")
            .arg(&key)
            .arg("genre")
            .arg(*genre)
            .query_async(conn)
            .await
            .unwrap();
        let out = ok(
            redis::vector_set_hash(rw.clone()),
            json!({"key": key, "field": "embedding", "vector": vector}),
        )
        .await;
        assert!(
            out.contains("3-dimension FLOAT32 vector (12 bytes)"),
            "{out}"
        );
    }
    err(
        redis::vector_set_hash(ro.clone()),
        json!({"key": format!("{p}doc:a"), "field": "embedding", "vector": [1.0]}),
    )
    .await;

    let out = ok(
        redis::vector_get_hash(ro.clone()),
        json!({"key": format!("{p}doc:c"), "field": "embedding"}),
    )
    .await;
    assert!(out.contains("3-dimension FLOAT32 vector"), "{out}");
    let decoded: Vec<f64> = serde_json::from_str(out.lines().last().unwrap()).unwrap();
    assert!(
        (decoded[0] - 0.9).abs() < 1e-6 && (decoded[1] - 0.1).abs() < 1e-6,
        "{out}"
    );
    // 3 bytes is not a whole number of FLOAT32 elements.
    let _: i64 = ::redis::cmd("HSET")
        .arg(format!("{p}doc:c"))
        .arg("label")
        .arg("abc")
        .query_async(conn)
        .await
        .unwrap();
    let out = err(
        redis::vector_get_hash(ro.clone()),
        json!({"key": format!("{p}doc:c"), "field": "label"}),
    )
    .await;
    assert!(
        out.contains("not a whole number of FLOAT32 elements"),
        "{out}"
    );
    let out = ok(
        redis::vector_get_hash(ro.clone()),
        json!({"key": format!("{p}doc:c"), "field": "missing"}),
    )
    .await;
    assert!(out.starts_with("(nil)"), "{out}");

    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let reply: Vec<::redis::Value> = ::redis::cmd("FT.SEARCH")
                .arg(&index)
                .arg("*")
                .arg("NOCONTENT")
                .query_async(conn)
                .await
                .unwrap();
            if matches!(reply.first(), Some(::redis::Value::Int(3))) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("index must contain the three documents");

    let out = ok(
        redis::ft_vector_search(ro.clone()),
        json!({"index": index, "vector_field": "embedding", "vector": [1.0, 0.0, 0.0], "k": 2}),
    )
    .await;
    let a = out.find(&format!("1. {p}doc:a")).expect(&out);
    let c = out.find(&format!("2. {p}doc:c")).expect(&out);
    assert!(a < c, "{out}");
    assert!(!out.contains(&format!("{p}doc:b")), "{out}");
    assert!(
        !out.contains("embedding:"),
        "vector bytes must not be printed: {out}"
    );

    let out = ok(
        redis::ft_vector_search(ro.clone()),
        json!({
            "index": index,
            "vector_field": "embedding",
            "vector": [1.0, 0.0, 0.0],
            "k": 1,
            "filter": "@genre:{rock}",
            "return_fields": ["genre"],
        }),
    )
    .await;
    assert!(out.contains(&format!("1. {p}doc:b")), "{out}");
    assert!(out.contains("genre: rock"), "{out}");

    err(
        redis::ft_vector_search(ro.clone()),
        json!({"index": index, "vector_field": "$.embedding", "vector": [1.0, 0.0, 0.0]}),
    )
    .await;
    err(
        redis::ft_vector_search(ro),
        json!({"index": index, "vector_field": "embedding", "vector": [1.0, 0.0, 0.0], "k": 0}),
    )
    .await;

    let _: () = ::redis::cmd("FT.DROPINDEX")
        .arg(&index)
        .query_async(conn)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker and the pinned Redis 8.6.7 image, or REDISCTL_GAP_TOOLS_URL"]
async fn live_gap_tools_resp2_and_resp3() {
    let fixture = Fixture::start().await;
    let mut conn = fixture.setup_conn().await;
    for protocol in ["resp2", "resp3"] {
        let url = fixture.url(protocol);
        let p = format!("gap_{protocol}_");
        delete_prefix(&mut conn, &p).await;
        check_lists(&url, &mut conn, &p).await;
        check_streams(&url, &mut conn, &p).await;
        check_publish(&url, &fixture, &p).await;
        check_vectors(&url, &mut conn, &p).await;
        delete_prefix(&mut conn, &p).await;
    }
}
