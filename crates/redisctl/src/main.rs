use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, shells};
use redisctl_core::{Config, ConfigError, DeploymentType};
use std::io::IsTerminal;
use tracing::{debug, info, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cli;
mod commands;
mod connection;
mod error;
mod output;
mod workflows;

use cli::{Cli, Commands};
use connection::ConnectionManager;
use error::RedisCtlError;

/// Commands that are already top-level (no prefix needed) or are explicit platform prefixes.
/// These pass through unchanged.
const PASSTHROUGH_COMMANDS: &[&str] = &[
    "cloud",
    "cl", // cloud alias
    "enterprise",
    "ent", // enterprise alias
    "en",  // enterprise alias
    "profile",
    "prof", // profile alias
    "pr",   // profile alias
    "api",
    "db",
    "version",
    "ver", // version alias
    "v",   // version alias
    "completions",
    "comp", // completions alias
    "help",
    "files-key",
    "fk", // files-key alias
];

/// Commands that exist only under `cloud`.
const CLOUD_ONLY_COMMANDS: &[&str] = &[
    "subscription",
    "account",
    "payment-method",
    "provider-account",
    "task",
    "connectivity",
    "fixed-database",
    "fixed-subscription",
    "cost-report",
];

/// Commands that exist only under `enterprise`.
const ENTERPRISE_ONLY_COMMANDS: &[&str] = &[
    "cluster",
    "node",
    "shard",
    "endpoint",
    "proxy",
    "role",
    "ldap",
    "ldap-mappings",
    "auth",
    "bootstrap",
    "crdb",
    "crdb-task",
    "job-scheduler",
    "jsonschema",
    "logs",
    "license",
    "migration",
    "module",
    "ocsp",
    "services",
    "local",
    "stats",
    "status",
    "support-package",
    "suffix",
    "usage-report",
    "bdb-group",
    "cm-settings",
    "debug-info",
    "diagnostics",
    "alerts",
    "action",
];

/// Commands that exist under both `cloud` and `enterprise`.
const SHARED_COMMANDS: &[&str] = &["database", "user", "acl", "workflow"];

/// Global flags that accept a following value (the value must be skipped when
/// scanning for the first positional arg).
///
/// Keep in sync with `Cli` struct global args.
const GLOBAL_VALUE_FLAGS: &[&str] = &[
    "--profile",
    "-p",
    "--config-file",
    "--output",
    "-o",
    "--query",
    "-q",
];

/// Rewrite `args` to inject the platform prefix when omitted.
///
/// Returns the (possibly modified) arg list that should be passed to
/// `Cli::parse_from()`.
fn maybe_inject_prefix(args: Vec<String>) -> Vec<String> {
    // Parse out global flags to find the first positional arg and any --profile value.
    let mut first_positional_idx: Option<usize> = None;
    let mut explicit_profile: Option<String> = None;
    let mut config_file: Option<String> = None;
    let mut has_help = false;

    let mut i = 1; // skip argv[0]
    while i < args.len() {
        let arg = &args[i];

        if arg == "--help" || arg == "-h" {
            has_help = true;
            i += 1;
            continue;
        }

        if arg == "--" {
            // Everything after `--` is positional — stop scanning.
            break;
        }

        // Boolean flags (no value)
        if arg == "--verbose" {
            i += 1;
            continue;
        }

        // Short verbose stacking: -v, -vv, -vvv
        if arg.starts_with('-') && !arg.starts_with("--") && arg.chars().skip(1).all(|c| c == 'v') {
            i += 1;
            continue;
        }

        // Value flags: --flag value or --flag=value
        if GLOBAL_VALUE_FLAGS.contains(&arg.as_str()) {
            // --flag value form
            if (arg == "--profile" || arg == "-p")
                && let Some(val) = args.get(i + 1)
            {
                explicit_profile = Some(val.clone());
            }
            if arg == "--config-file"
                && let Some(val) = args.get(i + 1)
            {
                config_file = Some(val.clone());
            }
            i += 2; // skip flag + value
            continue;
        }

        // --flag=value form
        if arg.starts_with("--")
            && let Some((key, val)) = arg.split_once('=')
            && GLOBAL_VALUE_FLAGS.contains(&key)
        {
            if key == "--profile" {
                explicit_profile = Some(val.to_string());
            }
            if key == "--config-file" {
                config_file = Some(val.to_string());
            }
            i += 1;
            continue;
        }

        // If we get here and it's a flag we don't recognise, skip it
        // (clap will handle the error later).
        if arg.starts_with('-') {
            i += 1;
            continue;
        }

        // First non-flag arg = the subcommand
        first_positional_idx = Some(i);
        break;
    }

    let first_positional_idx = match first_positional_idx {
        Some(idx) => idx,
        None => return args, // no subcommand found — let clap handle it
    };

    let subcmd = args[first_positional_idx].as_str();

    // Already a known top-level / explicit prefix → pass through
    if PASSTHROUGH_COMMANDS.contains(&subcmd) {
        return args;
    }

    // Unambiguous cloud-only command
    if CLOUD_ONLY_COMMANDS.contains(&subcmd) {
        let mut new_args = args[..first_positional_idx].to_vec();
        new_args.push("cloud".to_string());
        new_args.extend_from_slice(&args[first_positional_idx..]);
        return new_args;
    }

    // Unambiguous enterprise-only command
    if ENTERPRISE_ONLY_COMMANDS.contains(&subcmd) {
        let mut new_args = args[..first_positional_idx].to_vec();
        new_args.push("enterprise".to_string());
        new_args.extend_from_slice(&args[first_positional_idx..]);
        return new_args;
    }

    // Shared command — need to resolve from profile config
    if SHARED_COMMANDS.contains(&subcmd) {
        // If --help is present, show a helpful message about needing a platform
        if has_help {
            // Try to resolve, but if we can't, give guidance
            let config = load_config_for_prefix(config_file.as_deref());
            if let Some(config) = config
                && let Ok(deployment) =
                    config.resolve_profile_deployment(explicit_profile.as_deref())
            {
                let prefix = match deployment {
                    DeploymentType::Cloud => "cloud",
                    DeploymentType::Enterprise => "enterprise",
                    _ => return args,
                };
                let mut new_args = args[..first_positional_idx].to_vec();
                new_args.push(prefix.to_string());
                new_args.extend_from_slice(&args[first_positional_idx..]);
                return new_args;
            }
            // Can't resolve — print guidance and exit
            error::CliDiagnostic::error(&format!("cannot determine platform for '{}'", subcmd))
                .detail("This command exists in both cloud and enterprise.")
                .tip(
                    "specify the platform:",
                    &[
                        &format!("redisctl cloud {} --help", subcmd),
                        &format!("redisctl enterprise {} --help", subcmd),
                    ],
                )
                .tip(
                    "or configure a profile so the platform is inferred automatically:",
                    &[
                        "redisctl profile set <name> --type cloud ...",
                        "redisctl profile set <name> --type enterprise ...",
                    ],
                )
                .print();
            std::process::exit(0);
        }

        let config = load_config_for_prefix(config_file.as_deref());
        match config {
            Some(config) => {
                match config.resolve_profile_deployment(explicit_profile.as_deref()) {
                    Ok(deployment) => {
                        let prefix = match deployment {
                            DeploymentType::Cloud => "cloud",
                            DeploymentType::Enterprise => "enterprise",
                            _ => return args, // Database profiles don't apply
                        };
                        let mut new_args = args[..first_positional_idx].to_vec();
                        new_args.push(prefix.to_string());
                        new_args.extend_from_slice(&args[first_positional_idx..]);
                        new_args
                    }
                    Err(e) => {
                        match &e {
                            ConfigError::AmbiguousDeployment => {
                                error::CliDiagnostic::error(&format!(
                                    "cannot determine platform for '{}'",
                                    subcmd
                                ))
                                .detail("You have both cloud and enterprise profiles.")
                                .tip(
                                    "be explicit about the platform:",
                                    &[
                                        &format!("redisctl cloud {} list", subcmd),
                                        &format!("redisctl enterprise {} list", subcmd),
                                    ],
                                )
                                .tip(
                                    "or specify a profile:",
                                    &[&format!("redisctl {} list --profile <name>", subcmd)],
                                )
                                .print();
                            }
                            ConfigError::ProfileNotFound { name } => {
                                error::CliDiagnostic::error(&format!(
                                    "profile '{}' not found",
                                    name
                                ))
                                .tip("list available profiles:", &["redisctl profile list"])
                                .tip(
                                    "create a new profile:",
                                    &[&format!(
                                        "redisctl profile set {} --type cloud --api-key KEY --api-secret SECRET",
                                        name
                                    )],
                                )
                                .print();
                            }
                            _ => {
                                error::CliDiagnostic::error(&format!("{}", e)).print();
                            }
                        }
                        std::process::exit(1);
                    }
                }
            }
            None => {
                error::CliDiagnostic::error(&format!(
                    "cannot determine platform for '{}'",
                    subcmd
                ))
                .detail("No configuration found and this command exists in both cloud and enterprise.")
                .tip(
                    "be explicit about the platform:",
                    &[
                        &format!("redisctl cloud {} list", subcmd),
                        &format!("redisctl enterprise {} list", subcmd),
                    ],
                )
                .tip(
                    "create a profile so the platform is inferred automatically:",
                    &[
                        "redisctl profile set <name> --type cloud --api-key KEY --api-secret SECRET",
                        "redisctl profile set <name> --type enterprise --url URL --username USER",
                    ],
                )
                .print();
                std::process::exit(1);
            }
        }
    } else {
        // Unknown command — pass through and let clap produce its error
        args
    }
}

/// Resolve `@file` references in the `--query` flag.
///
/// If the query starts with `@`, treat the remainder as a file path and read
/// its contents (trimming leading/trailing whitespace). Otherwise, pass through.
fn resolve_query(query: Option<String>) -> Result<Option<String>> {
    match query {
        Some(q) if q.starts_with('@') => {
            let path = &q[1..];
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read query file: {path}"))?;
            Ok(Some(contents.trim().to_string()))
        }
        other => Ok(other),
    }
}

/// Try to load config for the prefix-inference layer. Returns None on any error.
fn load_config_for_prefix(config_file: Option<&str>) -> Option<Config> {
    if let Some(path) = config_file {
        Config::load_from_path(std::path::Path::new(path)).ok()
    } else {
        Config::load().ok()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    clap_complete::CompleteEnv::with_factory(cli::Cli::command).complete();

    let args: Vec<String> = std::env::args().collect();
    let args = maybe_inject_prefix(args);
    let mut cli = Cli::parse_from(args);
    cli.query = resolve_query(cli.query)?;

    // Initialize tracing based on verbosity level
    init_tracing(cli.verbose);

    // Load configuration from specified path or default location
    let (config, config_path) = if let Some(config_file) = &cli.config_file {
        let path = std::path::PathBuf::from(config_file);
        debug!("Loading config from explicit path: {:?}", path);
        let config = Config::load_from_path(&path)?;
        (config, Some(path))
    } else {
        debug!("Loading config from default location");
        (Config::load()?, None)
    };
    debug!(
        "Creating ConnectionManager with config_path: {:?}",
        config_path
    );
    let conn_mgr = ConnectionManager::with_config_path(config, config_path);

    // Execute command
    if let Err(e) = execute_command(&cli, &conn_mgr).await {
        e.print_diagnostic();
        std::process::exit(1);
    }

    Ok(())
}

fn init_tracing(verbose: u8) {
    // Check for RUST_LOG env var first, then fall back to verbosity flag
    let filter = if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::EnvFilter::from_default_env()
    } else {
        let level = match verbose {
            0 => "redisctl=warn,redis_cloud=warn,redis_enterprise=warn",
            1 => "redisctl=info,redis_cloud=info,redis_enterprise=info",
            2 => "redisctl=debug,redis_cloud=debug,redis_enterprise=debug",
            _ => "redisctl=trace,redis_cloud=trace,redis_enterprise=trace",
        };
        tracing_subscriber::EnvFilter::new(level)
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(
            // Log to stderr so structured output (-o json/yaml) on stdout is
            // never corrupted, and only colorize when stderr is a terminal.
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(std::io::stderr().is_terminal())
                .with_target(true)
                .with_thread_ids(false)
                .with_thread_names(false)
                .compact(),
        )
        .init();

    debug!("Tracing initialized with verbosity level: {}", verbose);
}

async fn execute_command(cli: &Cli, conn_mgr: &ConnectionManager) -> Result<(), RedisCtlError> {
    // Log command execution with sanitized parameters
    trace!("Executing command: {:?}", cli.command);
    info!("Command: {}", format_command(&cli.command));

    let start = std::time::Instant::now();
    let result = match &cli.command {
        Commands::Version => {
            debug!("Showing version information");
            match cli.output {
                cli::OutputFormat::Json | cli::OutputFormat::Yaml => {
                    let output_data = serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "name": env!("CARGO_PKG_NAME"),
                    });

                    crate::output::print_output(&output_data, cli.output, None)?;
                }
                _ => {
                    println!("redisctl {}", env!("CARGO_PKG_VERSION"));
                }
            }
            Ok(())
        }
        Commands::Completions { shell, register } => {
            if *register {
                debug!("Printing registration command for {:?}", shell);
                print_registration_command(*shell);
            } else {
                debug!("Generating completions for {:?}", shell);
                generate_completions(*shell);
            }
            Ok(())
        }

        Commands::Profile(profile_cmd) => {
            debug!("Executing profile command");
            commands::profile::handle_profile_command(profile_cmd, conn_mgr, cli.output).await
        }

        Commands::FilesKey(files_key_cmd) => {
            debug!("Executing files-key command");
            execute_files_key_command(files_key_cmd).await
        }

        Commands::Api {
            deployment,
            method,
            path,
            data,
            curl,
        } => {
            info!(
                "API call: {} {} {} (deployment: {:?})",
                method,
                path,
                if data.is_some() {
                    "with data"
                } else {
                    "no data"
                },
                deployment
            );
            execute_api_command(
                cli,
                conn_mgr,
                deployment,
                method,
                path,
                data.as_deref(),
                *curl,
            )
            .await
        }

        Commands::Cloud(cloud_cmd) => execute_cloud_command(cli, conn_mgr, cloud_cmd).await,

        Commands::Enterprise(enterprise_cmd) => {
            execute_enterprise_command(
                enterprise_cmd,
                conn_mgr,
                cli.profile.as_deref(),
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }

        Commands::Db(db_cmd) => commands::db::handle_db_command(db_cmd, conn_mgr, cli.output).await,
    };

    let duration = start.elapsed();
    match &result {
        Ok(_) => info!("Command completed successfully in {:?}", duration),
        // debug, not error: the stderr diagnostic (print_diagnostic) is the one
        // user-facing failure message. Logging at error here duplicated it.
        Err(e) => debug!("Command failed after {:?}: {}", duration, e),
    }

    result
}

/// Generate shell completions
fn generate_completions(shell: cli::Shell) {
    let mut cmd = cli::Cli::command();
    let name = cmd.get_name().to_string();

    match shell {
        cli::Shell::Bash => generate(shells::Bash, &mut cmd, name, &mut std::io::stdout()),
        cli::Shell::Zsh => generate(shells::Zsh, &mut cmd, name, &mut std::io::stdout()),
        cli::Shell::Fish => generate(shells::Fish, &mut cmd, name, &mut std::io::stdout()),
        cli::Shell::PowerShell => {
            generate(shells::PowerShell, &mut cmd, name, &mut std::io::stdout())
        }
        cli::Shell::Elvish => generate(shells::Elvish, &mut cmd, name, &mut std::io::stdout()),
    }
}

/// Print the shell command to register dynamic completions
fn print_registration_command(shell: cli::Shell) {
    let cmd = match shell {
        cli::Shell::Bash => "source <(COMPLETE=bash redisctl)",
        cli::Shell::Zsh => "source <(COMPLETE=zsh redisctl)",
        cli::Shell::Fish => "source (COMPLETE=fish redisctl | psub)",
        cli::Shell::PowerShell => "COMPLETE=powershell redisctl | Invoke-Expression",
        cli::Shell::Elvish => "eval (E:COMPLETE=elvish redisctl)",
    };
    println!("{cmd}");
}

/// Format command for human-readable logging (without sensitive data)
fn format_command(command: &Commands) -> String {
    match command {
        Commands::Version => "version".to_string(),
        Commands::Completions { shell, register } => {
            if *register {
                format!("completions {:?} --register", shell)
            } else {
                format!("completions {:?}", shell)
            }
        }
        Commands::Profile(cmd) => {
            use cli::ProfileCommands::*;
            match cmd {
                List { .. } => "profile list".to_string(),
                Path => "profile path".to_string(),
                Current { r#type } => format!("profile current --type {}", r#type),
                Show { name } => format!("profile show {}", name),
                Set { name, .. } => format!("profile set {} [credentials redacted]", name),
                Remove { name } => format!("profile remove {}", name),
                DefaultEnterprise { name } => format!("profile default-enterprise {}", name),
                DefaultCloud { name } => format!("profile default-cloud {}", name),
                DefaultDatabase { name } => format!("profile default-database {}", name),
                Validate { connect } => {
                    if *connect {
                        "profile validate --connect".to_string()
                    } else {
                        "profile validate".to_string()
                    }
                }
                Init => "profile init".to_string(),
            }
        }
        Commands::Api {
            deployment,
            method,
            path,
            ..
        } => {
            format!("api {:?} {} {}", deployment, method, path)
        }
        Commands::Cloud(cmd) => format!("cloud {:?}", cmd),
        Commands::Enterprise(cmd) => format!("enterprise {:?}", cmd),
        Commands::FilesKey(cmd) => {
            use cli::FilesKeyCommands::*;
            match cmd {
                Set { .. } => "files-key set [key redacted]".to_string(),
                Get { profile } => format!("files-key get {:?}", profile),
                Remove { .. } => "files-key remove".to_string(),
            }
        }
        Commands::Db(cmd) => {
            use cli::DbCommands::*;
            match cmd {
                Open { profile, .. } => format!("db open --profile {}", profile),
            }
        }
    }
}

async fn execute_enterprise_command(
    enterprise_cmd: &cli::EnterpriseCommands,
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
    output: cli::OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    use cli::EnterpriseCommands::*;

    match enterprise_cmd {
        Action(action_cmd) => {
            commands::enterprise::actions::handle_action_command(
                conn_mgr,
                profile,
                action_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Alerts(alerts_cmd) => alerts_cmd
            .execute(&conn_mgr.config, profile, output, query)
            .await
            .map_err(RedisCtlError::from),
        BdbGroup(bdb_group_cmd) => {
            commands::enterprise::bdb_group::handle_bdb_group_command(
                conn_mgr,
                profile,
                bdb_group_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Cluster(cluster_cmd) => {
            commands::enterprise::cluster::handle_cluster_command(
                conn_mgr,
                profile,
                cluster_cmd,
                output,
                query,
            )
            .await
        }
        CmSettings(cm_settings_cmd) => {
            commands::enterprise::cm_settings::handle_cm_settings_command(
                conn_mgr,
                profile,
                cm_settings_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Database(db_cmd) => {
            commands::enterprise::database::handle_database_command(
                conn_mgr, profile, db_cmd, output, query,
            )
            .await
        }
        DebugInfo(debuginfo_cmd) => {
            commands::enterprise::debuginfo::handle_debuginfo_command(
                conn_mgr,
                profile,
                debuginfo_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Diagnostics(diagnostics_cmd) => {
            commands::enterprise::diagnostics::handle_diagnostics_command(
                conn_mgr,
                profile,
                diagnostics_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Endpoint(endpoint_cmd) => {
            commands::enterprise::endpoint::handle_endpoint_command(
                conn_mgr,
                profile,
                endpoint_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Node(node_cmd) => {
            commands::enterprise::node::handle_node_command(
                conn_mgr, profile, node_cmd, output, query,
            )
            .await
        }
        Proxy(proxy_cmd) => {
            commands::enterprise::proxy::handle_proxy_command(
                conn_mgr,
                profile,
                proxy_cmd.clone(),
                output,
                query,
            )
            .await
        }
        User(user_cmd) => {
            commands::enterprise::rbac::handle_user_command(
                conn_mgr, profile, user_cmd, output, query,
            )
            .await
        }
        Role(role_cmd) => {
            commands::enterprise::rbac::handle_role_command(
                conn_mgr, profile, role_cmd, output, query,
            )
            .await
        }
        Acl(acl_cmd) => {
            commands::enterprise::rbac::handle_acl_command(
                conn_mgr, profile, acl_cmd, output, query,
            )
            .await
        }
        Ldap(ldap_cmd) => {
            commands::enterprise::ldap::handle_ldap_command(
                conn_mgr,
                profile,
                ldap_cmd.clone(),
                output,
                query,
            )
            .await
        }
        LdapMappings(ldap_mappings_cmd) => {
            commands::enterprise::ldap::handle_ldap_mappings_command(
                conn_mgr,
                profile,
                ldap_mappings_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Auth(auth_cmd) => {
            commands::enterprise::rbac::handle_auth_command(
                conn_mgr, profile, auth_cmd, output, query,
            )
            .await
        }
        Bootstrap(bootstrap_cmd) => {
            commands::enterprise::bootstrap::handle_bootstrap_command(
                conn_mgr,
                profile,
                bootstrap_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Crdb(crdb_cmd) => {
            commands::enterprise::crdb::handle_crdb_command(
                conn_mgr, profile, crdb_cmd, output, query,
            )
            .await
        }
        CrdbTask(crdb_task_cmd) => {
            commands::enterprise::crdb_task::handle_crdb_task_command(
                conn_mgr,
                profile,
                crdb_task_cmd.clone(),
                output,
                query,
            )
            .await
        }
        JobScheduler(job_scheduler_cmd) => {
            commands::enterprise::job_scheduler::handle_job_scheduler_command(
                conn_mgr,
                profile,
                job_scheduler_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Jsonschema(jsonschema_cmd) => {
            commands::enterprise::jsonschema::handle_jsonschema_command(
                conn_mgr,
                profile,
                jsonschema_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Logs(logs_cmd) => {
            commands::enterprise::logs_impl::handle_logs_commands(
                conn_mgr, profile, logs_cmd, output, query,
            )
            .await
        }
        License(license_cmd) => license_cmd
            .execute(&conn_mgr.config, profile, output, query)
            .await
            .map_err(RedisCtlError::from),
        Migration(migration_cmd) => {
            commands::enterprise::migration::handle_migration_command(
                conn_mgr,
                profile,
                migration_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Module(module_cmd) => {
            commands::enterprise::module_impl::handle_module_commands(
                conn_mgr, profile, module_cmd, output, query,
            )
            .await
        }
        Ocsp(ocsp_cmd) => {
            commands::enterprise::ocsp::handle_ocsp_command(
                conn_mgr,
                profile,
                ocsp_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Services(services_cmd) => {
            commands::enterprise::services::handle_services_command(
                conn_mgr,
                profile,
                services_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Workflow(workflow_cmd) => {
            handle_enterprise_workflow_command(conn_mgr, profile, workflow_cmd, output).await
        }
        Local(local_cmd) => {
            commands::enterprise::local::handle_local_command(
                conn_mgr,
                profile,
                local_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Shard(shard_cmd) => {
            commands::enterprise::shard::handle_shard_command(
                conn_mgr,
                profile,
                shard_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Stats(stats_cmd) => {
            commands::enterprise::stats::handle_stats_command(
                conn_mgr, profile, stats_cmd, output, query,
            )
            .await
        }
        Status {
            cluster,
            nodes,
            databases,
            shards,
            brief,
        } => {
            let sections = commands::enterprise::status::StatusSections {
                cluster: *cluster,
                nodes: *nodes,
                databases: *databases,
                shards: *shards,
            };
            commands::enterprise::status::get_status(
                conn_mgr, profile, sections, *brief, output, query,
            )
            .await
        }
        SupportPackage(support_cmd) => {
            commands::enterprise::support_package::handle_support_package_command(
                conn_mgr,
                profile,
                support_cmd.clone(),
                output,
                query,
            )
            .await
        }
        Suffix(suffix_cmd) => {
            commands::enterprise::suffix::handle_suffix_command(
                conn_mgr,
                profile,
                suffix_cmd.clone(),
                output,
                query,
            )
            .await
        }
        UsageReport(usage_report_cmd) => {
            commands::enterprise::usage_report::handle_usage_report_command(
                conn_mgr,
                profile,
                usage_report_cmd.clone(),
                output,
                query,
            )
            .await
        }
    }
}

async fn handle_cloud_workflow_command(
    conn_mgr: &ConnectionManager,
    cli: &cli::Cli,
    workflow_cmd: &cli::CloudWorkflowCommands,
) -> Result<(), RedisCtlError> {
    use cli::CloudWorkflowCommands::*;
    use workflows::{WorkflowArgs, WorkflowContext, WorkflowRegistry};

    let output = cli.output;
    let profile = cli.profile.as_deref();

    match workflow_cmd {
        List => {
            let registry = WorkflowRegistry::new();
            let workflows = registry.list();

            // Filter to show only cloud workflows
            let cloud_workflows: Vec<_> = workflows
                .into_iter()
                .filter(|(name, _)| name.contains("subscription") || name.contains("cloud"))
                .collect();

            match output {
                cli::OutputFormat::Json | cli::OutputFormat::Yaml => {
                    let workflow_list: Vec<serde_json::Value> = cloud_workflows
                        .into_iter()
                        .map(|(name, description)| {
                            serde_json::json!({
                                "name": name,
                                "description": description
                            })
                        })
                        .collect();
                    crate::output::print_output(serde_json::json!(workflow_list), output, None)?;
                }
                _ => {
                    println!("Available Cloud Workflows:");
                    println!();
                    for (name, description) in cloud_workflows {
                        println!("  {} - {}", name, description);
                    }
                }
            }
            Ok(())
        }
        SubscriptionSetup(args) => {
            let mut workflow_args = WorkflowArgs::new();
            workflow_args.insert("args", args);

            let context = WorkflowContext {
                conn_mgr: conn_mgr.clone(),
                profile_name: profile.map(String::from),
                output_format: output,
                wait_timeout: args.wait_timeout as u64,
            };

            let registry = WorkflowRegistry::new();
            let workflow =
                registry
                    .get("subscription-setup")
                    .ok_or_else(|| RedisCtlError::ApiError {
                        message: "Workflow not found".to_string(),
                    })?;

            let result = workflow
                .execute(context, workflow_args)
                .await
                .map_err(|e| RedisCtlError::ApiError {
                    message: e.to_string(),
                })?;

            if !result.success {
                return Err(RedisCtlError::ApiError {
                    message: result.message,
                });
            }

            // Print result as JSON/YAML if requested
            match output {
                cli::OutputFormat::Json | cli::OutputFormat::Yaml => {
                    let result_json = serde_json::json!({
                        "success": result.success,
                        "message": result.message,
                        "outputs": result.outputs,
                    });
                    crate::output::print_output(&result_json, output, None)?;
                }
                _ => {
                    // Human output
                    println!("{}", result.message);
                }
            }

            Ok(())
        }
    }
}

async fn handle_enterprise_workflow_command(
    conn_mgr: &ConnectionManager,
    profile: Option<&str>,
    workflow_cmd: &cli::EnterpriseWorkflowCommands,
    output: cli::OutputFormat,
) -> Result<(), RedisCtlError> {
    use cli::EnterpriseWorkflowCommands::*;
    use workflows::{WorkflowArgs, WorkflowContext, WorkflowRegistry};

    match workflow_cmd {
        List => {
            let registry = WorkflowRegistry::new();
            let workflows = registry.list();

            match output {
                cli::OutputFormat::Json | cli::OutputFormat::Yaml => {
                    let workflow_list: Vec<serde_json::Value> = workflows
                        .into_iter()
                        .map(|(name, description)| {
                            serde_json::json!({
                                "name": name,
                                "description": description
                            })
                        })
                        .collect();
                    crate::output::print_output(serde_json::json!(workflow_list), output, None)?;
                }
                _ => {
                    println!("Available Enterprise Workflows:");
                    println!();
                    for (name, description) in workflows {
                        println!("  {} - {}", name, description);
                    }
                }
            }
            Ok(())
        }
        License(license_workflow_cmd) => license_workflow_cmd
            .execute(&conn_mgr.config, output, None)
            .await
            .map_err(RedisCtlError::from),
        InitCluster {
            name,
            username,
            password,
            skip_database,
            database_name,
            database_memory_gb,
            async_ops,
        } => {
            let mut args = WorkflowArgs::new();
            args.insert("name", name);
            args.insert("username", username);
            args.insert("password", password);
            args.insert("create_database", !skip_database);
            args.insert("database_name", database_name);
            args.insert("database_memory_gb", database_memory_gb);

            let context = WorkflowContext {
                conn_mgr: conn_mgr.clone(),
                profile_name: profile.map(String::from),
                output_format: output,
                wait_timeout: if async_ops.wait {
                    async_ops.wait_timeout
                } else {
                    0
                },
            };

            let registry = WorkflowRegistry::new();
            let workflow = registry
                .get("init-cluster")
                .ok_or_else(|| RedisCtlError::ApiError {
                    message: "Workflow not found".to_string(),
                })?;

            let result =
                workflow
                    .execute(context, args)
                    .await
                    .map_err(|e| RedisCtlError::ApiError {
                        message: e.to_string(),
                    })?;

            if !result.success {
                return Err(RedisCtlError::ApiError {
                    message: result.message,
                });
            }

            // Print result as JSON/YAML if requested
            match output {
                cli::OutputFormat::Json | cli::OutputFormat::Yaml => {
                    let result_json = serde_json::json!({
                        "success": result.success,
                        "message": result.message,
                        "outputs": result.outputs,
                    });
                    crate::output::print_output(&result_json, output, None)?;
                }
                _ => {
                    // Human output was already printed by the workflow
                }
            }

            Ok(())
        }
    }
}

async fn execute_files_key_command(
    files_key_cmd: &cli::FilesKeyCommands,
) -> Result<(), RedisCtlError> {
    use cli::FilesKeyCommands::*;

    match files_key_cmd {
        Set {
            api_key,
            #[cfg(feature = "secure-storage")]
            use_keyring,
            global,
            profile,
        } => commands::files_key::handle_set(
            api_key.clone(),
            #[cfg(feature = "secure-storage")]
            *use_keyring,
            *global,
            profile.clone(),
        )
        .await
        .map_err(RedisCtlError::from),
        Get { profile } => commands::files_key::handle_get(profile.clone())
            .await
            .map_err(RedisCtlError::from),
        Remove {
            #[cfg(feature = "secure-storage")]
            keyring,
            global,
            profile,
        } => commands::files_key::handle_remove(
            #[cfg(feature = "secure-storage")]
            *keyring,
            *global,
            profile.clone(),
        )
        .await
        .map_err(RedisCtlError::from),
    }
}

async fn execute_api_command(
    cli: &Cli,
    conn_mgr: &ConnectionManager,
    deployment: &redisctl_core::DeploymentType,
    method: &cli::HttpMethod,
    path: &str,
    data: Option<&str>,
    curl: bool,
) -> Result<(), RedisCtlError> {
    commands::api::handle_api_command(commands::api::ApiCommandParams {
        config: conn_mgr.config.clone(),
        config_path: conn_mgr.config_path.clone(),
        profile_name: cli.profile.clone(),
        deployment: *deployment,
        method: method.clone(),
        path: path.to_string(),
        data: data.map(|s| s.to_string()),
        query: cli.query.clone(),
        output_format: cli.output,
        curl,
    })
    .await
}

async fn execute_cloud_command(
    cli: &Cli,
    conn_mgr: &ConnectionManager,
    cloud_cmd: &cli::CloudCommands,
) -> Result<(), RedisCtlError> {
    use cli::CloudCommands::*;

    match cloud_cmd {
        Account(account_cmd) => {
            commands::cloud::handle_account_command(
                conn_mgr,
                cli.profile.as_deref(),
                account_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }

        PaymentMethod(payment_method_cmd) => {
            commands::cloud::handle_payment_method_command(
                conn_mgr,
                cli.profile.as_deref(),
                payment_method_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }

        Subscription(sub_cmd) => {
            commands::cloud::handle_subscription_command(
                conn_mgr,
                cli.profile.as_deref(),
                sub_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }

        Database(db_cmd) => {
            commands::cloud::handle_database_command(
                conn_mgr,
                cli.profile.as_deref(),
                db_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }

        User(user_cmd) => {
            commands::cloud::handle_user_command(
                conn_mgr,
                cli.profile.as_deref(),
                user_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }
        Acl(acl_cmd) => {
            commands::cloud::acl::handle_acl_command(
                conn_mgr,
                cli.profile.as_deref(),
                acl_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }
        ProviderAccount(provider_account_cmd) => {
            commands::cloud::cloud_account::handle_cloud_account_command(
                conn_mgr,
                cli.profile.as_deref(),
                provider_account_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }
        Task(task_cmd) => {
            commands::cloud::task::handle_task_command(
                conn_mgr,
                cli.profile.as_deref(),
                task_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }
        Connectivity(connectivity_cmd) => {
            commands::cloud::connectivity::handle_connectivity_command(
                conn_mgr,
                cli.profile.as_deref(),
                connectivity_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }
        FixedDatabase(fixed_db_cmd) => {
            commands::cloud::fixed_database::handle_fixed_database_command(
                conn_mgr,
                cli.profile.as_deref(),
                fixed_db_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }
        FixedSubscription(fixed_sub_cmd) => {
            commands::cloud::fixed_subscription::handle_fixed_subscription_command(
                conn_mgr,
                cli.profile.as_deref(),
                fixed_sub_cmd,
                cli.output,
                cli.query.as_deref(),
            )
            .await
        }
        Workflow(workflow_cmd) => handle_cloud_workflow_command(conn_mgr, cli, workflow_cmd).await,
        CostReport(cost_report_cmd) => {
            commands::cloud::cost_report::handle_cost_report_command(
                conn_mgr,
                cli.profile.as_deref(),
                cost_report_cmd.clone(),
                cli.output,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    // --- Passthrough tests ---

    #[test]
    fn passthrough_explicit_cloud() {
        let input = args("redisctl cloud database list");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_explicit_enterprise() {
        let input = args("redisctl enterprise cluster get");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_cloud_alias() {
        let input = args("redisctl cl database list");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_enterprise_alias_ent() {
        let input = args("redisctl ent cluster get");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_enterprise_alias_en() {
        let input = args("redisctl en cluster get");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_profile() {
        let input = args("redisctl profile list");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_profile_alias() {
        let input = args("redisctl prof list");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_version() {
        let input = args("redisctl version");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_help() {
        let input = args("redisctl help");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_no_subcommand() {
        let input = args("redisctl --help");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    #[test]
    fn passthrough_no_args() {
        let input = args("redisctl");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    // --- Cloud-only injection ---

    #[test]
    fn inject_cloud_subscription() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl subscription list")),
            args("redisctl cloud subscription list")
        );
    }

    #[test]
    fn inject_cloud_account() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl account list")),
            args("redisctl cloud account list")
        );
    }

    #[test]
    fn inject_cloud_payment_method() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl payment-method list")),
            args("redisctl cloud payment-method list")
        );
    }

    #[test]
    fn inject_cloud_task() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl task list")),
            args("redisctl cloud task list")
        );
    }

    #[test]
    fn inject_cloud_connectivity() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl connectivity list")),
            args("redisctl cloud connectivity list")
        );
    }

    #[test]
    fn inject_cloud_fixed_database() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl fixed-database list")),
            args("redisctl cloud fixed-database list")
        );
    }

    #[test]
    fn inject_cloud_cost_report() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl cost-report list")),
            args("redisctl cloud cost-report list")
        );
    }

    // --- Enterprise-only injection ---

    #[test]
    fn inject_enterprise_cluster() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl cluster get")),
            args("redisctl enterprise cluster get")
        );
    }

    #[test]
    fn inject_enterprise_node() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl node list")),
            args("redisctl enterprise node list")
        );
    }

    #[test]
    fn inject_enterprise_shard() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl shard list")),
            args("redisctl enterprise shard list")
        );
    }

    #[test]
    fn inject_enterprise_module() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl module list")),
            args("redisctl enterprise module list")
        );
    }

    #[test]
    fn inject_enterprise_status() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl status")),
            args("redisctl enterprise status")
        );
    }

    // --- Global flags in various positions ---

    #[test]
    fn inject_cloud_with_profile_flag() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl -p myprofile subscription list")),
            args("redisctl -p myprofile cloud subscription list")
        );
    }

    #[test]
    fn inject_enterprise_with_verbose() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl -vvv cluster get")),
            args("redisctl -vvv enterprise cluster get")
        );
    }

    #[test]
    fn inject_with_output_flag() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl -o json subscription list")),
            args("redisctl -o json cloud subscription list")
        );
    }

    #[test]
    fn inject_with_long_profile() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl --profile mycloud subscription list")),
            args("redisctl --profile mycloud cloud subscription list")
        );
    }

    #[test]
    fn inject_with_equals_profile() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl --profile=mycloud subscription list")),
            args("redisctl --profile=mycloud cloud subscription list")
        );
    }

    #[test]
    fn inject_enterprise_with_multiple_flags() {
        assert_eq!(
            maybe_inject_prefix(args("redisctl -p myent -o json --verbose cluster get")),
            args("redisctl -p myent -o json --verbose enterprise cluster get")
        );
    }

    // --- Unknown command passes through ---

    #[test]
    fn unknown_command_passthrough() {
        let input = args("redisctl foobar baz");
        assert_eq!(maybe_inject_prefix(input.clone()), input);
    }

    // --- resolve_query tests ---

    #[test]
    fn resolve_query_none_passthrough() {
        assert_eq!(resolve_query(None).unwrap(), None);
    }

    #[test]
    fn resolve_query_inline_passthrough() {
        let q = Some("databases[?status==`active`]".to_string());
        assert_eq!(
            resolve_query(q).unwrap().as_deref(),
            Some("databases[?status==`active`]")
        );
    }

    #[test]
    fn resolve_query_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("query.jmespath");
        std::fs::write(&path, "databases[?status==`active`]").unwrap();

        let q = Some(format!("@{}", path.display()));
        assert_eq!(
            resolve_query(q).unwrap().as_deref(),
            Some("databases[?status==`active`]")
        );
    }

    #[test]
    fn resolve_query_trims_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("query.jmespath");
        std::fs::write(&path, "  databases[?status==`active`]  \n").unwrap();

        let q = Some(format!("@{}", path.display()));
        assert_eq!(
            resolve_query(q).unwrap().as_deref(),
            Some("databases[?status==`active`]")
        );
    }

    #[test]
    fn resolve_query_file_not_found() {
        let q = Some("@nonexistent/path/query.jmespath".to_string());
        let err = resolve_query(q).unwrap_err();
        assert!(
            err.to_string()
                .contains("Failed to read query file: nonexistent/path/query.jmespath"),
            "unexpected error: {err}"
        );
    }
}
