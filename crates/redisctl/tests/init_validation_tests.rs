//! Exercise the generated dotenv file and Redis validation through the CLI.
#![cfg(unix)]

use assert_cmd::Command;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

fn init(dir: &std::path::Path) -> Command {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in ["redis-cli", "docker", "npx"] {
        let path = bin.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.current_dir(dir)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .args(["init", "--agent", "claude"]);
    cmd
}

#[test]
fn generated_env_preserves_shell_metacharacters() {
    let dir = tempfile::tempdir().unwrap();
    let url = "redis://default:abc$REDISCTL_TEST_UNSET`printf-changed`@127.0.0.1:9";
    init(dir.path()).args(["--url", url]).assert().code(10);
    let output = std::process::Command::new("sh")
        .args(["-c", ". ./.env; printf '%s' \"$REDIS_URL\""])
        .env_remove("REDISCTL_TEST_UNSET")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), url);
    Command::new("node")
        .current_dir(dir.path())
        .env_remove("REDIS_URL")
        .args(["--env-file=.env", "-p", "process.env.REDIS_URL"])
        .assert()
        .success()
        .stdout(format!("{url}\n"));
    init(dir.path())
        .args(["--url", url, "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("unchanged .env"));
}

fn stalled_redis(command: &'static str) -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let count: usize = line.trim().strip_prefix('*').unwrap().parse().unwrap();
            let mut args = Vec::new();
            for _ in 0..count {
                line.clear();
                reader.read_line(&mut line).unwrap();
                line.clear();
                reader.read_line(&mut line).unwrap();
                args.push(line.trim().to_string());
            }
            if args[0].eq_ignore_ascii_case(command) {
                continue;
            }
            let reply = if args[0].eq_ignore_ascii_case("PING") {
                "+PONG\r\n"
            } else {
                "+OK\r\n"
            };
            if stream.write_all(reply.as_bytes()).is_err() {
                break;
            }
        }
    });
    port
}

#[test]
fn validation_times_out_when_ping_stalls() {
    assert_validation_timeout("PING");
}

#[test]
fn validation_times_out_when_set_stalls() {
    assert_validation_timeout("SET");
}

#[test]
fn validation_times_out_when_get_stalls() {
    assert_validation_timeout("GET");
}

fn assert_validation_timeout(command: &'static str) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("redis://127.0.0.1:{}", stalled_redis(command));
    init(dir.path())
        .args(["--url", &url])
        .timeout(Duration::from_secs(8))
        .assert()
        .code(10)
        .stdout(predicates::str::contains("timed out"));
}
