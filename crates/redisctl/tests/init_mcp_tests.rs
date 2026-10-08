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

/// The password crosses into the container as `-e REDIS_PWD`, never on a command
/// line that `ps` shows to every user on the machine.
#[test]
fn docker_mcp_rewrites_only_the_local_hostname() {
    for (url, expected, password) in [
        (
            "redis://default:s3cret@localhost:6379/2",
            "redis://default@host.docker.internal:6379/2",
            "s3cret",
        ),
        (
            "redis://:s3cret@127.0.0.1:6379",
            "redis://host.docker.internal:6379",
            "s3cret",
        ),
        (
            "redis://localhost.remote.example:6379",
            "redis://localhost.remote.example:6379",
            "",
        ),
        (
            "redis://localhost-password@remote.example:6379",
            "redis://localhost-password@remote.example:6379",
            "",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        for (name, source) in [
            (
                "docker",
                "#!/bin/sh\nif [ \"$1\" = run ]; then printf '%s\\n' \"$@\" \"REDIS_PWD=$REDIS_PWD\"; fi\n",
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
        let stdout = String::from_utf8(output.stdout).unwrap();
        let lines: Vec<&str> = stdout.lines().collect();
        let (argv, env) = lines.split_at(lines.len() - 1);
        assert_eq!(env, [format!("REDIS_PWD={password}")], "{url}");
        assert_eq!(argv.last(), Some(&expected), "{url}");
        assert!(
            argv.windows(2).any(|w| w == ["-e", "REDIS_PWD"]),
            "{argv:?}"
        );
        assert!(!argv.iter().any(|a| a.contains("s3cret")), "{argv:?}");
    }
}

/// The MCP client starts the server from the committed config; the URL it hands the
/// server must be the one init itself reads from `.env`, whatever else the file holds.
#[test]
fn mcp_launcher_reads_the_url_the_way_init_does() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for (name, source) in [
        (
            "uvx",
            "#!/bin/sh\nprintf '%s\\n' \"$@\" \"REDIS_PWD=$REDIS_PWD\"\n",
        ),
        ("docker", "#!/bin/sh\nexit 1\n"),
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
        .env("HOME", home.path())
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .arg("--config-file")
        .arg(home.path().join("config.toml"))
        .args([
            "init",
            "--no-telemetry",
            "--no-install-cli",
            "--url",
            "redis://127.0.0.1:9",
            "--agent",
            "claude",
            "--skills-repo",
        ])
        .arg(dir.path().join("no-checkout"))
        .assert()
        .code(10);
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap())
            .unwrap();
    let server = &config["mcpServers"]["redis"];

    let launch = |env: &str| {
        std::fs::write(dir.path().join(".env"), env).unwrap();
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
            .env_remove("REDIS_URL")
            .env("REDIS_PWD", "stray-from-the-shell")
            .output()
            .unwrap();
        assert!(output.status.success(), "{env:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let mut lines: Vec<String> = stdout.lines().map(str::to_string).collect();
        let password = lines.pop().unwrap();
        (lines, password)
    };

    let url = "redis://default:s3cret@127.0.0.1:6380";
    for env in [
        format!("TITLE=My App (dev)\nREDIS_URL=\"{url}\"\n"),
        format!("TITLE=My App (dev)\r\nREDIS_URL=\"{url}\"\r\n"),
        format!("REDIS_URL={url}\r\n"),
        format!("export REDIS_URL = '{url}'\nREDIS_URL=redis://second:1\n"),
        format!("NAME=$(whoami)\nREDIS_URL='{url}'\n"),
        format!("REDIS_URL=\"{url}\" # local\n"),
        format!("REDIS_URL={url} # local\r\n"),
    ] {
        let (argv, password) = launch(&env);
        assert_eq!(
            argv.last().map(String::as_str),
            Some("redis://default@127.0.0.1:6380"),
            "launcher reading of {env:?}"
        );
        assert_eq!(password, "REDIS_PWD=s3cret", "launcher reading of {env:?}");
    }

    // The password leaves the URL on the command line, split where `mask_url`
    // splits it, and travels in REDIS_PWD; a URL without one clears a stray value.
    for (url, argv_url, password) in [
        (
            "redis://default:s3c@ret@h:1",
            "redis://default@h:1",
            "s3c@ret",
        ),
        ("redis://:s3cret@h:1", "redis://h:1", "s3cret"),
        ("redis://localhost:6379", "redis://localhost:6379", ""),
        (
            "rediss://u:s3cret@h:1/0?ssl_cert_reqs=none",
            "rediss://u@h:1/0?ssl_cert_reqs=none",
            "s3cret",
        ),
    ] {
        let (argv, got) = launch(&format!("REDIS_URL='{url}'\n"));
        assert_eq!(argv.last().map(String::as_str), Some(argv_url), "{url}");
        assert_eq!(got, format!("REDIS_PWD={password}"), "{url}");
        assert!(!argv.iter().any(|a| a.contains("s3c")), "{argv:?}");
    }
}

#[test]
fn editing_mcp_json_keeps_the_users_key_order() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mcp.json"),
        r#"{"mcpServers":{"zeta":{"command":"z","args":[]}},"alpha":true}"#,
    )
    .unwrap();
    Command::cargo_bin("redisctl")
        .unwrap()
        .current_dir(dir.path())
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .arg("--config-file")
        .arg(home.path().join("config.toml"))
        .args([
            "init",
            "--no-telemetry",
            "--no-install-cli",
            "--url",
            "redis://127.0.0.1:9",
            "--agent",
            "claude",
            "--skills-repo",
        ])
        .arg(dir.path().join("no-checkout"))
        .assert()
        .code(10);
    // Raw text, not a parsed Value: parsing would hide the order on disk.
    let text = std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
    let at = |needle: &str, from: usize| {
        from + text[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} missing:\n{text}"))
    };
    assert!(at("\"mcpServers\"", 0) < at("\"alpha\"", 0), "{text}");
    for server in ["\"zeta\"", "\"redis\""] {
        let start = at(server, 0);
        assert!(at("\"command\"", start) < at("\"args\"", start), "{text}");
    }
}
