#![cfg(all(feature = "database", feature = "test-support"))]
//! Actual-tool wire contracts and one pinned RESP2 Search/JSON semantic fixture.
//! ```text
//! docker pull redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8
//! cargo test -p redisctl-mcp --all-features --test ft_create_guidance -- --include-ignored --nocapture
//! ```
//! Fixture startup/capability failures fail the live test, never skip it.

mod support;

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use redisctl_mcp::tools::redis;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tower_mcp::CallToolResult;

fn text(result: &CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0].as_text().expect("tool must return text")
}
fn alias_note(names: &str) -> String {
    format!(
        "\n\nNote: JSON fields without aliases: {names}. Optional aliases provide simple query names; raw JSONPath query parsing is limited."
    )
}
fn tag_note(names: &str) -> String {
    format!(
        "\n\nNote: JSON TAG fields without separators: {names}. JSON has no default separator. Whole strings are single tags; array paths (e.g. $.tags[*]) index separate values, and explicit separators split delimited strings."
    )
}
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
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

async fn synthetic(
    resp3: bool,
    alter: bool,
    input: Value,
    steps: Vec<(Vec<String>, &str)>,
) -> CallToolResult {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let steps: Vec<_> = steps
        .into_iter()
        .map(|(cmd, reply)| (cmd, reply.to_owned()))
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
    let state = support::database_state_write(format!("redis://{addr}/?protocol={protocol}"));
    let tool = if alter {
        redis::ft_alter(state.clone())
    } else {
        redis::ft_create(state.clone())
    };
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
async fn json_missing_and_mixed_aliases_get_aggregated_success_notes() {
    for resp3 in [false, true] {
        for on in ["JSON", "json", "JsOn"] {
            let result=synthetic(resp3,false,json!({"index":"idx","on":on,"schema":[{"name":"$.a","field_type":"text"},{"name":"$.b","alias":"b","field_type":"NUMERIC"},{"name":"$.c","field_type":"TEXT"}]}),vec![(args(&["FT.CREATE","idx","ON","JSON","SCHEMA","$.a","TEXT","$.b","AS","b","NUMERIC","$.c","TEXT"]),"+OK\r\n")]).await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                format!(
                    "Created index 'idx' with 3 field(s): $.a (text), $.b (NUMERIC), $.c (TEXT){}",
                    alias_note("$.a, $.c")
                )
            );
        }
    }
}

#[tokio::test]
async fn json_tag_notes_explain_whole_strings_and_array_paths_without_rewriting() {
    for resp3 in [false, true] {
        let result=synthetic(resp3,false,json!({"index":"idx","on":"JSON","schema":[{"name":"$.whole","alias":"whole","field_type":"tag"},{"name":"$.tags[*]","alias":"tags","field_type":"TAG"},{"name":"$.split","alias":"split","field_type":"TAG","separator":","}]}),vec![(args(&["FT.CREATE","idx","ON","JSON","SCHEMA","$.whole","AS","whole","TAG","$.tags[*]","AS","tags","TAG","$.split","AS","split","TAG","SEPARATOR",","]),"+OK\r\n")]).await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            format!(
                "Created index 'idx' with 3 field(s): $.whole (tag), $.tags[*] (TAG), $.split (TAG){}",
                tag_note("$.whole, $.tags[*]")
            )
        );
        let result = synthetic(
            resp3,
            false,
            json!({"index":"idx","on":"JSON","schema":[{"name":"$.tags","field_type":"TAG"}]}),
            vec![(
                args(&["FT.CREATE", "idx", "ON", "JSON", "SCHEMA", "$.tags", "TAG"]),
                "+OK\r\n",
            )],
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            format!(
                "Created index 'idx' with 1 field(s): $.tags (TAG){}{}",
                alias_note("$.tags"),
                tag_note("$.tags")
            )
        );
    }
}

#[tokio::test]
async fn explicit_options_and_hash_defaults_keep_exact_success_text() {
    for resp3 in [false, true] {
        for on in [None, Some("HASH"), Some("hash")] {
            let mut input = json!({"index":"idx","schema":[{"name":"tags","field_type":"TAG"}]});
            let mut expected = args(&["FT.CREATE", "idx"]);
            if let Some(on) = on {
                input["on"] = json!(on);
                expected.extend(args(&["ON", "HASH"]));
            }
            expected.extend(args(&["SCHEMA", "tags", "TAG"]));
            let result = synthetic(resp3, false, input, vec![(expected, "+OK\r\n")]).await;
            assert!(!result.is_error, "{result:?}");
            assert_eq!(
                text(&result),
                "Created index 'idx' with 1 field(s): tags (TAG)"
            );
        }
        let result=synthetic(resp3,false,json!({"index":"idx","on":"JSON","prefixes":["doc:","other:"],"schema":[{"name":"$.title","alias":"title","field_type":"TEXT","nostem":true,"weight":2.5,"sortable":true,"noindex":true},{"name":"$.comma","alias":"comma","field_type":"TAG","separator":","},{"name":"$.custom","alias":"custom","field_type":"TAG","separator":"|"}]}),vec![(args(&["FT.CREATE","idx","ON","JSON","PREFIX","2","doc:","other:","SCHEMA","$.title","AS","title","TEXT","NOSTEM","WEIGHT","2.5","SORTABLE","NOINDEX","$.comma","AS","comma","TAG","SEPARATOR",",","$.custom","AS","custom","TAG","SEPARATOR","|"]),"+OK\r\n")]).await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            "Created index 'idx' with 3 field(s): $.title (TEXT), $.comma (TAG), $.custom (TAG)"
        );
    }
}

#[tokio::test]
async fn skip_is_byte_identical_and_only_successful_create_gets_notes() {
    for resp3 in [false, true] {
        let input = json!({"index":"idx","on":"JSON","if_exists":"SKIP","schema":[{"name":"$.tags","field_type":"TAG"}]});
        let result = synthetic(
            resp3,
            false,
            input,
            vec![(args(&["FT.INFO", "idx"]), "*0\r\n")],
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            "Index 'idx' already exists — skipped (if_exists=skip)"
        );
        for mode in ["skip", "drop"] {
            let result=synthetic(resp3,false,json!({"index":"idx","on":"JSON","if_exists":mode,"schema":[{"name":"$.tags","field_type":"TAG"}]}),vec![(args(&["FT.INFO","idx"]),"-ERR Unknown index name\r\n"),(args(&["FT.CREATE","idx","ON","JSON","SCHEMA","$.tags","TAG"]),"+OK\r\n")]).await;
            assert!(!result.is_error, "{result:?}");
            let suffix = if mode == "drop" {
                " (replaced existing index)"
            } else {
                ""
            };
            assert_eq!(
                text(&result),
                format!(
                    "Created index 'idx' with 1 field(s): $.tags (TAG){suffix}{}{}",
                    alias_note("$.tags"),
                    tag_note("$.tags")
                )
            );
        }
        let result=synthetic(resp3,false,json!({"index":"idx","on":"JSON","if_exists":"drop","schema":[{"name":"$.title","field_type":"TEXT"}]}),vec![(args(&["FT.INFO","idx"]),"*0\r\n"),(args(&["FT.DROPINDEX","idx"]),"+OK\r\n"),(args(&["FT.CREATE","idx","ON","JSON","SCHEMA","$.title","TEXT"]),"+OK\r\n")]).await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(
            text(&result),
            format!(
                "Created index 'idx' with 1 field(s): $.title (TEXT) (replaced existing index){}",
                alias_note("$.title")
            )
        );
    }
}

#[tokio::test]
async fn server_decode_and_validation_errors_never_get_success_guidance() {
    for resp3 in [false, true] {
        for reply in ["-ERR create refused\r\n", "?invalid\r\n"] {
            let result = synthetic(
                resp3,
                false,
                json!({"index":"idx","on":"JSON","schema":[{"name":"$.tags","field_type":"TAG"}]}),
                vec![(
                    args(&["FT.CREATE", "idx", "ON", "JSON", "SCHEMA", "$.tags", "TAG"]),
                    reply,
                )],
            )
            .await;
            assert!(result.is_error, "{result:?}");
            assert!(text(&result).contains("FT.CREATE failed"));
            assert!(!text(&result).contains("Note:"));
        }
        for input in [
            json!({"index":"idx","schema":[]}),
            json!({"index":"idx","on":"invalid","schema":[{"name":"x","field_type":"TEXT"}]}),
            json!({"index":"idx","schema":[{"name":"x","field_type":"invalid"}]}),
            json!({"index":"idx","schema":[{"name":"x","field_type":"TAG","separator":",,"}]}),
            json!({"index":"idx","if_exists":"invalid","schema":[{"name":"x","field_type":"TAG"}]}),
        ] {
            let result = synthetic(resp3, false, input, vec![]).await;
            assert!(result.is_error, "{result:?}");
            assert!(!text(&result).contains("Note:"));
        }
        let result=synthetic(resp3,false,json!({"index":"idx","on":"JSON","if_exists":"drop","schema":[{"name":"$.tags","field_type":"TAG"}]}),vec![(args(&["FT.INFO","idx"]),"*0\r\n"),(args(&["FT.DROPINDEX","idx"]),"-ERR drop refused\r\n")]).await;
        assert!(result.is_error, "{result:?}");
        assert!(text(&result).contains("FT.DROPINDEX failed during if_exists=drop"));
        assert!(!text(&result).contains("Note:"));
        let result=synthetic(resp3,false,json!({"index":"idx","on":"JSON","if_exists":"drop","schema":[{"name":"$.tags","field_type":"TAG"}]}),vec![(args(&["FT.INFO","idx"]),"*0\r\n"),(args(&["FT.DROPINDEX","idx"]),"+OK\r\n"),(args(&["FT.CREATE","idx","ON","JSON","SCHEMA","$.tags","TAG"]),"-ERR create refused\r\n")]).await;
        assert!(result.is_error, "{result:?}");
        assert!(text(&result).contains("FT.CREATE failed"));
        assert!(!text(&result).contains("Note:"));
    }
}

#[tokio::test]
async fn alter_runtime_does_not_gain_create_notes() {
    for resp3 in [false, true] {
        let result = synthetic(
            resp3,
            true,
            json!({"index":"idx","field":{"name":"$.tags","field_type":"TAG"}}),
            vec![(
                args(&["FT.ALTER", "idx", "SCHEMA", "ADD", "$.tags", "TAG"]),
                "+OK\r\n",
            )],
        )
        .await;
        assert!(!result.is_error, "{result:?}");
        assert_eq!(text(&result), "Added field '$.tags' (TAG) to index 'idx'");
    }
}

#[test]
fn tool_descriptions_explain_optional_aliases_and_hash_json_separator_defaults() {
    let state = support::database_state_write("redis://127.0.0.1:1".to_owned());
    let definition = redis::ft_create(state.clone()).definition();
    let description = definition.description.unwrap();
    assert!(description.contains("Aliases are optional"));
    assert!(description.contains("limited JSONPath"));
    assert!(!description.contains("Always set"));
    assert!(!description.contains("raw JSONPath cannot"));
    assert!(description.contains("JSON TAG"));
    assert!(description.contains("no default separator"));
    for tool in [redis::ft_create(state.clone()), redis::ft_alter(state)] {
        let schema = tool.definition().input_schema;
        let description =
            schema["$defs"]["FieldDefinition"]["properties"]["separator"]["description"]
                .as_str()
                .unwrap();
        assert!(description.contains("HASH defaults"), "{description}");
        assert!(description.contains("JSON has no default"), "{description}");
    }
}

const IMAGE: &str =
    "redis:8.6.7@sha256:e6dae84f7db289dde3384fdb18d182556784e5a681df3a3f54d1289952364ce8";
struct Fixture {
    id: Option<String>,
    port: u16,
}
fn docker(args: &[&str]) -> String {
    let out = Command::new("docker")
        .args(args)
        .output()
        .expect("Docker is required");
    assert!(out.status.success(), "Docker fixture command failed");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
impl Fixture {
    async fn start() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("redisctl-mcp-ft-guidance-{}-{nonce}", std::process::id());
        let id = docker(&[
            "run",
            "--detach",
            "--rm",
            "--pull=never",
            "--name",
            &name,
            "--publish",
            "127.0.0.1::6379",
            IMAGE,
        ]);
        let mut f = Self {
            id: Some(id),
            port: 0,
        };
        f.port = docker(&["port", f.id.as_deref().unwrap(), "6379/tcp"])
            .strip_prefix("127.0.0.1:")
            .expect("loopback binding required")
            .parse()
            .unwrap();
        let client = ::redis::Client::open(f.url()).unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(mut c) = client.get_multiplexed_async_connection().await
                    && ::redis::cmd("PING")
                        .query_async::<String>(&mut c)
                        .await
                        .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("fixture must become ready");
        let image_id = docker(&[
            "inspect",
            f.id.as_deref().unwrap(),
            "--format",
            "{{.Image}}",
        ]);
        eprintln!(
            "fixture image={IMAGE}; image_id={image_id}; name={name}; port={}",
            f.port
        );
        f
    }
    fn url(&self) -> String {
        format!("redis://127.0.0.1:{}/?protocol=resp2", self.port)
    }
    fn remove(&mut self) {
        docker(&["rm", "--force", self.id.as_deref().unwrap()]);
        self.id = None;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(id) = &self.id {
            let _ = Command::new("docker").args(["rm", "--force", id]).output();
        }
    }
}

async fn search_count(
    conn: &mut ::redis::aio::MultiplexedConnection,
    index: &str,
    query: &str,
) -> i64 {
    let reply: Vec<::redis::Value> = ::redis::cmd("FT.SEARCH")
        .arg(index)
        .arg(query)
        .arg("NOCONTENT")
        .arg("DIALECT")
        .arg(2)
        .query_async(conn)
        .await
        .expect("fixture search must succeed");
    let Some(::redis::Value::Int(count)) = reply.first() else {
        panic!("unexpected search reply: {reply:?}")
    };
    *count
}

#[tokio::test]
#[ignore = "Requires Docker and pinned Redis 8.6.7 Search/JSON image"]
async fn live_json_schema_choices_preserve_search_semantics_resp2() {
    let mut fixture = Fixture::start().await;
    let url = fixture.url();
    let mut conn = ::redis::Client::open(url.clone())
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
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
    assert!(modules.contains("name=ReJSON,ver=80600,"));
    assert!(modules.contains("name=search,ver=80608,"));
    let state = support::database_state_write(url.clone());
    let mut tool_conn = state.redis_connection_for_url(&url).await.unwrap();
    let client_info: String = ::redis::cmd("CLIENT")
        .arg("INFO")
        .query_async(&mut tool_conn)
        .await
        .unwrap();
    assert!(client_info.split_whitespace().any(|x| x == "resp=2"));
    eprintln!(
        "protocol=RESP2; server=8.6.7; {}",
        modules
            .lines()
            .filter(|x| x.starts_with("module:"))
            .collect::<Vec<_>>()
            .join("; ")
    );
    let doc=json!({"title":"hello world","whole":"red,blue","split":"red,blue","custom":"red|blue","tags":["red","blue"]}).to_string();
    let _: () = ::redis::cmd("JSON.SET")
        .arg("guidance:json:1")
        .arg("$")
        .arg(&doc)
        .query_async(&mut conn)
        .await
        .expect("fixture must support JSON.SET");
    let _: () = ::redis::cmd("HSET")
        .arg("guidance:hash:1")
        .arg("tags")
        .arg("red,blue")
        .query_async(&mut conn)
        .await
        .unwrap();
    let cases = [
        (
            "guidance-noalias",
            json!({"index":"guidance-noalias","on":"JSON","prefixes":["guidance:json:"],"schema":[{"name":"$.title","field_type":"TEXT"}]}),
            format!(
                "Created index 'guidance-noalias' with 1 field(s): $.title (TEXT){}",
                alias_note("$.title")
            ),
        ),
        (
            "guidance-json",
            json!({"index":"guidance-json","on":"jSoN","prefixes":["guidance:json:"],"schema":[{"name":"$.title","alias":"title","field_type":"TEXT"},{"name":"$.whole","alias":"whole","field_type":"TAG"},{"name":"$.split","alias":"split","field_type":"TAG","separator":","},{"name":"$.custom","alias":"custom","field_type":"TAG","separator":"|"},{"name":"$.tags[*]","alias":"tags","field_type":"TAG"}]}),
            format!(
                "Created index 'guidance-json' with 5 field(s): $.title (TEXT), $.whole (TAG), $.split (TAG), $.custom (TAG), $.tags[*] (TAG){}",
                tag_note("$.whole, $.tags[*]")
            ),
        ),
        (
            "guidance-hash",
            json!({"index":"guidance-hash","prefixes":["guidance:hash:"],"schema":[{"name":"tags","field_type":"TAG"}]}),
            "Created index 'guidance-hash' with 1 field(s): tags (TAG)".to_owned(),
        ),
    ];
    let mut mismatches = Vec::new();
    for (index, input, expected) in cases {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            redis::ft_create(state.clone()).call(input),
        )
        .await
        .unwrap();
        assert!(!result.is_error, "{result:?}");
        eprintln!("create {index}: {}", text(&result));
        if text(&result) != expected {
            mismatches.push(format!(
                "{index}: expected {expected:?}, got {:?}",
                text(&result)
            ));
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if search_count(&mut conn, index, "*").await == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("index must become ready");
    }
    for (index, query, expected) in [
        ("guidance-noalias", "*", 1),
        ("guidance-json", "@title:hello", 1),
        ("guidance-json", r"@whole:{red\,blue}", 1),
        ("guidance-json", "@whole:{red}", 0),
        ("guidance-json", "@split:{red}", 1),
        ("guidance-json", "@split:{blue}", 1),
        ("guidance-json", r"@split:{red\,blue}", 0),
        ("guidance-json", "@tags:{red}", 1),
        ("guidance-json", "@tags:{blue}", 1),
        ("guidance-json", r"@tags:{red\,blue}", 0),
        ("guidance-json", "@custom:{red}", 1),
        ("guidance-json", "@custom:{blue}", 1),
        ("guidance-json", r"@custom:{red\|blue}", 0),
        ("guidance-hash", "@tags:{red}", 1),
        ("guidance-hash", r"@tags:{red\,blue}", 0),
    ] {
        let count = search_count(&mut conn, index, query).await;
        assert_eq!(count, expected, "{index} {query}");
        eprintln!("semantic {index} {query}: {count}");
    }
    let stored: String = ::redis::cmd("JSON.GET")
        .arg("guidance:json:1")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(stored, doc);
    let stored: String = ::redis::cmd("HGET")
        .arg("guidance:hash:1")
        .arg("tags")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(stored, "red,blue");
    fixture.remove();
    assert!(
        mismatches.is_empty(),
        "missing actual-tool JSON guidance: {mismatches:#?}"
    );
}
