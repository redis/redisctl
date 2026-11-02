//! Files.com key previews using synthetic values and disposable home/config directories.
//!
//! Native Windows known-folder APIs cannot be isolated with HOME/APPDATA overrides.
#![cfg(not(target_os = "windows"))]

use std::path::Path;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn command(home: &Path) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("USERPROFILE", home)
        .env("APPDATA", home.join("roaming"))
        .env("LOCALAPPDATA", home.join("local"))
        .env_remove("REDISCTL_CONFIG_FILE")
        .env_remove("REDISCTL_PROFILE")
        .env_remove("RUST_LOG")
        .env_remove("COMPLETE")
        .timeout(Duration::from_secs(5));
    command
}

fn fixture(home: &Path, key: &str) -> (std::path::PathBuf, String) {
    let path = home.join(".config/redisctl/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Values are fixed synthetic cases below, not user input; none contain TOML delimiters.
    let content = format!(
        "files_api_key = '{key}'\n[profiles.fixture]\ndeployment_type = 'database'\nhost = '127.0.0.1'\nport = 6379\ntls = false\nfiles_api_key = '{key}'\n"
    );
    std::fs::write(&path, &content).unwrap();
    (path, content)
}

#[test]
fn global_literal_keys_are_scalar_safe_and_short_values_are_fully_masked() {
    for (key, expected) in [
        ("a", "****"),
        ("abcdefgh", "****"),
        ("abcdefghi", "ab...hi"),
        ("abcdefghijkl", "ab...kl"),
        ("🔒🔑🔒", "****"),
        ("🔒password", "🔒p...rd"),
        ("password🔒", "pa...d🔒"),
        ("e\u{301}abcdef", "****"),
    ] {
        let directory = TempDir::new().unwrap();
        let (path, content) = fixture(directory.path(), key);
        let assertion = command(directory.path())
            .args(["files-key", "get"])
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(format!("Global key: {expected}\n"));
        let output = String::from_utf8_lossy(&assertion.get_output().stdout);
        let preview = output.trim_end().strip_prefix("Global key: ").unwrap();
        assert!(!preview.contains(key));
        assert_eq!(std::fs::read_to_string(path).unwrap(), content);
    }
}

#[test]
fn profile_literal_keys_use_the_same_preview_without_keyring_access() {
    for (key, expected) in [
        ("short", "****"),
        ("éabcdefg", "****"),
        ("密碼abcdefgh密碼", "密碼...密碼"),
        ("épassword", "ép...rd"),
    ] {
        let directory = TempDir::new().unwrap();
        let (path, content) = fixture(directory.path(), key);
        let assertion = command(directory.path())
            .args(["files-key", "get", "--profile", "fixture"])
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(format!("Profile 'fixture' key: {expected}\n"));
        let output = String::from_utf8_lossy(&assertion.get_output().stdout);
        let preview = output
            .trim_end()
            .strip_prefix("Profile 'fixture' key: ")
            .unwrap();
        assert!(!preview.contains(key));
        assert_eq!(std::fs::read_to_string(path).unwrap(), content);
    }
}
