#![cfg(all(feature = "database", feature = "test-support"))]
//! Captured wire replies replayed through the actual tool. No Redis service needed.

mod support;

use std::time::Duration;

use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
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
        let length: usize = line.strip_prefix('$').unwrap().trim().parse().unwrap();
        let mut bytes = vec![0; length + 2];
        reader.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes[length..], b"\r\n");
        args.push(String::from_utf8(bytes[..length].to_vec()).unwrap());
    }
    args
}

async fn call(
    resp3: bool,
    input: Value,
    reply: &[u8],
    expected: Option<Vec<String>>,
) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let reply = reply.to_vec();
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
        if let Some(expected) = expected {
            assert_eq!(command(&mut reader).await, expected);
            write.write_all(&reply).await.unwrap();
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
    let protocol = if resp3 { "resp3" } else { "resp2" };
    let state = support::database_state_readonly(format!("redis://{addr}/?protocol={protocol}"));
    let tool = redis::ft_profile(state.clone());
    let result = tokio::time::timeout(Duration::from_secs(5), tool.call(input))
        .await
        .expect("tool must finish");
    drop(tool);
    drop(state);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("fixture must finish without another query")
        .expect("wire assertions must pass");
    result
}

async fn profile(resp3: bool, command: &str, query: &str, reply: &[u8]) -> CallToolResult {
    call(
        resp3,
        json!({"index": "profile_idx", "command": command, "query": query}),
        reply,
        Some(
            [
                "FT.PROFILE",
                "profile_idx",
                &command.to_uppercase(),
                "QUERY",
                query,
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ),
    )
    .await
}

fn text(result: &CallToolResult) -> &str {
    assert!(!result.is_error, "unexpected tool error: {result:?}");
    assert_eq!(result.content.len(), 1);
    result.content[0].as_text().unwrap()
}

fn capture(resp3: bool, name: &str) -> Vec<u8> {
    let protocol = if resp3 { "resp3" } else { "resp2" };
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ft-profile")
            .join(format!("{protocol}-{name}.reply.resp")),
    )
    .unwrap()
}

const NESTED: &str = "(@title:(redis|profiling) @category:{a|b}) @price:[0 20]";

#[tokio::test]
async fn captured_search_replies_name_sections_and_retain_nested_and_limited_children() {
    for resp3 in [false, true] {
        for (name, query) in [
            ("search-basic", "redis"),
            ("search-nested", NESTED),
            ("search-limited", NESTED),
            ("search-count", "redis"),
        ] {
            // LIMITED and count-only replies are shape replays; input arguments stay unchanged.
            let result = profile(resp3, "search", query, &capture(resp3, name)).await;
            let output = text(&result);
            assert!(output.starts_with(&format!(
                "Profile for SEARCH '{query}' on 'profile_idx':\n\nResults:\n"
            )));
            for label in [
                "\nProfile:\n",
                "Shards:",
                "Coordinator:",
                "Iterators profile:",
                "Result processors profile:",
            ] {
                assert!(output.contains(label), "{name}: missing {label}: {output}");
            }
            assert!(output.contains("Total profile time:"));
            let last_processor = if name == "search-count" {
                "Counter"
            } else {
                "Sorter"
            };
            assert!(output.find("Index").unwrap() < output.find(last_processor).unwrap());
            if name.contains("nested") || name.contains("limited") {
                for label in [
                    "Child iterators:",
                    "INTERSECT",
                    "UNION",
                    "NUMERIC",
                    "TAG",
                    "TEXT",
                ] {
                    assert!(output.contains(label), "{name}: missing {label}");
                }
            }
            if name == "search-limited" {
                assert!(output.contains("The number of iterators in the union is 2"));
            }
            if name == "search-count" {
                assert!(output.contains("int(30)"));
            }
        }
    }
}

#[tokio::test]
async fn captured_aggregate_replies_keep_empty_rows_and_ordered_processors_without_inventing_total()
{
    for resp3 in [false, true] {
        for name in ["aggregate-bare", "aggregate-group", "aggregate-load"] {
            let result = profile(resp3, "AGGREGATE", "*", &capture(resp3, name)).await;
            let output = text(&result);
            assert!(output.contains("\nResults:\n"));
            assert!(output.contains("\nProfile:\n"));
            assert!(output.contains("Result processors profile:"));
            if name == "aggregate-bare" && !resp3 {
                let results = output.split("\nProfile:\n").next().unwrap();
                assert!(results.contains("[0]: int(1)"));
                assert_eq!(results.matches("array []").count(), 30);
                assert!(results.contains("[30]: array []"));
                assert!(!results.to_lowercase().contains("total"));
            }
            if name == "aggregate-group" {
                assert!(output.find("Index").unwrap() < output.find("Grouper").unwrap());
                assert!(output.find("Grouper").unwrap() < output.find("Sorter").unwrap());
                for category in ["a", "b", "c"] {
                    assert!(output.contains(&format!("\"{category}\"")));
                }
            }
            if name == "aggregate-load" {
                assert!(output.contains("title"));
                assert!(output.contains("price"));
            }
        }
    }
}

#[tokio::test]
async fn captured_empty_replies_and_timing_precision_are_preserved() {
    for resp3 in [false, true] {
        for (command, name) in [("SEARCH", "search-empty"), ("AGGREGATE", "aggregate-empty")] {
            let result = profile(resp3, command, "nomatchzzzz", &capture(resp3, name)).await;
            let output = text(&result);
            assert!(output.contains("int(0)"));
            assert!(output.contains("EMPTY"));
            assert!(output.contains("Coordinator:"));
            assert!(!output.contains("microseconds"));
            if !resp3 && command == "SEARCH" {
                assert!(output.contains("4.199999999999999e-5"));
            }
        }
    }
    let result = profile(false, "SEARCH", NESTED, &capture(false, "search-nested")).await;
    assert!(text(&result).contains("0.0013330000000000009"));
}

// The following wire values are deliberately synthetic, not additional live captures.
fn bulk(value: &[u8]) -> Vec<u8> {
    let mut reply = format!("${}\r\n", value.len()).into_bytes();
    reply.extend(value);
    reply.extend(b"\r\n");
    reply
}

fn array(items: &[Vec<u8>]) -> Vec<u8> {
    let mut reply = format!("*{}\r\n", items.len()).into_bytes();
    for item in items {
        reply.extend(item);
    }
    reply
}

fn map(items: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut reply = format!("%{}\r\n", items.len()).into_bytes();
    for (key, value) in items {
        reply.extend(bulk(key.as_bytes()));
        reply.extend(value);
    }
    reply
}

fn synthetic_profile(data: Vec<u8>) -> Vec<u8> {
    map(&[("Results", array(&[])), ("Profile", data)])
}

#[tokio::test]
async fn synthetic_multiple_shards_coordinator_warnings_and_unknown_fields_are_retained() {
    let iterator = map(&[
        ("Type", bulk(b"UNION")),
        ("Child iterators", bulk(b"limited child summary")),
    ]);
    let shard = map(&[
        ("Total profile time", b",0.0013330000000000009\r\n".to_vec()),
        ("Warning", array(&[bulk(b"Timeout limit was reached")])),
        ("Iterators profile", iterator),
        ("Future field", array(&[bulk(b"key"), bulk(b"value")])),
    ]);
    let reply = synthetic_profile(map(&[
        ("Shards", array(&[shard.clone(), shard])),
        (
            "Coordinator",
            map(&[
                ("Total profile time", b",-0\r\n".to_vec()),
                ("Result processors profile", array(&[])),
            ]),
        ),
        ("Future root", b"#t\r\n".to_vec()),
    ]));
    let result = profile(true, "SEARCH", "*", &reply).await;
    let output = text(&result);
    assert_eq!(output.matches("Timeout limit was reached").count(), 2);
    assert_eq!(output.matches("limited child summary").count(), 2);
    assert_eq!(output.matches("double(0.0013330000000000009)").count(), 2);
    assert!(output.contains("double(-0.0)"));
    assert!(output.contains("Future root: boolean(true)"));
    assert!(output.contains("[1]: bulk-string(\"value\")"));
    assert!(output.contains("Result processors profile: array []"));
}

#[tokio::test]
async fn unknown_odd_duplicate_and_nontext_records_fall_back_without_pairing_or_data_loss() {
    let bad_profiles = [
        array(&[bulk(b"Shards"), array(&[]), bulk(b"unpaired")]),
        array(&[
            bulk(b"Shards"),
            bulk(b"first"),
            bulk(b"Shards"),
            bulk(b"second"),
        ]),
        array(&[b":7\r\n".to_vec(), bulk(b"not a text key")]),
        map(&[("Future only", array(&[]))]),
    ];
    for (data, retained) in bad_profiles.into_iter().zip([
        &["unpaired"][..],
        &["first", "second"][..],
        &["int(7)", "not a text key"][..],
        &["Future only", "array []"][..],
    ]) {
        let result = profile(true, "SEARCH", "*", &synthetic_profile(data)).await;
        let output = text(&result);
        assert!(output.contains("Raw reply:"));
        assert!(output.contains("Profile:\nRaw reply:"));
        for value in retained {
            assert!(output.contains(value), "lost {value}: {output}");
        }
    }
    for (reply, retained) in [
        (
            map(&[
                ("Results", bulk(b"first")),
                ("Results", bulk(b"second")),
                ("Profile", map(&[("Shards", array(&[]))])),
            ]),
            &["first", "second", "Profile"][..],
        ),
        (
            array(&[bulk(b"one"), bulk(b"two"), bulk(b"three")]),
            &["one", "two", "three"][..],
        ),
    ] {
        let result = profile(true, "SEARCH", "*", &reply).await;
        let output = text(&result);
        assert!(output.contains("Raw reply:"));
        assert!(output.contains("bulk-string"));
        for value in retained {
            assert!(output.contains(value), "lost {value}: {output}");
        }
    }
    let bad_child = array(&[bulk(b"Type"), bulk(b"UNION"), bulk(b"dangling")]);
    let reply = synthetic_profile(map(&[(
        "Shards",
        array(&[map(&[("Iterators profile", bad_child)])]),
    )]));
    let result = profile(true, "SEARCH", "*", &reply).await;
    assert!(text(&result).contains("[2]: bulk-string(\"dangling\")"));
}

#[tokio::test]
async fn accepted_unknown_containers_preserve_types_binary_data_attributes_and_empty_values() {
    for (reply, expected) in [
        (b"_\r\n".to_vec(), "nil"),
        (bulk(b"opaque"), "bulk-string(\"opaque\")"),
        (b"~0\r\n".to_vec(), "set []"),
        (b"%0\r\n".to_vec(), "map {}"),
        (array(&[]), "array []"),
        (
            array(&[bulk(&[0xff, 0, 0xfe])]),
            "binary-data([255, 0, 254])",
        ),
        (
            array(&[b"|1\r\n+meta\r\n+keep me\r\n*0\r\n".to_vec()]),
            "simple-string(\"keep me\")",
        ),
        (
            array(&[
                b"#f\r\n".to_vec(),
                b",inf\r\n".to_vec(),
                b",-inf\r\n".to_vec(),
                b",nan\r\n".to_vec(),
            ]),
            "double(NaN)",
        ),
        (
            array(&[
                b"(123456789012345678901234567890\r\n".to_vec(),
                b"=7\r\ntxt:abc\r\n".to_vec(),
            ]),
            "123456789012345678901234567890",
        ),
    ] {
        let result = profile(true, "SEARCH", "*", &reply).await;
        let output = text(&result);
        assert!(output.contains("Raw reply:"));
        assert!(output.contains(expected), "missing {expected}: {output}");
        if output.contains("attribute") {
            for part in [
                "data:",
                "attributes:",
                "simple-string(\"meta\")",
                "array []",
            ] {
                assert!(output.contains(part), "missing attribute {part}: {output}");
            }
        }
        if expected == "double(NaN)" {
            for part in ["boolean(false)", "double(inf)", "double(-inf)"] {
                assert!(output.contains(part));
            }
        }
        if expected == "123456789012345678901234567890" {
            assert!(output.contains("verbatim-string(Text,\"abc\")"));
        }
    }
}

#[tokio::test]
async fn old_decoder_rejections_remain_tool_errors_for_scalar_and_attribute_roots() {
    for reply in [
        b"+opaque\r\n".as_slice(),
        b"+OK\r\n",
        b":7\r\n",
        b"#t\r\n",
        b",0.25\r\n",
        b"|1\r\n+meta\r\n+keep\r\n*0\r\n",
        b"=7\r\ntxt:abc\r\n",
        b"(123456789012345678901234567890\r\n",
    ] {
        let result = profile(true, "SEARCH", "*", reply).await;
        assert!(
            result.is_error,
            "previously rejected root succeeded: {reply:?}: {result:?}"
        );
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("FT.PROFILE failed")
        );
    }
}

#[tokio::test]
async fn captured_server_errors_nested_errors_and_wire_decode_errors_remain_errors() {
    for resp3 in [false, true] {
        for (name, message) in [
            ("search-syntax-error", "Syntax error at offset 0"),
            ("aggregate-reducer-error", "No such reducer: NOT_A_REDUCER"),
            (
                "command-error",
                "No `SEARCH`, `AGGREGATE`, or `HYBRID` provided",
            ),
            ("missing-index-error", "missing_profile_idx: no such index"),
        ] {
            // Server errors are replayed against a valid request, independently of client validation.
            let result = profile(resp3, "SEARCH", "*", &capture(resp3, name)).await;
            assert!(result.is_error);
            let output = result.content[0].as_text().unwrap();
            assert!(output.contains("FT.PROFILE failed"));
            assert!(output.contains(message), "{name}: {output}");
        }
    }
    for reply in [b"*1\r\n-ERR nested failure\r\n".as_slice(), b"!invalid\r\n"] {
        let result = profile(true, "SEARCH", "*", reply).await;
        assert!(result.is_error);
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .contains("FT.PROFILE failed")
        );
    }
}

#[tokio::test]
async fn invalid_command_is_rejected_without_sending_profile_or_followup_queries() {
    for resp3 in [false, true] {
        let result = call(
            resp3,
            json!({"index":"profile_idx", "command":"BOGUS", "query":"*"}),
            b"",
            None,
        )
        .await;
        assert!(result.is_error);
        assert_eq!(
            result.content[0].as_text(),
            Some("Tool error: Invalid command 'BOGUS'. Valid values: SEARCH, AGGREGATE")
        );
    }
    let state = support::database_state_readonly("redis://127.0.0.1:0".to_owned());
    let tool = redis::ft_profile(state);
    for (input, message) in [
        (
            json!({"index":"i", "command":"SEARCH", "query":"*", "limit":1}),
            "unknown field",
        ),
        (json!({"index":"i", "command":"SEARCH"}), "missing field"),
        (
            json!({"index":"i", "command":7, "query":"*"}),
            "invalid type",
        ),
    ] {
        let result = tool.call(input).await;
        assert!(result.is_error);
        assert!(result.content[0].as_text().unwrap().contains(message));
    }
}

#[tokio::test]
async fn captured_empty_aggregate_output_preserves_every_field_and_value() {
    for resp3 in [false, true] {
        let result = profile(
            resp3,
            "AGGREGATE",
            "nomatchzzzz",
            &capture(resp3, "aggregate-empty"),
        )
        .await;
        let expected = if resp3 {
            include_str!("fixtures/ft-profile/resp3-aggregate-empty.txt")
        } else {
            include_str!("fixtures/ft-profile/resp2-aggregate-empty.txt")
        };
        assert_eq!(text(&result), expected);
    }
}

#[tokio::test]
async fn legacy_unknown_profile_layout_and_nested_attribute_fallback_preserve_complete_values() {
    let legacy = array(&[
        array(&[bulk(b"Total profile time"), bulk(b"0.0000001")]),
        array(&[bulk(b"Warning")]),
        array(&[bulk(b"Future"), array(&[])]),
    ]);
    let result = profile(
        false,
        "SEARCH",
        "*",
        &array(&[array(&[b":0\r\n".to_vec()]), legacy]),
    )
    .await;
    assert_eq!(
        text(&result),
        "Profile for SEARCH '*' on 'profile_idx':\n\nResults:\narray [\n  [0]: int(0)\n]\n\nProfile:\nRaw reply:\narray [\n  [0]: array [\n    [0]: bulk-string(\"Total profile time\")\n    [1]: bulk-string(\"0.0000001\")\n  ]\n  [1]: array [\n    [0]: bulk-string(\"Warning\")\n  ]\n  [2]: array [\n    [0]: bulk-string(\"Future\")\n    [1]: array []\n  ]\n]\n"
    );
    let result = profile(
        true,
        "SEARCH",
        "*",
        b"*1\r\n|1\r\n+meta\r\n*2\r\n:9007199254740993\r\n$3\r\na\nb\r\n*0\r\n",
    )
    .await;
    assert_eq!(
        text(&result),
        "Profile for SEARCH '*' on 'profile_idx':\n\nRaw reply:\narray [\n  [0]: attribute { data: array [], attributes: map {\n    simple-string(\"meta\") => array [\n      [0]: int(9007199254740993)\n      [1]: bulk-string(\"a\\nb\")\n    ]\n  } }\n]\n"
    );
}
