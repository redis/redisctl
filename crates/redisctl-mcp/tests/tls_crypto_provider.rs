use std::io::{Read, Write};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);

fn wait_with_output_timeout(mut child: Child, timeout: Duration) -> (Output, bool) {
    let mut child_stdout = child.stdout.take().expect("stdout should be piped");
    let stdout_reader = thread::spawn(move || {
        let mut stdout = Vec::new();
        child_stdout
            .read_to_end(&mut stdout)
            .expect("stdout should be readable");
        stdout
    });

    let mut child_stderr = child.stderr.take().expect("stderr should be piped");
    let stderr_reader = thread::spawn(move || {
        let mut stderr = Vec::new();
        child_stderr
            .read_to_end(&mut stderr)
            .expect("stderr should be readable");
        stderr
    });

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait().expect("process status should be readable") {
            Some(status) => break status,
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                if let Err(kill_error) = child.kill() {
                    match child.try_wait().expect("process status should be readable") {
                        Some(status) => break status,
                        None => panic!("failed to kill timed-out redisctl-mcp: {kill_error}"),
                    }
                }
                timed_out = true;
                break child.wait().expect("killed process should be waitable");
            }
        }
    };

    let stdout = stdout_reader
        .join()
        .expect("stdout reader should not panic");
    let stderr = stderr_reader
        .join()
        .expect("stderr reader should not panic");

    (
        Output {
            status,
            stdout,
            stderr,
        },
        timed_out,
    )
}

#[test]
fn tls_database_tool_returns_an_error_without_panicking() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_redisctl-mcp"))
        .args([
            "--database-url",
            "rediss://default:test@localhost:1/0",
            "--tools",
            "database:keys",
            "--log-level",
            "error",
        ])
        .env_remove("REDISCTL_MCP_POLICY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("redisctl-mcp should start");

    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"tls-regression-test","version":"1.0.0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"redis_type","arguments":{"key":"test"}}}"#,
    ]
    .join("\n");

    child
        .stdin
        .take()
        .expect("stdin should be piped")
        .write_all(format!("{requests}\n").as_bytes())
        .expect("MCP requests should be written");

    let (output, timed_out) = wait_with_output_timeout(child, PROCESS_TIMEOUT);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout:\n{stdout}\nstderr:\n{stderr}");

    assert!(
        !timed_out,
        "redisctl-mcp timed out after {PROCESS_TIMEOUT:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        output.status.success(),
        "redisctl-mcp should report the connection error without crashing\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("CryptoProvider") && !stderr.contains("no process-level CryptoProvider"),
        "redisctl-mcp should not panic while selecting a rustls crypto provider\nstderr:\n{stderr}"
    );

    let responses: Vec<Value> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let list_response = responses
        .iter()
        .find(|response| response.get("id") == Some(&Value::from(2)))
        .expect("tools/list should return a response");
    let tools = list_response
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .expect("tools/list should return a tools array");
    assert!(
        tools
            .iter()
            .any(|tool| tool.get("name") == Some(&Value::from("redis_type"))),
        "database:keys should expose redis_type\nresponse:\n{list_response}"
    );

    let call_response = responses
        .iter()
        .find(|response| response.get("id") == Some(&Value::from(3)))
        .expect("redis_type should return a response");
    assert!(
        call_response.get("error").is_none(),
        "redis_type should reach the tool rather than return a JSON-RPC protocol error\nresponse:\n{call_response}"
    );
    assert_eq!(
        call_response.pointer("/result/isError"),
        Some(&Value::Bool(true)),
        "redis_type should report the expected connection failure\nresponse:\n{call_response}"
    );
    let error_text = call_response
        .pointer("/result/content")
        .and_then(Value::as_array)
        .expect("tool error should contain MCP content")
        .iter()
        .filter_map(|content| content.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    assert!(
        error_text.contains("connection") || error_text.contains("connect"),
        "redis_type should report an ordinary connection failure\nresponse:\n{call_response}"
    );
}
