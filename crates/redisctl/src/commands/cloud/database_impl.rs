//! Implementation of additional database commands

use super::async_utils::{AsyncOperationArgs, handle_async_response};
use super::utils::*;
use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use crate::output::print_output;
use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use redis_cloud::databases::DatabaseCreateRequest;
use redisctl_core::ProgressEvent;
use redisctl_core::cloud::{
    backup_database_and_wait, create_database_and_wait, delete_database_and_wait,
    import_database_and_wait, update_database_and_wait,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tabled::{Table, Tabled, settings::Style};

/// Helper to print non-table output
fn print_json_or_yaml(data: Value, output_format: OutputFormat) -> CliResult<()> {
    match output_format {
        OutputFormat::Json => print_output(data, crate::output::OutputFormat::Json, None)?,
        OutputFormat::Yaml => print_output(data, crate::output::OutputFormat::Yaml, None)?,
        _ => print_output(data, crate::output::OutputFormat::Json, None)?,
    }
    Ok(())
}

/// Parse database ID into subscription and database IDs
fn parse_database_id(id: &str) -> CliResult<(u32, u32)> {
    let parts: Vec<&str> = id.split(':').collect();
    if parts.len() != 2 {
        return Err(RedisCtlError::InvalidInput {
            message: format!(
                "Invalid database ID format: {}. Expected format: subscription_id:database_id",
                id
            ),
        });
    }

    let subscription_id = parts[0]
        .parse::<u32>()
        .map_err(|_| RedisCtlError::InvalidInput {
            message: format!("Invalid subscription ID: {}", parts[0]),
        })?;

    let database_id = parts[1]
        .parse::<u32>()
        .map_err(|_| RedisCtlError::InvalidInput {
            message: format!("Invalid database ID: {}", parts[1]),
        })?;

    Ok((subscription_id, database_id))
}

/// Read JSON data from string or file
fn read_json_data(data: &str) -> CliResult<Value> {
    let json_str = if let Some(file_path) = data.strip_prefix('@') {
        // Read from file
        std::fs::read_to_string(file_path).map_err(|e| RedisCtlError::InvalidInput {
            message: format!("Failed to read file {}: {}", file_path, e),
        })?
    } else {
        // Use as-is
        data.to_string()
    };

    serde_json::from_str(&json_str).map_err(|e| RedisCtlError::InvalidInput {
        message: format!("Invalid JSON: {}", e),
    })
}

/// Create a new database with first-class parameters
///
/// Uses Layer 2 (redisctl-core) workflows when possible for progress tracking.
/// Falls back to legacy raw API for advanced options not yet in Layer 2.
#[allow(clippy::too_many_arguments)]
pub async fn create_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    name: Option<&str>,
    memory: Option<f64>,
    dataset_size: Option<f64>,
    protocol: &str,
    replication: bool,
    data_persistence: Option<&str>,
    eviction_policy: &str,
    redis_version: Option<&str>,
    oss_cluster: bool,
    port: Option<i32>,
    data: Option<&str>,
    dry_run: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    // Use Layer 2 workflow for simple cases with --wait
    // Fall back to legacy for: --data, --dataset-size, advanced options, --dry-run
    let use_layer2 = async_ops.wait
        && !dry_run
        && data.is_none()
        && dataset_size.is_none()
        && eviction_policy == "volatile-lru"
        && redis_version.is_none()
        && !oss_cluster
        && port.is_none()
        && name.is_some()
        && memory.is_some();

    if use_layer2 {
        create_database_with_workflow(
            conn_mgr,
            profile_name,
            subscription_id,
            name.unwrap(),   // Safe: checked above
            memory.unwrap(), // Safe: checked above
            protocol,
            replication,
            data_persistence,
            async_ops,
            output_format,
            query,
        )
        .await
    } else {
        create_database_legacy(
            conn_mgr,
            profile_name,
            subscription_id,
            name,
            memory,
            dataset_size,
            protocol,
            replication,
            data_persistence,
            eviction_policy,
            redis_version,
            oss_cluster,
            port,
            data,
            dry_run,
            async_ops,
            output_format,
            query,
        )
        .await
    }
}

/// Create database using Layer 2 workflow with progress tracking
#[allow(clippy::too_many_arguments)]
async fn create_database_with_workflow(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    name: &str,
    memory_gb: f64,
    protocol: &str,
    replication: bool,
    data_persistence: Option<&str>,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Build request using TypedBuilder from Layer 1
    // Note: Optional fields with defaults don't need to be set
    let request = match (protocol != "redis", data_persistence) {
        (true, Some(persistence)) => DatabaseCreateRequest::builder()
            .name(name)
            .memory_limit_in_gb(memory_gb)
            .replication(replication)
            .protocol(protocol)
            .data_persistence(persistence)
            .build(),
        (true, None) => DatabaseCreateRequest::builder()
            .name(name)
            .memory_limit_in_gb(memory_gb)
            .replication(replication)
            .protocol(protocol)
            .build(),
        (false, Some(persistence)) => DatabaseCreateRequest::builder()
            .name(name)
            .memory_limit_in_gb(memory_gb)
            .replication(replication)
            .data_persistence(persistence)
            .build(),
        (false, None) => DatabaseCreateRequest::builder()
            .name(name)
            .memory_limit_in_gb(memory_gb)
            .replication(replication)
            .build(),
    };

    // Set up progress spinner
    let pb = Arc::new(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg} [{elapsed_precise}]")
            .unwrap(),
    );
    pb.set_message("Creating database...");

    // Create progress callback
    let pb_clone = Arc::clone(&pb);
    let progress_callback = Box::new(move |event: ProgressEvent| match event {
        ProgressEvent::Started { task_id } => {
            pb_clone.set_message(format!("Task {} started", task_id));
        }
        ProgressEvent::Polling {
            status, elapsed, ..
        } => {
            pb_clone.set_message(format!("{} ({:.0}s)", status, elapsed.as_secs_f64()));
        }
        ProgressEvent::Completed { resource_id, .. } => {
            pb_clone.finish_with_message(format!(
                "Created database {}",
                resource_id.map_or("".to_string(), |id| id.to_string())
            ));
        }
        ProgressEvent::Failed { error, .. } => {
            pb_clone.finish_with_message(format!("Failed: {}", error));
        }
    });

    // Call Layer 2 workflow
    #[allow(clippy::cast_possible_wrap)]
    let database = create_database_and_wait(
        &client,
        subscription_id as i32,
        &request,
        Duration::from_secs(async_ops.wait_timeout),
        Some(progress_callback),
    )
    .await
    .map_err(|e| RedisCtlError::ApiError {
        message: e.to_string(),
    })?;

    pb.finish_and_clear();

    // Output result
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            println!("Database created successfully");
            println!("  ID: {}:{}", subscription_id, database.database_id);
            println!("  Name: {}", database.name.as_deref().unwrap_or(""));
            println!("  Status: {}", database.status.as_deref().unwrap_or(""));
            if let Some(endpoint) = &database.public_endpoint {
                println!("  Endpoint: {}", endpoint);
            }
        }
        OutputFormat::Json | OutputFormat::Yaml => {
            let json_value = serde_json::to_value(&database)?;
            let data = if let Some(q) = query {
                apply_jmespath(&json_value, q)?
            } else {
                json_value
            };
            print_json_or_yaml(data, output_format)?;
        }
    }

    Ok(())
}

/// Legacy create_database implementation for --data mode and advanced options
#[allow(clippy::too_many_arguments)]
async fn create_database_legacy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    name: Option<&str>,
    memory: Option<f64>,
    dataset_size: Option<f64>,
    protocol: &str,
    replication: bool,
    data_persistence: Option<&str>,
    eviction_policy: &str,
    redis_version: Option<&str>,
    oss_cluster: bool,
    port: Option<i32>,
    data: Option<&str>,
    dry_run: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        json!({})
    };

    // Ensure request is an object
    if !request.is_object() {
        return Err(RedisCtlError::InvalidInput {
            message: "Database configuration must be a JSON object".to_string(),
        });
    }

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    // Required parameters (when not using pure --data mode)
    if let Some(name_val) = name {
        request_obj.insert("name".to_string(), json!(name_val));
    } else if data.is_none() {
        return Err(RedisCtlError::InvalidInput {
            message: "--name is required (unless using --data with complete configuration)"
                .to_string(),
        });
    }

    // Memory configuration (must have either --memory, --dataset-size, or in --data)
    if let Some(mem) = memory {
        request_obj.insert("memoryLimitInGb".to_string(), json!(mem));
    } else if let Some(dataset) = dataset_size {
        request_obj.insert("datasetSizeInGb".to_string(), json!(dataset));
    } else if data.is_none() {
        return Err(RedisCtlError::InvalidInput {
            message: "Either --memory or --dataset-size is required (unless using --data with complete configuration)".to_string(),
        });
    }

    // Protocol (only set if non-default or not already in data)
    if protocol != "redis" || !request_obj.contains_key("protocol") {
        request_obj.insert("protocol".to_string(), json!(protocol));
    }

    // Replication (only set if true or not already in data)
    if replication || !request_obj.contains_key("replication") {
        request_obj.insert("replication".to_string(), json!(replication));
    }

    // Optional parameters - only set if provided
    if let Some(persistence) = data_persistence {
        request_obj.insert("dataPersistence".to_string(), json!(persistence));
    }

    // Eviction policy (only set if non-default or not already in data)
    if eviction_policy != "volatile-lru" || !request_obj.contains_key("dataEvictionPolicy") {
        request_obj.insert("dataEvictionPolicy".to_string(), json!(eviction_policy));
    }

    if let Some(version) = redis_version {
        request_obj.insert("redisVersion".to_string(), json!(version));
    }

    if oss_cluster {
        request_obj.insert("supportOSSClusterAPI".to_string(), json!(true));
    }

    if let Some(port_val) = port {
        request_obj.insert("port".to_string(), json!(port_val));
    }

    if dry_run {
        request_obj.insert("dryRun".to_string(), json!(true));
    }

    let response = client
        .post_raw(
            &format!("/subscriptions/{}/databases", subscription_id),
            request,
        )
        .await
        .context("Failed to create database")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Database created successfully",
    )
    .await
}

/// Update database configuration
#[allow(clippy::too_many_arguments)]
pub async fn update_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    name: Option<&str>,
    memory: Option<f64>,
    replication: Option<bool>,
    data_persistence: Option<&str>,
    eviction_policy: Option<&str>,
    oss_cluster: Option<bool>,
    regex_rules: Option<&str>,
    data: Option<&str>,
    dry_run: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;

    // Use Layer 2 workflow for simple cases with --wait (no --data, no regex_rules, no --dry-run)
    let use_layer2 = async_ops.wait && !dry_run && data.is_none() && regex_rules.is_none();

    if use_layer2 {
        update_database_with_workflow(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            name,
            memory,
            replication,
            data_persistence,
            eviction_policy,
            oss_cluster,
            async_ops,
            output_format,
            query,
        )
        .await
    } else {
        update_database_legacy(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            name,
            memory,
            replication,
            data_persistence,
            eviction_policy,
            oss_cluster,
            regex_rules,
            data,
            dry_run,
            async_ops,
            output_format,
            query,
        )
        .await
    }
}

/// Update database using Layer 2 workflow with progress tracking
#[allow(clippy::too_many_arguments)]
async fn update_database_with_workflow(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    name: Option<&str>,
    memory: Option<f64>,
    replication: Option<bool>,
    data_persistence: Option<&str>,
    eviction_policy: Option<&str>,
    oss_cluster: Option<bool>,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    use redis_cloud::databases::DatabaseUpdateRequest;

    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Build request using TypedBuilder - start with defaults then override
    let mut request = DatabaseUpdateRequest::builder().build();

    if let Some(n) = name {
        request.name = Some(n.to_string());
    }
    if let Some(m) = memory {
        request.memory_limit_in_gb = Some(m);
    }
    if let Some(r) = replication {
        request.replication = Some(r);
    }
    if let Some(p) = data_persistence {
        request.data_persistence = Some(p.to_string());
    }
    if let Some(e) = eviction_policy {
        request.data_eviction_policy = Some(e.to_string());
    }
    if let Some(o) = oss_cluster {
        request.support_oss_cluster_api = Some(o);
    }

    // Validate at least one field is set
    if request.name.is_none()
        && request.memory_limit_in_gb.is_none()
        && request.replication.is_none()
        && request.data_persistence.is_none()
        && request.data_eviction_policy.is_none()
        && request.support_oss_cluster_api.is_none()
    {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required".to_string(),
        });
    }

    // Set up progress spinner
    let pb = Arc::new(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg} [{elapsed_precise}]")
            .unwrap(),
    );
    pb.set_message("Updating database...");

    // Create progress callback
    let pb_clone = Arc::clone(&pb);
    let progress_callback = Box::new(move |event: ProgressEvent| match event {
        ProgressEvent::Started { task_id } => {
            pb_clone.set_message(format!("Task {} started", task_id));
        }
        ProgressEvent::Polling {
            status, elapsed, ..
        } => {
            pb_clone.set_message(format!("{} ({:.0}s)", status, elapsed.as_secs_f64()));
        }
        ProgressEvent::Completed { .. } => {
            pb_clone.finish_with_message("Database updated");
        }
        ProgressEvent::Failed { error, .. } => {
            pb_clone.finish_with_message(format!("Failed: {}", error));
        }
    });

    // Call Layer 2 workflow
    #[allow(clippy::cast_possible_wrap)]
    let database = update_database_and_wait(
        &client,
        subscription_id as i32,
        database_id as i32,
        &request,
        Duration::from_secs(async_ops.wait_timeout),
        Some(progress_callback),
    )
    .await
    .map_err(|e| RedisCtlError::ApiError {
        message: e.to_string(),
    })?;

    pb.finish_and_clear();

    // Output result
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            println!("Database updated successfully");
            println!("  ID: {}:{}", subscription_id, database.database_id);
            println!("  Name: {}", database.name.as_deref().unwrap_or(""));
            println!("  Status: {}", database.status.as_deref().unwrap_or(""));
        }
        OutputFormat::Json | OutputFormat::Yaml => {
            let json_value = serde_json::to_value(&database)?;
            let data = if let Some(q) = query {
                apply_jmespath(&json_value, q)?
            } else {
                json_value
            };
            print_json_or_yaml(data, output_format)?;
        }
    }

    Ok(())
}

/// Legacy update_database implementation for --data mode and regex_rules
#[allow(clippy::too_many_arguments)]
async fn update_database_legacy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    name: Option<&str>,
    memory: Option<f64>,
    replication: Option<bool>,
    data_persistence: Option<&str>,
    eviction_policy: Option<&str>,
    oss_cluster: Option<bool>,
    regex_rules: Option<&str>,
    data: Option<&str>,
    dry_run: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(name_val) = name {
        request_obj.insert("name".to_string(), json!(name_val));
    }

    if let Some(mem) = memory {
        request_obj.insert("memoryLimitInGb".to_string(), json!(mem));
    }

    if let Some(repl) = replication {
        request_obj.insert("replication".to_string(), json!(repl));
    }

    if let Some(persistence) = data_persistence {
        request_obj.insert("dataPersistence".to_string(), json!(persistence));
    }

    if let Some(eviction) = eviction_policy {
        request_obj.insert("dataEvictionPolicy".to_string(), json!(eviction));
    }

    if let Some(oss) = oss_cluster {
        request_obj.insert("supportOSSClusterAPI".to_string(), json!(oss));
    }

    if let Some(regex) = regex_rules {
        request_obj.insert("regexRules".to_string(), json!([regex]));
    }

    // Validate that we have at least one field to update
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--name, --memory, --replication, --data-persistence, --eviction-policy, --oss-cluster, --regex-rules, or --data)".to_string(),
        });
    }

    if dry_run {
        request_obj.insert("dryRun".to_string(), json!(true));
    }

    let response = client
        .put_raw(
            &format!(
                "/subscriptions/{}/databases/{}",
                subscription_id, database_id
            ),
            request,
        )
        .await
        .context("Failed to update database")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Database updated successfully",
    )
    .await
}

/// Delete a database
#[allow(clippy::too_many_arguments)]
pub async fn delete_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    force: bool,
    dry_run: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;

    if dry_run {
        eprintln!(
            "Would delete database {} in subscription {}.",
            database_id, subscription_id
        );
        eprintln!();
        eprintln!("No changes were made.");
        return Ok(());
    }

    // Confirmation prompt unless --force is used
    if !force
        && !super::utils::confirm_action(&format!(
            "Are you sure you want to delete database {}?",
            id
        ))?
    {
        println!("Database deletion cancelled");
        return Ok(());
    }

    // Use Layer 2 workflow when --wait is specified
    if async_ops.wait {
        delete_database_with_workflow(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            async_ops,
            output_format,
        )
        .await
    } else {
        delete_database_legacy(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            async_ops,
            output_format,
            query,
        )
        .await
    }
}

/// Delete database using Layer 2 workflow with progress tracking
async fn delete_database_with_workflow(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Set up progress spinner
    let pb = Arc::new(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg} [{elapsed_precise}]")
            .unwrap(),
    );
    pb.set_message("Deleting database...");

    // Create progress callback
    let pb_clone = Arc::clone(&pb);
    let progress_callback = Box::new(move |event: ProgressEvent| match event {
        ProgressEvent::Started { task_id } => {
            pb_clone.set_message(format!("Task {} started", task_id));
        }
        ProgressEvent::Polling {
            status, elapsed, ..
        } => {
            pb_clone.set_message(format!("{} ({:.0}s)", status, elapsed.as_secs_f64()));
        }
        ProgressEvent::Completed { .. } => {
            pb_clone.finish_with_message("Database deleted");
        }
        ProgressEvent::Failed { error, .. } => {
            pb_clone.finish_with_message(format!("Failed: {}", error));
        }
    });

    // Call Layer 2 workflow
    #[allow(clippy::cast_possible_wrap)]
    delete_database_and_wait(
        &client,
        subscription_id as i32,
        database_id as i32,
        Duration::from_secs(async_ops.wait_timeout),
        Some(progress_callback),
    )
    .await
    .map_err(|e| RedisCtlError::ApiError {
        message: e.to_string(),
    })?;

    pb.finish_and_clear();

    // Output result
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            println!(
                "Database {}:{} deleted successfully",
                subscription_id, database_id
            );
        }
        OutputFormat::Json | OutputFormat::Yaml => {
            let result = json!({
                "message": "Database deleted successfully",
                "subscription_id": subscription_id,
                "database_id": database_id
            });
            print_json_or_yaml(result, output_format)?;
        }
    }

    Ok(())
}

/// Legacy delete_database implementation (no --wait)
async fn delete_database_legacy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .delete_raw(&format!(
            "/subscriptions/{}/databases/{}",
            subscription_id, database_id
        ))
        .await
        .context("Failed to delete database")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Database deletion initiated",
    )
    .await
}

/// Get database backup status
pub async fn get_backup_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!(
            "/subscriptions/{}/databases/{}/backup-status",
            subscription_id, database_id
        ))
        .await
        .context("Failed to get backup status")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            if let Some(status) = result.get("status") {
                println!(
                    "Backup Status: {}",
                    format_status_text(status.as_str().unwrap_or(""))
                );
            }
            if let Some(last_backup) = result.get("lastBackupTime") {
                println!(
                    "Last Backup: {}",
                    format_date(last_backup.as_str().unwrap_or("").to_string())
                );
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Trigger manual database backup
pub async fn backup_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;

    // Use Layer 2 workflow when --wait is specified
    if async_ops.wait {
        backup_database_with_workflow(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            async_ops,
            output_format,
        )
        .await
    } else {
        backup_database_legacy(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            async_ops,
            output_format,
            query,
        )
        .await
    }
}

/// Backup database using Layer 2 workflow with progress tracking
async fn backup_database_with_workflow(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Set up progress spinner
    let pb = Arc::new(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg} [{elapsed_precise}]")
            .unwrap(),
    );
    pb.set_message("Backing up database...");

    // Create progress callback
    let pb_clone = Arc::clone(&pb);
    let progress_callback = Box::new(move |event: ProgressEvent| match event {
        ProgressEvent::Started { task_id } => {
            pb_clone.set_message(format!("Task {} started", task_id));
        }
        ProgressEvent::Polling {
            status, elapsed, ..
        } => {
            pb_clone.set_message(format!("{} ({:.0}s)", status, elapsed.as_secs_f64()));
        }
        ProgressEvent::Completed { .. } => {
            pb_clone.finish_with_message("Backup completed");
        }
        ProgressEvent::Failed { error, .. } => {
            pb_clone.finish_with_message(format!("Failed: {}", error));
        }
    });

    // Call Layer 2 workflow
    #[allow(clippy::cast_possible_wrap)]
    backup_database_and_wait(
        &client,
        subscription_id as i32,
        database_id as i32,
        None, // region_name - only needed for Active-Active
        Duration::from_secs(async_ops.wait_timeout),
        Some(progress_callback),
    )
    .await
    .map_err(|e| RedisCtlError::ApiError {
        message: e.to_string(),
    })?;

    pb.finish_and_clear();

    // Output result
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            println!(
                "Database {}:{} backup completed successfully",
                subscription_id, database_id
            );
        }
        OutputFormat::Json | OutputFormat::Yaml => {
            let result = json!({
                "message": "Backup completed successfully",
                "subscription_id": subscription_id,
                "database_id": database_id
            });
            print_json_or_yaml(result, output_format)?;
        }
    }

    Ok(())
}

/// Legacy backup_database implementation (no --wait)
async fn backup_database_legacy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .post_raw(
            &format!(
                "/subscriptions/{}/databases/{}/backup",
                subscription_id, database_id
            ),
            json!({}),
        )
        .await
        .context("Failed to trigger backup")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Backup initiated successfully",
    )
    .await
}

/// Get database import status
pub async fn get_import_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!(
            "/subscriptions/{}/databases/{}/import-status",
            subscription_id, database_id
        ))
        .await
        .context("Failed to get import status")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            if let Some(status) = result.get("status") {
                println!(
                    "Import Status: {}",
                    format_status_text(status.as_str().unwrap_or(""))
                );
            }
            if let Some(progress) = result.get("progress") {
                println!("Progress: {}%", progress);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Import data into database
#[allow(clippy::too_many_arguments)]
pub async fn import_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    source_type: Option<&str>,
    import_from_uri: Option<&str>,
    aws_access_key: Option<&str>,
    aws_secret_key: Option<&str>,
    gcs_client_email: Option<&str>,
    gcs_private_key: Option<&str>,
    azure_account_name: Option<&str>,
    azure_account_key: Option<&str>,
    data: Option<&str>,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;

    // Check if we can use Layer 2 (simple case: no credentials, no --data)
    let has_credentials = aws_access_key.is_some()
        || aws_secret_key.is_some()
        || gcs_client_email.is_some()
        || gcs_private_key.is_some()
        || azure_account_name.is_some()
        || azure_account_key.is_some();

    let use_layer2 = async_ops.wait
        && data.is_none()
        && !has_credentials
        && source_type.is_some()
        && import_from_uri.is_some();

    if use_layer2 {
        import_database_with_workflow(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            source_type.unwrap(),     // safe: checked above
            import_from_uri.unwrap(), // safe: checked above
            async_ops,
            output_format,
        )
        .await
    } else {
        import_database_legacy(
            conn_mgr,
            profile_name,
            subscription_id,
            database_id,
            source_type,
            import_from_uri,
            aws_access_key,
            aws_secret_key,
            gcs_client_email,
            gcs_private_key,
            azure_account_name,
            azure_account_key,
            data,
            async_ops,
            output_format,
            query,
        )
        .await
    }
}

/// Import database using Layer 2 workflow with progress tracking
#[allow(clippy::too_many_arguments)]
async fn import_database_with_workflow(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    source_type: &str,
    import_from_uri: &str,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
) -> CliResult<()> {
    use redis_cloud::databases::DatabaseImportRequest;

    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Build request
    let request = DatabaseImportRequest::builder()
        .source_type(source_type)
        .import_from_uri(vec![import_from_uri.to_string()])
        .build();

    // Set up progress spinner
    let pb = Arc::new(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg} [{elapsed_precise}]")
            .unwrap(),
    );
    pb.set_message("Importing data into database...");

    // Create progress callback
    let pb_clone = Arc::clone(&pb);
    let progress_callback = Box::new(move |event: ProgressEvent| match event {
        ProgressEvent::Started { task_id } => {
            pb_clone.set_message(format!("Task {} started", task_id));
        }
        ProgressEvent::Polling {
            status, elapsed, ..
        } => {
            pb_clone.set_message(format!("{} ({:.0}s)", status, elapsed.as_secs_f64()));
        }
        ProgressEvent::Completed { .. } => {
            pb_clone.finish_with_message("Import completed");
        }
        ProgressEvent::Failed { error, .. } => {
            pb_clone.finish_with_message(format!("Failed: {}", error));
        }
    });

    // Call Layer 2 workflow
    #[allow(clippy::cast_possible_wrap)]
    import_database_and_wait(
        &client,
        subscription_id as i32,
        database_id as i32,
        &request,
        Duration::from_secs(async_ops.wait_timeout),
        Some(progress_callback),
    )
    .await
    .map_err(|e| RedisCtlError::ApiError {
        message: e.to_string(),
    })?;

    pb.finish_and_clear();

    // Output result
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            println!(
                "Import into database {}:{} completed successfully",
                subscription_id, database_id
            );
        }
        OutputFormat::Json | OutputFormat::Yaml => {
            let result = json!({
                "message": "Import completed successfully",
                "subscription_id": subscription_id,
                "database_id": database_id
            });
            print_json_or_yaml(result, output_format)?;
        }
    }

    Ok(())
}

/// Legacy import_database implementation (for credentials and --data)
#[allow(clippy::too_many_arguments)]
async fn import_database_legacy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    database_id: u32,
    source_type: Option<&str>,
    import_from_uri: Option<&str>,
    aws_access_key: Option<&str>,
    aws_secret_key: Option<&str>,
    gcs_client_email: Option<&str>,
    gcs_private_key: Option<&str>,
    azure_account_name: Option<&str>,
    azure_account_key: Option<&str>,
    data: Option<&str>,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(st) = source_type {
        request_obj.insert("sourceType".to_string(), json!(st));
    }

    if let Some(uri) = import_from_uri {
        request_obj.insert("importFromUri".to_string(), json!([uri]));
    }

    // AWS credentials
    if aws_access_key.is_some() || aws_secret_key.is_some() {
        let mut credentials = json!({});
        if let Some(key) = aws_access_key {
            credentials["accessKeyId"] = json!(key);
        }
        if let Some(secret) = aws_secret_key {
            credentials["accessSecretKey"] = json!(secret);
        }
        request_obj.insert("credentials".to_string(), credentials);
    }

    // GCS credentials
    if gcs_client_email.is_some() || gcs_private_key.is_some() {
        let mut credentials = json!({});
        if let Some(email) = gcs_client_email {
            credentials["clientEmail"] = json!(email);
        }
        if let Some(key) = gcs_private_key {
            credentials["privateKey"] = json!(key);
        }
        request_obj.insert("credentials".to_string(), credentials);
    }

    // Azure credentials
    if azure_account_name.is_some() || azure_account_key.is_some() {
        let mut credentials = json!({});
        if let Some(name) = azure_account_name {
            credentials["storageAccountName"] = json!(name);
        }
        if let Some(key) = azure_account_key {
            credentials["storageAccountKey"] = json!(key);
        }
        request_obj.insert("credentials".to_string(), credentials);
    }

    // Validate that we have required fields
    if !request_obj.contains_key("sourceType") {
        return Err(RedisCtlError::InvalidInput {
            message: "--source-type is required (or provide via --data JSON)".to_string(),
        });
    }

    if !request_obj.contains_key("importFromUri") {
        return Err(RedisCtlError::InvalidInput {
            message: "--import-from-uri is required (or provide via --data JSON)".to_string(),
        });
    }

    let response = client
        .post_raw(
            &format!(
                "/subscriptions/{}/databases/{}/import",
                subscription_id, database_id
            ),
            request,
        )
        .await
        .context("Failed to start import")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Import initiated successfully",
    )
    .await
}

/// Get database certificate
pub async fn get_certificate(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!(
            "/subscriptions/{}/databases/{}/certificate",
            subscription_id, database_id
        ))
        .await
        .context("Failed to get certificate")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            if let Some(cert) = result.get("certificate") {
                println!("{}", cert.as_str().unwrap_or(""));
            } else {
                println!("No certificate available");
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Slow log entry for table display
#[derive(Tabled)]
struct SlowLogEntry {
    #[tabled(rename = "TIMESTAMP")]
    timestamp: String,
    #[tabled(rename = "DURATION (ms)")]
    duration: String,
    #[tabled(rename = "COMMAND")]
    command: String,
    #[tabled(rename = "CLIENT")]
    client: String,
}

/// Get slow query log
pub async fn get_slow_log(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    limit: u32,
    offset: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!(
            "/subscriptions/{}/databases/{}/slowlog?limit={}&offset={}",
            subscription_id, database_id, limit, offset
        ))
        .await
        .context("Failed to get slow log")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            let mut entries = Vec::new();

            if let Some(Value::Array(logs)) = result.get("entries") {
                for entry in logs {
                    entries.push(SlowLogEntry {
                        timestamp: format_date(extract_field(entry, "timestamp", "")),
                        duration: extract_field(entry, "duration", ""),
                        command: truncate_string(&extract_field(entry, "command", ""), 50),
                        client: extract_field(entry, "client", ""),
                    });
                }
            }

            if entries.is_empty() {
                println!("No slow log entries found");
            } else {
                let mut table = Table::new(entries);
                table.with(Style::blank());
                output_with_pager(&table.to_string());
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Tag entry for table display
#[derive(Tabled)]
struct TagEntry {
    #[tabled(rename = "KEY")]
    key: String,
    #[tabled(rename = "VALUE")]
    value: String,
}

/// List database tags
pub async fn list_tags(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!(
            "/subscriptions/{}/databases/{}/tags",
            subscription_id, database_id
        ))
        .await
        .context("Failed to get tags")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            let mut entries = Vec::new();

            if let Some(Value::Object(tags)) = result.get("tags") {
                for (key, value) in tags {
                    entries.push(TagEntry {
                        key: key.clone(),
                        value: value.as_str().unwrap_or("").to_string(),
                    });
                }
            }

            if entries.is_empty() {
                println!("No tags found");
            } else {
                let mut table = Table::new(entries);
                table.with(Style::blank());
                println!("{}", table);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Add a tag to database
pub async fn add_tag(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    key: &str,
    value: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let request = json!({
        "key": key,
        "value": value
    });

    let response = client
        .post_raw(
            &format!(
                "/subscriptions/{}/databases/{}/tags",
                subscription_id, database_id
            ),
            request,
        )
        .await
        .context("Failed to add tag")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("Tag added successfully: {} = {}", key, value);
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Parse tag string in key=value format
fn parse_tag(tag: &str) -> CliResult<(String, String)> {
    let parts: Vec<&str> = tag.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(RedisCtlError::InvalidInput {
            message: format!("Invalid tag format '{}'. Expected 'key=value' format", tag),
        });
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Update database tags
pub async fn update_tags(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    tags: &[String],
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // Build tags array from --tag parameters
    if !tags.is_empty() {
        let mut tag_array = Vec::new();
        for tag in tags {
            let (key, value) = parse_tag(tag)?;
            tag_array.push(json!({
                "key": key,
                "value": value
            }));
        }
        request_obj.insert("tags".to_string(), json!(tag_array));
    }

    // Validate that we have at least one tag
    if request_obj.is_empty() || !request_obj.contains_key("tags") {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one --tag is required (or provide via --data JSON)".to_string(),
        });
    }

    let response = client
        .put_raw(
            &format!(
                "/subscriptions/{}/databases/{}/tags",
                subscription_id, database_id
            ),
            request,
        )
        .await
        .context("Failed to update tags")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("Tags updated successfully");
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Update a single tag value
pub async fn update_tag(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    key: &str,
    value: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let request = json!({
        "value": value
    });

    let response = client
        .put_raw(
            &format!(
                "/subscriptions/{}/databases/{}/tags/{}",
                subscription_id, database_id, key
            ),
            request,
        )
        .await
        .context("Failed to update tag")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("Tag '{}' updated successfully", key);
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Delete a tag from database
pub async fn delete_tag(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    key: &str,
    output_format: OutputFormat,
    _query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    client
        .delete_raw(&format!(
            "/subscriptions/{}/databases/{}/tags/{}",
            subscription_id, database_id, key
        ))
        .await
        .context("Failed to delete tag")?;

    match output_format {
        OutputFormat::Table => {
            println!("Tag '{}' deleted successfully", key);
        }
        _ => {
            let result = json!({"message": format!("Tag '{}' deleted", key)});
            print_json_or_yaml(result, output_format)?;
        }
    }

    Ok(())
}

/// Flush standard (non-Active-Active) database
pub async fn flush_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;

    // Confirmation prompt unless --force is used
    if !force
        && !super::utils::confirm_action(&format!(
            "Are you sure you want to flush database {}? This will delete all data!",
            id
        ))?
    {
        println!("Flush operation cancelled");
        return Ok(());
    }

    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .put_raw(
            &format!(
                "/subscriptions/{}/databases/{}/flush",
                subscription_id, database_id
            ),
            json!({}),
        )
        .await
        .context("Failed to flush database")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("Database flush initiated");
            if let Some(task_id) = result.get("taskId") {
                println!("Task ID: {}", task_id);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Get available Redis versions for upgrade
pub async fn get_available_versions(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!(
            "/subscriptions/{}/databases/{}/available-target-versions",
            subscription_id, database_id
        ))
        .await
        .context("Failed to get available versions")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            if let Some(versions) = result.as_array() {
                if versions.is_empty() {
                    println!("No upgrade versions available");
                } else {
                    println!("Available Redis versions for upgrade:");
                    for v in versions {
                        if let Some(version) = v.as_str() {
                            println!("  - {}", version);
                        } else {
                            println!("  - {}", v);
                        }
                    }
                }
            } else {
                print_json_or_yaml(result, output_format)?;
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Flush Active-Active database
pub async fn flush_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;

    // Confirmation prompt unless --force is used
    if !force
        && !super::utils::confirm_action(&format!(
            "Are you sure you want to flush Active-Active database {}? This will delete all data!",
            id
        ))?
    {
        println!("Flush operation cancelled");
        return Ok(());
    }

    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .post_raw(
            &format!(
                "/subscriptions/{}/databases/{}/flush",
                subscription_id, database_id
            ),
            json!({}),
        )
        .await
        .context("Failed to flush database")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("Active-Active database flush initiated");
            if let Some(task_id) = result.get("taskId") {
                println!("Task ID: {}", task_id);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Update Active-Active database regions
#[allow(clippy::too_many_arguments)]
pub async fn update_aa_regions(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    name: Option<&str>,
    memory: Option<f64>,
    dataset_size: Option<f64>,
    global_data_persistence: Option<&str>,
    global_password: Option<&str>,
    eviction_policy: Option<&str>,
    enable_tls: Option<bool>,
    oss_cluster: Option<bool>,
    dry_run: bool,
    data: Option<&str>,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(name_val) = name {
        request_obj.insert("name".to_string(), json!(name_val));
    }

    if let Some(mem) = memory {
        request_obj.insert("memoryLimitInGb".to_string(), json!(mem));
    }

    if let Some(dataset) = dataset_size {
        request_obj.insert("datasetSizeInGb".to_string(), json!(dataset));
    }

    if let Some(persistence) = global_data_persistence {
        request_obj.insert("globalDataPersistence".to_string(), json!(persistence));
    }

    if let Some(password) = global_password {
        request_obj.insert("globalPassword".to_string(), json!(password));
    }

    if let Some(eviction) = eviction_policy {
        request_obj.insert("dataEvictionPolicy".to_string(), json!(eviction));
    }

    if let Some(tls) = enable_tls {
        request_obj.insert("enableTls".to_string(), json!(tls));
    }

    if let Some(oss) = oss_cluster {
        request_obj.insert("supportOSSClusterAPI".to_string(), json!(oss));
    }

    if dry_run {
        request_obj.insert("dryRun".to_string(), json!(true));
    }

    // Validate that we have at least one field to update
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--name, --memory, --dataset-size, --global-data-persistence, --global-password, --eviction-policy, --enable-tls, --oss-cluster, or --data)".to_string(),
        });
    }

    let response = client
        .put_raw(
            &format!(
                "/subscriptions/{}/databases/{}/regions",
                subscription_id, database_id
            ),
            request,
        )
        .await
        .context("Failed to update Active-Active database regions")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Active-Active database regions updated successfully",
    )
    .await
}

/// Get Redis version upgrade status
pub async fn get_upgrade_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!(
            "/subscriptions/{}/databases/{}/redis-version-upgrade-status",
            subscription_id, database_id
        ))
        .await
        .context("Failed to get upgrade status")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            if let Some(status) = result.get("status") {
                println!(
                    "Upgrade Status: {}",
                    format_status_text(status.as_str().unwrap_or(""))
                );
            }
            if let Some(current) = result.get("currentVersion") {
                println!("Current Version: {}", current);
            }
            if let Some(target) = result.get("targetVersion") {
                println!("Target Version: {}", target);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Upgrade Redis version
pub async fn upgrade_redis(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: &str,
    version: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let (subscription_id, database_id) = parse_database_id(id)?;
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let request = json!({
        "redisVersion": version
    });

    let response = client
        .post_raw(
            &format!(
                "/subscriptions/{}/databases/{}/upgrade-redis-version",
                subscription_id, database_id
            ),
            request,
        )
        .await
        .context("Failed to upgrade Redis version")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("Redis version upgrade initiated to {}", version);
            if let Some(task_id) = result.get("taskId") {
                println!("Task ID: {}", task_id);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}
