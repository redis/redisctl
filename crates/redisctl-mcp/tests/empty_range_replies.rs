#![cfg(feature = "database")]
//! Actual read-only range tools against disposable, credential-free RESP2 fixtures.

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

async fn call_tool(name: &str, input: Value, reply: &'static str) -> (CallToolResult, Vec<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        for field in ["LIB-NAME", "LIB-VER"] {
            let handshake = read_command(&mut reader).await.unwrap();
            assert_eq!(handshake.len(), 4);
            assert_eq!(&handshake[..3], &["CLIENT", "SETINFO", field]);
            write.write_all(b"+OK\r\n").await.unwrap();
        }
        let command = read_command(&mut reader).await.unwrap();
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
        command
    });

    // Construction may read default Config; explicit loopback selection bypasses
    // profile target lookup and never accesses a configured credential-bearing target.
    let state = support::database_state_readonly(format!("redis://{addr}"));
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        match name {
            "LRANGE" => redis::lrange(state.clone()).call(input).await,
            "XRANGE" => redis::xrange(state.clone()).call(input).await,
            other => panic!("unsupported synthetic tool: {other}"),
        }
    })
    .await
    .expect("synthetic tool call must finish");
    drop(state);
    let command = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("connection must close without further queries")
        .expect("synthetic server assertions must pass");
    (result, command)
}

fn assert_text(result: &CallToolResult, expected: &str) {
    assert!(!result.is_error, "unexpected tool error: {result:?}");
    assert_eq!(result.content.len(), 1);
    assert_eq!(result.content[0].as_text(), Some(expected));
}

#[tokio::test]
async fn empty_list_ranges_report_returned_results_without_guessing_key_state() {
    for (arguments, start, stop) in [
        (json!({"start": 5, "stop": 1}), 5, 1),
        (json!({"start": 100, "stop": 200}), 100, 200),
        (json!({}), 0, -1),
        (json!({"start": "5", "stop": "1"}), 5, 1),
    ] {
        let mut input = json!({"key": "synthetic"});
        input
            .as_object_mut()
            .unwrap()
            .extend(arguments.as_object().unwrap().clone());
        let (result, command) = call_tool("LRANGE", input, "*0\r\n").await;
        assert_text(
            &result,
            &format!("No list elements returned for 'synthetic' in range [{start}, {stop}]"),
        );
        assert_eq!(
            command,
            vec![
                "LRANGE".to_string(),
                "synthetic".to_string(),
                start.to_string(),
                stop.to_string()
            ]
        );
    }
}

#[tokio::test]
async fn empty_stream_ranges_report_returned_results_without_guessing_key_state() {
    for (arguments, start, end, suffix) in [
        (json!({"start": "+", "end": "-"}), "+", "-", vec![]),
        (
            json!({"start": "20-0", "end": "30-0"}),
            "20-0",
            "30-0",
            vec![],
        ),
        (json!({}), "-", "+", vec![]),
        (json!({"count": "0"}), "-", "+", vec!["COUNT", "0"]),
    ] {
        let mut input = json!({"key": "synthetic"});
        input
            .as_object_mut()
            .unwrap()
            .extend(arguments.as_object().unwrap().clone());
        let (result, command) = call_tool("XRANGE", input, "*0\r\n").await;
        assert_text(
            &result,
            &format!("No stream entries returned for 'synthetic' in range [{start}, {end}]"),
        );
        let mut expected = vec!["XRANGE", "synthetic", start, end];
        expected.extend(suffix);
        assert_eq!(command, expected);
    }
}

#[tokio::test]
async fn nonempty_and_fallback_replies_preserve_arguments_and_output() {
    for (name, input, reply, expected, command) in [
        (
            "LRANGE",
            json!({"key":"synthetic","start":"0","stop":"1"}),
            "*2\r\n$1\r\na\r\n$1\r\nb\r\n",
            "List 'synthetic' (2 elements):\n0: a\n1: b",
            vec!["LRANGE", "synthetic", "0", "1"],
        ),
        (
            "XRANGE",
            json!({"key":"synthetic","count":"1"}),
            "*1\r\n*2\r\n$3\r\n1-0\r\n*2\r\n$1\r\nf\r\n$1\r\nv\r\n",
            "Stream 'synthetic' (1 entries):\n[1-0, [f, v]]\n",
            vec!["XRANGE", "synthetic", "-", "+", "COUNT", "1"],
        ),
        (
            "XRANGE",
            json!({"key":"synthetic"}),
            "$-1\r\n",
            "(nil)",
            vec!["XRANGE", "synthetic", "-", "+"],
        ),
    ] {
        let (result, actual) = call_tool(name, input, reply).await;
        assert_text(&result, expected);
        assert_eq!(actual, command);
    }
}

#[tokio::test]
async fn server_errors_are_still_errors_without_followup_queries() {
    for name in ["LRANGE", "XRANGE"] {
        let (result, command) = call_tool(
            name,
            json!({"key":"synthetic"}),
            "-WRONGTYPE synthetic wrong kind\r\n",
        )
        .await;
        assert!(
            result.is_error,
            "server failure must remain an error: {result:?}"
        );
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("synthetic wrong kind")
        );
        let expected = if name == "LRANGE" {
            vec!["LRANGE", "synthetic", "0", "-1"]
        } else {
            vec!["XRANGE", "synthetic", "-", "+"]
        };
        assert_eq!(command, expected);
    }
}
