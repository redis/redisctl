//! Configuration-only validation diagnostics using disposable, credential-free files.

use std::path::Path;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

const CONFIG: &str = r#"
default_database = "fixture-only"
[profiles.fixture-only]
deployment_type = "database"
host = "127.0.0.1"
port = 6379
tls = false
"#;

fn command() -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .env_remove("REDISCTL_CONFIG_FILE")
        .env_remove("REDISCTL_PROFILE")
        .env_remove("RUST_LOG")
        .env_remove("COMPLETE")
        .timeout(Duration::from_secs(5));
    command
}

fn report(path: &Path, format: &str) -> Value {
    let assertion = command()
        .arg("--config-file")
        .arg(path)
        .args(["profile", "validate", "--output", format])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    if format == "json" {
        serde_json::from_slice(&assertion.get_output().stdout).unwrap()
    } else {
        serde_yaml::from_slice(&assertion.get_output().stdout).unwrap()
    }
}

#[test]
fn structured_validation_reports_existing_selected_file_and_its_profiles() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("custom.toml");
    std::fs::write(&path, CONFIG).unwrap();
    for format in ["json", "yaml"] {
        let output = report(&path, format);
        assert_eq!(output["config_path"], path.to_str().unwrap());
        assert_eq!(output["config_exists"], true);
        assert_eq!(output["overall_valid"], true);
        assert_eq!(output["profile_count"], 1);
        assert_eq!(output["profiles"][0]["name"], "fixture-only");
        assert_eq!(output["defaults"]["database"]["valid"], true);
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), CONFIG);
}

#[test]
fn structured_validation_reports_missing_selected_file_not_default_existence() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("missing.toml");
    for format in ["json", "yaml"] {
        let output = report(&path, format);
        assert_eq!(output["config_path"], path.to_str().unwrap());
        assert_eq!(output["config_exists"], false);
        assert_eq!(output["overall_valid"], false);
        assert_eq!(output["profile_count"], 0);
        assert_eq!(output["profiles"], serde_json::json!([]));
    }
    assert!(!path.exists());
}

#[test]
fn human_validation_names_the_selected_existing_or_missing_file() {
    let directory = TempDir::new().unwrap();
    for (name, exists) in [("custom.toml", true), ("missing.toml", false)] {
        let path = directory.path().join(name);
        if exists {
            std::fs::write(&path, CONFIG).unwrap();
        }
        command()
            .arg("--config-file")
            .arg(&path)
            .args(["profile", "validate", "--output", "table"])
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(predicate::str::contains(format!(
                "Configuration file: {}",
                path.display()
            )))
            .stdout(predicate::str::contains(if exists {
                "Configuration file exists and is readable"
            } else {
                "Configuration file does not exist"
            }));
        assert_eq!(path.exists(), exists);
    }
}

#[test]
fn validation_honors_environment_selection_and_explicit_flag_precedence() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("environment.toml");
    let missing = directory.path().join("explicit-missing.toml");
    std::fs::write(&path, CONFIG).unwrap();
    let assertion = command()
        .env("REDISCTL_CONFIG_FILE", &path)
        .args(["profile", "validate", "--output", "json"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let output: Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["config_path"], path.to_str().unwrap());
    assert_eq!(output["profile_count"], 1);

    let assertion = command()
        .env("REDISCTL_CONFIG_FILE", &path)
        .arg("--config-file")
        .arg(&missing)
        .args(["profile", "validate", "--output", "json"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let output: Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["config_path"], missing.to_str().unwrap());
    assert_eq!(output["config_exists"], false);
    assert_eq!(output["profile_count"], 0);
    assert!(!missing.exists());
}
