//! A credential never lands in a `.env` git would commit. Hermetic: temp
//! directories, a throwaway git repository with no global or system config, dry
//! runs or refused loopback ports, pinned agent, config, HOME and skills repo.

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

    fn write(&self, rel: &str, content: &str) {
        std::fs::write(self.project.path().join(rel), content).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.project.path().join(rel)).unwrap()
    }

    /// A repository tracking `.env`, isolated from the machine's git config.
    fn track_env(&self) {
        for args in [&["init", "-q"][..], &["add", ".env"]] {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(self.project.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("HOME", self.home.path())
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
    }
}

fn git_missing() -> bool {
    let missing = std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err();
    if missing {
        eprintln!("skipping: git is not installed");
    }
    missing
}

#[test]
fn a_secret_is_never_written_into_a_tracked_env() {
    if git_missing() {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.write(".env", "APP_NAME=demo\n");
    sandbox.track_env();
    let url = "redis://default:s3cret@127.0.0.1:9";
    let product = [
        "--langcache",
        "https://127.0.0.1:9",
        "--cache",
        "c1",
        "--api-key",
        "S3cretKey",
    ];
    for args in [
        vec!["--dry-run", "--url", url],
        vec!["--url", url],
        [&["--dry-run"][..], &product].concat(),
    ] {
        sandbox
            .init(&args)
            .assert()
            .code(1)
            .stderr(predicate::str::contains(".env is tracked by git"))
            .stderr(predicate::str::contains("git rm --cached .env"))
            .stdout(predicate::str::contains("s3cret").not())
            .stderr(predicate::str::contains("s3cret").not())
            .stdout(predicate::str::contains("S3cretKey").not())
            .stderr(predicate::str::contains("S3cretKey").not());
        assert_eq!(sandbox.read(".env"), "APP_NAME=demo\n", "{args:?}");
    }
}

#[test]
fn a_secret_free_url_still_lands_in_a_tracked_env() {
    if git_missing() {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.write(".env", "APP_NAME=demo\n");
    sandbox.track_env();
    // A refused loopback port: the files are written, then validation fails.
    sandbox
        .init(&["--url", "redis://127.0.0.1:9"])
        .assert()
        .code(10);
    assert!(
        sandbox
            .read(".env")
            .contains("REDIS_URL=\"redis://127.0.0.1:9\""),
        "{}",
        sandbox.read(".env")
    );
}

#[cfg(unix)]
#[test]
fn gitignore_covers_env_before_a_later_write_fails() {
    use std::os::unix::fs::PermissionsExt;
    let sandbox = Sandbox::new();
    sandbox.write(".env.example", "APP_NAME=\n");
    std::fs::set_permissions(
        sandbox.project.path().join(".env.example"),
        std::fs::Permissions::from_mode(0o444),
    )
    .unwrap();
    sandbox
        .init(&["--url", "redis://default:s3cret@127.0.0.1:9"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cannot write '.env.example'"));
    assert!(
        sandbox.project.path().join(".gitignore").exists(),
        ".env holds the password but .gitignore was never written"
    );
    assert!(
        sandbox
            .read(".gitignore")
            .lines()
            .any(|line| line == ".env"),
        "{}",
        sandbox.read(".gitignore")
    );
}
