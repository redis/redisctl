use std::fs;
use std::path::PathBuf;

use redisctl_core::config::Config;
use tempfile::TempDir;

/// Returns true if running as root (euid == 0). Used to skip permission tests.
#[cfg(unix)]
fn is_root() -> bool {
    // Use `id -u` to check the effective user ID without depending on libc.
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 1. Missing config directory / nonexistent path
// ---------------------------------------------------------------------------

#[test]
fn load_from_nonexistent_path_returns_default_config() {
    let path = PathBuf::from("/tmp/redisctl-test-nonexistent/does/not/exist/config.toml");
    assert!(!path.exists());

    let config = Config::load_from_path(&path).expect("should not panic or error on missing path");

    assert!(config.profiles.is_empty());
    assert!(config.default_cloud.is_none());
    assert!(config.default_enterprise.is_none());
    assert!(config.default_database.is_none());
}

// ---------------------------------------------------------------------------
// 2. Empty config file
// ---------------------------------------------------------------------------

#[test]
fn load_empty_config_file_returns_default_config() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");
    fs::write(&config_path, "").unwrap();

    let config = Config::load_from_path(&config_path).expect("empty file should parse as default");

    assert!(config.profiles.is_empty());
    assert!(config.default_cloud.is_none());
    assert!(config.default_enterprise.is_none());
    assert!(config.default_database.is_none());
}

// ---------------------------------------------------------------------------
// 3. Corrupt / invalid TOML
// ---------------------------------------------------------------------------

#[test]
fn load_corrupt_toml_returns_parse_error() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");
    fs::write(&config_path, "[[[broken").unwrap();

    let result = Config::load_from_path(&config_path);
    assert!(result.is_err(), "corrupt TOML should produce an error");

    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("parse") || msg.contains("Parse"),
        "error should mention parsing: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 4. Partial / incomplete config (profile missing required fields)
// ---------------------------------------------------------------------------

#[test]
fn load_profile_missing_required_fields_returns_error() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");

    // A cloud profile that is missing api_key and api_secret
    let content = r#"
[profiles.broken]
deployment_type = "cloud"
"#;
    fs::write(&config_path, content).unwrap();

    let result = Config::load_from_path(&config_path);
    assert!(
        result.is_err(),
        "incomplete profile should produce an error"
    );
}

// ---------------------------------------------------------------------------
// 5. Config with unknown / extra fields
// ---------------------------------------------------------------------------

#[test]
fn load_config_with_unknown_fields_ignores_them() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");

    let content = r#"
unknown_top_level_key = "hello"

[profiles.mydb]
deployment_type = "database"
host = "localhost"
port = 6379
totally_unknown_field = true
"#;
    fs::write(&config_path, content).unwrap();

    let config =
        Config::load_from_path(&config_path).expect("unknown fields should be silently ignored");

    assert!(config.profiles.contains_key("mydb"));
}

// ---------------------------------------------------------------------------
// 6. Permission errors (unix only)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn load_unreadable_file_returns_clear_error() {
    use std::os::unix::fs::PermissionsExt;

    // Skip if running as root (permissions won't be enforced)
    if is_root() {
        eprintln!("skipping test: running as root");
        return;
    }

    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");
    fs::write(&config_path, "# valid toml").unwrap();

    // Make file unreadable
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o000)).unwrap();

    let result = Config::load_from_path(&config_path);
    assert!(result.is_err(), "unreadable file should produce an error");

    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("load") || msg.contains("Load") || msg.contains("Permission"),
        "error should reference loading or permissions: {msg}"
    );

    // Restore permissions so TempDir cleanup can remove the file
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o644)).unwrap();
}

// ---------------------------------------------------------------------------
// 7. Save to read-only directory (unix only)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn save_to_readonly_directory_returns_clear_error() {
    use std::os::unix::fs::PermissionsExt;

    // Skip if running as root (permissions won't be enforced)
    if is_root() {
        eprintln!("skipping test: running as root");
        return;
    }

    let dir = TempDir::new().unwrap();
    let readonly_dir = dir.path().join("readonly");
    fs::create_dir(&readonly_dir).unwrap();
    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o444)).unwrap();

    let config_path = readonly_dir.join("config.toml");
    let config = Config::default();

    let result = config.save_to_path(&config_path);
    assert!(
        result.is_err(),
        "saving to read-only directory should produce an error"
    );

    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("save") || msg.contains("Save") || msg.contains("Permission"),
        "error should reference saving or permissions: {msg}"
    );

    // Restore permissions so TempDir cleanup can remove the directory
    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o755)).unwrap();
}

// ---------------------------------------------------------------------------
// 8. Tags backward compatibility (TOML without tags loads as empty vec)
// ---------------------------------------------------------------------------

#[test]
fn load_profile_without_tags_defaults_to_empty_vec() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");

    let content = r#"
[profiles.mycloud]
deployment_type = "cloud"
api_key = "key"
api_secret = "secret"
api_url = "https://api.redislabs.com/v1"
"#;
    fs::write(&config_path, content).unwrap();

    let config = Config::load_from_path(&config_path).expect("should load without tags field");
    let profile = config.profiles.get("mycloud").unwrap();
    assert!(profile.tags.is_empty(), "tags should default to empty vec");
}

// ---------------------------------------------------------------------------
// 9. Tags round-trip (save and reload with tags)
// ---------------------------------------------------------------------------

#[test]
fn tags_round_trip_save_and_reload() {
    use redisctl_core::{DeploymentType, Profile, ProfileCredentials};

    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.toml");

    let mut config = Config::default();
    config.set_profile(
        "tagged".to_string(),
        Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "k".to_string(),
                api_secret: "s".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            files_api_key: None,
            tags: vec!["prod".to_string(), "us-east".to_string()],
        },
    );

    config
        .save_to_path(&config_path)
        .expect("save should succeed");

    let reloaded = Config::load_from_path(&config_path).expect("reload should succeed");
    let profile = reloaded.profiles.get("tagged").unwrap();
    assert_eq!(profile.tags, vec!["prod", "us-east"]);
}

// ---------------------------------------------------------------------------
// 10. Empty tags are not serialized
// ---------------------------------------------------------------------------

#[test]
fn empty_tags_not_serialized() {
    use redisctl_core::{DeploymentType, Profile, ProfileCredentials};

    let mut config = Config::default();
    config.set_profile(
        "plain".to_string(),
        Profile {
            deployment_type: DeploymentType::Database,
            credentials: ProfileCredentials::Database {
                host: "localhost".to_string(),
                port: 6379,
                password: None,
                tls: false,
                username: "default".to_string(),
                database: 0,
            },
            files_api_key: None,
            tags: vec![],
        },
    );

    let serialized = toml::to_string(&config).unwrap();
    assert!(
        !serialized.contains("tags"),
        "empty tags should not appear in serialized output: {serialized}"
    );
}
