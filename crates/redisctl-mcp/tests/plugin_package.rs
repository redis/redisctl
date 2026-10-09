//! Pure package/policy regressions: no AppState, user profiles or network.

use std::collections::HashMap;

use redisctl_mcp::{Policy, PolicyConfig, SafetyTier, ToolsetKind};
use tower_mcp::{CallToolResult, ToolBuilder};

const POLICY: &str = include_str!("fixtures/plugin-read-only.toml");

#[test]
fn plugin_policy_allows_reads_and_denies_writes_and_raw_access() {
    let config: PolicyConfig = toml::from_str(POLICY).expect("valid plugin policy");
    assert_eq!(config.tier, SafetyTier::ReadOnly);
    assert!(config.allow.is_empty());
    let mapping = HashMap::from([
        ("redis_ping".to_string(), ToolsetKind::Database),
        ("redis_set".to_string(), ToolsetKind::Database),
        ("redis_command".to_string(), ToolsetKind::Database),
        ("profile_show".to_string(), ToolsetKind::App),
        ("profile_create".to_string(), ToolsetKind::App),
    ]);
    let policy = Policy::new(config, mapping, "plugin-regression".to_string());
    let read = ToolBuilder::new("redis_ping")
        .read_only_safe()
        .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("synthetic")) })
        .build();
    assert!(policy.is_tool_allowed(&read));
    for name in ["profile_show", "redis_command"] {
        let tool = ToolBuilder::new(name)
            .read_only_safe()
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("synthetic")) })
            .build();
        assert!(!policy.is_tool_allowed(&tool), "explicitly denied {name}");
    }
    for name in ["redis_set", "profile_create"] {
        let tool = ToolBuilder::new(name)
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("synthetic")) })
            .build();
        assert!(!policy.is_tool_allowed(&tool), "write denied {name}");
    }
}
