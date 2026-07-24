use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Clone, Subcommand)]
pub enum DebugInfoCommands {
    /// Collect all debug info (cluster-wide)
    All {
        /// Output file path (saves as tar.gz). If not specified, returns JSON metadata
        #[arg(long, short = 'f')]
        file: Option<PathBuf>,

        /// Use new API endpoint (/v1/cluster/debuginfo) instead of deprecated one
        #[arg(long)]
        use_new_api: bool,
    },

    /// Collect node debug info
    Node {
        /// Specific node UID (optional, all nodes if not specified)
        #[arg(long)]
        node_uid: Option<u32>,

        /// Output file path (saves as tar.gz). If not specified, returns JSON metadata
        #[arg(long, short = 'f')]
        file: Option<PathBuf>,

        /// Use new API endpoint (/v1/nodes/debuginfo) instead of deprecated one
        #[arg(long)]
        use_new_api: bool,
    },

    /// Collect database-specific debug info
    Database {
        /// Database UID
        bdb_uid: u32,

        /// Output file path (saves as tar.gz). If not specified, returns JSON metadata
        #[arg(long, short = 'f')]
        file: Option<PathBuf>,

        /// Use new API endpoint (/v1/bdbs/{uid}/debuginfo) instead of deprecated one
        #[arg(long)]
        use_new_api: bool,
    },
}

pub async fn handle_debuginfo_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: DebugInfoCommands,
    _output_format: OutputFormat,
    _query: Option<&str>,
) -> Result<(), RedisCtlError> {
    match cmd {
        DebugInfoCommands::All { file, use_new_api } => {
            // These endpoints always return binary data, so we need to save to a file
            let output_path = file.unwrap_or_else(|| {
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                std::path::PathBuf::from(format!("support-package-cluster-{}.tar.gz", timestamp))
            });
            handle_debuginfo_all_binary(conn_mgr, profile_name, output_path, use_new_api).await
        }
        DebugInfoCommands::Node {
            node_uid,
            file,
            use_new_api,
        } => {
            // These endpoints always return binary data, so we need to save to a file
            let output_path = file.unwrap_or_else(|| {
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let prefix = if let Some(uid) = node_uid {
                    format!("support-package-node-{}", uid)
                } else {
                    "support-package-nodes".to_string()
                };
                std::path::PathBuf::from(format!("{}-{}.tar.gz", prefix, timestamp))
            });
            handle_debuginfo_node_binary(conn_mgr, profile_name, node_uid, output_path, use_new_api)
                .await
        }
        DebugInfoCommands::Database {
            bdb_uid,
            file,
            use_new_api,
        } => {
            // These endpoints always return binary data, so we need to save to a file
            let output_path = file.unwrap_or_else(|| {
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                std::path::PathBuf::from(format!(
                    "support-package-db-{}-{}.tar.gz",
                    bdb_uid, timestamp
                ))
            });
            handle_debuginfo_database_binary(
                conn_mgr,
                profile_name,
                bdb_uid,
                output_path,
                use_new_api,
            )
            .await
        }
    }
}

// Binary download handlers

async fn handle_debuginfo_all_binary(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_path: PathBuf,
    use_new_api: bool,
) -> Result<(), RedisCtlError> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::fs;

    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Show progress spinner while generating
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message("Generating support package...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // Use new or deprecated endpoint based on flag
    let debuginfo_handler = redis_enterprise::debuginfo::DebugInfoHandler::new(client);
    let data = if use_new_api {
        debuginfo_handler
            .cluster_debuginfo_binary()
            .await
            .map_err(RedisCtlError::from)?
    } else {
        debuginfo_handler
            .all_binary()
            .await
            .map_err(RedisCtlError::from)?
    };

    spinner.finish_and_clear();

    // Save to file
    fs::write(&output_path, &data)
        .context(format!("Failed to save debug info to {:?}", output_path))?;

    // Display success message with file info
    let file_size = data.len();
    let size_display = if file_size > 1_000_000 {
        format!("{:.1} MB", file_size as f64 / 1_048_576.0)
    } else if file_size > 1_000 {
        format!("{:.1} KB", file_size as f64 / 1_024.0)
    } else {
        format!("{} bytes", file_size)
    };

    println!("✓ Support package created successfully");
    println!("  File: {}", output_path.display());
    println!("  Size: {}", size_display);

    Ok(())
}

async fn handle_debuginfo_node_binary(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    node_uid: Option<u32>,
    output_path: PathBuf,
    use_new_api: bool,
) -> Result<(), RedisCtlError> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::fs;

    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Show progress spinner while generating
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message("Generating node support package...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let debuginfo_handler = redis_enterprise::debuginfo::DebugInfoHandler::new(client);
    let data = if let Some(uid) = node_uid {
        // Specific node
        if use_new_api {
            debuginfo_handler
                .node_debuginfo_binary(uid)
                .await
                .context(format!("Failed to collect debug info for node {}", uid))?
        } else {
            // Old API doesn't have specific node endpoint, use general node endpoint
            debuginfo_handler
                .node_binary()
                .await
                .map_err(RedisCtlError::from)?
        }
    } else {
        // All nodes
        if use_new_api {
            debuginfo_handler
                .nodes_debuginfo_binary()
                .await
                .map_err(RedisCtlError::from)?
        } else {
            debuginfo_handler
                .node_binary()
                .await
                .map_err(RedisCtlError::from)?
        }
    };

    spinner.finish_and_clear();

    // Save to file
    fs::write(&output_path, &data)
        .context(format!("Failed to save debug info to {:?}", output_path))?;

    // Display success message with file info
    let file_size = data.len();
    let size_display = if file_size > 1_000_000 {
        format!("{:.1} MB", file_size as f64 / 1_048_576.0)
    } else if file_size > 1_000 {
        format!("{:.1} KB", file_size as f64 / 1_024.0)
    } else {
        format!("{} bytes", file_size)
    };

    println!("✓ Node support package created successfully");
    println!("  File: {}", output_path.display());
    println!("  Size: {}", size_display);

    Ok(())
}

async fn handle_debuginfo_database_binary(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    bdb_uid: u32,
    output_path: PathBuf,
    use_new_api: bool,
) -> Result<(), RedisCtlError> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::fs;

    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Show progress spinner while generating
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message(format!(
        "Generating support package for database {}...",
        bdb_uid
    ));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let debuginfo_handler = redis_enterprise::debuginfo::DebugInfoHandler::new(client);
    let data = if use_new_api {
        debuginfo_handler
            .database_debuginfo_binary(bdb_uid)
            .await
            .context(format!(
                "Failed to collect debug info for database {}",
                bdb_uid
            ))?
    } else {
        debuginfo_handler
            .all_bdb_binary(bdb_uid)
            .await
            .context(format!(
                "Failed to collect debug info for database {}",
                bdb_uid
            ))?
    };

    spinner.finish_and_clear();

    // Save to file
    fs::write(&output_path, &data)
        .context(format!("Failed to save debug info to {:?}", output_path))?;

    // Display success message with file info
    let file_size = data.len();
    let size_display = if file_size > 1_000_000 {
        format!("{:.1} MB", file_size as f64 / 1_048_576.0)
    } else if file_size > 1_000 {
        format!("{:.1} KB", file_size as f64 / 1_024.0)
    } else {
        format!("{} bytes", file_size)
    };

    println!("✓ Database support package created successfully");
    println!("  File: {}", output_path.display());
    println!("  Size: {}", size_display);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debuginfo_commands() {
        use clap::CommandFactory;

        #[derive(clap::Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: DebugInfoCommands,
        }

        TestCli::command().debug_assert();
    }
}
