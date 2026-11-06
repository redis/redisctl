#![cfg(all(feature = "database", feature = "test-support"))]
//! Actual-tool reply handling against disposable RESP2/RESP3 fixtures.

mod support;

use std::time::Duration;

use redisctl_mcp::tools::redis;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::CallToolResult;

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

async fn call_tool(resp3: bool, path: Option<&str>, reply: &str) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected_path = path.unwrap_or("$").to_string();
    let response = reply.to_string();
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
        assert_eq!(
            command(&mut reader).await,
            ["JSON.TOGGLE", "synthetic", &expected_path]
        );
        write.write_all(response.as_bytes()).await.unwrap();
        let mut byte = [0];
        match reader.read(&mut byte).await {
            Ok(0) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("unexpected follow-up command or close error: {other:?}"),
        }
    });
    let protocol = if resp3 { "resp3" } else { "resp2" };
    let state = support::database_state_write(format!("redis://{addr}/?protocol={protocol}"));
    let tool = redis::json_toggle(state.clone());
    let mut input = json!({"key": "synthetic"});
    if let Some(path) = path {
        input["path"] = json!(path);
    }
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .unwrap();
    drop(tool);
    drop(state);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    result
}

#[tokio::test]
async fn empty_and_null_only_replies_do_not_claim_a_toggle() {
    let mut mismatches = Vec::new();
    for resp3 in [false, true] {
        let nil = if resp3 { "_\r\n" } else { "$-1\r\n" };
        for (path, reply) in [
            (None, "*0\r\n".to_string()),
            (Some("$.n"), format!("*1\r\n{nil}")),
            (Some("$.*"), format!("*2\r\n{nil}{nil}")),
            (Some(".n"), nil.to_string()),
        ] {
            let result = call_tool(resp3, path, &reply).await;
            assert!(!result.is_error, "no-op is not a server error: {result:?}");
            assert_eq!(result.content.len(), 1);
            let expected = format!(
                "Nothing toggled: no boolean values matched 'synthetic' at '{}'",
                path.unwrap_or("$")
            );
            if result.content[0].as_text() != Some(expected.as_str()) {
                mismatches.push((resp3, path, result.content[0].as_text().map(str::to_owned)));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "incorrect no-op output: {mismatches:?}"
    );
}

#[tokio::test]
async fn successful_zero_one_and_mixed_results_keep_existing_text() {
    for resp3 in [false, true] {
        let nil = if resp3 { "_\r\n" } else { "$-1\r\n" };
        for (path, reply, expected) in [
            (".n", ":0\r\n".to_string(), "0"),
            (".n", ":1\r\n".to_string(), "1"),
            ("$.n", "*1\r\n:0\r\n".to_string(), "[0]"),
            ("$.n", "*1\r\n:1\r\n".to_string(), "[1]"),
            ("$.*", format!("*3\r\n:0\r\n{nil}:1\r\n"), "[0, (nil), 1]"),
        ] {
            let result = call_tool(resp3, Some(path), &reply).await;
            assert!(!result.is_error, "{result:?}");
            let text = format!("Toggled 'synthetic' at '{path}': {expected}");
            assert_eq!(result.content[0].as_text(), Some(text.as_str()));
        }
    }
}

#[tokio::test]
async fn server_errors_stay_errors() {
    for resp3 in [false, true] {
        let result = call_tool(resp3, Some("$.n"), "-WRONGTYPE fixture refusal\r\n").await;
        assert!(result.is_error, "{result:?}");
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("JSON.TOGGLE failed")
        );
    }
}
