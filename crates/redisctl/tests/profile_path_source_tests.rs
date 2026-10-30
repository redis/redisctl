//! Read-only path reporting with disposable configuration/home directories.

use std::path::Path;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn command(home: &Path) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("roaming"))
        .env("LOCALAPPDATA", home.join("local"))
        .env_remove("REDISCTL_CONFIG_FILE")
        .env_remove("REDISCTL_PROFILE")
        .env_remove("RUST_LOG")
        .env_remove("COMPLETE")
        .timeout(Duration::from_secs(5));
    command
}

fn assert_path(mut command: Command, path: &Path, format: &str) {
    let assertion = command
        .args(["profile", "path", "--output", format])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let bytes = &assertion.get_output().stdout;
    if format == "table" {
        assert_eq!(
            String::from_utf8_lossy(bytes),
            format!("{}\n", path.display())
        );
    } else {
        let report: Value = if format == "json" {
            serde_json::from_slice(bytes).unwrap()
        } else {
            serde_norway::from_slice(bytes).unwrap()
        };
        assert_eq!(report, serde_json::json!({"config_path": path.to_str()}));
    }
}

#[test]
fn path_honors_explicit_existing_file_in_all_formats_without_changes() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("config with spaces.toml");
    let content = "# credential-free fixture\n";
    std::fs::write(&path, content).unwrap();
    for format in ["table", "json", "yaml"] {
        let mut command = command(directory.path());
        command.arg("--config-file").arg(&path);
        assert_path(command, &path, format);
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
}

#[test]
fn path_honors_environment_selection_in_all_formats() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("environment.toml");
    std::fs::write(&path, "").unwrap();
    for format in ["table", "json", "yaml"] {
        let mut command = command(directory.path());
        command.env("REDISCTL_CONFIG_FILE", &path);
        assert_path(command, &path, format);
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
}

#[test]
fn explicit_flag_overrides_environment_selected_file() {
    let directory = TempDir::new().unwrap();
    let environment = directory.path().join("environment.toml");
    let selected = directory.path().join("explicit.toml");
    std::fs::write(&environment, "").unwrap();
    std::fs::write(&selected, "").unwrap();
    for format in ["table", "json", "yaml"] {
        let mut command = command(directory.path());
        command
            .env("REDISCTL_CONFIG_FILE", &environment)
            .arg("--config-file")
            .arg(&selected);
        assert_path(command, &selected, format);
    }
}

#[test]
fn selected_missing_file_is_reported_without_creating_it() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("missing/config.toml");
    for format in ["table", "json", "yaml"] {
        let mut command = command(directory.path());
        command.arg("--config-file").arg(&path);
        assert_path(command, &path, format);
    }
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn relative_selected_path_is_not_canonicalized() {
    let directory = TempDir::new().unwrap();
    let selected = Path::new("nested/config with spaces.toml");
    for format in ["table", "json", "yaml"] {
        let mut command = command(directory.path());
        command
            .current_dir(directory.path())
            .arg("--config-file")
            .arg(selected);
        assert_path(command, selected, format);
    }
    assert!(!directory.path().join("nested").exists());
}

#[test]
#[cfg(not(target_os = "windows"))]
fn path_falls_back_to_isolated_home_config() {
    // Windows uses known-folder APIs, not APPDATA overrides; do not read personal config there.
    let directory = TempDir::new().unwrap();
    let selected = directory.path().join(".config/redisctl/config.toml");
    // On macOS the existing Linux-style directory selects the cross-platform fallback.
    std::fs::create_dir_all(selected.parent().unwrap()).unwrap();
    std::fs::write(&selected, "# isolated default\n").unwrap();
    for format in ["table", "json", "yaml"] {
        assert_path(command(directory.path()), &selected, format);
    }
    assert_eq!(
        std::fs::read_to_string(selected).unwrap(),
        "# isolated default\n"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn path_falls_back_to_isolated_native_macos_default() {
    let directory = TempDir::new().unwrap();
    let selected = directory
        .path()
        .join("Library/Application Support/com.redis.redisctl/config.toml");
    std::fs::create_dir_all(selected.parent().unwrap()).unwrap();
    std::fs::write(&selected, "# isolated native default\n").unwrap();
    assert!(!directory.path().join(".config").exists());
    for format in ["table", "json", "yaml"] {
        assert_path(command(directory.path()), &selected, format);
    }
    assert_eq!(
        std::fs::read_to_string(selected).unwrap(),
        "# isolated native default\n"
    );
}

#[test]
fn path_list_and_validation_agree_on_selected_file() {
    let directory = TempDir::new().unwrap();
    let selected = directory.path().join("selected.toml");
    std::fs::write(&selected, "").unwrap();
    for subcommand in ["path", "list", "validate"] {
        let assertion = command(directory.path())
            .env("REDISCTL_CONFIG_FILE", &selected)
            .args(["profile", subcommand, "--output", "json"])
            .assert()
            .success()
            .stderr(predicate::str::is_empty());
        let report: Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
        assert_eq!(report["config_path"], selected.to_str().unwrap());
    }
    assert_eq!(std::fs::read_to_string(selected).unwrap(), "");
}
