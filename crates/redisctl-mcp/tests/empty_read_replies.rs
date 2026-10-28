#![cfg(feature = "database")]
//! Synthetic RESP-to-MCP regressions with explicit credential-free loopback URLs.

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

async fn assert_reply(
    make_tool: impl FnOnce(std::sync::Arc<redisctl_mcp::AppState>) -> Tool,
    input: Value,
    command: &[&str],
    reply: &str,
    expected: &str,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected_command: Vec<String> = command.iter().map(|arg| (*arg).to_string()).collect();
    let response = format!("${}\r\n{}\r\n", reply.len(), reply);
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
        let actual = read_command(&mut reader).await;
        assert_eq!(actual, expected_command);
        write.write_all(response.as_bytes()).await.unwrap();
        // Detect extra data/metadata commands rather than silently accepting them.
        let mut byte = [0];
        match reader.read(&mut byte).await {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("unexpected command or close error after reply: {other:?}"),
        }
    });

    let state = support::database_state_readonly(format!("redis://{addr}"));
    let tool = make_tool(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .expect("synthetic tool call must complete");
    assert!(!result.is_error, "unexpected tool error: {result:?}");
    assert_eq!(result.content.len(), 1);
    assert_eq!(result.content[0].as_text(), Some(expected));
    drop(tool);
    drop(state);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("synthetic server must finish without extra commands")
        .expect("synthetic server assertions must pass");
}

#[tokio::test]
async fn getrange_empty_reply_is_explicit_without_missing_key_claim() {
    assert_reply(
        redis::getrange,
        json!({"key": "synthetic", "start": 0, "end": -1}),
        &["GETRANGE", "synthetic", "0", "-1"],
        "",
        "(empty string)",
    )
    .await;
}

#[tokio::test]
async fn getrange_nonempty_reply_is_preserved_verbatim() {
    for reply in [
        "abc",
        " \t\r\n",
        "café🙂\n",
        "(empty string)",
        "a\0b",
        "\u{a0}",
    ] {
        assert_reply(
            redis::getrange,
            json!({"key": "synthetic", "start": "1", "end": "9"}),
            &["GETRANGE", "synthetic", "1", "9"],
            reply,
            reply,
        )
        .await;
    }
}

#[tokio::test]
async fn info_empty_reply_is_neutral_with_and_without_section() {
    for (input, command) in [
        (json!({}), vec!["INFO"]),
        (json!({"section": "synthetic"}), vec!["INFO", "synthetic"]),
    ] {
        assert_reply(redis::info, input, &command, "", "(no INFO data returned)").await;
    }
}

#[tokio::test]
async fn info_nonempty_reply_is_preserved_verbatim() {
    for reply in ["# Server\r\nredis_version:synthetic\r\n", " \r\n", "é🙂\n"] {
        assert_reply(
            redis::info,
            json!({"section": "server"}),
            &["INFO", "server"],
            reply,
            reply,
        )
        .await;
    }
}
