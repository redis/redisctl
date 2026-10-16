#![cfg(feature = "database")]
//! Synthetic RESP2 replies through actual tools and credential-free loopback targets.

mod support;

use std::time::Duration;

use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::CallToolResult;

async fn read_command(reader: &mut BufReader<OwnedReadHalf>) -> Vec<String> {
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

async fn call_tool(input: Value, response: String) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("$")
        .to_owned();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        // redis-rs0.27.6 RESP2 setup uses two CLIENT SETINFO commands.
        for field in ["LIB-NAME", "LIB-VER"] {
            let handshake = read_command(&mut reader).await;
            assert_eq!(handshake.len(), 4);
            assert_eq!(&handshake[..3], &["CLIENT", "SETINFO", field]);
            write.write_all(b"+OK\r\n").await.unwrap();
        }
        assert_eq!(
            read_command(&mut reader).await,
            ["JSON.NUMINCRBY", "synthetic", &path, "2.0"]
        );
        write.write_all(response.as_bytes()).await.unwrap();
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
    });
    // Constructor may read default Config; explicit loopback selection bypasses
    // profile lookup. No credentials or real deployments are selected or contacted.
    let state = support::database_state_write(format!("redis://{addr}"));
    let tool = redis::json_numincrby(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .expect("synthetic tool call must finish");
    drop(tool);
    drop(state);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("fixture must finish without further queries")
        .expect("fixture assertions must pass");
    result
}

fn bulk_reply(payload: &str) -> String {
    format!("${}\r\n{}\r\n", payload.len(), payload)
}

#[tokio::test]
async fn empty_jsonpath_reply_reports_no_increment_without_claiming_a_missing_key() {
    for payload in ["[]", " [ ] ", "\n[\t]\r\n"] {
        let result = call_tool(
            json!({"key": "synthetic", "path": "$.missing", "value": 2}),
            bulk_reply(payload),
        )
        .await;
        assert!(
            !result.is_error,
            "no-match reply is not a server error: {result:?}"
        );
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0].as_text(),
            Some("Nothing incremented: path '$.missing' matched no values")
        );
    }
}

#[tokio::test]
async fn nonempty_and_legacy_replies_preserve_original_text_and_argument_defaults() {
    for (path, payload) in [
        (Some("$..a"), " [null,4,7,null] "),
        (Some("$.n"), "[0]"),
        (Some("$.n"), "[null]"),
        (Some("$.n"), "null"),
        (Some("$.n"), "\"[]\""),
        (Some("$.n"), "["),
        (Some(".n"), "42"),
        (Some(".n"), "1.25e+3"),
        (Some(".n"), "[]"),
        (None, "[3]"),
    ] {
        let mut input = json!({"key": "synthetic", "value": 2});
        if let Some(path) = path {
            input["path"] = json!(path);
        }
        let result = call_tool(input, bulk_reply(payload)).await;
        assert!(!result.is_error, "preserved reply: {path:?}: {result:?}");
        assert_eq!(result.content.len(), 1);
        let expected = format!("New value at '{}': {}", path.unwrap_or("$"), payload);
        assert_eq!(result.content[0].as_text(), Some(expected.as_str()));
    }
}

#[tokio::test]
async fn server_errors_and_nil_reply_type_errors_remain_errors() {
    for response in ["-ERR synthetic JSON failure\r\n", "$-1\r\n"] {
        let result = call_tool(
            json!({"key": "synthetic", "path": "$.n", "value": 2}),
            response.to_owned(),
        )
        .await;
        assert!(
            result.is_error,
            "Redis/decode errors must remain errors: {result:?}"
        );
        assert_eq!(result.content.len(), 1);
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("JSON.NUMINCRBY failed")
        );
    }
}
