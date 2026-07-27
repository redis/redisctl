//! Implementation of additional subscription commands

use super::async_utils::{AsyncOperationArgs, handle_async_response};
use super::utils::*;
use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use crate::output::print_output;
use anyhow::Context;
use redisctl_core::cloud::delete_subscription_and_wait;
use redisctl_core::progress::ProgressEvent;
use serde_json::{Value, json};
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

/// Create a new subscription
#[allow(clippy::too_many_arguments)]
pub async fn create_subscription(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    name: Option<&str>,
    dry_run: bool,
    deployment_type: Option<&str>,
    payment_method: &str,
    payment_method_id: Option<i32>,
    memory_storage: &str,
    persistent_storage_encryption: &str,
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
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // Validate: cloudProviders and databases are required
    if data.is_none() {
        return Err(RedisCtlError::InvalidInput {
            message: "--data is required for subscription creation (must include cloudProviders and databases arrays). Use first-class parameters (--name, --payment-method, etc.) to override specific values.".to_string(),
        });
    }

    // CLI parameters override JSON values
    if let Some(name_val) = name {
        request_obj.insert("name".to_string(), serde_json::json!(name_val));
    }

    // Always set dry_run if specified (even if false, to be explicit)
    if dry_run {
        request_obj.insert("dryRun".to_string(), serde_json::json!(true));
    }

    if let Some(deployment) = deployment_type {
        request_obj.insert("deploymentType".to_string(), serde_json::json!(deployment));
    }

    // Always set payment method (has default)
    request_obj.insert(
        "paymentMethod".to_string(),
        serde_json::json!(payment_method),
    );

    if let Some(pm_id) = payment_method_id {
        request_obj.insert("paymentMethodId".to_string(), serde_json::json!(pm_id));
    } else if payment_method == "credit-card" && !request_obj.contains_key("paymentMethodId") {
        return Err(RedisCtlError::InvalidInput {
            message: "--payment-method-id is required when using credit-card payment method"
                .to_string(),
        });
    }

    // Always set memory storage (has default)
    request_obj.insert(
        "memoryStorage".to_string(),
        serde_json::json!(memory_storage),
    );

    // Always set persistent storage encryption (has default)
    request_obj.insert(
        "persistentStorageEncryption".to_string(),
        serde_json::json!(persistent_storage_encryption),
    );

    // Validate required nested structures
    if !request_obj.contains_key("cloudProviders") {
        return Err(RedisCtlError::InvalidInput {
            message: "cloudProviders array is required in --data (defines provider, regions, and networking)".to_string(),
        });
    }

    if !request_obj.contains_key("databases") {
        return Err(RedisCtlError::InvalidInput {
            message:
                "databases array is required in --data (at least one database specification needed)"
                    .to_string(),
        });
    }

    let response = client
        .post_raw("/subscriptions", request)
        .await
        .context("Failed to create subscription")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Subscription created successfully",
    )
    .await
}

/// Update subscription configuration
#[allow(clippy::too_many_arguments)]
pub async fn update_subscription(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    name: Option<&str>,
    payment_method: Option<&str>,
    payment_method_id: Option<i32>,
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
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(name_val) = name {
        request_obj.insert("name".to_string(), serde_json::json!(name_val));
    }

    if let Some(pm) = payment_method {
        request_obj.insert("paymentMethod".to_string(), serde_json::json!(pm));
    }

    if let Some(pm_id) = payment_method_id {
        request_obj.insert("paymentMethodId".to_string(), serde_json::json!(pm_id));
    }

    // Validate that we have at least one field to update
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--name, --payment-method, --payment-method-id, or --data)".to_string(),
        });
    }

    if dry_run {
        request_obj.insert("dryRun".to_string(), serde_json::json!(true));
    }

    let response = client
        .put_raw(&format!("/subscriptions/{}", id), request)
        .await
        .context("Failed to update subscription")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Subscription updated successfully",
    )
    .await
}

/// Delete a subscription
#[allow(clippy::too_many_arguments)]
pub async fn delete_subscription(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    force: bool,
    dry_run: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    if dry_run {
        eprintln!("Would delete subscription {}.", id);
        eprintln!();
        eprintln!("No changes were made.");
        return Ok(());
    }

    // Confirmation prompt unless --force is used
    if !force {
        super::utils::confirm_action(&format!(
            "Are you sure you want to delete subscription {}? This will delete all databases in the subscription!",
            id
        ))?;
    }

    // Use Layer 2 workflow when --wait is specified
    if async_ops.wait {
        delete_subscription_with_workflow(conn_mgr, profile_name, id, async_ops, output_format)
            .await
    } else {
        delete_subscription_legacy(conn_mgr, profile_name, id, async_ops, output_format, query)
            .await
    }
}

/// Delete subscription using Layer 2 workflow (with --wait)
async fn delete_subscription_with_workflow(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Build progress callback for spinner updates
    let progress_callback: Option<Box<dyn Fn(ProgressEvent) + Send + Sync>> =
        Some(Box::new(|event| {
            if let ProgressEvent::Polling {
                status, elapsed, ..
            } = event
            {
                eprintln!("Status: {} ({:.0}s elapsed)", status, elapsed.as_secs());
            }
        }));

    // Use Layer 2 workflow
    delete_subscription_and_wait(
        &client,
        id as i32,
        Duration::from_secs(async_ops.wait_timeout),
        progress_callback,
    )
    .await
    .context("Failed to delete subscription")?;

    // Output result
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            println!("Subscription {} deleted successfully", id);
        }
        OutputFormat::Json | OutputFormat::Yaml => {
            let result = json!({
                "subscription_id": id,
                "status": "deleted"
            });
            print_json_or_yaml(result, output_format)?;
        }
    }

    Ok(())
}

/// Delete subscription using legacy approach (without --wait)
async fn delete_subscription_legacy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .delete_raw(&format!("/subscriptions/{}", id))
        .await
        .context("Failed to delete subscription")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Subscription deletion initiated",
    )
    .await
}

/// Redis version info for table display
#[derive(Tabled)]
struct RedisVersionRow {
    #[tabled(rename = "VERSION")]
    version: String,
    #[tabled(rename = "RELEASE DATE")]
    release_date: String,
    #[tabled(rename = "END OF LIFE")]
    end_of_life: String,
}

/// Get available Redis versions
pub async fn get_redis_versions(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: Option<u32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let path = if let Some(sub_id) = subscription_id {
        format!("/redis-versions?subscription={}", sub_id)
    } else {
        "/redis-versions".to_string()
    };

    let response = client
        .get_raw(&path)
        .await
        .context("Failed to get Redis versions")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            let mut rows = Vec::new();

            if let Some(Value::Array(versions)) = result.get("versions") {
                for version in versions {
                    rows.push(RedisVersionRow {
                        version: extract_field(version, "version", ""),
                        release_date: format_date(extract_field(version, "releaseDate", "")),
                        end_of_life: format_date(extract_field(version, "endOfLife", "")),
                    });
                }
            }

            if rows.is_empty() {
                println!("No Redis versions found");
            } else {
                let mut table = Table::new(rows);
                table.with(Style::blank());
                println!("{}", table);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Get subscription pricing
pub async fn get_pricing(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!("/subscriptions/{}/pricing", id))
        .await
        .context("Failed to get subscription pricing")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            if let Some(price) = result.get("estimatedMonthlyTotal") {
                println!("Estimated Monthly Total: ${}", price);
            }
            if let Some(currency) = result.get("currency") {
                println!("Currency: {}", currency);
            }
            if let Some(details) = result.get("shards") {
                println!(
                    "Shard Pricing Details: {}",
                    serde_json::to_string_pretty(details)?
                );
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// CIDR entry for table display
#[derive(Tabled)]
struct CidrEntry {
    #[tabled(rename = "CIDR")]
    cidr: String,
    #[tabled(rename = "DESCRIPTION")]
    description: String,
}

/// Get CIDR allowlist
pub async fn get_cidr_allowlist(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!("/subscriptions/{}/cidr", id))
        .await
        .context("Failed to get CIDR allowlist")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            let mut entries = Vec::new();

            if let Some(Value::Array(cidrs)) = result.get("cidrs") {
                for cidr in cidrs {
                    entries.push(CidrEntry {
                        cidr: extract_field(cidr, "cidr", ""),
                        description: extract_field(cidr, "description", ""),
                    });
                }
            }

            if entries.is_empty() {
                println!("No CIDR blocks configured");
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

/// Update CIDR allowlist
#[allow(clippy::too_many_arguments)]
pub async fn update_cidr_allowlist(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    cidrs: &[String],
    security_groups: &[String],
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // Build cidrIps array from --cidr parameters
    if !cidrs.is_empty() {
        let cidr_ips: Vec<Value> = cidrs
            .iter()
            .map(|cidr| serde_json::json!({ "cidr": cidr }))
            .collect();
        request_obj.insert("cidrIps".to_string(), Value::Array(cidr_ips));
    }

    // Build securityGroupIds array from --security-group parameters
    if !security_groups.is_empty() {
        request_obj.insert(
            "securityGroupIds".to_string(),
            serde_json::json!(security_groups),
        );
    }

    // Validate that we have at least one field to update
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--cidr, --security-group, or --data)"
                .to_string(),
        });
    }

    let response = client
        .put_raw(&format!("/subscriptions/{}/cidr", id), request)
        .await
        .context("Failed to update CIDR allowlist")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("CIDR allowlist updated successfully");
            if let Some(task_id) = result.get("taskId") {
                println!("Task ID: {}", task_id);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Maintenance window for table display
#[derive(Tabled)]
struct MaintenanceWindowRow {
    #[tabled(rename = "MODE")]
    mode: String,
    #[tabled(rename = "WINDOW")]
    window: String,
}

/// Get maintenance windows
pub async fn get_maintenance_windows(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!("/subscriptions/{}/maintenance-windows", id))
        .await
        .context("Failed to get maintenance windows")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            let mut rows = Vec::new();

            if let Some(mode) = result.get("mode") {
                let window_text = if let Some(windows) = result.get("windows") {
                    serde_json::to_string(windows).unwrap_or_else(|_| "N/A".to_string())
                } else {
                    "N/A".to_string()
                };

                rows.push(MaintenanceWindowRow {
                    mode: mode.as_str().unwrap_or("").to_string(),
                    window: window_text,
                });
            }

            if rows.is_empty() {
                println!("No maintenance windows configured");
            } else {
                let mut table = Table::new(rows);
                table.with(Style::blank());
                println!("{}", table);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Update maintenance windows
#[allow(clippy::too_many_arguments)]
pub async fn update_maintenance_windows(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    mode: Option<&str>,
    windows: &[String],
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(mode_val) = mode {
        request_obj.insert("mode".to_string(), serde_json::json!(mode_val));
    }

    // Build windows array from --window parameters
    // Format: "DAY:HH:MM-HH:MM" e.g., "Monday:03:00-07:00"
    if !windows.is_empty() {
        let window_objects: Result<Vec<Value>, _> = windows
            .iter()
            .map(|w| parse_maintenance_window(w))
            .collect();
        match window_objects {
            Ok(objs) => {
                request_obj.insert("windows".to_string(), Value::Array(objs));
            }
            Err(e) => return Err(e),
        }
    }

    // Validate that we have at least one field to update
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--mode, --window, or --data)"
                .to_string(),
        });
    }

    let response = client
        .put_raw(
            &format!("/subscriptions/{}/maintenance-windows", id),
            request,
        )
        .await
        .context("Failed to update maintenance windows")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            println!("Maintenance windows updated successfully");
            if let Some(task_id) = result.get("taskId") {
                println!("Task ID: {}", task_id);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Parse maintenance window string into JSON object
/// Format: "DAY:HH:MM-HH:MM" or "DAY:HH:MM:DURATION"
fn parse_maintenance_window(window: &str) -> CliResult<Value> {
    let parts: Vec<&str> = window.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(RedisCtlError::InvalidInput {
            message: format!(
                "Invalid window format '{}'. Expected 'DAY:HH:MM-HH:MM' (e.g., 'Monday:03:00-07:00')",
                window
            ),
        });
    }

    let day = parts[0];
    let time_part = parts[1];

    // Parse time range (HH:MM-HH:MM) or start time with duration
    if let Some((start, end)) = time_part.split_once('-') {
        Ok(serde_json::json!({
            "dayOfWeek": day,
            "startHour": parse_hour(start)?,
            "endHour": parse_hour(end)?
        }))
    } else {
        // Just start hour provided
        Ok(serde_json::json!({
            "dayOfWeek": day,
            "startHour": parse_hour(time_part)?
        }))
    }
}

/// Parse hour string (HH:MM or HH) to integer hour
fn parse_hour(time: &str) -> CliResult<i32> {
    let hour_str = time.split(':').next().unwrap_or(time);
    hour_str
        .parse::<i32>()
        .map_err(|_| RedisCtlError::InvalidInput {
            message: format!("Invalid hour '{}'. Expected HH or HH:MM format", time),
        })
}

/// Active-Active region for table display
#[derive(Tabled)]
struct AaRegionRow {
    #[tabled(rename = "REGION")]
    region: String,
    #[tabled(rename = "PROVIDER")]
    provider: String,
    #[tabled(rename = "STATUS")]
    status: String,
}

/// List Active-Active regions
pub async fn list_aa_regions(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!("/subscriptions/{}/regions", id))
        .await
        .context("Failed to get Active-Active regions")?;

    let result = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Table => {
            let mut rows = Vec::new();

            if let Some(Value::Array(regions)) = result.get("regions") {
                for region in regions {
                    rows.push(AaRegionRow {
                        region: extract_field(region, "region", ""),
                        provider: extract_field(region, "provider", ""),
                        status: format_status_text(&extract_field(region, "status", "")),
                    });
                }
            }

            if rows.is_empty() {
                println!("No Active-Active regions found");
            } else {
                let mut table = Table::new(rows);
                table.with(Style::blank());
                println!("{}", table);
            }
        }
        _ => print_json_or_yaml(result, output_format)?,
    }

    Ok(())
}

/// Add region to Active-Active subscription
#[allow(clippy::too_many_arguments)]
pub async fn add_aa_region(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    region: Option<&str>,
    deployment_cidr: Option<&str>,
    vpc_id: Option<&str>,
    resp_version: Option<&str>,
    dry_run: bool,
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
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(region_val) = region {
        request_obj.insert("region".to_string(), serde_json::json!(region_val));
    }

    if let Some(cidr) = deployment_cidr {
        request_obj.insert("deploymentCIDR".to_string(), serde_json::json!(cidr));
    }

    if let Some(vpc) = vpc_id {
        request_obj.insert("vpcId".to_string(), serde_json::json!(vpc));
    }

    if let Some(resp) = resp_version {
        request_obj.insert("respVersion".to_string(), serde_json::json!(resp));
    }

    if dry_run {
        request_obj.insert("dryRun".to_string(), serde_json::json!(true));
    }

    // Validate that region is provided
    if !request_obj.contains_key("region") {
        return Err(RedisCtlError::InvalidInput {
            message: "--region is required (or provide via --data JSON)".to_string(),
        });
    }

    let response = client
        .post_raw(&format!("/subscriptions/{}/regions", id), request)
        .await
        .context("Failed to add Active-Active region")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Active-Active region added successfully",
    )
    .await
}

/// Delete regions from Active-Active subscription
#[allow(clippy::too_many_arguments)]
pub async fn delete_aa_regions(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    regions: &[String],
    dry_run: bool,
    data: Option<&str>,
    force: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    // Confirmation prompt unless --force is used
    if !force {
        let region_list = if regions.is_empty() {
            "specified regions".to_string()
        } else {
            regions.join(", ")
        };
        super::utils::confirm_action(&format!(
            "Are you sure you want to delete regions ({}) from Active-Active subscription {}?",
            region_list, id
        ))?;
    }

    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // Build regions array from --region parameters
    if !regions.is_empty() {
        request_obj.insert("regions".to_string(), serde_json::json!(regions));
    }

    if dry_run {
        request_obj.insert("dryRun".to_string(), serde_json::json!(true));
    }

    // Validate that regions are provided
    if !request_obj.contains_key("regions") {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one --region is required (or provide via --data JSON)".to_string(),
        });
    }

    // Use DELETE with body - need to use post_raw with custom method or adjust API
    // The Redis Cloud API uses DELETE with a request body for this endpoint
    let response = client
        .delete_with_body(&format!("/subscriptions/{}/regions", id), request)
        .await
        .context("Failed to delete Active-Active regions")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "Active-Active regions deletion initiated",
    )
    .await
}
