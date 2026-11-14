//! Actual-tool wire contracts for generic WRONGTYPE guidance, without a live Redis.
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::{CallToolResult, Tool};

use crate::state::AppState;

const HINT: &str = "\nHint: Check the affected key types with redis_type, if permitted, then use a command compatible with those types.";

struct Case {
    make: fn(Arc<AppState>) -> Tool,
    input: Value,
    command: &'static [&'static str],
    context: &'static str,
}

fn cases() -> [Case; 4] {
    [
        Case {
            make: super::get,
            input: json!({"key": "fixture:key"}),
            command: &["GET", "fixture:key"],
            context: "GET failed",
        },
        Case {
            make: super::incr,
            input: json!({"key": "fixture:key"}),
            command: &["INCR", "fixture:key"],
            context: "INCR failed",
        },
        Case {
            make: super::hgetall,
            input: json!({"key": "fixture:key"}),
            command: &["HGETALL", "fixture:key"],
            context: "HGETALL failed",
        },
        Case {
            make: super::sinter,
            input: json!({"keys": ["fixture:first", "fixture:second"]}),
            command: &["SINTER", "fixture:first", "fixture:second"],
            context: "SINTER failed",
        },
    ]
}

fn text(result: &CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0].as_text().expect("tool must return text")
}

async fn command(reader: &mut BufReader<OwnedReadHalf>) -> Vec<String> {
    let mut line = String::new();
    assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
    let count: usize = line.strip_prefix('*').unwrap().trim().parse().unwrap();
    let mut args = Vec::new();
    for _ in 0..count {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let length: usize = line.strip_prefix('$').unwrap().trim().parse().unwrap();
        let mut bytes = vec![0; length + 2];
        reader.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes[length..], b"\r\n");
        args.push(String::from_utf8(bytes[..length].to_vec()).unwrap());
    }
    args
}

async fn call(case: &Case, resp3: bool, reply: Option<&str>) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let expected = case.command.to_vec();
    let reply = reply.map(str::to_owned);
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
        assert_eq!(command(&mut reader).await, expected);
        if let Some(reply) = reply {
            write.write_all(reply.as_bytes()).await.unwrap();
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
        }
        // None closes the connection after receiving the data command.
    });
    let protocol = if resp3 { "resp3" } else { "resp2" };
    let mut state = AppState::without_config_for_tests(AppState::test_write_policy());
    state.database_url = Some(format!("redis://{address}/?protocol={protocol}"));
    let state = Arc::new(state);
    let tool = (case.make)(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(case.input.clone()))
        .await
        .expect("tool call must complete");
    drop(tool);
    drop(state);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("fixture must finish")
        .expect("wire assertions must pass");
    result
}

#[tokio::test]
async fn wrongtype_keeps_original_errors_and_adds_only_manual_guidance() {
    let mut missing = Vec::new();
    for resp3 in [false, true] {
        for case in cases() {
            for line in [
                "WRONGTYPE",
                "WRONGTYPE Operation against a key holding the wrong kind of value",
            ] {
                let reply = format!("-{line}\r\n");
                let result = call(&case, resp3, Some(&reply)).await;
                assert!(result.is_error, "{result:?}");
                let server_text = if line == "WRONGTYPE" {
                    "WRONGTYPE Unknown extension error encountered"
                } else {
                    line
                };
                let original = format!("Tool error: {}: {server_text}", case.context);
                if text(&result) == original {
                    missing.push(format!("resp3={resp3} {} {line}", case.context));
                } else {
                    assert_eq!(text(&result), format!("{original}{HINT}"));
                }
                eprintln!(
                    "preserved WRONGTYPE resp3={resp3} {}: {:?}",
                    case.context,
                    text(&result)
                );
            }
        }
    }
    assert!(
        missing.is_empty(),
        "missing manual WRONGTYPE guidance: {missing:?}"
    );
}

#[tokio::test]
async fn successful_nil_and_empty_replies_never_receive_guidance() {
    let cases = cases();
    for resp3 in [false, true] {
        let nil = if resp3 { "_\r\n" } else { "$-1\r\n" };
        let empty_hash = if resp3 { "%0\r\n" } else { "*0\r\n" };
        let hash = if resp3 {
            "%1\r\n+field\r\n+value\r\n"
        } else {
            "*2\r\n+field\r\n+value\r\n"
        };
        let empty_set = if resp3 { "~0\r\n" } else { "*0\r\n" };
        let set = if resp3 {
            "~2\r\n+one\r\n+two\r\n"
        } else {
            "*2\r\n+one\r\n+two\r\n"
        };
        for (case, reply, expected) in [
            (&cases[0], "$9\r\nWRONGTYPE\r\n", "WRONGTYPE"),
            (&cases[0], nil, "(nil) - key 'fixture:key' not found"),
            (&cases[1], ":0\r\n", "fixture:key: 0"),
            (
                &cases[2],
                empty_hash,
                "(empty hash or key 'fixture:key' not found)",
            ),
            (
                &cases[2],
                hash,
                "Hash 'fixture:key' (1 fields):\nfield: value",
            ),
            (&cases[3], empty_set, "(empty set)"),
            (&cases[3], set, "Intersection (2 members):\none\ntwo"),
        ] {
            let result = call(case, resp3, Some(reply)).await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(text(&result), expected);
        }
    }
}

#[tokio::test]
async fn other_server_transport_and_decode_errors_never_receive_guidance() {
    let cases = cases();
    for resp3 in [false, true] {
        for (case, line) in [
            (&cases[0], "ERR WRONGTYPE appears only in detail"),
            (&cases[0], "NOAUTH Authentication required."),
            (
                &cases[2],
                "NOPERM this user has no permissions to run the 'hgetall' command",
            ),
            (&cases[3], "ERR syntax error"),
            (&cases[1], "ERR value is not an integer or out of range"),
            (&cases[1], "ERR increment or decrement would overflow"),
            (
                &cases[0],
                "wrongtype Operation against a key holding the wrong kind of value",
            ),
            (&cases[0], "WRONGTYPE_OTHER marker"),
            (&cases[0], "TypeError WRONGTYPE marker"),
        ] {
            let reply = format!("-{line}\r\n");
            let result = call(case, resp3, Some(&reply)).await;
            assert!(result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                format!("Tool error: {}: {line}", case.context)
            );
        }
        for (case, reply, marker) in [
            (
                &cases[1],
                Some("$9\r\nWRONGTYPE\r\n"),
                "Response was of incompatible type",
            ),
            (
                &cases[2],
                Some("+WRONGTYPE\r\n"),
                "Response was of incompatible type",
            ),
            (&cases[0], Some("?invalid\r\n"), "GET failed:"),
            (&cases[0], None, "GET failed:"),
        ] {
            let result = call(case, resp3, reply).await;
            assert!(result.is_error, "{result:?}");
            assert!(
                text(&result).starts_with(&format!("Tool error: {}:", case.context)),
                "{result:?}"
            );
            assert!(text(&result).contains(marker), "{result:?}");
            assert!(!text(&result).contains("\nHint:"), "{result:?}");
        }
    }
}

#[tokio::test]
async fn policy_denial_precedes_connection_and_sends_no_command() {
    for resp3 in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let protocol = if resp3 { "resp3" } else { "resp2" };
        let mut state = AppState::without_config_for_tests(AppState::test_policy());
        state.database_url = Some(format!("redis://{address}/?protocol={protocol}"));
        let result = super::incr(Arc::new(state))
            .call(json!({"key": "fixture:key"}))
            .await;
        assert!(result.is_error, "{result:?}");
        assert!(
            text(&result).contains("not allowed by the active policy"),
            "{result:?}"
        );
        assert!(!text(&result).contains("\nHint:"), "{result:?}");
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "policy denial must not connect"
        );
    }
}
