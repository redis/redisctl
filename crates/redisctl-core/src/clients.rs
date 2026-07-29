//! Shared profile resolution and API client construction.
//!
//! This module contains the product-level connection behavior shared by the
//! redisctl CLI and MCP server. Frontends choose the profile and environment
//! policy, while this layer resolves credentials and consistently configures
//! the underlying API clients.

use crate::{Config, ConfigError, CredentialStore, DeploymentType, EnvironmentOverrides, Profile};
use thiserror::Error;

const DEFAULT_CLOUD_API_URL: &str = "https://api.redislabs.com/v1";
const DEFAULT_USER_AGENT: &str = concat!("redisctl-core/", env!("CARGO_PKG_VERSION"));

/// Errors produced while resolving connection settings or building API clients.
#[derive(Debug, Error)]
pub enum ClientResolutionError {
    /// The redisctl configuration could not be resolved.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// A selected profile has the wrong deployment type.
    #[error("Profile '{name}' is type '{actual_type}' but a {expected_type} profile is required")]
    ProfileTypeMismatch {
        /// Selected profile name.
        name: String,
        /// Deployment type stored on the selected profile.
        actual_type: DeploymentType,
        /// Deployment type required by the requested client.
        expected_type: DeploymentType,
        /// Other configured profiles with the required type.
        available_profiles: Vec<String>,
    },

    /// Required credential fields were empty or absent.
    #[error("Missing credentials for {deployment_type} profile '{profile_name}': {missing_fields}")]
    MissingCredentials {
        /// Profile name, or `environment` when a complete environment-only
        /// configuration was selected.
        profile_name: String,
        /// Deployment type being resolved.
        deployment_type: DeploymentType,
        /// Comma-separated credential field names.
        missing_fields: String,
    },

    /// Redis Cloud client construction failed.
    #[error("Failed to build Redis Cloud client: {0}")]
    CloudClient(#[source] redis_cloud::CloudError),

    /// Redis Enterprise client construction failed.
    #[error("Failed to build Redis Enterprise client: {0}")]
    EnterpriseClient(#[source] redis_enterprise::RestError),
}

/// Fully resolved Redis Cloud connection settings.
#[derive(Clone)]
pub struct ResolvedCloudConnection {
    /// Cloud API base URL.
    pub base_url: String,
    /// Cloud API key.
    pub api_key: String,
    /// Cloud API secret.
    pub api_secret: String,
    /// HTTP user-agent value.
    pub user_agent: String,
}

impl ResolvedCloudConnection {
    /// Build a Redis Cloud client from these resolved settings.
    pub fn build_client(&self) -> Result<redis_cloud::CloudClient, ClientResolutionError> {
        redis_cloud::CloudClient::builder()
            .api_key(&self.api_key)
            .api_secret(&self.api_secret)
            .base_url(&self.base_url)
            .user_agent(&self.user_agent)
            .build()
            .map_err(ClientResolutionError::CloudClient)
    }
}

/// Fully resolved Redis Enterprise connection settings.
#[derive(Clone)]
pub struct ResolvedEnterpriseConnection {
    /// Enterprise REST API base URL.
    pub base_url: String,
    /// Enterprise API username.
    pub username: String,
    /// Enterprise API password.
    pub password: Option<String>,
    /// Whether TLS certificate verification is disabled.
    pub insecure: bool,
    /// Optional custom CA certificate path.
    pub ca_cert: Option<String>,
    /// HTTP user-agent value.
    pub user_agent: String,
}

impl ResolvedEnterpriseConnection {
    /// Build a Redis Enterprise client from these resolved settings.
    pub fn build_client(
        &self,
    ) -> Result<redis_enterprise::EnterpriseClient, ClientResolutionError> {
        let mut builder = redis_enterprise::EnterpriseClient::builder()
            .base_url(&self.base_url)
            .username(&self.username)
            .insecure(self.insecure)
            .user_agent(&self.user_agent);

        if let Some(password) = &self.password {
            builder = builder.password(password);
        }

        if let Some(ca_cert) = &self.ca_cert {
            builder = builder.ca_cert(ca_cert);
        }

        builder
            .build()
            .map_err(ClientResolutionError::EnterpriseClient)
    }
}

/// Resolves configured profiles into typed connection settings and API clients.
pub struct ClientResolver<'a> {
    config: &'a Config,
    environment_overrides: EnvironmentOverrides,
    user_agent: String,
}

impl<'a> ClientResolver<'a> {
    /// Create a resolver that allows supported environment variables to
    /// override stored profile values.
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            environment_overrides: EnvironmentOverrides::Enabled,
            user_agent: DEFAULT_USER_AGENT.to_string(),
        }
    }

    /// Set whether process environment variables may override profile values.
    #[must_use]
    pub fn environment_overrides(mut self, environment_overrides: EnvironmentOverrides) -> Self {
        self.environment_overrides = environment_overrides;
        self
    }

    /// Set the product-specific HTTP user agent applied to built clients.
    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Resolve Redis Cloud connection settings.
    pub fn resolve_cloud(
        &self,
        explicit_profile: Option<&str>,
    ) -> Result<ResolvedCloudConnection, ClientResolutionError> {
        let env_api_key = self.environment_value("REDIS_CLOUD_API_KEY");
        let env_api_secret = self
            .environment_value("REDIS_CLOUD_SECRET_KEY")
            .or_else(|| self.environment_value("REDIS_CLOUD_API_SECRET"));
        let env_api_url = self.environment_value("REDIS_CLOUD_API_URL");

        let (profile_name, api_key, api_secret, base_url) = if let (
            None,
            Some(api_key),
            Some(api_secret),
        ) =
            (explicit_profile, &env_api_key, &env_api_secret)
        {
            (
                "environment".to_string(),
                api_key.clone(),
                api_secret.clone(),
                env_api_url.unwrap_or_else(|| DEFAULT_CLOUD_API_URL.to_string()),
            )
        } else {
            let (profile_name, profile) =
                self.resolve_profile(explicit_profile, DeploymentType::Cloud)?;
            let (stored_api_key, stored_api_secret, stored_api_url) = profile
                    .cloud_credentials()
                    .ok_or_else(|| {
                        ConfigError::CredentialError(format!(
                            "Profile '{profile_name}' declares type cloud but contains different credentials"
                        ))
                    })?;
            let store = CredentialStore::new();

            let api_key = store.get_credential_with_environment(
                stored_api_key,
                &["REDIS_CLOUD_API_KEY"],
                self.environment_overrides,
            )?;
            let api_secret = store.get_credential_with_environment(
                stored_api_secret,
                &["REDIS_CLOUD_SECRET_KEY", "REDIS_CLOUD_API_SECRET"],
                self.environment_overrides,
            )?;
            let base_url = store.get_credential_with_environment(
                stored_api_url,
                &["REDIS_CLOUD_API_URL"],
                self.environment_overrides,
            )?;

            (profile_name, api_key, api_secret, base_url)
        };

        self.ensure_credentials(
            &profile_name,
            DeploymentType::Cloud,
            [
                ("api_key", api_key.as_str()),
                ("api_secret", api_secret.as_str()),
                ("api_url", base_url.as_str()),
            ],
        )?;

        Ok(ResolvedCloudConnection {
            base_url,
            api_key,
            api_secret,
            user_agent: self.user_agent.clone(),
        })
    }

    /// Resolve Redis Enterprise connection settings.
    pub fn resolve_enterprise(
        &self,
        explicit_profile: Option<&str>,
    ) -> Result<ResolvedEnterpriseConnection, ClientResolutionError> {
        let env_url = self.environment_value("REDIS_ENTERPRISE_URL");
        let env_username = self.environment_value("REDIS_ENTERPRISE_USER");
        let env_password = self.environment_value("REDIS_ENTERPRISE_PASSWORD");
        let env_insecure = self.environment_value("REDIS_ENTERPRISE_INSECURE");
        let env_ca_cert = self.environment_value("REDIS_ENTERPRISE_CA_CERT");

        let (profile_name, base_url, username, password, insecure, ca_cert) = if let (
            None,
            Some(url),
            Some(username),
        ) =
            (explicit_profile, &env_url, &env_username)
        {
            (
                "environment".to_string(),
                url.clone(),
                username.clone(),
                env_password,
                env_insecure.as_deref().map(parse_bool).unwrap_or_default(),
                env_ca_cert,
            )
        } else {
            let (profile_name, profile) =
                self.resolve_profile(explicit_profile, DeploymentType::Enterprise)?;
            let (stored_url, stored_username, stored_password, stored_insecure, stored_ca_cert) =
                    profile
                        .enterprise_credentials()
                        .ok_or_else(|| {
                            ConfigError::CredentialError(format!(
                                "Profile '{profile_name}' declares type enterprise but contains different credentials"
                            ))
                        })?;
            let store = CredentialStore::new();

            let base_url = store.get_credential_with_environment(
                stored_url,
                &["REDIS_ENTERPRISE_URL"],
                self.environment_overrides,
            )?;
            let username = store.get_credential_with_environment(
                stored_username,
                &["REDIS_ENTERPRISE_USER"],
                self.environment_overrides,
            )?;
            let password = match stored_password {
                Some(stored_password) => Some(store.get_credential_with_environment(
                    stored_password,
                    &["REDIS_ENTERPRISE_PASSWORD"],
                    self.environment_overrides,
                )?),
                None => env_password,
            };
            let insecure = env_insecure
                .as_deref()
                .map(parse_bool)
                .unwrap_or(stored_insecure);
            let ca_cert = env_ca_cert.or_else(|| stored_ca_cert.map(ToString::to_string));

            (
                profile_name,
                base_url,
                username,
                password,
                insecure,
                ca_cert,
            )
        };

        self.ensure_credentials(
            &profile_name,
            DeploymentType::Enterprise,
            [
                ("url", base_url.as_str()),
                ("username", username.as_str()),
                ("password", password.as_deref().unwrap_or_default()),
            ],
        )?;

        Ok(ResolvedEnterpriseConnection {
            base_url,
            username,
            password,
            insecure,
            ca_cert,
            user_agent: self.user_agent.clone(),
        })
    }

    /// Resolve settings and build a Redis Cloud client.
    pub fn build_cloud_client(
        &self,
        explicit_profile: Option<&str>,
    ) -> Result<redis_cloud::CloudClient, ClientResolutionError> {
        self.resolve_cloud(explicit_profile)?.build_client()
    }

    /// Resolve settings and build a Redis Enterprise client.
    pub fn build_enterprise_client(
        &self,
        explicit_profile: Option<&str>,
    ) -> Result<redis_enterprise::EnterpriseClient, ClientResolutionError> {
        self.resolve_enterprise(explicit_profile)?.build_client()
    }

    fn environment_value(&self, name: &str) -> Option<String> {
        if self.environment_overrides == EnvironmentOverrides::Enabled {
            std::env::var(name).ok()
        } else {
            None
        }
    }

    fn resolve_profile(
        &self,
        explicit_profile: Option<&str>,
        expected_type: DeploymentType,
    ) -> Result<(String, &Profile), ClientResolutionError> {
        let profile_name = match expected_type {
            DeploymentType::Cloud => self.config.resolve_cloud_profile(explicit_profile)?,
            DeploymentType::Enterprise => {
                self.config.resolve_enterprise_profile(explicit_profile)?
            }
            DeploymentType::Database => unreachable!("database clients are resolved elsewhere"),
        };
        let profile = self.config.profiles.get(&profile_name).ok_or_else(|| {
            ConfigError::ProfileNotFound {
                name: profile_name.clone(),
            }
        })?;

        if profile.deployment_type != expected_type {
            return Err(ClientResolutionError::ProfileTypeMismatch {
                name: profile_name,
                actual_type: profile.deployment_type,
                expected_type,
                available_profiles: self
                    .config
                    .get_profiles_of_type(expected_type)
                    .into_iter()
                    .map(ToString::to_string)
                    .collect(),
            });
        }

        Ok((profile_name, profile))
    }

    fn ensure_credentials<const N: usize>(
        &self,
        profile_name: &str,
        deployment_type: DeploymentType,
        fields: [(&str, &str); N],
    ) -> Result<(), ClientResolutionError> {
        let missing_fields = fields
            .into_iter()
            .filter_map(|(name, value)| value.trim().is_empty().then_some(name))
            .collect::<Vec<_>>();

        if missing_fields.is_empty() {
            Ok(())
        } else {
            Err(ClientResolutionError::MissingCredentials {
                profile_name: profile_name.to_string(),
                deployment_type,
                missing_fields: missing_fields.join(", "),
            })
        }
    }
}

fn parse_bool(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProfileCredentials;
    use std::collections::HashMap;

    fn cloud_profile(api_key: &str, api_secret: &str, api_url: &str) -> Profile {
        Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: api_key.to_string(),
                api_secret: api_secret.to_string(),
                api_url: api_url.to_string(),
            },
            files_api_key: None,
            tags: Vec::new(),
        }
    }

    fn enterprise_profile(
        url: &str,
        username: &str,
        password: Option<&str>,
        insecure: bool,
        ca_cert: Option<&str>,
    ) -> Profile {
        Profile {
            deployment_type: DeploymentType::Enterprise,
            credentials: ProfileCredentials::Enterprise {
                url: url.to_string(),
                username: username.to_string(),
                password: password.map(ToString::to_string),
                insecure,
                ca_cert: ca_cert.map(ToString::to_string),
            },
            files_api_key: None,
            tags: Vec::new(),
        }
    }

    fn isolated_resolver(config: &Config) -> ClientResolver<'_> {
        ClientResolver::new(config).environment_overrides(EnvironmentOverrides::Disabled)
    }

    #[test]
    fn resolves_default_cloud_profile() {
        let mut profiles = HashMap::new();
        profiles.insert(
            "first".to_string(),
            cloud_profile("first-key", "first-secret", "https://first.example/v1"),
        );
        profiles.insert(
            "default".to_string(),
            cloud_profile(
                "default-key",
                "default-secret",
                "https://default.example/v1",
            ),
        );
        let config = Config {
            default_cloud: Some("default".to_string()),
            profiles,
            ..Config::default()
        };

        let resolved = isolated_resolver(&config).resolve_cloud(None).unwrap();

        assert_eq!(resolved.api_key, "default-key");
        assert_eq!(resolved.base_url, "https://default.example/v1");
    }

    #[test]
    fn resolves_default_enterprise_profile() {
        let mut profiles = HashMap::new();
        profiles.insert(
            "first".to_string(),
            enterprise_profile(
                "https://first.example:9443",
                "first-user",
                Some("first-password"),
                false,
                None,
            ),
        );
        profiles.insert(
            "default".to_string(),
            enterprise_profile(
                "https://default.example:9443",
                "default-user",
                Some("default-password"),
                true,
                None,
            ),
        );
        let config = Config {
            default_enterprise: Some("default".to_string()),
            profiles,
            ..Config::default()
        };

        let resolved = isolated_resolver(&config).resolve_enterprise(None).unwrap();

        assert_eq!(resolved.username, "default-user");
        assert_eq!(resolved.base_url, "https://default.example:9443");
        assert!(resolved.insecure);
    }

    #[test]
    fn resolves_explicit_profile_and_custom_endpoints() {
        let mut profiles = HashMap::new();
        profiles.insert(
            "cloud-a".to_string(),
            cloud_profile("key-a", "secret-a", "https://cloud-a.example/v1"),
        );
        profiles.insert(
            "cloud-b".to_string(),
            cloud_profile("key-b", "secret-b", "https://cloud-b.example/v1"),
        );
        profiles.insert(
            "enterprise".to_string(),
            enterprise_profile(
                "https://enterprise.example:9443",
                "admin",
                Some("secret"),
                false,
                Some("/etc/redis/ca.pem"),
            ),
        );
        let config = Config {
            profiles,
            ..Config::default()
        };

        let resolver = isolated_resolver(&config).user_agent("redisctl-test/1");
        let cloud = resolver.resolve_cloud(Some("cloud-b")).unwrap();
        let enterprise = resolver.resolve_enterprise(Some("enterprise")).unwrap();

        assert_eq!(cloud.api_key, "key-b");
        assert_eq!(cloud.base_url, "https://cloud-b.example/v1");
        assert_eq!(cloud.user_agent, "redisctl-test/1");
        assert_eq!(enterprise.base_url, "https://enterprise.example:9443");
        assert_eq!(enterprise.ca_cert.as_deref(), Some("/etc/redis/ca.pem"));
        assert_eq!(enterprise.user_agent, "redisctl-test/1");
    }

    #[test]
    fn reports_missing_credentials() {
        let mut profiles = HashMap::new();
        profiles.insert(
            "cloud".to_string(),
            cloud_profile("key", "", "https://api.example/v1"),
        );
        profiles.insert(
            "enterprise".to_string(),
            enterprise_profile("https://cluster:9443", "admin", None, false, None),
        );
        let config = Config {
            profiles,
            ..Config::default()
        };

        let cloud_error = isolated_resolver(&config)
            .resolve_cloud(Some("cloud"))
            .err()
            .expect("missing Cloud secret must fail");
        let enterprise_error = isolated_resolver(&config)
            .resolve_enterprise(Some("enterprise"))
            .err()
            .expect("missing Enterprise password must fail");

        assert!(matches!(
            cloud_error,
            ClientResolutionError::MissingCredentials {
                missing_fields,
                ..
            } if missing_fields == "api_secret"
        ));
        assert!(matches!(
            enterprise_error,
            ClientResolutionError::MissingCredentials {
                missing_fields,
                ..
            } if missing_fields == "password"
        ));
    }

    #[test]
    fn rejects_an_explicit_profile_of_the_wrong_type() {
        let mut profiles = HashMap::new();
        profiles.insert(
            "enterprise".to_string(),
            enterprise_profile("https://cluster:9443", "admin", Some("secret"), false, None),
        );
        let config = Config {
            profiles,
            ..Config::default()
        };

        let error = isolated_resolver(&config)
            .resolve_cloud(Some("enterprise"))
            .err()
            .expect("wrong profile type must fail");

        assert!(matches!(
            error,
            ClientResolutionError::ProfileTypeMismatch {
                actual_type: DeploymentType::Enterprise,
                expected_type: DeploymentType::Cloud,
                ..
            }
        ));
    }

    #[test]
    #[serial_test::serial(client_resolution_env)]
    fn environment_overrides_and_explicit_config_isolation_are_distinct() {
        unsafe {
            std::env::set_var("REDIS_CLOUD_API_KEY", "environment-key");
            std::env::set_var("REDIS_CLOUD_SECRET_KEY", "environment-secret");
            std::env::set_var("REDIS_CLOUD_API_URL", "https://environment.example/v1");
        }
        let mut profiles = HashMap::new();
        profiles.insert(
            "cloud".to_string(),
            cloud_profile(
                "profile-key",
                "profile-secret",
                "https://profile.example/v1",
            ),
        );
        let config = Config {
            profiles,
            ..Config::default()
        };

        let overridden = ClientResolver::new(&config)
            .resolve_cloud(Some("cloud"))
            .unwrap();
        let isolated = isolated_resolver(&config)
            .resolve_cloud(Some("cloud"))
            .unwrap();

        unsafe {
            std::env::remove_var("REDIS_CLOUD_API_KEY");
            std::env::remove_var("REDIS_CLOUD_SECRET_KEY");
            std::env::remove_var("REDIS_CLOUD_API_URL");
        }

        assert_eq!(overridden.api_key, "environment-key");
        assert_eq!(overridden.base_url, "https://environment.example/v1");
        assert_eq!(isolated.api_key, "profile-key");
        assert_eq!(isolated.base_url, "https://profile.example/v1");
    }

    #[test]
    #[serial_test::serial(client_resolution_env)]
    fn environment_can_override_keyring_references() {
        unsafe {
            std::env::set_var("REDIS_CLOUD_API_KEY", "environment-key");
            std::env::set_var("REDIS_CLOUD_SECRET_KEY", "environment-secret");
        }
        let mut profiles = HashMap::new();
        profiles.insert(
            "cloud".to_string(),
            cloud_profile(
                "keyring:cloud-key",
                "keyring:cloud-secret",
                "https://api.example/v1",
            ),
        );
        let config = Config {
            profiles,
            ..Config::default()
        };

        let resolved = ClientResolver::new(&config)
            .resolve_cloud(Some("cloud"))
            .unwrap();

        unsafe {
            std::env::remove_var("REDIS_CLOUD_API_KEY");
            std::env::remove_var("REDIS_CLOUD_SECRET_KEY");
        }

        assert_eq!(resolved.api_key, "environment-key");
        assert_eq!(resolved.api_secret, "environment-secret");
    }

    #[test]
    fn builds_clients_from_resolved_settings() {
        let cloud = ResolvedCloudConnection {
            base_url: "https://api.example/v1".to_string(),
            api_key: "key".to_string(),
            api_secret: "secret".to_string(),
            user_agent: "redisctl-test/1".to_string(),
        };
        let enterprise = ResolvedEnterpriseConnection {
            base_url: "https://cluster.example:9443".to_string(),
            username: "admin".to_string(),
            password: Some("secret".to_string()),
            insecure: true,
            ca_cert: None,
            user_agent: "redisctl-test/1".to_string(),
        };

        cloud.build_client().unwrap();
        enterprise.build_client().unwrap();
    }
}
