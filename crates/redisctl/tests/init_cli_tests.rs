//! CLI-level tests for `redisctl init`: wiring, detection, dry-run, env contract.
//!
//! Everything here is hermetic: temp directories only, no Docker (the dry-run tests
//! pass --url so the database step never probes the Docker daemon), and network
//! contact limited to refused loopback connections.

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
fn dry_run_detects_a_node_project_and_plans_the_env_wiring() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"demo-shop","dependencies":{"express":"^4"}}"#,
    )
    .unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run", "--url", "redis://localhost:6379"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo-shop"))
        .stdout(predicate::str::contains("(node, npm, express)"))
        .stdout(predicate::str::contains("Plan"))
        .stdout(predicate::str::contains("created   .env"))
        .stdout(predicate::str::contains(".gitignore"))
        .stdout(predicate::str::contains("Dry run complete"));
    // Nothing written: the manifest is still the only file.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn unknown_runtime_gets_a_note_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run", "--url", "redis://localhost:6379"])
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
        ))
        // The generic file-format tips make no sense for a connection string.
        .stderr(predicate::str::contains("JSON/YAML").not());
}

#[test]
fn rejected_url_input_is_credential_masked_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "redisx://default:s3cret@host:6379"])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("redisx://default:****@host:6379"))
        .stderr(predicate::str::contains("s3cret").not());
}

#[test]
fn rejected_url_input_is_credential_masked_in_the_json_envelope() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "-o",
            "json",
            "--url",
            "redisx://default:s3cret@host:6379",
        ])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("****"))
        .stderr(predicate::str::contains("s3cret").not());
}

#[test]
fn verbatim_unquoted_console_paste_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    // The shell-split form of an unquoted paste: -u must not parse as a redisctl
    // flag. The URL points at a refused loopback port, so the run proceeds past
    // parsing, writes the contract, and fails at validation - proving both halves.
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "redis-cli",
            "-u",
            "redis://default:s3cret@127.0.0.1:9",
        ])
        .assert()
        .code(10)
        .stdout(predicate::str::contains("redis://default:****@127.0.0.1:9"))
        .stdout(predicate::str::contains("via provided URL"))
        .stdout(predicate::str::contains("s3cret").not())
        .stderr(predicate::str::contains("could not talk to Redis"))
        .stderr(predicate::str::contains("s3cret").not());
}

#[test]
fn dead_url_writes_env_then_fails_validation_with_the_stale_hint() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "redis://127.0.0.1:9"])
        .assert()
        .code(10)
        .stdout(predicate::str::contains("Validate"))
        .stderr(predicate::str::contains("could not talk to Redis"))
        .stderr(predicate::str::contains("remove REDIS_URL from .env"))
        // The message carries its own remedy; the profile-oriented connection tips
        // do not apply to init and must stay out.
        .stderr(predicate::str::contains("profile").not());
    // .env is written before validation, so the failure can name it and the user
    // can fix or remove the stale URL.
    let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(env.contains("REDIS_URL=\"redis://127.0.0.1:9\""), "{env}");
    let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(".env"), "{gitignore}");
}

#[test]
fn rerun_with_a_url_reports_env_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "REDIS_URL=\"redis://127.0.0.1:9\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join(".gitignore"), ".env\n").unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run", "--url", "redis://127.0.0.1:9"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unchanged .env"))
        .stdout(predicate::str::contains("unchanged .gitignore"));
}

#[test]
fn a_different_existing_redis_url_is_kept_not_clobbered() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".env"), "REDIS_URL=\"redis://keep-me:1\"\n").unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--url",
            "redis://default:s3cret@other:2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("kept"))
        .stdout(predicate::str::contains("left untouched"))
        // A short token here once collided with a random tempdir name printed in the
        // Project line; whole-output negative assertions need collision-proof tokens.
        .stdout(predicate::str::contains("s3cret").not());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        "REDIS_URL=\"redis://keep-me:1\"\n"
    );
}
