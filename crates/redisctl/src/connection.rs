//! Connection management for Redis Cloud and Enterprise clients

use crate::error::Result as CliResult;
use anyhow::Context;
use redisctl_core::{Config, DeploymentType};
use tracing::{debug, info, trace};

/// User agent string for redisctl HTTP requests
const REDISCTL_USER_AGENT: &str = concat!("redisctl/", env!("CARGO_PKG_VERSION"));

/// Resolved Cloud connection details (without creating an HTTP client)
// The credential fields are populated but not yet read by the curl builder in
// commands/curl.rs, so `api ... --curl` output omits its auth headers. Kept
// pending that fix (see #1049); do not delete the fields without fixing curl.
#[allow(dead_code)]
pub struct CloudConnectionInfo {
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
    pub user_agent: String,
}

/// Resolved Enterprise connection details (without creating an HTTP client)
// See CloudConnectionInfo: credential fields populated but not read by the curl
// builder, so `api ... --curl` output omits auth. Kept pending fix (see #1049).
#[allow(dead_code)]
pub struct EnterpriseConnectionInfo {
    pub base_url: String,
    pub username: String,
    pub password: Option<String>,
    pub insecure: bool,
    pub ca_cert: Option<String>,
    pub user_agent: String,
}

/// Connection manager for creating authenticated clients
#[derive(Clone)]
pub struct ConnectionManager {
    pub config: Config,
    pub config_path: Option<std::path::PathBuf>,
}

impl ConnectionManager {
    /// Create a new connection manager with the given configuration
    pub fn new(config: Config) -> Self {
        Self {
            config,
            config_path: None,
        }
    }

    /// Create a new connection manager with a custom config path
    pub fn with_config_path(config: Config, config_path: Option<std::path::PathBuf>) -> Self {
        Self {
            config,
            config_path,
        }
    }

    /// Resolve Cloud connection info without creating an HTTP client.
    ///
    /// Follows the same credential resolution logic as `create_cloud_client`:
    /// environment variables override profile config, and --config-file disables env vars.
    pub fn resolve_cloud_connection(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<CloudConnectionInfo> {
        let (api_key, api_secret, base_url) = self.resolve_cloud_credentials(profile_name)?;
        Ok(CloudConnectionInfo {
            base_url,
            api_key,
            api_secret,
            user_agent: REDISCTL_USER_AGENT.to_string(),
        })
    }

    /// Create a Cloud client from profile credentials with environment variable override support
    ///
    /// When --config-file is explicitly specified, environment variables are ignored to provide
    /// true configuration isolation. This allows testing with isolated configs and follows the
    /// principle of "explicit wins" (CLI args > env vars > defaults).
    pub async fn create_cloud_client(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<redis_cloud::CloudClient> {
        debug!("Creating Redis Cloud client");

        let (final_api_key, final_api_secret, final_api_url) =
            self.resolve_cloud_credentials(profile_name)?;

        info!("Connecting to Redis Cloud API: {}", final_api_url);
        trace!(
            "API key: {}...",
            &final_api_key[..final_api_key.len().min(8)]
        );

        // Create and configure the Cloud client
        let client = redis_cloud::CloudClient::builder()
            .api_key(&final_api_key)
            .api_secret(&final_api_secret)
            .base_url(&final_api_url)
            .user_agent(REDISCTL_USER_AGENT)
            .build()
            .context("Failed to create Redis Cloud client")?;

        debug!("Redis Cloud client created successfully");
        Ok(client)
    }

    /// Resolve Cloud credentials from profile and/or environment variables.
    fn resolve_cloud_credentials(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<(String, String, String)> {
        trace!("Profile name: {:?}", profile_name);

        // When --config-file is explicitly specified, ignore environment variables
        let use_env_vars = self.config_path.is_none();

        debug!(
            "Config path: {:?}, use_env_vars: {}",
            self.config_path, use_env_vars
        );

        if !use_env_vars {
            info!("--config-file specified explicitly, ignoring environment variables");
        }

        // Check if all required environment variables are present (only if we're using them)
        let env_api_key = if use_env_vars {
            std::env::var("REDIS_CLOUD_API_KEY").ok()
        } else {
            None
        };
        let env_api_secret = if use_env_vars {
            std::env::var("REDIS_CLOUD_SECRET_KEY")
                .ok()
                .or_else(|| std::env::var("REDIS_CLOUD_API_SECRET").ok())
        } else {
            None
        };
        let env_api_url = if use_env_vars {
            std::env::var("REDIS_CLOUD_API_URL").ok()
        } else {
            None
        };

        if env_api_key.is_some() {
            debug!("Found REDIS_CLOUD_API_KEY environment variable");
        }
        if env_api_secret.is_some() {
            debug!(
                "Found Redis Cloud secret environment variable (REDIS_CLOUD_SECRET_KEY or REDIS_CLOUD_API_SECRET)"
            );
        }
        if env_api_url.is_some() {
            debug!("Found REDIS_CLOUD_API_URL environment variable");
        }

        let result = if let (Some(key), Some(secret)) = (&env_api_key, &env_api_secret) {
            // Environment variables provide complete credentials
            info!("Using Redis Cloud credentials from environment variables");
            let url = env_api_url.unwrap_or_else(|| "https://api.redislabs.com/v1".to_string());
            (key.clone(), secret.clone(), url)
        } else {
            // Resolve the profile using type-specific logic
            let resolved_profile_name = self.config.resolve_cloud_profile(profile_name)?;
            info!("Using Redis Cloud profile: {}", resolved_profile_name);

            let profile = self.config.profiles.get(&resolved_profile_name).ok_or(
                crate::error::RedisCtlError::ProfileNotFound {
                    name: resolved_profile_name.clone(),
                },
            )?;

            // Verify it's a cloud profile
            if profile.deployment_type != DeploymentType::Cloud {
                return Err(crate::error::RedisCtlError::ProfileTypeMismatch {
                    name: resolved_profile_name.to_string(),
                    actual_type: match profile.deployment_type {
                        DeploymentType::Cloud => "cloud",
                        DeploymentType::Enterprise => "enterprise",
                        DeploymentType::Database => "database",
                    }
                    .to_string(),
                    expected_type: "cloud".to_string(),
                    available_profiles: self
                        .config
                        .get_profiles_of_type(DeploymentType::Cloud)
                        .into_iter()
                        .map(String::from)
                        .collect(),
                });
            }

            // Use the new resolve method which handles keyring lookup
            let (api_key, api_secret, api_url) = profile
                .resolve_cloud_credentials()
                .context("Failed to resolve Cloud credentials")?
                .context("Profile is not configured for Redis Cloud")?;

            // Check for partial overrides before consuming the Options
            let has_overrides =
                env_api_key.is_some() || env_api_secret.is_some() || env_api_url.is_some();

            // Allow partial environment variable overrides
            let key = env_api_key.unwrap_or(api_key);
            let secret = env_api_secret.unwrap_or(api_secret);
            let url = env_api_url.unwrap_or(api_url);

            if has_overrides {
                debug!("Applied partial environment variable overrides");
            }

            (key, secret, url)
        };

        Ok(result)
    }

    /// Resolve Enterprise connection info without creating an HTTP client.
    ///
    /// Follows the same credential resolution logic as `create_enterprise_client`:
    /// environment variables override profile config, and --config-file disables env vars.
    pub fn resolve_enterprise_connection(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<EnterpriseConnectionInfo> {
        let (url, username, password, insecure, ca_cert) =
            self.resolve_enterprise_credentials(profile_name)?;
        Ok(EnterpriseConnectionInfo {
            base_url: url,
            username,
            password,
            insecure,
            ca_cert,
            user_agent: REDISCTL_USER_AGENT.to_string(),
        })
    }

    /// Create an Enterprise client from profile credentials with environment variable override support
    ///
    /// When --config-file is explicitly specified, environment variables are ignored to provide
    /// true configuration isolation. This allows testing with isolated configs and follows the
    /// principle of "explicit wins" (CLI args > env vars > defaults).
    pub async fn create_enterprise_client(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<redis_enterprise::EnterpriseClient> {
        debug!("Creating Redis Enterprise client");

        let (final_url, final_username, final_password, final_insecure, final_ca_cert) =
            self.resolve_enterprise_credentials(profile_name)?;

        info!("Connecting to Redis Enterprise: {}", final_url);
        debug!("Username: {}", final_username);
        debug!(
            "Password: {}",
            if final_password.is_some() {
                "configured"
            } else {
                "not set"
            }
        );
        debug!("Insecure mode: {}", final_insecure);
        debug!(
            "CA cert: {}",
            if final_ca_cert.is_some() {
                "configured"
            } else {
                "not set"
            }
        );

        // Build the Enterprise client
        let mut builder = redis_enterprise::EnterpriseClient::builder()
            .base_url(&final_url)
            .username(&final_username)
            .user_agent(REDISCTL_USER_AGENT);

        // Add password if provided
        if let Some(ref password) = final_password {
            builder = builder.password(password);
            trace!("Password added to client builder");
        }

        // Set insecure flag if needed
        if final_insecure {
            builder = builder.insecure(true);
            debug!("SSL certificate verification disabled");
        }

        // Add CA certificate if provided
        if let Some(ref ca_cert_path) = final_ca_cert {
            builder = builder.ca_cert(ca_cert_path);
            debug!("Using custom CA certificate: {}", ca_cert_path);
        }

        let client = builder
            .build()
            .context("Failed to create Redis Enterprise client")?;

        debug!("Redis Enterprise client created successfully");
        Ok(client)
    }

    /// Resolve Enterprise credentials from profile and/or environment variables.
    #[allow(clippy::type_complexity)]
    fn resolve_enterprise_credentials(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<(String, String, Option<String>, bool, Option<String>)> {
        trace!("Profile name: {:?}", profile_name);

        // When --config-file is explicitly specified, ignore environment variables
        let use_env_vars = self.config_path.is_none();

        debug!(
            "Config path: {:?}, use_env_vars: {}",
            self.config_path, use_env_vars
        );

        if !use_env_vars {
            info!("--config-file specified explicitly, ignoring environment variables");
        }

        // Check if all required environment variables are present (only if we're using them)
        let env_url = if use_env_vars {
            std::env::var("REDIS_ENTERPRISE_URL").ok()
        } else {
            None
        };
        let env_user = if use_env_vars {
            std::env::var("REDIS_ENTERPRISE_USER").ok()
        } else {
            None
        };
        let env_password = if use_env_vars {
            std::env::var("REDIS_ENTERPRISE_PASSWORD").ok()
        } else {
            None
        };
        let env_insecure = if use_env_vars {
            std::env::var("REDIS_ENTERPRISE_INSECURE").ok()
        } else {
            None
        };
        let env_ca_cert = if use_env_vars {
            std::env::var("REDIS_ENTERPRISE_CA_CERT").ok()
        } else {
            None
        };

        if env_url.is_some() {
            debug!("Found REDIS_ENTERPRISE_URL environment variable");
        }
        if env_user.is_some() {
            debug!("Found REDIS_ENTERPRISE_USER environment variable");
        }
        if env_password.is_some() {
            debug!("Found REDIS_ENTERPRISE_PASSWORD environment variable");
        }
        if env_insecure.is_some() {
            debug!("Found REDIS_ENTERPRISE_INSECURE environment variable");
        }
        if env_ca_cert.is_some() {
            debug!("Found REDIS_ENTERPRISE_CA_CERT environment variable");
        }

        let result = if let (Some(url), Some(user)) = (&env_url, &env_user) {
            // Environment variables provide complete credentials
            info!("Using Redis Enterprise credentials from environment variables");
            let password = env_password.clone();
            let insecure = env_insecure
                .as_ref()
                .map(|s| s.to_lowercase() == "true" || s == "1")
                .unwrap_or(false);
            let ca_cert = env_ca_cert.clone();
            (url.clone(), user.clone(), password, insecure, ca_cert)
        } else {
            // Resolve the profile using type-specific logic
            let resolved_profile_name = self.config.resolve_enterprise_profile(profile_name)?;
            info!("Using Redis Enterprise profile: {}", resolved_profile_name);

            let profile = self.config.profiles.get(&resolved_profile_name).ok_or(
                crate::error::RedisCtlError::ProfileNotFound {
                    name: resolved_profile_name.clone(),
                },
            )?;

            // Verify it's an enterprise profile
            if profile.deployment_type != DeploymentType::Enterprise {
                return Err(crate::error::RedisCtlError::ProfileTypeMismatch {
                    name: resolved_profile_name.to_string(),
                    actual_type: match profile.deployment_type {
                        DeploymentType::Cloud => "cloud",
                        DeploymentType::Enterprise => "enterprise",
                        DeploymentType::Database => "database",
                    }
                    .to_string(),
                    expected_type: "enterprise".to_string(),
                    available_profiles: self
                        .config
                        .get_profiles_of_type(DeploymentType::Enterprise)
                        .into_iter()
                        .map(String::from)
                        .collect(),
                });
            }

            // Use the new resolve method which handles keyring lookup
            let (url, username, password, insecure, profile_ca_cert) = profile
                .resolve_enterprise_credentials()
                .context("Failed to resolve Enterprise credentials")?
                .context("Profile is not configured for Redis Enterprise")?;

            // Check for partial overrides before consuming the Options
            let has_overrides = env_url.is_some()
                || env_user.is_some()
                || env_password.is_some()
                || env_insecure.is_some()
                || env_ca_cert.is_some();

            // Allow partial environment variable overrides
            let final_url = env_url.unwrap_or(url);
            let final_user = env_user.unwrap_or(username);
            let final_password = env_password.or(password);
            let final_insecure = env_insecure
                .as_ref()
                .map(|s| s.to_lowercase() == "true" || s == "1")
                .unwrap_or(insecure);
            let final_ca_cert = env_ca_cert.or(profile_ca_cert);

            if has_overrides {
                debug!("Applied partial environment variable overrides");
            }

            (
                final_url,
                final_user,
                final_password,
                final_insecure,
                final_ca_cert,
            )
        };

        Ok(result)
    }
}
