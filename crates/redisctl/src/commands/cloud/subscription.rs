//! Cloud subscription command implementations

use anyhow::Context;
use serde_json::Value;
use tabled::{Table, Tabled, settings::Style};

use crate::cli::{CloudSubscriptionCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::subscription_impl;
use super::utils::*;

/// Subscription row for clean table display
#[derive(Tabled)]
struct SubscriptionRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "PLAN")]
    plan: String,
    #[tabled(rename = "MEMORY")]
    memory: String,
    #[tabled(rename = "DATABASES")]
    databases: String,
    #[tabled(rename = "REGION")]
    region: String,
    #[tabled(rename = "CREATED")]
    created: String,
}

/// Handle cloud subscription commands
pub async fn handle_subscription_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudSubscriptionCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        CloudSubscriptionCommands::List => {
            list_subscriptions(conn_mgr, profile_name, output_format, query).await
        }
        CloudSubscriptionCommands::Get { id } => {
            get_subscription(conn_mgr, profile_name, *id, output_format, query).await
        }
        CloudSubscriptionCommands::Create {
            name,
            dry_run,
            deployment_type,
            payment_method,
            payment_method_id,
            memory_storage,
            persistent_storage_encryption,
            data,
            async_ops,
        } => {
            subscription_impl::create_subscription(
                conn_mgr,
                profile_name,
                name.as_deref(),
                *dry_run,
                deployment_type.as_deref(),
                payment_method,
                *payment_method_id,
                memory_storage,
                persistent_storage_encryption,
                data.as_deref(),
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::Update {
            id,
            name,
            payment_method,
            payment_method_id,
            data,
            dry_run,
            async_ops,
        } => {
            subscription_impl::update_subscription(
                conn_mgr,
                profile_name,
                *id,
                name.as_deref(),
                payment_method.as_deref(),
                *payment_method_id,
                data.as_deref(),
                *dry_run,
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::Delete {
            id,
            force,
            dry_run,
            async_ops,
        } => {
            subscription_impl::delete_subscription(
                conn_mgr,
                profile_name,
                *id,
                *force,
                *dry_run,
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::RedisVersions { subscription } => {
            subscription_impl::get_redis_versions(
                conn_mgr,
                profile_name,
                *subscription,
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::GetPricing { id } => {
            subscription_impl::get_pricing(conn_mgr, profile_name, *id, output_format, query).await
        }
        CloudSubscriptionCommands::GetCidrAllowlist { id } => {
            subscription_impl::get_cidr_allowlist(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
        CloudSubscriptionCommands::UpdateCidrAllowlist {
            id,
            cidrs,
            security_groups,
            data,
        } => {
            subscription_impl::update_cidr_allowlist(
                conn_mgr,
                profile_name,
                *id,
                cidrs,
                security_groups,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::GetMaintenanceWindows { id } => {
            subscription_impl::get_maintenance_windows(
                conn_mgr,
                profile_name,
                *id,
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::UpdateMaintenanceWindows {
            id,
            mode,
            windows,
            data,
        } => {
            subscription_impl::update_maintenance_windows(
                conn_mgr,
                profile_name,
                *id,
                mode.as_deref(),
                windows,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::ListAaRegions { id } => {
            subscription_impl::list_aa_regions(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
        CloudSubscriptionCommands::AddAaRegion {
            id,
            region,
            deployment_cidr,
            vpc_id,
            resp_version,
            dry_run,
            data,
            async_ops,
        } => {
            subscription_impl::add_aa_region(
                conn_mgr,
                profile_name,
                *id,
                region.as_deref(),
                deployment_cidr.as_deref(),
                vpc_id.as_deref(),
                resp_version.as_deref(),
                *dry_run,
                data.as_deref(),
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudSubscriptionCommands::DeleteAaRegions {
            id,
            regions,
            dry_run,
            data,
            force,
            async_ops,
        } => {
            subscription_impl::delete_aa_regions(
                conn_mgr,
                profile_name,
                *id,
                regions,
                *dry_run,
                data.as_deref(),
                *force,
                async_ops,
                output_format,
                query,
            )
            .await
        }
    }
}

/// List all cloud subscriptions with human-friendly output
async fn list_subscriptions(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Try to get both flexible and fixed subscriptions
    let flex_response = client
        .get_raw("/subscriptions")
        .await
        .context("Failed to fetch flexible subscriptions")?;

    let fixed_response = client
        .get_raw("/fixed/subscriptions")
        .await
        .context("Failed to fetch fixed subscriptions")?;

    // Combine subscriptions from both endpoints
    let mut all_subs = Vec::new();

    // Add flexible subscriptions
    if let Some(Value::Array(flex_subs)) = flex_response.get("subscriptions") {
        all_subs.extend(flex_subs.clone());
    }

    // Add fixed subscriptions
    if let Some(Value::Array(fixed_subs)) = fixed_response.get("subscriptions") {
        all_subs.extend(fixed_subs.clone());
    }

    let combined_data = Value::Array(all_subs);

    // Apply JMESPath query if provided
    let data = handle_output(combined_data, output_format, query)?;

    // Format output based on requested format
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_subscriptions_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Print subscriptions in a clean table format
fn print_subscriptions_table(data: &Value) -> CliResult<()> {
    let subscriptions = match data {
        Value::Array(arr) => arr.clone(),
        Value::Object(_) => vec![data.clone()],
        _ => {
            println!("No subscriptions found");
            return Ok(());
        }
    };

    if subscriptions.is_empty() {
        println!("No subscriptions found");
        return Ok(());
    }

    let mut rows = Vec::new();
    for sub in subscriptions {
        rows.push(SubscriptionRow {
            id: extract_field(&sub, "id", "—"),
            name: truncate_string(&extract_field(&sub, "name", "—"), 25),
            status: format_status(extract_field(&sub, "status", "unknown")),
            plan: extract_plan_info(&sub),
            memory: format_memory(&sub),
            databases: count_databases(&sub),
            region: extract_region(&sub),
            created: format_date(extract_field(&sub, "created", "")),
        });
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    output_with_pager(&table.to_string());
    Ok(())
}

/// Get detailed subscription information
async fn get_subscription(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Try flexible subscription first
    let flex_response = client
        .get_raw(&format!("/subscriptions/{}", subscription_id))
        .await
        .ok();

    // If not found, try fixed subscription
    let response = if let Some(resp) = flex_response {
        resp
    } else {
        let fixed_response = client
            .get_raw(&format!("/fixed/subscriptions/{}", subscription_id))
            .await;

        match fixed_response {
            Ok(resp) => resp,
            Err(_) => {
                return Err(anyhow::Error::msg(format!(
                    "Subscription {} not found",
                    subscription_id
                ))
                .into());
            }
        }
    };

    let data = handle_output(response, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_subscription_detail(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Print subscription detail in vertical format
fn print_subscription_detail(data: &Value) -> CliResult<()> {
    let mut rows = Vec::new();

    // Basic information
    if let Some(id) = data.get("id") {
        rows.push(DetailRow {
            field: "ID".to_string(),
            value: id.to_string().trim_matches('"').to_string(),
        });
    }

    if let Some(name) = data.get("name").and_then(|n| n.as_str()) {
        rows.push(DetailRow {
            field: "Name".to_string(),
            value: name.to_string(),
        });
    }

    if let Some(status) = data.get("status").and_then(|s| s.as_str()) {
        rows.push(DetailRow {
            field: "Status".to_string(),
            value: format_status_text(status),
        });
    }

    // Plan information
    if let Some(plan_name) = data.get("planName").and_then(|p| p.as_str()) {
        rows.push(DetailRow {
            field: "Plan".to_string(),
            value: plan_name.to_string(),
        });
    }

    if let Some(price) = data.get("price") {
        let currency = extract_field(data, "priceCurrency", "USD");
        let period = extract_field(data, "pricePeriod", "Month");
        rows.push(DetailRow {
            field: "Pricing".to_string(),
            value: format!("${} {} per {}", price, currency, period.to_lowercase()),
        });
    }

    // Infrastructure
    if let Some(provider) = data.get("provider").and_then(|p| p.as_str()) {
        rows.push(DetailRow {
            field: "Provider".to_string(),
            value: provider.to_string(),
        });
    }

    if let Some(region) = data.get("region").and_then(|r| r.as_str()) {
        rows.push(DetailRow {
            field: "Region".to_string(),
            value: region.to_string(),
        });
    }

    // Memory and storage
    if let Some(size) = data.get("size") {
        let unit = extract_field(data, "sizeMeasurementUnit", "MB");
        rows.push(DetailRow {
            field: "Memory".to_string(),
            value: format!("{} {}", size, unit),
        });
    }

    // Features
    if let Some(persistence) = data.get("supportDataPersistence").and_then(|p| p.as_bool()) {
        rows.push(DetailRow {
            field: "Data Persistence".to_string(),
            value: if persistence {
                "✓ Enabled".to_string()
            } else {
                "✗ Disabled".to_string()
            },
        });
    }

    if let Some(replication) = data.get("supportReplication").and_then(|r| r.as_bool()) {
        rows.push(DetailRow {
            field: "Replication".to_string(),
            value: if replication {
                "✓ Enabled".to_string()
            } else {
                "✗ Disabled".to_string()
            },
        });
    }

    if let Some(clustering) = data.get("supportClustering").and_then(|c| c.as_bool()) {
        rows.push(DetailRow {
            field: "Clustering".to_string(),
            value: if clustering {
                "✓ Enabled".to_string()
            } else {
                "✗ Disabled".to_string()
            },
        });
    }

    // Limits
    if let Some(max_dbs) = data.get("maximumDatabases").and_then(|m| m.as_u64()) {
        rows.push(DetailRow {
            field: "Max Databases".to_string(),
            value: max_dbs.to_string(),
        });
    }

    if let Some(connections) = data.get("connections").and_then(|c| c.as_str()) {
        rows.push(DetailRow {
            field: "Max Connections".to_string(),
            value: connections.to_string(),
        });
    }

    // Dates
    if let Some(created) = data
        .get("creationDate")
        .and_then(|c| c.as_str())
        .or_else(|| data.get("created").and_then(|c| c.as_str()))
    {
        rows.push(DetailRow {
            field: "Created".to_string(),
            value: format_date(created.to_string()),
        });
    }

    if rows.is_empty() {
        println!("No subscription information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    output_with_pager(&table.to_string());
    Ok(())
}

// Helper functions specific to subscriptions

/// Extract plan information (Pro/Fixed, pricing info)
fn extract_plan_info(sub: &Value) -> String {
    // Check if it's a fixed or flexible subscription
    if let Some(plan_name) = sub.get("planName").and_then(|p| p.as_str()) {
        // Fixed subscription with plan name - simplify the display
        if let Some(price) = sub.get("price").and_then(|p| p.as_u64()) {
            format!("${}/mo", price)
        } else {
            truncate_string(plan_name, 15)
        }
    } else if sub.get("paymentMethod").is_some() {
        "Pro".to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Format memory information
fn format_memory(sub: &Value) -> String {
    // Check for fixed subscription size field
    if let Some(size) = sub.get("size").and_then(|s| s.as_f64()) {
        let unit = extract_field(sub, "sizeMeasurementUnit", "MB");
        if unit == "GB" {
            return format_memory_size(size);
        } else if unit == "MB" {
            return format_memory_size(size / 1024.0);
        }
    }

    // Try to get memory from cloudProviders[].regions[].networking
    if let Some(providers) = sub.get("cloudProviders").and_then(|p| p.as_array()) {
        let total_memory_gb: f64 = providers
            .iter()
            .filter_map(|provider| provider.get("regions").and_then(|r| r.as_array()))
            .flatten()
            .filter_map(|region| {
                region
                    .get("memoryStorage")
                    .and_then(|m| m.get("quantity").and_then(|q| q.as_f64()))
            })
            .sum();

        if total_memory_gb > 0.0 {
            return format_memory_size(total_memory_gb);
        }
    }

    // Fallback to memoryStorage field
    if let Some(memory) = sub
        .get("memoryStorage")
        .and_then(|m| m.get("quantity").and_then(|q| q.as_f64()))
    {
        return format_memory_size(memory);
    }

    "—".to_string()
}

/// Count number of databases in subscription
fn count_databases(sub: &Value) -> String {
    // Check for fixed subscription maximumDatabases field
    if let Some(max_dbs) = sub.get("maximumDatabases").and_then(|n| n.as_u64()) {
        return max_dbs.to_string();
    }

    if let Some(dbs) = sub.get("numberOfDatabases").and_then(|n| n.as_u64()) {
        dbs.to_string()
    } else if let Some(dbs) = sub.get("databases").and_then(|d| d.as_array()) {
        dbs.len().to_string()
    } else {
        "0".to_string()
    }
}

/// Extract primary region from subscription
fn extract_region(sub: &Value) -> String {
    // Check for fixed subscription provider/region fields
    if let Some(_provider) = sub.get("provider").and_then(|p| p.as_str())
        && let Some(region) = sub.get("region").and_then(|r| r.as_str())
    {
        // Just return region without provider for compactness
        return region.to_string();
    }

    // Try to get from cloudProviders[].regions[]
    if let Some(providers) = sub.get("cloudProviders").and_then(|p| p.as_array())
        && let Some(first_provider) = providers.first()
    {
        let provider_name = extract_field(first_provider, "provider", "");
        if let Some(regions) = first_provider.get("regions").and_then(|r| r.as_array())
            && let Some(first_region) = regions.first()
        {
            let region_name = extract_field(first_region, "region", "");
            if !provider_name.is_empty() && !region_name.is_empty() {
                return format!("{}/{}", provider_short_name(&provider_name), region_name);
            }
        }
    }

    "—".to_string()
}
