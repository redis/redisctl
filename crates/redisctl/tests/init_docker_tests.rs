//! Live Docker suite for `redisctl init` local provisioning.
//!
//! Requires a running Docker daemon; each test provisions (and removes) a container
//! named after its project directory. Run with:
//!
//! ```bash
//! cargo test --test init_docker_tests -- --ignored --nocapture
//! ```

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::path::{Path, PathBuf};

fn redisctl() -> Command {
    Command::cargo_bin("redisctl").unwrap()
}

/// Remove the test's container even when an assertion panics.
struct RemoveContainer(String);

impl Drop for RemoveContainer {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.0])
            .output();
    }
}

/// A project directory whose basename is alphanumeric, so the container name the
/// command derives from it is exactly `redis-init-<basename>`.
fn project_dir(tmp: &Path, suffix: &str) -> (PathBuf, String, RemoveContainer) {
    let basename = format!("live{}{}", std::process::id(), suffix);
    let dir = tmp.join(&basename);
    std::fs::create_dir(&dir).unwrap();
    let container = format!("redis-init-{basename}");
    (dir.clone(), container.clone(), RemoveContainer(container))
}

fn container_running(name: &str) -> Option<bool> {
    let out = std::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", name])
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim() == "true")
}

#[test]
#[ignore = "requires Docker"]
#[serial]
fn full_run_provisions_validates_and_rerun_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, container, _cleanup) = project_dir(tmp.path(), "full");
    std::fs::write(dir.join("package.json"), r#"{"name":"live-full"}"#).unwrap();

    redisctl()
        .current_dir(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(&container))
        .stdout(predicate::str::contains("✓ PING  ✓ SET/GET"));
    let env = std::fs::read_to_string(dir.join(".env")).unwrap();
    assert!(env.contains("REDIS_URL=\"redis://localhost:"), "{env}");
    let gitignore = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(gitignore.contains(".env"), "{gitignore}");
    assert_eq!(container_running(&container), Some(true));

    // Idempotent re-run: nothing changes, validation still green.
    redisctl()
        .current_dir(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("unchanged .env"))
        .stdout(predicate::str::contains("unchanged .gitignore"))
        .stdout(predicate::str::contains("✓ PING  ✓ SET/GET"));
    assert_eq!(std::fs::read_to_string(dir.join(".env")).unwrap(), env);
}

#[test]
#[ignore = "requires Docker"]
#[serial]
fn stopped_container_is_restarted() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, container, _cleanup) = project_dir(tmp.path(), "stop");

    redisctl().current_dir(&dir).arg("init").assert().success();
    let out = std::process::Command::new("docker")
        .args(["stop", &container])
        .output()
        .unwrap();
    assert!(out.status.success());

    redisctl()
        .current_dir(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("restarted stopped container"))
        .stdout(predicate::str::contains("✓ PING  ✓ SET/GET"));
    assert_eq!(container_running(&container), Some(true));
}

#[test]
#[ignore = "requires Docker"]
#[serial]
fn dry_run_plans_the_container_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, container, _cleanup) = project_dir(tmp.path(), "dry");

    redisctl()
        .current_dir(&dir)
        .args(["init", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "would run: docker run -d --name {container}"
        )))
        .stdout(predicate::str::contains("Dry run complete"));
    assert!(!dir.join(".env").exists());
    assert!(!dir.join(".gitignore").exists());
    assert_eq!(container_running(&container), None);
}
