#![cfg(feature = "database")]
//! Synthetic RESP regressions: non-positive TTLs must never reach Redis, where
//! `EXPIRE`/`HEXPIRE` would delete the key (fields) instead of setting a TTL.

mod support;

use std::time::Duration;

use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tower_mcp::Tool;

/// Read the bulk-string command arrays emitted by redis-rs, not arbitrary RESP.
async fn read_command(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> Vec<String> {
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with('*'), "expected command array: {line:?}");
    let count: usize = line[1..].trim_end().parse().unwrap();
    let mut command = Vec::with_capacity(count);
    for _ in 0..count {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.starts_with('$'), "expected bulk argument: {line:?}");
        let length: usize = line[1..].trim_end().parse().unwrap();
        let mut bytes = vec![0; length + 2];
        reader.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes[length..], b"\r\n");
        bytes.truncate(length);
        command.push(String::from_utf8(bytes).unwrap());
    }
    command
}

/// Assert the client sends nothing further and closes the connection.
async fn assert_no_more_commands(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) {
    let mut byte = [0];
    match reader.read(&mut byte).await {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        other => panic!("unexpected command or close error: {other:?}"),
    }
}

/// Run `make_tool` against a loopback server. `expected` is the one command the
/// server must receive after the handshake (answered with `reply`), or `None`
/// when no command at all may arrive.
async fn run(
    make_tool: impl FnOnce(std::sync::Arc<redisctl_mcp::AppState>) -> Tool,
    input: Value,
    expected: Option<(&[&str], &str)>,
) -> tower_mcp::CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected: Option<(Vec<String>, String)> = expected.map(|(command, reply)| {
        (
            command.iter().map(|arg| (*arg).to_string()).collect(),
            reply.to_string(),
        )
    });
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        // The default RESP2 connection setup sends two CLIENT SETINFO commands.
        for field in ["LIB-NAME", "LIB-VER"] {
            let handshake = read_command(&mut reader).await;
            assert_eq!(handshake.len(), 4);
            assert_eq!(&handshake[..3], &["CLIENT", "SETINFO", field]);
            write.write_all(b"+OK\r\n").await.unwrap();
        }
        if let Some((command, reply)) = expected {
            assert_eq!(read_command(&mut reader).await, command);
            write.write_all(reply.as_bytes()).await.unwrap();
        }
        assert_no_more_commands(&mut reader).await;
    });

    let state = support::database_state_write(format!("redis://{addr}"));
    let tool = make_tool(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .expect("synthetic tool call must complete");
    drop(tool);
    drop(state);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("synthetic server must finish without extra commands")
        .expect("synthetic server assertions must pass");
    result
}

fn text(result: &tower_mcp::CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1);
    result.content[0].as_text().expect("text content")
}

#[tokio::test]
async fn expire_refuses_non_positive_ttl_without_sending_expire() {
    for seconds in [json!(0), json!(-5), json!("-1"), json!(i64::MIN)] {
        let result = run(redis::expire, json!({"key": "k", "seconds": seconds}), None).await;
        assert!(result.is_error, "expected tool error: {result:?}");
        let message = text(&result);
        assert!(message.contains("must be positive"), "{message}");
        assert!(message.contains("redis_del"), "{message}");
        assert!(message.contains("redis_persist"), "{message}");
    }
}

#[tokio::test]
async fn expire_positive_ttl_still_sends_exact_command() {
    let result = run(
        redis::expire,
        json!({"key": "k", "seconds": 30}),
        Some((&["EXPIRE", "k", "30"], ":1\r\n")),
    )
    .await;
    assert!(!result.is_error, "unexpected tool error: {result:?}");
    assert_eq!(text(&result), "OK - TTL set to 30 seconds on 'k'");
}

#[tokio::test]
async fn hexpire_refuses_non_positive_ttl_without_sending_hexpire() {
    for seconds in [json!(0), json!(-5), json!("-1")] {
        let result = run(
            redis::hexpire,
            json!({"key": "h", "seconds": seconds, "fields": ["a", "b"]}),
            None,
        )
        .await;
        assert!(result.is_error, "expected tool error: {result:?}");
        let message = text(&result);
        assert!(message.contains("must be positive"), "{message}");
        assert!(message.contains("redis_hdel"), "{message}");
    }
}

#[tokio::test]
async fn hexpire_positive_ttl_still_sends_exact_command() {
    let result = run(
        redis::hexpire,
        json!({"key": "h", "seconds": 30, "fields": ["a", "b"], "condition": "nx"}),
        Some((
            &["HEXPIRE", "h", "30", "NX", "FIELDS", "2", "a", "b"],
            "*2\r\n:1\r\n:-2\r\n",
        )),
    )
    .await;
    assert!(!result.is_error, "unexpected tool error: {result:?}");
    assert_eq!(
        text(&result),
        "HEXPIRE 'h' (30s) on 2 field(s):\n  a: expiry set\n  b: field not found"
    );
}
