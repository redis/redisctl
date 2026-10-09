//! Subprocess contracts for Cloud user deletion; only synthetic config/mock APIs.

#![cfg(feature = "cloud")]

use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn command(directory: &TempDir) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .arg("--config-file")
        .arg(directory.path().join("config.toml"))
        .env_remove("REDISCTL_PROFILE")
        // Passing --config-file already disables environment credential overrides
        // (ConnectionManager runs the resolver with EnvironmentOverrides::Disabled),
        // so ambient Cloud credentials cannot reach these subprocesses. Strip them
        // anyway to match the other cloud suites and stay robust if that changes.
        .env_remove("REDIS_CLOUD_API_KEY")
        .env_remove("REDIS_CLOUD_SECRET_KEY")
        .env_remove("REDIS_CLOUD_API_SECRET")
        .env_remove("REDIS_CLOUD_API_URL")
        .env_remove("RUST_LOG")
        .env_remove("COMPLETE")
        .timeout(Duration::from_secs(5));
    command
}

#[test]
fn cloud_user_delete_noninteractive_inputs_are_cancelled() {
    let directory = TempDir::new().unwrap();
    for input in ["", "n\n", "y\n", "yes\n"] {
        command(&directory)
            .args(["cloud", "user", "delete", "42"])
            .write_stdin(input)
            .assert()
            .code(12)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(
                "Cancelled at the confirmation prompt",
            ))
            .stderr(predicate::str::contains("--force"));
    }
}

#[test]
fn cloud_user_delete_json_cancellation_preserves_error_contract() {
    let directory = TempDir::new().unwrap();
    let assertion = command(&directory)
        .args(["cloud", "user", "delete", "42", "--output", "json"])
        .write_stdin("")
        .assert()
        .code(12)
        .stdout(predicate::str::is_empty());
    let error: Value = serde_json::from_slice(&assertion.get_output().stderr).unwrap();
    assert_eq!(error["error"]["code"], "cancelled");
    assert_eq!(error["error"]["exit_code"], 12);
    assert!(error["error"]["message"].as_str().unwrap().contains("42"));
    assert!(
        error["error"]["tips"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tip| { tip.as_str().unwrap().contains("--force") })
    );
}

#[test]
fn cloud_user_delete_yaml_cancellation_preserves_error_contract() {
    let directory = TempDir::new().unwrap();
    let assertion = command(&directory)
        .args(["cloud", "user", "delete", "42", "--output", "yaml"])
        .write_stdin("n\n")
        .assert()
        .code(12)
        .stdout(predicate::str::is_empty());
    let error: Value = serde_yaml::from_slice(&assertion.get_output().stderr).unwrap();
    assert_eq!(error["error"]["code"], "cancelled");
    assert_eq!(error["error"]["exit_code"], 12);
}

#[test]
fn cloud_user_delete_force_advances_to_profile_resolution() {
    let directory = TempDir::new().unwrap();
    let assertion = command(&directory)
        .args([
            "cloud", "user", "delete", "42", "--force", "--output", "json",
        ])
        .write_stdin("")
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty());
    let error: Value = serde_json::from_slice(&assertion.get_output().stderr).unwrap();
    assert_eq!(error["error"]["code"], "no_profile_configured");
}

#[tokio::test]
async fn cloud_user_delete_only_explicit_force_reaches_mock_api() {
    let directory = TempDir::new().unwrap();
    let mock = MockServer::start().await;
    std::fs::write(
        directory.path().join("config.toml"),
        format!(
            "default_cloud = \"test\"\n[profiles.test]\ndeployment_type = \"cloud\"\napi_key = \"test-key\"\napi_secret = \"test-secret\"\napi_url = \"{}\"\n",
            mock.uri()
        ),
    )
    .unwrap();
    Mock::given(method("DELETE"))
        .and(path("/users/42"))
        .and(header("x-api-key", "test-key"))
        .and(header("x-api-secret-key", "test-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"deleted": 42})))
        .expect(1)
        .mount(&mock)
        .await;

    for input in ["", "n\n", "yes\n"] {
        command(&directory)
            .args(["cloud", "user", "delete", "42", "--output", "json"])
            .write_stdin(input)
            .assert()
            .code(12)
            .stdout(predicate::str::is_empty());
    }
    assert!(mock.received_requests().await.unwrap().is_empty());

    let assertion = command(&directory)
        .args([
            "cloud", "user", "delete", "42", "--force", "--output", "json",
        ])
        .write_stdin("")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let output: Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output, json!({"deleted": 42}));
    mock.verify().await;
}
