//! Launch the generated MCP configuration with offline executable shims.
#![cfg(unix)]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

#[test]
fn replacing_mcp_never_prints_existing_arguments() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".mcp.json"), json!({
        "mcpServers": { "redis": { "command": "redis-mcp-server", "args": ["--password", "s3cret-existing-password"] } }
    }).to_string()).unwrap();
    Command::cargo_bin("redisctl")
        .unwrap()
        .current_dir(dir.path())
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .args([
            "init",
            "--dry-run",
            "--url",
            "redis://127.0.0.1:9",
            "--agent",
            "claude",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("replaced existing redis server"))
        .stdout(predicate::str::contains("s3cret-existing-password").not())
        .stderr(predicate::str::contains("s3cret-existing-password").not());
}

#[test]
fn docker_mcp_rewrites_only_the_local_hostname() {
    for (url, expected) in [
        (
            "redis://default:s3cret@localhost:6379/2",
            "redis://default:s3cret@host.docker.internal:6379/2",
        ),
        (
            "redis://:s3cret@127.0.0.1:6379",
            "redis://:s3cret@host.docker.internal:6379",
        ),
        (
            "redis://localhost.remote.example:6379",
            "redis://localhost.remote.example:6379",
        ),
        (
            "redis://localhost-password@remote.example:6379",
            "redis://localhost-password@remote.example:6379",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        for (name, source) in [
            (
                "docker",
                "#!/bin/sh\nif [ \"$1\" = run ]; then printf '%s\\n' \"$@\"; fi\n",
            ),
            ("npx", "#!/bin/sh\nexit 1\n"),
            ("redis-cli", "#!/bin/sh\nexit 0\n"),
        ] {
            let path = bin.join(name);
            std::fs::write(&path, source).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path_env = format!("{}:/usr/bin:/bin", bin.display());
        Command::cargo_bin("redisctl")
            .unwrap()
            .current_dir(dir.path())
            .env("PATH", &path_env)
            .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
            .args(["init", "--url", "redis://127.0.0.1:9", "--agent", "claude"])
            .assert()
            .code(10);
        std::fs::write(dir.path().join(".env"), format!("REDIS_URL='{url}'\n")).unwrap();
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap())
                .unwrap();
        let server = &config["mcpServers"]["redis"];
        let output = std::process::Command::new(server["command"].as_str().unwrap())
            .args(
                server["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(dir.path())
            .env("PATH", &path_env)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().lines().last(),
            Some(expected)
        );
    }
}
