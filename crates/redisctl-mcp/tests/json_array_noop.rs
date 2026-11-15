#![cfg(all(feature = "database", feature = "test-support"))]
//! Actual-tool synthetic RESP2/RESP3 tests and one isolated live JSON fixture.
//! To include the live test:
//! ```text
//! docker pull redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8
//! cargo test -p redisctl-mcp --all-features --test json_array_noop -- --include-ignored --nocapture
//! ```
//! Docker/image/startup/JSON capability failures fail the live test, never skip it.

mod support;

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ::redis::aio::MultiplexedConnection;
use redisctl_mcp::AppState;
use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::{CallToolResult, Tool};

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Append,
    Insert,
    Trim,
}

impl Mutation {
    fn command(self) -> &'static str {
        match self {
            Self::Append => "JSON.ARRAPPEND",
            Self::Insert => "JSON.ARRINSERT",
            Self::Trim => "JSON.ARRTRIM",
        }
    }
    fn tool(self, state: Arc<AppState>) -> Tool {
        match self {
            Self::Append => redis::json_arrappend(state),
            Self::Insert => redis::json_arrinsert(state),
            Self::Trim => redis::json_arrtrim(state),
        }
    }
    fn input(self, key: &str, path: Option<&str>) -> Value {
        let mut input = match self {
            Self::Append => json!({"key":key,"values":["3"]}),
            Self::Insert => json!({"key":key,"index":"0","values":["3"]}),
            Self::Trim => json!({"key":key,"start":"0","stop":"0"}),
        };
        if let Some(path) = path {
            input["path"] = json!(path);
        }
        input
    }
    fn arguments(self, path: &str) -> Vec<String> {
        let mut args = vec![self.command(), "synthetic", path];
        args.extend(match self {
            Self::Append => vec!["3"],
            Self::Insert => vec!["0", "3"],
            Self::Trim => vec!["0", "0"],
        });
        args.into_iter().map(str::to_owned).collect()
    }
    fn noop(self, key: &str, path: &str) -> String {
        let action = match self {
            Self::Append => "appended",
            Self::Insert => "inserted",
            Self::Trim => "trimmed",
        };
        format!("Nothing {action}: no array values matched '{key}' at '{path}'")
    }
    fn success(self, path: &str, value: &str) -> String {
        match self {
            Self::Append | Self::Insert => format!("New array length: {value}"),
            Self::Trim => format!("Trimmed array at '{path}': new length {value}"),
        }
    }
}

const MUTATIONS: [Mutation; 3] = [Mutation::Append, Mutation::Insert, Mutation::Trim];

fn text(result: &CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0].as_text().expect("tool must return text")
}

async fn call(tool: Tool, input: Value) -> CallToolResult {
    tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .expect("tool call must finish")
}

async fn read_command(reader: &mut BufReader<OwnedReadHalf>) -> Vec<String> {
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let count: usize = line.strip_prefix('*').unwrap().trim().parse().unwrap();
    let mut args = Vec::new();
    for _ in 0..count {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let len: usize = line.strip_prefix('$').unwrap().trim().parse().unwrap();
        let mut bytes = vec![0; len + 2];
        reader.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes[len..], b"\r\n");
        args.push(String::from_utf8(bytes[..len].to_vec()).unwrap());
    }
    args
}

async fn synthetic(op: Mutation, resp3: bool, path: Option<&str>, reply: &str) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected = op.arguments(path.unwrap_or("$"));
    let response = reply.to_owned();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        if resp3 {
            assert_eq!(read_command(&mut reader).await, ["HELLO", "3"]);
            write.write_all(b"%1\r\n+proto\r\n:3\r\n").await.unwrap();
        }
        for field in ["LIB-NAME", "LIB-VER"] {
            let setup = read_command(&mut reader).await;
            assert_eq!(setup.len(), 4);
            assert_eq!(&setup[..3], &["CLIENT", "SETINFO", field]);
            write.write_all(b"+OK\r\n").await.unwrap();
        }
        assert_eq!(read_command(&mut reader).await, expected);
        write.write_all(response.as_bytes()).await.unwrap();
        let mut byte = [0];
        match reader.read(&mut byte).await {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("unexpected follow-up query or close error: {other:?}"),
        }
    });
    let protocol = if resp3 { "resp3" } else { "resp2" };
    let state = support::database_state_full(format!("redis://{addr}/?protocol={protocol}"));
    let result = call(op.tool(state.clone()), op.input("synthetic", path)).await;
    drop(state);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    result
}

async fn noop_replies(op: Mutation) {
    for resp3 in [false, true] {
        let nil = if resp3 { "_\r\n" } else { "$-1\r\n" };
        for (path, reply) in [
            (".n", nil.to_owned()),
            ("$.missing", "*0\r\n".to_owned()),
            ("$.n", format!("*1\r\n{nil}")),
            ("$..n", format!("*2\r\n{nil}{nil}")),
        ] {
            let result = synthetic(op, resp3, Some(path), &reply).await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(text(&result), op.noop("synthetic", path));
        }
    }
}

#[tokio::test]
async fn append_nil_empty_and_null_only_replies_report_noop() {
    noop_replies(Mutation::Append).await;
}
#[tokio::test]
async fn insert_nil_empty_and_null_only_replies_report_noop() {
    noop_replies(Mutation::Insert).await;
}
#[tokio::test]
async fn trim_nil_empty_and_null_only_replies_report_noop() {
    noop_replies(Mutation::Trim).await;
}

#[tokio::test]
async fn zero_mixed_and_non_nil_replies_keep_existing_success_text() {
    for op in MUTATIONS {
        for resp3 in [false, true] {
            let nil = if resp3 { "_\r\n" } else { "$-1\r\n" };
            for (reply, expected) in [
                (":0\r\n".to_owned(), "0"),
                ("*1\r\n:0\r\n".to_owned(), "[0]"),
                ("*1\r\n:3\r\n".to_owned(), "[3]"),
                (format!("*3\r\n:1\r\n{nil}:0\r\n"), "[1, (nil), 0]"),
                ("$2\r\n[]\r\n".to_owned(), "[]"),
                ("*1\r\n*0\r\n".to_owned(), "[[]]"),
            ] {
                let result = synthetic(op, resp3, Some("$.values[*]"), &reply).await;
                assert!(!result.is_error, "{result:?}");
                assert_eq!(text(&result), op.success("$.values[*]", expected));
            }
        }
    }
}

#[tokio::test]
async fn server_and_decode_errors_stay_errors() {
    for op in MUTATIONS {
        for resp3 in [false, true] {
            for reply in [
                "-WRONGTYPE fixture refusal\r\n",
                "*1\r\n-ERR nested failure\r\n",
                "?invalid\r\n",
            ] {
                let result = synthetic(op, resp3, Some("$.n"), reply).await;
                assert!(result.is_error, "{result:?}");
                assert!(
                    text(&result).contains(&format!("{} failed", op.command())),
                    "{result:?}"
                );
            }
        }
    }
}

#[tokio::test]
async fn omitted_paths_and_wire_arguments_are_preserved() {
    for op in MUTATIONS {
        for resp3 in [false, true] {
            let result = synthetic(op, resp3, None, ":1\r\n").await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(text(&result), op.success("$", "1"));
        }
    }
}

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
        .expect("Docker is required for the live test");
    assert!(output.status.success(), "Docker fixture command failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

impl Fixture {
    async fn start() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("redisctl-mcp-array-noop-{}-{nonce}", std::process::id());
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
        let mut fixture = Self {
            id: Some(id),
            port: 0,
        };
        fixture.port = docker(&["port", fixture.id.as_deref().unwrap(), "6379/tcp"])
            .strip_prefix("127.0.0.1:")
            .expect("fixture must bind only to loopback")
            .parse()
            .unwrap();
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

async fn document(conn: &mut MultiplexedConnection, key: &str) -> String {
    ::redis::cmd("JSON.GET")
        .arg(key)
        .query_async(conn)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "Requires Docker and the pinned Redis 8.6.7 image"]
async fn live_array_mutation_reports_match_persisted_json_resp2_and_resp3() {
    let mut fixture = Fixture::start().await;
    let mut mismatches = Vec::new();
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
        assert!(info.lines().any(|line| line == "redis_version:8.6.7"));
        let modules: String = ::redis::cmd("INFO")
            .arg("modules")
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(
            modules.contains("name=ReJSON,ver=80600,"),
            "fixture must load RedisJSON 80600"
        );
        let state = support::database_state_full(url.clone());
        let mut tool_conn = state.redis_connection_for_url(&url).await.unwrap();
        let client_info: String = ::redis::cmd("CLIENT")
            .arg("INFO")
            .query_async(&mut tool_conn)
            .await
            .unwrap();
        assert!(client_info.split_whitespace().any(|field| field
            == if protocol == "resp2" {
                "resp=2"
            } else {
                "resp=3"
            }));
        eprintln!(
            "protocol={protocol}; redis_version=8.6.7; {}",
            modules
                .lines()
                .filter(|line| line.starts_with("module:"))
                .collect::<Vec<_>>()
                .join("; ")
        );
        let initial = json!({"n":7,"nonnumeric":[7,"x",null],"values":[[1,2],"x",[],null],"array":[1,2,3],"empty":[]});
        for op in MUTATIONS {
            let key = format!("array-noop:{protocol}:{}", op.command());
            let mut mixed = initial.clone();
            let mut array = initial.clone();
            let mut emptied = initial.clone();
            let (mixed_length, array_length) = match op {
                Mutation::Append => {
                    mixed["values"] = json!([[1, 2, 3], "x", [3], null]);
                    array["array"] = json!([1, 2, 3, 3]);
                    ("[3, (nil), 1, (nil)]", "[4]")
                }
                Mutation::Insert => {
                    mixed["values"] = json!([[3, 1, 2], "x", [3], null]);
                    array["array"] = json!([3, 1, 2, 3]);
                    ("[3, (nil), 1, (nil)]", "[4]")
                }
                Mutation::Trim => {
                    mixed["values"] = json!([[1], "x", [], null]);
                    array["array"] = json!([1]);
                    ("[1, (nil), 0, (nil)]", "[1]")
                }
            };
            emptied["array"] = json!([]);
            let mut cases = vec![
                ("$.n", initial.clone(), op.noop(&key, "$.n"), false),
                (
                    "$.missing",
                    initial.clone(),
                    op.noop(&key, "$.missing"),
                    false,
                ),
                (
                    "$.nonnumeric[*]",
                    initial.clone(),
                    op.noop(&key, "$.nonnumeric[*]"),
                    false,
                ),
                (
                    "$.values[*]",
                    mixed,
                    op.success("$.values[*]", mixed_length),
                    false,
                ),
                ("$.array", array, op.success("$.array", array_length), false),
            ];
            if matches!(op, Mutation::Trim) {
                cases.push(("$.array", emptied, op.success("$.array", "[0]"), true));
                cases.push((
                    "$.empty",
                    initial.clone(),
                    op.success("$.empty", "[0]"),
                    false,
                ));
            }
            for (path, expected, expected_text, trim_to_empty) in cases {
                let _: () = ::redis::cmd("JSON.SET")
                    .arg(&key)
                    .arg("$")
                    .arg(initial.to_string())
                    .query_async(&mut conn)
                    .await
                    .expect("fixture must support JSON.SET");
                let before = document(&mut conn, &key).await;
                assert_eq!(before, initial.to_string());
                let mut input = op.input(&key, Some(path));
                if trim_to_empty {
                    input["start"] = json!(5);
                    input["stop"] = json!(2);
                }
                let result = call(op.tool(state.clone()), input).await;
                assert!(!result.is_error, "{protocol} {op:?} {path}: {result:?}");
                let after = document(&mut conn, &key).await;
                assert_eq!(
                    after,
                    expected.to_string(),
                    "{protocol} {op:?} {path} persisted JSON"
                );
                if text(&result) != expected_text {
                    mismatches.push(format!(
                        "{protocol} {op:?} {path}: expected {expected_text:?}, got {:?}",
                        text(&result)
                    ));
                }
                eprintln!(
                    "{protocol} {op:?} {path}; changed={}; output={}; before={before}; after={after}",
                    before != after,
                    text(&result)
                );
            }
            let wrong = format!("{key}:wrongtype");
            let _: () = ::redis::cmd("SET")
                .arg(&wrong)
                .arg("unchanged")
                .query_async(&mut conn)
                .await
                .unwrap();
            let result = call(op.tool(state.clone()), op.input(&wrong, Some("$.n"))).await;
            assert!(result.is_error, "{result:?}");
            assert!(
                text(&result).contains(&format!("{} failed", op.command())),
                "{result:?}"
            );
            let stored: String = ::redis::cmd("GET")
                .arg(&wrong)
                .query_async(&mut conn)
                .await
                .unwrap();
            assert_eq!(stored, "unchanged");
            eprintln!(
                "{protocol} {op:?} preserved server error: {}",
                text(&result)
            );
        }
    }
    fixture.remove();
    assert!(
        mismatches.is_empty(),
        "incorrect actual MCP reporting: {mismatches:#?}"
    );
}
