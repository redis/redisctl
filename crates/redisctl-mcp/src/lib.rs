//! MCP (Model Context Protocol) server for Redis Cloud and Enterprise
//!
//! This crate provides an MCP server that exposes Redis Cloud and Enterprise
//! management operations as tools for AI systems.
//!
//! ## Binary Usage
//!
//! The primary way to use this crate is as a standalone binary:
//!
//! ```bash
//! # Stdio transport (for Claude Desktop, etc.)
//! redisctl-mcp --profile my-profile
//!
//! # Multiple profiles for multi-cluster support
//! redisctl-mcp --profile cluster-west --profile cluster-east --profile cluster-central
//!
//! # Enable only specific toolsets
//! redisctl-mcp --tools cloud,app
//! ```
//!
//! ## Library Usage
//!
//! You can also embed the tools in your own MCP server using sub-routers:
//!
//! ```no_run
//! use std::sync::Arc;
//! use redisctl_mcp::{AppState, CredentialSource, tools};
//! use redisctl_mcp::policy::{Policy, PolicyConfig};
//! use tower_mcp::McpRouter;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let policy = Arc::new(Policy::new(
//!     PolicyConfig::default(), // read-only
//!     std::collections::HashMap::new(),
//!     "default".to_string(),
//! ));
//! let state = Arc::new(AppState::new(
//!     CredentialSource::Profiles(vec!["default".to_string()]),
//!     policy,
//!     None, // no database URL
//!     false, // cluster mode
//!     Some("redisctl-mcp".to_string()), // client name
//! )?);
//!
//! // Use merge to compose sub-routers
//! let router = McpRouter::new()
//!     .merge(tools::cloud::router(state.clone()))
//!     .merge(tools::enterprise::router(state.clone()))
//!     .merge(tools::profile::router(state.clone()));
//! # Ok(())
//! # }
//! ```

pub mod audit;
pub mod policy;
pub mod presets;
pub mod prompts;
pub mod resources;
pub mod serde_helpers;
pub mod state;
pub mod tools;

pub use state::{AppState, CredentialSource};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_credential_source_profiles() {
        let CredentialSource::Profiles(profiles) =
            CredentialSource::Profiles(vec!["test".to_string()]);
        assert_eq!(profiles, vec!["test".to_string()]);
    }

    #[test]
    fn test_credential_source_multiple_profiles() {
        let CredentialSource::Profiles(profiles) = CredentialSource::Profiles(vec![
            "cluster-west".to_string(),
            "cluster-east".to_string(),
        ]);
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0], "cluster-west");
        assert_eq!(profiles[1], "cluster-east");
    }

    #[test]
    fn test_app_state_read_only() {
        use policy::{Policy, PolicyConfig};
        let read_only_policy = Arc::new(Policy::new(
            PolicyConfig::default(), // read-only
            std::collections::HashMap::new(),
            "test".to_string(),
        ));
        let state = AppState::new(
            CredentialSource::Profiles(vec![]),
            read_only_policy,
            None,
            false,
            None,
        )
        .unwrap();

        assert!(!state.is_write_allowed());
    }

    #[test]
    fn test_app_state_write_allowed() {
        use policy::{Policy, PolicyConfig, SafetyTier};
        let write_policy = Arc::new(Policy::new(
            PolicyConfig {
                tier: SafetyTier::Full,
                ..Default::default()
            },
            std::collections::HashMap::new(),
            "test".to_string(),
        ));
        let state = AppState::new(
            CredentialSource::Profiles(vec![]),
            write_policy,
            None,
            false,
            None,
        )
        .unwrap();

        assert!(state.is_write_allowed());
    }

    #[test]
    fn test_app_state_database_url() {
        let state = AppState::new(
            CredentialSource::Profiles(vec![]),
            AppState::test_policy(),
            Some("redis://localhost:6379".to_string()),
            false,
            None,
        )
        .unwrap();

        assert_eq!(
            state.database_url,
            Some("redis://localhost:6379".to_string())
        );
    }

    #[test]
    fn test_app_state_available_profiles() {
        let state = AppState::new(
            CredentialSource::Profiles(vec![
                "cluster-west".to_string(),
                "cluster-east".to_string(),
            ]),
            AppState::test_policy(),
            None,
            false,
            None,
        )
        .unwrap();

        let profiles = state.available_profiles();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0], "cluster-west");
        assert_eq!(profiles[1], "cluster-east");
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn test_cloud_tools_build() {
        let state = Arc::new(
            AppState::new(
                CredentialSource::Profiles(vec![]),
                AppState::test_policy(),
                None,
                false,
                None,
            )
            .unwrap(),
        );

        // Verify all cloud tools build successfully
        // Subscriptions & Databases
        let _ = tools::cloud::list_subscriptions(state.clone());
        let _ = tools::cloud::get_subscription(state.clone());
        let _ = tools::cloud::list_databases(state.clone());
        let _ = tools::cloud::get_database(state.clone());
        let _ = tools::cloud::get_backup_status(state.clone());
        let _ = tools::cloud::get_slow_log(state.clone());
        let _ = tools::cloud::get_tags(state.clone());
        // Account & Configuration
        let _ = tools::cloud::get_account(state.clone());
        let _ = tools::cloud::get_regions(state.clone());
        let _ = tools::cloud::get_modules(state.clone());
        let _ = tools::cloud::list_account_users(state.clone());
        let _ = tools::cloud::get_account_user(state.clone());
        let _ = tools::cloud::list_acl_users(state.clone());
        let _ = tools::cloud::get_acl_user(state.clone());
        let _ = tools::cloud::list_acl_roles(state.clone());
        let _ = tools::cloud::list_redis_rules(state.clone());
        // Logs
        let _ = tools::cloud::get_system_logs(state.clone());
        let _ = tools::cloud::get_session_logs(state.clone());
        // Tasks
        let _ = tools::cloud::list_tasks(state.clone());
        let _ = tools::cloud::get_task(state.clone());
        // Write operations
        let _ = tools::cloud::create_database(state.clone());
        let _ = tools::cloud::update_database(state.clone());
        let _ = tools::cloud::delete_database(state.clone());
        let _ = tools::cloud::backup_database(state.clone());
        let _ = tools::cloud::import_database(state.clone());
        let _ = tools::cloud::delete_subscription(state.clone());
        let _ = tools::cloud::flush_database(state.clone());
        let _ = tools::cloud::create_subscription(state.clone());
        // Raw
        let _ = tools::cloud::cloud_raw_api(state.clone());
    }

    #[cfg(feature = "enterprise")]
    #[test]
    fn test_enterprise_tools_build() {
        let state = Arc::new(
            AppState::new(
                CredentialSource::Profiles(vec![]),
                AppState::test_policy(),
                None,
                false,
                None,
            )
            .unwrap(),
        );

        // Verify all enterprise tools build successfully
        // Cluster
        let _ = tools::enterprise::get_cluster(state.clone());
        // License
        let _ = tools::enterprise::get_license(state.clone());
        let _ = tools::enterprise::get_license_usage(state.clone());
        // Logs
        let _ = tools::enterprise::list_logs(state.clone());
        // Databases
        let _ = tools::enterprise::list_databases(state.clone());
        let _ = tools::enterprise::get_database(state.clone());
        // Nodes
        let _ = tools::enterprise::list_nodes(state.clone());
        let _ = tools::enterprise::get_node(state.clone());
        // Users
        let _ = tools::enterprise::list_users(state.clone());
        let _ = tools::enterprise::get_user(state.clone());
        // Alerts
        let _ = tools::enterprise::list_alerts(state.clone());
        let _ = tools::enterprise::list_database_alerts(state.clone());
        // Stats
        let _ = tools::enterprise::get_cluster_stats(state.clone());
        let _ = tools::enterprise::get_database_stats(state.clone());
        let _ = tools::enterprise::get_node_stats(state.clone());
        let _ = tools::enterprise::get_all_nodes_stats(state.clone());
        let _ = tools::enterprise::get_all_databases_stats(state.clone());
        // Shards
        let _ = tools::enterprise::list_shards(state.clone());
        let _ = tools::enterprise::get_shard(state.clone());
        let _ = tools::enterprise::get_shard_stats(state.clone());
        let _ = tools::enterprise::get_all_shards_stats(state.clone());
        // Endpoints
        let _ = tools::enterprise::get_database_endpoints(state.clone());
        // Debug info
        let _ = tools::enterprise::list_debug_info_tasks(state.clone());
        let _ = tools::enterprise::get_debug_info_status(state.clone());
        // Modules
        let _ = tools::enterprise::list_modules(state.clone());
        let _ = tools::enterprise::get_module(state.clone());
        // Write operations
        let _ = tools::enterprise::backup_enterprise_database(state.clone());
        let _ = tools::enterprise::import_enterprise_database(state.clone());
        let _ = tools::enterprise::create_enterprise_database(state.clone());
        let _ = tools::enterprise::update_enterprise_database(state.clone());
        let _ = tools::enterprise::delete_enterprise_database(state.clone());
        let _ = tools::enterprise::flush_enterprise_database(state.clone());
        // Raw
        let _ = tools::enterprise::enterprise_raw_api(state.clone());
    }

    #[cfg(feature = "database")]
    #[test]
    fn test_database_tools_build() {
        let state = Arc::new(
            AppState::new(
                CredentialSource::Profiles(vec![]),
                AppState::test_policy(),
                Some("redis://localhost:6379".to_string()),
                false,
                None,
            )
            .unwrap(),
        );

        // Verify database tools build successfully
        let _ = tools::redis::ping(state.clone());
        let _ = tools::redis::info(state.clone());
        let _ = tools::redis::keys(state.clone());
        let _ = tools::redis::get(state.clone());
        let _ = tools::redis::set(state.clone());
        let _ = tools::redis::del(state.clone());
        let _ = tools::redis::flushdb(state.clone());
        // Raw
        let _ = tools::redis::redis_command(state.clone());
    }

    #[test]
    fn test_profile_tools_build() {
        let state = Arc::new(
            AppState::new(
                CredentialSource::Profiles(vec![]),
                AppState::test_policy(),
                None,
                false,
                None,
            )
            .unwrap(),
        );

        // Verify profile tools build successfully
        let _ = tools::profile::list_profiles(state.clone());
        let _ = tools::profile::show_profile(state.clone());
        let _ = tools::profile::config_path(state.clone());
        let _ = tools::profile::validate_config(state.clone());
        let _ = tools::profile::set_default_cloud(state.clone());
        let _ = tools::profile::set_default_enterprise(state.clone());
        let _ = tools::profile::delete_profile(state.clone());
    }
}
