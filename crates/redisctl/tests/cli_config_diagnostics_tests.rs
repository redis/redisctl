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
