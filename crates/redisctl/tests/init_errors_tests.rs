//! How `redisctl init` fails: argument parsing, exit codes, and error messages.
//!
//! Hermetic: temp directories and a pinned config file only; network contact is
//! limited to refused loopback ports.

mod init_common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

struct Project {
    dir: TempDir,
    skills: TempDir,
}

impl Project {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            skills: init_common::skills_fixture(),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// `redisctl --config-file <tmp> init --no-telemetry --agent claude ...`
    fn init(&self) -> Command {
        let mut cmd = Command::cargo_bin("redisctl").unwrap();
        for var in [
            "REDIS_CLOUD_API_KEY",
            "REDIS_CLOUD_SECRET_KEY",
            "REDIS_CLOUD_API_URL",
            "REDISCTL_PROFILE",
        ] {
            cmd.env_remove(var);
        }
        cmd.current_dir(self.path())
            .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
            .arg("--config-file")
            .arg(self.skills.path().join("config.toml"))
            .args([
                "init",
                "--no-telemetry",
                "--agent",
                "claude",
                "--skills-repo",
            ])
            .arg(self.skills.path())
            .arg("--no-install-cli");
        cmd
    }

    fn entries(&self) -> Vec<String> {
        std::fs::read_dir(self.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }
}

#[test]
fn a_mistyped_flag_is_a_usage_error_and_writes_nothing() {
    let project = Project::new();
    project
        .init()
        .args(["--dryrun", "--url", "redis://localhost:1", "--dry-run"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument '--dryrun'"))
        .stderr(predicate::str::contains("--dry-run"));
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn a_wrapper_forwarded_short_flag_is_a_usage_error() {
    let project = Project::new();
    project
        .init()
        .args(["-y", "--dry-run"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument '-y'"))
        .stderr(predicate::str::contains("no redis:// or rediss:// URL found").not());
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn a_stray_word_is_a_usage_error() {
    let project = Project::new();
    project
        .init()
        .args(["strayword", "--dry-run"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "unexpected positional argument found",
        ))
        .stderr(predicate::str::contains("no redis:// or rediss:// URL found").not());
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn a_stray_positional_is_never_echoed() {
    let project = Project::new();
    for args in [
        &["s3cret", "--dry-run"][..],
        &["--url", "redis://127.0.0.1:9", "s3cret", "--dry-run"],
    ] {
        let out = project.init().args(args).output().unwrap();
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(2), "{args:?}: {all}");
        assert!(!all.contains("s3cret"), "{args:?}: {all}");
        assert!(all.contains("see redisctl init --help"), "{args:?}: {all}");
    }
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn a_paste_without_a_url_never_echoes_its_password() {
    let project = Project::new();
    for paste in [
        "redis-cli -h host.example -p 12000 -a s3cret",
        "redis-cli -h host.example -p 12000 --user default --pass s3cret",
        "redis-cli -h host.example -p 12000 --password s3cret",
    ] {
        for args in [
            &[paste, "--dry-run"][..],
            &["--url", paste, "--dry-run"],
            &["-o", "json", paste, "--dry-run"],
        ] {
            let out = project.init().args(args).output().unwrap();
            let all = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.status.code(), Some(6), "{args:?}: {all}");
            assert!(!all.contains("s3cret"), "{args:?}: {all}");
            assert!(all.contains("redis-cli -u <url>"), "{args:?}: {all}");
        }
    }
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn cloud_with_a_pasted_url_is_a_usage_error_and_writes_nothing() {
    let project = Project::new();
    let flag = project
        .init()
        .args(["--cloud", "--url", "redis://localhost:6379"])
        .output()
        .unwrap();
    let pasted = project
        .init()
        .args(["--cloud", "redis://localhost:6379"])
        .output()
        .unwrap();
    assert_eq!(flag.status.code(), Some(2));
    assert_eq!(
        pasted.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&pasted.stderr)
    );
    assert!(
        String::from_utf8_lossy(&pasted.stderr)
            .contains("the argument '--cloud' cannot be used with '[REDIS_URL]...'"),
        "{}",
        String::from_utf8_lossy(&pasted.stderr)
    );
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn flags_after_an_unquoted_console_paste_still_apply() {
    let project = Project::new();
    project
        .init()
        .args([
            "redis-cli",
            "-u",
            "redis://default:S3cretPw@127.0.0.1:9",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run complete"))
        .stdout(predicate::str::contains("redis://default:****@127.0.0.1:9"))
        .stdout(predicate::str::contains("S3cretPw").not());
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn a_bare_url_or_quoted_paste_positional_is_accepted() {
    let project = Project::new();
    for paste in [
        "redis://127.0.0.1:9",
        "redis-cli -u redis://default:S3cretPw@127.0.0.1:9",
    ] {
        project
            .init()
            .args([paste, "--dry-run"])
            .assert()
            .success()
            .stdout(predicate::str::contains("via local Redis"))
            .stdout(predicate::str::contains("S3cretPw").not());
    }
}

#[test]
fn a_rejected_positional_is_not_echoed() {
    let project = Project::new();
    project
        .init()
        .args(["redisx://default:S3cretPw@h:1", "--dry-run"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("redisx://").not())
        .stderr(predicate::str::contains("S3cretPw").not());
}

#[test]
fn a_rejected_endpoint_flag_masks_the_password() {
    let project = Project::new();
    project
        .init()
        .args([
            "--langcache",
            "rediss://default:S3cretPw@h:1",
            "--cache",
            "c",
        ])
        .assert()
        .code(6)
        .stderr(predicate::str::contains(
            "--langcache takes the service endpoint",
        ))
        .stderr(predicate::str::contains("S3cretPw").not());
    project
        .init()
        .args([
            "-o",
            "json",
            "--langcache",
            "rediss://default:S3cretPw@h:1",
            "--cache",
            "c",
        ])
        .assert()
        .code(6)
        .stderr(predicate::str::contains(
            "--langcache takes the service endpoint",
        ))
        .stderr(predicate::str::contains("S3cretPw").not());
}

#[test]
fn a_rejected_endpoint_flag_never_echoes_its_value() {
    let project = Project::new();
    let paste = "redis-cli -a s3cret";
    for (flag, args) in [
        ("--langcache", &["--langcache", paste, "--cache", "c"][..]),
        (
            "--agent-memory",
            &["--agent-memory", paste, "--store", "s1"],
        ),
        ("--context-retriever", &["--context-retriever", paste]),
    ] {
        for json in [false, true] {
            let mut cmd = project.init();
            if json {
                cmd.args(["-o", "json"]);
            }
            let out = cmd.args(args).output().unwrap();
            let all = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.status.code(), Some(6), "{args:?}: {all}");
            assert!(!all.contains("s3cret"), "{args:?}: {all}");
            assert!(
                all.contains(&format!("{flag} takes the service endpoint")),
                "{args:?}: {all}"
            );
        }
    }
    assert!(project.entries().is_empty(), "{:?}", project.entries());
}

#[test]
fn init_input_errors_get_no_generic_tips() {
    let project = Project::new();
    for args in [
        &["--complete"][..],
        &["--store", "s1"],
        &["--url", "redis://127.0.0.1:9", "--api-key", "k"],
    ] {
        project
            .init()
            .args(args)
            .assert()
            .code(6)
            .stderr(predicate::str::contains("JSON/YAML").not())
            .stderr(predicate::str::contains("tip:").not());
    }
}

#[test]
fn cloud_without_a_sign_in_names_one_fix() {
    let project = Project::new();
    project
        .init()
        .arg("--cloud")
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "Sign in first: redisctl cloud auth login",
        ))
        .stderr(predicate::str::contains("redisctl profile set").not());
}

#[test]
fn a_dns_failure_gets_no_profile_tips() {
    let project = Project::new();
    project
        .init()
        .args(["--url", "redis://no-such-host.invalid:6379"])
        .assert()
        .code(10)
        .stderr(predicate::str::contains(
            "DNS record may still be propagating",
        ))
        .stderr(predicate::str::contains("redisctl profile").not());
}

#[cfg(unix)]
#[test]
fn an_unwritable_project_is_a_failure_naming_the_file_not_a_usage_error() {
    use std::os::unix::fs::PermissionsExt;

    struct RestoreWritable<'a>(&'a Path);
    impl Drop for RestoreWritable<'_> {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    let project = Project::new();
    std::fs::set_permissions(project.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = RestoreWritable(project.path());
    project
        .init()
        .args(["--url", "redis://127.0.0.1:9"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cannot write '.gitignore'"))
        .stderr(predicate::str::contains("tip:").not());
}
