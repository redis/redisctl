//! CLI-level tests for `redisctl init`: wiring, detection, dry-run.
//!
//! Everything here runs against a temp directory and writes nothing.

use assert_cmd::Command;
use predicates::prelude::*;

fn redisctl() -> Command {
    Command::cargo_bin("redisctl").unwrap()
}

#[test]
fn init_is_hidden_from_top_level_help() {
    redisctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init").not());
}

#[test]
fn init_help_documents_the_flags() {
    redisctl()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--url"))
        .stdout(predicate::str::contains("--agent"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn unknown_agent_is_a_usage_error() {
    redisctl()
        .args(["init", "--agent", "bogus"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("possible values"));
}

#[test]
fn dry_run_detects_a_node_project() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"demo-shop","dependencies":{"express":"^4"}}"#,
    )
    .unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo-shop"))
        .stdout(predicate::str::contains("(node, npm, express)"))
        .stdout(predicate::str::contains("Plan"))
        .stdout(predicate::str::contains("Dry run complete"));
    // Nothing written: the manifest is still the only file.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn unknown_runtime_gets_a_note_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no package manifest detected"));
}

#[test]
fn provided_url_is_masked_in_the_plan_subject() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--url",
            "redis-cli -u redis://default:s3cret@host.example:12000",
            "--name",
            "my-db",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "redis://default:****@host.example:12000",
        ))
        .stdout(predicate::str::contains("[my-db]"))
        .stdout(predicate::str::contains("s3cret").not());
}

#[test]
fn url_without_a_redis_url_is_a_validation_error() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "http://not-redis"])
        .assert()
        .code(6)
        .stderr(predicate::str::contains(
            "no redis:// or rediss:// URL found",
        ));
}
