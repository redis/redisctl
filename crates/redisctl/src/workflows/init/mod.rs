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

    let subject = plan.database.as_ref().map(|db| {
        format!(
            "database: {}{} via {}",
            engine::mask_url(&db.url),
            args.name
                .as_deref()
                .map(|n| format!(" [{n}]"))
                .unwrap_or_default(),
            db.source
        )
    });
    println!(
        "{}{}",
        bold(if dry { "Plan" } else { "Changes" }),
        subject
            .map(|s| format!("  {}", dim(&format!("({s})"))))
            .unwrap_or_default()
    );

    if dry {
        println!(
            "{}",
            dim("\nDry run complete. Run again without --dry-run to apply.\n")
        );
    }
    Ok(())
}
