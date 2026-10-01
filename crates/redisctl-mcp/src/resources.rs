//! MCP Resources for Redis management
//!
//! Resources expose read-only data that can be fetched by URI.

use redisctl_core::Config;
use tower_mcp::protocol::{ReadResourceResult, ResourceContent};
use tower_mcp::resource::{Resource, ResourceBuilder};

/// Build a resource exposing the current configuration path
pub fn config_path_resource() -> Resource {
    ResourceBuilder::new("redis://config/path")
        .name("Configuration Path")
        .description("Path to the redisctl configuration file")
        .mime_type("text/plain")
        .handler(|| async {
            let path = Config::config_path()
                .map(|p: std::path::PathBuf| p.display().to_string())
                .unwrap_or_else(|_| "(no config path available)".to_string());

            Ok(ReadResourceResult {
                contents: vec![ResourceContent {
                    uri: "redis://config/path".to_string(),
                    mime_type: Some("text/plain".to_string()),
                    text: Some(path),
                    blob: None,
                    meta: None,
                }],
                meta: None,
                ..Default::default()
            })
        })
        .build()
}

/// Build a resource exposing the list of configured profiles
pub fn profiles_resource() -> Resource {
    ResourceBuilder::new("redis://profiles")
        .name("Profiles")
        .description("List of configured redisctl profiles")
        .mime_type("application/json")
        .handler(|| async {
            let profiles = match Config::load() {
                Ok(config) => {
                    let profile_names: Vec<&String> = config.profiles.keys().collect();
                    serde_json::json!({
                        "profiles": profile_names,
                        "default_cloud": config.default_cloud,
                        "default_enterprise": config.default_enterprise
                    })
                    .to_string()
                }
                Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
            };

            Ok(ReadResourceResult {
                contents: vec![ResourceContent {
                    uri: "redis://profiles".to_string(),
                    mime_type: Some("application/json".to_string()),
                    text: Some(profiles),
                    blob: None,
                    meta: None,
                }],
                meta: None,
                ..Default::default()
            })
        })
        .build()
}

/// Build a resource exposing server instructions/help
pub fn help_resource() -> Resource {
    ResourceBuilder::new("redis://help")
        .name("Help")
        .description("Usage instructions for the Redis MCP server")
        .mime_type("text/markdown")
        .text(
            r#"# Redis MCP Server Help

## Tool Categories

### Redis Cloud
- **Subscriptions**: list_subscriptions, get_subscription
- **Databases**: list_databases, get_database, get_backup_status, get_slow_log
- **Account**: get_account, list_account_users
- **Tasks**: list_tasks, get_task

### Redis Enterprise
- **Cluster**: get_cluster, get_cluster_stats
- **License**: get_license, get_license_usage
- **Databases**: list_enterprise_databases, get_enterprise_database
- **Nodes**: list_nodes, get_node, get_node_stats
- **Modules**: list_modules, get_module

### Direct Redis
- **Connection**: redis_ping, redis_info, redis_dbsize
- **Keys**: redis_keys, redis_get, redis_type, redis_ttl
- **Data Structures**: redis_hgetall, redis_lrange, redis_smembers, redis_zrange

## Prompts

Use prompts for common workflows:
- `troubleshoot_database` - Diagnose database issues
- `analyze_performance` - Analyze performance metrics
- `capacity_planning` - Help with capacity planning decisions
- `migration_planning` - Plan Redis migrations between environments

## Resources

- `redis://config/path` - Configuration file location
- `redis://profiles` - List of configured profiles
- `redis://help` - This help text
"#,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_help_resource() {
        let resource = help_resource();
        assert_eq!(resource.uri, "redis://help");
        assert_eq!(resource.name, "Help");

        let result = resource.read().await;
        assert_eq!(result.contents.len(), 1);
        assert!(
            result.contents[0]
                .text
                .as_ref()
                .unwrap()
                .contains("Redis MCP Server")
        );
    }

    #[tokio::test]
    async fn test_config_path_resource() {
        let resource = config_path_resource();
        assert_eq!(resource.uri, "redis://config/path");

        let result = resource.read().await;
        assert_eq!(result.contents.len(), 1);
        // Should return either a path or error message
        assert!(result.contents[0].text.is_some());
    }

    #[tokio::test]
    async fn test_profiles_resource() {
        let resource = profiles_resource();
        assert_eq!(resource.uri, "redis://profiles");

        let result = resource.read().await;
        assert_eq!(result.contents.len(), 1);
        // Should return JSON (either profiles or error)
        let text = result.contents[0].text.as_ref().unwrap();
        assert!(text.starts_with('{'));
    }
}
