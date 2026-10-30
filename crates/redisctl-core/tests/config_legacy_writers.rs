//! Deprecated APIs are exercised intentionally, using only disposable synthetic files.

use redisctl_core::{Config, ConfigError};
use std::path::Path;

#[allow(deprecated)]
fn legacy_save(config: &Config, path: &Path, owner_only: bool) -> Result<(), ConfigError> {
    if owner_only {
        config.save_to_path_owner_only(path)
    } else {
        config.save_to_path(path)
    }
}

#[test]
fn legacy_writers_refuse_existing_references_without_mutating_the_file() {
    for source in [
        "files_api_key = '${REDISCTL_LEGACY_UNSET:-synthetic-secret}'",
        "files_api_key = '$REDISCTL_LEGACY_UNSET'",
        "[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = 6379\ntags = ['${REDISCTL_LEGACY_UNSET}']",
        "[unknown]\nnested = [[{value = '${REDISCTL_LEGACY_UNSET:-synthetic-secret}'}]]",
    ] {
        for owner_only in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("config.toml");
            std::fs::write(&path, source).unwrap();
            #[cfg(unix)]
            let before = {
                use std::os::unix::fs::MetadataExt;
                let metadata = std::fs::metadata(&path).unwrap();
                (metadata.ino(), metadata.mode())
            };
            let error = legacy_save(&Config::default(), &path, owner_only).unwrap_err();
            assert!(error.to_string().contains("ConfigDocument"));
            assert!(!format!("{error:?}").contains("synthetic-secret"));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let metadata = std::fs::metadata(&path).unwrap();
                assert_eq!((metadata.ino(), metadata.mode()), before);
            }
        }
    }
}

#[test]
fn legacy_writers_refuse_reference_outputs_before_creating_directories() {
    for reference in [
        "$REDISCTL_LEGACY_VALUE",
        "${REDISCTL_LEGACY_VALUE:-fallback}",
    ] {
        for owner_only in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("not-created/config.toml");
            let config = Config {
                files_api_key: Some(reference.into()),
                ..Config::default()
            };
            let error = legacy_save(&config, &path, owner_only).unwrap_err();
            assert!(error.to_string().contains("ConfigDocument"));
            assert!(!path.parent().unwrap().exists());
        }
    }
}

#[test]
fn legacy_writers_do_not_overwrite_malformed_or_unreadable_destinations() {
    for source in [
        b"files_api_key = 'synthetic-private-marker'\n[broken".as_slice(),
        b"synthetic-private-marker\xff".as_slice(),
    ] {
        for owner_only in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("config.toml");
            std::fs::write(&path, source).unwrap();
            let error = legacy_save(&Config::default(), &path, owner_only).unwrap_err();
            assert!(!format!("{error:?}").contains("synthetic-private-marker"));
            assert!(!error.to_string().contains("synthetic-private-marker"));
            assert_eq!(std::fs::read(&path).unwrap(), source);
        }
    }
    for owner_only in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::create_dir(&path).unwrap();
        assert!(legacy_save(&Config::default(), &path, owner_only).is_err());
        assert!(path.is_dir());
    }
}

#[test]
fn legacy_literal_only_saves_still_work_for_new_and_existing_destinations() {
    for existing in [false, true] {
        for owner_only in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("config.toml");
            if existing {
                std::fs::write(&path, "default_cloud = 'old-literal'").unwrap();
            }
            let config = Config {
                default_cloud: Some("new-literal".into()),
                files_api_key: Some("synthetic literal with $ and no variable".into()),
                ..Config::default()
            };
            legacy_save(&config, &path, owner_only).unwrap();
            let saved = Config::load_from_path(&path).unwrap();
            assert_eq!(saved.default_cloud, config.default_cloud);
            assert_eq!(saved.files_api_key, config.files_api_key);
            #[cfg(unix)]
            if owner_only {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }
}

#[test]
fn reference_syntax_in_literal_table_names_is_not_a_value_reference() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, "[unknown.'${LITERAL_KEY}']\nvalue = 'literal'").unwrap();
    legacy_save(&Config::default(), &path, false).unwrap();
}

#[test]
#[allow(deprecated)]
fn legacy_public_method_signatures_are_retained_without_accessing_the_real_home() {
    let _standard: fn(&Config) -> Result<(), ConfigError> = Config::save;
    let _normal: fn(&Config, &Path) -> Result<(), ConfigError> = Config::save_to_path;
    let _owner_only: fn(&Config, &Path) -> Result<(), ConfigError> =
        Config::save_to_path_owner_only;
}
