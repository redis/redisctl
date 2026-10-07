//! Config-only subprocess checks: disposable files, no API requests or keyring operations.

use assert_cmd::Command;
use predicates::prelude::*;
use std::time::Duration;
use tempfile::TempDir;

fn command(directory: &TempDir) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .arg("--config-file")
        .arg(directory.path().join("config.toml"))
        .env_remove("REDISCTL_PROFILE")
        .env_remove("RUST_LOG")
        .env_remove("COMPLETE")
        .timeout(Duration::from_secs(5));
    command
}

#[test]
fn rejected_configuration_values_are_absent_from_cli_diagnostics() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("config.toml");
    let marker = "synthetic-cli-config-marker";
    for content in [
        format!("default_cloud = {marker}"),
        "[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = '${REDISCTL_CLI_CONFIG_VALUE}'".to_string(),
    ] {
        std::fs::write(&path, &content).unwrap();
        for output in ["json", "yaml", "table"] {
            command(&directory)
                .env("REDISCTL_CLI_CONFIG_VALUE", marker)
                .args(["profile", "list", "--output", output])
                .assert()
                .code(1)
                .stdout(predicate::str::is_empty())
                .stderr(predicate::str::contains("Failed to parse config"))
                .stderr(predicate::str::contains(marker).not());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        }
    }
}

#[test]
fn literal_environment_credentials_do_not_break_cli_config_loading() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("config.toml");
    let content = "[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = 6379\npassword = '${REDISCTL_CLI_CONFIG_VALUE}'";
    std::fs::write(&path, content).unwrap();
    let marker = "synthetic\"credential\\nwith\nlines";
    command(&directory)
        .env("REDISCTL_CLI_CONFIG_VALUE", marker)
        .args(["profile", "list", "--output", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("db"))
        .stdout(predicate::str::contains("synthetic").not())
        .stderr(predicate::str::is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
}

const EDIT_FIXTURE: &str = r#"
files_api_key = '${REDISCTL_CLI_EDIT_FILES}'
default_database = '${REDISCTL_CLI_EDIT_DEFAULT}'
[profiles."db.old"]
deployment_type = 'database'
host = 'localhost'
port = 6379
password = '${REDISCTL_CLI_EDIT_PASSWORD}'
files_api_key = '${REDISCTL_CLI_EDIT_PROFILE_FILES}'
tags = ['${REDISCTL_CLI_EDIT_TAG}', 'second']
[profiles."db.new"]
deployment_type = 'database'
host = 'localhost'
port = 6379
"#;

fn edit_command(directory: &TempDir) -> Command {
    let mut command = command(directory);
    command
        .env("REDISCTL_CLI_EDIT_FILES", "synthetic-global-files")
        .env("REDISCTL_CLI_EDIT_DEFAULT", "db.old")
        .env("REDISCTL_CLI_EDIT_PASSWORD", "synthetic-password")
        .env("REDISCTL_CLI_EDIT_PROFILE_FILES", "synthetic-profile-files")
        .env("REDISCTL_CLI_EDIT_TAG", "old-tag");
    command
}

fn edit_fixture() -> (TempDir, std::path::PathBuf) {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, EDIT_FIXTURE).unwrap();
    (directory, path)
}

#[test]
fn default_change_keeps_unrelated_credential_and_metadata_references() {
    let (directory, path) = edit_fixture();
    edit_command(&directory)
        .args(["profile", "default-database", "db.new"])
        .assert()
        .success();
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(!saved.contains("${REDISCTL_CLI_EDIT_DEFAULT}"));
    for reference in ["PASSWORD", "FILES", "PROFILE_FILES", "TAG"] {
        assert!(saved.contains(&format!("${{REDISCTL_CLI_EDIT_{reference}}}")));
    }
    assert!(!saved.contains("synthetic-password"));
    assert!(!saved.contains("synthetic-global-files"));
}

#[test]
fn profile_replacement_changes_credentials_but_preserves_unsupplied_metadata() {
    let (directory, path) = edit_fixture();
    edit_command(&directory)
        .args([
            "profile",
            "set",
            "db.old",
            "--type",
            "database",
            "--host",
            "localhost",
            "--port",
            "6379",
            "--password",
            "synthetic-password",
        ])
        .write_stdin("yes\n")
        .assert()
        .success();
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(!saved.contains("${REDISCTL_CLI_EDIT_PASSWORD}"));
    assert!(saved.contains("synthetic-password"));
    for reference in ["FILES", "PROFILE_FILES", "TAG", "DEFAULT"] {
        assert!(saved.contains(&format!("${{REDISCTL_CLI_EDIT_{reference}}}")));
    }
}

#[test]
fn explicit_tag_replacement_can_change_a_reference_backed_array_shape() {
    let (directory, path) = edit_fixture();
    edit_command(&directory)
        .args([
            "profile",
            "set",
            "db.old",
            "--type",
            "database",
            "--host",
            "localhost",
            "--port",
            "6379",
            "--password",
            "synthetic-new-password",
            "--tag",
            "new-tag",
        ])
        .write_stdin("y\n")
        .assert()
        .success();
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(!saved.contains("${REDISCTL_CLI_EDIT_TAG}"));
    assert!(!saved.contains("second"));
    assert!(saved.contains("new-tag"));
    assert!(saved.contains("${REDISCTL_CLI_EDIT_PROFILE_FILES}"));
}

#[test]
fn new_profile_does_not_inline_an_existing_profiles_credentials() {
    let (directory, path) = edit_fixture();
    edit_command(&directory)
        .args([
            "profile",
            "set",
            "fresh",
            "--type",
            "cloud",
            "--api-key",
            "synthetic-new-key",
            "--api-secret",
            "synthetic-new-secret",
        ])
        .assert()
        .success();
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(saved.contains("${REDISCTL_CLI_EDIT_PASSWORD}"));
    assert!(saved.contains("synthetic-new-secret"));
}

#[test]
fn profile_removal_keeps_untouched_credentials_and_metadata_references() {
    let (directory, path) = edit_fixture();
    std::fs::write(&path, format!("{EDIT_FIXTURE}\n[profiles.removable]\ndeployment_type = 'cloud'\napi_key = 'unused-key'\napi_secret = 'unused-secret'\n")).unwrap();
    edit_command(&directory)
        .args(["profile", "remove", "removable"])
        .write_stdin("yes\n")
        .assert()
        .success();
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(!saved.contains("unused-secret"));
    for reference in ["PASSWORD", "FILES", "PROFILE_FILES", "TAG", "DEFAULT"] {
        assert!(saved.contains(&format!("${{REDISCTL_CLI_EDIT_{reference}}}")));
    }
}

#[test]
fn files_key_writes_use_the_selected_document_and_preserve_other_references() {
    for profile in [false, true] {
        let (directory, path) = edit_fixture();
        let mut invocation = edit_command(&directory);
        invocation.args(["files-key", "set", "fresh-files-marker"]);
        if profile {
            invocation.args(["--profile", "db.old"]);
        } else {
            invocation.arg("--global");
        }
        invocation.assert().success();
        let mut read_back = edit_command(&directory);
        read_back.args(["files-key", "get"]);
        if profile {
            read_back.args(["--profile", "db.old"]);
        }
        read_back
            .assert()
            .success()
            .stdout(predicate::str::contains("fresh-fi...rker"));
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("${REDISCTL_CLI_EDIT_PASSWORD}"));
        assert!(saved.contains("${REDISCTL_CLI_EDIT_TAG}"));
        let changed = if profile { "PROFILE_FILES" } else { "FILES" };
        let untouched = if profile { "FILES" } else { "PROFILE_FILES" };
        assert!(!saved.contains(&format!("${{REDISCTL_CLI_EDIT_{changed}}}")));
        assert!(saved.contains(&format!("${{REDISCTL_CLI_EDIT_{untouched}}}")));
        let mut removal = edit_command(&directory);
        removal.args(["files-key", "remove"]);
        if profile {
            removal.args(["--profile", "db.old"]);
        } else {
            removal.arg("--global");
        }
        removal.assert().success();
        let saved = std::fs::read_to_string(path).unwrap();
        assert!(!saved.contains("fresh-files-marker"));
        assert!(saved.contains("${REDISCTL_CLI_EDIT_PASSWORD}"));
    }
}

#[test]
fn malformed_source_is_not_overwritten_by_a_cli_write_command() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("config.toml");
    let content = "files_api_key = 'synthetic-private-marker'\n[broken";
    std::fs::write(&path, content).unwrap();
    for arguments in [
        vec!["files-key", "set", "new-key", "--global"],
        vec![
            "profile",
            "set",
            "fresh",
            "--type",
            "cloud",
            "--api-key",
            "new-key",
            "--api-secret",
            "new-secret",
        ],
    ] {
        command(&directory)
            .args(arguments)
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("synthetic-private-marker").not());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
}
