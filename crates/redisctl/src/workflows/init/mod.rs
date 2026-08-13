//! `redisctl init` - onboard a project to Redis services and make its AI coding
//! agent Redis-fluent.
//!
//! This module owns only the CLI surface: argument shaping, banner, colours, and
//! rendering. The decisions live in the `redisctl-init` engine crate.

mod output;
mod wizard;

use redisctl_init as engine;

use crate::cli::{AgentArg, InitArgs};
use crate::error::RedisCtlError;
use output::{bold, dim, ok, yellow};

fn requested_agents(flags: &[AgentArg]) -> Option<Vec<engine::Agent>> {
    if flags.is_empty() {
        return None;
    }
    if flags.contains(&AgentArg::All) {
        return Some(engine::KNOWN_AGENTS.to_vec());
    }
    Some(
        flags
            .iter()
            .filter_map(|flag| match flag {
                AgentArg::Claude => Some(engine::Agent::Claude),
                AgentArg::Cursor => Some(engine::Agent::Cursor),
                AgentArg::Vscode => Some(engine::Agent::Vscode),
                AgentArg::Codex => Some(engine::Agent::Codex),
                AgentArg::All => None,
            })
            .collect(),
    )
}

pub async fn run(args: &InitArgs) -> Result<(), RedisCtlError> {
    let pasted = [args.url.clone().unwrap_or_default()]
        .into_iter()
        .chain(args.pasted.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    // Validated before the banner: a rejected --url should not decorate first.
    let url_input = match pasted.trim().is_empty() {
        true => None,
        false => Some(engine::extract_url(&pasted)?),
    };
    let cwd = std::env::current_dir().map_err(|e| RedisCtlError::FileError {
        path: ".".into(),
        message: e.to_string(),
    })?;
    let mut options = engine::Options {
        cwd: cwd.clone(),
        name: args.name.clone(),
        url_input,
        agents: requested_agents(&args.agents),
        install_cli: !args.no_install_cli,
        skills_repo: args.skills_repo.clone(),
        skills_global: args.skills_global,
    };

    output::banner();
    let dry = args.dry_run;
    println!(
        "{}",
        bold(&format!(
            "\nredisctl init{}\n",
            if dry {
                " (dry run - nothing will be written)"
            } else {
                ""
            }
        ))
    );

    let project = engine::detect_project(&cwd);
    let mut descriptor = project.runtime.as_str().to_string();
    if let Some(pm) = project.pm {
        descriptor.push_str(&format!(", {pm}"));
    }
    if let Some(framework) = &project.framework {
        descriptor.push_str(&format!(", {framework}"));
    }
    println!(
        "{}   {} {}",
        bold("Project"),
        project.name,
        dim(&format!("({descriptor})"))
    );

    let pending = wizard::pending_questions(args, options.url_input.is_some());
    let mut asked_agents = false;
    if wizard::applies(args, &pending) {
        let answers = wizard::run(
            &pending,
            &engine::detect_agents(&cwd),
            engine::docker_available(),
        )?;
        if let Some(url) = answers.url {
            options.url_input = Some(url);
        }
        if let Some(agents) = answers.agents {
            options.agents = Some(agents);
            asked_agents = true;
        }
        if let Some(global) = answers.skills_global {
            options.skills_global = global;
        }
    }
    let plan = engine::plan(&options)?;

    let proj = &plan.project;
    let agent_bits = proj
        .agent_markers
        .iter()
        .map(|(marker, found)| format!("{marker} {}", if *found { ok("✓") } else { dim("✗") }))
        .collect::<Vec<_>>()
        .join("  ");
    let names = plan
        .agents
        .iter()
        .map(|a| a.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "{}    {}{}   {}\n",
        bold("Agents"),
        names,
        if args.agents.is_empty() && !asked_agents {
            dim(" (detected)")
        } else {
            String::new()
        },
        dim(&format!("existing: {agent_bits}"))
    );
    if proj.runtime == engine::Runtime::Unknown {
        println!(
            "{}",
            yellow(
                "  note: no package manifest detected - continuing; everything redisctl init writes is language-agnostic.\n"
            )
        );
    }

    let subject = |applied: bool| {
        format!(
            "database: {}{} via {}",
            engine::mask_url(plan.database_url()),
            args.name
                .as_deref()
                .map(|n| format!(" [{n}]"))
                .unwrap_or_default(),
            plan.database_source(applied)
        )
    };

    if dry {
        println!(
            "{}  {}",
            bold("Plan"),
            dim(&format!("({})", subject(false)))
        );
        for change in plan.changes() {
            println!("{}", output::change_line(&change));
        }
        uvx_note(&plan);
        println!(
            "{}",
            dim("\nDry run complete. Run again without --dry-run to apply.\n")
        );
        return Ok(());
    }

    let mut progress: Option<output::Progress> = None;
    let report = engine::apply(&plan, &mut |event| match event {
        engine::Event::ProgressStart(label) => progress = Some(output::progress(&label)),
        engine::Event::ProgressDone(outcome) => {
            if let Some(p) = progress.as_mut() {
                p.done(&outcome);
            }
        }
        engine::Event::Note(text) => println!("{}", dim(&text)),
        engine::Event::Warning(text) => println!("{}", yellow(&text)),
    })
    .await?;

    println!(
        "{}  {}",
        bold("Changes"),
        dim(&format!("({})", subject(true)))
    );
    for change in &report.changes {
        println!("{}", output::change_line(change));
    }
    uvx_note(&plan);

    print!("\n{}  ", bold("Validate"));
    let _ = std::io::Write::flush(&mut std::io::stdout());
    match engine::validate(plan.database_url()).await {
        Ok(()) => println!(
            "{} PING  {} SET/GET  {}",
            ok("✓"),
            ok("✓"),
            dim(&format!("({})", engine::mask_url(plan.database_url())))
        ),
        Err(e) => {
            println!("{} {e}", output::red("✗"));
            return Err(RedisCtlError::ConnectionError {
                message: format!(
                    "could not talk to Redis at {}\n  If this URL is stale, remove REDIS_URL from .env and re-run, or pass --url.",
                    engine::mask_url(plan.database_url())
                ),
            });
        }
    }

    let suggestion = match (&plan.project.framework, plan.project.runtime) {
        (Some(framework), _) => format!(
            "Cache the slowest GET endpoint of this {framework} app in Redis with a 60-second TTL."
        ),
        (None, engine::Runtime::Unknown) => {
            "Add a Redis-backed cache for the most expensive operation in this project.".to_string()
        }
        _ => "Cache the most expensive read path in Redis with a 60-second TTL.".to_string(),
    };
    println!(
        "\n{}\n  1. Start your coding agent here ({names}) - it picks up the redis MCP server and the skills in {}.\n  2. Try asking it: {}\n",
        bold("Next steps"),
        report.skills_dir,
        bold(&format!("\"{suggestion}\""))
    );
    Ok(())
}

fn uvx_note(plan: &engine::Plan) {
    if plan.mcp_runner_missing() {
        println!(
            "{}",
            yellow(
                "\n  note: neither uvx nor Docker found - .mcp.json is written for uvx; install uv (https://docs.astral.sh/uv/) to use it."
            )
        );
    }
}
