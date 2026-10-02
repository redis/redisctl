//! Experimental fixed-target host adapter. Database behavior remains library-owned.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use async_trait::async_trait;
use redis_mcp::{
    AccessMode, DirectRedis, DirectRedisCluster, OutputBudget, RedisCommand, RedisDeployment,
    RedisError, RedisErrorKind, RedisExecutor, RedisMcp, RedisValue, ToolFamily,
};
use redisctl_core::{Config, DeploymentType};
use tokio::sync::OnceCell;
use tower_mcp::McpRouter;

use crate::policy::{Policy, ToolSafety};
use crate::state::CredentialSource;

const FAMILIES: [ToolFamily; 2] = [ToolFamily::Keyspace, ToolFamily::Strings];

pub(crate) struct Options {
    pub timeout: Duration,
    pub budget: OutputBudget,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            timeout: redis_mcp::DEFAULT_COMMAND_TIMEOUT,
            budget: OutputBudget::default(),
        }
    }
}

// Intentionally no Debug implementation: a target owns resolved credentials.
pub(crate) struct Target {
    url: String,
    cluster: bool,
}

pub(crate) fn tool_names() -> Vec<&'static str> {
    redis_mcp::tool_names_for_families(AccessMode::ReadOnly, FAMILIES)
}

pub(crate) fn resolve_target(
    source: &CredentialSource,
    url: Option<&str>,
    cluster: bool,
) -> Result<Target> {
    let CredentialSource::Profiles(names) = source;
    let config = if names.is_empty() {
        None
    } else {
        // Config diagnostics can contain expanded credential values or TOML source.
        Some(Config::load().map_err(|_| {
            anyhow::anyhow!("Cannot load selected profile configuration; inspect it locally")
        })?)
    };
    resolve_from_config(config.as_ref(), names, url, cluster)
}

fn resolve_from_config(
    config: Option<&Config>,
    names: &[String],
    url: Option<&str>,
    cluster: bool,
) -> Result<Target> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name) {
            continue;
        }
        let profile = config
            .and_then(|config| config.profiles.get(name))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "A selected profile does not exist; inspect profile configuration locally"
                )
            })?;
        if profile.deployment_type == DeploymentType::Database {
            candidates.push(profile);
        }
    }
    if candidates.len() > 1 {
        bail!("Ambiguous database target: select exactly one database profile per process");
    }
    if url.is_some() && !candidates.is_empty() {
        bail!(
            "Conflicting database target sources: remove either the direct URL (including REDIS_URL) or the selected database profile"
        );
    }
    let url = if let Some(url) = url {
        url.to_string()
    } else if let Some(profile) = candidates.first() {
        let (host, port, password, tls, username, database) = profile
            .resolve_database_credentials()
            .map_err(|_| anyhow::anyhow!("Cannot resolve database profile credentials; inspect the credential source locally"))?
            .ok_or_else(|| anyhow::anyhow!("Selected database profile has no database credentials"))?;
        let scheme = if tls { "rediss" } else { "redis" };
        let auth = match (username.as_str(), password) {
            ("" | "default", None) => String::new(),
            (username, Some(password)) => format!(
                "{}:{}@",
                urlencoding::encode(username),
                urlencoding::encode(&password)
            ),
            (username, None) => format!("{}@", urlencoding::encode(username)),
        };
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host
        };
        format!("{scheme}://{auth}{host}:{port}/{database}")
    } else {
        bail!(
            "Missing database target: provide a direct URL or explicitly select one database profile"
        );
    };
    let deployment = if cluster {
        RedisDeployment::Cluster
    } else {
        RedisDeployment::Standalone
    };
    redis_mcp::validate_redis_target_url(&url, deployment).map_err(|_| {
        anyhow::anyhow!("Invalid database target; inspect its URL or profile locally")
    })?;
    Ok(Target { url, cluster })
}

enum Connected {
    Standalone(DirectRedis),
    Cluster(DirectRedisCluster),
}

struct LazyExecutor {
    target: Target,
    connected: OnceCell<Connected>,
    policy: Arc<Policy>,
    visible: HashSet<String>,
}

fn safe_error(error: RedisError) -> RedisError {
    // Preserve classification, never forward upstream diagnostics with host credentials.
    RedisError::new(
        error.kind(),
        "Redis database operation failed; connection details were redacted",
    )
}

#[async_trait]
impl RedisExecutor for LazyExecutor {
    async fn execute(&self, command: RedisCommand) -> Result<RedisValue, RedisError> {
        let name = command.tool_name();
        if command.required_access() != AccessMode::ReadOnly
            || !self.visible.contains(name)
            || !self
                .policy
                .is_named_tool_allowed(name, ToolSafety::ReadOnly)
        {
            return Err(RedisError::new(
                RedisErrorKind::Authorization,
                "Database operation denied by host policy",
            ));
        }
        // One fixed client per process, initialized only by a permitted call.
        // Failed initialization may retry the SAME target, never another profile.
        let connected = self
            .connected
            .get_or_try_init(|| async {
                if self.target.cluster {
                    DirectRedisCluster::connect([self.target.url.as_str()])
                        .await
                        .map(Connected::Cluster)
                        .map_err(safe_error)
                } else {
                    DirectRedis::connect(&self.target.url)
                        .await
                        .map(Connected::Standalone)
                        .map_err(safe_error)
                }
            })
            .await?;
        match connected {
            Connected::Standalone(client) => client.execute(command).await,
            Connected::Cluster(client) => client.execute(command).await,
        }
        .map_err(safe_error)
    }
}

pub(crate) fn router(
    target: Target,
    options: Options,
    policy: Arc<Policy>,
    visible: HashSet<String>,
) -> Result<McpRouter> {
    let deployment = if target.cluster {
        RedisDeployment::Cluster
    } else {
        RedisDeployment::Standalone
    };
    RedisMcp::builder(LazyExecutor {
        target,
        connected: OnceCell::new(),
        policy,
        visible,
    })
    .access(AccessMode::ReadOnly)
    .capabilities(redis_mcp::RedisCapabilities::unknown().with_deployment(deployment))
    .families(FAMILIES)
    .raw_commands(false)
    .command_timeout(options.timeout)
    .output_budget(options.budget)
    .try_build()
    .map_err(|_| anyhow::anyhow!("Invalid redis-mcp pilot limits or family configuration"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redisctl_core::{Profile, ProfileCredentials};

    fn config() -> Config {
        let mut config = Config::default();
        for name in ["first", "second"] {
            config.set_profile(
                name.to_string(),
                Profile {
                    deployment_type: DeploymentType::Database,
                    credentials: ProfileCredentials::Database {
                        host: "127.0.0.1".to_string(),
                        port: 6379,
                        username: "default".to_string(),
                        password: Some("fake-secret".to_string()),
                        tls: false,
                        database: 0,
                    },
                    files_api_key: None,
                    tags: Vec::new(),
                },
            );
        }
        config.default_database = Some("first".to_string());
        config
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn explicit_profile_is_fixed_and_duplicate_selection_is_idempotent() {
        let config = config();
        let target =
            resolve_from_config(Some(&config), &names(&["second", "second"]), None, false).unwrap();
        assert_eq!(target.url, "redis://default:fake-secret@127.0.0.1:6379/0");
    }

    #[test]
    fn profile_tls_ipv6_database_and_credentials_survive_url_encoding() {
        let mut config = config();
        config.profiles.get_mut("second").unwrap().credentials = ProfileCredentials::Database {
            host: "::1".to_string(),
            port: 6379,
            username: "user:name".to_string(),
            password: Some("p@ss?#/%:".to_string()),
            tls: true,
            database: 2,
        };
        let target = resolve_from_config(Some(&config), &names(&["second"]), None, false).unwrap();
        assert_eq!(
            target.url,
            "rediss://user%3Aname:p%40ss%3F%23%2F%25%3A@[::1]:6379/2"
        );
        assert!(resolve_from_config(Some(&config), &names(&["second"]), None, true).is_err());
    }

    #[test]
    fn implicit_defaults_unknown_profiles_and_ambiguity_fail_closed() {
        let config = config();
        for selected in [names(&[]), names(&["missing"]), names(&["first", "second"])] {
            assert!(resolve_from_config(Some(&config), &selected, None, false).is_err());
        }
        assert!(
            resolve_from_config(
                Some(&config),
                &names(&["first"]),
                Some("redis://127.0.0.1:6379"),
                false
            )
            .is_err()
        );
        let error = resolve_from_config(
            Some(&config),
            &names(&["first"]),
            Some("redis://:fake-secret@localhost"),
            false,
        )
        .err()
        .unwrap();
        assert!(!format!("{error:?}").contains("fake-secret"));
    }

    #[test]
    fn urls_are_validated_offline_without_echoing_credentials() {
        for url in [
            "http://default:fake-secret@localhost",
            "redis://default:fake-secret@localhost/not-a-db",
            "redis://default:fake-secret@localhost?insecure=true",
            "rediss://default:fake-secret@localhost/#fragment",
        ] {
            let error = resolve_from_config(None, &[], Some(url), false)
                .err()
                .unwrap();
            assert!(!format!("{error:?}").contains("fake-secret"));
        }
        assert!(resolve_from_config(None, &[], Some("rediss://localhost/0"), false).is_ok());
        assert!(resolve_from_config(None, &[], Some("redis://localhost/1"), true).is_err());
        assert!(resolve_from_config(None, &[], Some("redis://localhost/0"), true).is_ok());
    }

    #[test]
    fn management_profiles_do_not_become_database_targets() {
        let mut config = config();
        config.set_profile(
            "cloud".to_string(),
            Profile {
                deployment_type: DeploymentType::Cloud,
                credentials: ProfileCredentials::Cloud {
                    api_key: "fake-key".to_string(),
                    api_secret: "fake-secret".to_string(),
                    api_url: "https://api.redislabs.com/v1".to_string(),
                },
                files_api_key: None,
                tags: Vec::new(),
            },
        );
        assert!(resolve_from_config(Some(&config), &names(&["cloud"]), None, false).is_err());
        assert!(
            resolve_from_config(Some(&config), &names(&["cloud", "first"]), None, false).is_ok()
        );
        assert!(
            resolve_from_config(
                Some(&config),
                &names(&["cloud"]),
                Some("redis://127.0.0.1:6379"),
                false
            )
            .is_ok()
        );
    }
}
