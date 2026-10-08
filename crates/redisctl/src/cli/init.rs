//! `redisctl init` argument surface.
//!
//! Scripts, agents and wrappers drive these flags without a terminal, so they are a
//! stability contract: flags may be added, never renamed or repurposed once released.

use crate::workflows::init::engine::mask_url;
use clap::Args;

/// Agents `redisctl init` can configure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum AgentArg {
    Claude,
    Cursor,
    Vscode,
    Codex,
    All,
}

/// Onboard this project to Redis services + set up your AI coding agent.
#[derive(Args)]
pub struct InitArgs {
    /// Use an existing redis:// or rediss:// database. Also accepts a pasted Redis
    /// Cloud connect command:
    /// redisctl init --url "redis-cli -u redis://default:...@host:port"
    #[arg(long, short_alias = 'u', value_name = "REDIS_URL")]
    pub url: Option<String>,

    /// Take the database from Redis Cloud: reuse the database named by --name, pick
    /// one on a terminal, or create one on the free Essentials plan. Signs in first
    /// on a terminal when no Cloud profile exists
    #[arg(long, conflicts_with_all = ["url", "pasted"])]
    pub cloud: bool,

    /// Create in this Essentials subscription instead of the free plan
    #[arg(long, value_name = "ID", requires = "cloud")]
    pub cloud_subscription: Option<i32>,

    /// Database name: with --cloud it is the reuse key (a database already carrying
    /// it is connected, not created) and the name of a new one; a database outside
    /// Docker is named by it in the generated project skill
    #[arg(long, value_name = "LABEL")]
    pub name: Option<String>,

    /// Wire Agent Memory: the service endpoint from the Redis Cloud console
    #[arg(long, value_name = "ENDPOINT")]
    pub agent_memory: Option<String>,

    /// The Agent Memory store id, from the same console page
    #[arg(long, value_name = "ID")]
    pub store: Option<String>,

    /// Wire LangCache: the service endpoint from the Redis Cloud console
    #[arg(long, value_name = "ENDPOINT")]
    pub langcache: Option<String>,

    /// The LangCache cache id, from the same console page
    #[arg(long, value_name = "ID")]
    pub cache: Option<String>,

    /// Wire Context Retriever: the MCP endpoint from the Redis Cloud console
    #[arg(long, value_name = "ENDPOINT")]
    pub context_retriever: Option<String>,

    /// Discovery-only: teach the agent to recommend the smallest Iris setup;
    /// adds no product runtime until you approve one
    #[arg(long)]
    pub iris: bool,

    /// API key for the one product being wired. A real key already in .env wins over
    /// it, and so does the product's own env var when .env has none, keeping keys
    /// out of shell history; a placeholder in .env gives way to it
    #[arg(long, value_name = "KEY")]
    pub api_key: Option<String>,

    /// Validate a product setup already present in .env (fill the placeholders
    /// first)
    #[arg(long)]
    pub complete: bool,

    /// Skip writing the per-product example module
    #[arg(long = "no-example")]
    pub no_example: bool,

    /// Configure a specific agent (repeatable or comma-separated). Default: detect
    /// installed tools and configure all of them
    #[arg(long = "agent", value_enum, value_delimiter = ',', value_name = "NAME")]
    pub agents: Vec<AgentArg>,

    /// Take the defaults instead of asking: with no database flag that is a local
    /// Docker container. With --cloud and no Cloud sign-in, a terminal still opens
    /// the browser to sign in. Piped stdin never prompts
    #[arg(long)]
    pub defaults: bool,

    /// Skip installing redis-cli when it is missing
    #[arg(long = "no-install-cli")]
    pub no_install_cli: bool,

    /// Path to a redis/agent-skills checkout to copy skills from (offline-safe;
    /// default: install via the standard skills CLI)
    #[arg(long, value_name = "DIR", env = "REDISCTL_INIT_SKILLS_REPO")]
    pub skills_repo: Option<std::path::PathBuf>,

    /// Install the official Redis skills for your user instead of into this project
    #[arg(long)]
    pub skills_global: bool,

    /// Print the plan without changing anything
    #[arg(long)]
    pub dry_run: bool,

    /// Do not send the anonymous usage event for this run (also honored:
    /// REDISCTL_INIT_TELEMETRY=0, DO_NOT_TRACK=1)
    #[arg(long = "no-telemetry")]
    pub no_telemetry: bool,

    /// A pasted connect command, same as --url (the Cloud console's Copy button
    /// output works verbatim): `redis-cli` lands here and its `-u` is the hidden
    /// alias of --url, so flags before or after the paste keep parsing as flags
    #[arg(value_name = "REDIS_URL", hide = true, num_args = 0..)]
    pub pasted: Vec<String>,
}

// Manual, because `{:?}` reaches trace logs: a URL or paste can carry a real
// password and must never survive into them.
impl std::fmt::Debug for InitArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InitArgs")
            .field("url", &self.url.as_ref().map(|_| "<redacted>"))
            .field("cloud", &self.cloud)
            .field("cloud_subscription", &self.cloud_subscription)
            .field("agent_memory", &self.agent_memory.as_deref().map(mask_url))
            .field("store", &self.store)
            .field("langcache", &self.langcache.as_deref().map(mask_url))
            .field("cache", &self.cache)
            .field(
                "context_retriever",
                &self.context_retriever.as_deref().map(mask_url),
            )
            .field("iris", &self.iris)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("complete", &self.complete)
            .field("no_example", &self.no_example)
            .field("name", &self.name)
            .field("agents", &self.agents)
            .field("defaults", &self.defaults)
            .field("no_install_cli", &self.no_install_cli)
            .field("skills_repo", &self.skills_repo)
            .field("skills_global", &self.skills_global)
            .field("dry_run", &self.dry_run)
            .field("no_telemetry", &self.no_telemetry)
            .field("pasted", &(!self.pasted.is_empty()).then_some("<redacted>"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_url_endpoint_or_paste_passwords() {
        let args = InitArgs {
            url: Some("redis://default:s3cret@h:1".into()),
            cloud: false,
            cloud_subscription: None,
            name: Some("db".into()),
            agents: vec![AgentArg::Claude],
            defaults: false,
            no_install_cli: false,
            skills_repo: None,
            skills_global: false,
            dry_run: false,
            no_telemetry: false,
            agent_memory: Some("https://u:s3cret@memory.example".into()),
            store: None,
            langcache: Some("rediss://default:s3cret@h:1".into()),
            cache: None,
            context_retriever: Some("https://u:s3cret@retriever.example".into()),
            iris: false,
            api_key: None,
            complete: false,
            no_example: false,
            pasted: vec![
                "redis-cli".into(),
                "-u".into(),
                "redis://default:s3cret@h:2".into(),
            ],
        };
        let debug = format!("{args:?}");
        assert!(!debug.contains("s3cret"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }
}
