#![cfg(feature = "database")]
//! Actual tool calls against disposable, credential-free RESP2 loopback fixtures.

mod support;

use std::time::Duration;

use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::CallToolResult;

async fn read_command(reader: &mut BufReader<OwnedReadHalf>) -> Option<Vec<String>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await.unwrap() == 0 {
        return None;
    }
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
    Some(command)
}

async fn call_tool(input: Value, reply: &'static str) -> (CallToolResult, Option<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        // Preserve the macro's connection acquisition: validation follows RESP2 setup.
        for field in ["LIB-NAME", "LIB-VER"] {
            let handshake = read_command(&mut reader).await.unwrap();
            assert_eq!(handshake.len(), 4);
            assert_eq!(&handshake[..3], &["CLIENT", "SETINFO", field]);
            write.write_all(b"+OK\r\n").await.unwrap();
        }
        let command = read_command(&mut reader).await;
        if command.is_some() {
            // Also answer buggy baseline queries, so the negative control is an
            // observable false-success response, not a fixture/connection timeout.
            write.write_all(reply.as_bytes()).await.unwrap();
            let mut byte = [0];
            match reader.read(&mut byte).await {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) => {}
                other => panic!("unexpected extra command or close error: {other:?}"),
            }
        }
        command
    });

    // AppState construction may read the default Config; explicit loopback URL
    // selection bypasses profile lookup and never uses credential-bearing targets.
    let state = support::database_state_readonly(format!("redis://{addr}"));
    let tool = redis::zrangebyscore(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .expect("synthetic tool call must finish");
    drop(tool);
    drop(state);
    let command = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("connection must close without further queries")
        .expect("synthetic server assertions must pass");
    (result, command)
}

#[tokio::test]
async fn incomplete_pagination_is_an_actionable_tool_error_without_a_data_command() {
    for pagination in [
        json!({"count": 1}),
        json!({"offset": 2}),
        json!({"count": "1", "withscores": true}),
        json!({"offset": "0"}),
        json!({"count": 1, "offset": null}),
        json!({"offset": 0, "count": null}),
    ] {
        let mut input = json!({"key": "synthetic", "min": "-inf", "max": "+inf"});
        input
            .as_object_mut()
            .unwrap()
            .extend(pagination.as_object().unwrap().clone());
        let (result, command) = call_tool(input, "*0\r\n").await;
        assert!(
            result.is_error,
            "incomplete pair must not succeed: {pagination:?}: {result:?}"
        );
        assert_eq!(result.content.len(), 1);
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("offset and count must be provided together")
        );
        assert_eq!(command, None, "invalid pagination must not reach Redis");
    }
}

#[tokio::test]
async fn omitted_and_complete_pairs_preserve_wire_arguments_and_output() {
    for (pagination, suffix, reply, expected) in [
        (
            json!({}),
            vec![],
            "*2\r\n$1\r\na\r\n$1\r\nb\r\n",
            "'synthetic' (2 members in [-inf, +inf]):\na\nb",
        ),
        (
            json!({"offset": "0", "count": "1", "withscores": true}),
            vec!["WITHSCORES", "LIMIT", "0", "1"],
            "*2\r\n$1\r\na\r\n$1\r\n2\r\n",
            "'synthetic' (1 members in [-inf, +inf]):\na (score: 2)",
        ),
        (
            json!({"offset": 2, "count": 0}),
            vec!["LIMIT", "2", "0"],
            "*0\r\n",
            "No members in 'synthetic' with scores in [-inf, +inf]",
        ),
        (
            json!({"offset": 0, "count": -1}),
            vec!["LIMIT", "0", "-1"],
            "*2\r\n$1\r\na\r\n$1\r\nb\r\n",
            "'synthetic' (2 members in [-inf, +inf]):\na\nb",
        ),
    ] {
        let mut input = json!({"key": "synthetic", "min": "-inf", "max": "+inf"});
        input
            .as_object_mut()
            .unwrap()
            .extend(pagination.as_object().unwrap().clone());
        let (result, actual) = call_tool(input, reply).await;
        assert!(
            !result.is_error,
            "valid pagination: {pagination:?}: {result:?}"
        );
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].as_text(), Some(expected));
        let mut command = vec!["ZRANGEBYSCORE", "synthetic", "-inf", "+inf"];
        command.extend(suffix);
        assert_eq!(actual.unwrap(), command);
    }
}
