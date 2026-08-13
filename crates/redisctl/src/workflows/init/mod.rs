//! `redisctl init` - onboard a project to Redis services and make its AI coding
//! agent Redis-fluent.
//!
//! This module owns only the CLI surface: argument shaping, banner, colours, and
//! rendering. The decisions live in the `redisctl-init` engine crate.

mod output;

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
    let options = engine::Options {
        cwd: std::env::current_dir().map_err(|e| RedisCtlError::FileError {
            path: ".".into(),
            message: e.to_string(),
        })?,
        url_input: (!pasted.trim().is_empty()).then_some(pasted),
        agents: requested_agents(&args.agents),
        install_cli: !args.no_install_cli,
        skills_repo: args.skills_repo.clone(),
        skills_global: args.skills_global,
    };
    let plan = engine::plan(&options)?;

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

    let proj = &plan.project;
    let mut descriptor = proj.runtime.as_str().to_string();
    if let Some(pm) = proj.pm {
        descriptor.push_str(&format!(", {pm}"));
    }
    if let Some(framework) = &proj.framework {
        descriptor.push_str(&format!(", {framework}"));
    }
    println!(
        "{}   {} {}",
        bold("Project"),
        proj.name,
        dim(&format!("({descriptor})"))
    );

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
        if args.agents.is_empty() {
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
    println!();
    Ok(())
}
