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
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    // Explicitly off: a key in the developer's (or CI's) environment must not
    // make test runs send telemetry.
    cmd.env("REDISCTL_INIT_AMPLITUDE_KEY", "");
    cmd
}

/// A fake redis/agent-skills checkout keeps the skills step offline; this suite
/// needs Docker only.
fn skills_fixture() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let skill = repo.path().join("skills/redis-basics");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "# basics\n").unwrap();
    repo
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
    // Deliberately no manifest: a package.json here would make the run perform a
    // real npm install, and this suite needs Docker only.
    let (dir, container, _cleanup) = project_dir(tmp.path(), "full");

    let repo = skills_fixture();
    redisctl()
        .current_dir(&dir)
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--no-install-cli", "--agent", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&container))
        // Solo-Claude checkout copies land in Claude's own skills dir.
        .stdout(predicate::str::contains(".claude/skills/redis-basics/"))
        .stdout(predicate::str::contains(
            ".agents/skills/redis-project-setup/SKILL.md",
        ))
        .stdout(predicate::str::contains("✓ PING  ✓ SET/GET"))
        .stdout(predicate::str::contains("Next steps"));
    let mcp = std::fs::read_to_string(dir.join(".mcp.json")).unwrap();
    assert!(!mcp.contains("redis://"), "credential-free: {mcp}");
    let env = std::fs::read_to_string(dir.join(".env")).unwrap();
    assert!(env.contains("REDIS_URL=\"redis://localhost:"), "{env}");
    let gitignore = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(gitignore.contains(".env"), "{gitignore}");
    assert_eq!(container_running(&container), Some(true));

    // Idempotent re-run: nothing changes, validation still green.
    redisctl()
        .current_dir(&dir)
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--no-install-cli", "--agent", "claude"])
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

    let repo = skills_fixture();
    redisctl()
        .current_dir(&dir)
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--no-install-cli"])
        .assert()
        .success();
    let out = std::process::Command::new("docker")
        .args(["stop", &container])
        .output()
        .unwrap();
    assert!(out.status.success());

    redisctl()
        .current_dir(&dir)
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--no-install-cli"])
        .assert()
        .success()
        .stdout(predicate::str::contains("restarted stopped container"))
        .stdout(predicate::str::contains("✓ PING  ✓ SET/GET"));
    assert_eq!(container_running(&container), Some(true));
}

#[test]
#[ignore = "requires Docker"]
#[serial]
fn restart_that_never_serves_redis_fails_instead_of_reporting_updated() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, container, _cleanup) = project_dir(tmp.path(), "dead");

    // A stopped container that starts fine but never serves Redis: the image's
    // entrypoint runs `sleep` instead of redis-server.
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let out = std::process::Command::new("docker")
        .args([
            "create",
            "--name",
            &container,
            "-p",
            &format!("127.0.0.1:{port}:6379"),
            "redis:8-alpine",
            "sleep",
            "300",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::write(
        dir.join(".env"),
        format!("REDIS_URL=\"redis://localhost:{port}\"\n"),
    )
    .unwrap();

    let repo = skills_fixture();
    redisctl()
        .current_dir(&dir)
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--no-install-cli"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("restarted stopped container").not())
        .stderr(predicate::str::contains("did not become ready"));
    // The start itself succeeded; the failure is Redis never answering.
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
