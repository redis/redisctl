#![cfg(all(feature = "database", feature = "test-support"))]
//! Actual-tool SCAN TYPE wire tests plus an isolated RedisJSON fixture.
//! ```text
//! docker pull redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8
//! cargo test -p redisctl-mcp --all-features --test scan_json_alias -- --include-ignored --nocapture
//! ```
//! Missing Docker, image, startup or JSON capability fails the live test.

mod support;

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::CallToolResult;

fn text(result: &CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0].as_text().expect("tool must return text")
}

async fn command(reader: &mut BufReader<OwnedReadHalf>) -> Vec<String> {
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

fn page(cursor: &str, keys: &[&str]) -> String {
    let mut response = format!("*2\r\n${}\r\n{cursor}\r\n*{}\r\n", cursor.len(), keys.len());
    for key in keys {
        response.push_str(&format!("${}\r\n{key}\r\n", key.len()));
    }
    response
}

async fn synthetic(
    resp3: bool,
    input: Value,
    expected_type: Option<&str>,
    replies: Vec<(&str, String)>,
) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let pattern = input.get("pattern").and_then(Value::as_str).unwrap_or("*");
    let expected: Vec<Vec<String>> = replies
        .iter()
        .map(|(cursor, _)| {
            let mut args = vec!["SCAN", cursor, "MATCH", pattern, "COUNT", "100"];
            if let Some(kind) = expected_type {
                args.extend(["TYPE", kind]);
            }
            args.into_iter().map(str::to_owned).collect()
        })
        .collect();
    let responses: Vec<String> = replies.into_iter().map(|(_, reply)| reply).collect();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        if resp3 {
            assert_eq!(command(&mut reader).await, ["HELLO", "3"]);
            write.write_all(b"%1\r\n+proto\r\n:3\r\n").await.unwrap();
        }
        for field in ["LIB-NAME", "LIB-VER"] {
            let setup = command(&mut reader).await;
            assert_eq!(setup.len(), 4);
            assert_eq!(&setup[..3], &["CLIENT", "SETINFO", field]);
            write.write_all(b"+OK\r\n").await.unwrap();
        }
        let mut commands = Vec::new();
        for reply in responses {
            commands.push(command(&mut reader).await);
            write.write_all(reply.as_bytes()).await.unwrap();
        }
        let mut byte = [0];
        match reader.read(&mut byte).await {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("unexpected extra query or close error: {other:?}"),
        }
        commands
    });
    let protocol = if resp3 { "resp3" } else { "resp2" };
    let state = support::database_state_readonly(format!("redis://{addr}/?protocol={protocol}"));
    let tool = redis::scan(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .unwrap();
    drop(tool);
    drop(state);
    let actual = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(actual, expected, "SCAN wire arguments and query count");
    result
}

#[tokio::test]
async fn json_alias_casing_maps_only_wire_type_and_keeps_caller_label() {
    for resp3 in [false, true] {
        for kind in ["json", "JSON", "jSoN"] {
            let result = synthetic(
                resp3,
                json!({"pattern":"doc:*","key_type":kind}),
                Some("ReJSON-RL"),
                vec![("0", page("0", &["doc:1"]))],
            )
            .await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                format!("Found 1 key(s) of type '{kind}' matching 'doc:*'\n\ndoc:1")
            );
        }
    }
}

#[tokio::test]
async fn empty_continuation_page_retains_type_filter_on_every_scan() {
    for resp3 in [false, true] {
        let result = synthetic(
            resp3,
            json!({"pattern":"doc:*","key_type":"JsOn","limit":"2"}),
            Some("ReJSON-RL"),
            vec![("0", page("7", &[])), ("7", page("0", &["doc:1"]))],
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            "Found 1 key(s) of type 'JsOn' matching 'doc:*'\n\ndoc:1"
        );
    }
}

#[tokio::test]
async fn native_core_custom_and_near_alias_bytes_pass_through() {
    for resp3 in [false, true] {
        for kind in [
            "ReJSON-RL",
            "rejson-rl",
            "string",
            "STRING",
            "sTrInG",
            "list",
            "Custom-模块",
            " json",
            "json ",
            "jſon",
            "",
        ] {
            let result = synthetic(
                resp3,
                json!({"pattern":"doc:*","key_type":kind}),
                Some(kind),
                vec![("0", page("0", &["doc:1"]))],
            )
            .await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                format!("Found 1 key(s) of type '{kind}' matching 'doc:*'\n\ndoc:1")
            );
        }
    }
}

#[tokio::test]
async fn omitted_filter_and_defaults_add_no_type_or_queries() {
    for resp3 in [false, true] {
        let result = synthetic(resp3, json!({}), None, vec![("0", page("0", &["k"]))]).await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(text(&result), "Found 1 key(s) matching '*'\n\nk");
    }
}

#[tokio::test]
async fn backend_errors_retain_scan_context() {
    for resp3 in [false, true] {
        let result = synthetic(
            resp3,
            json!({"key_type":"ReJSON-RL"}),
            Some("ReJSON-RL"),
            vec![("0", "-ERR fixture refusal\r\n".to_owned())],
        )
        .await;
        assert!(result.is_error, "{result:?}");
        assert!(text(&result).contains("SCAN failed"), "{result:?}");
        assert!(text(&result).contains("fixture refusal"), "{result:?}");
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
        let name = format!("redisctl-mcp-json-alias-{}-{nonce}", std::process::id());
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

fn listed_keys(output: &str) -> Vec<String> {
    let mut keys: Vec<String> = output
        .split_once("\n\n")
        .map(|(_, keys)| keys.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[tokio::test]
#[ignore = "Requires Docker and the pinned Redis 8.6.7 JSON image"]
async fn live_json_alias_native_and_string_controls_resp2_and_resp3() {
    let mut fixture = Fixture::start().await;
    let json_keys = ["scan-json-alias:json:1", "scan-json-alias:json:2"];
    let string_key = "scan-json-alias:string:1";
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
        for (i, key) in json_keys.iter().enumerate() {
            let _: () = ::redis::cmd("JSON.SET")
                .arg(key)
                .arg("$")
                .arg(json!({"id":i}).to_string())
                .query_async(&mut conn)
                .await
                .expect("fixture must support JSON.SET");
            let kind: String = ::redis::cmd("TYPE")
                .arg(key)
                .query_async(&mut conn)
                .await
                .unwrap();
            assert_eq!(kind, "ReJSON-RL");
            eprintln!("{protocol} TYPE {key}={kind}");
        }
        let _: () = ::redis::cmd("SET")
            .arg(string_key)
            .arg("unchanged")
            .query_async(&mut conn)
            .await
            .unwrap();
        let state = support::database_state_readonly(url.clone());
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
        for kind in [
            Some("json"),
            Some("JSON"),
            Some("jSoN"),
            Some("ReJSON-RL"),
            Some("string"),
            None,
        ] {
            let mut input = json!({"pattern":"scan-json-alias:*"});
            if let Some(kind) = kind {
                input["key_type"] = json!(kind);
            }
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                redis::scan(state.clone()).call(input),
            )
            .await
            .unwrap();
            assert!(!result.is_error, "{result:?}");
            let mut expected: Vec<String> = match kind {
                Some("string") => vec![string_key.to_owned()],
                None => json_keys
                    .iter()
                    .copied()
                    .chain([string_key])
                    .map(str::to_owned)
                    .collect(),
                _ => json_keys.iter().map(|key| (*key).to_owned()).collect(),
            };
            expected.sort();
            let label = kind
                .map(|kind| format!(" of type '{kind}'"))
                .unwrap_or_default();
            let expected_header = format!(
                "Found {} key(s){label} matching 'scan-json-alias:*'",
                expected.len()
            );
            if listed_keys(text(&result)) != expected
                || text(&result).lines().next() != Some(expected_header.as_str())
            {
                mismatches.push(format!(
                    "{protocol} {kind:?}: expected {expected:?} with {expected_header:?}, got {:?}",
                    text(&result)
                ));
            }
            eprintln!("{protocol} type={kind:?}; output={:?}", text(&result));
        }
        for (i, key) in json_keys.iter().enumerate() {
            let value: String = ::redis::cmd("JSON.GET")
                .arg(key)
                .query_async(&mut conn)
                .await
                .unwrap();
            assert_eq!(value, json!({"id":i}).to_string());
        }
        let value: String = ::redis::cmd("GET")
            .arg(string_key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert_eq!(value, "unchanged");
    }
    fixture.remove();
    assert!(
        mismatches.is_empty(),
        "incorrect actual MCP JSON alias results: {mismatches:#?}"
    );
}
