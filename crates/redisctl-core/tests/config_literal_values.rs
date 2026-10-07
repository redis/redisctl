//! Isolated environment fixtures: no process-global environment mutation, keyring or API calls.

use redisctl_core::{Config, ConfigDocument, ConfigError, ProfileCredentials};
use std::error::Error;
use std::process::Command;

const CHILD: &str = "REDISCTL_CONFIG_LITERAL_CHILD";
const VALUE: &str = "REDISCTL_CONFIG_LITERAL_VALUE";

fn run_child(case: &str, value: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "config_literal_child", "--nocapture"])
        .env(CHILD, case)
        .env(VALUE, value)
        .env_remove("REDISCTL_CONFIG_LITERAL_UNSET")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fixture {case} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn environment_values_are_literal_data() {
    for value in [
        "ordinary",
        r#"quoted"value'with\backslashes\n\t"#,
        "first line\nsecond line\r\nthird\tcolumn",
        "非 ASCII 🔑 ${OTHER_VARIABLE} $unchanged",
        "",
    ] {
        run_child("literal", value);
    }
}

#[test]
fn configuration_diagnostics_do_not_include_rejected_values() {
    run_child("diagnostics", "synthetic-diagnostic-marker");
}

#[test]
fn changed_or_removed_environment_does_not_change_save_provenance() {
    run_child(
        "changed_environment",
        "synthetic-loaded\"value\\nwith\nlines",
    );
}

#[test]
fn unquoted_placeholders_are_not_toml_syntax() {
    run_child("unquoted", "6379");
}

fn assert_safe_error(error: &ConfigError, marker: &str) {
    assert!(!error.to_string().contains(marker));
    assert!(!format!("{error:?}").contains(marker));
    assert!(!format!("{error:#?}").contains(marker));
    let mut source = error.source();
    while let Some(error) = source {
        assert!(!error.to_string().contains(marker));
        assert!(!format!("{error:?}").contains(marker));
        source = error.source();
    }
    if let ConfigError::ParseError(inner) = error {
        // Even callers inspecting the converted public payload do not receive source data.
        assert!(!inner.to_string().contains(marker));
        assert!(!format!("{inner:?}").contains(marker));
    }
}

#[test]
fn explicitly_constructed_toml_errors_have_safe_formatting_and_sources() {
    let marker = "synthetic-explicit-marker";
    let raw = toml::from_str::<Config>(&format!("default_cloud = {marker}")).unwrap_err();
    assert!(format!("{raw:?}").contains(marker));
    let error = ConfigError::ParseError(raw);
    // Raw construction preserves the public payload for compatibility, but formatting/chains
    // must not reveal it. Conversion from parser errors additionally sanitizes the payload.
    assert!(!error.to_string().contains(marker));
    assert!(!format!("{error:?}").contains(marker));
    assert!(error.source().is_none());

    let raw = <toml::ser::Error as serde::ser::Error>::custom(marker);
    let error = ConfigError::SerializeError(raw);
    assert!(!error.to_string().contains(marker));
    assert!(!format!("{error:?}").contains(marker));
    assert!(error.source().is_none());

    let raw = <toml::ser::Error as serde::ser::Error>::custom(marker);
    let error = ConfigError::from(raw);
    assert!(!error.to_string().contains(marker));
    assert!(!format!("{error:?}").contains(marker));
    assert!(error.source().is_none());
    let ConfigError::SerializeError(inner) = error else {
        panic!("expected serialize error");
    };
    assert!(!inner.to_string().contains(marker));
    assert!(!format!("{inner:?}").contains(marker));
}

#[test]
fn config_literal_child() {
    let Ok(case) = std::env::var(CHILD) else {
        return;
    };
    let value = std::env::var(VALUE).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    match case.as_str() {
        "literal" => {
            std::fs::write(
                &path,
                r#"
files_api_key = "prefix-${REDISCTL_CONFIG_LITERAL_VALUE}-suffix"
default_database = "${REDISCTL_CONFIG_LITERAL_UNSET:-fallback}"
[profiles."${REDISCTL_CONFIG_LITERAL_VALUE}"]
deployment_type = "database"
host = 'localhost'
port = 6379
password = '${REDISCTL_CONFIG_LITERAL_VALUE}'
tags = ["${REDISCTL_CONFIG_LITERAL_VALUE}", "${REDISCTL_CONFIG_LITERAL_UNSET}", "${REDISCTL_CONFIG_LITERAL_UNSET:-fallback}"]
"#,
            )
            .unwrap();
            let config = Config::load_from_path(&path).unwrap();
            assert_eq!(config.files_api_key, Some(format!("prefix-{value}-suffix")));
            assert_eq!(config.default_database.as_deref(), Some("fallback"));
            // Table names never expand; single-quoted and array values do.
            let profile = &config.profiles["${REDISCTL_CONFIG_LITERAL_VALUE}"];
            assert_eq!(
                profile.tags,
                [value.clone(), "${REDISCTL_CONFIG_LITERAL_UNSET}".into(), "fallback".into()]
            );
            let ProfileCredentials::Database { password, .. } = &profile.credentials else {
                panic!("expected database profile");
            };
            assert_eq!(password.as_deref(), Some(value.as_str()));
            // Public document load/save APIs see the real isolated environment, while saves
            // keep the original references for both normal and owner-only writes.
            for owner_only in [false, true] {
                let mut document = ConfigDocument::load_from_path(&path).unwrap();
                document.config_mut().default_cloud = Some("unrelated-literal".to_string());
                if owner_only {
                    document.save_to_path_owner_only(&path).unwrap();
                } else {
                    document.save().unwrap();
                }
                let saved: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                let profile = &saved["profiles"]["${REDISCTL_CONFIG_LITERAL_VALUE}"];
                assert_eq!(profile["password"].as_str(), Some("${REDISCTL_CONFIG_LITERAL_VALUE}"));
                assert_eq!(profile["tags"][0].as_str(), Some("${REDISCTL_CONFIG_LITERAL_VALUE}"));
                let loaded = Config::load_from_path(&path).unwrap();
                let ProfileCredentials::Database { password, .. } = &loaded.profiles["${REDISCTL_CONFIG_LITERAL_VALUE}"].credentials else {
                    panic!("expected database profile");
                };
                assert_eq!(password.as_deref(), Some(value.as_str()));
            }
        }
        "changed_environment" => {
            for owner_only in [false, true] {
                for unset in [false, true] {
                    // SAFETY: only this fixture runs in the isolated subprocess; these names
                    // are not used by background tasks. The parent environment is untouched.
                    unsafe { std::env::set_var(VALUE, &value); }
                    std::fs::write(&path, "[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = 6379\npassword = '${REDISCTL_CONFIG_LITERAL_VALUE}'").unwrap();
                    let mut document = ConfigDocument::load_from_path(&path).unwrap();
                    // Same single-fixture subprocess isolation as the setup above.
                    unsafe {
                        if unset { std::env::remove_var(VALUE); }
                        else { std::env::set_var(VALUE, "synthetic-rotated-value"); }
                    }
                    document.config_mut().default_database = Some("db".to_string());
                    if owner_only { document.save_to_path_owner_only(&path).unwrap(); }
                    else { document.save().unwrap(); }
                    let saved: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                    assert_eq!(saved["profiles"]["db"]["password"].as_str(), Some("${REDISCTL_CONFIG_LITERAL_VALUE}"));
                    assert!(!std::fs::read_to_string(&path).unwrap().contains("synthetic"));
                    let ProfileCredentials::Database { password, .. } = &document.config().profiles["db"].credentials else { panic!("expected database profile"); };
                    assert_eq!(password.as_deref(), Some(value.as_str()));
                }
            }
        }
        "diagnostics" => {
            for content in [
                format!("default_cloud = {value}"),
                format!("[profiles.{value}]\ndeployment_type = 'unknown-{value}'"),
                format!("[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = '{value}'"),
                "[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = '${REDISCTL_CONFIG_LITERAL_VALUE}'".into(),
            ] {
                std::fs::write(&path, content).unwrap();
                let error = Config::load_from_path(&path).unwrap_err();
                assert_safe_error(&error, &value);
            }
        }
        "unquoted" => {
            std::fs::write(
                &path,
                "[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = ${REDISCTL_CONFIG_LITERAL_VALUE}",
            )
            .unwrap();
            assert!(matches!(Config::load_from_path(&path), Err(ConfigError::ParseError(_))));
        }
        _ => panic!("unknown fixture"),
    }
}
