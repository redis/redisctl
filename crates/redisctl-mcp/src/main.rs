//! redisctl-mcp: MCP server for Redis Cloud and Enterprise
//!
//! A standalone MCP server that exposes Redis management operations
//! as tools for AI systems.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use redisctl_core::Config;
#[cfg(any(feature = "cloud", feature = "enterprise", feature = "database"))]
use redisctl_core::DeploymentType;
use tower_mcp::{
    CapabilityFilter, DenialBehavior, DynamicPromptRegistry, McpRouter, PromptBuilder, Tool,
    transport::StdioTransport,
};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod audit;
mod error;
mod policy;
mod presets;
mod prompts;
mod resources;
mod serde_helpers;
mod state;
mod tools;

use audit::AuditLayer;
use policy::{Policy, PolicyConfig, SafetyTier, ToolsetKind};
use presets::{ToolVisibility, ToolsConfig};
use state::{AppState, CredentialSource};

/// Transport mode for the MCP server
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum Transport {
    /// Standard input/output (for CLI integrations)
    #[default]
    Stdio,
    /// HTTP with Server-Sent Events (for shared deployments)
    Http,
}

/// Toolsets that can be enabled or disabled at runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Toolset {
    /// Redis Cloud management tools
    #[cfg(feature = "cloud")]
    Cloud,
    /// Redis Enterprise management tools
    #[cfg(feature = "enterprise")]
    Enterprise,
    /// Direct Redis database tools
    #[cfg(feature = "database")]
    Database,
    /// App-level tools: profile management, resources, and prompts
    App,
}

impl Toolset {
    /// Parse a toolset name string into a `Toolset` variant.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            #[cfg(feature = "cloud")]
            "cloud" => Some(Toolset::Cloud),
            #[cfg(feature = "enterprise")]
            "enterprise" => Some(Toolset::Enterprise),
            #[cfg(feature = "database")]
            "database" => Some(Toolset::Database),
            "app" => Some(Toolset::App),
            _ => None,
        }
    }

    /// All compiled-in toolset names (for error messages).
    #[allow(clippy::vec_init_then_push)]
    fn all_names() -> Vec<&'static str> {
        let mut names = Vec::new();
        #[cfg(feature = "cloud")]
        names.push("cloud");
        #[cfg(feature = "enterprise")]
        names.push("enterprise");
        #[cfg(feature = "database")]
        names.push("database");
        names.push("app");
        names
    }
}

impl std::fmt::Display for Toolset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "cloud")]
            Toolset::Cloud => write!(f, "cloud"),
            #[cfg(feature = "enterprise")]
            Toolset::Enterprise => write!(f, "enterprise"),
            #[cfg(feature = "database")]
            Toolset::Database => write!(f, "database"),
            Toolset::App => write!(f, "app"),
        }
    }
}

/// Whether a toolset loads all sub-modules or a selected subset.
#[derive(Debug, Clone)]
enum SubModuleSelection {
    /// Load all sub-modules in this toolset.
    All,
    /// Load only the named sub-modules.
    Selected(HashSet<String>),
}

/// Resolved set of enabled toolsets with optional sub-module selection.
#[derive(Debug, Clone)]
struct EnabledToolsets {
    selections: HashMap<Toolset, SubModuleSelection>,
}

impl EnabledToolsets {
    /// Create an `EnabledToolsets` with all sub-modules for each given toolset.
    fn all_of(toolsets: impl IntoIterator<Item = Toolset>) -> Self {
        Self {
            selections: toolsets
                .into_iter()
                .map(|t| (t, SubModuleSelection::All))
                .collect(),
        }
    }

    /// Check whether a toolset is enabled (with any selection).
    #[allow(dead_code)]
    fn contains(&self, toolset: &Toolset) -> bool {
        self.selections.contains_key(toolset)
    }

    /// Get the sub-module selection for a toolset.
    #[allow(dead_code)]
    fn selection(&self, toolset: &Toolset) -> Option<&SubModuleSelection> {
        self.selections.get(toolset)
    }

    /// Remove toolsets that match a predicate.
    fn retain(&mut self, f: impl Fn(&Toolset) -> bool) {
        self.selections.retain(|t, _| f(t));
    }

    /// Iterate over enabled toolsets.
    fn iter(&self) -> impl Iterator<Item = &Toolset> {
        self.selections.keys()
    }
}

/// MCP server for Redis Cloud and Enterprise management
#[derive(Parser, Debug)]
#[command(name = "redisctl-mcp")]
#[command(version, about, long_about = None)]
struct Args {
    /// Transport mode
    #[arg(short, long, value_enum, default_value = "stdio")]
    transport: Transport,

    /// Profile name(s) for local credential resolution. Can be specified multiple times.
    #[arg(short, long, env = "REDISCTL_PROFILE")]
    profile: Vec<String>,

    /// Read-only mode (enabled by default; use --read-only=false to allow writes).
    /// Ignored when a policy file is active.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    read_only: bool,

    /// Path to MCP policy file for granular tool access control.
    /// Overrides --read-only when set.
    #[arg(long, env = "REDISCTL_MCP_POLICY")]
    policy: Option<PathBuf>,

    /// Redis database URL for direct connections
    #[arg(long, env = "REDIS_URL")]
    database_url: Option<String>,

    /// Enable Redis Cluster mode (handles MOVED/ASK redirections)
    #[arg(long, env = "REDIS_CLUSTER")]
    cluster: bool,

    /// Client name for CLIENT SETNAME (identifies MCP connections in CLIENT LIST)
    #[arg(long, env = "REDIS_CLIENT_NAME", default_value = "redisctl-mcp")]
    client_name: Option<String>,

    /// Toolsets to enable (default: all compiled-in).
    /// Use bare names for all sub-modules: cloud,enterprise,database,app.
    /// Use colon syntax for specific sub-modules: cloud:subscriptions,cloud:networking.
    #[arg(long, value_delimiter = ',')]
    tools: Option<Vec<String>>,

    // --- HTTP transport options ---
    /// Host to bind HTTP server
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind HTTP server
    #[arg(long, default_value = "8080")]
    port: u16,

    // --- OAuth options (HTTP mode) ---
    /// Enable OAuth authentication for HTTP transport
    #[arg(long)]
    oauth: bool,

    /// OAuth issuer URL (e.g., https://accounts.google.com)
    #[arg(long, env = "OAUTH_ISSUER")]
    oauth_issuer: Option<String>,

    /// OAuth audience (client ID or API identifier)
    #[arg(long, env = "OAUTH_AUDIENCE")]
    oauth_audience: Option<String>,

    /// JWKS URI for token validation (auto-discovered from issuer if not set)
    #[arg(long, env = "OAUTH_JWKS_URI")]
    jwks_uri: Option<String>,

    // --- Rate limiting ---
    /// Maximum concurrent requests
    #[arg(long, default_value = "10")]
    max_concurrent: usize,

    /// Request timeout in seconds (HTTP mode)
    #[arg(long, default_value = "30")]
    request_timeout_secs: u64,

    // --- Skills ---
    /// Directory containing SKILL.md files to load as MCP prompts.
    /// Each subdirectory should contain a SKILL.md with YAML frontmatter.
    #[arg(long, env = "REDISCTL_MCP_SKILLS_DIR")]
    skills_dir: Option<PathBuf>,

    // --- Logging ---
    /// Log level
    #[arg(long, default_value = "info", env = "RUST_LOG")]
    log_level: String,
}

/// Parse `--tools` specs like `["cloud:subscriptions", "cloud:networking", "enterprise", "app"]`
/// into an `EnabledToolsets`.
///
/// Rules:
/// - Bare name (e.g. `cloud`) selects all sub-modules for that toolset.
/// - Colon syntax (e.g. `cloud:subscriptions`) selects a single sub-module.
/// - If both bare and colon forms appear for the same toolset, bare wins (all sub-modules).
/// - `app` has no sub-modules; `app:anything` is an error.
fn parse_tool_specs(specs: &[String]) -> Result<EnabledToolsets> {
    let mut selections: HashMap<Toolset, SubModuleSelection> = HashMap::new();

    for spec in specs {
        if let Some((toolset_name, sub_name)) = spec.split_once(':') {
            let toolset = Toolset::from_str(toolset_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown toolset '{}'. Valid toolsets: {}",
                    toolset_name,
                    Toolset::all_names().join(", ")
                )
            })?;

            // app has no sub-modules
            if matches!(toolset, Toolset::App) {
                bail!("'app' has no sub-modules (got 'app:{}')", sub_name);
            }

            // Validate sub-module name
            if !is_valid_sub_module(&toolset, sub_name) {
                bail!(
                    "Unknown sub-module '{}' for toolset '{}'. Valid sub-modules: {}",
                    sub_name,
                    toolset,
                    valid_sub_module_names(&toolset).join(", ")
                );
            }

            match selections.get_mut(&toolset) {
                Some(SubModuleSelection::All) => {
                    // Bare already seen, keep All
                }
                Some(SubModuleSelection::Selected(set)) => {
                    set.insert(sub_name.to_string());
                }
                None => {
                    let mut set = HashSet::new();
                    set.insert(sub_name.to_string());
                    selections.insert(toolset, SubModuleSelection::Selected(set));
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
            // Bare name: select all sub-modules (overrides any previous selective)
            selections.insert(toolset, SubModuleSelection::All);
        }
    }

    Ok(EnabledToolsets { selections })
}

/// Check whether a sub-module name is valid for a given toolset.
#[allow(unused_variables)]
fn is_valid_sub_module(toolset: &Toolset, name: &str) -> bool {
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

/// Return valid sub-module names for a toolset (for error messages).
fn valid_sub_module_names(toolset: &Toolset) -> Vec<&'static str> {
    match toolset {
        #[cfg(feature = "cloud")]
        Toolset::Cloud => tools::cloud::SUB_MODULES.iter().map(|sm| sm.name).collect(),
        #[cfg(feature = "enterprise")]
        Toolset::Enterprise => tools::enterprise::SUB_MODULES
            .iter()
            .map(|sm| sm.name)
            .collect(),
        #[cfg(feature = "database")]
        Toolset::Database => tools::redis::SUB_MODULES.iter().map(|sm| sm.name).collect(),
        Toolset::App => vec![],
    }
}

/// Get tool names for a toolset according to its sub-module selection.
fn selected_tool_names(toolset: &Toolset, selection: &SubModuleSelection) -> Vec<String> {
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
        SubModuleSelection::Selected(sub_modules) => {
            let mut names = Vec::new();
            for _sub in sub_modules {
                let sub_names: Option<&[&str]> = match toolset {
                    #[cfg(feature = "cloud")]
                    Toolset::Cloud => tools::cloud::sub_tool_names(_sub),
                    #[cfg(feature = "enterprise")]
                    Toolset::Enterprise => tools::enterprise::sub_tool_names(_sub),
                    #[cfg(feature = "database")]
                    Toolset::Database => tools::redis::sub_tool_names(_sub),
                    Toolset::App => None,
                };
                if let Some(tool_names) = sub_names {
                    names.extend(tool_names.iter().map(|s| (*s).to_string()));
                }
            }
            names
        }
    }
}

/// Derive which toolsets to enable based on profile types in the config.
/// Returns `None` if config has no profiles (caller should fall back to all compiled-in).
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

/// Try to auto-detect toolsets from the config file on disk.
/// Returns `None` if config cannot be loaded or has no profiles.
fn detect_toolsets_from_config() -> Option<EnabledToolsets> {
    let config = Config::load().ok()?;
    let result = toolsets_from_config(&config);
    if let Some(ref enabled) = result {
        let names: Vec<String> = enabled.iter().map(|t| t.to_string()).collect();
        info!(toolsets = ?names, "Auto-detected toolsets from config profiles");
    }
    result
}

/// Resolve which toolsets are enabled based on CLI args, config profiles, and compiled features.
///
/// Priority: explicit `--tools` flag > config-based auto-detection > all compiled-in features.
fn resolve_enabled_toolsets(args: &Args) -> Result<EnabledToolsets> {
    // 1. Explicit --tools flag always wins
    if let Some(ref tools) = args.tools {
        return parse_tool_specs(tools);
    }

    // 2. Auto-detect from config profiles
    if let Some(toolsets) = detect_toolsets_from_config() {
        return Ok(toolsets);
    }

    // 3. Fallback: all compiled-in features
    #[allow(unused_mut)]
    let mut all = vec![Toolset::App];
    #[cfg(feature = "cloud")]
    all.push(Toolset::Cloud);
    #[cfg(feature = "enterprise")]
    all.push(Toolset::Enterprise);
    #[cfg(feature = "database")]
    all.push(Toolset::Database);
    Ok(EnabledToolsets::all_of(all))
}

/// Map a CLI `Toolset` to its corresponding `ToolsetKind` for policy lookup.
fn toolset_to_kind(t: &Toolset) -> ToolsetKind {
    match t {
        #[cfg(feature = "cloud")]
        Toolset::Cloud => ToolsetKind::Cloud,
        #[cfg(feature = "enterprise")]
        Toolset::Enterprise => ToolsetKind::Enterprise,
        #[cfg(feature = "database")]
        Toolset::Database => ToolsetKind::Database,
        Toolset::App => ToolsetKind::App,
    }
}

/// Resolve the policy configuration.
///
/// If a policy file is found (via `--policy`, env var, or default path), it takes precedence
/// and `--read-only` is ignored. Otherwise, synthesize a policy from the `--read-only` flag.
fn resolve_policy(args: &Args) -> Result<(PolicyConfig, String)> {
    let has_explicit_policy = args.policy.is_some();
    let has_env_policy = std::env::var("REDISCTL_MCP_POLICY").is_ok() && args.policy.is_none();
    let has_default_policy = PolicyConfig::default_path_exists();

    resolve_policy_with_source_flags(
        args,
        has_explicit_policy || has_env_policy,
        has_default_policy,
    )
}

fn resolve_policy_with_source_flags(
    args: &Args,
    has_explicit_or_env_policy: bool,
    has_default_policy: bool,
) -> Result<(PolicyConfig, String)> {
    if has_explicit_or_env_policy || has_default_policy {
        let (config, source) = PolicyConfig::load(args.policy.as_deref())?;
        if !args.read_only {
            tracing::warn!(
                "--read-only=false is ignored when a policy file is active (source: {})",
                source
            );
        }
        return Ok((config, source));
    }

    // No policy file: synthesize from --read-only flag
    let tier = if args.read_only {
        SafetyTier::ReadOnly
    } else {
        SafetyTier::Full
    };
    let source = if args.read_only {
        "cli: --read-only=true (default)".to_string()
    } else {
        "cli: --read-only=false".to_string()
    };

    let mut config = PolicyConfig::synthesized_default();
    config.tier = tier;

    Ok((config, source))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut enabled = resolve_enabled_toolsets(&args)?;

    // Resolve policy configuration (includes audit config)
    let (policy_config, policy_source) = resolve_policy(&args)?;

    // Apply policy-based toolset disabling (only when --tools was not explicit)
    if args.tools.is_none() {
        let disabled = policy_config.disabled_toolsets();
        if !disabled.is_empty() {
            enabled.retain(|t| !disabled.contains(&toolset_to_kind(t)));
        }
    }

    let enabled_names: Vec<String> = enabled.iter().map(|t| t.to_string()).collect();
    let audit_config = Arc::new(policy_config.audit.clone());

    // Initialize tracing with optional audit layer
    // App logs: human-readable text to stderr (excludes audit target)
    // Audit logs: JSON to stderr (only audit target, when enabled)
    init_tracing(&args.log_level, audit_config.enabled);

    info!(
        transport = ?args.transport,
        profiles = ?args.profile,
        policy_tier = %policy_config.tier,
        policy_source = %policy_source,
        audit_enabled = audit_config.enabled,
        toolsets = ?enabled_names,
        "Starting redisctl-mcp server"
    );

    // Determine credential source
    let credential_source = if args.oauth {
        CredentialSource::OAuth {
            issuer: args.oauth_issuer.clone(),
            audience: args.oauth_audience.clone(),
        }
    } else {
        CredentialSource::Profiles(args.profile.clone())
    };

    // Build tool-to-toolset mapping for policy evaluation
    let tool_toolset = build_tool_toolset_mapping(&enabled);

    // Extract tools visibility config before consuming policy_config
    let tools_config = policy_config.tools.clone();

    // Build resolved policy (consumes a clone of the mapping)
    let policy = Arc::new(Policy::new(
        policy_config,
        tool_toolset.clone(),
        policy_source,
    ));

    // Build application state
    let state = Arc::new(AppState::new(
        credential_source,
        policy.clone(),
        args.database_url.clone(),
        args.cluster,
        args.client_name.clone(),
    )?);

    // Resolve skills directory
    let skills_dir = resolve_skills_dir(&args);

    // Build router with tools and policy-based filter
    let router = build_router(
        state.clone(),
        policy,
        &enabled,
        tools_config,
        &tool_toolset,
        skills_dir.as_deref(),
    )?;

    // Wrap mapping in Arc for shared use (after last borrow)
    let tool_toolset_arc = Arc::new(tool_toolset);

    match args.transport {
        Transport::Stdio => {
            info!("Running with stdio transport");
            if audit_config.enabled {
                info!("Audit logging enabled (level: {:?})", audit_config.level);
                StdioTransport::new(router)
                    .layer(AuditLayer::new(audit_config, tool_toolset_arc))
                    .run()
                    .await?;
            } else {
                StdioTransport::new(router).run().await?;
            }
        }
        Transport::Http => {
            info!(host = %args.host, port = args.port, "Running with HTTP transport");
            run_http_server(router, &args, audit_config, tool_toolset_arc).await?;
        }
    }

    Ok(())
}

/// Initialize the tracing subscriber with optional audit logging layer.
///
/// When audit is enabled, adds a second JSON-formatted layer that captures only
/// events with `target = "audit"`. The app layer excludes audit events to avoid
/// double-logging.
fn init_tracing(log_level: &str, audit_enabled: bool) {
    use tracing_subscriber::filter;

    if audit_enabled {
        // Dual-layer: app (text, no audit) + audit (JSON, only audit)
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| log_level.to_string().into());

        let app_layer = fmt::layer().with_writer(std::io::stderr).with_filter(
            filter::Targets::new()
                .with_default(tracing::Level::TRACE)
                .with_target("audit", filter::LevelFilter::OFF),
        );

        let audit_layer = fmt::layer()
            .json()
            .with_writer(std::io::stderr)
            .with_target(true)
            .with_filter(filter::Targets::new().with_target("audit", tracing::Level::INFO));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(app_layer)
            .with(audit_layer)
            .init();
    } else {
        // Single layer: standard app logs
        tracing_subscriber::registry()
            .with(fmt::layer().with_writer(std::io::stderr))
            .with(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| log_level.to_string().into()),
            )
            .init();
    }
}

/// A parsed skill from a SKILL.md file.
struct Skill {
    name: String,
    description: String,
    body: String,
}

/// Strip surrounding quotes (single or double) from a YAML value.
fn strip_yaml_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Parse a SKILL.md file with YAML frontmatter.
fn parse_skill(content: &str) -> Option<Skill> {
    let content = content.trim();
    if !content.starts_with("---") {
        tracing::debug!("skill parse failed: missing opening frontmatter delimiter");
        return None;
    }
    let after_first = &content[3..];
    // Match `---` only at the start of a line to avoid matching inside YAML values
    let end = after_first.find("\n---").map(|i| i + 1)?;
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
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(strip_yaml_quotes(val).to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            description = Some(strip_yaml_quotes(val).to_string());
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

/// Load skills from a directory and register them as dynamic prompts.
fn load_skills(dir: &std::path::Path, registry: &DynamicPromptRegistry) -> usize {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("Failed to read skills directory {}: {e}", dir.display());
            return 0;
        }
    };

    let mut count = 0;
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let skill_file = entry.path().join("SKILL.md");
        if let Ok(content) = std::fs::read_to_string(&skill_file)
            && let Some(skill) = parse_skill(&content)
        {
            info!(skill = %skill.name, "Loaded skill prompt");
            let body = skill.body.clone();
            let description = skill.description.clone();
            let prompt = PromptBuilder::new(&skill.name)
                .description(&skill.description)
                .handler(move |_args| {
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

/// Resolve the skills directory: explicit flag > bundled skills.
fn resolve_skills_dir(args: &Args) -> Option<PathBuf> {
    if let Some(ref dir) = args.skills_dir {
        if dir.is_dir() {
            return Some(dir.clone());
        }
        tracing::warn!("Skills directory not found: {}", dir.display());
        return None;
    }

    // Fall back to bundled skills next to the binary
    if let Ok(exe) = std::env::current_exe() {
        let bundled = exe
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("skills");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }

    None
}

/// Build the tool name -> toolset kind mapping for policy evaluation.
fn build_tool_toolset_mapping(enabled: &EnabledToolsets) -> HashMap<String, ToolsetKind> {
    let mut mapping = HashMap::new();

    for (toolset, selection) in &enabled.selections {
        let kind = toolset_to_kind(toolset);
        for name in selected_tool_names(toolset, selection) {
            mapping.insert(name, kind);
        }
    }

    mapping
}

/// Merge a single toolset's router(s) into the main router, respecting sub-module selection.
fn merge_toolset_router(
    router: McpRouter,
    toolset: &Toolset,
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
        SubModuleSelection::Selected(sub_modules) => {
            let mut r = router;
            for _sub in sub_modules {
                let sub_router: Option<McpRouter> = match toolset {
                    #[cfg(feature = "cloud")]
                    Toolset::Cloud => tools::cloud::sub_router(_sub, state.clone()),
                    #[cfg(feature = "enterprise")]
                    Toolset::Enterprise => tools::enterprise::sub_router(_sub, state.clone()),
                    #[cfg(feature = "database")]
                    Toolset::Database => tools::redis::sub_router(_sub, state.clone()),
                    Toolset::App => None,
                };
                if let Some(sr) = sub_router {
                    r = r.merge(sr);
                }
            }
            r
        }
    }
}

/// Build the MCP router with modular sub-routers based on enabled toolsets
fn build_router(
    state: Arc<AppState>,
    policy: Arc<Policy>,
    enabled: &EnabledToolsets,
    tools_config: ToolsConfig,
    tool_toolset: &HashMap<String, ToolsetKind>,
    skills_dir: Option<&std::path::Path>,
) -> Result<McpRouter> {
    let router = McpRouter::new().server_info("redisctl-mcp", env!("CARGO_PKG_VERSION"));

    // Enable dynamic prompts for skill loading
    let (mut router, prompt_registry) = router.with_dynamic_prompts();

    // Merge toolsets, respecting sub-module selection
    for (toolset, selection) in &enabled.selections {
        router = merge_toolset_router(router, toolset, selection, state.clone());
    }

    // Register built-in prompts
    prompt_registry.register(crate::prompts::troubleshoot_database_prompt());
    prompt_registry.register(crate::prompts::analyze_performance_prompt());
    prompt_registry.register(crate::prompts::capacity_planning_prompt());
    prompt_registry.register(crate::prompts::migration_planning_prompt());

    // Load skills as dynamic prompts
    if let Some(dir) = skills_dir {
        let count = load_skills(dir, &prompt_registry);
        if count > 0 {
            info!(count, dir = %dir.display(), "Loaded skill prompts");
        }
    }

    // Register the show_policy tool (always available, bypasses visibility)
    router = router.tool(policy::show_policy_tool(policy.clone()));

    // Resolve tool visibility from preset config
    let all_tools: HashSet<String> = tool_toolset.keys().cloned().collect();
    let visible_set = presets::resolve_visible_tools(&tools_config, &all_tools, tool_toolset);
    let is_preset_active = !tools_config.is_all();

    if is_preset_active {
        info!(
            preset = %tools_config.preset,
            active = visible_set.len(),
            total = all_tools.len(),
            "Tool visibility preset active"
        );
    }

    // Register list_available_tools tool (always available, bypasses visibility)
    let visibility = Arc::new(ToolVisibility {
        visible: visible_set.clone(),
        all_tools: tool_toolset.clone(),
        config: tools_config,
    });
    router = router.tool(presets::list_available_tools_tool(visibility));

    // Build instructions with policy description
    let mut prefix = format!(
        "# Redis Cloud and Enterprise MCP Server\n\n## Safety Model\n\n{}\n",
        policy.describe()
    );

    if is_preset_active {
        prefix.push_str(&format!(
            "\n## Tool Visibility\n\n\
             A visibility preset is active: {active}/{total} tools are loaded. \
             Use the `list_available_tools` tool to see all tools grouped by toolset, \
             including hidden tools that can be enabled via the `include` list in the \
             policy config.\n",
            active = visible_set.len(),
            total = all_tools.len(),
        ));
    }

    let suffix = "\n## Authentication\n\n\
         In stdio mode, credentials are resolved from redisctl profiles.\n\
         In HTTP mode with OAuth, credentials can be passed via JWT claims.";

    router = router.auto_instructions_with(Some(prefix), Some(suffix));

    // Apply combined visibility + policy filter
    // System tools (show_policy, list_available_tools) bypass visibility.
    // All other tools must pass both visibility and policy checks.
    let policy_for_filter = policy.clone();
    let visible_for_filter = Arc::new(visible_set);
    info!(tier = %policy.global_tier(), "Applying policy filter");
    let router = router.tool_filter(
        CapabilityFilter::<Tool>::new(move |_session, tool: &Tool| {
            let name = tool.name.as_str();
            let is_system = presets::SYSTEM_TOOLS.contains(&name);
            let is_visible = is_system || visible_for_filter.contains(name);
            is_visible && policy_for_filter.is_tool_allowed(tool)
        })
        .denial_behavior(DenialBehavior::Unauthorized),
    );

    Ok(router)
}

/// Run the HTTP server with middleware
#[cfg(feature = "http")]
async fn run_http_server(
    router: McpRouter,
    args: &Args,
    audit_config: Arc<audit::AuditConfig>,
    tool_toolset: Arc<HashMap<String, ToolsetKind>>,
) -> Result<()> {
    use std::time::Duration;
    use tower::limit::ConcurrencyLimitLayer;
    use tower::timeout::TimeoutLayer;
    use tower_mcp::HttpTransport;

    let addr = format!("{}:{}", args.host, args.port);

    let mut transport = HttpTransport::new(router)
        .layer(TimeoutLayer::new(Duration::from_secs(
            args.request_timeout_secs,
        )))
        .layer(ConcurrencyLimitLayer::new(args.max_concurrent));

    if audit_config.enabled {
        info!(
            "Audit logging enabled for HTTP transport (level: {:?})",
            audit_config.level
        );
        transport = transport.layer(AuditLayer::new(audit_config, tool_toolset));
    }

    if args.oauth {
        // OAuth-enabled HTTP transport
        bail!(
            "OAuth support is not yet implemented. Remove --oauth to start the server without authentication."
        );
    }

    transport.serve(&addr).await?;

    Ok(())
}

#[cfg(not(feature = "http"))]
async fn run_http_server(
    _router: McpRouter,
    _args: &Args,
    _audit_config: Arc<audit::AuditConfig>,
    _tool_toolset: Arc<HashMap<String, ToolsetKind>>,
) -> Result<()> {
    anyhow::bail!("HTTP transport requires the 'http' feature")
}

#[cfg(test)]
mod tests {
    use super::*;
    use redisctl_core::{Profile, ProfileCredentials};

    fn cloud_profile() -> Profile {
        Profile {
            deployment_type: DeploymentType::Cloud,
            credentials: ProfileCredentials::Cloud {
                api_key: "key".to_string(),
                api_secret: "secret".to_string(),
                api_url: "https://api.redislabs.com/v1".to_string(),
            },
            files_api_key: None,
            tags: vec![],
        }
    }

    fn enterprise_profile() -> Profile {
        Profile {
            deployment_type: DeploymentType::Enterprise,
            credentials: ProfileCredentials::Enterprise {
                url: "https://localhost:9443".to_string(),
                username: "admin".to_string(),
                password: Some("password".to_string()),
                insecure: false,
                ca_cert: None,
            },
            files_api_key: None,
            tags: vec![],
        }
    }

    fn database_profile() -> Profile {
        Profile {
            deployment_type: DeploymentType::Database,
            credentials: ProfileCredentials::Database {
                host: "localhost".to_string(),
                port: 6379,
                password: None,
                tls: false,
                username: "default".to_string(),
                database: 0,
            },
            files_api_key: None,
            tags: vec![],
        }
    }

    fn test_state() -> Arc<AppState> {
        Arc::new(
            AppState::new(
                state::CredentialSource::Profiles(vec![]),
                AppState::test_policy(),
                None,
                false,
                None,
            )
            .unwrap(),
        )
    }

    fn test_policy_arc(tier: SafetyTier) -> Arc<Policy> {
        Arc::new(Policy::new(
            PolicyConfig {
                tier,
                ..Default::default()
            },
            HashMap::new(),
            "test".to_string(),
        ))
    }

    #[test]
    fn empty_config_returns_none() {
        let config = Config::default();
        assert!(toolsets_from_config(&config).is_none());
    }

    #[test]
    fn cloud_only_profiles() {
        let mut config = Config::default();
        config.set_profile("mycloud".to_string(), cloud_profile());

        let toolsets = toolsets_from_config(&config).unwrap();
        assert!(toolsets.contains(&Toolset::App));
        #[cfg(feature = "cloud")]
        assert!(toolsets.contains(&Toolset::Cloud));
        #[cfg(feature = "enterprise")]
        assert!(!toolsets.contains(&Toolset::Enterprise));
        #[cfg(feature = "database")]
        assert!(!toolsets.contains(&Toolset::Database));
    }

    #[test]
    fn enterprise_only_profiles() {
        let mut config = Config::default();
        config.set_profile("myent".to_string(), enterprise_profile());

        let toolsets = toolsets_from_config(&config).unwrap();
        assert!(toolsets.contains(&Toolset::App));
        #[cfg(feature = "cloud")]
        assert!(!toolsets.contains(&Toolset::Cloud));
        #[cfg(feature = "enterprise")]
        assert!(toolsets.contains(&Toolset::Enterprise));
        #[cfg(feature = "database")]
        assert!(!toolsets.contains(&Toolset::Database));
    }

    #[test]
    fn cloud_and_enterprise_profiles() {
        let mut config = Config::default();
        config.set_profile("mycloud".to_string(), cloud_profile());
        config.set_profile("myent".to_string(), enterprise_profile());

        let toolsets = toolsets_from_config(&config).unwrap();
        assert!(toolsets.contains(&Toolset::App));
        #[cfg(feature = "cloud")]
        assert!(toolsets.contains(&Toolset::Cloud));
        #[cfg(feature = "enterprise")]
        assert!(toolsets.contains(&Toolset::Enterprise));
        #[cfg(feature = "database")]
        assert!(!toolsets.contains(&Toolset::Database));
    }

    #[test]
    fn database_only_profiles() {
        let mut config = Config::default();
        config.set_profile("mydb".to_string(), database_profile());

        let toolsets = toolsets_from_config(&config).unwrap();
        assert!(toolsets.contains(&Toolset::App));
        #[cfg(feature = "cloud")]
        assert!(!toolsets.contains(&Toolset::Cloud));
        #[cfg(feature = "enterprise")]
        assert!(!toolsets.contains(&Toolset::Enterprise));
        #[cfg(feature = "database")]
        assert!(toolsets.contains(&Toolset::Database));
    }

    #[test]
    fn all_three_profile_types() {
        let mut config = Config::default();
        config.set_profile("mycloud".to_string(), cloud_profile());
        config.set_profile("myent".to_string(), enterprise_profile());
        config.set_profile("mydb".to_string(), database_profile());

        let toolsets = toolsets_from_config(&config).unwrap();
        assert!(toolsets.contains(&Toolset::App));
        #[cfg(feature = "cloud")]
        assert!(toolsets.contains(&Toolset::Cloud));
        #[cfg(feature = "enterprise")]
        assert!(toolsets.contains(&Toolset::Enterprise));
        #[cfg(feature = "database")]
        assert!(toolsets.contains(&Toolset::Database));
    }

    #[test]
    fn synthesized_cli_policy_keeps_raw_tool_denies_at_full_tier() {
        let args = Args::parse_from(["redisctl-mcp", "--read-only=false"]);

        let (config, source) = resolve_policy_with_source_flags(&args, false, false).unwrap();

        assert_eq!(config.tier, SafetyTier::Full);
        assert_eq!(source, "cli: --read-only=false");
        assert!(config.deny.contains(&"cloud_raw_api".to_string()));
        assert!(config.deny.contains(&"enterprise_raw_api".to_string()));
        assert!(config.deny.contains(&"redis_command".to_string()));
    }

    // ========================================================================
    // Safety annotation tests
    // ========================================================================

    /// Helper: assert a tool has read-only, idempotent, non-destructive annotations
    fn assert_read_only(tool: &Tool, name: &str) {
        let ann = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: missing annotations"));
        assert!(ann.read_only_hint, "{name}: should be read_only");
        assert!(ann.idempotent_hint, "{name}: should be idempotent");
        assert!(
            !ann.destructive_hint,
            "{name}: should be non-destructive (destructive_hint=false)"
        );
    }

    /// Helper: assert a tool is a non-destructive write (not read-only, destructive_hint=false)
    fn assert_non_destructive_write(tool: &Tool, name: &str) {
        let ann = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: missing annotations"));
        assert!(!ann.read_only_hint, "{name}: should NOT be read_only");
        assert!(
            !ann.destructive_hint,
            "{name}: should be non-destructive (destructive_hint=false)"
        );
    }

    /// Helper: assert a tool is destructive (explicit annotations with destructive_hint=true)
    fn assert_destructive(tool: &Tool, name: &str) {
        let ann = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: missing annotations"));
        assert!(
            ann.destructive_hint,
            "{name}: should be destructive (destructive_hint=true)"
        );
        assert!(
            !ann.read_only_hint,
            "{name}: destructive tool should NOT be read_only"
        );

        // Description should start with "DANGEROUS:"
        let desc = tool
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("{name}: missing description"));
        assert!(
            desc.starts_with("DANGEROUS:"),
            "{name}: destructive tool description should start with 'DANGEROUS:', got: {desc}"
        );
    }

    // -- Profile tools --

    #[test]
    fn profile_read_tools_are_read_only() {
        let state = test_state();
        assert_read_only(
            &tools::profile::list_profiles(state.clone()),
            "profile_list",
        );
        assert_read_only(&tools::profile::show_profile(state.clone()), "profile_show");
        assert_read_only(&tools::profile::config_path(state.clone()), "profile_path");
        assert_read_only(
            &tools::profile::validate_config(state.clone()),
            "profile_validate",
        );
    }

    #[test]
    fn profile_write_tools_are_non_destructive() {
        let state = test_state();
        assert_non_destructive_write(
            &tools::profile::create_profile(state.clone()),
            "profile_create",
        );
        assert_non_destructive_write(
            &tools::profile::set_default_cloud(state.clone()),
            "profile_set_default_cloud",
        );
        assert_non_destructive_write(
            &tools::profile::set_default_enterprise(state.clone()),
            "profile_set_default_enterprise",
        );
    }

    #[test]
    fn profile_destructive_tools() {
        let state = test_state();
        assert_destructive(
            &tools::profile::delete_profile(state.clone()),
            "profile_delete",
        );
    }

    // -- Cloud tools (representative samples) --

    #[cfg(feature = "cloud")]
    mod cloud_annotations {
        use super::*;

        #[test]
        fn cloud_read_tools_are_read_only() {
            let state = test_state();
            assert_read_only(
                &tools::cloud::list_subscriptions(state.clone()),
                "list_subscriptions",
            );
            assert_read_only(&tools::cloud::get_account(state.clone()), "get_account");
            assert_read_only(
                &tools::cloud::list_fixed_subscriptions(state.clone()),
                "list_fixed_subscriptions",
            );
            assert_read_only(
                &tools::cloud::get_vpc_peering(state.clone()),
                "get_vpc_peering",
            );
            assert_read_only(
                &tools::cloud::wait_for_cloud_task(state.clone()),
                "wait_for_cloud_task",
            );
        }

        #[test]
        fn cloud_write_tools_are_non_destructive() {
            let state = test_state();
            assert_non_destructive_write(
                &tools::cloud::create_database(state.clone()),
                "create_database",
            );
            assert_non_destructive_write(
                &tools::cloud::update_database(state.clone()),
                "update_database",
            );
            assert_non_destructive_write(
                &tools::cloud::backup_database(state.clone()),
                "backup_database",
            );
            assert_non_destructive_write(
                &tools::cloud::update_account_user(state.clone()),
                "update_account_user",
            );
            assert_non_destructive_write(
                &tools::cloud::create_acl_user(state.clone()),
                "create_acl_user",
            );
            assert_non_destructive_write(
                &tools::cloud::create_vpc_peering(state.clone()),
                "create_vpc_peering",
            );
            assert_non_destructive_write(
                &tools::cloud::create_fixed_database(state.clone()),
                "create_fixed_database",
            );
        }

        #[test]
        fn cloud_raw_api_is_destructive() {
            let state = test_state();
            assert_destructive(&tools::cloud::cloud_raw_api(state.clone()), "cloud_raw_api");
        }

        #[test]
        fn cloud_destructive_tools() {
            let state = test_state();
            assert_destructive(
                &tools::cloud::delete_database(state.clone()),
                "delete_database",
            );
            assert_destructive(
                &tools::cloud::delete_subscription(state.clone()),
                "delete_subscription",
            );
            assert_destructive(
                &tools::cloud::flush_database(state.clone()),
                "flush_database",
            );
            assert_destructive(
                &tools::cloud::flush_crdb_database(state.clone()),
                "flush_crdb_database",
            );
            assert_destructive(
                &tools::cloud::delete_account_user(state.clone()),
                "delete_account_user",
            );
            assert_destructive(
                &tools::cloud::delete_acl_user(state.clone()),
                "delete_acl_user",
            );
            assert_destructive(
                &tools::cloud::delete_cloud_account(state.clone()),
                "delete_cloud_account",
            );
            assert_destructive(
                &tools::cloud::delete_vpc_peering(state.clone()),
                "delete_vpc_peering",
            );
            assert_destructive(
                &tools::cloud::delete_private_link(state.clone()),
                "delete_private_link",
            );
            assert_destructive(
                &tools::cloud::delete_fixed_database(state.clone()),
                "delete_fixed_database",
            );
            assert_destructive(
                &tools::cloud::delete_fixed_subscription(state.clone()),
                "delete_fixed_subscription",
            );
        }
    }

    // -- Enterprise tools (representative samples) --

    #[cfg(feature = "enterprise")]
    mod enterprise_annotations {
        use super::*;

        #[test]
        fn enterprise_read_tools_are_read_only() {
            let state = test_state();
            assert_read_only(
                &tools::enterprise::get_cluster(state.clone()),
                "get_cluster",
            );
            assert_read_only(
                &tools::enterprise::list_databases(state.clone()),
                "list_enterprise_databases",
            );
            assert_read_only(
                &tools::enterprise::list_users(state.clone()),
                "list_enterprise_users",
            );
            assert_read_only(
                &tools::enterprise::list_alerts(state.clone()),
                "list_alerts",
            );
        }

        #[test]
        fn enterprise_write_tools_are_non_destructive() {
            let state = test_state();
            assert_non_destructive_write(
                &tools::enterprise::update_cluster(state.clone()),
                "update_enterprise_cluster",
            );
            assert_non_destructive_write(
                &tools::enterprise::create_enterprise_database(state.clone()),
                "create_enterprise_database",
            );
            assert_non_destructive_write(
                &tools::enterprise::create_enterprise_user(state.clone()),
                "create_enterprise_user",
            );
        }

        #[test]
        fn enterprise_raw_api_is_destructive() {
            let state = test_state();
            assert_destructive(
                &tools::enterprise::enterprise_raw_api(state.clone()),
                "enterprise_raw_api",
            );
        }

        #[test]
        fn enterprise_destructive_tools() {
            let state = test_state();
            assert_destructive(
                &tools::enterprise::delete_enterprise_database(state.clone()),
                "delete_enterprise_database",
            );
            assert_destructive(
                &tools::enterprise::flush_enterprise_database(state.clone()),
                "flush_enterprise_database",
            );
            assert_destructive(
                &tools::enterprise::delete_enterprise_user(state.clone()),
                "delete_enterprise_user",
            );
            assert_destructive(
                &tools::enterprise::delete_enterprise_role(state.clone()),
                "delete_enterprise_role",
            );
            assert_destructive(
                &tools::enterprise::delete_enterprise_acl(state.clone()),
                "delete_enterprise_acl",
            );
        }
    }

    // -- Redis database tools (representative samples) --

    #[cfg(feature = "database")]
    mod database_annotations {
        use super::*;

        #[test]
        fn redis_read_tools_are_read_only() {
            let state = test_state();
            assert_read_only(&tools::redis::ping(state.clone()), "redis_ping");
            assert_read_only(&tools::redis::info(state.clone()), "redis_info");
            assert_read_only(&tools::redis::keys(state.clone()), "redis_keys");
            assert_read_only(&tools::redis::get(state.clone()), "redis_get");
            assert_read_only(&tools::redis::hgetall(state.clone()), "redis_hgetall");
            assert_read_only(
                &tools::redis::health_check(state.clone()),
                "redis_health_check",
            );
        }

        #[test]
        fn redis_write_tools_are_non_destructive() {
            let state = test_state();
            assert_non_destructive_write(
                &tools::redis::config_set(state.clone()),
                "redis_config_set",
            );
            assert_non_destructive_write(&tools::redis::set(state.clone()), "redis_set");
            assert_non_destructive_write(&tools::redis::expire(state.clone()), "redis_expire");
            assert_non_destructive_write(&tools::redis::hset(state.clone()), "redis_hset");
            assert_non_destructive_write(&tools::redis::lpush(state.clone()), "redis_lpush");
            assert_non_destructive_write(&tools::redis::xadd(state.clone()), "redis_xadd");
        }

        #[test]
        fn redis_command_is_destructive() {
            let state = test_state();
            assert_destructive(&tools::redis::redis_command(state.clone()), "redis_command");
        }

        #[test]
        fn redis_destructive_tools() {
            let state = test_state();
            assert_destructive(&tools::redis::flushdb(state.clone()), "redis_flushdb");
            assert_destructive(&tools::redis::del(state.clone()), "redis_del");
        }
    }

    #[test]
    fn instructions_contain_safety_model() {
        let state = test_state();
        let enabled = EnabledToolsets::all_of([Toolset::App]);
        let tool_toolset = build_tool_toolset_mapping(&enabled);

        let policy_ro = test_policy_arc(SafetyTier::ReadOnly);
        let _router = build_router(
            state.clone(),
            policy_ro,
            &enabled,
            ToolsConfig::default(),
            &tool_toolset,
            None,
        )
        .unwrap();
        // Verify the build succeeds (no panics) for both modes.
        let policy_full = test_policy_arc(SafetyTier::Full);
        let _router_write = build_router(
            state.clone(),
            policy_full,
            &enabled,
            ToolsConfig::default(),
            &tool_toolset,
            None,
        )
        .unwrap();
    }

    #[test]
    fn show_policy_tool_is_always_registered() {
        let state = test_state();
        let enabled = EnabledToolsets::all_of([Toolset::App]);
        let tool_toolset = build_tool_toolset_mapping(&enabled);
        let policy = test_policy_arc(SafetyTier::ReadOnly);
        // Build should succeed and include show_policy and list_available_tools
        let _router = build_router(
            state,
            policy,
            &enabled,
            ToolsConfig::default(),
            &tool_toolset,
            None,
        )
        .unwrap();
    }

    #[test]
    fn essentials_preset_filters_tools() {
        let state = test_state();
        let enabled = EnabledToolsets::all_of([Toolset::App]);
        let tool_toolset = build_tool_toolset_mapping(&enabled);
        let policy = test_policy_arc(SafetyTier::Full);
        let tools_config = ToolsConfig {
            preset: "essentials".to_string(),
            ..Default::default()
        };
        // Build should succeed with essentials preset
        let _router =
            build_router(state, policy, &enabled, tools_config, &tool_toolset, None).unwrap();
    }

    #[test]
    fn policy_disabled_toolset_removes_from_enabled() {
        use policy::ToolsetPolicy;

        let config = PolicyConfig {
            enterprise: Some(ToolsetPolicy {
                enabled: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        let disabled = config.disabled_toolsets();

        let mut toolsets = vec![Toolset::App];
        #[cfg(feature = "cloud")]
        toolsets.push(Toolset::Cloud);
        #[cfg(feature = "enterprise")]
        toolsets.push(Toolset::Enterprise);
        let mut enabled = EnabledToolsets::all_of(toolsets);

        enabled.retain(|t| !disabled.contains(&toolset_to_kind(t)));

        #[cfg(feature = "cloud")]
        assert!(enabled.contains(&Toolset::Cloud));
        #[cfg(feature = "enterprise")]
        assert!(!enabled.contains(&Toolset::Enterprise));
        assert!(enabled.contains(&Toolset::App));
    }

    #[test]
    fn toolset_to_kind_mapping() {
        #[cfg(feature = "cloud")]
        assert_eq!(toolset_to_kind(&Toolset::Cloud), ToolsetKind::Cloud);
        #[cfg(feature = "enterprise")]
        assert_eq!(
            toolset_to_kind(&Toolset::Enterprise),
            ToolsetKind::Enterprise
        );
        #[cfg(feature = "database")]
        assert_eq!(toolset_to_kind(&Toolset::Database), ToolsetKind::Database);
        assert_eq!(toolset_to_kind(&Toolset::App), ToolsetKind::App);
    }

    // ========================================================================
    // --tools parsing tests
    // ========================================================================

    #[test]
    fn parse_bare_toolset_names() {
        let specs = vec!["app".to_string()];
        let enabled = parse_tool_specs(&specs).unwrap();
        assert!(enabled.contains(&Toolset::App));
        assert!(matches!(
            enabled.selection(&Toolset::App),
            Some(SubModuleSelection::All)
        ));
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn parse_sub_module_syntax() {
        let specs = vec![
            "cloud:subscriptions".to_string(),
            "cloud:networking".to_string(),
        ];
        let enabled = parse_tool_specs(&specs).unwrap();
        assert!(enabled.contains(&Toolset::Cloud));
        match enabled.selection(&Toolset::Cloud) {
            Some(SubModuleSelection::Selected(set)) => {
                assert!(set.contains("subscriptions"));
                assert!(set.contains("networking"));
                assert!(!set.contains("account"));
            }
            other => panic!("Expected Selected, got {:?}", other),
        }
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn parse_mixed_bare_and_sub_module() {
        let specs = vec!["cloud:subscriptions".to_string(), "app".to_string()];
        let enabled = parse_tool_specs(&specs).unwrap();
        assert!(enabled.contains(&Toolset::Cloud));
        assert!(enabled.contains(&Toolset::App));
        assert!(matches!(
            enabled.selection(&Toolset::App),
            Some(SubModuleSelection::All)
        ));
        assert!(matches!(
            enabled.selection(&Toolset::Cloud),
            Some(SubModuleSelection::Selected(_))
        ));
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn parse_bare_overrides_sub_module() {
        let specs = vec!["cloud:subscriptions".to_string(), "cloud".to_string()];
        let enabled = parse_tool_specs(&specs).unwrap();
        assert!(matches!(
            enabled.selection(&Toolset::Cloud),
            Some(SubModuleSelection::All)
        ));
    }

    #[test]
    fn parse_invalid_toolset_errors() {
        let specs = vec!["bogus".to_string()];
        assert!(parse_tool_specs(&specs).is_err());
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn parse_invalid_sub_module_errors() {
        let specs = vec!["cloud:nonexistent".to_string()];
        assert!(parse_tool_specs(&specs).is_err());
    }

    #[test]
    fn parse_app_with_sub_module_errors() {
        let specs = vec!["app:anything".to_string()];
        assert!(parse_tool_specs(&specs).is_err());
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn sub_module_selection_limits_tool_names() {
        let specs = vec!["cloud:raw".to_string()];
        let enabled = parse_tool_specs(&specs).unwrap();
        let mapping = build_tool_toolset_mapping(&enabled);
        // Only the raw sub-module tool should be present
        assert!(mapping.contains_key("cloud_raw_api"));
        assert!(!mapping.contains_key("list_subscriptions"));
    }

    #[test]
    fn parse_skill_valid() {
        let content = "---\nname: my-skill\ndescription: A test skill\n---\nBody content here.";
        let skill = parse_skill(content).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "A test skill");
        assert_eq!(skill.body, "Body content here.");
    }

    #[test]
    fn parse_skill_missing_frontmatter() {
        assert!(parse_skill("No frontmatter here").is_none());
    }

    #[test]
    fn parse_skill_missing_name() {
        let content = "---\ndescription: A test skill\n---\nBody";
        assert!(parse_skill(content).is_none());
    }

    #[test]
    fn parse_skill_missing_description() {
        let content = "---\nname: my-skill\n---\nBody";
        assert!(parse_skill(content).is_none());
    }

    #[test]
    fn parse_skill_extra_dashes_in_body() {
        let content = "---\nname: my-skill\ndescription: desc\n---\nBody with --- dashes inside.";
        let skill = parse_skill(content).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.body, "Body with --- dashes inside.");
    }

    #[test]
    fn parse_skill_quoted_yaml_values() {
        let content = "---\nname: \"quoted-name\"\ndescription: 'quoted desc'\n---\nBody";
        let skill = parse_skill(content).unwrap();
        assert_eq!(skill.name, "quoted-name");
        assert_eq!(skill.description, "quoted desc");
    }

    #[test]
    fn parse_skill_trailing_whitespace_on_delimiter() {
        let content = "---  \nname: my-skill\ndescription: desc\n---  \nBody";
        let skill = parse_skill(content).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.body, "Body");
    }
}
