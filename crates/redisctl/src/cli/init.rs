//! `redisctl init` argument surface.
//!
//! This surface is the stability contract for the planned npm exec-wrapper: flags
//! may be added, never renamed or repurposed once released.

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
#[derive(Args, Debug)]
pub struct InitArgs {
    /// Use an existing redis:// or rediss:// database (default: provision a local
    /// Docker container). Also accepts a pasted Redis Cloud connect command:
    /// redisctl init --url "redis-cli -u redis://default:...@host:port"
    #[arg(long, value_name = "REDIS_URL")]
    pub url: Option<String>,

    /// Database name, recorded in the generated project skill
    #[arg(long, value_name = "LABEL")]
    pub name: Option<String>,

    /// Configure a specific agent (repeatable or comma-separated). Default: detect
    /// installed tools and configure all of them
    #[arg(long = "agent", value_enum, value_delimiter = ',', value_name = "NAME")]
    pub agents: Vec<AgentArg>,

    /// Print the plan without changing anything
    #[arg(long)]
    pub dry_run: bool,

    /// A pasted connect command, same as --url (the Cloud console's Copy button
    /// output works verbatim)
    #[arg(value_name = "PASTED", hide = true, num_args = 0..)]
    pub pasted: Vec<String>,
}
