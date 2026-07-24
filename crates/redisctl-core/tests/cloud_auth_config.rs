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
    // Prod endpoints aren't provisioned yet, so it isn't complete, but the CAPI base is known.
    assert!(!resolved.is_complete());
    assert_eq!(resolved.capi_url, "https://api.redislabs.com/v1");
}

#[test]
fn apply_cloud_login_writes_profile_default_and_endpoints() {
    let mut config = Config::default();
    let store = CredentialStore::plaintext(); // never touches the keyring
    let creds = MintedCredentials {
        account_id: Some("112117".to_string()),
        email: Some("u@e.com".to_string()),
        api_key: "ACCT-KEY".to_string(),
        api_secret: "USER-SECRET".to_string(),
        api_url: "https://api.example.com/v1".to_string(),
        refresh_token: Some("RT".to_string()),
        capi_key_name: "redisctl-demo".to_string(),
        redisctl_key_count: 1,
    };

    config
        .apply_cloud_login(&store, "qa", &creds, Some(qa_cloud_auth()))
        .unwrap();

    // Default cloud profile is set.
    assert_eq!(config.default_cloud.as_deref(), Some("qa"));
    // Profile is a Cloud profile; plaintext store records the values as-is.
    let (key, secret, url) = config.profiles["qa"].cloud_credentials().unwrap();
    assert_eq!(key, "ACCT-KEY");
    assert_eq!(secret, "USER-SECRET");
    assert_eq!(url, "https://api.example.com/v1");
    // Login endpoints were recorded for re-login.
    assert_eq!(config.resolve_cloud_auth("qa"), qa_cloud_auth());

    // And it survives a save/load round-trip.
    let (_, loaded) = roundtrip(&config);
    assert_eq!(loaded.default_cloud.as_deref(), Some("qa"));
    assert_eq!(loaded.resolve_cloud_auth("qa"), qa_cloud_auth());
}

/// Mirrors what `cloud auth logout` does at the config layer: it removes the profile (and its
/// credentials) but must PRESERVE the `[cloud_auth.<profile>]` login endpoints, so a later
/// `auth login` still resolves them instead of failing with "not configured".
#[test]
fn logout_removes_profile_but_preserves_cloud_auth_endpoints() {
    let mut config = Config::default();
    config.set_profile("qa".to_string(), cloud_profile("https://api-qa.example/v1"));
    config.cloud_auth.insert("qa".to_string(), qa_cloud_auth());

    // The logout sequence: save endpoints, remove the profile, restore endpoints.
    let saved = config.cloud_auth.get("qa").cloned();
    config.remove_profile("qa");
    if let Some(auth) = saved {
        config.cloud_auth.insert("qa".to_string(), auth);
    }

    let (_, loaded) = roundtrip(&config);
    // Profile (credentials) is gone…
    assert!(!loaded.list_profiles().iter().any(|(n, _)| *n == "qa"));
    // …but the login endpoints survive so re-login works.
    let auth = loaded.resolve_cloud_auth("qa");
    assert!(auth.is_complete());
    assert_eq!(auth.okta_client_id, "test-client-id");
}
