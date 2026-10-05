//! Exercise the installed-binary shape: no skills directory or repository working directory.

use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout, Command};

async fn request(
    stdin: &mut ChildStdin,
    stdout: &mut Lines<BufReader<ChildStdout>>,
    id: u32,
    method: &str,
    params: Value,
) -> Value {
    stdin
        .write_all(
            format!(
                "{}\n",
                json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let line = stdout
                .next_line()
                .await
                .unwrap()
                .expect("MCP stdout ended early");
            // Every stdout line must be protocol JSON, never tracing output.
            let response: Value = serde_json::from_str(&line).expect("non-JSON MCP stdout");
            if response["id"] == id {
                break response;
            }
        }
    })
    .await
    .expect("MCP request timed out");
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

#[tokio::test]
async fn copied_binary_discovers_reads_and_prompts_for_embedded_setup() {
    let directory = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_BIN_EXE_redisctl-mcp"));
    let executable = directory.path().join(source.file_name().unwrap());
    std::fs::copy(source, &executable).unwrap();
    let policy = directory.path().join("read-only.toml");
    std::fs::write(&policy, "tier = \"read-only\"\n").unwrap();
    assert!(!directory.path().join("skills").exists());

    let mut child = Command::new(executable)
        .args(["--tools", "app", "--log-level", "off", "--policy"])
        .arg(policy)
        .current_dir(directory.path())
        .env_remove("REDISCTL_PROFILE")
        .env_remove("REDIS_URL")
        .env_remove("REDIS_CLUSTER")
        .env_remove("REDIS_CLIENT_NAME")
        .env_remove("REDISCTL_MCP_SKILLS_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let init = request(
        &mut stdin,
        &mut stdout,
        1,
        "initialize",
        json!({
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "installed-skills-test", "version": "1"}
        }),
    )
    .await;
    let instructions = init["instructions"].as_str().unwrap();
    assert!(instructions.len() < 2048);
    assert!(instructions.contains("redisctl://skills/redisctl-setup"));
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();

    let listed = request(&mut stdin, &mut stdout, 2, "resources/list", json!({})).await;
    assert!(
        listed["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["uri"] == "redisctl://skills")
    );
    let index = request(
        &mut stdin,
        &mut stdout,
        3,
        "resources/read",
        json!({"uri": "redisctl://skills"}),
    )
    .await;
    let index: Value =
        serde_json::from_str(index["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(index["skills"].as_array().unwrap().len(), 11);
    assert!(
        !index["available_tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "profile_create")
    );
    let setup = request(
        &mut stdin,
        &mut stdout,
        4,
        "resources/read",
        json!({"uri": "redisctl://skills/redisctl-setup"}),
    )
    .await;
    assert!(
        setup["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("redisctl profile init")
    );
    let prompts = request(&mut stdin, &mut stdout, 5, "prompts/list", json!({})).await;
    assert!(
        prompts["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "redisctl-setup")
    );
    let prompt = request(
        &mut stdin,
        &mut stdout,
        6,
        "prompts/get",
        json!({"name": "redisctl-setup", "arguments": {}}),
    )
    .await;
    assert!(prompt.to_string().contains("redisctl profile init"));

    drop(stdin);
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());
}
