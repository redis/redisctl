#![cfg(feature = "database")]
//! `redis_config_set` must refuse protected parameters before any CONFIG command is sent.
//! Uses a credential-free loopback listener; no Docker required.

mod support;

use std::time::Duration;

use redisctl_mcp::tools::redis;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Accept one connection, acknowledge any handshake commands, and record whether a
/// CONFIG command ever arrives.
async fn run_guarded_call(parameter: &str) -> (tower_mcp::CallToolResult, bool) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let Ok(Ok((mut socket, _))) =
            tokio::time::timeout(Duration::from_secs(2), listener.accept()).await
        else {
            return false;
        };
        let mut saw_config = false;
        let mut buffer = [0u8; 4096];
        // Reply +OK to every handshake write until the client closes the connection.
        while let Ok(Ok(read)) =
            tokio::time::timeout(Duration::from_millis(500), socket.read(&mut buffer)).await
        {
            if read == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&buffer[..read]).to_ascii_uppercase();
            if text.contains("CONFIG") {
                saw_config = true;
            }
            let _ = socket.write_all(b"+OK\r\n+OK\r\n").await;
        }
        saw_config
    });

    let state = support::database_state_write(format!("redis://{addr}"));
    let tool = redis::config_set(state.clone());
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tool.call(json!({"parameter": parameter, "value": "x"})),
    )
    .await
    .expect("tool call must complete");
    drop(tool);
    drop(state);
    let saw_config = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("synthetic server must finish")
        .unwrap();
    (result, saw_config)
}

#[tokio::test]
async fn config_set_refuses_requirepass_without_sending_config() {
    for parameter in ["requirepass", "  RequirePass "] {
        let (result, saw_config) = run_guarded_call(parameter).await;
        assert!(result.is_error, "expected tool error: {result:?}");
        let text = result.content[0].as_text().unwrap_or_default().to_string();
        assert!(
            text.contains("config parameter 'requirepass' is blocked for safety"),
            "unexpected error: {text}"
        );
        assert!(!saw_config, "CONFIG must never reach the server");
    }
}
