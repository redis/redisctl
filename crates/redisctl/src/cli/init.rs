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
#[derive(Args)]
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

    /// Skip installing redis-cli when it is missing
    #[arg(long = "no-install-cli")]
    pub no_install_cli: bool,

    /// Print the plan without changing anything
    #[arg(long)]
    pub dry_run: bool,

    /// A pasted connect command, same as --url (the Cloud console's Copy button
    /// output works verbatim)
    #[arg(
        value_name = "PASTED",
        hide = true,
        num_args = 0..,
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub pasted: Vec<String>,
}

// Manual, because `{:?}` reaches trace logs: a URL or paste can carry a real
// password and must never survive into them.
impl std::fmt::Debug for InitArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InitArgs")
            .field("url", &self.url.as_ref().map(|_| "<redacted>"))
            .field("name", &self.name)
            .field("agents", &self.agents)
            .field("dry_run", &self.dry_run)
            .field("pasted", &(!self.pasted.is_empty()).then_some("<redacted>"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_url_or_paste_content() {
        let args = InitArgs {
            url: Some("redis://default:s3cret@h:1".into()),
            name: Some("db".into()),
            agents: vec![AgentArg::Claude],
            dry_run: false,
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
