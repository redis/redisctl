//! MCP resources for redisctl configuration and usage.
//!
//! Resources expose read-only data that can be fetched by URI.

use redisctl_core::Config;
use tower_mcp::protocol::{ReadResourceResult, ResourceContent};
use tower_mcp::resource::{Resource, ResourceBuilder};

/// Build a resource exposing the current configuration path
pub fn config_path_resource() -> Resource {
    config_path_resource_at("redisctl://config/path")
}

pub(crate) fn config_path_resource_at(uri: &'static str) -> Resource {
    ResourceBuilder::new(uri)
        .name("Configuration Path")
        .description("Path to the redisctl configuration file")
        .mime_type("text/plain")
        .handler(move || async move {
            let path = Config::config_path()
                .map(|p: std::path::PathBuf| p.display().to_string())
                .unwrap_or_else(|_| "(no config path available)".to_string());

            Ok(ReadResourceResult {
                contents: vec![ResourceContent {
                    uri: uri.to_string(),
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
    profiles_resource_at("redisctl://profiles")
}

fn profiles_summary(config: redisctl_core::config::Result<Config>) -> String {
    match config {
        Ok(config) => {
            let mut profile_names = config.profiles.keys().collect::<Vec<_>>();
            profile_names.sort();
            serde_json::json!({
                "profiles": profile_names,
                "default_cloud": config.default_cloud,
                "default_enterprise": config.default_enterprise,
                "default_database": config.default_database
            })
            .to_string()
        }
        // TOML errors may include the credential-bearing source line. Do not
        // send raw config errors to an MCP client, including through aliases.
        Err(_) => serde_json::json!({
            "error": "Cannot load redisctl configuration. Validate it locally; do not share config contents or credentials."
        })
        .to_string(),
    }
}

pub(crate) fn profiles_resource_at(uri: &'static str) -> Resource {
    ResourceBuilder::new(uri)
        .name("Profiles")
        .description("List of configured redisctl profiles")
        .mime_type("application/json")
        .handler(move || async move {
            Ok(ReadResourceResult {
                contents: vec![ResourceContent {
                    uri: uri.to_string(),
                    mime_type: Some("application/json".to_string()),
                    text: Some(profiles_summary(Config::load())),
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
    help_resource_at("redisctl://help")
}

pub(crate) fn help_resource_at(uri: &'static str) -> Resource {
    ResourceBuilder::new(uri)
        .name("Help")
        .description("Usage instructions for the redisctl MCP server")
        .mime_type("text/markdown")
        .text(
            r#"# Redis MCP Server Help

Read `redisctl://skills` for workflow discovery and tools available in this session.
Read `redisctl://skills/redisctl-setup` for safe first-run setup. Skills are also
available as prompts; load only the workflow needed for the task.

Tool selection and policy still apply. Call `show_policy` and
`list_available_tools` when available; never invoke unavailable tools or enable
writes merely to follow a workflow. Enter credentials through a trusted local
user flow, not chat or model-visible tool arguments.

## Prompts

Use prompts for common workflows:

- `troubleshoot_database` - Diagnose database issues
- `analyze_performance` - Analyze performance metrics
- `capacity_planning` - Help with capacity planning decisions
- `migration_planning` - Plan Redis migrations between environments

## Resources

- `redisctl://config/path` - Configuration file location, not file contents
- `redisctl://profiles` - Profile names and defaults, not credentials
- `redisctl://help` - This help text
- `redisctl://skills` - Workflow index and current tool availability
- `redisctl://skills/<name>` - One complete workflow

The old `redis://config/path`, `redis://profiles`, and `redis://help` URIs remain
compatible aliases. These describe redisctl, not a particular Redis instance.
"#,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_help_resource() {
        let resource = help_resource();
        assert_eq!(resource.uri, "redisctl://help");
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
        assert_eq!(resource.uri, "redisctl://config/path");

        let result = resource.read().await;
        assert_eq!(result.contents.len(), 1);
        // Should return either a path or error message
        assert!(result.contents[0].text.is_some());
    }

    #[tokio::test]
    async fn test_profiles_resource() {
        let resource = profiles_resource();
        assert_eq!(resource.uri, "redisctl://profiles");

        let result = resource.read().await;
        assert_eq!(result.contents.len(), 1);
        // Should return JSON (either profiles or error)
        let text = result.contents[0].text.as_ref().unwrap();
        assert!(text.starts_with('{'));
    }

    #[test]
    fn profile_resource_summary_reports_all_defaults_without_credentials() {
        // Parse synthetic data directly: never load the operator's config.
        let config: Config = toml::from_str(
            r#"
default_database = "db-selected"
default_cloud = "cloud"
default_enterprise = "enterprise"
files_api_key = "synthetic-global-secret"
[profiles.db-selected]
deployment_type = "database"
host = "127.0.0.1"
port = 6379
password = "synthetic-database-secret"
[profiles.db-other]
deployment_type = "database"
host = "127.0.0.1"
port = 6380
[profiles.cloud]
deployment_type = "cloud"
api_key = "synthetic-cloud-key"
api_secret = "synthetic-cloud-secret"
[profiles.enterprise]
deployment_type = "enterprise"
url = "https://example.invalid:9443"
username = "synthetic-user"
password = "synthetic-enterprise-secret"
"#,
        )
        .unwrap();
        let summary: serde_json::Value =
            serde_json::from_str(&profiles_summary(Ok(config))).unwrap();
        // Exact shape also protects against leaking credential/connection fields.
        assert_eq!(
            summary,
            serde_json::json!({
                "profiles": ["cloud", "db-other", "db-selected", "enterprise"],
                "default_cloud": "cloud",
                "default_enterprise": "enterprise",
                "default_database": "db-selected"
            })
        );
    }

    #[test]
    fn profile_resource_summary_reports_unset_defaults_as_null() {
        let summary: serde_json::Value =
            serde_json::from_str(&profiles_summary(Ok(Config::default()))).unwrap();
        assert_eq!(
            summary,
            serde_json::json!({
                "profiles": [],
                "default_cloud": null,
                "default_enterprise": null,
                "default_database": null
            })
        );
    }

    #[test]
    fn profile_resource_errors_never_include_toml_source() {
        let error =
            toml::from_str::<Config>("files_api_key = \"resource-secret-sentinel").unwrap_err();
        assert!(error.to_string().contains("resource-secret-sentinel"));
        let summary = profiles_summary(Err(error.into()));
        assert!(!summary.contains("resource-secret-sentinel"));
        assert!(!summary.contains("files_api_key"));
        assert!(summary.contains("Validate it locally"));
    }
}
