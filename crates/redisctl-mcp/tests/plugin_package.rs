//! Pure package/policy regressions: no AppState, user profiles or network.

use std::collections::HashMap;

use redisctl_mcp::{Policy, PolicyConfig, SafetyTier, ToolsetKind};
use tower_mcp::{CallToolResult, ToolAnnotations, ToolBuilder};

const POLICY: &str = include_str!("fixtures/plugin-read-only.toml");

#[test]
fn plugin_policy_allows_reads_and_denies_writes_and_raw_access() {
    let config: PolicyConfig = toml::from_str(POLICY).expect("valid plugin policy");
    assert_eq!(config.tier, SafetyTier::ReadOnly);
    assert!(config.allow.is_empty());
    let mapping = HashMap::from([
        ("redis_ping".to_string(), ToolsetKind::Database),
        ("redis_set".to_string(), ToolsetKind::Database),
        ("synthetic_delete".to_string(), ToolsetKind::Database),
        ("redis_command".to_string(), ToolsetKind::Database),
        ("profile_show".to_string(), ToolsetKind::App),
        ("profile_create".to_string(), ToolsetKind::App),
    ]);
    let mut full_config = config.clone();
    full_config.tier = SafetyTier::Full;
    let mut unrestricted_config = full_config.clone();
    unrestricted_config.deny_categories.clear();
    let unrestricted_policy = Policy::new(
        unrestricted_config,
        mapping.clone(),
        "plugin-full-tier-without-category-denial".to_string(),
    );
    let full_policy = Policy::new(
        full_config,
        mapping.clone(),
        "plugin-full-tier-regression".to_string(),
    );
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
        let write_annotations = ToolAnnotations {
            read_only_hint: false,
            destructive_hint: false,
            ..ToolAnnotations::default()
        };
        let tool = ToolBuilder::new(name)
            .annotations(write_annotations)
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("synthetic")) })
            .build();
        let annotations = tool.annotations.as_ref().expect("explicit write hints");
        assert!(!annotations.read_only_hint);
        assert!(!annotations.destructive_hint);
        // Full tier admits these non-destructive writes even though the same
        // category deny remains active; read-only must reject them by tier.
        assert!(
            full_policy.is_tool_allowed(&tool),
            "full tier allows {name}"
        );
        assert!(!policy.is_tool_allowed(&tool), "write denied {name}");
    }
    let destructive = ToolBuilder::new("synthetic_delete")
        .destructive()
        .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("synthetic")) })
        .build();
    assert!(
        unrestricted_policy.is_tool_allowed(&destructive),
        "full tier without category denial allows the mapped destructive tool"
    );
    assert!(
        !full_policy.is_tool_allowed(&destructive),
        "category denial remains active"
    );
    assert!(!policy.is_tool_allowed(&destructive));
}
