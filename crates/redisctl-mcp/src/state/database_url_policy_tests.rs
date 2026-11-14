//! URL policy contracts through actual legacy tool handlers, without user config or Redis.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, tcp::OwnedReadHalf};
use tokio::task::JoinHandle;
use tower_mcp::{CallToolResult, Tool};

use super::AppState;
use crate::policy::{Policy, PolicyConfig, is_policy_denied_message};
use crate::tools::redis;

fn state(source: &str) -> AppState {
    AppState::without_config_for_tests(Arc::new(Policy::new(
        toml::from_str::<PolicyConfig>(source).unwrap(),
        AppState::test_tool_mapping(),
        "url-policy-test".to_string(),
    )))
}

async fn call(tool: &Tool, input: Value) -> CallToolResult {
    tokio::time::timeout(Duration::from_secs(2), tool.call(input))
        .await
        .expect("loopback tool call must finish")
}

fn assert_policy_denial(result: &CallToolResult) -> &str {
    assert!(result.is_error);
    // tower-mcp adds this display prefix when converting a handler ToolError.
    let text = result.first_text().unwrap();
    let message = text.strip_prefix("Tool error: ").unwrap_or(text);
    assert!(is_policy_denied_message(message), "{message}");
    message
}

async fn read_command(reader: &mut BufReader<OwnedReadHalf>) -> Option<Vec<String>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await.unwrap() == 0 {
        return None;
    }
    assert!(line.starts_with('*'));
    let count: usize = line[1..].trim_end().parse().unwrap();
    let mut command = Vec::with_capacity(count);
    for _ in 0..count {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.starts_with('$'));
        let length: usize = line[1..].trim_end().parse().unwrap();
        let mut bytes = vec![0; length + 2];
        reader.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes[length..], b"\r\n");
        bytes.truncate(length);
        command.push(String::from_utf8(bytes).unwrap());
    }
    Some(command)
}

/// One connection only: supports redis-rs handshakes and records each PING.
async fn loopback() -> (String, JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("redis://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (read, mut write) = socket.into_split();
            let mut reader = BufReader::new(read);
            let mut pings = 0;
            while let Some(command) = read_command(&mut reader).await {
                let reply = match command[0].as_str() {
                    "CLIENT" => {
                        assert_eq!(command[1], "SETINFO");
                        b"+OK\r\n".as_slice()
                    }
                    "PING" => {
                        pings += 1;
                        b"+PONG\r\n".as_slice()
                    }
                    other => panic!("unexpected synthetic command: {other}"),
                };
                write.write_all(reply).await.unwrap();
            }
            pings
        })
        .await
        .expect("synthetic connection must close")
    });
    (url, handle)
}

#[tokio::test]
async fn explicit_urls_are_denied_before_cache_access_or_network() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "redis://url-user:url-password@{}/0",
        listener.local_addr().unwrap()
    );
    for source in [
        "",
        "tier = 'full'\nallow = ['redis_ping', 'redis_alias_list', 'redis_command']",
        "[database]\ntier = 'full'\nallow = ['redis_ping', 'redis_alias_list']",
    ] {
        let mut state = state(source);
        state.database_url = Some("invalid startup target".to_string());
        let state = Arc::new(state);
        // Any cache read (including a health check) would block this call.
        let cache = state.clients.write().await;
        for tool in [redis::ping(state.clone()), redis::alias_list(state.clone())] {
            for explicit_url in [&url, "", "malformed-url-password"] {
                let result = call(
                    &tool,
                    json!({"url": explicit_url, "profile": "missing-profile"}),
                )
                .await;
                let message = assert_policy_denial(&result);
                assert!(message.contains("Per-call Redis URL overrides are disabled"));
                let serialized = serde_json::to_string(&result).unwrap();
                for secret in ["url-user", "url-password", "missing-profile", &url] {
                    assert!(!serialized.contains(secret), "{serialized}");
                }
            }
        }
        assert!(cache.database.is_empty());
    }
    let state = Arc::new(state("tier = 'full'"));
    let cache = state.clients.write().await;
    let result = call(
        &redis::redis_command(state.clone()),
        json!({"command": "PING", "url": url}),
    )
    .await;
    assert!(assert_policy_denial(&result).contains("URL overrides are disabled"));
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("url-password")
    );
    assert!(cache.database.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err(),
        "denied overrides must not open a socket"
    );
}

#[tokio::test]
async fn operator_opt_in_restores_url_precedence_for_both_macro_variants() {
    let (url, server) = loopback().await;
    let mut state = state("[database]\nallow_url_override = true");
    state.database_url = Some("invalid startup target".to_string());
    let state = Arc::new(state);
    for tool in [redis::ping(state.clone()), redis::alias_list(state.clone())] {
        let result = call(&tool, json!({"url": url, "profile": "missing-profile"})).await;
        assert!(!result.is_error, "{result:?}");
    }
    assert_eq!(state.clients.read().await.database.len(), 1);
    assert!(state.clients.read().await.database.contains_key(&url));
    drop(state);
    // The second tool reuses the first connection, including its health check.
    assert_eq!(server.await.unwrap(), 2);
}

#[tokio::test]
async fn configured_targets_and_profiles_work_without_url_opt_in() {
    for route in [
        "startup",
        "explicit-profile",
        "default-profile",
        "selected-profile",
    ] {
        let (url, server) = loopback().await;
        let mut state = state("");
        let port = url.rsplit(':').next().unwrap();
        state.config = Some(
            toml::from_str(&format!(
                "default_database = 'db'\n[profiles.db]\ndeployment_type = 'database'\nhost = '127.0.0.1'\nport = {port}\ntls = false"
            ))
            .unwrap(),
        );
        let input = match route {
            "startup" => {
                state.database_url = Some(url);
                json!({})
            }
            "explicit-profile" => {
                state.database_url = Some("invalid startup target".to_string());
                json!({"profile": "db"})
            }
            "selected-profile" => {
                state.config.as_mut().unwrap().default_database = None;
                state.profiles = vec!["db".to_string()];
                json!({})
            }
            _ => json!({}),
        };
        let state = Arc::new(state);
        assert!(!state.policy().allows_database_url_override());
        let result = call(&redis::ping(state.clone()), input).await;
        assert!(!result.is_error, "{route}: {result:?}");
        drop(state);
        assert_eq!(server.await.unwrap(), 1);
    }
}

#[tokio::test]
async fn url_opt_in_does_not_grant_tool_or_agent_policy_changes() {
    let state = Arc::new(state("[database]\nallow_url_override = true"));
    let cache = state.clients.write().await;
    let denied = call(
        &redis::set(state.clone()),
        json!({"url": "redis://url-user:url-password@127.0.0.1:1", "key": "k", "value": "v"}),
    )
    .await;
    assert_eq!(
        assert_policy_denial(&denied),
        "Redisctl policy denied: Tool 'redis_set' is not allowed by the active policy"
    );
    assert!(cache.database.is_empty());
    drop(cache);

    let state = Arc::new(super::AppState::without_config_for_tests(
        AppState::test_policy(),
    ));
    for tool in [redis::ping(state.clone()), redis::alias_list(state.clone())] {
        let denied = call(&tool, json!({"allow_url_override": true})).await;
        assert!(denied.is_error);
        assert!(denied.first_text().unwrap().contains("unknown field"));
    }
    assert!(!state.policy().allows_database_url_override());
}

#[test]
fn both_macro_variants_retain_optional_url_and_profile_inputs() {
    for schema in [
        schemars::schema_for!(redis::PingInput),
        schemars::schema_for!(redis::AliasListInput),
    ] {
        let schema = serde_json::to_value(schema).unwrap();
        for field in ["url", "profile"] {
            assert_eq!(
                schema["properties"][field]["type"],
                json!(["string", "null"])
            );
            assert!(
                schema["required"]
                    .as_array()
                    .is_none_or(|required| !required.contains(&json!(field)))
            );
        }
    }
}
