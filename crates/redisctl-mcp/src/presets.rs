//! Tool visibility presets for managing the number of tools exposed to MCP clients.
//!
//! MCP clients degrade past ~50-100 tools (token overhead, selection confusion,
//! latency). This module provides curated "essentials" subsets per platform so
//! users get a manageable tool surface, with raw API passthrough tools (#768)
//! as an escape hatch for anything not in the preset.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tower_mcp::{CallToolResult, Error as McpError, Tool, ToolBuilder};

use crate::policy::ToolsetKind;

// ============================================================================
// Configuration
// ============================================================================

/// Tool visibility configuration from the `[tools]` section of `mcp-policy.toml`.
///
/// ```toml
/// [tools]
/// preset = "essentials"
/// include = ["enterprise_raw_api", "get_enterprise_crdb"]
/// exclude = ["flush_database"]
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ToolsConfig {
    /// Preset name. `"all"` (default) loads every tool; `"essentials"` loads
    /// a curated subset per enabled toolset.
    pub preset: String,
    /// Extra tool names to include on top of the preset.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    /// Tool names to exclude from the resolved set.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

impl ToolsConfig {
    /// Returns true when visibility is the default "all tools visible" mode.
    pub fn is_all(&self) -> bool {
        self.preset.is_empty() || self.preset == "all"
    }
}

// ============================================================================
// Essentials constants
// ============================================================================

/// Cloud essentials: subscriptions, databases, account, tasks, logs, backups.
pub const CLOUD_ESSENTIALS: &[&str] = &[
    "list_subscriptions",
    "get_subscription",
    "list_databases",
    "get_database",
    "get_account",
    "get_regions",
    "list_tasks",
    "get_task",
    "wait_for_cloud_task",
    "get_system_logs",
    "get_backup_status",
    "get_slow_log",
    "get_database_tags",
    "create_database",
    "update_database",
    "backup_database",
    "list_fixed_subscriptions",
    "list_fixed_databases",
    "get_fixed_database",
    "get_fixed_database_backup_status",
];

/// Enterprise essentials: cluster, nodes, databases, RBAC, observability.
pub const ENTERPRISE_ESSENTIALS: &[&str] = &[
    "get_cluster",
    "get_license",
    "list_nodes",
    "get_node",
    "get_cluster_stats",
    "list_enterprise_databases",
    "get_enterprise_database",
    "get_database_stats",
    "list_enterprise_users",
    "get_enterprise_user",
    "list_enterprise_roles",
    "get_enterprise_role",
    "list_alerts",
    "list_logs",
    "list_shards",
    "get_all_nodes_stats",
    "get_all_databases_stats",
];

/// Database essentials: server info, diagnostics, key inspection.
pub const DATABASE_ESSENTIALS: &[&str] = &[
    "redis_ping",
    "redis_info",
    "redis_dbsize",
    "redis_slowlog",
    "redis_config_get",
    "redis_memory_stats",
    "redis_health_check",
    "redis_key_summary",
    "redis_connection_summary",
    "redis_keys",
    "redis_scan",
    "redis_get",
    "redis_type",
    "redis_ttl",
    "redis_hgetall",
    "redis_mget",
    "redis_hget",
    "redis_scard",
    "redis_zcard",
    "redis_llen",
    "redis_incr",
    "redis_zscore",
    "redis_sismember",
];

/// App essentials: all profile tools (always included).
pub const APP_ESSENTIALS: &[&str] = &[
    "profile_list",
    "profile_show",
    "profile_path",
    "profile_validate",
    "profile_set_default_cloud",
    "profile_set_default_enterprise",
    "profile_delete",
    "profile_create",
];

// ============================================================================
// Resolution
// ============================================================================

/// Resolve the set of visible tool names from config, available tools, and
/// toolset membership.
///
/// Resolution: preset base set -> +include -> -exclude
pub fn resolve_visible_tools(
    config: &ToolsConfig,
    all_tools: &HashSet<String>,
    tool_toolset: &HashMap<String, ToolsetKind>,
) -> HashSet<String> {
    if config.is_all() && config.include.is_empty() && config.exclude.is_empty() {
        return all_tools.clone();
    }

    let mut visible = if config.is_all() {
        all_tools.clone()
    } else if config.preset == "essentials" {
        build_essentials_set(tool_toolset)
    } else {
        // Unknown preset: treat as empty (only include list will populate)
        tracing::warn!(preset = %config.preset, "Unknown preset, starting with empty tool set");
        HashSet::new()
    };

    // +include: add tools that exist in the full set
    for name in &config.include {
        if all_tools.contains(name) {
            visible.insert(name.clone());
        } else {
            tracing::warn!(tool = %name, "Include references unknown tool, ignoring");
        }
    }

    // -exclude: remove from visible set
    for name in &config.exclude {
        visible.remove(name);
    }

    visible
}

/// Build the essentials set from only the toolsets that are actually enabled.
fn build_essentials_set(tool_toolset: &HashMap<String, ToolsetKind>) -> HashSet<String> {
    let mut set = HashSet::new();

    // Determine which toolsets are present
    let active_kinds: HashSet<ToolsetKind> = tool_toolset.values().copied().collect();

    if active_kinds.contains(&ToolsetKind::Cloud) {
        for name in CLOUD_ESSENTIALS {
            set.insert((*name).to_string());
        }
    }
    if active_kinds.contains(&ToolsetKind::Enterprise) {
        for name in ENTERPRISE_ESSENTIALS {
            set.insert((*name).to_string());
        }
    }
    if active_kinds.contains(&ToolsetKind::Database) {
        for name in DATABASE_ESSENTIALS {
            set.insert((*name).to_string());
        }
    }
    if active_kinds.contains(&ToolsetKind::App) {
        for name in APP_ESSENTIALS {
            set.insert((*name).to_string());
        }
    }

    set
}

// ============================================================================
// Runtime visibility
// ============================================================================

/// Runtime tool visibility state, shared via `Arc`.
pub struct ToolVisibility {
    /// Tools that passed visibility resolution.
    pub visible: HashSet<String>,
    /// Full tool-to-toolset mapping (for grouping in list_available_tools).
    pub all_tools: HashMap<String, ToolsetKind>,
    /// The config that produced this visibility.
    pub config: ToolsConfig,
}

// ============================================================================
// list_available_tools tool
// ============================================================================

/// Serializable summary returned by `list_available_tools`.
#[derive(Debug, Serialize)]
struct AvailableToolsSummary {
    preset: String,
    active_count: usize,
    total_count: usize,
    toolsets: BTreeMap<String, ToolsetGroup>,
}

/// Per-toolset group showing active and hidden tools.
#[derive(Debug, Serialize)]
struct ToolsetGroup {
    active: Vec<String>,
    hidden: Vec<String>,
}

/// Build the `list_available_tools` MCP tool.
///
/// Always registered, read-only. Returns the current preset, counts,
/// and per-toolset active/hidden tool lists so the LLM can discover
/// what tools are available and request specific ones via `include`.
pub fn list_available_tools_tool(visibility: Arc<ToolVisibility>) -> Tool {
    ToolBuilder::new("list_available_tools")
        .description(
            "List all available tools grouped by toolset, showing which are active \
             (visible) and which are hidden by the current preset. Use this to discover \
             tools you can request be enabled via the include list in the policy config.",
        )
        .read_only_safe()
        .handler(move |_: tower_mcp::NoParams| {
            let vis = visibility.clone();
            async move {
                let mut toolsets: BTreeMap<String, ToolsetGroup> = BTreeMap::new();

                // Group tools by toolset
                for (name, kind) in &vis.all_tools {
                    let key = kind.to_string();
                    let group = toolsets.entry(key).or_insert_with(|| ToolsetGroup {
                        active: Vec::new(),
                        hidden: Vec::new(),
                    });
                    if vis.visible.contains(name) {
                        group.active.push(name.clone());
                    } else {
                        group.hidden.push(name.clone());
                    }
                }

                // Sort lists for deterministic output
                for group in toolsets.values_mut() {
                    group.active.sort();
                    group.hidden.sort();
                }

                let active_count = vis.visible.len();
                let total_count = vis.all_tools.len();

                let summary = AvailableToolsSummary {
                    preset: if vis.config.is_all() {
                        "all".to_string()
                    } else {
                        vis.config.preset.clone()
                    },
                    active_count,
                    total_count,
                    toolsets,
                };

                CallToolResult::from_serialize(&summary)
                    .map_err(|e| McpError::tool(format!("Failed to serialize summary: {e}")))
            }
        })
        .build()
}

/// Names of system tools that bypass visibility filtering.
pub const SYSTEM_TOOLS: &[&str] = &["show_policy", "list_available_tools"];

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_toolset(entries: &[(&str, ToolsetKind)]) -> HashMap<String, ToolsetKind> {
        entries.iter().map(|(n, k)| (n.to_string(), *k)).collect()
    }

    fn make_all_tools(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    // -- ToolsConfig tests --

    #[test]
    fn default_config_is_all() {
        let config = ToolsConfig::default();
        assert!(config.is_all());
    }

    #[test]
    fn empty_string_is_all() {
        let config = ToolsConfig {
            preset: String::new(),
            ..Default::default()
        };
        assert!(config.is_all());
    }

    #[test]
    fn explicit_all() {
        let config = ToolsConfig {
            preset: "all".to_string(),
            ..Default::default()
        };
        assert!(config.is_all());
    }

    #[test]
    fn essentials_is_not_all() {
        let config = ToolsConfig {
            preset: "essentials".to_string(),
            ..Default::default()
        };
        assert!(!config.is_all());
    }

    // -- resolve_visible_tools tests --

    #[test]
    fn all_preset_returns_all_tools() {
        let all = make_all_tools(&["tool_a", "tool_b", "tool_c"]);
        let mapping = make_tool_toolset(&[("tool_a", ToolsetKind::Cloud)]);
        let config = ToolsConfig::default();

        let visible = resolve_visible_tools(&config, &all, &mapping);
        assert_eq!(visible, all);
    }

    #[test]
    fn essentials_preset_filters_to_known_tools() {
        let mut mapping = HashMap::new();
        // Add some cloud tools
        for name in CLOUD_ESSENTIALS {
            mapping.insert(name.to_string(), ToolsetKind::Cloud);
        }
        mapping.insert("delete_database".to_string(), ToolsetKind::Cloud);
        mapping.insert("flush_database".to_string(), ToolsetKind::Cloud);

        let all: HashSet<String> = mapping.keys().cloned().collect();
        let config = ToolsConfig {
            preset: "essentials".to_string(),
            ..Default::default()
        };

        let visible = resolve_visible_tools(&config, &all, &mapping);

        // Essentials should be present
        assert!(visible.contains("list_subscriptions"));
        assert!(visible.contains("get_database"));
        // Non-essentials should not
        assert!(!visible.contains("delete_database"));
        assert!(!visible.contains("flush_database"));
    }

    #[test]
    fn include_adds_tools() {
        let mapping = make_tool_toolset(&[
            ("tool_a", ToolsetKind::Cloud),
            ("tool_b", ToolsetKind::Cloud),
        ]);
        let all = make_all_tools(&["tool_a", "tool_b"]);
        let config = ToolsConfig {
            preset: "essentials".to_string(),
            include: vec!["tool_b".to_string()],
            ..Default::default()
        };

        let visible = resolve_visible_tools(&config, &all, &mapping);
        assert!(visible.contains("tool_b"));
    }

    #[test]
    fn include_ignores_unknown_tools() {
        let mapping = make_tool_toolset(&[("tool_a", ToolsetKind::Cloud)]);
        let all = make_all_tools(&["tool_a"]);
        let config = ToolsConfig {
            preset: "essentials".to_string(),
            include: vec!["nonexistent_tool".to_string()],
            ..Default::default()
        };

        let visible = resolve_visible_tools(&config, &all, &mapping);
        assert!(!visible.contains("nonexistent_tool"));
    }

    #[test]
    fn exclude_removes_tools() {
        let all = make_all_tools(&["tool_a", "tool_b", "tool_c"]);
        let mapping = make_tool_toolset(&[]);
        let config = ToolsConfig {
            preset: "all".to_string(),
            exclude: vec!["tool_b".to_string()],
            ..Default::default()
        };

        let visible = resolve_visible_tools(&config, &all, &mapping);
        assert!(visible.contains("tool_a"));
        assert!(!visible.contains("tool_b"));
        assert!(visible.contains("tool_c"));
    }

    #[test]
    fn essentials_only_includes_active_toolsets() {
        // Only enterprise tools in the mapping = only enterprise essentials
        let mut mapping = HashMap::new();
        for name in ENTERPRISE_ESSENTIALS {
            mapping.insert(name.to_string(), ToolsetKind::Enterprise);
        }
        let all: HashSet<String> = mapping.keys().cloned().collect();

        let config = ToolsConfig {
            preset: "essentials".to_string(),
            ..Default::default()
        };

        let visible = resolve_visible_tools(&config, &all, &mapping);

        // Enterprise essentials should be present
        assert!(visible.contains("get_cluster"));
        // Cloud essentials should NOT (cloud toolset is not active)
        assert!(!visible.contains("list_subscriptions"));
    }

    #[test]
    fn include_and_exclude_compose() {
        let all = make_all_tools(&["a", "b", "c", "d"]);
        let mapping = make_tool_toolset(&[]);
        let config = ToolsConfig {
            preset: "essentials".to_string(),
            include: vec!["a".to_string(), "b".to_string()],
            exclude: vec!["b".to_string()],
        };

        let visible = resolve_visible_tools(&config, &all, &mapping);
        assert!(visible.contains("a"));
        assert!(!visible.contains("b")); // exclude wins
    }

    // -- Cross-validation: essentials constants vs SUB_MODULES tool names --

    #[cfg(feature = "cloud")]
    #[test]
    fn cloud_essentials_are_valid_tool_names() {
        let valid: HashSet<&str> = crate::tools::cloud::SUB_MODULES
            .iter()
            .flat_map(|sm| sm.tool_names.iter().copied())
            .collect();
        for name in CLOUD_ESSENTIALS {
            assert!(
                valid.contains(name),
                "Cloud essentials contains unknown tool: {name}"
            );
        }
    }

    #[cfg(feature = "enterprise")]
    #[test]
    fn enterprise_essentials_are_valid_tool_names() {
        let valid: HashSet<&str> = crate::tools::enterprise::SUB_MODULES
            .iter()
            .flat_map(|sm| sm.tool_names.iter().copied())
            .collect();
        for name in ENTERPRISE_ESSENTIALS {
            assert!(
                valid.contains(name),
                "Enterprise essentials contains unknown tool: {name}"
            );
        }
    }

    #[cfg(feature = "database")]
    #[test]
    fn database_essentials_are_valid_tool_names() {
        let valid: HashSet<&str> = crate::tools::redis::SUB_MODULES
            .iter()
            .flat_map(|sm| sm.tool_names.iter().copied())
            .collect();
        for name in DATABASE_ESSENTIALS {
            assert!(
                valid.contains(name),
                "Database essentials contains unknown tool: {name}"
            );
        }
    }

    #[test]
    fn app_essentials_are_valid_tool_names() {
        let valid: HashSet<&str> = crate::tools::profile::TOOL_NAMES.iter().copied().collect();
        for name in APP_ESSENTIALS {
            assert!(
                valid.contains(name),
                "App essentials contains unknown tool: {name}"
            );
        }
    }

    // -- list_available_tools tool tests --

    #[test]
    fn list_available_tools_is_read_only() {
        let vis = Arc::new(ToolVisibility {
            visible: HashSet::new(),
            all_tools: HashMap::new(),
            config: ToolsConfig::default(),
        });
        let tool = list_available_tools_tool(vis);
        let ann = tool.annotations.as_ref().unwrap();
        assert!(ann.read_only_hint);
        assert!(!ann.destructive_hint);
    }

    // -- TOML deserialization tests --

    #[test]
    fn toml_empty_tools_section() {
        let config: ToolsConfig = toml::from_str("").unwrap();
        assert!(config.is_all());
        assert!(config.include.is_empty());
        assert!(config.exclude.is_empty());
    }

    #[test]
    fn toml_preset_only() {
        let config: ToolsConfig = toml::from_str(r#"preset = "essentials""#).unwrap();
        assert_eq!(config.preset, "essentials");
        assert!(!config.is_all());
    }

    #[test]
    fn toml_full_config() {
        let config: ToolsConfig = toml::from_str(
            r#"
preset = "essentials"
include = ["enterprise_raw_api", "get_enterprise_crdb"]
exclude = ["flush_database"]
"#,
        )
        .unwrap();
        assert_eq!(config.preset, "essentials");
        assert_eq!(
            config.include,
            vec!["enterprise_raw_api", "get_enterprise_crdb"]
        );
        assert_eq!(config.exclude, vec!["flush_database"]);
    }
}
