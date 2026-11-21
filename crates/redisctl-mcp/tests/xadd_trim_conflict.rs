#![cfg(feature = "database")]
//! XADD trimming-strategy validation against a disposable RESP2 loopback fixture.

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
            // Answer whatever arrives, so a missing trim check shows up as a
            // false success rather than a timeout.
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

    let state = support::database_state_write(format!("redis://{addr}"));
    let tool = redis::xadd(state.clone());
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
async fn maxlen_with_minid_is_a_tool_error_without_a_data_command() {
    for trim in [
        json!({"maxlen": 10, "minid": "1-0"}),
        json!({"maxlen": "10", "minid": "1-0", "approximate": true}),
    ] {
        let mut input = json!({"key": "synthetic", "fields": {"f": "v"}});
        input
            .as_object_mut()
            .unwrap()
            .extend(trim.as_object().unwrap().clone());
        let (result, command) = call_tool(input, "$3\r\n1-0\r\n").await;
        assert!(
            result.is_error,
            "maxlen with minid must not succeed: {trim:?}: {result:?}"
        );
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("maxlen and minid are mutually exclusive")
        );
        assert_eq!(command, None, "invalid trimming must not reach Redis");
    }
}

#[tokio::test]
async fn single_trim_strategy_still_reaches_redis() {
    let (result, command) = call_tool(
        json!({"key": "synthetic", "fields": {"f": "v"}, "minid": "5-0"}),
        "$3\r\n6-0\r\n",
    )
    .await;
    assert!(!result.is_error, "{result:?}");
    assert_eq!(
        command.unwrap(),
        ["XADD", "synthetic", "MINID", "5-0", "*", "f", "v"]
    );
}
