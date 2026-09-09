//! MCP server policy configuration for granular tool access control.
//!
//! Provides a TOML-based policy system with three safety tiers, per-toolset
//! overrides, and explicit allow/deny lists. Replaces the binary `--read-only`
//! flag with fine-grained control over which tools are available.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tower_mcp::{CallToolResult, Error as McpError, Tool, ToolBuilder};

use crate::audit::AuditConfig;
use crate::presets::ToolsConfig;

/// Safety tier determining which categories of tools are allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SafetyTier {
    /// Only read-only tools (`read_only_hint = true`)
    #[default]
    ReadOnly,
    /// Reads + non-destructive writes (`destructive_hint = false`)
    ReadWrite,
    /// All operations including destructive
    Full,
}

impl fmt::Display for SafetyTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SafetyTier::ReadOnly => write!(f, "read-only"),
            SafetyTier::ReadWrite => write!(f, "read-write"),
            SafetyTier::Full => write!(f, "full"),
        }
    }
}

/// Per-toolset policy override.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ToolsetPolicy {
    /// Whether this toolset is enabled. `None` or `Some(true)` = enabled,
    /// `Some(false)` = disabled (tools are never registered).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Safety tier for this toolset (overrides the global tier)
    pub tier: Option<SafetyTier>,
    /// Explicit tool names to allow (evaluated after tier)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    /// Explicit tool names to deny (wins over allow and tier)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
}

/// TOML-deserializable policy configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[non_exhaustive]
pub struct PolicyConfig {
    /// Global default safety tier
    pub tier: SafetyTier,
    /// Deny entire categories globally (e.g., "destructive")
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deny_categories: Vec<String>,
    /// Global explicit allow list (tool names)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    /// Global explicit deny list (tool names, wins over allow)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
    /// Cloud toolset overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<ToolsetPolicy>,
    /// Enterprise toolset overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise: Option<ToolsetPolicy>,
    /// Database toolset overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<ToolsetPolicy>,
    /// App/profile toolset overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<ToolsetPolicy>,
    /// Audit logging configuration
    #[serde(default)]
    pub audit: AuditConfig,
    /// Tool visibility presets
    #[serde(default)]
    pub tools: ToolsConfig,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            tier: SafetyTier::ReadOnly,
            deny_categories: vec![],
            allow: vec![],
            deny: vec![],
            cloud: None,
            enterprise: None,
            database: None,
            app: None,
            audit: AuditConfig::default(),
            tools: ToolsConfig::default(),
        }
    }
}

/// Raw API passthrough tool names that are denied by default when no policy
/// file is loaded. Custom policy files use serde defaults (`deny = []`), so
/// raw tools become available through normal tier/allow mechanisms.
pub const RAW_TOOL_DENY_DEFAULTS: &[&str] =
    &["cloud_raw_api", "enterprise_raw_api", "redis_command"];

impl PolicyConfig {
    /// Return the set of toolsets explicitly disabled via `enabled = false`.
    pub fn disabled_toolsets(&self) -> HashSet<ToolsetKind> {
        let mut disabled = HashSet::new();
        for (kind, policy) in [
            (ToolsetKind::Cloud, &self.cloud),
            (ToolsetKind::Enterprise, &self.enterprise),
            (ToolsetKind::Database, &self.database),
            (ToolsetKind::App, &self.app),
        ] {
            if let Some(tp) = policy
                && tp.enabled == Some(false)
            {
                disabled.insert(kind);
            }
        }
        disabled
    }

    /// Build the synthesized default policy (used when no config file exists).
    /// This adds raw API passthrough tools to the deny list.
    pub fn synthesized_default() -> Self {
        Self {
            deny: RAW_TOOL_DENY_DEFAULTS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            ..Default::default()
        }
    }

    /// Load policy with resolution chain:
    /// 1. Explicit path (from `--policy` CLI flag)
    /// 2. `REDISCTL_MCP_POLICY` env var
    /// 3. `~/.config/redisctl/mcp-policy.toml`
    /// 4. Built-in default (read-only)
    ///
    /// Returns `(config, source_description)`.
    pub fn load(explicit_path: Option<&Path>) -> Result<(Self, String)> {
        // 1. Explicit path
        if let Some(path) = explicit_path {
            let config = Self::load_from_path(path)?;
            return Ok((config, format!("file: {}", path.display())));
        }

        // 2. Environment variable
        if let Ok(env_path) = std::env::var("REDISCTL_MCP_POLICY") {
            let path = PathBuf::from(&env_path);
            if path.exists() {
                let config = Self::load_from_path(&path)?;
                return Ok((config, format!("env: {}", path.display())));
            }
            tracing::warn!(
                path = %path.display(),
                "REDISCTL_MCP_POLICY path does not exist, using defaults"
            );
        }

        // 3. Standard config location
        if let Some(path) = Self::default_path()
            && path.exists()
        {
            let config = Self::load_from_path(&path)?;
            return Ok((config, format!("file: {}", path.display())));
        }

        // 4. Built-in default (includes raw tool deny list)
        Ok((Self::synthesized_default(), "default".to_string()))
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read policy file: {}", path.display()))?;
        let config: PolicyConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse policy file: {}", path.display()))?;
        tracing::info!(path = %path.display(), tier = %config.tier, "Loaded MCP policy");
        Ok(config)
    }

    /// Check if a policy file exists at the default path.
    pub fn default_path_exists() -> bool {
        Self::default_path().is_some_and(|p| p.exists())
    }

    /// Get the default policy file path following redisctl-core conventions.
    pub fn default_path() -> Option<PathBuf> {
        // On macOS, prefer Linux-style ~/.config/redisctl/ for cross-platform consistency
        // (matches redisctl-core Config::config_path())
        #[cfg(target_os = "macos")]
        {
            if let Some(base_dirs) = directories::BaseDirs::new() {
                let linux_style = base_dirs
                    .home_dir()
                    .join(".config")
                    .join("redisctl")
                    .join("mcp-policy.toml");
                if linux_style.exists() || linux_style.parent().is_some_and(|p| p.exists()) {
                    return Some(linux_style);
                }
            }
        }

        let proj_dirs = directories::ProjectDirs::from("com", "redis", "redisctl")?;
        Some(proj_dirs.config_dir().join("mcp-policy.toml"))
    }
}

/// Which toolset a tool belongs to (for per-toolset policy lookup).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ToolsetKind {
    /// Redis Cloud management tools.
    Cloud,
    /// Redis Enterprise management tools.
    Enterprise,
    /// Direct Redis database tools.
    Database,
    /// Application-level profile and system tools.
    App,
}

impl fmt::Display for ToolsetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolsetKind::Cloud => write!(f, "cloud"),
            ToolsetKind::Enterprise => write!(f, "enterprise"),
            ToolsetKind::Database => write!(f, "database"),
            ToolsetKind::App => write!(f, "app"),
        }
    }
}

/// Resolved policy ready for tool evaluation.
///
/// Contains the raw config plus precomputed lookup sets for O(1) evaluation.
pub struct Policy {
    config: PolicyConfig,
    /// Maps tool name -> toolset for per-toolset override lookup
    tool_toolset: HashMap<String, ToolsetKind>,
    /// Precomputed global deny set
    global_deny: HashSet<String>,
    /// Precomputed global allow set
    global_allow: HashSet<String>,
    /// Precomputed per-toolset deny sets
    toolset_deny: HashMap<ToolsetKind, HashSet<String>>,
    /// Precomputed per-toolset allow sets
    toolset_allow: HashMap<ToolsetKind, HashSet<String>>,
    /// Whether "destructive" is in deny_categories
    deny_destructive: bool,
    /// Description of policy source (for instructions and show_policy)
    source: String,
}

impl fmt::Debug for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Policy")
            .field("tier", &self.config.tier)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl Policy {
    /// Build a resolved policy from config and tool-to-toolset mapping.
    pub fn new(
        config: PolicyConfig,
        tool_toolset: HashMap<String, ToolsetKind>,
        source: String,
    ) -> Self {
        let global_deny: HashSet<String> = config.deny.iter().cloned().collect();
        let global_allow: HashSet<String> = config.allow.iter().cloned().collect();
        let deny_destructive = config.deny_categories.iter().any(|c| c == "destructive");

        let mut toolset_deny = HashMap::new();
        let mut toolset_allow = HashMap::new();

        for (kind, policy) in [
            (ToolsetKind::Cloud, &config.cloud),
            (ToolsetKind::Enterprise, &config.enterprise),
            (ToolsetKind::Database, &config.database),
            (ToolsetKind::App, &config.app),
        ] {
            if let Some(tp) = policy {
                toolset_deny.insert(kind, tp.deny.iter().cloned().collect());
                toolset_allow.insert(kind, tp.allow.iter().cloned().collect());
            }
        }

        Self {
            config,
            tool_toolset,
            global_deny,
            global_allow,
            toolset_deny,
            toolset_allow,
            deny_destructive,
            source,
        }
    }

    /// Determine if a tool is allowed by the policy.
    ///
    /// Evaluation order:
    /// 1. Global deny list -> DENY
    /// 2. Per-toolset deny list -> DENY
    /// 3. `deny_categories` (e.g., "destructive") -> DENY
    /// 4. Global allow list -> ALLOW
    /// 5. Per-toolset allow list -> ALLOW
    /// 6. Per-toolset tier (if set) -> evaluate against annotations
    /// 7. Global tier -> evaluate against annotations
    pub fn is_tool_allowed(&self, tool: &Tool) -> bool {
        let name = &tool.name;
        let annotations = tool.annotations.as_ref();
        let is_read_only = annotations.is_some_and(|a| a.read_only_hint);
        let is_destructive = annotations.is_some_and(|a| a.destructive_hint);

        // 1. Global deny always wins
        if self.global_deny.contains(name.as_str()) {
            return false;
        }

        // 2. Per-toolset deny
        let toolset = self.tool_toolset.get(name.as_str()).copied();
        if let Some(kind) = toolset
            && let Some(deny_set) = self.toolset_deny.get(&kind)
            && deny_set.contains(name.as_str())
        {
            return false;
        }

        // 3. Category deny
        if is_destructive && self.deny_destructive {
            return false;
        }

        // 4. Global allow overrides tier
        if self.global_allow.contains(name.as_str()) {
            return true;
        }

        // 5. Per-toolset allow overrides tier
        if let Some(kind) = toolset
            && let Some(allow_set) = self.toolset_allow.get(&kind)
            && allow_set.contains(name.as_str())
        {
            return true;
        }

        // 6 & 7. Effective tier (per-toolset overrides global)
        let effective_tier = toolset
            .and_then(|kind| self.toolset_config(kind).and_then(|tp| tp.tier))
            .unwrap_or(self.config.tier);

        Self::tier_allows(effective_tier, is_read_only, is_destructive)
    }

    /// Check if a tier allows a tool based on its annotations.
    fn tier_allows(tier: SafetyTier, is_read_only: bool, is_destructive: bool) -> bool {
        match tier {
            SafetyTier::ReadOnly => is_read_only,
            SafetyTier::ReadWrite => !is_destructive,
            SafetyTier::Full => true,
        }
    }

    fn toolset_config(&self, kind: ToolsetKind) -> Option<&ToolsetPolicy> {
        match kind {
            ToolsetKind::Cloud => self.config.cloud.as_ref(),
            ToolsetKind::Enterprise => self.config.enterprise.as_ref(),
            ToolsetKind::Database => self.config.database.as_ref(),
            ToolsetKind::App => self.config.app.as_ref(),
        }
    }

    /// Get the global safety tier.
    pub fn global_tier(&self) -> SafetyTier {
        self.config.tier
    }

    /// Generate a human-readable description for router instructions.
    pub fn describe(&self) -> String {
        let mut desc = String::new();

        desc.push_str(
            "Every tool carries MCP annotation hints that describe its safety characteristics:\n",
        );
        desc.push_str("- `readOnlyHint = true` -- reads data, never modifies state\n");
        desc.push_str("- `destructiveHint = false` -- writes data but is non-destructive (create, update, backup)\n");
        desc.push_str("- `destructiveHint = true` -- irreversible operation (delete, flush)\n\n");

        let tier_desc = match self.config.tier {
            SafetyTier::ReadOnly => {
                "**Active safety tier: READ-ONLY** -- only read-only tools are available. \
                 Write and destructive tools are hidden and will return unauthorized if called directly."
            }
            SafetyTier::ReadWrite => {
                "**Active safety tier: READ-WRITE** -- read-only and non-destructive write tools are available. \
                 Destructive tools (delete, flush) are hidden and will return unauthorized if called directly."
            }
            SafetyTier::Full => {
                "**Active safety tier: FULL** -- all tools including writes and destructive operations are available. \
                 Exercise caution with destructive tools."
            }
        };
        desc.push_str(tier_desc);

        // Note per-toolset overrides
        let overrides: Vec<String> = [
            ("cloud", self.config.cloud.as_ref()),
            ("enterprise", self.config.enterprise.as_ref()),
            ("database", self.config.database.as_ref()),
            ("app", self.config.app.as_ref()),
        ]
        .iter()
        .filter_map(|(name, tp)| tp.and_then(|p| p.tier).map(|t| format!("{}: {}", name, t)))
        .collect();

        if !overrides.is_empty() {
            desc.push_str(&format!(
                "\n\nPer-toolset overrides: {}",
                overrides.join(", ")
            ));
        }

        let disabled = self.config.disabled_toolsets();
        if !disabled.is_empty() {
            let mut names: Vec<String> = disabled.iter().map(|k| k.to_string()).collect();
            names.sort();
            desc.push_str(&format!(
                "\n\nDisabled toolsets (tools not registered): {}",
                names.join(", ")
            ));
        }

        if !self.config.deny.is_empty() {
            desc.push_str(&format!(
                "\n\nExplicitly denied tools: {}",
                self.config.deny.join(", ")
            ));
        }

        if self.deny_destructive {
            desc.push_str("\n\nAll destructive operations are denied by category.");
        }

        desc.push_str(
            "\n\nRaw API passthrough tools (`cloud_raw_api`, `enterprise_raw_api`, `redis_command`) \
             are available but denied by default. They can be enabled via a custom policy file.",
        );

        desc.push_str(
            "\n\nUse the `show_policy` tool to see the full active policy configuration.",
        );

        desc
    }

    /// Build a serializable summary of the policy for the show_policy tool.
    fn to_summary(&self) -> PolicySummary {
        PolicySummary {
            global_tier: self.config.tier.to_string(),
            deny_categories: self.config.deny_categories.clone(),
            global_allow: self.config.allow.clone(),
            global_deny: self.config.deny.clone(),
            cloud: self.config.cloud.as_ref().map(ToolsetPolicySummary::from),
            enterprise: self
                .config
                .enterprise
                .as_ref()
                .map(ToolsetPolicySummary::from),
            database: self
                .config
                .database
                .as_ref()
                .map(ToolsetPolicySummary::from),
            app: self.config.app.as_ref().map(ToolsetPolicySummary::from),
            source: self.source.clone(),
        }
    }
}

/// Serializable summary of the active policy.
#[derive(Debug, Serialize)]
pub struct PolicySummary {
    pub global_tier: String,
    pub deny_categories: Vec<String>,
    pub global_allow: Vec<String>,
    pub global_deny: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<ToolsetPolicySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise: Option<ToolsetPolicySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<ToolsetPolicySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<ToolsetPolicySummary>,
    pub source: String,
}

/// Serializable summary of a per-toolset policy.
#[derive(Debug, Serialize)]
pub struct ToolsetPolicySummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
}

impl From<&ToolsetPolicy> for ToolsetPolicySummary {
    fn from(tp: &ToolsetPolicy) -> Self {
        Self {
            enabled: tp.enabled,
            tier: tp.tier.map(|t| t.to_string()),
            allow: tp.allow.clone(),
            deny: tp.deny.clone(),
        }
    }
}

/// Build the `show_policy` MCP tool.
pub fn show_policy_tool(policy: Arc<Policy>) -> Tool {
    ToolBuilder::new("show_policy")
        .description(
            "Show the active MCP server policy including safety tiers, \
             per-toolset overrides, and allow/deny lists. \
             Use this to understand what operations are permitted.",
        )
        .read_only_safe()
        .handler(move |_: tower_mcp::NoParams| {
            let policy = policy.clone();
            async move {
                let summary = policy.to_summary();
                CallToolResult::from_serialize(&summary)
                    .map_err(|e| McpError::tool(format!("Failed to serialize policy: {}", e)))
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_mcp::ToolBuilder;

    fn make_read_only_tool(name: &str) -> Tool {
        ToolBuilder::new(name)
            .description("Read-only test tool")
            .read_only_safe()
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
            .build()
    }

    fn make_write_tool(name: &str) -> Tool {
        ToolBuilder::new(name)
            .description("Write test tool")
            .non_destructive()
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
            .build()
    }

    fn make_destructive_tool(name: &str) -> Tool {
        ToolBuilder::new(name)
            .description("DANGEROUS: Destructive test tool")
            .destructive()
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
            .build()
    }

    fn empty_mapping() -> HashMap<String, ToolsetKind> {
        HashMap::new()
    }

    fn policy_with_config(config: PolicyConfig) -> Policy {
        Policy::new(config, empty_mapping(), "test".to_string())
    }

    // -- Tier evaluation tests --

    #[test]
    fn default_policy_is_read_only() {
        let config = PolicyConfig::default();
        assert_eq!(config.tier, SafetyTier::ReadOnly);
    }

    #[test]
    fn read_only_allows_read_tools() {
        let policy = policy_with_config(PolicyConfig::default());
        let tool = make_read_only_tool("list_subscriptions");
        assert!(policy.is_tool_allowed(&tool));
    }

    #[test]
    fn read_only_blocks_write_tools() {
        let policy = policy_with_config(PolicyConfig::default());
        let tool = make_write_tool("create_database");
        assert!(!policy.is_tool_allowed(&tool));
    }

    #[test]
    fn read_only_blocks_destructive_tools() {
        let policy = policy_with_config(PolicyConfig::default());
        let tool = make_destructive_tool("delete_database");
        assert!(!policy.is_tool_allowed(&tool));
    }

    #[test]
    fn read_write_allows_non_destructive() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::ReadWrite,
            ..Default::default()
        });
        let read = make_read_only_tool("list_subscriptions");
        let write = make_write_tool("create_database");
        assert!(policy.is_tool_allowed(&read));
        assert!(policy.is_tool_allowed(&write));
    }

    #[test]
    fn read_write_blocks_destructive() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::ReadWrite,
            ..Default::default()
        });
        let tool = make_destructive_tool("delete_database");
        assert!(!policy.is_tool_allowed(&tool));
    }

    #[test]
    fn full_allows_everything() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::Full,
            ..Default::default()
        });
        let read = make_read_only_tool("list_subscriptions");
        let write = make_write_tool("create_database");
        let destructive = make_destructive_tool("delete_database");
        assert!(policy.is_tool_allowed(&read));
        assert!(policy.is_tool_allowed(&write));
        assert!(policy.is_tool_allowed(&destructive));
    }

    // -- Explicit deny/allow tests --

    #[test]
    fn explicit_deny_overrides_full_tier() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::Full,
            deny: vec!["delete_database".to_string()],
            ..Default::default()
        });
        let tool = make_destructive_tool("delete_database");
        assert!(!policy.is_tool_allowed(&tool));
    }

    #[test]
    fn explicit_allow_overrides_read_only_tier() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::ReadOnly,
            allow: vec!["create_database".to_string()],
            ..Default::default()
        });
        let tool = make_write_tool("create_database");
        assert!(policy.is_tool_allowed(&tool));
    }

    #[test]
    fn deny_wins_over_allow() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::Full,
            allow: vec!["delete_database".to_string()],
            deny: vec!["delete_database".to_string()],
            ..Default::default()
        });
        let tool = make_destructive_tool("delete_database");
        assert!(!policy.is_tool_allowed(&tool));
    }

    // -- Category deny tests --

    #[test]
    fn deny_category_destructive_blocks_all_destructive() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::Full,
            deny_categories: vec!["destructive".to_string()],
            ..Default::default()
        });
        let write = make_write_tool("create_database");
        let destructive = make_destructive_tool("delete_database");
        assert!(policy.is_tool_allowed(&write));
        assert!(!policy.is_tool_allowed(&destructive));
    }

    // -- Per-toolset override tests --

    #[test]
    fn per_toolset_tier_overrides_global() {
        let mut mapping = HashMap::new();
        mapping.insert("create_database".to_string(), ToolsetKind::Cloud);
        mapping.insert(
            "create_enterprise_database".to_string(),
            ToolsetKind::Enterprise,
        );

        let policy = Policy::new(
            PolicyConfig {
                tier: SafetyTier::ReadOnly,
                cloud: Some(ToolsetPolicy {
                    tier: Some(SafetyTier::ReadWrite),
                    ..Default::default()
                }),
                ..Default::default()
            },
            mapping,
            "test".to_string(),
        );

        let cloud_write = make_write_tool("create_database");
        let ent_write = make_write_tool("create_enterprise_database");

        assert!(policy.is_tool_allowed(&cloud_write)); // Cloud is read-write
        assert!(!policy.is_tool_allowed(&ent_write)); // Enterprise inherits read-only
    }

    #[test]
    fn per_toolset_deny_overrides_toolset_tier() {
        let mut mapping = HashMap::new();
        mapping.insert("flush_database".to_string(), ToolsetKind::Cloud);

        let policy = Policy::new(
            PolicyConfig {
                tier: SafetyTier::Full,
                cloud: Some(ToolsetPolicy {
                    tier: Some(SafetyTier::Full),
                    deny: vec!["flush_database".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
            mapping,
            "test".to_string(),
        );

        let tool = make_destructive_tool("flush_database");
        assert!(!policy.is_tool_allowed(&tool));
    }

    #[test]
    fn per_toolset_allow_overrides_global_tier() {
        let mut mapping = HashMap::new();
        mapping.insert("redis_set".to_string(), ToolsetKind::Database);

        let policy = Policy::new(
            PolicyConfig {
                tier: SafetyTier::ReadOnly,
                database: Some(ToolsetPolicy {
                    tier: None, // inherits global read-only
                    allow: vec!["redis_set".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
            mapping,
            "test".to_string(),
        );

        let tool = make_write_tool("redis_set");
        assert!(policy.is_tool_allowed(&tool));
    }

    // -- TOML parsing tests --

    #[test]
    fn toml_roundtrip() {
        let config = PolicyConfig {
            tier: SafetyTier::ReadWrite,
            deny_categories: vec!["destructive".to_string()],
            allow: vec!["backup_database".to_string()],
            deny: vec!["flush_database".to_string()],
            cloud: Some(ToolsetPolicy {
                tier: Some(SafetyTier::Full),
                ..Default::default()
            }),
            enterprise: None,
            database: Some(ToolsetPolicy {
                tier: Some(SafetyTier::ReadOnly),
                allow: vec!["redis_set".to_string()],
                ..Default::default()
            }),
            app: None,
            audit: AuditConfig::default(),
            tools: ToolsConfig::default(),
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: PolicyConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.tier, SafetyTier::ReadWrite);
        assert_eq!(parsed.deny_categories, vec!["destructive"]);
        assert_eq!(parsed.allow, vec!["backup_database"]);
        assert_eq!(parsed.deny, vec!["flush_database"]);
        assert_eq!(parsed.cloud.unwrap().tier, Some(SafetyTier::Full));
        assert!(parsed.enterprise.is_none());
        let db = parsed.database.unwrap();
        assert_eq!(db.tier, Some(SafetyTier::ReadOnly));
        assert_eq!(db.allow, vec!["redis_set"]);
    }

    #[test]
    fn toml_minimal_config() {
        let config: PolicyConfig = toml::from_str("tier = \"full\"").unwrap();
        assert_eq!(config.tier, SafetyTier::Full);
        assert!(config.deny.is_empty());
        assert!(config.allow.is_empty());
        assert!(config.cloud.is_none());
    }

    #[test]
    fn toml_empty_is_read_only() {
        let config: PolicyConfig = toml::from_str("").unwrap();
        assert_eq!(config.tier, SafetyTier::ReadOnly);
    }

    #[test]
    fn toml_complex_config() {
        let toml_str = r#"
tier = "read-only"
deny_categories = ["destructive"]
deny = ["flush_database", "delete_subscription"]

[cloud]
tier = "read-write"

[enterprise]
tier = "read-only"

[database]
tier = "read-only"
allow = ["redis_set", "redis_expire"]
"#;
        let config: PolicyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.tier, SafetyTier::ReadOnly);
        assert_eq!(config.deny_categories, vec!["destructive"]);
        assert_eq!(config.deny, vec!["flush_database", "delete_subscription"]);
        assert_eq!(
            config.cloud.as_ref().unwrap().tier,
            Some(SafetyTier::ReadWrite)
        );
        assert_eq!(
            config.enterprise.as_ref().unwrap().tier,
            Some(SafetyTier::ReadOnly)
        );
        let db = config.database.as_ref().unwrap();
        assert_eq!(db.tier, Some(SafetyTier::ReadOnly));
        assert_eq!(db.allow, vec!["redis_set", "redis_expire"]);
    }

    // -- Backward compatibility tests --

    #[test]
    fn backward_compat_read_only_true() {
        // Synthesized policy from --read-only=true should match old behavior:
        // only read-only tools allowed
        let policy = policy_with_config(PolicyConfig::default());
        let read = make_read_only_tool("list_subscriptions");
        let write = make_write_tool("create_database");
        let destructive = make_destructive_tool("delete_database");

        assert!(policy.is_tool_allowed(&read));
        assert!(!policy.is_tool_allowed(&write));
        assert!(!policy.is_tool_allowed(&destructive));
    }

    #[test]
    fn backward_compat_read_only_false() {
        // Synthesized policy from --read-only=false should match old behavior:
        // all tools allowed
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::Full,
            ..Default::default()
        });
        let read = make_read_only_tool("list_subscriptions");
        let write = make_write_tool("create_database");
        let destructive = make_destructive_tool("delete_database");

        assert!(policy.is_tool_allowed(&read));
        assert!(policy.is_tool_allowed(&write));
        assert!(policy.is_tool_allowed(&destructive));
    }

    // -- Global tier accessor --

    #[test]
    fn global_tier_returns_config_tier() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::ReadWrite,
            ..Default::default()
        });
        assert_eq!(policy.global_tier(), SafetyTier::ReadWrite);
    }

    // -- Describe output --

    #[test]
    fn describe_contains_tier() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::ReadWrite,
            ..Default::default()
        });
        let desc = policy.describe();
        assert!(desc.contains("READ-WRITE"));
    }

    #[test]
    fn describe_notes_overrides() {
        let policy = policy_with_config(PolicyConfig {
            tier: SafetyTier::ReadOnly,
            cloud: Some(ToolsetPolicy {
                tier: Some(SafetyTier::ReadWrite),
                ..Default::default()
            }),
            ..Default::default()
        });
        let desc = policy.describe();
        assert!(desc.contains("cloud: read-write"));
    }

    // -- show_policy tool --

    #[test]
    fn show_policy_tool_is_read_only() {
        let policy = Arc::new(policy_with_config(PolicyConfig::default()));
        let tool = show_policy_tool(policy);
        let ann = tool.annotations.as_ref().unwrap();
        assert!(ann.read_only_hint);
        assert!(!ann.destructive_hint);
    }

    // -- Tools without annotations (edge case) --

    #[test]
    fn tool_without_annotations_treated_as_non_read_only() {
        let tool = ToolBuilder::new("unknown")
            .description("No annotations")
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
            .build();

        let policy = policy_with_config(PolicyConfig::default()); // read-only
        // Tool has no read_only_hint=true, so it should be blocked at read-only tier
        assert!(!policy.is_tool_allowed(&tool));
    }

    // -- Raw tool default deny tests --

    #[test]
    fn synthesized_default_denies_raw_tools() {
        let config = PolicyConfig::synthesized_default();
        assert!(config.deny.contains(&"cloud_raw_api".to_string()));
        assert!(config.deny.contains(&"enterprise_raw_api".to_string()));
        assert!(config.deny.contains(&"redis_command".to_string()));

        let policy = policy_with_config(config);
        let cloud_raw = make_destructive_tool("cloud_raw_api");
        let enterprise_raw = make_destructive_tool("enterprise_raw_api");
        let redis_cmd = make_destructive_tool("redis_command");

        assert!(!policy.is_tool_allowed(&cloud_raw));
        assert!(!policy.is_tool_allowed(&enterprise_raw));
        assert!(!policy.is_tool_allowed(&redis_cmd));
    }

    #[test]
    fn synthesized_default_denies_raw_even_at_full_tier() {
        let mut config = PolicyConfig::synthesized_default();
        config.tier = SafetyTier::Full;
        let policy = policy_with_config(config);

        let cloud_raw = make_destructive_tool("cloud_raw_api");
        let enterprise_raw = make_destructive_tool("enterprise_raw_api");
        let redis_cmd = make_destructive_tool("redis_command");

        assert!(!policy.is_tool_allowed(&cloud_raw));
        assert!(!policy.is_tool_allowed(&enterprise_raw));
        assert!(!policy.is_tool_allowed(&redis_cmd));
    }

    #[test]
    fn custom_policy_file_does_not_auto_deny_raw_tools() {
        // A custom TOML policy with tier=full and empty deny list should allow raw tools
        let config: PolicyConfig = toml::from_str("tier = \"full\"").unwrap();
        assert!(config.deny.is_empty());

        let policy = policy_with_config(config);
        let cloud_raw = make_destructive_tool("cloud_raw_api");
        let enterprise_raw = make_destructive_tool("enterprise_raw_api");
        let redis_cmd = make_destructive_tool("redis_command");

        assert!(policy.is_tool_allowed(&cloud_raw));
        assert!(policy.is_tool_allowed(&enterprise_raw));
        assert!(policy.is_tool_allowed(&redis_cmd));
    }

    #[test]
    fn describe_mentions_raw_tools() {
        let policy = policy_with_config(PolicyConfig::synthesized_default());
        let desc = policy.describe();
        assert!(desc.contains("cloud_raw_api"));
        assert!(desc.contains("enterprise_raw_api"));
        assert!(desc.contains("redis_command"));
    }

    // -- [tools] section TOML tests --

    #[test]
    fn toml_no_tools_section_defaults_to_all() {
        let config: PolicyConfig = toml::from_str("tier = \"full\"").unwrap();
        assert!(config.tools.is_all());
        assert!(config.tools.include.is_empty());
        assert!(config.tools.exclude.is_empty());
    }

    #[test]
    fn toml_empty_tools_section_defaults_to_all() {
        let config: PolicyConfig = toml::from_str("[tools]\n").unwrap();
        assert!(config.tools.is_all());
    }

    #[test]
    fn toml_tools_with_preset() {
        let config: PolicyConfig = toml::from_str("[tools]\npreset = \"essentials\"\n").unwrap();
        assert_eq!(config.tools.preset, "essentials");
        assert!(!config.tools.is_all());
    }

    #[test]
    fn toml_tools_with_include_exclude() {
        let toml_str = r#"
tier = "read-only"

[tools]
preset = "essentials"
include = ["enterprise_raw_api", "get_enterprise_crdb"]
exclude = ["flush_database"]
"#;
        let config: PolicyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.tools.preset, "essentials");
        assert_eq!(
            config.tools.include,
            vec!["enterprise_raw_api", "get_enterprise_crdb"]
        );
        assert_eq!(config.tools.exclude, vec!["flush_database"]);
        // Other fields still parse correctly
        assert_eq!(config.tier, SafetyTier::ReadOnly);
    }

    // -- Toolset enabled/disabled tests --

    #[test]
    fn toml_enabled_false_parses() {
        let toml_str = r#"
tier = "read-only"

[enterprise]
enabled = false

[database]
enabled = false
"#;
        let config: PolicyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.enterprise.as_ref().unwrap().enabled, Some(false));
        assert_eq!(config.database.as_ref().unwrap().enabled, Some(false));
        // Cloud not mentioned: should be None
        assert!(config.cloud.is_none());
    }

    #[test]
    fn toml_enabled_true_parses() {
        let toml_str = r#"
[cloud]
enabled = true
tier = "read-write"
"#;
        let config: PolicyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.cloud.as_ref().unwrap().enabled, Some(true));
    }

    #[test]
    fn toml_enabled_omitted_defaults_to_none() {
        let toml_str = r#"
[cloud]
tier = "read-write"
"#;
        let config: PolicyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.cloud.as_ref().unwrap().enabled, None);
    }

    #[test]
    fn disabled_toolsets_returns_correct_set() {
        let config = PolicyConfig {
            enterprise: Some(ToolsetPolicy {
                enabled: Some(false),
                ..Default::default()
            }),
            database: Some(ToolsetPolicy {
                enabled: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        let disabled = config.disabled_toolsets();
        assert!(disabled.contains(&ToolsetKind::Enterprise));
        assert!(disabled.contains(&ToolsetKind::Database));
        assert!(!disabled.contains(&ToolsetKind::Cloud));
        assert!(!disabled.contains(&ToolsetKind::App));
    }

    #[test]
    fn disabled_toolsets_empty_when_all_enabled() {
        let config = PolicyConfig {
            cloud: Some(ToolsetPolicy {
                enabled: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(config.disabled_toolsets().is_empty());
    }

    #[test]
    fn disabled_toolsets_empty_when_no_overrides() {
        let config = PolicyConfig::default();
        assert!(config.disabled_toolsets().is_empty());
    }

    #[test]
    fn describe_mentions_disabled_toolsets() {
        let policy = policy_with_config(PolicyConfig {
            enterprise: Some(ToolsetPolicy {
                enabled: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        });
        let desc = policy.describe();
        assert!(desc.contains("Disabled toolsets"));
        assert!(desc.contains("enterprise"));
    }

    #[test]
    fn describe_omits_disabled_when_none() {
        let policy = policy_with_config(PolicyConfig::default());
        let desc = policy.describe();
        assert!(!desc.contains("Disabled toolsets"));
    }

    #[test]
    fn enabled_false_serializes_in_summary() {
        let tp = ToolsetPolicy {
            enabled: Some(false),
            tier: Some(SafetyTier::ReadOnly),
            ..Default::default()
        };
        let summary = ToolsetPolicySummary::from(&tp);
        assert_eq!(summary.enabled, Some(false));
        assert_eq!(summary.tier, Some("read-only".to_string()));
    }
}
