//! Connection management for Redis Cloud and Enterprise clients.

use crate::error::Result as CliResult;
use redisctl_core::{
    ClientResolver, Config, ConfigDocument, ConfigPathSegment, EnvironmentOverrides,
};
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
    document: Option<ConfigDocument>,
}

impl ConnectionManager {
    /// Create a new connection manager with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            config_path: None,
            document: None,
        }
    }

    /// Create a new connection manager with a custom config path.
    pub fn with_config_path(config: Config, config_path: Option<std::path::PathBuf>) -> Self {
        Self {
            config,
            config_path,
            document: None,
        }
    }

    /// Retain the document loaded at command startup, not a new snapshot taken at save time.
    pub fn with_document(
        document: ConfigDocument,
        config_path: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            config: document.config().clone(),
            config_path,
            document: Some(document),
        }
    }

    /// Persist one command's edits through its load-time document. In-memory connection-only
    /// managers deliberately cannot write files without provenance.
    pub fn save_config(
        &self,
        config: &Config,
        replacements: &[Vec<ConfigPathSegment>],
        owner_only: bool,
    ) -> CliResult<()> {
        let mut document = self.document.clone().ok_or_else(|| {
            crate::error::RedisCtlError::Configuration(
                "Configuration writes require a document loaded at command startup".into(),
            )
        })?;
        *document.config_mut() = config.clone();
        for path in replacements {
            document.allow_literal_replacement(path);
        }
        if owner_only {
            let path = document.source_path().to_path_buf();
            document.save_to_path_owner_only(&path)?;
        } else {
            document.save()?;
        }
        Ok(())
    }

    /// Credential fields are supplied intentionally when a command creates/replaces a profile.
    /// Metadata is excluded: retained tags and Files API keys keep their original references.
    pub fn credential_replacements(name: &str) -> Vec<Vec<ConfigPathSegment>> {
        [
            "deployment_type",
            "api_key",
            "api_secret",
            "api_url",
            "url",
            "username",
            "password",
            "ca_cert",
            "host",
        ]
        .into_iter()
        .map(|field| Self::key_path(&["profiles", name, field]))
        .collect()
    }

    pub fn key_path(keys: &[&str]) -> Vec<ConfigPathSegment> {
        keys.iter()
            .map(|key| ConfigPathSegment::Key((*key).into()))
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: &str = "files_api_key = '${REDISCTL_DOCUMENT_TEST_ABSENT:-synthetic-files}'\n[profiles.db]\ndeployment_type = 'database'\nhost = 'localhost'\nport = 6379\npassword = '${REDISCTL_DOCUMENT_TEST_ABSENT:-synthetic-password}'\n";

    fn fixture() -> (tempfile::TempDir, ConnectionManager) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, CONTENT).unwrap();
        let document = ConfigDocument::load_from_path(&path).unwrap();
        (
            directory,
            ConnectionManager::with_document(document, Some(path)),
        )
    }

    #[test]
    fn command_saves_preserve_untouched_references_on_both_writers() {
        for owner_only in [false, true] {
            let (_directory, manager) = fixture();
            let mut updated = manager.config.clone();
            updated.default_database = Some("db".into());
            manager
                .save_config(
                    &updated,
                    &[ConnectionManager::key_path(&["default_database"])],
                    owner_only,
                )
                .unwrap();
            let text = std::fs::read_to_string(manager.config_path.as_ref().unwrap()).unwrap();
            assert!(text.contains("${REDISCTL_DOCUMENT_TEST_ABSENT:-synthetic-password}"));
            assert!(text.contains("${REDISCTL_DOCUMENT_TEST_ABSENT:-synthetic-files}"));
            assert_eq!(
                Config::load_from_path(manager.config_path.as_ref().unwrap())
                    .unwrap()
                    .default_database
                    .as_deref(),
                Some("db")
            );
        }
    }

    #[test]
    fn command_save_does_not_reload_and_adopt_a_changed_source() {
        for owner_only in [false, true] {
            let (_directory, manager) = fixture();
            let path = manager.config_path.as_ref().unwrap();
            let intervening = "default_cloud = 'other-writer'\n";
            std::fs::write(path, intervening).unwrap();
            let error = manager
                .save_config(&manager.config, &[], owner_only)
                .unwrap_err();
            assert!(matches!(
                error,
                crate::error::RedisCtlError::Configuration(_)
            ));
            assert!(error.to_string().contains("source changed"));
            assert_eq!(std::fs::read_to_string(path).unwrap(), intervening);
        }
    }

    #[test]
    fn connection_only_manager_cannot_guess_a_write_baseline() {
        let manager = ConnectionManager::new(Config::default());
        let error = manager
            .save_config(&manager.config, &[], false)
            .unwrap_err();
        assert!(error.to_string().contains("require a document"));
    }

    #[test]
    fn new_missing_configuration_can_be_saved_without_losing_its_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/config.toml");
        let document = ConfigDocument::load_from_path(&path).unwrap();
        let manager = ConnectionManager::with_document(document, Some(path.clone()));
        manager.save_config(&manager.config, &[], false).unwrap();
        assert!(path.exists());
    }
}
