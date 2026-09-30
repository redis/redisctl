//! The `.env` contract under awkward real-world input: passwords with raw `@`,
//! CRLF files, `export` prefixes, blank lines. Hermetic: temp directories only,
//! dry runs or refused loopback ports, pinned agent, config, HOME and skills repo.

mod init_common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

struct Sandbox {
    project: TempDir,
    home: TempDir,
    repo: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            project: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
            repo: init_common::skills_fixture(),
        }
    }

    fn init(&self, extra: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("redisctl").unwrap();
        cmd.current_dir(self.project.path())
            .env("HOME", self.home.path())
            .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
            .env("REDISCTL_INIT_SKILLS_REPO", self.repo.path())
            .arg("--config-file")
            .arg(self.home.path().join("config.toml"))
            .args([
                "init",
                "--no-telemetry",
                "--no-install-cli",
                "--agent",
                "claude",
            ])
            .args(extra);
        cmd
    }

    fn write_env(&self, content: &str) {
        std::fs::write(self.project.path().join(".env"), content).unwrap();
    }

    fn read_env(&self) -> String {
        std::fs::read_to_string(self.project.path().join(".env")).unwrap()
    }
}

#[test]
fn a_password_with_a_raw_at_sign_is_fully_masked_in_the_plan() {
    let sandbox = Sandbox::new();
    for output in ["auto", "json"] {
        sandbox
            .init(&[
                "-o",
                output,
                "--dry-run",
                "--url",
                "redis://default:ab@cdS3cretPw@localhost:1",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("redis://default:****@localhost:1"))
            .stdout(predicate::str::contains("S3cretPw").not())
            .stderr(predicate::str::contains("S3cretPw").not());
    }
}

#[test]
fn a_replaced_env_url_with_a_raw_at_sign_is_fully_masked_in_the_note() {
    let sandbox = Sandbox::new();
    sandbox.write_env("REDIS_URL=\"redis://u:x@x0ldS3cret@h:1\"\n");
    sandbox
        .init(&["--dry-run", "--url", "redis://127.0.0.1:9"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "REDIS_URL replaced (was redis://u:****@h:1)",
        ))
        .stdout(predicate::str::contains("0ldS3cret").not());
}

#[test]
fn a_password_with_a_raw_at_sign_connects_to_the_host_after_the_last_at() {
    let sandbox = Sandbox::new();
    let port = init_common::fake_redis();
    let url = format!("redis://default:ab@cdS3cretPw@127.0.0.1:{port}");
    sandbox
        .init(&["--url", &url])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "redis://default:****@127.0.0.1:{port}"
        )))
        .stdout(predicate::str::contains("S3cretPw").not());
}

#[test]
fn replacing_redis_url_changes_only_its_own_line() {
    let sandbox = Sandbox::new();
    sandbox.write_env("A=1\r\n\r\nexport REDIS_URL=redis://old:1\r\n\r\nB=2\r\n");
    // A refused loopback port: the contract is written, then validation fails.
    sandbox
        .init(&["--url", "redis://127.0.0.1:9"])
        .assert()
        .code(10);
    assert_eq!(
        sandbox.read_env(),
        "A=1\r\n\r\nexport REDIS_URL=\"redis://127.0.0.1:9\"\r\n\r\nB=2\r\n"
    );
}
