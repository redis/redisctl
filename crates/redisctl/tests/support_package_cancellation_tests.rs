//! Disposable output/overwrite contracts; no live Enterprise instance is needed.

#![cfg(feature = "enterprise")]

use std::path::Path;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ORIGINAL: &[u8] = b"synthetic package: do not overwrite";

fn fixture() -> TempDir {
    let directory = TempDir::new().unwrap();
    std::fs::write(directory.path().join("config.toml"), "").unwrap();
    std::fs::write(directory.path().join("existing.tar.gz"), ORIGINAL).unwrap();
    directory
}

fn command(directory: &TempDir, kind: &str, format: &str) -> Command {
    command_to(
        directory,
        kind,
        format,
        &directory.path().join("existing.tar.gz"),
    )
}

fn command_to(directory: &TempDir, kind: &str, format: &str, output: &Path) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .arg("--config-file")
        .arg(directory.path().join("config.toml"))
        .args(["--output", format, "enterprise", "support-package", kind])
        .arg("--file")
        .arg(output)
        .env_remove("REDISCTL_PROFILE")
        .env_remove("REDIS_ENTERPRISE_URL")
        .env_remove("REDIS_ENTERPRISE_USER")
        .env_remove("REDIS_ENTERPRISE_PASSWORD")
        .env_remove("REDIS_ENTERPRISE_INSECURE")
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
                    serde_norway::from_slice(stderr).unwrap()
                };
                assert_eq!(error["error"]["code"], "cancelled");
                assert_eq!(error["error"]["exit_code"], 12);
                assert!(
                    error["error"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("existing.tar.gz")
                );
                assert!(String::from_utf8_lossy(stderr).contains("--force"));
                assert!(String::from_utf8_lossy(stderr).contains("--skip-checks"));
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

#[test]
fn explicit_force_skips_only_confirmation_and_preserves_structured_errors() {
    let directory = fixture();
    for kind in ["cluster", "database", "node"] {
        for format in ["json", "yaml"] {
            for input in ["", "y\n"] {
                let assertion = command(&directory, kind, format)
                    .arg("--force")
                    .write_stdin(input)
                    .assert()
                    .code(3)
                    .stdout(predicate::str::is_empty());
                let stderr = &assertion.get_output().stderr;
                let error: Value = if format == "json" {
                    serde_json::from_slice(stderr).unwrap()
                } else {
                    serde_norway::from_slice(stderr).unwrap()
                };
                assert_eq!(error["error"]["code"], "no_profile_configured");
                assert!(!String::from_utf8_lossy(stderr).contains("Overwrite?"));
                assert_eq!(
                    std::fs::read(directory.path().join("existing.tar.gz")).unwrap(),
                    ORIGINAL
                );
            }
        }
    }
}

#[test]
fn force_keeps_missing_output_parent_check_before_profile_resolution() {
    let directory = fixture();
    let output = directory.path().join("missing-parent/new.tar.gz");
    for kind in ["cluster", "database", "node"] {
        for format in ["json", "yaml"] {
            let assertion = command_to(&directory, kind, format, &output)
                .arg("--force")
                .write_stdin("")
                .assert()
                .code(1)
                .stdout(predicate::str::is_empty());
            let stderr = &assertion.get_output().stderr;
            let error: Value = if format == "json" {
                serde_json::from_slice(stderr).unwrap()
            } else {
                serde_norway::from_slice(stderr).unwrap()
            };
            assert!(
                error["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Output directory")
            );
            assert!(
                error["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("does not exist")
            );
            assert!(!output.exists());
        }
    }
}

#[test]
fn skip_checks_retains_broader_bypass_with_or_without_force() {
    let directory = fixture();
    let output = directory.path().join("missing-parent/new.tar.gz");
    for kind in ["cluster", "database", "node"] {
        for format in ["json", "yaml"] {
            for force in [false, true] {
                let mut command = command_to(&directory, kind, format, &output);
                command.arg("--skip-checks");
                if force {
                    command.arg("--force");
                }
                let assertion = command
                    .write_stdin("")
                    .assert()
                    .code(3)
                    .stdout(predicate::str::is_empty());
                let stderr = &assertion.get_output().stderr;
                let error: Value = if format == "json" {
                    serde_json::from_slice(stderr).unwrap()
                } else {
                    serde_norway::from_slice(stderr).unwrap()
                };
                assert_eq!(error["error"]["code"], "no_profile_configured");
                assert!(!output.exists());
            }
        }
    }
}

#[test]
fn new_output_path_does_not_prefix_structured_error_with_human_warning() {
    let directory = fixture();
    let output = directory.path().join("new.tar.gz");
    for kind in ["cluster", "database", "node"] {
        for format in ["json", "yaml"] {
            let assertion = command_to(&directory, kind, format, &output)
                .write_stdin("")
                .assert()
                .code(3)
                .stdout(predicate::str::is_empty());
            let stderr = &assertion.get_output().stderr;
            let error: Value = if format == "json" {
                serde_json::from_slice(stderr).unwrap()
            } else {
                serde_norway::from_slice(stderr).unwrap()
            };
            assert_eq!(error["error"]["code"], "no_profile_configured");
            assert!(!output.exists());
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn force_overwrites_disposable_file_with_mock_package_without_prompting() {
    const REPLACEMENT: &[u8] = b"replacement synthetic package";
    for (kind, node_uid, metadata_path, download_path, package_type) in [
        (
            "cluster",
            None,
            Some("/v1/cluster"),
            "/v1/cluster/debuginfo",
            "cluster",
        ),
        (
            "database",
            None,
            Some("/v1/bdbs/42"),
            "/v1/bdbs/42/debuginfo",
            "database-42",
        ),
        ("node", None, None, "/v1/nodes/debuginfo", "nodes"),
        (
            "node",
            Some("42"),
            Some("/v1/nodes/42"),
            "/v1/nodes/42/debuginfo",
            "node-42",
        ),
    ] {
        let directory = fixture();
        let mock = MockServer::start().await;
        let config = format!(
            "default_enterprise = \"synthetic\"\n[profiles.synthetic]\ndeployment_type = \"enterprise\"\nurl = \"{}\"\nusername = \"synthetic-user\"\npassword = \"synthetic-password\"\n",
            mock.uri()
        );
        std::fs::write(directory.path().join("config.toml"), &config).unwrap();
        if let Some(metadata_path) = metadata_path {
            Mock::given(method("GET"))
                .and(path(metadata_path))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "name":"synthetic", "software_version":"synthetic", "addr":"synthetic"
                })))
                .expect(1)
                .mount(&mock)
                .await;
        }
        Mock::given(method("GET"))
            .and(path(download_path))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/gzip")
                    .set_body_bytes(REPLACEMENT.to_vec()),
            )
            .expect(1)
            .mount(&mock)
            .await;

        // Piped consent still must not start generation without explicit force.
        let mut denied = command(&directory, kind, "json");
        if let Some(uid) = node_uid {
            denied.arg(uid);
        }
        denied
            .write_stdin("yes\n")
            .assert()
            .code(12)
            .stdout(predicate::str::is_empty());
        assert!(mock.received_requests().await.unwrap().is_empty());
        assert_eq!(
            std::fs::read(directory.path().join("existing.tar.gz")).unwrap(),
            ORIGINAL
        );

        let mut forced = command(&directory, kind, "json");
        forced.arg("--force");
        if let Some(uid) = node_uid {
            forced.arg(uid);
        }
        let assertion = forced
            .write_stdin("")
            .assert()
            .success()
            .stderr(predicate::str::is_empty());
        let result: Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["package_type"], package_type);
        assert_eq!(result["file_size"], REPLACEMENT.len());
        assert_eq!(
            std::fs::read(directory.path().join("existing.tar.gz")).unwrap(),
            REPLACEMENT
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join("config.toml")).unwrap(),
            config
        );
        let expected_requests = if metadata_path.is_some() { 2 } else { 1 };
        assert_eq!(
            mock.received_requests().await.unwrap().len(),
            expected_requests
        );
    }
}
