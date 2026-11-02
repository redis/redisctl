//! The local-database plan with a stub `docker` on PATH, so neither the Docker
//! daemon nor its state on this machine decides the outcome.
#![cfg(unix)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// `docker` answers `info` with `daemon_up`, knows no containers, and has the
/// preferred image locally.
fn stub_docker(bin: &Path, daemon_up: bool) {
    std::fs::create_dir_all(bin).unwrap();
    let info = if daemon_up { 0 } else { 1 };
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in\n  info) exit {info} ;;\n  image) exit 0 ;;\n  *) exit 1 ;;\nesac\n"
    );
    let path = bin.join("docker");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn init(project: &Path, home: &Path, bin: &Path) -> Command {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.current_dir(project)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("HOME", home)
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .arg("--config-file")
        .arg(home.join("config.toml"))
        .args([
            "init",
            "--no-telemetry",
            "--no-install-cli",
            "--agent",
            "claude",
            "--skills-repo",
        ])
        .arg(home.join("no-checkout"));
    cmd
}

fn planned_container(stdout: &str) -> String {
    let start = stdout.find("docker:").expect("a docker line") + "docker:".len();
    stdout[start..]
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn projects_with_the_same_folder_name_get_their_own_containers() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    stub_docker(&bin, true);
    let names: Vec<String> = ["a/api", "b/api"]
        .into_iter()
        .map(|rel| {
            let project = root.path().join(rel);
            std::fs::create_dir_all(&project).unwrap();
            let output = init(&project, root.path(), &bin)
                .arg("--dry-run")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            planned_container(&String::from_utf8(output.stdout).unwrap())
        })
        .collect();
    assert!(names[0].starts_with("redisctl-api-"), "{names:?}");
    assert_ne!(names[0], names[1]);

    // Stable per project: the same directory plans the same name again.
    let again = init(&root.path().join("a/api"), root.path(), &bin)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert_eq!(
        planned_container(&String::from_utf8(again.stdout).unwrap()),
        names[0]
    );
}

#[test]
fn dry_run_without_docker_shows_the_plan_and_the_real_run_explains() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("shop");
    std::fs::create_dir(&project).unwrap();
    let bin = root.path().join("bin");
    stub_docker(&bin, false);

    init(&project, root.path(), &bin)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("docker:redisctl-shop-"))
        .stdout(predicate::str::contains("Docker is not running"))
        .stdout(predicate::str::contains("Dry run complete"));
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);

    init(&project, root.path(), &bin)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Docker is not available"))
        .stderr(predicate::str::contains("--url"))
        .stderr(predicate::str::contains("--cloud"));
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
}
