#![cfg(all(feature = "database", feature = "test-support"))]
//! Actual-tool compatibility and interpretation guidance through bounded RESP fixtures.
mod support;

use std::sync::Arc;
use std::time::Duration;

use redisctl_mcp::{AppState, tools::redis};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::{CallToolResult, Tool};

const NOTE: &str = "\n\nMemory interpretation: A high fragmentation ratio alone does not establish a memory problem. Consider absolute overhead in bytes, allocator metrics and prior peak usage, especially when current allocation is small.";

fn bulk(value: &str) -> String {
    format!("${}\r\n{value}\r\n", value.len())
}
fn text(result: &CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0].as_text().expect("tool must return text")
}
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
async fn run(
    make: fn(Arc<AppState>) -> Tool,
    resp3: bool,
    steps: Vec<(&[&str], String)>,
) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let steps: Vec<_> = steps
        .into_iter()
        .map(|(args, reply)| {
            (
                args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                reply,
            )
        })
        .collect();
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
        for (expected, reply) in steps {
            assert_eq!(command(&mut reader).await, expected);
            write.write_all(reply.as_bytes()).await.unwrap();
        }
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
    let state = support::database_state_readonly(format!("redis://{addr}/?protocol={protocol}"));
    let tool = make(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(json!({})))
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

fn check_preserved_output(
    result: &CallToolResult,
    prefix: &str,
    label: &str,
    missing: &mut Vec<String>,
) {
    assert!(!result.is_error, "{label}: {result:?}");
    let output = text(result);
    if output == prefix {
        missing.push(label.to_owned());
    } else {
        assert_eq!(
            output,
            format!("{prefix}{NOTE}"),
            "changed reported values/raw prefix or guidance for {label}"
        );
    }
    eprintln!("preserved {label}: {output:?}");
}

const BASIC_INFO: &str = "redis_version:8.6.7\r\nuptime_in_seconds:3600\r\nuptime_in_days:0\r\ninstantaneous_ops_per_sec:12\r\ntotal_commands_processed:40\r\nconnected_clients:2\r\n";
fn health_prefix(used: &str, max: &str, ratio: &str) -> String {
    format!(
        "Redis Health Check\n==================\n\nConnectivity: PONG\nVersion: 8.6.7\nUptime: 3600 seconds (0 days)\n\nMemory:\n- Used: {used}\n- Max: {max}\n- Fragmentation ratio: {ratio}\n\nStats:\n- Ops/sec: 12\n- Total commands processed: 40\n- Connected clients: 2\n\nKeys: 7"
    )
}

#[tokio::test]
async fn health_values_and_missing_malformed_fallbacks_are_preserved() {
    let cases=[
        ("high-ratio-small-allocation",format!("{BASIC_INFO}used_memory:1048576\r\nused_memory_human:1.00M\r\nmaxmemory:0\r\nmem_fragmentation_ratio:16.0\r\nmem_fragmentation_bytes:15728640\r\nallocator_frag_ratio:1.1\r\nused_memory_peak:4000000\r\n"),health_prefix("1.00M","unlimited","16.0")),
        ("high-ratio-large-allocation",format!("{BASIC_INFO}used_memory:1073741824\r\nused_memory_human:1.00G\r\nmaxmemory:0\r\nmem_fragmentation_ratio:16.0\r\nmem_fragmentation_bytes:16106127360\r\nallocator_frag_ratio:1.1\r\nused_memory_peak:2000000000\r\n"),health_prefix("1.00G","unlimited","16.0")),
        ("ordinary-ratio",format!("{BASIC_INFO}used_memory_human:96.00M\r\nmaxmemory:1073741824\r\nmaxmemory_human:1.00G\r\nmem_fragmentation_ratio:1.05\r\n"),health_prefix("96.00M","1.00G","1.05")),
        ("missing-fields","# Memory\r\nmalformed-line\r\n".to_owned(),"Redis Health Check\n==================\n\nConnectivity: PONG\nVersion: unknown\nUptime: unknown seconds (unknown days)\n\nMemory:\n- Used: unknown\n- Max: unlimited\n- Fragmentation ratio: unknown\n\nStats:\n- Ops/sec: unknown\n- Total commands processed: unknown\n- Connected clients: unknown\n\nKeys: 7".to_owned()),
        ("malformed-optional-values",format!("{BASIC_INFO}used_memory:not-a-number\r\nused_memory_human:unparsed value\r\nmaxmemory:not-a-number\r\nmaxmemory_human:odd cap\r\nmem_fragmentation_ratio:not-a-ratio\r\nmem_fragmentation_bytes:NaN\r\nused_memory_peak:invalid\r\n"),health_prefix("unparsed value","odd cap","not-a-ratio")),
    ];
    let mut missing = Vec::new();
    for resp3 in [false, true] {
        for (label, info, prefix) in &cases {
            let result = run(
                redis::health_check,
                resp3,
                vec![
                    (&["PING"], "+PONG\r\n".to_owned()),
                    (&["INFO"], bulk(info)),
                    (&["DBSIZE"], ":7\r\n".to_owned()),
                ],
            )
            .await;
            check_preserved_output(
                &result,
                prefix,
                &format!("health resp3={resp3} {label}"),
                &mut missing,
            );
        }
    }
    assert!(
        missing.is_empty(),
        "missing general memory guidance: {missing:?}"
    );
}

#[tokio::test]
async fn memory_stats_resp2_arrays_keep_nested_and_unknown_metrics() {
    let reply = format!(
        "*8\r\n{}:99999\r\n{}{}{}*2\r\n{}*2\r\n{}:7\r\n{}$-1\r\n",
        bulk("peak.allocated"),
        bulk("fragmentation"),
        bulk("16.0"),
        bulk("db.0"),
        bulk("future.metric"),
        bulk("alpha"),
        bulk("unknown")
    );
    let mut missing = Vec::new();
    for (label, reply, prefix) in [
        (
            "nested-unknown",
            reply,
            "[peak.allocated, 99999, fragmentation, 16.0, db.0, [future.metric, [alpha, 7]], unknown, (nil)]",
        ),
        ("empty", "*0\r\n".to_owned(), "[]"),
    ] {
        let result = run(
            redis::memory_stats,
            false,
            vec![(&["MEMORY", "STATS"], reply)],
        )
        .await;
        check_preserved_output(
            &result,
            prefix,
            &format!("memory RESP2 {label}"),
            &mut missing,
        );
    }
    assert!(
        missing.is_empty(),
        "missing general memory guidance: {missing:?}"
    );
}

#[tokio::test]
async fn memory_stats_resp3_maps_keep_native_nested_and_unknown_metrics() {
    let reply = format!(
        "%3\r\n{},16\r\n{}%1\r\n{}*2\r\n{}:7\r\n{}_\r\n",
        bulk("fragmentation"),
        bulk("db.0"),
        bulk("future.metric"),
        bulk("alpha"),
        bulk("unknown")
    );
    let raw = r#"map([(bulk-string('"fragmentation"'), double(16.0)), (bulk-string('"db.0"'), map([(bulk-string('"future.metric"'), array([bulk-string('"alpha"'), int(7)]))])), (bulk-string('"unknown"'), nil)])"#;
    let mut missing = Vec::new();
    for (label, reply, prefix) in [
        ("nested-unknown", reply, raw),
        ("empty", "%0\r\n".to_owned(), "map([])"),
    ] {
        let result = run(
            redis::memory_stats,
            true,
            vec![(&["MEMORY", "STATS"], reply)],
        )
        .await;
        check_preserved_output(
            &result,
            prefix,
            &format!("memory RESP3 {label}"),
            &mut missing,
        );
    }
    assert!(
        missing.is_empty(),
        "missing general memory guidance: {missing:?}"
    );
}

#[tokio::test]
async fn command_server_and_decode_error_boundaries_remain_errors() {
    for resp3 in [false, true] {
        for (steps, context) in [
            (
                vec![(&["PING"][..], "-ERR fixture failure\r\n".to_owned())],
                "PING failed",
            ),
            (
                vec![
                    (&["PING"][..], "+PONG\r\n".to_owned()),
                    (&["INFO"][..], "-ERR fixture failure\r\n".to_owned()),
                ],
                "INFO failed",
            ),
            (
                vec![
                    (&["PING"][..], "+PONG\r\n".to_owned()),
                    (&["INFO"][..], bulk(BASIC_INFO)),
                    (&["DBSIZE"][..], "-ERR fixture failure\r\n".to_owned()),
                ],
                "DBSIZE failed",
            ),
            (
                vec![
                    (&["PING"][..], "+PONG\r\n".to_owned()),
                    (&["INFO"][..], "*0\r\n".to_owned()),
                ],
                "INFO failed",
            ),
        ] {
            let result = run(redis::health_check, resp3, steps).await;
            assert!(result.is_error, "{result:?}");
            assert!(text(&result).contains(context), "{result:?}");
            assert!(!text(&result).contains("Memory interpretation:"));
        }
        for reply in ["-ERR fixture failure\r\n", "?invalid\r\n"] {
            let result = run(
                redis::memory_stats,
                resp3,
                vec![(&["MEMORY", "STATS"], reply.to_owned())],
            )
            .await;
            assert!(result.is_error, "{result:?}");
            assert!(text(&result).contains("MEMORY STATS failed"), "{result:?}");
            assert!(!text(&result).contains("Memory interpretation:"));
        }
    }
}
