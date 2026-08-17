//! `redisctl init` - onboard a project to Redis services and make its AI coding
//! agent Redis-fluent.
//!
//! This module owns only the CLI surface: argument shaping, banner, colours, and
//! rendering. The decisions live in the `redisctl-init` engine crate.

mod cloud;
mod dns;
mod output;
mod telemetry;
pub(crate) mod wizard;

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

pub async fn run(
    args: &InitArgs,
    conn_mgr: &crate::connection::ConnectionManager,
    profile: Option<&str>,
) -> Result<(), RedisCtlError> {
    // The wrapper owns the one telemetry event so every exit path - success,
    // failure, cancel - is counted exactly once, after the outcome exists.
    let mut telemetry = telemetry::Telemetry::start(args);
    let result = run_inner(args, conn_mgr, profile, &mut telemetry).await;
    telemetry.finish(args, &result).await;
    result
}

async fn run_inner(
    args: &InitArgs,
    conn_mgr: &crate::connection::ConnectionManager,
    profile: Option<&str>,
    telemetry: &mut telemetry::Telemetry,
) -> Result<(), RedisCtlError> {
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
        cloud: None,
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
    let mut wants_cloud = args.cloud;
    let interactive = wizard::applies(args, &pending);
    telemetry.props.interactive = interactive;
    telemetry.props.wizard_questions_asked = if interactive { pending.len() } else { 0 };
    if interactive {
        telemetry.step("wizard");
        let answers = wizard::run(
            &pending,
            &engine::detect_agents(&cwd),
            engine::docker_available(),
        )?;
        if let Some(url) = answers.url {
            options.url_input = Some(url);
        }
        wants_cloud = wants_cloud || answers.cloud;
        if let Some(agents) = answers.agents {
            options.agents = Some(agents);
            asked_agents = true;
        }
        if let Some(global) = answers.skills_global {
            options.skills_global = global;
        }
    }
    telemetry.step("database");
    // Never-clobber means a freshly provisioned database could never be recorded
    // in .env; keep the existing value instead of creating one the project would
    // not use (and burning the free-plan slot).
    if wants_cloud && engine::read_env_key(&cwd, ".env", "REDIS_URL").is_some() {
        println!(
            "{}",
            yellow(
                "  note: .env already carries REDIS_URL - keeping it (init never overwrites credentials). Remove that line to take the database from Redis Cloud.\n"
            )
        );
        wants_cloud = false;
    }
    let mut cloud_changes = Vec::new();
    if wants_cloud {
        // A missing profile has one fix worth naming here; other client errors
        // keep their own classification.
        let client = conn_mgr.create_cloud_client(profile).await.map_err(|e| {
            if matches!(
                e,
                RedisCtlError::NoProfileConfigured { .. }
                    | RedisCtlError::MissingCredentials { .. }
                    | RedisCtlError::ProfileNotFound { .. }
            ) {
                RedisCtlError::Other(format!(
                    "{e}\n  Sign in first: redisctl cloud auth login   (or pass -p <profile> with API keys)"
                ))
            } else {
                e
            }
        })?;
        let outcome = cloud::resolve(
            &client,
            &cwd,
            args.name.as_deref(),
            args.cloud_subscription,
            profile,
            dry,
            args.defaults,
        )
        .await?;
        // A freshly published endpoint's DNS can lag the API; gate before any
        // OS-resolver lookup so a premature failure never gets negative-cached.
        dns::wait_for_endpoint_dns(&outcome.url).await;
        options.url_input = Some(outcome.url);
        options.cloud = Some(outcome.facts);
        cloud_changes = outcome.changes;
    }
    let plan = engine::plan(&options)?;
    telemetry.props.database_source = Some(plan.database_source(!dry));
    telemetry.props.cloud_created = options.cloud.as_ref().is_some_and(|c| c.created);
    telemetry.props.runtime = Some(plan.project.runtime.as_str());
    telemetry.props.package_manager = plan.project.pm;
    telemetry.props.framework = plan.project.framework.clone();
    telemetry.props.agents = plan.agents.iter().map(|a| a.as_str()).collect();
    telemetry.props.agent_count = plan.agents.len();

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
        for change in cloud_changes.iter().cloned().chain(plan.changes()) {
            println!("{}", output::change_line(&change));
        }
        uvx_note(&plan);
        println!(
            "{}",
            dim("\nDry run complete. Run again without --dry-run to apply.\n")
        );
        return Ok(());
    }

    telemetry.step("apply");
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
    for change in cloud_changes.iter().chain(report.changes.iter()) {
        println!("{}", output::change_line(change));
    }
    uvx_note(&plan);

    telemetry.props.skills_installed_count = report.skills_installed;
    telemetry.step("validate");
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
            // A name that does not resolve is almost never stale input - it is a
            // record still propagating, or an OS cache remembering the gap.
            let remedy = if dns::is_dns_error(&e) {
                "The endpoint's DNS record may still be propagating (new databases can take a minute) - wait a moment and re-run.\n  macOS caches failed lookups; clear with: sudo dscacheutil -flushcache && sudo killall -HUP mDNSResponder"
            } else {
                "If this URL is stale, remove REDIS_URL from .env and re-run, or pass --url."
            };
            return Err(RedisCtlError::ConnectionError {
                message: format!(
                    "could not talk to Redis at {}\n  {remedy}",
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
