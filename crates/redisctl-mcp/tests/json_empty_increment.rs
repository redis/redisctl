#![cfg(feature = "database")]
//! Synthetic RESP2/RESP3 replies through actual tools and credential-free loopback targets.

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
    call_tool_protocol(false, input, response).await
}

async fn call_tool_protocol(resp3: bool, input: Value, response: String) -> CallToolResult {
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
        if resp3 {
            assert_eq!(read_command(&mut reader).await, ["HELLO", "3"]);
            write.write_all(b"%1\r\n+proto\r\n:3\r\n").await.unwrap();
        }
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
    let protocol = if resp3 { "resp3" } else { "resp2" };
    let state = support::database_state_write(format!("redis://{addr}/?protocol={protocol}"));
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
async fn resp3_native_empty_and_null_only_arrays_report_no_increment() {
    for (path, response, expected) in [
        (
            "$.missing",
            "*0\r\n",
            "Nothing incremented: path '$.missing' matched no values",
        ),
        (
            "$.n",
            "*1\r\n_\r\n",
            "Nothing incremented: path '$.n' matched no numeric values",
        ),
        (
            "$..n",
            "*2\r\n_\r\n_\r\n",
            "Nothing incremented: path '$..n' matched no numeric values",
        ),
        (
            ".missing",
            "*0\r\n",
            "Nothing incremented: path '.missing' matched no values",
        ),
        (
            ".n",
            "*2\r\n_\r\n_\r\n",
            "Nothing incremented: path '.n' matched no numeric values",
        ),
    ] {
        let result = call_tool_protocol(
            true,
            json!({"key":"synthetic","path":path,"value":2}),
            response.to_owned(),
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].as_text(), Some(expected));
    }
}

#[tokio::test]
async fn resp3_native_numeric_and_mixed_arrays_preserve_numeric_success() {
    for path in ["$.n", ".n"] {
        for (response, expected) in [
            ("*1\r\n,0\r\n", "[0.0]"),
            ("*1\r\n:0\r\n", "[0]"),
            ("*1\r\n,1.25\r\n", "[1.25]"),
            ("*1\r\n,-0\r\n", "[-0.0]"),
            ("*1\r\n:9223372036854775807\r\n", "[9223372036854775807]"),
            ("*4\r\n_\r\n,5\r\n:0\r\n_\r\n", "[null,5.0,0,null]"),
        ] {
            let result = call_tool_protocol(
                true,
                json!({"key":"synthetic","path":path,"value":2}),
                response.to_owned(),
            )
            .await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(result.content.len(), 1);
            assert_eq!(
                result.content[0].as_text(),
                Some(format!("New value at '{path}': {expected}").as_str())
            );
        }
    }
}

#[tokio::test]
async fn resp3_server_decode_and_invalid_member_errors_remain_errors() {
    for response in [
        "-ERR synthetic JSON failure\r\n",
        "_\r\n",
        "*1\r\n?invalid\r\n",
        "*1\r\n#t\r\n",
        "*1\r\n+unexpected\r\n",
        "*1\r\n$1\r\n5\r\n",
        "*1\r\n*0\r\n",
        "*1\r\n%0\r\n",
        "*1\r\n-ERR nested JSON failure\r\n",
        "*1\r\n~0\r\n",
        "*1\r\n(123456789012345678901234567890\r\n",
        "*1\r\n,inf\r\n",
        "*1\r\n,-inf\r\n",
        "*1\r\n,nan\r\n",
    ] {
        let result = call_tool_protocol(
            true,
            json!({"key":"synthetic","path":"$.n","value":2}),
            response.to_owned(),
        )
        .await;
        assert!(result.is_error, "{response:?}: {result:?}");
        assert_eq!(result.content.len(), 1);
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("JSON.NUMINCRBY failed"),
            "{result:?}"
        );
    }
}

#[tokio::test]
async fn resp3_bulk_strings_and_legacy_scalars_keep_existing_rendering() {
    for (path, response, expected) in [
        ("$.n", bulk_reply(" [null,0,1.25] "), " [null,0,1.25] "),
        (".n", ",1.25\r\n".to_owned(), "1.25"),
        (".n", ":0\r\n".to_owned(), "0"),
        (".n", bulk_reply("1.25e+3"), "1.25e+3"),
    ] {
        let result = call_tool_protocol(
            true,
            json!({"key":"synthetic","path":path,"value":2}),
            response,
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            result.content[0].as_text(),
            Some(format!("New value at '{path}': {expected}").as_str())
        );
    }
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
async fn null_only_jsonpath_replies_report_no_numeric_matches() {
    let mut mismatches = Vec::new();
    for path in [Some("$.n"), Some("$..n"), None] {
        for payload in ["[null]", "[null,null]", " \n[ null , null ]\t"] {
            let mut input = json!({"key": "synthetic", "value": 2});
            if let Some(path) = path {
                input["path"] = json!(path);
            }
            let result = call_tool(input, bulk_reply(payload)).await;
            assert!(
                !result.is_error,
                "non-numeric matches are not server errors: {result:?}"
            );
            assert_eq!(result.content.len(), 1);
            let expected = format!(
                "Nothing incremented: path '{}' matched no numeric values",
                path.unwrap_or("$")
            );
            if result.content[0].as_text() != Some(expected.as_str()) {
                mismatches.push((
                    path,
                    payload,
                    result.content[0].as_text().map(str::to_owned),
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "incorrect no-op output: {mismatches:?}"
    );
}

#[tokio::test]
async fn nonempty_and_legacy_replies_preserve_original_text_and_argument_defaults() {
    for (path, payload) in [
        (Some("$..a"), " [null,4,7,null] "),
        (Some("$.n"), "[0]"),
        (Some("$.n"), "[null,0]"),
        (Some("$.n"), "null"),
        (Some("$.n"), "\"[]\""),
        (Some("$.n"), "\"[null]\""),
        (Some("$.n"), "["),
        (Some(".n"), "42"),
        (Some(".n"), "1.25e+3"),
        (Some(".n"), "[]"),
        (Some(".n"), "[null]"),
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
