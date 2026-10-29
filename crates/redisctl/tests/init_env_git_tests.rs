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
        self.track(".env");
    }

    /// A repository tracking `rel`, isolated from the machine's git config.
    fn track(&self, rel: &str) {
        for args in [&["init", "-q"][..], &["add", rel]] {
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

/// init writes `.env` and reads `.gitignore` through whatever they link to, so a
/// layout that would put the password in a committed file, or leave `.env`
/// unignored, is refused before anything is written. Dry run and apply agree.
#[cfg(unix)]
#[test]
fn a_secret_path_symlink_is_refused_before_any_write() {
    use std::os::unix::fs::symlink;
    if git_missing() {
        return;
    }
    let example: fn(&Sandbox) = |sandbox| {
        sandbox.write(".env.example", "REDIS_URL=redis://localhost:6379\n");
        symlink(".env.example", sandbox.project.path().join(".env")).unwrap();
    };
    let tracked: fn(&Sandbox) = |sandbox| {
        sandbox.write(".env.development", "APP_NAME=demo\n");
        symlink(".env.development", sandbox.project.path().join(".env")).unwrap();
        sandbox.track(".env.development");
    };
    let dangling: fn(&Sandbox) = |sandbox| {
        symlink(
            "gitignore.shared",
            sandbox.project.path().join(".gitignore"),
        )
        .unwrap();
        sandbox.track(".gitignore");
    };
    let url = "redis://default:s3cret@127.0.0.1:9";
    for (setup, refusal) in [
        (example, ".env is a symlink to .env.example"),
        (tracked, ".env is a symlink to .env.development"),
        (dangling, ".gitignore is a symlink to gitignore.shared"),
    ] {
        for args in [vec!["--dry-run", "--url", url], vec!["--url", url]] {
            let sandbox = Sandbox::new();
            setup(&sandbox);
            let snapshot = |sandbox: &Sandbox| {
                [".env.example", ".env.development", "gitignore.shared"]
                    .map(|rel| std::fs::read_to_string(sandbox.project.path().join(rel)).ok())
            };
            let before = snapshot(&sandbox);
            sandbox
                .init(&args)
                .assert()
                .code(1)
                .stderr(predicate::str::contains(refusal))
                .stderr(predicate::str::contains(
                    "Replace the link with a regular file",
                ))
                .stdout(predicate::str::contains("s3cret").not())
                .stderr(predicate::str::contains("s3cret").not());
            assert_eq!(snapshot(&sandbox), before, "{refusal}: {args:?}");
            assert!(
                !sandbox.project.path().join(".mcp.json").exists(),
                "{refusal}: {args:?}"
            );
        }
    }
}

/// A hard link (or `.env.example` linking to `.env`) makes the two one file: the
/// example would hold the credentials, or `.env` the placeholder.
#[cfg(unix)]
#[test]
fn env_and_env_example_as_one_file_are_refused() {
    let sandbox = Sandbox::new();
    sandbox.write(".env.example", "REDIS_URL=redis://localhost:6379\n");
    std::fs::hard_link(
        sandbox.project.path().join(".env.example"),
        sandbox.project.path().join(".env"),
    )
    .unwrap();
    sandbox
        .init(&["--url", "redis://default:s3cret@127.0.0.1:9"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            ".env and .env.example are the same file",
        ));
    assert_eq!(
        sandbox.read(".env.example"),
        "REDIS_URL=redis://localhost:6379\n"
    );
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
