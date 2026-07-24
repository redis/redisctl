//! Application state and credential resolution

use std::collections::HashMap;
use std::sync::Arc;

#[cfg(any(feature = "cloud", feature = "enterprise", feature = "database"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(feature = "cloud")]
use redis_cloud::CloudClient;
#[cfg(feature = "enterprise")]
use redis_enterprise::EnterpriseClient;
use redisctl_core::Config;
use tokio::sync::RwLock;

use crate::policy::{Policy, SafetyTier};

/// How credentials are resolved
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CredentialSource {
    /// Resolve from redisctl profiles (local mode)
    /// Empty vec means use default profiles from config
    Profiles(Vec<String>),
}

/// Cached API clients and connections (per-profile for multi-cluster support)
pub struct CachedClients {
    #[cfg(feature = "cloud")]
    pub cloud: HashMap<String, CloudClient>,
    #[cfg(feature = "enterprise")]
    pub enterprise: HashMap<String, EnterpriseClient>,
    #[cfg(feature = "database")]
    pub database: HashMap<String, crate::tools::redis::RedisConnection>,
}

/// Shared application state
pub struct AppState {
    /// Credential source configuration
    pub credential_source: CredentialSource,
    /// Resolved policy for granular tool access control
    pub policy: Arc<Policy>,
    /// Optional Redis database URL for direct connections
    pub database_url: Option<String>,
    /// Enable Redis Cluster mode (handles MOVED/ASK redirections)
    pub cluster: bool,
    /// Client name for CLIENT SETNAME (identifies connections in CLIENT LIST)
    pub client_name: Option<String>,
    /// redisctl config (for profile-based auth)
    config: Option<Config>,
    /// Configured profiles (for multi-cluster support)
    profiles: Vec<String>,
    /// Cached API clients (keyed by profile name, "_default" for default)
    #[allow(dead_code)]
    clients: RwLock<CachedClients>,
    /// Session-scoped command aliases (name → list of command arg arrays)
    #[cfg(feature = "database")]
    aliases: RwLock<HashMap<String, Vec<Vec<String>>>>,
}

impl AppState {
    /// Create new application state
    pub fn new(
        credential_source: CredentialSource,
        policy: Arc<Policy>,
        database_url: Option<String>,
        cluster: bool,
        client_name: Option<String>,
    ) -> Result<Self> {
        // Extract profiles list
        let CredentialSource::Profiles(profiles) = &credential_source;
        let profiles = profiles.clone();

        // Load config from profiles
        let config = Config::load().ok();

        Ok(Self {
            credential_source,
            policy,
            database_url,
            cluster,
            client_name,
            config,
            profiles,
            clients: RwLock::new(CachedClients {
                #[cfg(feature = "cloud")]
                cloud: HashMap::new(),
                #[cfg(feature = "enterprise")]
                enterprise: HashMap::new(),
                #[cfg(feature = "database")]
                database: HashMap::new(),
            }),
            #[cfg(feature = "database")]
            aliases: RwLock::new(HashMap::new()),
        })
    }

    /// Get the list of configured profiles
    #[allow(dead_code)]
    pub fn available_profiles(&self) -> &[String] {
        &self.profiles
    }

    /// Get or create Cloud API client for a specific profile
    ///
    /// If profile is None, uses the first configured profile or default from config
    #[cfg(feature = "cloud")]
    pub async fn cloud_client_for_profile(&self, profile: Option<&str>) -> Result<CloudClient> {
        let cache_key = profile.unwrap_or("_default").to_string();

        // Check cache first
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.cloud.get(&cache_key) {
                return Ok(client.clone());
            }
        }

        // Create new client
        let client = self.create_cloud_client(profile).await?;

        // Cache it
        {
            let mut clients = self.clients.write().await;
            clients.cloud.insert(cache_key, client.clone());
        }

        Ok(client)
    }

    /// Get or create Cloud API client (uses default profile)
    #[cfg(feature = "cloud")]
    #[allow(dead_code)]
    pub async fn cloud_client(&self) -> Result<CloudClient> {
        self.cloud_client_for_profile(None).await
    }

    /// Get or create Enterprise API client for a specific profile
    ///
    /// If profile is None, uses the first configured profile or default from config
    #[cfg(feature = "enterprise")]
    pub async fn enterprise_client_for_profile(
        &self,
        profile: Option<&str>,
    ) -> Result<EnterpriseClient> {
        let cache_key = profile.unwrap_or("_default").to_string();

        // Check cache first
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.enterprise.get(&cache_key) {
                return Ok(client.clone());
            }
        }

        // Create new client
        let client = self.create_enterprise_client(profile).await?;

        // Cache it
        {
            let mut clients = self.clients.write().await;
            clients.enterprise.insert(cache_key, client.clone());
        }

        Ok(client)
    }

    /// Get or create Enterprise API client (uses default profile)
    #[cfg(feature = "enterprise")]
    #[allow(dead_code)]
    pub async fn enterprise_client(&self) -> Result<EnterpriseClient> {
        self.enterprise_client_for_profile(None).await
    }

    /// Create a new Cloud client from credentials
    #[cfg(feature = "cloud")]
    async fn create_cloud_client(&self, profile: Option<&str>) -> Result<CloudClient> {
        match &self.credential_source {
            CredentialSource::Profiles(profiles) => {
                let config = self
                    .config
                    .as_ref()
                    .context("No redisctl config available")?;

                // Use specified profile, first configured profile, or let config resolve default
                let profile_to_use = profile
                    .map(|s| s.to_string())
                    .or_else(|| profiles.first().cloned());

                // Resolve the profile name
                let resolved_profile_name = config
                    .resolve_cloud_profile(profile_to_use.as_deref())
                    .context("Failed to resolve cloud profile")?;

                // Get the profile
                let profile = config
                    .profiles
                    .get(&resolved_profile_name)
                    .with_context(|| format!("Profile '{}' not found", resolved_profile_name))?;

                // Get credentials
                let (api_key, api_secret, _base_url) = profile
                    .resolve_cloud_credentials()
                    .context("Failed to resolve cloud credentials")?
                    .context("No cloud credentials in profile")?;

                CloudClient::builder()
                    .api_key(api_key)
                    .api_secret(api_secret)
                    .build()
                    .context("Failed to build Cloud client")
            }
        }
    }

    /// Create a new Enterprise client from credentials
    #[cfg(feature = "enterprise")]
    async fn create_enterprise_client(&self, profile: Option<&str>) -> Result<EnterpriseClient> {
        match &self.credential_source {
            CredentialSource::Profiles(profiles) => {
                let config = self
                    .config
                    .as_ref()
                    .context("No redisctl config available")?;

                // Use specified profile, first configured profile, or let config resolve default
                let profile_to_use = profile
                    .map(|s| s.to_string())
                    .or_else(|| profiles.first().cloned());

                // Resolve the profile name
                let resolved_profile_name = config
                    .resolve_enterprise_profile(profile_to_use.as_deref())
                    .context("Failed to resolve enterprise profile")?;

                // Get the profile
                let profile_config = config
                    .profiles
                    .get(&resolved_profile_name)
                    .with_context(|| format!("Profile '{}' not found", resolved_profile_name))?;

                // Get credentials
                let (url, username, password, insecure, ca_cert) = profile_config
                    .resolve_enterprise_credentials()
                    .context("Failed to resolve enterprise credentials")?
                    .context("No enterprise credentials in profile")?;

                let mut builder = EnterpriseClient::builder()
                    .base_url(&url)
                    .username(&username)
                    .insecure(insecure);

                if let Some(pwd) = password {
                    builder = builder.password(&pwd);
                }

                if let Some(cert_path) = ca_cert {
                    builder = builder.ca_cert(&cert_path);
                }

                builder.build().context("Failed to build Enterprise client")
            }
        }
    }

    /// Resolve a Redis database URL from a profile
    ///
    /// If profile is `None`, uses the first configured profile or default from config
    #[cfg(feature = "database")]
    pub fn database_url_for_profile(&self, profile: Option<&str>) -> Result<String> {
        let config = self
            .config
            .as_ref()
            .context("No redisctl config available")?;

        let profile_to_use = profile
            .map(|s| s.to_string())
            .or_else(|| self.profiles.first().cloned());

        let resolved_name = config
            .resolve_database_profile(profile_to_use.as_deref())
            .context("Failed to resolve database profile")?;

        let profile_config = config
            .profiles
            .get(&resolved_name)
            .with_context(|| format!("Profile '{}' not found", resolved_name))?;

        let (host, port, password, tls, username, database) = profile_config
            .resolve_database_credentials()
            .context("Failed to resolve database credentials")?
            .context("No database credentials in profile")?;

        // Build Redis URL: redis[s]://[username[:password]@]host:port[/database]
        let scheme = if tls { "rediss" } else { "redis" };
        let auth = match (username.as_str(), password) {
            ("", None) | ("default", None) => String::new(),
            (user, Some(pass)) => format!(
                "{}:{}@",
                urlencoding::encode(user),
                urlencoding::encode(&pass)
            ),
            (user, None) => format!("{}@", urlencoding::encode(user)),
        };
        let db_path = if database > 0 {
            format!("/{}", database)
        } else {
            String::new()
        };

        Ok(format!("{}://{}{}:{}{}", scheme, auth, host, port, db_path))
    }

    /// Get or create a cached Redis connection for a resolved URL.
    ///
    /// Connections are cached by URL. If a cached connection fails a PING
    /// health check, it is evicted and a fresh connection is created.
    ///
    /// Returns a `RedisConnection` (standalone or cluster) based on the
    /// `cluster` flag in AppState.
    #[cfg(feature = "database")]
    pub async fn redis_connection_for_url(
        &self,
        url: &str,
    ) -> Result<crate::tools::redis::RedisConnection> {
        use crate::tools::redis::RedisConnection;

        // Check cache first
        {
            let clients = self.clients.read().await;
            if let Some(conn) = clients.database.get(url) {
                // Quick health check -- if PING fails the connection is stale
                let mut test_conn = conn.clone();
                if redis::cmd("PING")
                    .query_async::<String>(&mut test_conn)
                    .await
                    .is_ok()
                {
                    return Ok(conn.clone());
                }
                // Fall through to evict + reconnect
            }
        }

        // Create new connection (or reconnect after eviction)
        let conn = if self.cluster {
            let client = redis::cluster::ClusterClient::new(vec![url])
                .context("Failed to create Redis cluster client")?;
            let mut cluster_conn = client
                .get_async_connection()
                .await
                .context("Failed to connect to Redis cluster")?;

            // Set client name if configured
            if let Some(ref name) = self.client_name {
                let _ = redis::cmd("CLIENT")
                    .arg("SETNAME")
                    .arg(name)
                    .query_async::<String>(&mut cluster_conn)
                    .await;
            }

            RedisConnection::Cluster(cluster_conn)
        } else {
            let client = redis::Client::open(url).context("Failed to create Redis client")?;
            let mut standalone_conn = client
                .get_multiplexed_async_connection()
                .await
                .context("Failed to connect to Redis")?;

            // Set client name if configured
            if let Some(ref name) = self.client_name {
                let _ = redis::cmd("CLIENT")
                    .arg("SETNAME")
                    .arg(name)
                    .query_async::<String>(&mut standalone_conn)
                    .await;
            }

            RedisConnection::Standalone(standalone_conn)
        };

        // Cache it
        {
            let mut clients = self.clients.write().await;
            clients.database.insert(url.to_string(), conn.clone());
        }

        Ok(conn)
    }

    /// Check if write operations are allowed by the global policy tier.
    ///
    /// Returns `true` for `ReadWrite` and `Full` tiers.
    /// Used for defense-in-depth in non-destructive write tool handlers.
    #[allow(dead_code)]
    pub fn is_write_allowed(&self) -> bool {
        matches!(
            self.policy.global_tier(),
            SafetyTier::ReadWrite | SafetyTier::Full
        )
    }

    /// Check if destructive operations are allowed by the global policy tier.
    ///
    /// Returns `true` only for `Full` tier.
    /// Used for defense-in-depth in destructive tool handlers.
    #[allow(dead_code)]
    pub fn is_destructive_allowed(&self) -> bool {
        matches!(self.policy.global_tier(), SafetyTier::Full)
    }

    /// Store a named command alias (session-scoped, in-memory only).
    #[cfg(feature = "database")]
    pub async fn set_alias(&self, name: String, commands: Vec<Vec<String>>) {
        let mut aliases = self.aliases.write().await;
        aliases.insert(name, commands);
    }

    /// Retrieve a named command alias.
    #[cfg(feature = "database")]
    pub async fn get_alias(&self, name: &str) -> Option<Vec<Vec<String>>> {
        let aliases = self.aliases.read().await;
        aliases.get(name).cloned()
    }

    /// List all aliases with their command counts.
    #[cfg(feature = "database")]
    pub async fn list_aliases(&self) -> Vec<(String, usize)> {
        let aliases = self.aliases.read().await;
        let mut entries: Vec<_> = aliases.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    /// Delete a named alias. Returns true if it existed.
    #[cfg(feature = "database")]
    pub async fn delete_alias(&self, name: &str) -> bool {
        let mut aliases = self.aliases.write().await;
        aliases.remove(name).is_some()
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        // Note: We don't clone the clients cache, each clone gets fresh cache
        Self {
            credential_source: self.credential_source.clone(),
            policy: self.policy.clone(),
            database_url: self.database_url.clone(),
            cluster: self.cluster,
            client_name: self.client_name.clone(),
            config: self.config.clone(),
            profiles: self.profiles.clone(),
            clients: RwLock::new(CachedClients {
                #[cfg(feature = "cloud")]
                cloud: HashMap::new(),
                #[cfg(feature = "enterprise")]
                enterprise: HashMap::new(),
                #[cfg(feature = "database")]
                database: HashMap::new(),
            }),
            #[cfg(feature = "database")]
            aliases: RwLock::new(HashMap::new()),
        }
    }
}

/// Test helpers for creating AppState with pre-configured clients
#[allow(dead_code)]
impl AppState {
    /// Create a default read-only policy for tests
    pub fn test_policy() -> Arc<Policy> {
        Arc::new(Policy::new(
            crate::policy::PolicyConfig::default(),
            std::collections::HashMap::new(),
            "test".to_string(),
        ))
    }

    /// Create a read-write policy for tests that exercise write tools
    pub fn test_write_policy() -> Arc<Policy> {
        Arc::new(Policy::new(
            crate::policy::PolicyConfig {
                tier: SafetyTier::ReadWrite,
                ..Default::default()
            },
            std::collections::HashMap::new(),
            "test".to_string(),
        ))
    }

    /// Create test state with a pre-configured Cloud client and write access
    /// enabled, for tests that exercise write tools.
    #[cfg(feature = "cloud")]
    pub fn with_cloud_client_write(client: CloudClient) -> Self {
        let mut state = Self::with_cloud_client(client);
        state.policy = Self::test_write_policy();
        state
    }

    /// Create test state with a pre-configured Cloud client
    #[cfg(feature = "cloud")]
    pub fn with_cloud_client(client: CloudClient) -> Self {
        let mut cloud = HashMap::new();
        cloud.insert("_default".to_string(), client);
        Self {
            credential_source: CredentialSource::Profiles(vec![]),
            policy: Self::test_policy(),
            database_url: None,
            cluster: false,
            client_name: None,
            config: None,
            profiles: vec![],
            clients: RwLock::new(CachedClients {
                cloud,
                #[cfg(feature = "enterprise")]
                enterprise: HashMap::new(),
                #[cfg(feature = "database")]
                database: HashMap::new(),
            }),
            #[cfg(feature = "database")]
            aliases: RwLock::new(HashMap::new()),
        }
    }

    /// Create test state with a pre-configured Enterprise client
    #[cfg(feature = "enterprise")]
    pub fn with_enterprise_client(client: EnterpriseClient) -> Self {
        let mut enterprise = HashMap::new();
        enterprise.insert("_default".to_string(), client);
        Self {
            credential_source: CredentialSource::Profiles(vec![]),
            policy: Self::test_policy(),
            database_url: None,
            cluster: false,
            client_name: None,
            config: None,
            profiles: vec![],
            clients: RwLock::new(CachedClients {
                #[cfg(feature = "cloud")]
                cloud: HashMap::new(),
                enterprise,
                #[cfg(feature = "database")]
                database: HashMap::new(),
            }),
            #[cfg(feature = "database")]
            aliases: RwLock::new(HashMap::new()),
        }
    }

    /// Create test state with both Cloud and Enterprise clients
    #[cfg(all(feature = "cloud", feature = "enterprise"))]
    pub fn with_clients(cloud: CloudClient, enterprise: EnterpriseClient) -> Self {
        let mut cloud_map = HashMap::new();
        cloud_map.insert("_default".to_string(), cloud);
        let mut enterprise_map = HashMap::new();
        enterprise_map.insert("_default".to_string(), enterprise);
        Self {
            credential_source: CredentialSource::Profiles(vec![]),
            policy: Self::test_policy(),
            database_url: None,
            cluster: false,
            client_name: None,
            config: None,
            profiles: vec![],
            clients: RwLock::new(CachedClients {
                cloud: cloud_map,
                enterprise: enterprise_map,
                #[cfg(feature = "database")]
                database: HashMap::new(),
            }),
            #[cfg(feature = "database")]
            aliases: RwLock::new(HashMap::new()),
        }
    }
}
