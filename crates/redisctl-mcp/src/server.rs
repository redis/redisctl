//! Safe construction of an embeddable redisctl MCP router.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};
use redisctl_core::Config;
#[cfg(any(feature = "cloud", feature = "enterprise", feature = "database"))]
use redisctl_core::DeploymentType;
use tower_mcp::{
    CapabilityFilter, DenialBehavior, DynamicPromptRegistry, McpRouter, PromptBuilder, Tool,
};

use crate::policy::{Policy, PolicyConfig, ToolsetKind};
use crate::presets::{self, ToolVisibility, ToolsConfig};
use crate::state::{AppState, CredentialSource};
use crate::{prompts, tools};

/// Builder for the supported redisctl MCP embedding surface.
///
/// The builder creates the tool-to-toolset mapping before it resolves the
/// policy, then installs the same policy and visibility filters used by the
/// `redisctl-mcp` binary. This prevents embedders from accidentally registering
/// raw tool routers without the configured safety policy.
pub struct McpServerBuilder {
    credential_source: CredentialSource,
    policy_config: PolicyConfig,
    policy_source: String,
    enabled: EnabledToolsets,
    database_url: Option<String>,
    cluster: bool,
    client_name: Option<String>,
    skills_dir: Option<PathBuf>,
}

impl McpServerBuilder {
    /// Create a builder using all toolsets compiled into this crate.
    ///
    /// `policy_source` is a human-readable description such as a policy file
    /// path or `"embedded default"`; it is included in policy diagnostics.
    pub fn new(
        credential_source: CredentialSource,
        policy_config: PolicyConfig,
        policy_source: impl Into<String>,
    ) -> Self {
        Self {
            credential_source,
            policy_config,
            policy_source: policy_source.into(),
            enabled: EnabledToolsets::all_compiled(),
            database_url: None,
            cluster: false,
            client_name: None,
            skills_dir: None,
        }
    }

    /// Select toolsets or submodules using the same syntax as `--tools`.
    ///
    /// Examples include `"cloud"`, `"database"`, and
    /// `"cloud:subscriptions"`. A toolset disabled by policy remains disabled
    /// even when it is selected explicitly.
    pub fn with_tool_specs<I, S>(mut self, specs: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.enabled = parse_tool_specs(specs)?;
        Ok(self)
    }

    /// Select toolsets represented by profiles in a redisctl configuration.
    ///
    /// An empty configuration leaves the default of all compiled toolsets in
    /// place. The app/profile toolset is always included.
    pub fn with_profile_toolsets(mut self, config: &Config) -> Self {
        if let Some(enabled) = toolsets_from_config(config) {
            self.enabled = enabled;
        }
        self
    }

    /// Set the direct Redis connection URL used by database tools.
    pub fn with_database_url(mut self, database_url: Option<String>) -> Self {
        self.database_url = database_url;
        self
    }

    /// Enable or disable Redis Cluster redirection handling.
    pub fn with_cluster_mode(mut self, cluster: bool) -> Self {
        self.cluster = cluster;
        self
    }

    /// Set the Redis client name used by direct database connections.
    pub fn with_client_name(mut self, client_name: Option<String>) -> Self {
        self.client_name = client_name;
        self
    }

    /// Load Agent Skills from the provided directory as dynamic MCP prompts.
    pub fn with_skills_dir(mut self, skills_dir: Option<PathBuf>) -> Self {
        self.skills_dir = skills_dir;
        self
    }

    /// Build the router, shared application state, and audit metadata.
    pub fn build(mut self) -> Result<McpServer> {
        let disabled = self.policy_config.disabled_toolsets();
        self.enabled
            .retain(|toolset| !disabled.contains(&toolset.kind()));

        let tool_toolset = build_tool_toolset_mapping(&self.enabled);
        let tools_config = self.policy_config.tools.clone();
        let policy = Arc::new(Policy::new(
            self.policy_config,
            tool_toolset.clone(),
            self.policy_source,
        ));
        let state = Arc::new(AppState::new(
            self.credential_source,
            policy.clone(),
            self.database_url,
            self.cluster,
            self.client_name,
        )?);
        let router = build_router(
            state.clone(),
            policy,
            &self.enabled,
            tools_config,
            &tool_toolset,
            self.skills_dir.as_deref(),
        )?;

        let mut enabled_toolsets = self
            .enabled
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        enabled_toolsets.sort();

        Ok(McpServer {
            router,
            state,
            tool_toolset: Arc::new(tool_toolset),
            enabled_toolsets,
        })
    }
}

/// A policy-filtered MCP router and its resolved runtime metadata.
pub struct McpServer {
    router: McpRouter,
    state: Arc<AppState>,
    tool_toolset: Arc<HashMap<String, ToolsetKind>>,
    enabled_toolsets: Vec<String>,
}

impl McpServer {
    /// Return the enabled top-level toolset names in deterministic order.
    pub fn enabled_toolsets(&self) -> &[String] {
        &self.enabled_toolsets
    }

    /// Return the shared state used by the router.
    pub fn state(&self) -> &Arc<AppState> {
        &self.state
    }

    #[cfg(test)]
    pub(crate) fn tool_toolset(&self) -> &HashMap<String, ToolsetKind> {
        &self.tool_toolset
    }

    /// Consume the server and return only its MCP router.
    pub fn into_router(self) -> McpRouter {
        self.router
    }

    /// Consume the server and return the router plus audit toolset metadata.
    pub fn into_parts(self) -> (McpRouter, Arc<HashMap<String, ToolsetKind>>) {
        (self.router, self.tool_toolset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Toolset {
    #[cfg(feature = "cloud")]
    Cloud,
    #[cfg(feature = "enterprise")]
    Enterprise,
    #[cfg(feature = "database")]
    Database,
    App,
}

impl Toolset {
    fn from_str(value: &str) -> Option<Self> {
        match value {
            #[cfg(feature = "cloud")]
            "cloud" => Some(Self::Cloud),
            #[cfg(feature = "enterprise")]
            "enterprise" => Some(Self::Enterprise),
            #[cfg(feature = "database")]
            "database" => Some(Self::Database),
            "app" => Some(Self::App),
            _ => None,
        }
    }

    fn all_names() -> Vec<&'static str> {
        #[allow(unused_mut)]
        let mut names = vec!["app"];
        #[cfg(feature = "cloud")]
        names.push("cloud");
        #[cfg(feature = "enterprise")]
        names.push("enterprise");
        #[cfg(feature = "database")]
        names.push("database");
        names.sort_unstable();
        names
    }

    fn kind(self) -> ToolsetKind {
        match self {
            #[cfg(feature = "cloud")]
            Self::Cloud => ToolsetKind::Cloud,
            #[cfg(feature = "enterprise")]
            Self::Enterprise => ToolsetKind::Enterprise,
            #[cfg(feature = "database")]
            Self::Database => ToolsetKind::Database,
            Self::App => ToolsetKind::App,
        }
    }
}

impl std::fmt::Display for Toolset {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "cloud")]
            Self::Cloud => formatter.write_str("cloud"),
            #[cfg(feature = "enterprise")]
            Self::Enterprise => formatter.write_str("enterprise"),
            #[cfg(feature = "database")]
            Self::Database => formatter.write_str("database"),
            Self::App => formatter.write_str("app"),
        }
    }
}

#[derive(Debug, Clone)]
enum SubModuleSelection {
    All,
    Selected(HashSet<String>),
}

#[derive(Debug, Clone)]
struct EnabledToolsets {
    selections: HashMap<Toolset, SubModuleSelection>,
}

impl EnabledToolsets {
    fn all_of(toolsets: impl IntoIterator<Item = Toolset>) -> Self {
        Self {
            selections: toolsets
                .into_iter()
                .map(|toolset| (toolset, SubModuleSelection::All))
                .collect(),
        }
    }

    fn all_compiled() -> Self {
        #[allow(unused_mut)]
        let mut toolsets = vec![Toolset::App];
        #[cfg(feature = "cloud")]
        toolsets.push(Toolset::Cloud);
        #[cfg(feature = "enterprise")]
        toolsets.push(Toolset::Enterprise);
        #[cfg(feature = "database")]
        toolsets.push(Toolset::Database);
        Self::all_of(toolsets)
    }

    fn retain(&mut self, predicate: impl Fn(&Toolset) -> bool) {
        self.selections.retain(|toolset, _| predicate(toolset));
    }

    fn iter(&self) -> impl Iterator<Item = &Toolset> {
        self.selections.keys()
    }
}

fn parse_tool_specs<I, S>(specs: I) -> Result<EnabledToolsets>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut selections = HashMap::new();

    for spec in specs {
        let spec = spec.as_ref();
        if let Some((toolset_name, submodule_name)) = spec.split_once(':') {
            let toolset = Toolset::from_str(toolset_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown toolset '{}'. Valid toolsets: {}",
                    toolset_name,
                    Toolset::all_names().join(", ")
                )
            })?;

            if matches!(toolset, Toolset::App) {
                bail!("'app' has no sub-modules (got 'app:{}')", submodule_name);
            }
            if !is_valid_submodule(toolset, submodule_name) {
                bail!(
                    "Unknown sub-module '{}' for toolset '{}'. Valid sub-modules: {}",
                    submodule_name,
                    toolset,
                    valid_submodule_names(toolset).join(", ")
                );
            }

            match selections.get_mut(&toolset) {
                Some(SubModuleSelection::All) => {}
                Some(SubModuleSelection::Selected(selected)) => {
                    selected.insert(submodule_name.to_string());
                }
                None => {
                    selections.insert(
                        toolset,
                        SubModuleSelection::Selected(HashSet::from([submodule_name.to_string()])),
                    );
                }
            }
        } else {
            let toolset = Toolset::from_str(spec).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown toolset '{}'. Valid toolsets: {}",
                    spec,
                    Toolset::all_names().join(", ")
                )
            })?;
            selections.insert(toolset, SubModuleSelection::All);
        }
    }

    Ok(EnabledToolsets { selections })
}

#[allow(unused_variables)]
fn is_valid_submodule(toolset: Toolset, name: &str) -> bool {
    match toolset {
        #[cfg(feature = "cloud")]
        Toolset::Cloud => tools::cloud::sub_tool_names(name).is_some(),
        #[cfg(feature = "enterprise")]
        Toolset::Enterprise => tools::enterprise::sub_tool_names(name).is_some(),
        #[cfg(feature = "database")]
        Toolset::Database => tools::redis::sub_tool_names(name).is_some(),
        Toolset::App => false,
    }
}

fn valid_submodule_names(toolset: Toolset) -> Vec<&'static str> {
    match toolset {
        #[cfg(feature = "cloud")]
        Toolset::Cloud => tools::cloud::SUB_MODULES
            .iter()
            .map(|submodule| submodule.name)
            .collect(),
        #[cfg(feature = "enterprise")]
        Toolset::Enterprise => tools::enterprise::SUB_MODULES
            .iter()
            .map(|submodule| submodule.name)
            .collect(),
        #[cfg(feature = "database")]
        Toolset::Database => tools::redis::SUB_MODULES
            .iter()
            .map(|submodule| submodule.name)
            .collect(),
        Toolset::App => Vec::new(),
    }
}

fn selected_tool_names(toolset: Toolset, selection: &SubModuleSelection) -> Vec<String> {
    match selection {
        SubModuleSelection::All => match toolset {
            #[cfg(feature = "cloud")]
            Toolset::Cloud => tools::cloud::tool_names(),
            #[cfg(feature = "enterprise")]
            Toolset::Enterprise => tools::enterprise::tool_names(),
            #[cfg(feature = "database")]
            Toolset::Database => tools::redis::tool_names(),
            Toolset::App => tools::profile::tool_names(),
        },
        SubModuleSelection::Selected(submodules) => {
            let mut names = Vec::new();
            for _submodule in submodules {
                let selected: Option<&[&str]> = match toolset {
                    #[cfg(feature = "cloud")]
                    Toolset::Cloud => tools::cloud::sub_tool_names(_submodule),
                    #[cfg(feature = "enterprise")]
                    Toolset::Enterprise => tools::enterprise::sub_tool_names(_submodule),
                    #[cfg(feature = "database")]
                    Toolset::Database => tools::redis::sub_tool_names(_submodule),
                    Toolset::App => None,
                };
                if let Some(selected) = selected {
                    names.extend(selected.iter().map(|name| (*name).to_string()));
                }
            }
            names
        }
    }
}

fn toolsets_from_config(config: &Config) -> Option<EnabledToolsets> {
    if config.profiles.is_empty() {
        return None;
    }

    #[allow(unused_mut)]
    let mut toolsets = vec![Toolset::App];
    #[cfg(feature = "cloud")]
    if !config
        .get_profiles_of_type(DeploymentType::Cloud)
        .is_empty()
    {
        toolsets.push(Toolset::Cloud);
    }
    #[cfg(feature = "enterprise")]
    if !config
        .get_profiles_of_type(DeploymentType::Enterprise)
        .is_empty()
    {
        toolsets.push(Toolset::Enterprise);
    }
    #[cfg(feature = "database")]
    if !config
        .get_profiles_of_type(DeploymentType::Database)
        .is_empty()
    {
        toolsets.push(Toolset::Database);
    }
    Some(EnabledToolsets::all_of(toolsets))
}

fn build_tool_toolset_mapping(enabled: &EnabledToolsets) -> HashMap<String, ToolsetKind> {
    let mut mapping = HashMap::new();
    for (toolset, selection) in &enabled.selections {
        for name in selected_tool_names(*toolset, selection) {
            mapping.insert(name, toolset.kind());
        }
    }
    mapping
}

fn merge_toolset_router(
    router: McpRouter,
    toolset: Toolset,
    selection: &SubModuleSelection,
    state: Arc<AppState>,
) -> McpRouter {
    match selection {
        SubModuleSelection::All => match toolset {
            #[cfg(feature = "cloud")]
            Toolset::Cloud => router.merge(tools::cloud::router(state)),
            #[cfg(feature = "enterprise")]
            Toolset::Enterprise => router.merge(tools::enterprise::router(state)),
            #[cfg(feature = "database")]
            Toolset::Database => router.merge(tools::redis::router(state)),
            Toolset::App => router.merge(tools::profile::router(state)),
        },
        SubModuleSelection::Selected(submodules) => {
            let mut router = router;
            for _submodule in submodules {
                let selected = match toolset {
                    #[cfg(feature = "cloud")]
                    Toolset::Cloud => tools::cloud::sub_router(_submodule, state.clone()),
                    #[cfg(feature = "enterprise")]
                    Toolset::Enterprise => tools::enterprise::sub_router(_submodule, state.clone()),
                    #[cfg(feature = "database")]
                    Toolset::Database => tools::redis::sub_router(_submodule, state.clone()),
                    Toolset::App => None,
                };
                if let Some(selected) = selected {
                    router = router.merge(selected);
                }
            }
            router
        }
    }
}

fn build_router(
    state: Arc<AppState>,
    policy: Arc<Policy>,
    enabled: &EnabledToolsets,
    tools_config: ToolsConfig,
    tool_toolset: &HashMap<String, ToolsetKind>,
    skills_dir: Option<&Path>,
) -> Result<McpRouter> {
    let router = McpRouter::new().server_info("redisctl-mcp", env!("CARGO_PKG_VERSION"));
    let (mut router, prompt_registry) = router.with_dynamic_prompts();

    for (toolset, selection) in &enabled.selections {
        router = merge_toolset_router(router, *toolset, selection, state.clone());
    }

    prompt_registry.register(prompts::troubleshoot_database_prompt());
    prompt_registry.register(prompts::analyze_performance_prompt());
    prompt_registry.register(prompts::capacity_planning_prompt());
    prompt_registry.register(prompts::migration_planning_prompt());

    if let Some(directory) = skills_dir {
        let count = load_skills(directory, &prompt_registry);
        if count > 0 {
            tracing::info!(count, dir = %directory.display(), "Loaded skill prompts");
        }
    }

    router = router.tool(crate::policy::show_policy_tool(policy.clone()));

    let all_tools = tool_toolset.keys().cloned().collect::<HashSet<_>>();
    let visible = presets::resolve_visible_tools(&tools_config, &all_tools, tool_toolset);
    let preset_active = !tools_config.is_all()
        || !tools_config.include.is_empty()
        || !tools_config.exclude.is_empty();
    if preset_active {
        tracing::info!(
            preset = %tools_config.preset,
            active = visible.len(),
            total = all_tools.len(),
            "Tool visibility preset active"
        );
    }

    let visibility = Arc::new(ToolVisibility {
        visible: visible.clone(),
        all_tools: tool_toolset.clone(),
        config: tools_config,
    });
    router = router.tool(presets::list_available_tools_tool(visibility));

    let mut prefix = format!(
        "# Redis Cloud and Enterprise MCP Server\n\n## Safety Model\n\n{}\n",
        policy.describe()
    );
    if preset_active {
        prefix.push_str(&format!(
            "\n## Tool Visibility\n\n\
             A visibility preset is active: {active}/{total} tools are loaded. \
             Use the `list_available_tools` tool to see all tools grouped by toolset, \
             including hidden tools that can be enabled via the `include` list in the \
             policy config.\n",
            active = visible.len(),
            total = all_tools.len(),
        ));
    }
    let suffix = "\n## Authentication\n\n\
         Credentials are resolved from redisctl profiles in both transport modes.\n\
         HTTP transport does not authenticate clients; keep it on loopback or place it \
         behind a trusted gateway that provides authentication, authorization, and TLS.";
    router = router.auto_instructions_with(Some(prefix), Some(suffix));

    let policy_for_filter = policy.clone();
    let visible_for_filter = Arc::new(visible);
    tracing::info!(tier = %policy.global_tier(), "Applying policy filter");
    Ok(router.tool_filter(
        CapabilityFilter::<Tool>::new(move |_session, tool: &Tool| {
            let name = tool.name.as_str();
            let system = presets::SYSTEM_TOOLS.contains(&name);
            let visible = system || visible_for_filter.contains(name);
            visible && policy_for_filter.is_tool_allowed(tool)
        })
        .denial_behavior(DenialBehavior::Unauthorized),
    ))
}

pub(crate) struct Skill {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) body: String,
}

fn strip_yaml_quotes(value: &str) -> &str {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

pub(crate) fn parse_skill(content: &str) -> Option<Skill> {
    let content = content.trim();
    if !content.starts_with("---") {
        tracing::debug!("skill parse failed: missing opening frontmatter delimiter");
        return None;
    }
    let after_first = &content[3..];
    let end = after_first.find("\n---").map(|index| index + 1)?;
    if end == 0 {
        tracing::debug!("skill parse failed: empty frontmatter");
        return None;
    }
    let frontmatter = &after_first[..end];
    let body = after_first[end + 3..].trim().to_string();

    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(strip_yaml_quotes(value).to_string());
        } else if let Some(value) = line.strip_prefix("description:") {
            description = Some(strip_yaml_quotes(value).to_string());
        }
    }

    if name.is_none() || description.is_none() {
        tracing::debug!(
            "skill parse failed: missing {} field(s)",
            match (name.is_none(), description.is_none()) {
                (true, true) => "name and description",
                (true, false) => "name",
                _ => "description",
            }
        );
    }

    Some(Skill {
        name: name?,
        description: description?,
        body,
    })
}

fn load_skills(directory: &Path, registry: &DynamicPromptRegistry) -> usize {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(
                "Failed to read skills directory {}: {error}",
                directory.display()
            );
            return 0;
        }
    };

    let mut count = 0;
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let skill_file = entry.path().join("SKILL.md");
        if let Ok(content) = std::fs::read_to_string(&skill_file)
            && let Some(skill) = parse_skill(&content)
        {
            tracing::info!(skill = %skill.name, "Loaded skill prompt");
            let body = skill.body.clone();
            let description = skill.description.clone();
            let prompt = PromptBuilder::new(&skill.name)
                .description(&skill.description)
                .handler(move |_arguments| {
                    let body = body.clone();
                    let description = description.clone();
                    async move {
                        Ok(tower_mcp::GetPromptResult::user_message_with_description(
                            body,
                            description,
                        ))
                    }
                })
                .build();
            registry.register(prompt);
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::SafetyTier;
    #[cfg(feature = "enterprise")]
    use crate::policy::ToolsetPolicy;
    use redisctl_core::{DeploymentType, Profile, ProfileCredentials};
    use tower_mcp::TestClient;

    fn policy(tier: SafetyTier) -> PolicyConfig {
        PolicyConfig {
            tier,
            ..Default::default()
        }
    }

    fn profile(deployment_type: DeploymentType) -> Profile {
        let credentials = match deployment_type {
            DeploymentType::Cloud => ProfileCredentials::Cloud {
                api_key: "key".to_string(),
                api_secret: "secret".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            DeploymentType::Enterprise => ProfileCredentials::Enterprise {
                url: "https://localhost:9443".to_string(),
                username: "admin".to_string(),
                password: Some("password".to_string()),
                insecure: false,
                ca_cert: None,
            },
            DeploymentType::Database => ProfileCredentials::Database {
                host: "localhost".to_string(),
                port: 6379,
                password: None,
                tls: false,
                username: "default".to_string(),
                database: 0,
            },
        };
        Profile {
            deployment_type,
            credentials,
            files_api_key: None,
            tags: Vec::new(),
        }
    }

    async fn listed_tool_names(server: McpServer) -> HashSet<String> {
        let mut client = TestClient::from_router(server.into_router());
        client.initialize().await;
        client
            .list_tools()
            .await
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn invalid_tool_specs_are_rejected() {
        let result = McpServerBuilder::new(
            CredentialSource::Profiles(Vec::new()),
            PolicyConfig::default(),
            "test",
        )
        .with_tool_specs(["bogus"]);
        assert!(result.is_err());
    }

    #[test]
    fn app_submodules_are_rejected() {
        let result = McpServerBuilder::new(
            CredentialSource::Profiles(Vec::new()),
            PolicyConfig::default(),
            "test",
        )
        .with_tool_specs(["app:anything"]);
        assert!(result.is_err());
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn explicit_submodule_selection_limits_mapping() {
        let server = McpServerBuilder::new(
            CredentialSource::Profiles(Vec::new()),
            policy(SafetyTier::Full),
            "test",
        )
        .with_tool_specs(["cloud:raw"])
        .unwrap()
        .build()
        .unwrap();
        assert!(server.tool_toolset.contains_key("cloud_raw_api"));
        assert!(!server.tool_toolset.contains_key("list_subscriptions"));
    }

    #[test]
    fn profile_selection_always_keeps_app_tools() {
        let mut config = Config::default();
        config.set_profile("database".to_string(), profile(DeploymentType::Database));
        let server = McpServerBuilder::new(
            CredentialSource::Profiles(Vec::new()),
            PolicyConfig::default(),
            "test",
        )
        .with_profile_toolsets(&config)
        .build()
        .unwrap();
        assert!(server.enabled_toolsets().iter().any(|name| name == "app"));
    }

    #[cfg(feature = "enterprise")]
    #[test]
    fn policy_disabled_toolset_is_not_built_when_explicitly_selected() {
        let policy = PolicyConfig {
            enterprise: Some(ToolsetPolicy {
                enabled: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        let server = McpServerBuilder::new(CredentialSource::Profiles(Vec::new()), policy, "test")
            .with_tool_specs(["enterprise", "app"])
            .unwrap()
            .build()
            .unwrap();
        assert!(
            !server
                .enabled_toolsets()
                .iter()
                .any(|name| name == "enterprise")
        );
    }

    #[tokio::test]
    async fn read_only_builder_filters_write_and_destructive_profile_tools() {
        let names = listed_tool_names(
            McpServerBuilder::new(
                CredentialSource::Profiles(Vec::new()),
                policy(SafetyTier::ReadOnly),
                "test",
            )
            .with_tool_specs(["app"])
            .unwrap()
            .build()
            .unwrap(),
        )
        .await;

        assert!(names.contains("profile_list"));
        assert!(!names.contains("profile_create"));
        assert!(!names.contains("profile_delete"));
        assert!(names.contains("show_policy"));
        assert!(names.contains("list_available_tools"));
    }

    #[tokio::test]
    async fn full_builder_exposes_write_and_destructive_profile_tools() {
        let names = listed_tool_names(
            McpServerBuilder::new(
                CredentialSource::Profiles(Vec::new()),
                policy(SafetyTier::Full),
                "test",
            )
            .with_tool_specs(["app"])
            .unwrap()
            .build()
            .unwrap(),
        )
        .await;

        assert!(names.contains("profile_list"));
        assert!(names.contains("profile_create"));
        assert!(names.contains("profile_delete"));
    }

    #[test]
    fn skill_frontmatter_parser_handles_quotes_and_body_dashes() {
        let skill = parse_skill(
            "---\nname: \"my-skill\"\ndescription: 'quoted desc'\n---\nBody with --- dashes.",
        )
        .unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "quoted desc");
        assert_eq!(skill.body, "Body with --- dashes.");
    }
}
