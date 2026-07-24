//! Database command implementations

use crate::cli::{DbCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;
use redisctl_core::DeploymentType;
use std::process::Command;
use tracing::debug;

/// Handle database commands
pub async fn handle_db_command(
    db_cmd: &DbCommands,
    conn_mgr: &ConnectionManager,
    _output: OutputFormat,
) -> Result<(), RedisCtlError> {
    match db_cmd {
        DbCommands::Open {
            profile,
            dry_run,
            redis_cli,
            args,
        } => handle_open(conn_mgr, profile, *dry_run, redis_cli, args).await,
    }
}

/// Handle the 'db open' command - spawn redis-cli with profile credentials
async fn handle_open(
    conn_mgr: &ConnectionManager,
    profile_name: &str,
    dry_run: bool,
    redis_cli_path: &str,
    extra_args: &[String],
) -> Result<(), RedisCtlError> {
    // Get the profile
    let profile = conn_mgr.config.profiles.get(profile_name).ok_or_else(|| {
        RedisCtlError::Configuration(format!("Profile '{}' not found", profile_name))
    })?;

    // Verify it's a database profile
    if profile.deployment_type != DeploymentType::Database {
        return Err(RedisCtlError::Configuration(format!(
            "Profile '{}' is not a database profile (type: {})",
            profile_name, profile.deployment_type
        )));
    }

    // Get resolved credentials
    let (host, port, password, tls, username, _database) = profile
        .resolve_database_credentials()
        .map_err(|e| RedisCtlError::Configuration(format!("Failed to resolve credentials: {}", e)))?
        .ok_or_else(|| {
            RedisCtlError::Configuration(format!(
                "Profile '{}' has no database credentials",
                profile_name
            ))
        })?;

    // Build redis-cli arguments
    let mut cli_args = vec![
        "-h".to_string(),
        host.clone(),
        "-p".to_string(),
        port.to_string(),
    ];

    // Add username if not default
    if username != "default" {
        cli_args.push("--user".to_string());
        cli_args.push(username.clone());
    }

    // Add password if present
    if let Some(ref pwd) = password {
        cli_args.push("-a".to_string());
        cli_args.push(pwd.clone());
    }

    // Add TLS if enabled
    if tls {
        cli_args.push("--tls".to_string());
    }

    // Add any extra arguments
    cli_args.extend(extra_args.iter().cloned());

    debug!(
        "redis-cli args (password redacted): {:?}",
        cli_args
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                // Redact password value (comes after -a)
                if i > 0 && cli_args[i - 1] == "-a" {
                    "***".to_string()
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
    );

    if dry_run {
        // Print the command (with password redacted for safety)
        let display_args: Vec<String> = cli_args
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                if i > 0 && cli_args[i - 1] == "-a" {
                    "***".to_string()
                } else {
                    // Quote args with spaces
                    if arg.contains(' ') {
                        format!("\"{}\"", arg)
                    } else {
                        arg.clone()
                    }
                }
            })
            .collect();

        println!("{} {}", redis_cli_path, display_args.join(" "));
        return Ok(());
    }

    // Check if redis-cli exists
    if which::which(redis_cli_path).is_err() {
        return Err(RedisCtlError::Configuration(format!(
            "redis-cli not found at '{}'. Install Redis or specify --redis-cli path",
            redis_cli_path
        )));
    }

    // Execute redis-cli
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // On Unix, use exec to replace the current process
        let mut cmd = Command::new(redis_cli_path);
        cmd.args(&cli_args);

        // This replaces the current process and doesn't return on success
        let err = cmd.exec();
        Err(RedisCtlError::Configuration(format!(
            "Failed to exec redis-cli: {}",
            err
        )))
    }

    #[cfg(not(unix))]
    {
        // On Windows, spawn and wait
        let status = Command::new(redis_cli_path)
            .args(&cli_args)
            .status()
            .map_err(|e| {
                RedisCtlError::Configuration(format!("Failed to spawn redis-cli: {}", e))
            })?;

        if !status.success() {
            return Err(RedisCtlError::Configuration(format!(
                "redis-cli exited with status: {}",
                status
            )));
        }

        Ok(())
    }
}
