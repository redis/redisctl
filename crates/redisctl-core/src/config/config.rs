//! Configuration management for Redis CLI tools
//!
//! Handles configuration loading from files, environment variables, and command-line arguments.
//! Configuration is stored in TOML format with support for multiple named profiles.

#[cfg(target_os = "macos")]
use directories::BaseDirs;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::credential::CredentialStore;
use super::error::{ConfigError, Result};

/// Main configuration structure
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Config {
    /// Default profile for enterprise commands
    #[serde(default, rename = "default_enterprise")]
    pub default_enterprise: Option<String>,
    /// Default profile for cloud commands
    #[serde(default, rename = "default_cloud")]
    pub default_cloud: Option<String>,
    /// Default profile for database commands
    #[serde(default, rename = "default_database")]
    pub default_database: Option<String>,
    /// Global Files.com API key for support package uploads
    /// Can be overridden per-profile. Supports keyring: prefix for secure storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_api_key: Option<String>,
    /// Map of profile name -> profile configuration
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
    /// Per-profile OIDC login endpoints for `cloud auth login`, keyed by profile name.
    /// A missing entry falls back to the built-in production defaults. Kept separate from the
    /// profile's stored credentials since it describes *how to log in*, not the resulting keys.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub cloud_auth: HashMap<String, CloudAuthConfig>,
}

/// Individual profile configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    /// Type of deployment this profile connects to
    pub deployment_type: DeploymentType,
    /// Connection credentials (flattened into the profile)
    #[serde(flatten)]
    pub credentials: ProfileCredentials,
    /// Files.com API key for this profile (overrides global setting)
    /// Supports keyring: prefix for secure storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_api_key: Option<String>,
    /// Tags for organizing profiles
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// OIDC endpoints used by `cloud auth login` for one environment. Held in `Config.cloud_auth`
/// keyed by profile name, so it serializes under `[cloud_auth.<name>]`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CloudAuthConfig {
    /// Okta authorization-server issuer, e.g. `https://<your-okta-issuer>/oauth2/default`.
    pub okta_issuer: String,
    /// Public Okta client id for the redisctl app.
    pub okta_client_id: String,
    /// SM API base used for the token→CAPI-key exchange, e.g. `https://<sm-api-host>/api/v1`.
    pub sm_api_url: String,
    /// Public CAPI base recorded in the resulting cloud profile (`api_url`).
    #[serde(default = "default_prod_capi_url")]
    pub capi_url: String,
    /// Which account the profile's key was last minted for.
    ///
    /// Recorded because it cannot be derived locally: an API key does not name its account, and
    /// `setcurrent` is session-scoped server-side, so a fresh sign-in reports the user's default
    /// account rather than the one this profile is on. Absent for profiles written before this
    /// existed, or set up by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<u64>,
}

fn default_prod_capi_url() -> String {
    "https://api.redislabs.com/v1".to_string()
}

impl CloudAuthConfig {
    /// Built-in production defaults, so `cloud auth login` works against production without a
    /// `[cloud_auth.<profile>]` section. The client id is a public OIDC identifier (the app is a
    /// public client with no secret), so it ships in the binary.
    pub fn prod_defaults() -> Self {
        Self {
            okta_issuer: "https://auth.redis.com/oauth2/default".to_string(),
            okta_client_id: "0oaw90hjzrLoATW0q5d7".to_string(),
            sm_api_url: "https://cloud.redis.io/api/v1".to_string(),
            capi_url: default_prod_capi_url(),
            account_id: None,
        }
    }

    /// Whether the login endpoints are fully specified (issuer, client id, SM API base).
    pub fn is_complete(&self) -> bool {
        !self.okta_issuer.is_empty()
            && !self.okta_client_id.is_empty()
            && !self.sm_api_url.is_empty()
    }
}

/// Supported deployment types
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentType {
    Cloud,
    Enterprise,
    Database,
}

/// Connection credentials for different deployment types
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ProfileCredentials {
    Cloud {
        api_key: String,
        api_secret: String,
        #[serde(default = "default_cloud_url")]
        api_url: String,
    },
    Enterprise {
        url: String,
        username: String,
        password: Option<String>, // Optional for interactive prompting
        #[serde(default)]
        insecure: bool,
        /// Path to custom CA certificate for TLS verification (Kubernetes deployments)
        #[serde(default)]
        ca_cert: Option<String>,
    },
    Database {
        host: String,
        port: u16,
        #[serde(default)]
        password: Option<String>,
        #[serde(default = "default_tls")]
        tls: bool,
        #[serde(default = "default_username")]
        username: String,
        #[serde(default)]
        database: u8,
    },
}

impl std::fmt::Display for DeploymentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentType::Cloud => write!(f, "cloud"),
            DeploymentType::Enterprise => write!(f, "enterprise"),
            DeploymentType::Database => write!(f, "database"),
        }
    }
}

fn default_tls() -> bool {
    true
}

fn default_username() -> String {
    "default".to_string()
}

impl Profile {
    /// Returns Cloud credentials if this is a Cloud profile
    pub fn cloud_credentials(&self) -> Option<(&str, &str, &str)> {
        match &self.credentials {
            ProfileCredentials::Cloud {
                api_key,
                api_secret,
                api_url,
            } => Some((api_key.as_str(), api_secret.as_str(), api_url.as_str())),
            _ => None,
        }
    }

    /// Returns Enterprise credentials if this is an Enterprise profile
    #[allow(clippy::type_complexity)]
    pub fn enterprise_credentials(&self) -> Option<(&str, &str, Option<&str>, bool, Option<&str>)> {
        match &self.credentials {
            ProfileCredentials::Enterprise {
                url,
                username,
                password,
                insecure,
                ca_cert,
            } => Some((
                url.as_str(),
                username.as_str(),
                password.as_deref(),
                *insecure,
                ca_cert.as_deref(),
            )),
            _ => None,
        }
    }

    /// Check if this profile has a stored password
    pub fn has_password(&self) -> bool {
        matches!(
            self.credentials,
            ProfileCredentials::Enterprise {
                password: Some(_),
                ..
            } | ProfileCredentials::Database {
                password: Some(_),
                ..
            }
        )
    }

    /// Get resolved Cloud credentials (with keyring support)
    pub fn resolve_cloud_credentials(&self) -> Result<Option<(String, String, String)>> {
        match &self.credentials {
            ProfileCredentials::Cloud {
                api_key,
                api_secret,
                api_url,
            } => {
                let store = CredentialStore::new();

                // Resolve each credential with environment variable fallback
                let resolved_key = store
                    .get_credential(api_key, Some("REDIS_CLOUD_API_KEY"))
                    .map_err(|e| {
                        ConfigError::CredentialError(format!("Failed to resolve API key: {}", e))
                    })?;
                let resolved_secret = store
                    .get_credential_with_env_vars(
                        api_secret,
                        vec!["REDIS_CLOUD_SECRET_KEY", "REDIS_CLOUD_API_SECRET"],
                    )
                    .map_err(|e| {
                        ConfigError::CredentialError(format!("Failed to resolve API secret: {}", e))
                    })?;
                let resolved_url = store
                    .get_credential(api_url, Some("REDIS_CLOUD_API_URL"))
                    .map_err(|e| {
                        ConfigError::CredentialError(format!("Failed to resolve API URL: {}", e))
                    })?;

                Ok(Some((resolved_key, resolved_secret, resolved_url)))
            }
            _ => Ok(None),
        }
    }

    /// Get resolved Enterprise credentials (with keyring support)
    #[allow(clippy::type_complexity)]
    pub fn resolve_enterprise_credentials(
        &self,
    ) -> Result<Option<(String, String, Option<String>, bool, Option<String>)>> {
        match &self.credentials {
            ProfileCredentials::Enterprise {
                url,
                username,
                password,
                insecure,
                ca_cert,
            } => {
                let store = CredentialStore::new();

                // Resolve each credential with environment variable fallback
                let resolved_url = store
                    .get_credential(url, Some("REDIS_ENTERPRISE_URL"))
                    .map_err(|e| {
                        ConfigError::CredentialError(format!("Failed to resolve URL: {}", e))
                    })?;
                let resolved_username = store
                    .get_credential(username, Some("REDIS_ENTERPRISE_USER"))
                    .map_err(|e| {
                        ConfigError::CredentialError(format!("Failed to resolve username: {}", e))
                    })?;
                let resolved_password = password
                    .as_ref()
                    .map(|p| {
                        store
                            .get_credential(p, Some("REDIS_ENTERPRISE_PASSWORD"))
                            .map_err(|e| {
                                ConfigError::CredentialError(format!(
                                    "Failed to resolve password: {}",
                                    e
                                ))
                            })
                    })
                    .transpose()?;

                Ok(Some((
                    resolved_url,
                    resolved_username,
                    resolved_password,
                    *insecure,
                    ca_cert.clone(),
                )))
            }
            _ => Ok(None),
        }
    }

    /// Returns Database credentials if this is a Database profile
    #[allow(clippy::type_complexity)]
    pub fn database_credentials(&self) -> Option<(&str, u16, Option<&str>, bool, &str, u8)> {
        match &self.credentials {
            ProfileCredentials::Database {
                host,
                port,
                password,
                tls,
                username,
                database,
            } => Some((
                host.as_str(),
                *port,
                password.as_deref(),
                *tls,
                username.as_str(),
                *database,
            )),
            _ => None,
        }
    }

    /// Get resolved Database credentials (with keyring support)
    #[allow(clippy::type_complexity)]
    pub fn resolve_database_credentials(
        &self,
    ) -> Result<Option<(String, u16, Option<String>, bool, String, u8)>> {
        match &self.credentials {
            ProfileCredentials::Database {
                host,
                port,
                password,
                tls,
                username,
                database,
            } => {
                let store = CredentialStore::new();

                // Resolve each credential with environment variable fallback
                let resolved_host =
                    store
                        .get_credential(host, Some("REDIS_HOST"))
                        .map_err(|e| {
                            ConfigError::CredentialError(format!("Failed to resolve host: {}", e))
                        })?;
                let resolved_username = store
                    .get_credential(username, Some("REDIS_USERNAME"))
                    .map_err(|e| {
                        ConfigError::CredentialError(format!("Failed to resolve username: {}", e))
                    })?;
                let resolved_password = password
                    .as_ref()
                    .map(|p| {
                        store
                            .get_credential(p, Some("REDIS_PASSWORD"))
                            .map_err(|e| {
                                ConfigError::CredentialError(format!(
                                    "Failed to resolve password: {}",
                                    e
                                ))
                            })
                    })
                    .transpose()?;

                Ok(Some((
                    resolved_host,
                    *port,
                    resolved_password,
                    *tls,
                    resolved_username,
                    *database,
                )))
            }
            _ => Ok(None),
        }
    }
}

impl Config {
    /// Get the first profile of the specified deployment type (sorted alphabetically by name)
    pub fn find_first_profile_of_type(&self, deployment_type: DeploymentType) -> Option<&str> {
        let mut profiles: Vec<_> = self
            .profiles
            .iter()
            .filter(|(_, p)| p.deployment_type == deployment_type)
            .map(|(name, _)| name.as_str())
            .collect();
        profiles.sort();
        profiles.first().copied()
    }

    /// Get all profiles of the specified deployment type
    pub fn get_profiles_of_type(&self, deployment_type: DeploymentType) -> Vec<&str> {
        let mut profiles: Vec<_> = self
            .profiles
            .iter()
            .filter(|(_, p)| p.deployment_type == deployment_type)
            .map(|(name, _)| name.as_str())
            .collect();
        profiles.sort();
        profiles
    }

    /// Resolve the profile to use for enterprise commands
    pub fn resolve_enterprise_profile(&self, explicit_profile: Option<&str>) -> Result<String> {
        if let Some(profile_name) = explicit_profile {
            // Explicitly specified profile
            return Ok(profile_name.to_string());
        }

        if let Some(ref default) = self.default_enterprise {
            // Type-specific default
            return Ok(default.clone());
        }

        if let Some(profile_name) = self.find_first_profile_of_type(DeploymentType::Enterprise) {
            // First enterprise profile
            return Ok(profile_name.to_string());
        }

        // No enterprise profiles available - suggest other profile types
        let cloud_profiles = self.get_profiles_of_type(DeploymentType::Cloud);
        let database_profiles = self.get_profiles_of_type(DeploymentType::Database);
        if !cloud_profiles.is_empty() || !database_profiles.is_empty() {
            let mut suggestions = Vec::new();
            if !cloud_profiles.is_empty() {
                suggestions.push(format!("cloud: {}", cloud_profiles.join(", ")));
            }
            if !database_profiles.is_empty() {
                suggestions.push(format!("database: {}", database_profiles.join(", ")));
            }
            Err(ConfigError::NoProfilesOfType {
                deployment_type: "enterprise".to_string(),
                suggestion: format!(
                    "Available profiles: {}. Use 'redisctl profile set' to create an enterprise profile.",
                    suggestions.join("; ")
                ),
            })
        } else {
            Err(ConfigError::NoProfilesOfType {
                deployment_type: "enterprise".to_string(),
                suggestion: "Use 'redisctl profile set' to create a profile.".to_string(),
            })
        }
    }

    /// Resolve the profile to use for cloud commands
    pub fn resolve_cloud_profile(&self, explicit_profile: Option<&str>) -> Result<String> {
        if let Some(profile_name) = explicit_profile {
            // Explicitly specified profile
            return Ok(profile_name.to_string());
        }

        if let Some(ref default) = self.default_cloud {
            // Type-specific default
            return Ok(default.clone());
        }

        if let Some(profile_name) = self.find_first_profile_of_type(DeploymentType::Cloud) {
            // First cloud profile
            return Ok(profile_name.to_string());
        }

        // No cloud profiles available - suggest other profile types
        let enterprise_profiles = self.get_profiles_of_type(DeploymentType::Enterprise);
        let database_profiles = self.get_profiles_of_type(DeploymentType::Database);
        if !enterprise_profiles.is_empty() || !database_profiles.is_empty() {
            let mut suggestions = Vec::new();
            if !enterprise_profiles.is_empty() {
                suggestions.push(format!("enterprise: {}", enterprise_profiles.join(", ")));
            }
            if !database_profiles.is_empty() {
                suggestions.push(format!("database: {}", database_profiles.join(", ")));
            }
            Err(ConfigError::NoProfilesOfType {
                deployment_type: "cloud".to_string(),
                suggestion: format!(
                    "Available profiles: {}. Use 'redisctl profile set' to create a cloud profile.",
                    suggestions.join("; ")
                ),
            })
        } else {
            Err(ConfigError::NoProfilesOfType {
                deployment_type: "cloud".to_string(),
                suggestion: "Use 'redisctl profile set' to create a profile.".to_string(),
            })
        }
    }

    /// Resolve the profile to use for database commands
    pub fn resolve_database_profile(&self, explicit_profile: Option<&str>) -> Result<String> {
        if let Some(profile_name) = explicit_profile {
            // Explicitly specified profile
            return Ok(profile_name.to_string());
        }

        if let Some(ref default) = self.default_database {
            // Type-specific default
            return Ok(default.clone());
        }

        if let Some(profile_name) = self.find_first_profile_of_type(DeploymentType::Database) {
            // First database profile
            return Ok(profile_name.to_string());
        }

        // No database profiles available - suggest other profile types
        let cloud_profiles = self.get_profiles_of_type(DeploymentType::Cloud);
        let enterprise_profiles = self.get_profiles_of_type(DeploymentType::Enterprise);
        if !cloud_profiles.is_empty() || !enterprise_profiles.is_empty() {
            let mut suggestions = Vec::new();
            if !cloud_profiles.is_empty() {
                suggestions.push(format!("cloud: {}", cloud_profiles.join(", ")));
            }
            if !enterprise_profiles.is_empty() {
                suggestions.push(format!("enterprise: {}", enterprise_profiles.join(", ")));
            }
            Err(ConfigError::NoProfilesOfType {
                deployment_type: "database".to_string(),
                suggestion: format!(
                    "Available profiles: {}. Use 'redisctl profile set' to create a database profile.",
                    suggestions.join("; ")
                ),
            })
        } else {
            Err(ConfigError::NoProfilesOfType {
                deployment_type: "database".to_string(),
                suggestion: "Use 'redisctl profile set' to create a profile.".to_string(),
            })
        }
    }

    /// Resolve the deployment type from the active profile context.
    ///
    /// Used by the arg-rewriting layer to infer whether a shared command
    /// (e.g. `database`, `user`, `acl`) should be routed to Cloud or Enterprise.
    ///
    /// Resolution order:
    /// 1. If `explicit_profile` is given, look it up and return its `deployment_type`
    /// 2. If only cloud profiles exist, return Cloud
    /// 3. If only enterprise profiles exist, return Enterprise
    /// 4. If both exist, return `Err(AmbiguousDeployment)` with guidance
    /// 5. If neither exists, return `Err(NoProfilesOfType)`
    pub fn resolve_profile_deployment(
        &self,
        explicit_profile: Option<&str>,
    ) -> Result<DeploymentType> {
        // 1. Explicit profile → look it up
        if let Some(name) = explicit_profile {
            let profile = self
                .profiles
                .get(name)
                .ok_or_else(|| ConfigError::ProfileNotFound {
                    name: name.to_string(),
                })?;
            return Ok(profile.deployment_type);
        }

        let has_cloud = self
            .profiles
            .values()
            .any(|p| p.deployment_type == DeploymentType::Cloud);
        let has_enterprise = self
            .profiles
            .values()
            .any(|p| p.deployment_type == DeploymentType::Enterprise);

        match (has_cloud, has_enterprise) {
            (true, false) => Ok(DeploymentType::Cloud),
            (false, true) => Ok(DeploymentType::Enterprise),
            (true, true) => Err(ConfigError::AmbiguousDeployment),
            (false, false) => Err(ConfigError::NoProfilesOfType {
                deployment_type: "cloud or enterprise".to_string(),
                suggestion: "Use 'redisctl profile set' to create a profile.".to_string(),
            }),
        }
    }

    /// Load configuration from the standard location
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        Self::load_from_path(&config_path)
    }

    /// Load configuration from a specific path
    pub fn load_from_path(config_path: &Path) -> Result<Self> {
        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(config_path).map_err(|e| ConfigError::LoadError {
            path: config_path.display().to_string(),
            source: e,
        })?;

        // Expand environment variables in the config content
        let expanded_content = Self::expand_env_vars(&content);

        let config: Config = toml::from_str(&expanded_content)?;

        Ok(config)
    }

    /// Save configuration to the standard location
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        self.save_to_path(&config_path)
    }

    /// Save configuration to a specific path
    pub fn save_to_path(&self, config_path: &Path) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ConfigError::SaveError {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        let content = toml::to_string_pretty(self)?;

        fs::write(config_path, content).map_err(|e| ConfigError::SaveError {
            path: config_path.display().to_string(),
            source: e,
        })?;

        Ok(())
    }

    /// Set or update a profile
    pub fn set_profile(&mut self, name: String, profile: Profile) {
        self.profiles.insert(name, profile);
    }

    /// Effective `cloud auth login` endpoints for a profile: its `cloud_auth` entry, or the
    /// built-in production defaults when none is set.
    pub fn resolve_cloud_auth(&self, profile_name: &str) -> CloudAuthConfig {
        self.cloud_auth
            .get(profile_name)
            .cloned()
            .unwrap_or_else(CloudAuthConfig::prod_defaults)
    }

    /// Persist a completed `cloud auth login` into `profile_name` and make it the default
    /// cloud profile.
    ///
    /// The CAPI key/secret are stored via `store` (keyring when available, else plaintext) and
    /// the profile records the resulting references. The Okta refresh token, if present, is
    /// stored under `<profile>-okta-refresh` (keyring only — with a plaintext store there is
    /// nowhere to keep it, so silent re-auth won't be available and a fresh login is needed
    /// when the CAPI key is lost). `cloud_auth` (the login endpoints) is recorded so re-login
    /// works without re-specifying it.
    pub fn apply_cloud_login(
        &mut self,
        store: &CredentialStore,
        profile_name: &str,
        creds: &crate::auth::MintedCredentials,
        cloud_auth: Option<CloudAuthConfig>,
        make_default: bool,
    ) -> Result<()> {
        let api_key =
            store.store_credential(&format!("{profile_name}-cloud-api-key"), &creds.api_key)?;
        let api_secret = store.store_credential(
            &format!("{profile_name}-cloud-api-secret"),
            &creds.api_secret,
        )?;
        if let Some(refresh) = &creds.refresh_token {
            // Reference is implicit (looked up by the well-known key on refresh); ignore it.
            let _ = store.store_credential(&format!("{profile_name}-okta-refresh"), refresh)?;
        }
        let profile = Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key,
                api_secret,
                api_url: creds.api_url.clone(),
            },
            files_api_key: None,
            tags: Vec::new(),
        };
        self.profiles.insert(profile_name.to_string(), profile);
        if let Some(mut auth) = cloud_auth {
            // Remember the account this key is for, so a later switch can say which one the
            // profile is on without asking the server (which would report the user's default).
            auth.account_id = creds.account_id;
            self.cloud_auth.insert(profile_name.to_string(), auth);
        }
        // Only a login bootstraps a profile; re-pointing the default is not something a caller
        // asked for when it merely changed which account an existing profile targets.
        if make_default {
            self.default_cloud = Some(profile_name.to_string());
        }
        Ok(())
    }

    /// Remove a profile by name
    pub fn remove_profile(&mut self, name: &str) -> Option<Profile> {
        // Clear type-specific defaults if this profile was set as default
        if self.default_enterprise.as_deref() == Some(name) {
            self.default_enterprise = None;
        }
        if self.default_cloud.as_deref() == Some(name) {
            self.default_cloud = None;
        }
        if self.default_database.as_deref() == Some(name) {
            self.default_database = None;
        }
        self.cloud_auth.remove(name);
        self.profiles.remove(name)
    }

    /// List all profiles sorted by name
    pub fn list_profiles(&self) -> Vec<(&String, &Profile)> {
        let mut profiles: Vec<_> = self.profiles.iter().collect();
        profiles.sort_by_key(|(name, _)| *name);
        profiles
    }

    /// Get the path to the configuration file
    ///
    /// On macOS, this supports both the standard macOS path and Linux-style ~/.config path:
    /// 1. Check ~/.config/redisctl/config.toml (Linux-style, preferred for consistency)
    /// 2. Fall back to ~/Library/Application Support/com.redis.redisctl/config.toml (macOS standard)
    ///
    /// On Linux: ~/.config/redisctl/config.toml
    /// On Windows: %APPDATA%\redis\redisctl\config.toml
    pub fn config_path() -> Result<PathBuf> {
        // On macOS, check for Linux-style path first for cross-platform consistency
        #[cfg(target_os = "macos")]
        {
            if let Some(base_dirs) = BaseDirs::new() {
                let home_dir = base_dirs.home_dir();
                let linux_style_path = home_dir
                    .join(".config")
                    .join("redisctl")
                    .join("config.toml");

                // If Linux-style config exists, use it
                if linux_style_path.exists() {
                    return Ok(linux_style_path);
                }

                // Also check if the config directory exists (user might have created it)
                if linux_style_path
                    .parent()
                    .map(|p| p.exists())
                    .unwrap_or(false)
                {
                    return Ok(linux_style_path);
                }
            }
        }

        // Use platform-specific standard path
        let proj_dirs =
            ProjectDirs::from("com", "redis", "redisctl").ok_or(ConfigError::ConfigDirError)?;

        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    /// Expand environment variables in configuration content
    ///
    /// Supports ${VAR} and ${VAR:-default} syntax for environment variable expansion.
    /// This allows configs to reference environment variables while maintaining
    /// static fallback values.
    ///
    /// Example:
    /// ```toml
    /// api_key = "${REDIS_CLOUD_API_KEY}"
    /// api_url = "${REDIS_CLOUD_API_URL:-https://api.redislabs.com/v1}"
    /// ```
    fn expand_env_vars(content: &str) -> String {
        // Use shellexpand::env_with_context_no_errors which returns unexpanded vars as-is
        // This prevents errors when env vars for unused profiles aren't set
        let expanded =
            shellexpand::env_with_context_no_errors(content, |var| std::env::var(var).ok());
        expanded.to_string()
    }
}

fn default_cloud_url() -> String {
    "https://api.redislabs.com/v1".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let mut config = Config::default();

        let cloud_profile = Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "test-key".to_string(),
                api_secret: "test-secret".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            files_api_key: None,
            tags: vec![],
        };

        config.set_profile("test".to_string(), cloud_profile);
        config.default_cloud = Some("test".to_string());

        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config.default_cloud, deserialized.default_cloud);
        assert_eq!(config.profiles.len(), deserialized.profiles.len());
    }

    #[test]
    fn test_profile_credential_access() {
        let cloud_profile = Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "key".to_string(),
                api_secret: "secret".to_string(),
                api_url: "url".to_string(),
            },
            files_api_key: None,
            tags: vec![],
        };

        let (key, secret, url) = cloud_profile.cloud_credentials().unwrap();
        assert_eq!(key, "key");
        assert_eq!(secret, "secret");
        assert_eq!(url, "url");
        assert!(cloud_profile.enterprise_credentials().is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_env_var_expansion() {
        // Test basic environment variable expansion
        unsafe {
            std::env::set_var("TEST_API_KEY", "test-key-value");
            std::env::set_var("TEST_API_SECRET", "test-secret-value");
        }

        let content = r#"
[profiles.test]
deployment_type = "cloud"
api_key = "${TEST_API_KEY}"
api_secret = "${TEST_API_SECRET}"
"#;

        let expanded = Config::expand_env_vars(content);
        assert!(expanded.contains("test-key-value"));
        assert!(expanded.contains("test-secret-value"));

        // Clean up
        unsafe {
            std::env::remove_var("TEST_API_KEY");
            std::env::remove_var("TEST_API_SECRET");
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_env_var_expansion_with_defaults() {
        // Test environment variable expansion with defaults
        unsafe {
            std::env::remove_var("NONEXISTENT_VAR"); // Ensure it doesn't exist
        }

        let content = r#"
[profiles.test]
deployment_type = "cloud"
api_key = "${NONEXISTENT_VAR:-default-key}"
api_url = "${NONEXISTENT_URL:-https://api.redislabs.com/v1}"
"#;

        let expanded = Config::expand_env_vars(content);
        assert!(expanded.contains("default-key"));
        assert!(expanded.contains("https://api.redislabs.com/v1"));
    }

    #[test]
    #[serial_test::serial]
    fn test_env_var_expansion_mixed() {
        // Test mixed static and dynamic values
        unsafe {
            std::env::set_var("TEST_DYNAMIC_KEY", "dynamic-value");
        }

        let content = r#"
[profiles.test]
deployment_type = "cloud"
api_key = "${TEST_DYNAMIC_KEY}"
api_secret = "static-secret"
api_url = "${MISSING_VAR:-https://api.redislabs.com/v1}"
"#;

        let expanded = Config::expand_env_vars(content);
        assert!(expanded.contains("dynamic-value"));
        assert!(expanded.contains("static-secret"));
        assert!(expanded.contains("https://api.redislabs.com/v1"));

        // Clean up
        unsafe {
            std::env::remove_var("TEST_DYNAMIC_KEY");
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_full_config_with_env_expansion() {
        // Test complete config parsing with environment variables
        unsafe {
            std::env::set_var("REDIS_TEST_KEY", "expanded-key");
            std::env::set_var("REDIS_TEST_SECRET", "expanded-secret");
        }

        let config_content = r#"
default_cloud = "test"

[profiles.test]
deployment_type = "cloud"
api_key = "${REDIS_TEST_KEY}"
api_secret = "${REDIS_TEST_SECRET}"
api_url = "${REDIS_TEST_URL:-https://api.redislabs.com/v1}"
"#;

        let expanded = Config::expand_env_vars(config_content);
        let config: Config = toml::from_str(&expanded).unwrap();

        assert_eq!(config.default_cloud, Some("test".to_string()));

        let profile = config.profiles.get("test").unwrap();
        let (key, secret, url) = profile.cloud_credentials().unwrap();
        assert_eq!(key, "expanded-key");
        assert_eq!(secret, "expanded-secret");
        assert_eq!(url, "https://api.redislabs.com/v1");

        // Clean up
        unsafe {
            std::env::remove_var("REDIS_TEST_KEY");
            std::env::remove_var("REDIS_TEST_SECRET");
        }
    }

    #[test]
    fn test_enterprise_profile_resolution() {
        let mut config = Config::default();

        // Add an enterprise profile
        let enterprise_profile = Profile {
            deployment_type: DeploymentType::Enterprise,
            credentials: ProfileCredentials::Enterprise {
                url: "https://localhost:9443".to_string(),
                username: "admin".to_string(),
                password: Some("password".to_string()),
                insecure: false,
                ca_cert: None,
            },
            files_api_key: None,
            tags: vec![],
        };
        config.set_profile("ent1".to_string(), enterprise_profile);

        // Test explicit profile
        assert_eq!(
            config.resolve_enterprise_profile(Some("ent1")).unwrap(),
            "ent1"
        );

        // Test first enterprise profile (no default set)
        assert_eq!(config.resolve_enterprise_profile(None).unwrap(), "ent1");

        // Set default enterprise
        config.default_enterprise = Some("ent1".to_string());
        assert_eq!(config.resolve_enterprise_profile(None).unwrap(), "ent1");
    }

    #[test]
    fn test_cloud_profile_resolution() {
        let mut config = Config::default();

        // Add a cloud profile
        let cloud_profile = Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "key".to_string(),
                api_secret: "secret".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            files_api_key: None,
            tags: vec![],
        };
        config.set_profile("cloud1".to_string(), cloud_profile);

        // Test explicit profile
        assert_eq!(
            config.resolve_cloud_profile(Some("cloud1")).unwrap(),
            "cloud1"
        );

        // Test first cloud profile (no default set)
        assert_eq!(config.resolve_cloud_profile(None).unwrap(), "cloud1");

        // Set default cloud
        config.default_cloud = Some("cloud1".to_string());
        assert_eq!(config.resolve_cloud_profile(None).unwrap(), "cloud1");
    }

    #[test]
    fn test_mixed_profile_resolution() {
        let mut config = Config::default();

        // Add a cloud profile
        let cloud_profile = Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "key".to_string(),
                api_secret: "secret".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            files_api_key: None,
            tags: vec![],
        };
        config.set_profile("cloud1".to_string(), cloud_profile.clone());
        config.set_profile("cloud2".to_string(), cloud_profile);

        // Add enterprise profiles
        let enterprise_profile = Profile {
            deployment_type: DeploymentType::Enterprise,
            credentials: ProfileCredentials::Enterprise {
                url: "https://localhost:9443".to_string(),
                username: "admin".to_string(),
                password: Some("password".to_string()),
                insecure: false,
                ca_cert: None,
            },
            files_api_key: None,
            tags: vec![],
        };
        config.set_profile("ent1".to_string(), enterprise_profile.clone());
        config.set_profile("ent2".to_string(), enterprise_profile);

        // Without defaults, should use first of each type
        assert_eq!(config.resolve_cloud_profile(None).unwrap(), "cloud1");
        assert_eq!(config.resolve_enterprise_profile(None).unwrap(), "ent1");

        // Set type-specific defaults
        config.default_cloud = Some("cloud2".to_string());
        config.default_enterprise = Some("ent2".to_string());

        // Should now use the type-specific defaults
        assert_eq!(config.resolve_cloud_profile(None).unwrap(), "cloud2");
        assert_eq!(config.resolve_enterprise_profile(None).unwrap(), "ent2");
    }

    #[test]
    fn test_no_profile_errors() {
        let config = Config::default();

        // No profiles at all
        assert!(config.resolve_enterprise_profile(None).is_err());
        assert!(config.resolve_cloud_profile(None).is_err());
    }

    #[test]
    fn test_wrong_profile_type_help() {
        let mut config = Config::default();

        // Only add cloud profiles
        let cloud_profile = Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "key".to_string(),
                api_secret: "secret".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            files_api_key: None,
            tags: vec![],
        };
        config.set_profile("cloud1".to_string(), cloud_profile);

        // Try to resolve enterprise profile - should get helpful error
        let err = config.resolve_enterprise_profile(None).unwrap_err();
        assert!(err.to_string().contains("No enterprise profiles"));
        assert!(err.to_string().contains("cloud: cloud1"));
    }

    #[test]
    fn test_database_profile_serialization() {
        let mut config = Config::default();

        let db_profile = Profile {
            deployment_type: DeploymentType::Database,
            credentials: ProfileCredentials::Database {
                host: "localhost".to_string(),
                port: 6379,
                password: Some("secret".to_string()),
                tls: true,
                username: "default".to_string(),
                database: 0,
            },
            files_api_key: None,
            tags: vec![],
        };

        config.set_profile("myredis".to_string(), db_profile);
        config.default_database = Some("myredis".to_string());

        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config.default_database, deserialized.default_database);
        assert_eq!(config.profiles.len(), deserialized.profiles.len());

        let profile = deserialized.profiles.get("myredis").unwrap();
        assert_eq!(profile.deployment_type, DeploymentType::Database);

        let (host, port, password, tls, username, database) =
            profile.database_credentials().unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 6379);
        assert_eq!(password, Some("secret"));
        assert!(tls);
        assert_eq!(username, "default");
        assert_eq!(database, 0);
    }

    #[test]
    fn test_database_profile_resolution() {
        let mut config = Config::default();

        // Add a database profile
        let db_profile = Profile {
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
        };
        config.set_profile("db1".to_string(), db_profile);

        // Test explicit profile
        assert_eq!(config.resolve_database_profile(Some("db1")).unwrap(), "db1");

        // Test first database profile (no default set)
        assert_eq!(config.resolve_database_profile(None).unwrap(), "db1");

        // Set default database
        config.default_database = Some("db1".to_string());
        assert_eq!(config.resolve_database_profile(None).unwrap(), "db1");
    }

    #[test]
    fn test_database_profile_defaults() {
        // Test that TLS defaults to true and username defaults to "default"
        let toml_content = r#"
[profiles.minimal]
deployment_type = "database"
host = "redis.example.com"
port = 12345
"#;
        let config: Config = toml::from_str(toml_content).unwrap();
        let profile = config.profiles.get("minimal").unwrap();

        let (host, port, password, tls, username, database) =
            profile.database_credentials().unwrap();
        assert_eq!(host, "redis.example.com");
        assert_eq!(port, 12345);
        assert!(password.is_none());
        assert!(tls); // defaults to true
        assert_eq!(username, "default"); // defaults to "default"
        assert_eq!(database, 0); // defaults to 0
    }

    // --- resolve_profile_deployment tests ---

    fn make_cloud_profile() -> Profile {
        Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "k".to_string(),
                api_secret: "s".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            files_api_key: None,
            tags: vec![],
        }
    }

    fn make_enterprise_profile() -> Profile {
        Profile {
            deployment_type: DeploymentType::Enterprise,
            credentials: ProfileCredentials::Enterprise {
                url: "https://localhost:9443".to_string(),
                username: "admin".to_string(),
                password: Some("pw".to_string()),
                insecure: false,
                ca_cert: None,
            },
            files_api_key: None,
            tags: vec![],
        }
    }

    #[test]
    fn test_resolve_profile_deployment_explicit_cloud() {
        let mut config = Config::default();
        config.set_profile("mycloud".to_string(), make_cloud_profile());
        config.set_profile("myent".to_string(), make_enterprise_profile());

        assert_eq!(
            config.resolve_profile_deployment(Some("mycloud")).unwrap(),
            DeploymentType::Cloud
        );
    }

    #[test]
    fn test_resolve_profile_deployment_explicit_enterprise() {
        let mut config = Config::default();
        config.set_profile("mycloud".to_string(), make_cloud_profile());
        config.set_profile("myent".to_string(), make_enterprise_profile());

        assert_eq!(
            config.resolve_profile_deployment(Some("myent")).unwrap(),
            DeploymentType::Enterprise
        );
    }

    #[test]
    fn test_resolve_profile_deployment_explicit_not_found() {
        let config = Config::default();
        let err = config
            .resolve_profile_deployment(Some("nonexistent"))
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_resolve_profile_deployment_cloud_only() {
        let mut config = Config::default();
        config.set_profile("c1".to_string(), make_cloud_profile());

        assert_eq!(
            config.resolve_profile_deployment(None).unwrap(),
            DeploymentType::Cloud
        );
    }

    #[test]
    fn test_resolve_profile_deployment_enterprise_only() {
        let mut config = Config::default();
        config.set_profile("e1".to_string(), make_enterprise_profile());

        assert_eq!(
            config.resolve_profile_deployment(None).unwrap(),
            DeploymentType::Enterprise
        );
    }

    #[test]
    fn test_resolve_profile_deployment_ambiguous() {
        let mut config = Config::default();
        config.set_profile("c1".to_string(), make_cloud_profile());
        config.set_profile("e1".to_string(), make_enterprise_profile());

        let err = config.resolve_profile_deployment(None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ambiguous deployment type"));
    }

    #[test]
    fn test_resolve_profile_deployment_no_profiles() {
        let config = Config::default();
        let err = config.resolve_profile_deployment(None).unwrap_err();
        assert!(err.to_string().contains("No cloud or enterprise"));
    }

    #[test]
    fn test_resolve_profile_deployment_ignores_database_profiles() {
        let mut config = Config::default();
        config.set_profile(
            "db1".to_string(),
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

        // Only database profiles → treated as "no profiles" for deployment resolution
        let err = config.resolve_profile_deployment(None).unwrap_err();
        assert!(err.to_string().contains("No cloud or enterprise"));
    }
}
