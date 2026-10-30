//! Profile removal is configuration-only: no Redis, API, or keyring access is needed.

use std::path::Path;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use redisctl_core::Config;
use serde_json::Value;
use tempfile::TempDir;

const CONFIG: &str = r#"
default_database = "victim"
default_cloud = "cloud"
default_enterprise = "enterprise"
files_api_key = "synthetic-files-metadata"

[profiles.victim]
deployment_type = "database"
host = "127.0.0.1"
port = 6379
tls = false

[profiles.remaining]
deployment_type = "database"
host = "127.0.0.1"
port = 6380
tls = false
tags = ["retain"]

[profiles.cloud]
deployment_type = "cloud"
api_key = "synthetic-key"
api_secret = "synthetic-secret"

[profiles.enterprise]
deployment_type = "enterprise"
url = "https://localhost:9443"
username = "synthetic-user"
password = "synthetic-password"

[cloud_auth.cloud]
okta_issuer = "https://example.invalid/oauth2/default"
okta_client_id = "synthetic-client"
sm_api_url = "https://example.invalid/api/v1"
"#;

fn fixture(directory: &TempDir, contents: &str) -> std::path::PathBuf {
    let path = directory.path().join("selected.toml");
    std::fs::write(&path, contents).unwrap();
    path
}

fn command(path: &Path) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .arg("--config-file")
        .arg(path)
        .env_remove("REDISCTL_CONFIG_FILE")
        .env_remove("REDISCTL_PROFILE")
        .env_remove("RUST_LOG")
        .env_remove("COMPLETE")
        .timeout(Duration::from_secs(5));
    command
}

fn config(path: &Path) -> Value {
    serde_json::to_value(Config::load_from_path(path).unwrap()).unwrap()
}

fn expected_removal(path: &Path, name: &str, cleared_default: Option<&str>) -> Value {
    let mut expected = config(path);
    expected["profiles"].as_object_mut().unwrap().remove(name);
    if let Some(field) = cleared_default {
        expected[field] = Value::Null;
    }
    expected
}

#[test]
fn confirmed_database_default_removal_clears_reference_and_preserves_other_config() {
    let directory = TempDir::new().unwrap();
    let path = fixture(&directory, CONFIG);
    let expected = expected_removal(&path, "victim", Some("default_database"));
    command(&path)
        .args(["profile", "remove", "victim"])
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Warning: 'victim' is the default profile for database commands.",
        ))
        .stdout(predicate::str::contains(
            "Default database profile cleared.",
        ));
    assert_eq!(config(&path), expected);
    command(&path)
        .args(["profile", "current", "--type", "database"])
        .assert()
        .success()
        .stdout("remaining\n");
}

#[test]
fn removing_the_only_database_default_leaves_no_dangling_reference() {
    let directory = TempDir::new().unwrap();
    let path = fixture(
        &directory,
        "default_database = 'victim'\n[profiles.victim]\ndeployment_type = 'database'\nhost = '127.0.0.1'\nport = 6379\ntls = false\n",
    );
    let expected = expected_removal(&path, "victim", Some("default_database"));
    command(&path)
        .args(["profile", "remove", "victim"])
        .write_stdin("yes\n")
        .assert()
        .success();
    assert_eq!(config(&path), expected);
    command(&path)
        .args(["profile", "current", "--type", "database"])
        .assert()
        .code(3);
}

#[test]
fn removing_non_default_database_preserves_the_default() {
    let directory = TempDir::new().unwrap();
    let path = fixture(
        &directory,
        &CONFIG.replace(
            "default_database = \"victim\"",
            "default_database = \"remaining\"",
        ),
    );
    let expected = expected_removal(&path, "victim", None);
    command(&path)
        .args(["profile", "remove", "victim"])
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("default profile for database").not())
        .stdout(predicate::str::contains("Default database profile cleared.").not());
    assert_eq!(config(&path), expected);
}

#[test]
fn cancelled_removal_preserves_original_file_bytes() {
    let directory = TempDir::new().unwrap();
    let path = fixture(&directory, CONFIG);
    for input in ["", "n\n", "no\n", "unexpected\n"] {
        command(&path)
            .args(["profile", "remove", "victim"])
            .write_stdin(input)
            .assert()
            .success()
            .stdout(predicate::str::contains("Profile removal cancelled."));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), CONFIG);
    }
}

#[test]
fn cloud_and_enterprise_removal_still_clear_only_their_own_defaults() {
    for deployment in ["cloud", "enterprise"] {
        let directory = TempDir::new().unwrap();
        let path = fixture(&directory, CONFIG);
        let expected = expected_removal(&path, deployment, Some(&format!("default_{deployment}")));
        command(&path)
            .args(["profile", "remove", deployment])
            .write_stdin("y\n")
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "Default {deployment} profile cleared."
            )));
        assert_eq!(config(&path), expected);
    }
}

#[test]
fn missing_profile_removal_preserves_original_file_bytes() {
    let directory = TempDir::new().unwrap();
    let path = fixture(&directory, CONFIG);
    command(&path)
        .args(["profile", "remove", "missing"])
        .write_stdin("y\n")
        .assert()
        .code(3);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), CONFIG);
}
