#![cfg(all(feature = "database", feature = "test-support"))]
//! Seed reporting through actual tools and disposable RESP pipeline fixtures.
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

async fn call_seed(input: Value, expected: Vec<Vec<String>>, refuse: bool) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        for field in ["LIB-NAME", "LIB-VER"] {
            let setup = read_command(&mut reader).await;
            assert_eq!(setup.len(), 4);
            assert_eq!(&setup[..3], &["CLIENT", "SETINFO", field]);
            write.write_all(b"+OK\r\n").await.unwrap();
        }
        for command in expected {
            assert_eq!(read_command(&mut reader).await, command);
            let reply: &[u8] = if refuse {
                b"-OOM fixture refusal\r\n"
            } else if matches!(command[0].as_str(), "SET" | "JSON.SET") {
                b"+OK\r\n"
            } else {
                b":1\r\n"
            };
            write.write_all(reply).await.unwrap();
        }
        let mut byte = [0];
        match reader.read(&mut byte).await {
            Ok(0) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("unexpected extra command or close error: {other:?}"),
        }
    });
    let state = support::database_state_write(format!("redis://{addr}"));
    let tool = redis::seed(state.clone());
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

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

fn reported(
    result: &CallToolResult,
    count: u64,
    kind: &str,
    commands: usize,
    constant: bool,
) -> bool {
    assert!(!result.is_error, "{result:?}");
    assert_eq!(result.content.len(), 1);
    let text = result.content[0].as_text().unwrap();
    text.starts_with(&format!(
        "Seed complete: {count} {kind} iterations completed ({commands} commands)"
    )) && !text.contains("items seeded")
        && text.contains("key_pattern does not vary") == constant
        && text.contains("Tip: use redis_info")
}

#[tokio::test]
async fn constant_string_key_reports_iterations_and_preserves_batches_and_ttl() {
    let result = call_seed(
        json!({
            "data_type":"STRING", "key_pattern":"fixed", "value_pattern":"v{i}",
            "count":"3", "batch_size":"2", "ttl":"5"
        }),
        vec![
            args(&["SET", "fixed", "v0"]),
            args(&["EXPIRE", "fixed", "5"]),
            args(&["SET", "fixed", "v1"]),
            args(&["EXPIRE", "fixed", "5"]),
            args(&["SET", "fixed", "v2"]),
            args(&["EXPIRE", "fixed", "5"]),
        ],
        false,
    )
    .await;
    assert!(reported(&result, 3, "string", 6, true), "{result:?}");
}

#[tokio::test]
async fn indexed_zero_and_single_iteration_cases_do_not_warn() {
    let mut outcomes = Vec::new();
    for (pattern, keys) in [
        ("k:{i}", ["k:0", "k:1"]),
        ("k:{i:04}", ["k:0000", "k:0001"]),
    ] {
        let result = call_seed(
            json!({"data_type":"string", "key_pattern":pattern,
            "value_pattern":"v{i}", "count":2, "batch_size":1}),
            vec![args(&["SET", keys[0], "v0"]), args(&["SET", keys[1], "v1"])],
            false,
        )
        .await;
        outcomes.push(reported(&result, 2, "string", 2, false));
    }
    for count in [0, 1] {
        let expected = if count == 0 {
            vec![]
        } else {
            vec![args(&["SET", "fixed", "v0"])]
        };
        let result = call_seed(
            json!({"data_type":"string", "key_pattern":"fixed",
            "value_pattern":"v{i}", "count":count}),
            expected,
            false,
        )
        .await;
        outcomes.push(reported(&result, count, "string", count as usize, false));
    }
    assert!(
        outcomes.iter().all(|v| *v),
        "incorrect reporting: {outcomes:?}"
    );
}

#[tokio::test]
async fn constant_keys_stay_supported_for_all_other_types() {
    let mut outcomes = Vec::new();
    for (kind, extra, expected) in [
        (
            "hash",
            json!({"field_values":[{"name":"f", "value":"v{i}"}]}),
            vec![
                args(&["HSET", "fixed", "f", "v0"]),
                args(&["HSET", "fixed", "f", "v1"]),
            ],
        ),
        (
            "json",
            json!({"value_pattern":"{\"n\":{i}}"}),
            vec![
                args(&["JSON.SET", "fixed", "$", "{\"n\":0}"]),
                args(&["JSON.SET", "fixed", "$", "{\"n\":1}"]),
            ],
        ),
        (
            "list",
            json!({"member_pattern":"m{i}"}),
            vec![
                args(&["RPUSH", "fixed", "m0"]),
                args(&["RPUSH", "fixed", "m1"]),
            ],
        ),
        (
            "set",
            json!({"member_pattern":"m{i}"}),
            vec![
                args(&["SADD", "fixed", "m0"]),
                args(&["SADD", "fixed", "m1"]),
            ],
        ),
        (
            "sorted_set",
            json!({"member_pattern":"m{i}", "score_min":0, "score_max":1}),
            vec![
                args(&["ZADD", "fixed", "0.0", "m0"]),
                args(&["ZADD", "fixed", "1.0", "m1"]),
            ],
        ),
    ] {
        let mut input = json!({"data_type":kind,"key_pattern":"fixed","count":2});
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = call_seed(input, expected, false).await;
        outcomes.push((kind, reported(&result, 2, kind, 2, true)));
    }
    assert!(
        outcomes.iter().all(|(_, ok)| *ok),
        "incorrect reporting: {outcomes:?}"
    );
}

#[tokio::test]
async fn server_and_validation_errors_do_not_report_completion() {
    for (input, expected, refuse, message) in [
        (
            json!({"data_type":"string","key_pattern":"fixed","count":1,"value_pattern":"v"}),
            vec![args(&["SET", "fixed", "v"])],
            true,
            "Seed pipeline failed",
        ),
        (
            json!({"data_type":"other","key_pattern":"fixed","count":1}),
            vec![],
            false,
            "Invalid data_type",
        ),
    ] {
        let result = call_seed(input, expected, refuse).await;
        assert!(result.is_error, "{result:?}");
        let text = result.content[0].as_text().unwrap();
        assert!(text.contains(message), "{text}");
        assert!(!text.contains("Seed complete"));
    }
}
