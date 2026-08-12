//! Onboarding engine for `redisctl init`: project detection, secret-safe URL
//! handling, and typed planning.
//!
//! Callers (the CLI today, the MCP server eventually) own their surface - flags,
//! prompts, rendering, exit codes - and consume this crate's decisions. Nothing
//! here prints, prompts, or exits.

mod project;
mod util;

pub use project::{Agent, KNOWN_AGENTS, Project, Runtime};
pub use util::mask_url;

use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    /// The input carried no connection string. The echoed text is already
    /// credential-masked and safe to display.
    #[error("no redis:// or rediss:// URL found in: {masked_input}")]
    NoUrlInInput { masked_input: String },
}

/// What the caller asked for, resolved from its own surface (flags, prompts, tools).
#[derive(Debug)]
pub struct Options {
    pub cwd: PathBuf,
    /// Raw connection input: a URL, or a pasted `redis-cli -u <url>` command.
    /// `None` means no input was given (not blank input).
    pub url_input: Option<String>,
    /// `None` detects installed tools; detecting none still configures all agents.
    pub agents: Option<Vec<Agent>>,
}

/// What a run works with: the facts detected and the database it targets.
#[derive(Debug)]
pub struct Plan {
    pub project: Project,
    pub agents: Vec<Agent>,
    pub database: Option<DatabasePlan>,
}

#[derive(Debug)]
pub struct DatabasePlan {
    pub url: String,
    pub source: &'static str,
}

pub fn plan(options: &Options) -> Result<Plan, InitError> {
    let database = options
        .url_input
        .as_deref()
        .map(extract_url)
        .transpose()?
        .map(|url| DatabasePlan {
            url,
            source: "provided URL",
        });
    let project = project::detect(&options.cwd);
    let agents = project::resolve_agents(
        options.agents.as_deref(),
        &project::detect_agents(&options.cwd),
    );
    Ok(Plan {
        project,
        agents,
        database,
    })
}

/// What counts as a connection string, wherever one arrives: a flag, a bare
/// positional, or a console paste (the Redis Cloud console hands out
/// `redis-cli -u <url>`; accept that whole and pull the URL out of it).
fn url_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"rediss?://[^\s"']+"#).expect("static regex"))
}

fn extract_url(input: &str) -> Result<String, InitError> {
    match url_regex().find(input) {
        Some(m) => Ok(m.as_str().to_string()),
        None => Err(InitError::NoUrlInInput {
            masked_input: util::mask_url(input.trim()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_urls_pass_through() {
        assert_eq!(
            extract_url("redis://localhost:6379").unwrap(),
            "redis://localhost:6379"
        );
        assert_eq!(extract_url("rediss://h:12000").unwrap(), "rediss://h:12000");
    }

    #[test]
    fn pasted_connect_command_yields_its_url() {
        assert_eq!(
            extract_url("redis-cli -u redis://default:pw@host:12000").unwrap(),
            "redis://default:pw@host:12000"
        );
    }

    #[test]
    fn quotes_delimit_the_url() {
        assert_eq!(
            extract_url(r#"redis-cli -u "rediss://h:1""#).unwrap(),
            "rediss://h:1"
        );
    }

    #[test]
    fn text_without_a_url_is_an_error_naming_the_text() {
        let msg = extract_url("garbage in").unwrap_err().to_string();
        assert!(msg.contains("no redis:// or rediss:// URL found"), "{msg}");
        assert!(msg.contains("garbage in"), "{msg}");
    }

    #[test]
    fn rejected_input_never_echoes_a_credential() {
        let msg = extract_url("redisx://default:secret@host:6379")
            .unwrap_err()
            .to_string();
        assert!(msg.contains("redisx://default:****@host:6379"), "{msg}");
        assert!(!msg.contains("secret"), "{msg}");
    }

    #[test]
    fn plan_carries_detection_and_the_provided_database() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module demo\n").unwrap();
        let plan = plan(&Options {
            cwd: dir.path().to_path_buf(),
            url_input: Some("redis-cli -u redis://h:1".into()),
            agents: Some(vec![Agent::Claude]),
        })
        .unwrap();
        assert_eq!(plan.project.runtime, Runtime::Go);
        assert_eq!(plan.agents, vec![Agent::Claude]);
        let db = plan.database.unwrap();
        assert_eq!(db.url, "redis://h:1");
        assert_eq!(db.source, "provided URL");
    }

    #[test]
    fn plan_without_input_has_no_database() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&Options {
            cwd: dir.path().to_path_buf(),
            url_input: None,
            agents: Some(vec![Agent::Codex]),
        })
        .unwrap();
        assert!(plan.database.is_none());
    }
}
