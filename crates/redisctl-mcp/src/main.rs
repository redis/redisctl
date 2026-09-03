//! redisctl-mcp: MCP server for Redis Cloud and Enterprise
//!
//! A standalone MCP server that exposes Redis management operations
//! as tools for AI systems.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use redisctl_core::Config;
use redisctl_mcp::{
    AuditConfig, AuditLayer, CredentialSource, McpServerBuilder, PolicyConfig, SafetyTier,
    ToolsetKind,
};
use tower_mcp::{McpRouter, transport::StdioTransport};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Transport mode for the MCP server
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum Transport {
    /// Standard input/output (for CLI integrations)
    #[default]
    Stdio,
    /// HTTP with Server-Sent Events (for shared deployments)
    Http,
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
    // Mixed rustls features need a provider; preserve a host choice and ignore install races.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    let args = Args::parse();

    // Resolve policy configuration (includes audit config)
    let (policy_config, policy_source) = resolve_policy(&args)?;
    let audit_config = Arc::new(policy_config.audit.clone());

    // Initialize tracing with optional audit layer
    // App logs: human-readable text to stderr (excludes audit target)
    // Audit logs: JSON to stderr (only audit target, when enabled)
    init_tracing(&args.log_level, audit_config.enabled);

    let credential_source = CredentialSource::Profiles(args.profile.clone());
    let skills_dir = resolve_skills_dir(&args);
    let mut builder =
        McpServerBuilder::new(credential_source, policy_config.clone(), &policy_source)
            .with_database_url(args.database_url.clone())
            .with_cluster_mode(args.cluster)
            .with_client_name(args.client_name.clone())
            .with_skills_dir(skills_dir);

    if let Some(tool_specs) = &args.tools {
        builder = builder.with_tool_specs(tool_specs)?;
    } else if let Ok(config) = Config::load() {
        if !config.profiles.is_empty() {
            info!("Auto-detected toolsets from config profiles");
        }
        builder = builder.with_profile_toolsets(&config);
    }

    let server = builder.build()?;
    info!(
        transport = ?args.transport,
        profiles = ?args.profile,
        policy_tier = %policy_config.tier,
        policy_source = %policy_source,
        audit_enabled = audit_config.enabled,
        toolsets = ?server.enabled_toolsets(),
        "Starting redisctl-mcp server"
    );
    let (router, tool_toolset_arc) = server.into_parts();

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

/// Run the HTTP server with middleware
#[cfg(feature = "http")]
async fn run_http_server(
    router: McpRouter,
    args: &Args,
    audit_config: Arc<AuditConfig>,
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

    transport.serve(&addr).await?;

    Ok(())
}

#[cfg(not(feature = "http"))]
async fn run_http_server(
    _router: McpRouter,
    _args: &Args,
    _audit_config: Arc<AuditConfig>,
    _tool_toolset: Arc<HashMap<String, ToolsetKind>>,
) -> Result<()> {
    anyhow::bail!("HTTP transport requires the 'http' feature")
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn default_cli_policy_is_read_only() {
        let args = Args::parse_from(["redisctl-mcp"]);

        let (config, source) = resolve_policy_with_source_flags(&args, false, false).unwrap();

        assert_eq!(config.tier, SafetyTier::ReadOnly);
        assert_eq!(source, "cli: --read-only=true (default)");
    }
}
