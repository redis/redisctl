#![cfg(all(feature = "database", feature = "test-support"))]
//! TAG query guidance through the actual tool, bounded RESP fixtures and pinned Redis.
mod support;

use std::process::Command;
use std::time::Duration;

use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::CallToolResult;

const HINT: &str = "\nHint: If this query uses TAG filters (@field:{value}), check escaping inside literal TAG values. For example, use @sku:{usb\\-c} in the query, or \"@sku:{usb\\\\-c}\" in a JSON string. Escape literal punctuation as needed for the Redis Search version/dialect; keep intentional operators such as | intact. Whitespace rules depend on version/dialect.";

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

/// None closes the connection after receiving FT.SEARCH, without a reply.
async fn synthetic(
    resp3: bool,
    input: Value,
    options: &[&str],
    reply: Option<&str>,
) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut expected = vec![
        "FT.SEARCH".to_owned(),
        input["index"].as_str().unwrap().to_owned(),
        input["query"].as_str().unwrap().to_owned(),
    ];
    expected.extend(options.iter().map(|arg| (*arg).to_owned()));
    let reply = reply.map(str::to_owned);
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
        assert_eq!(command(&mut reader).await, expected);
        if let Some(reply) = reply {
            write.write_all(reply.as_bytes()).await.unwrap();
            let mut byte = [0];
            match reader.read(&mut byte).await {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) => {}
                other => panic!("unexpected retry/schema lookup/extra command: {other:?}"),
            }
        }
    });
    let protocol = if resp3 { "resp3" } else { "resp2" };
    let state = support::database_state_readonly(format!("redis://{addr}/?protocol={protocol}"));
    let tool = redis::ft_search(state.clone());
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
async fn tag_syntax_errors_keep_original_error_and_append_conditional_guidance() {
    for resp3 in [false, true] {
        for query in [
            "@sku:{usb-c}",
            "@sku: {usb-c}",
            "@title:hello -@sku:{usb-c}",
            "@sku:{usb-c",
            "@sku:{usb-c} private-query-sentinel",
        ] {
            for error in [
                "Syntax error at offset 9 near usb",
                "ERR Syntax error at offset 9 near usb",
            ] {
                let result = synthetic(
                    resp3,
                    json!({"index":"idx", "query":query}),
                    &[],
                    Some(&format!("-{error}\r\n")),
                )
                .await;
                assert!(result.is_error, "{result:?}");
                assert_eq!(
                    text(&result),
                    format!("Tool error: FT.SEARCH failed: {error}{HINT}")
                );
                assert!(!text(&result).contains("private-query-sentinel"));
            }
        }
    }
}

#[tokio::test]
async fn other_server_errors_and_non_tag_syntax_errors_are_unchanged() {
    for resp3 in [false, true] {
        for error in [
            "Unknown index name",
            "No such index idx",
            "NOPERM this user has no permissions to run the FT.SEARCH command",
            "ERR unknown command 'FT.SEARCH'",
            "Timeout limit was reached",
            "No such parameter `value`",
            "ERR syntax error",
            "ERR invalid dialect",
        ] {
            let result = synthetic(
                resp3,
                json!({"index":"idx", "query":"@sku:{usb-c}"}),
                &[],
                Some(&format!("-{error}\r\n")),
            )
            .await;
            assert!(result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                format!("Tool error: FT.SEARCH failed: {error}")
            );
        }
        for query in [
            "hello (",
            "@price:[0 nope]",
            "(@title:hello)",
            "@title:hello => {$weight: nope}",
            r#""@sku:{usb-c}" ("#,
            r"\@sku:{usb-c}",
            r"@sku:\{usb-c\}",
            "@:{usb-c}",
        ] {
            let result = synthetic(
                resp3,
                json!({"index":"idx", "query":query}),
                &[],
                Some("-Syntax error at offset 0 near invalid\r\n"),
            )
            .await;
            assert!(result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                "Tool error: FT.SEARCH failed: Syntax error at offset 0 near invalid"
            );
        }
        let result = synthetic(
            resp3,
            json!({"index":"idx", "query":"@sku:{usb-c}"}),
            &[],
            Some("-WRONGTYPE Operation against a key holding the wrong kind of value\r\n"),
        )
        .await;
        assert!(result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            "Tool error: FT.SEARCH failed: WRONGTYPE Operation against a key holding the wrong kind of value\nHint: Check the affected key types with redis_type, if permitted, then use a command compatible with those types."
        );
    }
}

#[tokio::test]
async fn connection_failure_keeps_its_cause_without_tag_hint() {
    for resp3 in [false, true] {
        let result = synthetic(
            resp3,
            json!({"index":"idx", "query":"@sku:{usb-c}"}),
            &[],
            None,
        )
        .await;
        assert!(result.is_error, "{result:?}");
        assert!(
            text(&result).starts_with("Tool error: FT.SEARCH failed: "),
            "{result:?}"
        );
        let cause = text(&result).to_ascii_lowercase();
        assert!(
            ["broken pipe", "connection", "end of file"]
                .iter()
                .any(|message| cause.contains(message)),
            "{result:?}"
        );
        assert!(!text(&result).contains("Hint:"));
    }
}

#[tokio::test]
async fn valid_and_already_escaped_queries_keep_wire_arguments_and_success_output() {
    for resp3 in [false, true] {
        for query in [
            r"@sku:{usb\-c}",
            r"@sku:{a\|b}",
            r"@sku:{a\{b}",
            "@sku:{electronics|sports}",
            "@sku:{to be or not to be}",
            "*",
        ] {
            for (reply, output) in [
                ("*0\r\n", "No results"),
                ("*1\r\n:0\r\n", "Total results: 0\n\n"),
                (
                    "*3\r\n:1\r\n$5\r\ndoc:1\r\n*2\r\n$3\r\nsku\r\n$5\r\nusb-c\r\n",
                    "Total results: 1\n\n1. doc:1\n  sku: usb-c\n",
                ),
            ] {
                let result = synthetic(
                    resp3,
                    json!({"index":"idx", "query":query}),
                    &[],
                    Some(reply),
                )
                .await;
                assert!(!result.is_error, "{result:?}");
                assert_eq!(text(&result), output);
            }
        }
        // Parse actual JSON transport text to verify the two escaping layers.
        let input: Value = serde_json::from_str(
            r#"{"index":"idx","query":"@sku:{usb\\-c}","nocontent":true,"withscores":true,"verbatim":true,"sortby":"price","sortby_order":"DESC","limit_offset":"2","limit_num":"4","return_fields":["sku","price"]}"#,
        ).unwrap();
        let result = synthetic(
            resp3,
            input,
            &[
                "NOCONTENT",
                "VERBATIM",
                "WITHSCORES",
                "RETURN",
                "2",
                "sku",
                "price",
                "SORTBY",
                "price",
                "DESC",
                "LIMIT",
                "2",
                "4",
            ],
            Some("*3\r\n:1\r\n$5\r\ndoc:1\r\n$3\r\n0.5\r\n"),
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(text(&result), "Total results: 1\n\n1. doc:1 (score: 0.5)\n");
    }
}

#[test]
fn search_description_distinguishes_query_escaping_from_json_and_dialect_rules() {
    let state = support::database_state_readonly("redis://127.0.0.1:1".to_owned());
    let definition = redis::ft_search(state).definition();
    let description = definition.description.unwrap();
    assert!(description.contains(r"@sku:{usb\-c}"));
    assert!(description.contains(r#""@sku:{usb\\-c}""#));
    assert!(description.contains("intentional query operators"));
    assert!(description.contains("RediSearch 2.4+ with DIALECT 2+"));
    assert!(description.contains("unescaped spaces"));
    assert!(
        definition.input_schema["properties"]
            .get("dialect")
            .is_none()
    );
}

const IMAGE: &str =
    "redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8";

struct Fixture(String);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "--force", &self.0])
            .output();
    }
}

fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("Docker is required");
    assert!(output.status.success(), "Docker fixture command failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
#[ignore = "Requires Docker and pinned Redis 8.6.7 Search image"]
async fn live_tag_escaping_and_whitespace_follow_server_dialect() {
    let name = format!("redisctl-tag-guidance-1218-{}", std::process::id());
    let fixture = Fixture(docker(&[
        "run",
        "--detach",
        "--rm",
        "--pull=never",
        "--name",
        &name,
        "--publish",
        "127.0.0.1::6379",
        IMAGE,
    ]));
    let binding = docker(&["port", &fixture.0, "6379/tcp"]);
    let port: u16 = binding.strip_prefix("127.0.0.1:").unwrap().parse().unwrap();
    let url = format!("redis://127.0.0.1:{port}/?protocol=resp2");
    let client = ::redis::Client::open(url.clone()).unwrap();
    let mut conn = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await
                && ::redis::cmd("PING")
                    .query_async::<String>(&mut conn)
                    .await
                    .is_ok()
            {
                break conn;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("pinned Redis fixture must become ready");
    let info: String = ::redis::cmd("INFO")
        .arg("server")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(info.lines().any(|line| line == "redis_version:8.6.7"));
    let modules: String = ::redis::cmd("INFO")
        .arg("modules")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(modules.contains("name=search,ver=80608,"));
    eprintln!(
        "image={IMAGE}; container={}; protocol=RESP2; search=80608",
        fixture.0
    );
    let _: () = ::redis::cmd("FT.CREATE")
        .arg("idx")
        .arg("ON")
        .arg("HASH")
        .arg("PREFIX")
        .arg(1)
        .arg("doc:")
        .arg("SCHEMA")
        .arg("sku")
        .arg("TAG")
        .query_async(&mut conn)
        .await
        .unwrap();
    for (id, value) in [
        ("doc:1", "usb-c"),
        ("doc:2", "to be or not to be"),
        ("doc:3", "a|b"),
    ] {
        let _: i64 = ::redis::cmd("HSET")
            .arg(id)
            .arg("sku")
            .arg(value)
            .query_async(&mut conn)
            .await
            .unwrap();
    }
    let state = support::database_state_readonly(url);
    let tool = redis::ft_search(state);
    for dialect in [1, 2] {
        let _: () = ::redis::cmd("CONFIG")
            .arg("SET")
            .arg("search-default-dialect")
            .arg(dialect)
            .query_async(&mut conn)
            .await
            .unwrap();
        for query in ["@sku:{usb-c}", "@sku:{a{b}"] {
            let original: ::redis::RedisError = ::redis::cmd("FT.SEARCH")
                .arg("idx")
                .arg(query)
                .arg("NOCONTENT")
                .query_async::<Vec<::redis::Value>>(&mut conn)
                .await
                .unwrap_err();
            let original = format!(
                "{} {}",
                original.code().unwrap(),
                original.detail().unwrap()
            );
            assert!(original.starts_with("Syntax error at offset "));
            let result = tool
                .call(json!({"index":"idx","query":query,"nocontent":true}))
                .await;
            assert!(result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                format!("Tool error: FT.SEARCH failed: {original}{HINT}")
            );
        }
        for (query, id) in [
            (r"@sku:{usb\-c}", "doc:1"),
            (r"@sku:{a\|b}", "doc:3"),
            (r"@sku:{to\ be\ or\ not\ to\ be}", "doc:2"),
        ] {
            let result = tool
                .call(json!({"index":"idx","query":query,"nocontent":true}))
                .await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(text(&result), format!("Total results: 1\n\n1. {id}\n"));
        }
        let result = tool
            .call(json!({"index":"idx","query":"@sku:{to be or not to be}","nocontent":true}))
            .await;
        if dialect == 1 {
            assert!(result.is_error, "{result:?}");
            assert!(text(&result).ends_with(HINT));
        } else {
            assert!(!result.is_error, "{result:?}");
            assert_eq!(text(&result), "Total results: 1\n\n1. doc:2\n");
        }
    }
    let id = fixture.0.clone();
    drop(tool);
    drop(conn);
    drop(fixture);
    assert_eq!(
        docker(&["ps", "--all", "--quiet", "--filter", &format!("id={id}")]),
        ""
    );
}
