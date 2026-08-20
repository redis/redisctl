//! Onboarding engine for `redisctl init`: project detection, secret-safe URL
//! handling, typed planning, and idempotent apply.
//!
//! Callers (the CLI today, the MCP server eventually) own their surface - flags,
//! prompts, rendering, exit codes - and consume this crate's decisions. Nothing
//! here prints, prompts, or exits; long-running steps report through [`Event`].

mod change;
mod docker;
mod env;
mod example;
mod install;
mod mcp;
mod products;
mod project;
mod project_skill;
mod skills;
mod util;

pub use change::{Change, Status};
pub use docker::docker_ok as docker_available;
pub use project::{detect as detect_project, detect_agents};

pub(crate) const SKILLS_DIR: &str = ".agents/skills";
pub use docker::{LOCAL_REDIS_URL, LocalRedis, probe_local_redis, validate};
pub use env::read_env_key;
pub use products::{
    ProductKey, ProductRequest, SECRET_PLACEHOLDER, WiredProduct, validate_product,
};
pub use project::{Agent, KNOWN_AGENTS, Project, Runtime};
pub use util::{mask_url, slug};

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

    #[error("cannot complete {label}: .env needs {needed}.")]
    ProductIncomplete { label: String, needed: String },

    #[error("nothing to complete: .env has no Redis database or Redis Iris product setup.")]
    NothingToComplete,
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
    /// A caution the caller should make visually distinct.
    Warning(String),
}

/// What the caller asked for, resolved from its own surface (flags, prompts, tools).
pub struct Options {
    pub cwd: PathBuf,
    /// Database name, recorded in the generated project skill.
    pub name: Option<String>,
    /// Raw connection input: a URL, or a pasted `redis-cli -u <url>` command.
    /// `None` means no input was given (not blank input).
    pub url_input: Option<String>,
    /// The Redis Cloud database behind `url_input`, when the caller provisioned or
    /// picked one. Plain data - the engine makes no cloud calls.
    pub cloud: Option<CloudFacts>,
    /// Iris products to wire (endpoints and ids from the caller's flags).
    pub products: Vec<ProductRequest>,
    /// Discovery-only: guidance lands in the skill, no product runtime is added,
    /// and no database is provisioned unless `url_input` says so.
    pub iris: bool,
    /// Rediscover products (and the database) from `.env` and validate them.
    pub complete: bool,
    /// The API key for a single requested product; env vars and `.env` win over it.
    pub api_key: Option<String>,
    /// Skip writing the per-product example module.
    pub no_example: bool,
    /// `None` detects installed tools; detecting none still configures all agents.
    pub agents: Option<Vec<Agent>>,
    /// Install redis-cli when it is missing (`--no-install-cli` turns this off).
    pub install_cli: bool,
    /// A local redis/agent-skills checkout to copy from instead of the skills CLI.
    pub skills_repo: Option<PathBuf>,
    /// Install the official skills for the user instead of into this project.
    pub skills_global: bool,
}

// Manual, because `{:?}` reaches logs: `url_input` can carry a password and
// `api_key` is a credential outright.
impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("cwd", &self.cwd)
            .field("name", &self.name)
            .field("url_input", &self.url_input.as_ref().map(|_| "<redacted>"))
            .field("cloud", &self.cloud)
            .field("products", &self.products)
            .field("iris", &self.iris)
            .field("complete", &self.complete)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("no_example", &self.no_example)
            .field("agents", &self.agents)
            .field("install_cli", &self.install_cli)
            .field("skills_repo", &self.skills_repo)
            .field("skills_global", &self.skills_global)
            .finish()
    }
}

/// A Redis Cloud database the caller resolved before planning; flows into the
/// generated skill's facts and the control-plane MCP entry.
#[derive(Debug, Clone)]
pub struct CloudFacts {
    pub name: String,
    pub subscription_id: String,
    pub database_id: String,
    pub tier: CloudTier,
    /// The redisctl profile the control-plane hints should name, when not the default.
    pub profile: Option<String>,
    /// Freshly created this run (as opposed to reusing an existing database).
    pub created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudTier {
    Essentials,
    Flexible,
}

impl CloudTier {
    pub fn as_str(self) -> &'static str {
        match self {
            CloudTier::Essentials => "Essentials",
            CloudTier::Flexible => "Flexible",
        }
    }

    /// The CAPI path family this tier's databases live under.
    pub fn api_base(self) -> &'static str {
        match self {
            CloudTier::Essentials => "/fixed/subscriptions",
            CloudTier::Flexible => "/subscriptions",
        }
    }
}

/// What a run works with: the facts detected, the database it targets, and the
/// mutations decided. Rendering a plan is the dry run; [`apply`] performs it.
#[derive(Debug)]
pub struct Plan {
    pub project: Project,
    pub agents: Vec<Agent>,
    name: Option<String>,
    cloud: Option<CloudFacts>,
    database: Option<docker::DatabaseAction>,
    products: Vec<WiredProduct>,
    iris: bool,
    file_actions: Vec<env::FileAction>,
    client: Option<install::InstallAction>,
    sdks: Vec<install::InstallAction>,
    cli: Option<install::InstallAction>,
    examples: Vec<env::FileAction>,
    example_note: Option<Change>,
    skills: skills::SkillsAction,
    mcp: mcp::McpPlan,
    cwd: PathBuf,
}

impl Plan {
    /// `None` means this run works without a database (products-only, `--iris`).
    pub fn database_url(&self) -> Option<&str> {
        self.database.as_ref().map(|db| db.url())
    }

    /// The database's provenance for the summary line. `applied` picks the wording
    /// for a real run over the planned one ("new Docker container" vs
    /// "Docker (planned)").
    pub fn database_source(&self, applied: bool) -> Option<&'static str> {
        match (&self.cloud, &self.database) {
            (Some(cloud), _) if cloud.created && applied => Some("Redis Cloud (new database)"),
            (Some(cloud), _) if cloud.created => Some("Redis Cloud (planned)"),
            (Some(_), _) => Some("Redis Cloud (existing database)"),
            (None, Some(database)) => Some(database.source(applied)),
            (None, None) => None,
        }
    }

    /// The products this run works with, for the caller's Validate section,
    /// Action-required epilogue, and telemetry.
    pub fn products(&self) -> &[WiredProduct] {
        &self.products
    }

    /// Neither uvx nor Docker can run the MCP server; the configs are still written
    /// for uvx and the caller should say so.
    pub fn mcp_runner_missing(&self) -> bool {
        self.mcp.uvx_missing
    }

    /// The change report this plan predicts, in the order a run reports it.
    pub fn changes(&self) -> Vec<Change> {
        let mut changes: Vec<Change> = self
            .database
            .as_ref()
            .and_then(|database| database.preview())
            .into_iter()
            .collect();
        if self.iris {
            changes.push(iris_note());
        }
        changes.extend(self.file_actions.iter().map(|action| action.preview()));
        changes.extend(self.client.iter().map(|client| client.preview()));
        changes.extend(self.sdks.iter().map(|sdk| sdk.preview()));
        changes.extend(self.cli.iter().map(|cli| cli.preview()));
        changes.extend(self.examples.iter().map(|example| example.preview()));
        changes.extend(self.example_note.iter().cloned());
        changes.push(self.skills.preview());
        changes.push(project_skill::preview(&self.cwd));
        changes.extend(self.mcp.actions.iter().map(|action| action.preview()));
        changes
    }
}

/// The one thing an --iris run deliberately does not do, stated so no demo script
/// has to.
fn iris_note() -> Change {
    Change::new(
        "product runtime",
        Status::Skipped,
        "no .env, SDK, example, or MCP server until you approve a product",
    )
}

/// The change report of an applied plan.
#[derive(Debug)]
pub struct Report {
    pub changes: Vec<Change>,
    /// Where the skills actually landed, for the caller's next-steps epilogue.
    pub skills_dir: String,
    /// How many official skills were installed this run.
    pub skills_installed: usize,
}

pub fn plan(options: &Options) -> Result<Plan, InitError> {
    let products = products::wire(
        &options.cwd,
        &options.products,
        options.api_key.as_deref(),
        options.complete,
        &|key| std::env::var(key).ok(),
    )?;
    // The database is opt-out by absence: a products-only or --iris run adds no
    // runtime it was not asked for.
    let wants_database = options.url_input.is_some()
        || (!options.iris && products.is_empty() && !options.complete)
        || (options.complete && env::read_env_key(&options.cwd, ".env", "REDIS_URL").is_some());
    if options.complete && products.is_empty() && !wants_database {
        return Err(InitError::NothingToComplete);
    }
    let database = match options.url_input.as_deref() {
        Some(input) => Some(docker::DatabaseAction::Provided {
            url: extract_url(input)?,
        }),
        None if wants_database => Some(docker::plan_local_database(&options.cwd)?),
        None => None,
    };
    let project = project::detect(&options.cwd);
    let agents = project::resolve_agents(
        options.agents.as_deref(),
        &project::detect_agents(&options.cwd),
    );

    let mut file_actions = Vec::new();
    // Each Write's content threads into the next block, so later planners see
    // earlier additions instead of a stale disk read.
    let mut env_base: Option<String> = None;
    let mut example_base: Option<String> = None;
    if let Some(database) = &database {
        let action = env::plan_env_set(&options.cwd, ".env", "REDIS_URL", database.url())?;
        if let env::FileAction::Write { content, .. } = &action {
            env_base = Some(content.clone());
        }
        file_actions.push(action);
        let action = env::plan_env_set(
            &options.cwd,
            ".env.example",
            "REDIS_URL",
            "redis://localhost:6379",
        )?;
        if let env::FileAction::Write { content, .. } = &action {
            example_base = Some(content.clone());
        }
        file_actions.push(action);
    }
    for product in &products {
        let (action, next) = env::plan_env_set_block(
            &options.cwd,
            ".env",
            env_base.take(),
            &product.env_entries(),
        )?;
        env_base = next;
        file_actions.push(action);
        let (action, next) = env::plan_env_set_block(
            &options.cwd,
            ".env.example",
            example_base.take(),
            &product.example_entries(),
        )?;
        example_base = next;
        file_actions.push(action);
    }
    if database.is_some() || !products.is_empty() {
        file_actions.push(env::plan_gitignore_env(&options.cwd)?);
    }

    let client = database
        .is_some()
        .then(|| install::plan_client_install(&options.cwd, &project));
    let sdks = products
        .iter()
        .map(|product| install::plan_product_install(&options.cwd, &project, product))
        .collect();
    let cli = database
        .is_some()
        .then(|| install::plan_redis_cli(options.install_cli));
    let (examples, example_note) =
        example::plan_examples(&options.cwd, project.runtime, &products, options.no_example);
    let skills = skills::SkillsAction {
        agents: agents.clone(),
        global: options.skills_global,
        repo: options.skills_repo.clone(),
    };
    let mcp = mcp::plan_mcp(
        &options.cwd,
        &agents,
        mcp::McpInputs {
            database: database.is_some(),
            cloud: options
                .cloud
                .as_ref()
                .map(|cloud| (cloud, util::has_bin("redisctl-mcp"))),
            context_retriever: products
                .iter()
                .any(|p| matches!(p.spec.key, products::ProductKey::ContextRetriever))
                .then(|| util::has_bin("npx")),
        },
    )?;
    Ok(Plan {
        project,
        agents,
        name: options.name.clone(),
        cloud: options.cloud.clone(),
        database,
        products,
        iris: options.iris,
        file_actions,
        client,
        sdks,
        cli,
        examples,
        example_note,
        skills,
        mcp,
        cwd: options.cwd.clone(),
    })
}

/// Perform the plan's mutations: provision or revive the database, then write the
/// project files. Idempotent by construction - a second run plans only unchanged
/// entries.
pub async fn apply(plan: &Plan, on_event: &mut dyn FnMut(Event)) -> Result<Report, InitError> {
    let mut changes = Vec::new();
    if let Some(database) = &plan.database
        && let Some(change) = docker::apply_database(database, on_event).await?
    {
        changes.push(change);
    }
    if plan.iris {
        changes.push(iris_note());
    }
    for action in &plan.file_actions {
        changes.push(action.perform(&plan.cwd)?);
    }
    let mut client_installed = false;
    if let Some(client) = &plan.client {
        let client_change = client.perform(&plan.cwd, on_event)?;
        client_installed = matches!(client_change.status, Status::Updated | Status::Unchanged);
        changes.push(client_change);
    }
    for sdk in &plan.sdks {
        changes.push(sdk.perform(&plan.cwd, on_event)?);
    }
    if let Some(cli) = &plan.cli {
        changes.push(cli.perform(&plan.cwd, on_event)?);
    }
    for example in &plan.examples {
        changes.push(example.perform(&plan.cwd)?);
    }
    changes.extend(plan.example_note.iter().cloned());

    let skills = plan.skills.perform(&plan.cwd, on_event)?;
    let skills_installed = skills.installed.len();
    let skills_dir = skills
        .installed_dir
        .as_ref()
        .map(|dir| format!("{}/", dir.strip_prefix(&plan.cwd).unwrap_or(dir).display()))
        .unwrap_or_else(|| skills::describe_target(plan.skills.global));
    changes.extend(skills.changes);

    let facts = project_skill::SkillFacts {
        runtime: plan.project.runtime,
        name: plan.name.as_deref(),
        cloud: plan.cloud.as_ref(),
        database: plan.database.is_some(),
        container: plan
            .database
            .as_ref()
            .and_then(|database| database.container()),
        products: &plan.products,
        skills: &skills.installed,
        client_installed,
        cli_available: util::has_bin("redis-cli"),
        docker: docker::docker_ok(),
    };
    // Checkout-copied skills need links too; a global install lives under $HOME,
    // where Claude Code's own discovery already reads it.
    let also_link: &[String] = if !skills.via_npx && !plan.skills.global {
        &skills.installed
    } else {
        &[]
    };
    changes.extend(project_skill::generate(
        &plan.cwd,
        &facts,
        plan.agents.contains(&Agent::Claude),
        also_link,
    )?);
    for action in &plan.mcp.actions {
        changes.push(action.perform(&plan.cwd)?);
    }
    Ok(Report {
        changes,
        skills_dir,
        skills_installed,
    })
}

/// What counts as a connection string, wherever one arrives: a flag, a bare
/// positional, or a console paste (the Redis Cloud console hands out
/// `redis-cli -u <url>`; accept that whole and pull the URL out of it).
fn url_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"rediss?://[^\s"']+"#).expect("static regex"))
}

/// Pull the connection string out of raw input (a URL, or a pasted
/// `redis-cli -u <url>` command). The error's echoed text is credential-masked.
pub fn extract_url(input: &str) -> Result<String, InitError> {
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
            name: None,
            url_input: Some("redis-cli -u redis://h:1".into()),
            cloud: None,
            products: Vec::new(),
            iris: false,
            complete: false,
            api_key: None,
            no_example: false,
            agents: Some(vec![Agent::Claude]),
            install_cli: false,
            skills_repo: None,
            skills_global: false,
        })
        .unwrap();
        assert_eq!(plan.project.runtime, Runtime::Go);
        assert_eq!(plan.agents, vec![Agent::Claude]);
        assert_eq!(plan.database_url(), Some("redis://h:1"));
        assert_eq!(plan.database_source(true), Some("provided URL"));
    }

    #[test]
    fn plan_predicts_the_env_wiring() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&Options {
            cwd: dir.path().to_path_buf(),
            name: None,
            url_input: Some("redis://h:1".into()),
            cloud: None,
            products: Vec::new(),
            iris: false,
            complete: false,
            api_key: None,
            no_example: false,
            agents: Some(vec![Agent::Codex]),
            install_cli: false,
            skills_repo: None,
            skills_global: false,
        })
        .unwrap();
        let changes = plan.changes();
        let subjects: Vec<_> = changes.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(
            subjects[..3],
            [".env", ".env.example", ".gitignore"],
            "env wiring leads the report"
        );
        assert!(changes[..3].iter().all(|c| c.status == Status::Created));
        // Planning writes nothing.
        assert!(!dir.path().join(".env").exists());
    }

    #[tokio::test]
    async fn apply_writes_what_the_plan_predicted() {
        let dir = tempfile::tempdir().unwrap();
        // A fixture checkout keeps the skills step offline.
        let repo = tempfile::tempdir().unwrap();
        let skill = repo.path().join("skills/redis-basics");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "# basics\n").unwrap();
        let options = |cwd: &std::path::Path| Options {
            cwd: cwd.to_path_buf(),
            name: None,
            url_input: Some("redis-cli -u redis://h:1".into()),
            cloud: None,
            products: Vec::new(),
            iris: false,
            complete: false,
            api_key: None,
            no_example: false,
            agents: Some(vec![Agent::Codex]),
            install_cli: false,
            skills_repo: Some(repo.path().to_path_buf()),
            skills_global: false,
        };
        let plan = plan(&options(dir.path())).unwrap();
        let report = apply(&plan, &mut |_| {}).await.unwrap();
        let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
        assert!(env.contains("REDIS_URL=\"redis://h:1\""), "{env}");
        assert!(
            std::fs::read_to_string(dir.path().join(".env.example"))
                .unwrap()
                .contains("REDIS_URL=\"redis://localhost:6379\"")
        );
        assert!(
            std::fs::read_to_string(dir.path().join(".gitignore"))
                .unwrap()
                .contains(".env")
        );
        assert!(
            dir.path()
                .join(".agents/skills/redis-basics/SKILL.md")
                .exists()
        );
        assert!(report.changes.len() >= 6, "{:?}", report.changes);

        // A second plan over the applied state reads the file contract back as
        // all-unchanged (install and skills lines depend on machine state, the
        // files must not).
        let replan = super::plan(&options(dir.path())).unwrap();
        for subject in [".env", ".env.example", ".gitignore"] {
            let change = replan
                .changes()
                .into_iter()
                .find(|c| c.subject == subject)
                .unwrap();
            assert_eq!(change.status, Status::Unchanged, "{subject}");
        }
    }
}
