//! Onboarding engine for `redisctl init`: project detection, secret-safe URL
//! handling, typed planning, and idempotent apply.
//!
//! Callers (the CLI today, the MCP server eventually) own their surface - flags,
//! prompts, rendering, exit codes - and consume this crate's decisions. Nothing
//! here prints, prompts, or exits; long-running steps report through [`Event`].

mod change;
mod docker;
mod env;
mod project;
mod util;

pub use change::{Change, Status};
pub use docker::validate;
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

    #[error(
        "Docker is not available and no --url was given.\n  Either start Docker, or point at an existing database:\n    redisctl init --url redis://localhost:6379"
    )]
    DockerUnavailable,

    #[error("{command} failed: {stderr}")]
    DockerCommand { command: String, stderr: String },

    #[error("no free port found between 6379 and 6478")]
    NoFreePort,

    #[error("Redis at {url} did not become ready: {error}")]
    NotReady { url: String, error: String },

    #[error("'{rel}' exists but cannot be read - refusing to overwrite it")]
    UnreadableFile { rel: String },

    #[error("cannot write '{rel}': {message}")]
    WriteFailed { rel: String, message: String },
}

/// Progress reporting from [`apply`]: the caller renders these however its surface
/// wants (spinner, log line, nothing).
#[derive(Debug)]
pub enum Event {
    /// A long-running step began; keep its line open until `ProgressDone`.
    ProgressStart(String),
    /// The open step finished with this outcome (" ready", " done", or empty when
    /// an error message follows on its own line).
    ProgressDone(String),
    /// A one-off informational line.
    Note(String),
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

/// What a run works with: the facts detected, the database it targets, and the
/// mutations decided. Rendering a plan is the dry run; [`apply`] performs it.
#[derive(Debug)]
pub struct Plan {
    pub project: Project,
    pub agents: Vec<Agent>,
    database: docker::DatabaseAction,
    file_actions: Vec<env::FileAction>,
    cwd: PathBuf,
}

impl Plan {
    pub fn database_url(&self) -> &str {
        self.database.url()
    }

    /// The database's provenance for the summary line. `applied` picks the wording
    /// for a real run over the planned one ("new Docker container" vs
    /// "Docker (planned)").
    pub fn database_source(&self, applied: bool) -> &'static str {
        self.database.source(applied)
    }

    /// The change report this plan predicts, in the order a run reports it.
    pub fn changes(&self) -> Vec<Change> {
        self.database
            .preview()
            .into_iter()
            .chain(self.file_actions.iter().map(|action| action.preview()))
            .collect()
    }
}

/// The change report of an applied plan.
#[derive(Debug)]
pub struct Report {
    pub changes: Vec<Change>,
}

pub fn plan(options: &Options) -> Result<Plan, InitError> {
    let database = match options.url_input.as_deref() {
        Some(input) => docker::DatabaseAction::Provided {
            url: extract_url(input)?,
        },
        None => docker::plan_local_database(&options.cwd)?,
    };
    let project = project::detect(&options.cwd);
    let agents = project::resolve_agents(
        options.agents.as_deref(),
        &project::detect_agents(&options.cwd),
    );
    let file_actions = vec![
        env::plan_env_set(&options.cwd, ".env", "REDIS_URL", database.url())?,
        env::plan_gitignore_env(&options.cwd)?,
    ];
    Ok(Plan {
        project,
        agents,
        database,
        file_actions,
        cwd: options.cwd.clone(),
    })
}

/// Perform the plan's mutations: provision or revive the database, then write the
/// project files. Idempotent by construction - a second run plans only unchanged
/// entries.
pub async fn apply(plan: &Plan, on_event: &mut dyn FnMut(Event)) -> Result<Report, InitError> {
    let mut changes = Vec::new();
    if let Some(change) = docker::apply_database(&plan.database, on_event).await? {
        changes.push(change);
    }
    for action in &plan.file_actions {
        changes.push(action.perform(&plan.cwd)?);
    }
    Ok(Report { changes })
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
        assert_eq!(plan.database_url(), "redis://h:1");
        assert_eq!(plan.database_source(true), "provided URL");
    }

    #[test]
    fn plan_predicts_the_env_wiring() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&Options {
            cwd: dir.path().to_path_buf(),
            url_input: Some("redis://h:1".into()),
            agents: Some(vec![Agent::Codex]),
        })
        .unwrap();
        let changes = plan.changes();
        let subjects: Vec<_> = changes.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, vec![".env", ".gitignore"]);
        assert!(changes.iter().all(|c| c.status == Status::Created));
        // Planning writes nothing.
        assert!(!dir.path().join(".env").exists());
    }

    #[tokio::test]
    async fn apply_writes_what_the_plan_predicted() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&Options {
            cwd: dir.path().to_path_buf(),
            url_input: Some("redis://h:1".into()),
            agents: Some(vec![Agent::Codex]),
        })
        .unwrap();
        let report = apply(&plan, &mut |_| {}).await.unwrap();
        assert_eq!(report.changes.len(), 2);
        let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
        assert!(env.contains("REDIS_URL=\"redis://h:1\""), "{env}");
        assert!(
            std::fs::read_to_string(dir.path().join(".gitignore"))
                .unwrap()
                .contains(".env")
        );

        // A second plan over the applied state reads back all-unchanged.
        let replan = super::plan(&Options {
            cwd: dir.path().to_path_buf(),
            url_input: Some("redis://h:1".into()),
            agents: Some(vec![Agent::Codex]),
        })
        .unwrap();
        assert!(
            replan
                .changes()
                .iter()
                .all(|c| c.status == Status::Unchanged)
        );
    }
}
