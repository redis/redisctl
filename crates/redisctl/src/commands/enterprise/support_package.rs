mod optimizer;
#[cfg(feature = "upload")]
mod upload;

use crate::error::RedisCtlError;

use anyhow::{Context, Result as AnyhowResult};
use chrono::Local;
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::OutputFormat;
use crate::commands::cloud::async_utils::AsyncOperationArgs;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use optimizer::{OptimizationOptions, optimize_support_package};

/// Support package generation commands for troubleshooting
#[derive(Subcommand, Debug, Clone)]
pub enum SupportPackageCommands {
    /// Generate full cluster support package
    Cluster {
        /// Output file path (defaults to ./support-package-cluster-{timestamp}.tar.gz)
        #[arg(long = "file", short = 'f')]
        file: Option<PathBuf>,

        /// Skip pre-flight checks
        #[arg(long)]
        skip_checks: bool,

        /// Optimize package size by truncating logs and removing redundant data
        #[arg(long)]
        optimize: bool,

        /// Disable optimization (generate full package)
        #[arg(long, conflicts_with = "optimize")]
        no_optimize: bool,

        /// Maximum number of lines to keep in log files when optimizing (default: 1000)
        #[arg(long, default_value = "1000", requires = "optimize")]
        log_lines: usize,

        /// Show optimization details
        #[arg(long = "optimize-verbose")]
        optimize_verbose: bool,

        /// Upload package to Files.com after generation
        #[cfg(feature = "upload")]
        #[arg(long)]
        upload: bool,

        /// Don't save package locally (only works with --upload)
        #[cfg(feature = "upload")]
        #[arg(long, requires = "upload")]
        no_save: bool,

        /// Async operation options
        #[command(flatten)]
        async_ops: AsyncOperationArgs,
    },

    /// Generate database-specific support package
    Database {
        /// Database UID
        uid: u32,

        /// Output file path (defaults to ./support-package-database-{uid}-{timestamp}.tar.gz)
        #[arg(long = "file", short = 'f')]
        file: Option<PathBuf>,

        /// Skip pre-flight checks
        #[arg(long)]
        skip_checks: bool,

        /// Optimize package size by truncating logs and removing redundant data
        #[arg(long)]
        optimize: bool,

        /// Disable optimization (generate full package)
        #[arg(long, conflicts_with = "optimize")]
        no_optimize: bool,

        /// Maximum number of lines to keep in log files when optimizing (default: 1000)
        #[arg(long, default_value = "1000", requires = "optimize")]
        log_lines: usize,

        /// Show optimization details
        #[arg(long = "optimize-verbose")]
        optimize_verbose: bool,

        /// Upload package to Files.com after generation
        #[cfg(feature = "upload")]
        #[arg(long)]
        upload: bool,

        /// Don't save package locally (only works with --upload)
        #[cfg(feature = "upload")]
        #[arg(long, requires = "upload")]
        no_save: bool,

        /// Async operation options
        #[command(flatten)]
        async_ops: AsyncOperationArgs,
    },

    /// Generate node-specific support package
    Node {
        /// Node UID (optional, all nodes if not specified)
        uid: Option<u32>,

        /// Output file path (defaults to ./support-package-node-{uid}-{timestamp}.tar.gz)
        #[arg(long = "file", short = 'f')]
        file: Option<PathBuf>,

        /// Skip pre-flight checks
        #[arg(long)]
        skip_checks: bool,

        /// Optimize package size by truncating logs and removing redundant data
        #[arg(long)]
        optimize: bool,

        /// Disable optimization (generate full package)
        #[arg(long, conflicts_with = "optimize")]
        no_optimize: bool,

        /// Maximum number of lines to keep in log files when optimizing (default: 1000)
        #[arg(long, default_value = "1000", requires = "optimize")]
        log_lines: usize,

        /// Show optimization details
        #[arg(long = "optimize-verbose")]
        optimize_verbose: bool,

        /// Upload package to Files.com after generation
        #[cfg(feature = "upload")]
        #[arg(long)]
        upload: bool,

        /// Don't save package locally (only works with --upload)
        #[cfg(feature = "upload")]
        #[arg(long, requires = "upload")]
        no_save: bool,

        /// Async operation options
        #[command(flatten)]
        async_ops: AsyncOperationArgs,
    },
}

/// Result structure for JSON output
#[derive(Debug, Serialize, Deserialize)]
pub struct SupportPackageResult {
    pub success: bool,
    pub package_type: String,
    pub file_path: String,
    pub file_size: usize,
    pub file_size_display: String,
    pub elapsed_seconds: u64,
    pub cluster_name: Option<String>,
    pub cluster_version: Option<String>,
    pub message: String,
    pub timestamp: String,
}

/// Handle the support-package command
pub async fn handle_support_package_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: SupportPackageCommands,
    output_format: OutputFormat,
    _query: Option<&str>,
) -> CliResult<()> {
    match cmd {
        SupportPackageCommands::Cluster {
            file,
            skip_checks,
            optimize,
            no_optimize: _,
            log_lines,
            optimize_verbose,
            #[cfg(feature = "upload")]
            upload,
            #[cfg(feature = "upload")]
            no_save,
            async_ops,
        } => {
            let output_path = file.unwrap_or_else(|| {
                let timestamp = Local::now().format("%Y%m%dT%H%M%S");
                PathBuf::from(format!("support-package-cluster-{}.tar.gz", timestamp))
            });

            if !skip_checks {
                perform_preflight_checks(&output_path)?;
            }

            let optimization_opts = if optimize {
                Some(OptimizationOptions {
                    max_log_lines: log_lines,
                    remove_nested_gz: true,
                    exclude_patterns: vec![],
                    verbose: optimize_verbose,
                })
            } else {
                None
            };

            generate_cluster_package(
                conn_mgr,
                profile_name,
                output_path,
                &async_ops,
                output_format,
                optimization_opts,
                #[cfg(feature = "upload")]
                upload,
                #[cfg(feature = "upload")]
                no_save,
            )
            .await
        }

        SupportPackageCommands::Database {
            uid,
            file,
            skip_checks,
            optimize,
            no_optimize: _,
            log_lines,
            optimize_verbose,
            #[cfg(feature = "upload")]
            upload,
            #[cfg(feature = "upload")]
            no_save,
            async_ops,
        } => {
            let output_path = file.unwrap_or_else(|| {
                let timestamp = Local::now().format("%Y%m%dT%H%M%S");
                PathBuf::from(format!(
                    "support-package-database-{}-{}.tar.gz",
                    uid, timestamp
                ))
            });

            if !skip_checks {
                perform_preflight_checks(&output_path)?;
            }

            let optimization_opts = if optimize {
                Some(OptimizationOptions {
                    max_log_lines: log_lines,
                    remove_nested_gz: true,
                    exclude_patterns: vec![],
                    verbose: optimize_verbose,
                })
            } else {
                None
            };

            generate_database_package(
                conn_mgr,
                profile_name,
                uid,
                output_path,
                &async_ops,
                output_format,
                optimization_opts,
                #[cfg(feature = "upload")]
                upload,
                #[cfg(feature = "upload")]
                no_save,
            )
            .await
        }

        SupportPackageCommands::Node {
            uid,
            file,
            skip_checks,
            optimize,
            no_optimize: _,
            log_lines,
            optimize_verbose,
            #[cfg(feature = "upload")]
            upload,
            #[cfg(feature = "upload")]
            no_save,
            async_ops,
        } => {
            let output_path = file.unwrap_or_else(|| {
                let timestamp = Local::now().format("%Y%m%dT%H%M%S");
                let prefix = if let Some(node_uid) = uid {
                    format!("support-package-node-{}", node_uid)
                } else {
                    "support-package-nodes".to_string()
                };
                PathBuf::from(format!("{}-{}.tar.gz", prefix, timestamp))
            });

            if !skip_checks {
                perform_preflight_checks(&output_path)?;
            }

            let optimization_opts = if optimize {
                Some(OptimizationOptions {
                    max_log_lines: log_lines,
                    remove_nested_gz: true,
                    exclude_patterns: vec![],
                    verbose: optimize_verbose,
                })
            } else {
                None
            };

            generate_node_package(
                conn_mgr,
                profile_name,
                uid,
                output_path,
                &async_ops,
                output_format,
                optimization_opts,
                #[cfg(feature = "upload")]
                upload,
                #[cfg(feature = "upload")]
                no_save,
            )
            .await
        }
    }
}

/// Perform pre-flight checks before generating support package
fn perform_preflight_checks(output_path: &Path) -> AnyhowResult<()> {
    // Check if output file already exists
    if output_path.exists() {
        eprintln!("Warning: File {} already exists", output_path.display());
        eprint!("Overwrite? (y/N): ");
        let mut response = String::new();
        std::io::stdin().read_line(&mut response)?;
        if !response.trim().eq_ignore_ascii_case("y") {
            return Err(anyhow::anyhow!("Operation cancelled by user"));
        }
    }

    // Check write permissions to parent directory
    let parent_dir = output_path.parent().unwrap_or(Path::new("."));
    let parent_dir = if parent_dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent_dir
    };
    if !parent_dir.exists() {
        return Err(anyhow::anyhow!(
            "Output directory {} does not exist",
            parent_dir.display()
        ));
    }

    // Check available disk space (warn if less than 1GB)
    // Only perform check on Unix systems where we can read block count
    #[cfg(unix)]
    if let Ok(metadata) = fs::metadata(parent_dir) {
        use std::os::unix::fs::MetadataExt;
        // Basic check - would need proper disk space checking
        if metadata.blocks() < 1000000 {
            eprintln!("Warning: Low disk space detected");
        }
    }

    Ok(())
}

/// Generate cluster support package
#[allow(clippy::too_many_arguments)]
async fn generate_cluster_package(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_path: PathBuf,
    _async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    optimization_opts: Option<OptimizationOptions>,
    #[cfg(feature = "upload")] upload: bool,
    #[cfg(feature = "upload")] no_save: bool,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Try to get cluster info for display (non-blocking if it fails)
    let mut cluster_name = None;
    let mut cluster_version = None;

    if let Ok(cluster_info) = client.get::<serde_json::Value>("/v1/cluster").await {
        cluster_name = cluster_info
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from);
        cluster_version = cluster_info
            .get("software_version")
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    // Only show interactive output if not in JSON mode
    let spinner = if matches!(output_format, OutputFormat::Json) {
        None
    } else {
        println!("Redis Enterprise Support Package");
        println!("================================");

        if let Some(ref name) = cluster_name {
            println!("Cluster: {}", name);
        }
        if let Some(ref version) = cluster_version {
            println!("Version: {}", version);
        }

        println!("\nOutput: {}", output_path.display());
        println!("\nGenerating support package...");

        // Show progress spinner
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        spinner.set_message("Collecting cluster data...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(spinner)
    };

    let start_time = std::time::Instant::now();

    let debuginfo_handler = redis_enterprise::debuginfo::DebugInfoHandler::new(client);
    let mut data = debuginfo_handler
        .cluster_debuginfo_binary()
        .await
        .map_err(RedisCtlError::from)?;

    let original_size = data.len();

    // Apply optimization if requested
    if let Some(opts) = optimization_opts {
        if let Some(ref spinner) = spinner {
            spinner.set_message("Optimizing package...");
        }

        data =
            optimize_support_package(&data, &opts).context("Failed to optimize support package")?;

        if !matches!(output_format, OutputFormat::Json) && opts.verbose {
            let reduction = ((original_size - data.len()) as f64 / original_size as f64) * 100.0;
            eprintln!(
                "Optimization: {} → {} ({:.1}% reduction)",
                format_file_size(original_size),
                format_file_size(data.len()),
                reduction
            );
        }
    }

    if let Some(spinner) = spinner {
        spinner.finish_and_clear();
    }

    // Handle upload if requested
    #[cfg(feature = "upload")]
    let uploaded_path = if upload {
        let api_key =
            upload::get_files_api_key(profile_name).context("Failed to get Files.com API key")?;

        let filename = output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("support-package.tar.gz");

        let upload_path = upload::upload_package(&api_key, &data, filename, None)
            .await
            .context("Failed to upload support package")?;

        Some(upload_path)
    } else {
        None
    };

    // Save to file unless --no-save
    #[cfg(feature = "upload")]
    let should_save = !no_save;
    #[cfg(not(feature = "upload"))]
    let should_save = true;

    if should_save {
        fs::write(&output_path, &data).context(format!(
            "Failed to save support package to {:?}",
            output_path
        ))?;
    }

    let elapsed = start_time.elapsed();
    let file_size = data.len();
    let size_display = format_file_size(file_size);

    // Output based on format
    match output_format {
        OutputFormat::Json => {
            #[cfg(feature = "upload")]
            let message = if uploaded_path.is_some() {
                format!(
                    "Support package {} and uploaded to Files.com",
                    if should_save { "created" } else { "uploaded" }
                )
            } else {
                "Support package created successfully".to_string()
            };
            #[cfg(not(feature = "upload"))]
            let message = "Support package created successfully".to_string();

            let result = SupportPackageResult {
                success: true,
                package_type: "cluster".to_string(),
                file_path: output_path.display().to_string(),
                file_size,
                file_size_display: size_display.clone(),
                elapsed_seconds: elapsed.as_secs(),
                cluster_name,
                cluster_version,
                message,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };

            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        _ => {
            // Display success message with helpful information
            println!("\n✓ Support package created successfully");

            #[cfg(feature = "upload")]
            if let Some(ref upload_path) = uploaded_path {
                println!("  Uploaded to: {}", upload_path);
            }

            if should_save {
                println!("  File: {}", output_path.display());
            }
            println!("  Size: {}", size_display);
            println!("  Time: {}s", elapsed.as_secs());

            #[cfg(feature = "upload")]
            if uploaded_path.is_none() {
                println!("\nNext steps:");
                println!("1. Upload to Redis Support: https://support.redis.com/upload");
                println!("2. Reference your case number when uploading");
                println!("3. Delete local file after upload to free space");
            } else if should_save {
                println!("\nPackage uploaded to Files.com and saved locally.");
                println!("You can delete the local file after confirming upload.");
            }

            #[cfg(not(feature = "upload"))]
            {
                println!("\nNext steps:");
                println!("1. Upload to Redis Support: https://support.redis.com/upload");
                println!("2. Reference your case number when uploading");
                println!("3. Delete local file after upload to free space");
            }
        }
    }

    Ok(())
}

/// Generate database support package
#[allow(clippy::too_many_arguments)]
async fn generate_database_package(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: u32,
    output_path: PathBuf,
    _async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    optimization_opts: Option<OptimizationOptions>,
    #[cfg(feature = "upload")] upload: bool,
    #[cfg(feature = "upload")] no_save: bool,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Try to get database info for display
    let mut database_name = None;
    if let Ok(db_info) = client
        .get::<serde_json::Value>(&format!("/v1/bdbs/{}", uid))
        .await
    {
        database_name = db_info
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    // Only show interactive output if not in JSON mode
    let spinner = if matches!(output_format, OutputFormat::Json) {
        None
    } else {
        println!("Redis Enterprise Support Package");
        println!("================================");
        println!("Database: {}", uid);

        if let Some(ref name) = database_name {
            println!("Name: {}", name);
        }

        println!("\nOutput: {}", output_path.display());
        println!("\nGenerating support package...");

        // Show progress spinner
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        spinner.set_message(format!("Collecting database {} data...", uid));
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(spinner)
    };

    let start_time = std::time::Instant::now();

    let debuginfo_handler = redis_enterprise::debuginfo::DebugInfoHandler::new(client);
    let mut data = debuginfo_handler
        .database_debuginfo_binary(uid)
        .await
        .context(format!("Failed to collect debug info for database {}", uid))?;

    let original_size = data.len();

    // Apply optimization if requested
    if let Some(opts) = optimization_opts {
        if let Some(ref spinner) = spinner {
            spinner.set_message("Optimizing package...");
        }

        data =
            optimize_support_package(&data, &opts).context("Failed to optimize support package")?;

        if !matches!(output_format, OutputFormat::Json) && opts.verbose {
            let reduction = ((original_size - data.len()) as f64 / original_size as f64) * 100.0;
            eprintln!(
                "Optimization: {} → {} ({:.1}% reduction)",
                format_file_size(original_size),
                format_file_size(data.len()),
                reduction
            );
        }
    }

    if let Some(spinner) = spinner {
        spinner.finish_and_clear();
    }

    // Handle upload if requested
    #[cfg(feature = "upload")]
    let uploaded_path = if upload {
        let api_key =
            upload::get_files_api_key(profile_name).context("Failed to get Files.com API key")?;

        let filename = output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("support-package.tar.gz");

        let upload_path = upload::upload_package(&api_key, &data, filename, None)
            .await
            .context("Failed to upload support package")?;

        Some(upload_path)
    } else {
        None
    };

    // Save to file unless --no-save
    #[cfg(feature = "upload")]
    let should_save = !no_save;
    #[cfg(not(feature = "upload"))]
    let should_save = true;

    if should_save {
        fs::write(&output_path, &data).context(format!(
            "Failed to save support package to {:?}",
            output_path
        ))?;
    }

    let elapsed = start_time.elapsed();
    let file_size = data.len();
    let size_display = format_file_size(file_size);

    // Output based on format
    match output_format {
        OutputFormat::Json => {
            #[cfg(feature = "upload")]
            let message = if uploaded_path.is_some() {
                format!(
                    "Database support package {} and uploaded to Files.com",
                    if should_save { "created" } else { "uploaded" }
                )
            } else {
                "Database support package created successfully".to_string()
            };
            #[cfg(not(feature = "upload"))]
            let message = "Database support package created successfully".to_string();

            let result = SupportPackageResult {
                success: true,
                package_type: format!("database-{}", uid),
                file_path: output_path.display().to_string(),
                file_size,
                file_size_display: size_display.clone(),
                elapsed_seconds: elapsed.as_secs(),
                cluster_name: Some(format!("Database {}", uid)),
                cluster_version: database_name,
                message,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };

            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        _ => {
            println!("\n✓ Database support package created successfully");

            #[cfg(feature = "upload")]
            if let Some(ref upload_path) = uploaded_path {
                println!("  Uploaded to: {}", upload_path);
            }

            if should_save {
                println!("  File: {}", output_path.display());
            }
            println!("  Size: {}", size_display);
            println!("  Time: {}s", elapsed.as_secs());

            #[cfg(feature = "upload")]
            if uploaded_path.is_none() {
                println!("\nNext steps:");
                println!("1. Upload to Redis Support: https://support.redis.com/upload");
                println!("2. Reference your case number when uploading");
                println!("3. Delete local file after upload to free space");
            } else if should_save {
                println!("\nPackage uploaded to Files.com and saved locally.");
                println!("You can delete the local file after confirming upload.");
            }

            #[cfg(not(feature = "upload"))]
            {
                println!("\nNext steps:");
                println!("1. Upload to Redis Support: https://support.redis.com/upload");
                println!("2. Reference your case number when uploading");
                println!("3. Delete local file after upload to free space");
            }
        }
    }

    Ok(())
}

/// Generate node support package
#[allow(clippy::too_many_arguments)]
async fn generate_node_package(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: Option<u32>,
    output_path: PathBuf,
    _async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    optimization_opts: Option<OptimizationOptions>,
    #[cfg(feature = "upload")] upload: bool,
    #[cfg(feature = "upload")] no_save: bool,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Try to get node info for display
    let mut node_address = None;
    if let Some(node_uid) = uid
        && let Ok(node_info) = client
            .get::<serde_json::Value>(&format!("/v1/nodes/{}", node_uid))
            .await
    {
        node_address = node_info
            .get("addr")
            .and_then(|v| v.as_str())
            .map(String::from);
    }

    // Only show interactive output if not in JSON mode
    let spinner = if matches!(output_format, OutputFormat::Json) {
        None
    } else {
        println!("Redis Enterprise Support Package");
        println!("================================");

        if let Some(node_uid) = uid {
            println!("Node: {}", node_uid);
            if let Some(ref addr) = node_address {
                println!("Address: {}", addr);
            }
        } else {
            println!("Nodes: All");
        }

        println!("\nOutput: {}", output_path.display());
        println!("\nGenerating support package...");

        // Show progress spinner
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        let msg = if let Some(node_uid) = uid {
            format!("Collecting node {} data...", node_uid)
        } else {
            "Collecting all nodes data...".to_string()
        };
        spinner.set_message(msg);
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(spinner)
    };

    let start_time = std::time::Instant::now();

    let debuginfo_handler = redis_enterprise::debuginfo::DebugInfoHandler::new(client);
    let mut data = if let Some(node_uid) = uid {
        debuginfo_handler
            .node_debuginfo_binary(node_uid)
            .await
            .context(format!(
                "Failed to collect debug info for node {}",
                node_uid
            ))?
    } else {
        debuginfo_handler
            .nodes_debuginfo_binary()
            .await
            .map_err(RedisCtlError::from)?
    };

    let original_size = data.len();

    // Apply optimization if requested
    if let Some(opts) = optimization_opts {
        if let Some(ref spinner) = spinner {
            spinner.set_message("Optimizing package...");
        }

        data =
            optimize_support_package(&data, &opts).context("Failed to optimize support package")?;

        if !matches!(output_format, OutputFormat::Json) && opts.verbose {
            let reduction = ((original_size - data.len()) as f64 / original_size as f64) * 100.0;
            eprintln!(
                "Optimization: {} → {} ({:.1}% reduction)",
                format_file_size(original_size),
                format_file_size(data.len()),
                reduction
            );
        }
    }

    if let Some(spinner) = spinner {
        spinner.finish_and_clear();
    }

    // Handle upload if requested
    #[cfg(feature = "upload")]
    let uploaded_path = if upload {
        let api_key =
            upload::get_files_api_key(profile_name).context("Failed to get Files.com API key")?;

        let filename = output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("support-package.tar.gz");

        let upload_path = upload::upload_package(&api_key, &data, filename, None)
            .await
            .context("Failed to upload support package")?;

        Some(upload_path)
    } else {
        None
    };

    // Save to file unless --no-save
    #[cfg(feature = "upload")]
    let should_save = !no_save;
    #[cfg(not(feature = "upload"))]
    let should_save = true;

    if should_save {
        fs::write(&output_path, &data).context(format!(
            "Failed to save support package to {:?}",
            output_path
        ))?;
    }

    let elapsed = start_time.elapsed();
    let file_size = data.len();
    let size_display = format_file_size(file_size);

    // Output based on format
    match output_format {
        OutputFormat::Json => {
            let package_type = if let Some(node_uid) = uid {
                format!("node-{}", node_uid)
            } else {
                "nodes".to_string()
            };

            #[cfg(feature = "upload")]
            let message = if uploaded_path.is_some() {
                format!(
                    "{} support package {} and uploaded to Files.com",
                    if uid.is_some() { "Node" } else { "Nodes" },
                    if should_save { "created" } else { "uploaded" }
                )
            } else if uid.is_some() {
                "Node support package created successfully".to_string()
            } else {
                "Nodes support package created successfully".to_string()
            };
            #[cfg(not(feature = "upload"))]
            let message = if uid.is_some() {
                "Node support package created successfully".to_string()
            } else {
                "Nodes support package created successfully".to_string()
            };

            let result = SupportPackageResult {
                success: true,
                package_type,
                file_path: output_path.display().to_string(),
                file_size,
                file_size_display: size_display.clone(),
                elapsed_seconds: elapsed.as_secs(),
                cluster_name: uid.map(|id| format!("Node {}", id)),
                cluster_version: node_address,
                message,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };

            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        _ => {
            let package_type = if uid.is_some() { "Node" } else { "Nodes" };
            println!("\n✓ {} support package created successfully", package_type);

            #[cfg(feature = "upload")]
            if let Some(ref upload_path) = uploaded_path {
                println!("  Uploaded to: {}", upload_path);
            }

            if should_save {
                println!("  File: {}", output_path.display());
            }
            println!("  Size: {}", size_display);
            println!("  Time: {}s", elapsed.as_secs());

            #[cfg(feature = "upload")]
            if uploaded_path.is_none() {
                println!("\nNext steps:");
                println!("1. Upload to Redis Support: https://support.redis.com/upload");
                println!("2. Reference your case number when uploading");
                println!("3. Delete local file after upload to free space");
            } else if should_save {
                println!("\nPackage uploaded to Files.com and saved locally.");
                println!("You can delete the local file after confirming upload.");
            }

            #[cfg(not(feature = "upload"))]
            {
                println!("\nNext steps:");
                println!("1. Upload to Redis Support: https://support.redis.com/upload");
                println!("2. Reference your case number when uploading");
                println!("3. Delete local file after upload to free space");
            }
        }
    }

    Ok(())
}

/// Format file size for display
fn format_file_size(size: usize) -> String {
    if size > 1_000_000_000 {
        format!("{:.1} GB", size as f64 / 1_073_741_824.0)
    } else if size > 1_000_000 {
        format!("{:.1} MB", size as f64 / 1_048_576.0)
    } else if size > 1_000 {
        format!("{:.1} KB", size as f64 / 1_024.0)
    } else {
        format!("{} bytes", size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(500), "500 bytes");
        assert_eq!(format_file_size(1_500), "1.5 KB");
        assert_eq!(format_file_size(1_500_000), "1.4 MB");
        assert_eq!(format_file_size(1_500_000_000), "1.4 GB");
    }
}
