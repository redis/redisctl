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
struct RedisTheme;

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
        self.format_select_prompt_selection(f, prompt, sel)
    }
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
    if !url_given {
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
            prompt: "interrupted".to_string(),
        }
    } else {
        RedisCtlError::Other(format!("prompt failed: {io}"))
    }
}

pub fn run(
    pending: &[Question],
    detected: &[engine::Agent],
    docker: bool,
) -> Result<Answers, RedisCtlError> {
    let mut answers = Answers::default();
    for question in pending {
        match question {
            Question::Database => answers.url = ask_database(docker)?,
            Question::Agents => answers.agents = Some(ask_agents(detected)?),
            Question::Skills => answers.skills_global = Some(ask_skills_scope()?),
        }
    }
    Ok(answers)
}

/// `Ok(None)` means the local Docker default; a paste comes back as the URL.
fn ask_database(docker: bool) -> Result<Option<String>, RedisCtlError> {
    const PROMPT: &str = "Where should the database come from?";
    // An option that cannot work stays on the list carrying the reason - the same
    // information the error would deliver after the run, shown before it instead.
    let docker_item = if docker {
        "Local Docker container"
    } else {
        "Local Docker container  (Docker is not running)"
    };
    let items = [docker_item, "Paste a connection string"];
    loop {
        let selection = Select::with_theme(&RedisTheme)
            .with_prompt(PROMPT)
            .items(&items)
            .default(if docker { 0 } else { 1 })
            .interact_opt()
            .map_err(prompt_failed)?;
        match selection {
            None => return Err(cancelled(PROMPT)),
            Some(0) if !docker => {
                eprintln!("  Docker is not running - start it, or paste a connection string.");
            }
            Some(0) => return Ok(None),
            _ => break,
        }
    }
    let pasted: String = Input::with_theme(&RedisTheme)
        .with_prompt("Paste the connection string")
        .validate_with(|input: &String| {
            engine::extract_url(input)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .interact_text()
        .map_err(prompt_failed)?;
    Ok(Some(engine::extract_url(&pasted)?))
}

/// Detection preselects, it does not decide: having Cursor installed is not consent
/// to write .cursor/mcp.json into this repo.
fn ask_agents(detected: &[engine::Agent]) -> Result<Vec<engine::Agent>, RedisCtlError> {
    const PROMPT: &str = "Which agent(s) should be configured?";
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
    const PROMPT: &str = "Where should the Redis skills be installed?";
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
            name: None,
            agents: Vec::new(),
            defaults: false,
            no_install_cli: false,
            skills_repo: None,
            skills_global: false,
            dry_run: false,
            pasted: Vec::new(),
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
    fn piped_stdin_never_prompts() {
        // Tests run with piped stdin, so this exercises the real guard.
        let pending = pending_questions(&args(), false);
        assert!(!applies(&args(), &pending));
    }
}
