//! The wizard: three questions, and only the three that change what happens. Each
//! is skipped the moment a flag already answers it; `--defaults` takes the defaults
//! instead, and piped stdin never prompts - an agent-invoked run never blocks.

use std::io::IsTerminal;

use dialoguer::console::Style;
use dialoguer::theme::Theme;
use dialoguer::{Input, MultiSelect, Select};
use redisctl_init as engine;

use crate::cli::InitArgs;
use crate::error::RedisCtlError;

/// A clack-style rail in brand red: `│` down the side, `◆` while a question is
/// live, `◇` once answered. The colour index matches the banner's 256-colour
/// fallback tone.
pub(crate) struct RedisTheme;

fn brand() -> Style {
    Style::new().color256(203)
}

impl Theme for RedisTheme {
    fn format_prompt(&self, f: &mut dyn std::fmt::Write, prompt: &str) -> std::fmt::Result {
        write!(
            f,
            "{}  {}",
            brand().apply_to('◆'),
            Style::new().bold().apply_to(prompt)
        )
    }

    fn format_error(&self, f: &mut dyn std::fmt::Write, err: &str) -> std::fmt::Result {
        write!(
            f,
            "{}  {}",
            brand().apply_to('└'),
            Style::new().red().apply_to(err)
        )
    }

    fn format_select_prompt_item(
        &self,
        f: &mut dyn std::fmt::Write,
        text: &str,
        active: bool,
    ) -> std::fmt::Result {
        let mark = if active {
            brand().apply_to('●').to_string()
        } else {
            Style::new().dim().apply_to('○').to_string()
        };
        let label = if active {
            text.to_string()
        } else {
            Style::new().dim().apply_to(text).to_string()
        };
        write!(f, "{}  {mark} {label}", brand().apply_to('│'))
    }

    fn format_multi_select_prompt_item(
        &self,
        f: &mut dyn std::fmt::Write,
        text: &str,
        checked: bool,
        active: bool,
    ) -> std::fmt::Result {
        let mark = if checked {
            brand().apply_to('■').to_string()
        } else {
            Style::new().dim().apply_to('□').to_string()
        };
        let label = if active {
            text.to_string()
        } else {
            Style::new().dim().apply_to(text).to_string()
        };
        write!(f, "{}  {mark} {label}", brand().apply_to('│'))
    }

    fn format_select_prompt_selection(
        &self,
        f: &mut dyn std::fmt::Write,
        prompt: &str,
        sel: &str,
    ) -> std::fmt::Result {
        write!(
            f,
            "{}  {}\n{}  {}",
            brand().apply_to('◇'),
            Style::new().bold().apply_to(prompt),
            brand().apply_to('│'),
            Style::new().dim().apply_to(sel)
        )
    }

    fn format_multi_select_prompt_selection(
        &self,
        f: &mut dyn std::fmt::Write,
        prompt: &str,
        selections: &[&str],
    ) -> std::fmt::Result {
        self.format_select_prompt_selection(f, prompt, &selections.join(", "))
    }

    fn format_input_prompt(
        &self,
        f: &mut dyn std::fmt::Write,
        prompt: &str,
        default: Option<&str>,
    ) -> std::fmt::Result {
        match default {
            Some(default) => write!(
                f,
                "{}  {} {}",
                brand().apply_to('◆'),
                Style::new().bold().apply_to(prompt),
                Style::new().dim().apply_to(format!("[{default}]"))
            ),
            None => write!(
                f,
                "{}  {}",
                brand().apply_to('◆'),
                Style::new().bold().apply_to(prompt)
            ),
        }
    }

    fn format_input_prompt_selection(
        &self,
        f: &mut dyn std::fmt::Write,
        prompt: &str,
        sel: &str,
    ) -> std::fmt::Result {
        // The pasted connection string carries a password; the confirmation line
        // must not reprint it.
        self.format_select_prompt_selection(f, prompt, &engine::mask_url(sel))
    }
}

const DATABASE_PROMPT: &str = "Where should the database come from?";
const DATABASE_ITEMS: [&str; 3] = ["Redis Cloud", "Local Redis", "Paste a connection string"];
const LOCAL_FOUND_PROMPT: &str = "Found a local Redis at localhost:6379 - use it?";
const LOCAL_NONE_PROMPT: &str = "No local Redis found - create one?";
const AGENTS_PROMPT: &str = "Which agent(s) should be configured?";
const SKILLS_PROMPT: &str = "Where should the Redis skills be installed?";
const INTERRUPTED: &str = "interrupted";

/// Cancel tips are picked by prompt (see `error.rs`): the wizard's questions get
/// `--defaults` guidance, while confirmation prompts elsewhere keep `--force`.
pub(crate) fn is_wizard_prompt(prompt: &str) -> bool {
    matches!(
        prompt,
        DATABASE_PROMPT
            | LOCAL_FOUND_PROMPT
            | LOCAL_NONE_PROMPT
            | AGENTS_PROMPT
            | SKILLS_PROMPT
            | INTERRUPTED
    ) || prompt == super::cloud::PICKER_PROMPT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    Database,
    Agents,
    Skills,
}

/// A question is worth asking only when no flag has already answered it.
pub fn pending_questions(args: &InitArgs, url_given: bool) -> Vec<Question> {
    let mut pending = Vec::new();
    // A product flag, --iris, or --complete already says where (or whether) the
    // database comes from.
    let products_answered = args.agent_memory.is_some()
        || args.langcache.is_some()
        || args.context_retriever.is_some()
        || args.iris
        || args.complete;
    if !url_given && !args.cloud && !products_answered {
        pending.push(Question::Database);
    }
    if args.agents.is_empty() {
        pending.push(Question::Agents);
    }
    if !args.skills_global {
        pending.push(Question::Skills);
    }
    pending
}

pub fn applies(args: &InitArgs, pending: &[Question]) -> bool {
    std::io::stdin().is_terminal() && !args.defaults && !pending.is_empty()
}

/// What the wizard decided; `None` per field means the question was not asked.
#[derive(Default)]
pub struct Answers {
    pub url: Option<String>,
    pub cloud: bool,
    pub agents: Option<Vec<engine::Agent>>,
    pub skills_global: Option<bool>,
}

fn cancelled(prompt: &str) -> RedisCtlError {
    RedisCtlError::Cancelled {
        prompt: prompt.to_string(),
    }
}

fn prompt_failed(e: dialoguer::Error) -> RedisCtlError {
    let dialoguer::Error::IO(io) = e;
    if io.kind() == std::io::ErrorKind::Interrupted {
        RedisCtlError::Cancelled {
            prompt: INTERRUPTED.to_string(),
        }
    } else {
        RedisCtlError::Other(format!("prompt failed: {io}"))
    }
}

pub fn run(
    pending: &[Question],
    detected: &[engine::Agent],
    docker: bool,
    local: engine::LocalRedis,
) -> Result<Answers, RedisCtlError> {
    let mut answers = Answers::default();
    for question in pending {
        match question {
            Question::Database => match ask_database(docker, local)? {
                DatabaseChoice::Docker => {}
                DatabaseChoice::Cloud => answers.cloud = true,
                DatabaseChoice::Url(url) => answers.url = Some(url),
            },
            Question::Agents => answers.agents = Some(ask_agents(detected)?),
            Question::Skills => answers.skills_global = Some(ask_skills_scope()?),
        }
    }
    Ok(answers)
}

enum DatabaseChoice {
    Docker,
    Cloud,
    Url(String),
}

/// Redis Cloud leads and is the default; "Local Redis" reuses a server that is
/// already running (asking for credentials when it wants them) and only falls back
/// to creating a Docker container; a paste comes back as the URL.
fn ask_database(docker: bool, local: engine::LocalRedis) -> Result<DatabaseChoice, RedisCtlError> {
    const PROMPT: &str = DATABASE_PROMPT;
    // No tier qualifier on Redis Cloud: the cloud flow connects to existing
    // databases on any plan; only creating a new one defaults to the free plan.
    let selection = Select::with_theme(&RedisTheme)
        .with_prompt(PROMPT)
        .items(&DATABASE_ITEMS)
        .default(0)
        .interact_opt()
        .map_err(prompt_failed)?;
    match selection {
        None => Err(cancelled(PROMPT)),
        Some(0) => Ok(DatabaseChoice::Cloud),
        Some(1) => ask_local(docker, local),
        _ => ask_paste(),
    }
}

/// The "Local Redis" sub-question, shaped by what the probe found: a ready server
/// is offered for reuse, one that wants credentials routes to the paste prompt,
/// and an absent one offers to create a Docker container.
fn ask_local(docker: bool, local: engine::LocalRedis) -> Result<DatabaseChoice, RedisCtlError> {
    if local == engine::LocalRedis::NeedsAuth {
        eprintln!(
            "  Found a local Redis at localhost:6379, but it needs credentials - paste its connection string."
        );
        return ask_paste();
    }
    // An option that cannot work stays on the list carrying the reason - the same
    // information the error would deliver after the run, shown before it instead.
    let docker_item = |label: &str| {
        if docker {
            label.to_string()
        } else {
            format!("{label}  (Docker is not running)")
        }
    };
    let found = local == engine::LocalRedis::Ready;
    let (prompt, items) = if found {
        (
            LOCAL_FOUND_PROMPT,
            [
                "Use it".to_string(),
                docker_item("Start a fresh Docker container instead"),
            ],
        )
    } else {
        (
            LOCAL_NONE_PROMPT,
            [
                docker_item("Start one with Docker"),
                "Paste a connection string".to_string(),
            ],
        )
    };
    let docker_index = if found { 1 } else { 0 };
    let default = if found || docker { 0 } else { 1 };
    loop {
        let selection = Select::with_theme(&RedisTheme)
            .with_prompt(prompt)
            .items(&items)
            .default(default)
            .interact_opt()
            .map_err(prompt_failed)?;
        match selection {
            None => return Err(cancelled(prompt)),
            Some(i) if i == docker_index && !docker => {
                eprintln!("  Docker is not running - start it, or paste a connection string.");
            }
            Some(i) if i == docker_index => return Ok(DatabaseChoice::Docker),
            Some(_) if found => return Ok(DatabaseChoice::Url(engine::LOCAL_REDIS_URL.into())),
            Some(_) => return ask_paste(),
        }
    }
}

fn ask_paste() -> Result<DatabaseChoice, RedisCtlError> {
    let pasted: String = Input::with_theme(&RedisTheme)
        .with_prompt("Paste the connection string")
        .validate_with(|input: &String| {
            engine::extract_url(input)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .interact_text()
        .map_err(prompt_failed)?;
    Ok(DatabaseChoice::Url(engine::extract_url(&pasted)?))
}

/// Detection preselects, it does not decide: having Cursor installed is not consent
/// to write .cursor/mcp.json into this repo.
fn ask_agents(detected: &[engine::Agent]) -> Result<Vec<engine::Agent>, RedisCtlError> {
    const PROMPT: &str = AGENTS_PROMPT;
    const LABELS: [&str; 4] = ["Claude Code", "Cursor", "VS Code", "Codex"];
    let preselected: Vec<bool> = engine::KNOWN_AGENTS
        .iter()
        .map(|agent| detected.contains(agent))
        .collect();
    loop {
        let picks = MultiSelect::with_theme(&RedisTheme)
            .with_prompt(format!("{PROMPT} (space toggles, enter confirms)"))
            .items(&LABELS)
            .defaults(&preselected)
            .interact_opt()
            .map_err(prompt_failed)?;
        match picks {
            None => return Err(cancelled(PROMPT)),
            Some(picks) if picks.is_empty() => eprintln!("  pick at least one"),
            Some(picks) => {
                return Ok(picks
                    .into_iter()
                    .map(|index| engine::KNOWN_AGENTS[index])
                    .collect());
            }
        }
    }
}

fn ask_skills_scope() -> Result<bool, RedisCtlError> {
    const PROMPT: &str = SKILLS_PROMPT;
    let selection = Select::with_theme(&RedisTheme)
        .with_prompt(PROMPT)
        .items(&[
            "This project only (.agents/skills)",
            "Global (available in every project)",
        ])
        .default(0)
        .interact_opt()
        .map_err(prompt_failed)?;
    match selection {
        None => Err(cancelled(PROMPT)),
        Some(choice) => Ok(choice == 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::AgentArg;

    fn args() -> InitArgs {
        InitArgs {
            url: None,
            cloud: false,
            cloud_subscription: None,
            name: None,
            agents: Vec::new(),
            defaults: false,
            no_install_cli: false,
            skills_repo: None,
            skills_global: false,
            dry_run: false,
            no_telemetry: false,
            agent_memory: None,
            store: None,
            langcache: None,
            cache: None,
            context_retriever: None,
            iris: false,
            api_key: None,
            complete: false,
            no_example: false,
            pasted: Vec::new(),
        }
    }

    #[test]
    fn the_cloud_picker_cancel_gets_wizard_tips() {
        assert!(is_wizard_prompt(super::super::cloud::PICKER_PROMPT));
        assert!(!is_wizard_prompt("Delete user 5?"));
    }

    #[test]
    fn redis_cloud_leads_the_database_options() {
        assert_eq!(
            DATABASE_ITEMS,
            ["Redis Cloud", "Local Redis", "Paste a connection string"]
        );
    }

    #[test]
    fn the_local_subflow_prompts_get_wizard_cancel_tips() {
        assert!(is_wizard_prompt(LOCAL_FOUND_PROMPT));
        assert!(is_wizard_prompt(LOCAL_NONE_PROMPT));
    }

    #[test]
    fn cloud_answers_the_database_question() {
        let mut a = args();
        a.cloud = true;
        assert_eq!(
            pending_questions(&a, false),
            vec![Question::Agents, Question::Skills]
        );
    }

    #[test]
    fn product_flags_iris_and_complete_answer_the_database_question() {
        for set in [
            |a: &mut InitArgs| a.agent_memory = Some("https://x.io".into()),
            |a: &mut InitArgs| a.langcache = Some("https://x.io".into()),
            |a: &mut InitArgs| a.context_retriever = Some("https://x.io".into()),
            |a: &mut InitArgs| a.iris = true,
            |a: &mut InitArgs| a.complete = true,
        ] {
            let mut a = args();
            set(&mut a);
            assert_eq!(
                pending_questions(&a, false),
                vec![Question::Agents, Question::Skills]
            );
        }
    }

    #[test]
    fn with_no_flags_all_three_questions_are_open() {
        assert_eq!(
            pending_questions(&args(), false),
            vec![Question::Database, Question::Agents, Question::Skills]
        );
    }

    #[test]
    fn a_url_answers_the_database_question() {
        assert_eq!(
            pending_questions(&args(), true),
            vec![Question::Agents, Question::Skills]
        );
    }

    #[test]
    fn agent_flags_answer_the_agents_question() {
        let mut a = args();
        a.agents = vec![AgentArg::Claude];
        assert_eq!(
            pending_questions(&a, false),
            vec![Question::Database, Question::Skills]
        );
    }

    #[test]
    fn skills_global_answers_the_skills_question() {
        let mut a = args();
        a.skills_global = true;
        assert_eq!(
            pending_questions(&a, false),
            vec![Question::Database, Question::Agents]
        );
    }

    #[test]
    fn wizard_cancel_tips_never_hijack_destructive_confirmations() {
        use crate::error::RedisCtlError;
        let wizard = RedisCtlError::Cancelled {
            prompt: DATABASE_PROMPT.to_string(),
        };
        assert!(
            wizard
                .suggestions()
                .iter()
                .any(|t| t.contains("--defaults"))
        );

        let destructive = RedisCtlError::Cancelled {
            prompt: "Delete user 5?".to_string(),
        };
        assert!(
            destructive
                .suggestions()
                .iter()
                .any(|t| t.contains("--force")),
            "{:?}",
            destructive.suggestions()
        );
    }

    #[test]
    fn defaults_flag_disables_the_wizard() {
        let mut a = args();
        a.defaults = true;
        let pending = pending_questions(&a, false);
        assert!(!applies(&a, &pending));
    }

    #[test]
    fn nothing_pending_disables_the_wizard() {
        assert!(!applies(&args(), &[]));
    }

    #[test]
    fn pasted_url_confirmation_is_masked() {
        let mut rendered = String::new();
        RedisTheme
            .format_input_prompt_selection(
                &mut rendered,
                "Paste the connection string",
                "redis://default:s3cret@host:6379",
            )
            .unwrap();
        assert!(
            rendered.contains("redis://default:****@host:6379"),
            "{rendered}"
        );
        assert!(!rendered.contains("s3cret"), "{rendered}");
    }
}
