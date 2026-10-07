//! Disposable output/overwrite contracts; no live Enterprise instance is needed.

#![cfg(feature = "enterprise")]

use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

const ORIGINAL: &[u8] = b"synthetic package: do not overwrite";

fn fixture() -> TempDir {
    let directory = TempDir::new().unwrap();
    std::fs::write(directory.path().join("config.toml"), "").unwrap();
    std::fs::write(directory.path().join("existing.tar.gz"), ORIGINAL).unwrap();
    directory
}

fn command(directory: &TempDir, kind: &str, format: &str) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .arg("--config-file")
        .arg(directory.path().join("config.toml"))
        .args(["--output", format, "enterprise", "support-package", kind])
        .arg("--file")
        .arg(directory.path().join("existing.tar.gz"))
        .env_remove("REDISCTL_PROFILE")
        .env_remove("RUST_LOG")
        .env_remove("COMPLETE")
        .timeout(Duration::from_secs(5));
    if kind == "database" {
        command.arg("42");
    }
    command
}

#[test]
fn overwrite_cancellation_preserves_structured_errors_and_existing_file() {
    let directory = fixture();
    for kind in ["cluster", "database", "node"] {
        for format in ["json", "yaml"] {
            for input in ["", "n\n", "y\n", "yes\n"] {
                let assertion = command(&directory, kind, format)
                    .write_stdin(input)
                    .assert()
                    .code(12)
                    .stdout(predicate::str::is_empty());
                let stderr = &assertion.get_output().stderr;
                let error: Value = if format == "json" {
                    serde_json::from_slice(stderr).unwrap()
                } else {
                    serde_yaml::from_slice(stderr).unwrap()
                };
                assert_eq!(error["error"]["code"], "cancelled");
                assert_eq!(error["error"]["exit_code"], 12);
                assert!(
                    error["error"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("existing.tar.gz")
                );
                // This command has no --force flag: cancellation advice must not invent one.
                assert!(!String::from_utf8_lossy(stderr).contains("--force"));
                assert_eq!(
                    std::fs::read(directory.path().join("existing.tar.gz")).unwrap(),
                    ORIGINAL
                );
            }
        }
    }
}

#[test]
fn overwrite_cancellation_is_a_human_diagnostic_without_piped_confirmation() {
    let directory = fixture();
    for kind in ["cluster", "database", "node"] {
        command(&directory, kind, "table")
            .write_stdin("n\n")
            .assert()
            .code(12)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("Cancelled"))
            .stderr(predicate::str::contains("Overwrite?").not());
    }
    assert_eq!(
        std::fs::read(directory.path().join("existing.tar.gz")).unwrap(),
        ORIGINAL
    );
}

#[test]
fn explicit_skip_checks_still_advances_to_profile_resolution() {
    let directory = fixture();
    for kind in ["cluster", "database", "node"] {
        let assertion = command(&directory, kind, "json")
            .arg("--skip-checks")
            .write_stdin("")
            .assert()
            .code(3)
            .stdout(predicate::str::is_empty());
        let error: Value = serde_json::from_slice(&assertion.get_output().stderr).unwrap();
        assert_eq!(error["error"]["code"], "no_profile_configured");
    }
    assert_eq!(
        std::fs::read(directory.path().join("existing.tar.gz")).unwrap(),
        ORIGINAL
    );
}
