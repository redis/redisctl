//! Connection management for Redis Cloud and Enterprise clients.

use crate::error::Result as CliResult;
use redisctl_core::{ClientResolver, Config, EnvironmentOverrides};
use tracing::{debug, info};

pub use redisctl_core::{
    ResolvedCloudConnection as CloudConnectionInfo,
    ResolvedEnterpriseConnection as EnterpriseConnectionInfo,
};

/// User agent string for redisctl HTTP requests. Defined once in the core crate so the CLI, the
/// MCP server and the login flows cannot drift apart.
const REDISCTL_USER_AGENT: &str = redisctl_core::USER_AGENT;

/// Connection manager for creating authenticated clients.
#[derive(Clone)]
pub struct ConnectionManager {
    pub config: Config,
    pub config_path: Option<std::path::PathBuf>,
}

impl ConnectionManager {
    /// Create a new connection manager with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            config_path: None,
        }
    }

    /// Create a new connection manager with a custom config path.
    pub fn with_config_path(config: Config, config_path: Option<std::path::PathBuf>) -> Self {
        Self {
            config,
            config_path,
        }
    }

    /// Resolve Cloud connection info without creating an HTTP client.
    pub fn resolve_cloud_connection(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<CloudConnectionInfo> {
        self.resolver()
            .resolve_cloud(profile_name)
            .map_err(Into::into)
    }

    /// Create a Cloud client from resolved profile credentials.
    pub async fn create_cloud_client(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<redis_cloud::CloudClient> {
        debug!("Creating Redis Cloud client");
        let connection = self.resolve_cloud_connection(profile_name)?;
        info!("Connecting to Redis Cloud API: {}", connection.base_url);
        let client = connection.build_client()?;
        debug!("Redis Cloud client created successfully");
        Ok(client)
    }

    /// Resolve Enterprise connection info without creating an HTTP client.
    pub fn resolve_enterprise_connection(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<EnterpriseConnectionInfo> {
        self.resolver()
            .resolve_enterprise(profile_name)
            .map_err(Into::into)
    }

    /// Create an Enterprise client from resolved profile credentials.
    pub async fn create_enterprise_client(
        &self,
        profile_name: Option<&str>,
    ) -> CliResult<redis_enterprise::EnterpriseClient> {
        debug!("Creating Redis Enterprise client");
        let connection = self.resolve_enterprise_connection(profile_name)?;
        info!("Connecting to Redis Enterprise: {}", connection.base_url);
        let client = connection.build_client()?;
        debug!("Redis Enterprise client created successfully");
        Ok(client)
    }

    fn resolver(&self) -> ClientResolver<'_> {
        let environment_overrides = if self.config_path.is_some() {
            EnvironmentOverrides::Disabled
        } else {
            EnvironmentOverrides::Enabled
        };

        ClientResolver::new(&self.config)
            .environment_overrides(environment_overrides)
            .user_agent(REDISCTL_USER_AGENT)
    }
}
