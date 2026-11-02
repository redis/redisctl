//! Launch the generated MCP configuration with offline executable shims.
#![cfg(unix)]

mod init_common;

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
        // The server sends REDIS_PWD as-is, so it arrives decoded, as redis-rs reads it.
        (
            "redis://default:p%40ss%3Aw%2Frd@h:1",
            "redis://default@h:1",
            "p@ss:w/rd",
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
        // The image has no ENTRYPOINT: arguments after it replace its CMD, so the
        // launcher names the server script itself, on an image pinned by digest.
        let image = argv
            .iter()
            .position(|a| a.starts_with("mcp/redis@sha256:"))
            .unwrap_or_else(|| panic!("{argv:?}"));
        assert_eq!(
            argv[image + 1..image + 5],
            ["uv", "run", "redis-mcp-server", "--url"],
            "{argv:?}"
        );
        for secret in ["s3cret", "p%40ss", "p@ss"] {
            assert!(!argv.iter().any(|a| a.contains(secret)), "{argv:?}");
        }
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
        format!("REDIS_URL=redis://first:1\nexport REDIS_URL = '{url}'\n"),
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

    // A key set twice resolves to its last line, as application dotenv loaders read it.
    let twice = format!("REDIS_URL=redis://first:1\nREDIS_URL='{url}'\n");
    let (argv, _) = launch(&twice);
    // uvx runs an exact release, never whatever @latest resolves to at launch.
    let pinned = argv[1].strip_prefix("redis-mcp-server==").unwrap_or("");
    assert!(
        argv[0] == "--from"
            && pinned.split('.').count() == 3
            && pinned.split('.').all(|p| p.parse::<u32>().is_ok())
            && argv[2..4] == ["redis-mcp-server", "--url"],
        "{argv:?}"
    );
    let node = std::process::Command::new("node")
        .current_dir(dir.path())
        .env_remove("REDIS_URL")
        .args(["--env-file=.env", "-p", "process.env.REDIS_URL"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(node.stdout).unwrap().trim(), url);
    assert_eq!(
        argv.last().map(String::as_str),
        Some("redis://default@127.0.0.1:6380")
    );

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
        // Percent-decoded as redis-rs decodes the URL: every %XX becomes its byte,
        // anything else (a stray % included) stays.
        (
            "redis://default:p%40ss%3Aw%2Frd@h:1",
            "redis://default@h:1",
            "p@ss:w/rd",
        ),
        (
            "redis://default:s3c%C3%A9%7e%zz%4@h:1",
            "redis://default@h:1",
            "s3c\u{e9}~%zz%4",
        ),
    ] {
        let (argv, got) = launch(&format!("REDIS_URL='{url}'\n"));
        assert_eq!(argv.last().map(String::as_str), Some(argv_url), "{url}");
        assert_eq!(got, format!("REDIS_PWD={password}"), "{url}");
        for secret in ["s3c", "p%40ss", "p@ss"] {
            assert!(!argv.iter().any(|a| a.contains(secret)), "{argv:?}");
        }
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

/// Every launcher runs through `sh`. Without one on PATH (native Windows without
/// Git Bash's sh) the configs are still written, and init says the server cannot
/// start yet instead of promising the agent picks it up.
#[test]
fn without_sh_on_path_init_notes_the_launcher_cannot_start() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let repo = init_common::skills_fixture();
    // Only fakes on PATH: uvx is found, sh is not (the shebang is absolute).
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for name in ["uvx", "docker"] {
        let path = bin.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let url = format!("redis://127.0.0.1:{}", init_common::fake_redis());
    let note = "the MCP launcher runs through sh, which is not on PATH - on Windows use WSL or put Git Bash's sh on PATH";
    let init = |extra: &[&str]| {
        let mut cmd = Command::cargo_bin("redisctl").unwrap();
        cmd.current_dir(dir.path())
            .env("PATH", &bin)
            .env("HOME", home.path())
            .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
            .arg("--config-file")
            .arg(home.path().join("config.toml"))
            .args([
                "init",
                "--no-telemetry",
                "--no-install-cli",
                "--url",
                &url,
                "--agent",
                "claude",
                "--skills-repo",
            ])
            .arg(repo.path())
            .args(extra);
        cmd
    };

    init(&["--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(note))
        .stdout(predicate::str::is_match(r"created +\.mcp\.json  redis:").unwrap());
    assert!(!dir.path().join(".mcp.json").exists());

    init(&[])
        .assert()
        .success()
        .stdout(predicate::str::contains(note))
        .stdout(predicate::str::contains("it picks up the skills in"))
        .stdout(predicate::str::contains("redis MCP server").not());
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap())
            .unwrap();
    assert_eq!(config["mcpServers"]["redis"]["command"], "sh");
}

/// With sh on PATH the note stays away and the epilogue names the server.
#[test]
fn with_sh_on_path_the_epilogue_names_the_redis_mcp_server() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let repo = init_common::skills_fixture();
    let url = format!("redis://127.0.0.1:{}", init_common::fake_redis());
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
            &url,
            "--agent",
            "claude",
            "--skills-repo",
        ])
        .arg(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("runs through sh").not())
        .stdout(predicate::str::contains(
            "it picks up the redis MCP server and the skills",
        ));
}
