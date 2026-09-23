//! Tests for the `cloud_auth` config surface + `apply_cloud_login` persistence.
//! Uses save/load round-trips and a plaintext credential store, so nothing touches the real
//! OS keyring.

use redisctl_core::{
    CloudAuthConfig, Config, CredentialStore, DeploymentType, MintedCredentials, Profile,
    ProfileCredentials,
};

fn cloud_profile(api_url: &str) -> Profile {
    Profile {
        deployment_type: DeploymentType::Cloud,
        credentials: ProfileCredentials::Cloud {
            api_key: "acct-key".to_string(),
            api_secret: "user-secret".to_string(),
            api_url: api_url.to_string(),
        },
        files_api_key: None,
        tags: Vec::new(),
    }
}

fn qa_cloud_auth() -> CloudAuthConfig {
    CloudAuthConfig {
        okta_issuer: "https://okta.example.com/oauth2/default".to_string(),
        okta_client_id: "test-client-id".to_string(),
        sm_api_url: "https://sm.example.com/api/v1".to_string(),
        capi_url: "https://api.example.com/v1".to_string(),
        account_id: None,
        capi_key_name: None,
    }
}

fn roundtrip(config: &Config) -> (String, Config) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    config.save_to_path(&path).unwrap();
    let toml = std::fs::read_to_string(&path).unwrap();
    let loaded = Config::load_from_path(&path).unwrap();
    (toml, loaded)
}

#[test]
fn config_without_cloud_auth_roundtrips_and_omits_the_key() {
    let mut config = Config::default();
    config.set_profile(
        "prod".to_string(),
        cloud_profile("https://api.redislabs.com/v1"),
    );

    let (toml, loaded) = roundtrip(&config);
    // Backward compatible: no cloud_auth section is written when none is set.
    assert!(
        !toml.contains("cloud_auth"),
        "unexpected cloud_auth in:\n{toml}"
    );
    assert!(loaded.profiles.contains_key("prod"));
    assert_eq!(
        loaded.profiles["prod"].cloud_credentials().map(|c| c.2),
        Some("https://api.redislabs.com/v1")
    );
}

#[test]
fn config_with_cloud_auth_roundtrips() {
    let mut config = Config::default();
    config.set_profile(
        "qa".to_string(),
        cloud_profile("https://api.example.com/v1"),
    );
    config.cloud_auth.insert("qa".to_string(), qa_cloud_auth());

    let (toml, loaded) = roundtrip(&config);
    assert!(
        toml.contains("[cloud_auth.qa]"),
        "expected cloud_auth table in:\n{toml}"
    );
    assert_eq!(loaded.resolve_cloud_auth("qa"), qa_cloud_auth());
}

#[test]
fn resolve_cloud_auth_falls_back_to_prod_defaults() {
    let config = Config::default();
    let resolved = config.resolve_cloud_auth("anything");
    assert_eq!(resolved, CloudAuthConfig::prod_defaults());
    // Production endpoints are built in, so a profile with no `[cloud_auth]` section can log in.
    assert!(resolved.is_complete());
    assert_eq!(resolved.capi_url, "https://api.redislabs.com/v1");
    assert_eq!(
        resolved.okta_issuer,
        "https://auth.redis.com/oauth2/default"
    );
    assert!(!resolved.okta_client_id.is_empty());
    assert_eq!(resolved.sm_api_url, "https://cloud.redis.io/api/v1");
}

fn minted() -> MintedCredentials {
    MintedCredentials {
        account_id: Some(112117),
        email: Some("u@e.com".to_string()),
        api_key: "ACCT-KEY".to_string(),
        api_secret: "USER-SECRET".to_string(),
        api_url: "https://api.example.com/v1".to_string(),
        refresh_token: Some("RT".to_string()),
        capi_key_name: "redisctl-demo".to_string(),
        redisctl_key_count: 1,
        account_name: None,
        capi_newly_enabled: false,
        superseded_revoked: None,
        superseded_key_name: None,
        accounts: vec![redisctl_core::auth::LoginAccount {
            id: 112117,
            name: None,
        }],
    }
}

#[test]
fn apply_cloud_login_writes_profile_default_and_endpoints() {
    let mut config = Config::default();
    let store = CredentialStore::plaintext();
    let creds = minted();

    config
        .apply_cloud_login(&store, "qa", &creds, Some(qa_cloud_auth()), true)
        .unwrap();

    // Default cloud profile is set.
    assert_eq!(config.default_cloud.as_deref(), Some("qa"));
    // Profile is a Cloud profile; plaintext store records the values as-is.
    let (key, secret, url) = config.profiles["qa"].cloud_credentials().unwrap();
    assert_eq!(key, "ACCT-KEY");
    assert_eq!(secret, "USER-SECRET");
    assert_eq!(url, "https://api.example.com/v1");
    // Login endpoints were recorded for re-login, along with the account the key is for — which
    // cannot be derived later, since a key does not name its account.
    let expected = CloudAuthConfig {
        account_id: Some(112117),
        capi_key_name: Some("redisctl-demo".to_string()),
        ..qa_cloud_auth()
    };
    assert_eq!(config.resolve_cloud_auth("qa"), expected);

    // And it survives a save/load round-trip.
    let (_, loaded) = roundtrip(&config);
    assert_eq!(loaded.default_cloud.as_deref(), Some("qa"));
    assert_eq!(loaded.resolve_cloud_auth("qa"), expected);
}

/// Mirrors what `cloud auth logout` does at the config layer: it removes the profile (and its
/// credentials) but must PRESERVE the `[cloud_auth.<profile>]` login endpoints, so a later
/// `auth login` still resolves them instead of failing with "not configured".
#[test]
fn logout_removes_profile_but_preserves_cloud_auth_endpoints() {
    let mut config = Config::default();
    config.set_profile("qa".to_string(), cloud_profile("https://api-qa.example/v1"));
    config.cloud_auth.insert("qa".to_string(), qa_cloud_auth());

    // Record a key, as a login would.
    config
        .cloud_auth
        .get_mut("qa")
        .map(|auth| {
            auth.account_id = Some(492752);
            auth.capi_key_name = Some("redisctl-cli-1".to_string());
        })
        .unwrap();

    // The logout sequence: save endpoints, remove the profile, restore the endpoints only.
    let saved = config.cloud_auth.get("qa").cloned();
    config.remove_profile("qa");
    if let Some(mut auth) = saved {
        auth.account_id = None;
        auth.capi_key_name = None;
        config.cloud_auth.insert("qa".to_string(), auth);
    }

    let (_, loaded) = roundtrip(&config);
    // Profile (credentials) is gone…
    assert!(!loaded.list_profiles().iter().any(|(n, _)| *n == "qa"));
    // …but the login endpoints survive so re-login works.
    let auth = loaded.resolve_cloud_auth("qa");
    assert!(auth.is_complete());
    assert_eq!(auth.okta_client_id, "test-client-id");
    // The key logout just deleted must not be recorded any more, or the next login reports it as
    // one it could not revoke.
    assert_eq!(auth.account_id, None);
    assert_eq!(auth.capi_key_name, None);
}

/// A login rewrites the profile, but must not discard settings it does not own.
#[test]
fn apply_cloud_login_preserves_files_api_key_and_tags() {
    let mut config = Config::default();
    config.set_profile(
        "qa".to_string(),
        Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "OLD".to_string(),
                api_secret: "OLD".to_string(),
                api_url: "https://api.example.com/v1".to_string(),
            },
            files_api_key: Some("FILES-KEY".to_string()),
            tags: vec!["team-a".to_string(), "prod".to_string()],
        },
    );

    config
        .apply_cloud_login(
            &CredentialStore::plaintext(),
            "qa",
            &minted(),
            Some(qa_cloud_auth()),
            true,
        )
        .unwrap();

    let profile = &config.profiles["qa"];
    assert_eq!(profile.files_api_key.as_deref(), Some("FILES-KEY"));
    assert_eq!(profile.tags, vec!["team-a", "prod"]);
    let (key, _, _) = profile.cloud_credentials().unwrap();
    assert_eq!(key, "ACCT-KEY", "the credentials themselves are replaced");
}

/// A first login has no profile to merge with, so the fields are simply absent.
#[test]
fn apply_cloud_login_on_a_fresh_profile_has_no_extras() {
    let mut config = Config::default();
    config
        .apply_cloud_login(
            &CredentialStore::plaintext(),
            "qa",
            &minted(),
            Some(qa_cloud_auth()),
            true,
        )
        .unwrap();
    let profile = &config.profiles["qa"];
    assert!(profile.files_api_key.is_none());
    assert!(profile.tags.is_empty());
}

/// The plaintext path puts the CAPI secret in this file, so that save must not be readable by
/// other users. The ordinary save is left as it is, to keep every other command's behaviour.
#[cfg(unix)]
#[test]
fn owner_only_save_is_0600_and_tightens_an_existing_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");

    let mut config = Config::default();
    config
        .apply_cloud_login(
            &CredentialStore::plaintext(),
            "qa",
            &minted(),
            Some(qa_cloud_auth()),
            true,
        )
        .unwrap();
    config.save_to_path_owner_only(&path).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "got {mode:o}");

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    config.save_to_path_owner_only(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "an existing loose file is tightened, got {mode:o}"
    );
    // Written via a sibling and renamed, so nothing is left behind and the secret is never on
    // disk at the looser permissions.
    let strays: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .filter(|n| n != "config.toml")
        .collect();
    assert!(strays.is_empty(), "left temporary files behind: {strays:?}");

    // A descriptor opened before the save keeps the *old* inode, so it cannot read the secret
    // written afterwards — which is what the in-place variant could not promise.
    let mut prior = std::fs::File::open(&path).unwrap();
    let mut config = config.clone();
    config.profiles.remove("qa");
    config.save_to_path_owner_only(&path).unwrap();
    let mut seen = String::new();
    std::io::Read::read_to_string(&mut prior, &mut seen).unwrap();
    assert!(
        seen.contains("[profiles.qa]"),
        "a pre-existing handle should still see the old contents"
    );
}

/// Two variables can resolve to the same string today and diverge tomorrow, so a reference has to
/// be restored on the field that held it — not wherever that value happens to appear. Restoring by
/// value alone pointed both profiles at the first variable, which silently selects the wrong
/// credential once they differ.
#[test]
fn saving_keeps_each_reference_on_its_own_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[profiles.prod]
deployment_type = "cloud"
api_key = "${B_PROD_KEY}"
api_secret = "shared-secret-value"
api_url = "https://api.redislabs.com/v1"

[profiles.stage]
deployment_type = "cloud"
api_key = "${B_STAGE_KEY}"
api_secret = "shared-secret-value"
api_url = "https://api.redislabs.com/v1"
"#,
    )
    .unwrap();

    // SAFETY: single-threaded test; these names are read nowhere else.
    unsafe {
        std::env::set_var("B_PROD_KEY", "identical-for-now");
        std::env::set_var("B_STAGE_KEY", "identical-for-now");
    }
    let config = Config::load_from_path(&path).unwrap();
    config.save_to_path(&path).unwrap();
    unsafe {
        std::env::remove_var("B_PROD_KEY");
        std::env::remove_var("B_STAGE_KEY");
    }

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("${B_PROD_KEY}") && written.contains("${B_STAGE_KEY}"),
        "each profile keeps its own variable:\n{written}"
    );
    assert!(
        !written.contains("identical-for-now"),
        "neither resolved value should be written:\n{written}"
    );
    // The literal that merely equals a resolved value is left alone.
    assert_eq!(
        written.matches("shared-secret-value").count(),
        2,
        "a literal field must not be rewritten as a reference:\n{written}"
    );
}

/// Profile names are unrestricted strings, so a name with a dot is quoted in the file and its
/// field path is still three segments. Anything that reconstructs the path from the serialized
/// text has to implement TOML quoting to see that — which is why the edit is made on the parsed
/// document instead.
#[test]
fn saving_keeps_a_reference_under_a_quoted_profile_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[profiles."prod.eu"]
deployment_type = "cloud"
api_key = "${B_DOTTED_KEY}"
api_secret = "s"
api_url = "https://api.redislabs.com/v1"
"#,
    )
    .unwrap();

    // SAFETY: single-threaded test; this name is read nowhere else.
    unsafe { std::env::set_var("B_DOTTED_KEY", "dotted-resolved-secret") };
    let config = Config::load_from_path(&path).unwrap();
    config.save_to_path(&path).unwrap();
    unsafe { std::env::remove_var("B_DOTTED_KEY") };

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("${B_DOTTED_KEY}"),
        "the reference should survive a quoted profile name:\n{written}"
    );
    assert!(
        !written.contains("dotted-resolved-secret"),
        "the resolved value should not be written:\n{written}"
    );
}

/// `${VAR:-default}` is supported by the loader, so it has to survive a save too. The reference is
/// kept whole rather than parsed, which is what makes that work.
#[test]
fn saving_keeps_a_reference_with_a_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[profiles.prod]
deployment_type = "cloud"
api_key = "${B_DEFAULTED_KEY:-fallback-key}"
api_secret = "s"
api_url = "https://api.redislabs.com/v1"
"#,
    )
    .unwrap();

    // SAFETY: single-threaded test; this name is read nowhere else.
    unsafe { std::env::set_var("B_DEFAULTED_KEY", "resolved-not-fallback") };
    let config = Config::load_from_path(&path).unwrap();
    config.save_to_path(&path).unwrap();
    unsafe { std::env::remove_var("B_DEFAULTED_KEY") };

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("${B_DEFAULTED_KEY:-fallback-key}"),
        "the whole reference should survive, defaults included:\n{written}"
    );
    assert!(
        !written.contains("resolved-not-fallback"),
        "the resolved value should not be written:\n{written}"
    );
}

/// A login saves the whole config, so a profile that keeps its secret in an environment variable
/// must not come back with that secret written into the file. The reference survives the
/// round trip; the value never lands on disk.
#[test]
fn saving_preserves_env_references_from_other_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[profiles.prod]
deployment_type = "cloud"
api_key = "${A_PROD_KEY}"
api_secret = "${A_PROD_SECRET}"
api_url = "https://api.redislabs.com/v1"
"#,
    )
    .unwrap();

    // SAFETY: single-threaded test; removed before it returns.
    unsafe {
        std::env::set_var("A_PROD_KEY", "live-key-must-not-be-written");
        std::env::set_var("A_PROD_SECRET", "live-secret-must-not-be-written");
    }
    let mut config = Config::load_from_path(&path).unwrap();

    // Reads still see the resolved value — this changes what a save writes, nothing else.
    let ProfileCredentials::Cloud { api_key, .. } = &config.profiles["prod"].credentials else {
        panic!("expected a cloud profile");
    };
    assert_eq!(api_key, "live-key-must-not-be-written");

    // An unrelated profile logs in, which rewrites the file.
    config
        .apply_cloud_login(
            &CredentialStore::plaintext(),
            "scratch",
            &minted(),
            Some(qa_cloud_auth()),
            false,
        )
        .unwrap();
    config.save_to_path_owner_only(&path).unwrap();
    unsafe {
        std::env::remove_var("A_PROD_KEY");
        std::env::remove_var("A_PROD_SECRET");
    }

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        !written.contains("live-key-must-not-be-written")
            && !written.contains("live-secret-must-not-be-written"),
        "a resolved secret reached the file:\n{written}"
    );
    assert!(
        written.contains("${A_PROD_KEY}") && written.contains("${A_PROD_SECRET}"),
        "the references should survive the save:\n{written}"
    );

    // And the file still loads to the same resolved values.
    unsafe { std::env::set_var("A_PROD_KEY", "live-key-must-not-be-written") };
    let reloaded = Config::load_from_path(&path).unwrap();
    unsafe { std::env::remove_var("A_PROD_KEY") };
    let ProfileCredentials::Cloud { api_key, .. } = &reloaded.profiles["prod"].credentials else {
        panic!("expected a cloud profile");
    };
    assert_eq!(api_key, "live-key-must-not-be-written");
}
