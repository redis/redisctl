//! Cloud account command implementations

use anyhow::Context;
use redis_cloud::AccountHandler;
use serde_json::Value;
use tabled::{Table, settings::Style};

use crate::cli::{CloudAccountCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::utils::*;

/// Handle cloud account commands
pub async fn handle_account_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudAccountCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        CloudAccountCommands::Get => {
            get_account(conn_mgr, profile_name, output_format, query).await
        }
        CloudAccountCommands::GetPaymentMethods => {
            get_payment_methods(conn_mgr, profile_name, output_format, query).await
        }
        CloudAccountCommands::ListRegions { provider } => {
            list_regions(
                conn_mgr,
                profile_name,
                provider.clone(),
                output_format,
                query,
            )
            .await
        }
        CloudAccountCommands::ListModules => {
            list_modules(conn_mgr, profile_name, output_format, query).await
        }
        CloudAccountCommands::GetPersistenceOptions => {
            get_persistence_options(conn_mgr, profile_name, output_format, query).await
        }
        CloudAccountCommands::GetSystemLogs { limit, offset } => {
            get_system_logs(
                conn_mgr,
                profile_name,
                *limit,
                *offset,
                output_format,
                query,
            )
            .await
        }
        CloudAccountCommands::GetSessionLogs { limit, offset } => {
            get_session_logs(
                conn_mgr,
                profile_name,
                *limit,
                *offset,
                output_format,
                query,
            )
            .await
        }
        CloudAccountCommands::GetSearchScaling => {
            get_search_scaling(conn_mgr, profile_name, output_format, query).await
        }
    }
}

/// Get account information
async fn get_account(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw("/")
        .await
        .context("Failed to fetch account")?;

    let data = handle_output(response, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_account_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Print payment methods in table format
fn print_payment_methods_table(data: &Value) -> CliResult<()> {
    let methods = data.get("paymentMethods").and_then(|p| p.as_array());

    if let Some(methods) = methods {
        if methods.is_empty() {
            println!("No payment methods configured");
            return Ok(());
        }

        let mut rows = Vec::new();
        for method in methods {
            let id = method.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let type_ = method
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let last4 = method
                .get("last4Digits")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let exp = method
                .get("expirationDate")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            rows.push(PaymentMethodRow {
                id: id.to_string(),
                payment_type: type_.to_string(),
                last_4: last4.to_string(),
                expiration: exp.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No payment methods data available");
    }
    Ok(())
}

/// Print regions in table format
fn print_regions_table(data: &Value) -> CliResult<()> {
    let regions = data.get("regions").and_then(|r| r.as_array());

    if let Some(regions) = regions {
        if regions.is_empty() {
            println!("No regions available");
            return Ok(());
        }

        let mut rows = Vec::new();
        for region in regions {
            let name = region.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let provider = region
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let region_id = region
                .get("regionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            rows.push(RegionRow {
                name: name.to_string(),
                provider: provider.to_string(),
                region_id: region_id.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No regions data available");
    }
    Ok(())
}

/// Print modules in table format
fn print_modules_table(data: &Value) -> CliResult<()> {
    let modules = data.get("modules").and_then(|m| m.as_array());

    if let Some(modules) = modules {
        if modules.is_empty() {
            println!("No modules available");
            return Ok(());
        }

        let mut rows = Vec::new();
        for module in modules {
            let name = module.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let description = module
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let version = module.get("version").and_then(|v| v.as_str()).unwrap_or("");

            rows.push(ModuleRow {
                name: name.to_string(),
                description: description.to_string(),
                version: version.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No modules data available");
    }
    Ok(())
}

/// Print persistence options in table format
fn print_persistence_table(data: &Value) -> CliResult<()> {
    let options = data.get("dataPersistence").and_then(|d| d.as_array());

    if let Some(options) = options {
        if options.is_empty() {
            println!("No persistence options available");
            return Ok(());
        }

        let mut rows = Vec::new();
        for option in options {
            let name = option.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let description = option
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            rows.push(PersistenceRow {
                name: name.to_string(),
                description: description.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No persistence options data available");
    }
    Ok(())
}

/// Print system logs in table format
fn print_system_logs_table(data: &Value) -> CliResult<()> {
    let entries = data.get("entries").and_then(|e| e.as_array());

    if let Some(entries) = entries {
        if entries.is_empty() {
            println!("No system log entries");
            return Ok(());
        }

        let mut rows = Vec::new();
        for entry in entries {
            let time = entry.get("time").and_then(|v| v.as_str()).unwrap_or("");
            let originator = entry
                .get("originator")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let resource = entry.get("resource").and_then(|v| v.as_str()).unwrap_or("");
            let action = entry.get("action").and_then(|v| v.as_str()).unwrap_or("");

            rows.push(LogRow {
                time: format_date(time.to_string()),
                originator: originator.to_string(),
                resource: resource.to_string(),
                action: action.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No system log data available");
    }
    Ok(())
}

/// Print session logs in table format
fn print_session_logs_table(data: &Value) -> CliResult<()> {
    let entries = data.get("entries").and_then(|e| e.as_array());

    if let Some(entries) = entries {
        if entries.is_empty() {
            println!("No session log entries");
            return Ok(());
        }

        let mut rows = Vec::new();
        for entry in entries {
            let time = entry.get("time").and_then(|v| v.as_str()).unwrap_or("");
            let originator = entry
                .get("originator")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let resource = entry.get("resource").and_then(|v| v.as_str()).unwrap_or("");
            let action = entry.get("action").and_then(|v| v.as_str()).unwrap_or("");

            rows.push(LogRow {
                time: format_date(time.to_string()),
                originator: originator.to_string(),
                resource: resource.to_string(),
                action: action.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No session log data available");
    }
    Ok(())
}

/// Print search scaling factors in table format
fn print_search_scaling_table(data: &Value) -> CliResult<()> {
    let factors = data.get("searchScalingFactors").and_then(|s| s.as_array());

    if let Some(factors) = factors {
        if factors.is_empty() {
            println!("No search scaling factors available");
            return Ok(());
        }

        let mut rows = Vec::new();
        for factor in factors {
            let value = factor.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let description = factor
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            rows.push(ScalingRow {
                factor: value.to_string(),
                description: description.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No search scaling data available");
    }
    Ok(())
}

// Table row structures for formatting
#[derive(tabled::Tabled)]
struct PaymentMethodRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Type")]
    payment_type: String,
    #[tabled(rename = "Last 4")]
    last_4: String,
    #[tabled(rename = "Expiration")]
    expiration: String,
}

#[derive(tabled::Tabled)]
struct RegionRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Provider")]
    provider: String,
    #[tabled(rename = "Region ID")]
    region_id: String,
}

#[derive(tabled::Tabled)]
struct ModuleRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Description")]
    description: String,
    #[tabled(rename = "Version")]
    version: String,
}

#[derive(tabled::Tabled)]
struct PersistenceRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Description")]
    description: String,
}

#[derive(tabled::Tabled)]
struct LogRow {
    #[tabled(rename = "Time")]
    time: String,
    #[tabled(rename = "Originator")]
    originator: String,
    #[tabled(rename = "Resource")]
    resource: String,
    #[tabled(rename = "Action")]
    action: String,
}

#[derive(tabled::Tabled)]
struct ScalingRow {
    #[tabled(rename = "Factor")]
    factor: String,
    #[tabled(rename = "Description")]
    description: String,
}

/// Print account info in clean table format
fn print_account_table(data: &Value) -> CliResult<()> {
    let mut rows = Vec::new();

    // Extract account object from response
    let account = data.get("account").unwrap_or(data);

    // Add account fields in a logical order
    if let Some(id) = account.get("id") {
        rows.push(DetailRow {
            field: "Account ID".to_string(),
            value: id.to_string().trim_matches('"').to_string(),
        });
    }

    if let Some(name) = account.get("name").and_then(|n| n.as_str()) {
        rows.push(DetailRow {
            field: "Name".to_string(),
            value: name.to_string(),
        });
    }

    // Try to get email from key.owner.email or direct email field
    let email = account
        .get("key")
        .and_then(|k| k.get("owner"))
        .and_then(|o| o.get("email"))
        .and_then(|e| e.as_str())
        .or_else(|| account.get("email").and_then(|e| e.as_str()));

    if let Some(email) = email {
        rows.push(DetailRow {
            field: "Email".to_string(),
            value: email.to_string(),
        });
    }

    if let Some(company) = account.get("companyName").and_then(|c| c.as_str()) {
        rows.push(DetailRow {
            field: "Company".to_string(),
            value: company.to_string(),
        });
    }

    // Use createdTimestamp field
    if let Some(created) = account
        .get("createdTimestamp")
        .and_then(|c| c.as_str())
        .or_else(|| account.get("created").and_then(|c| c.as_str()))
    {
        rows.push(DetailRow {
            field: "Created".to_string(),
            value: format_date(created.to_string()),
        });
    }

    if let Some(status) = account.get("status").and_then(|s| s.as_str()) {
        rows.push(DetailRow {
            field: "Status".to_string(),
            value: format_status_text(status),
        });
    }

    if let Some(payment) = account.get("paymentMethods").and_then(|p| p.as_array()) {
        rows.push(DetailRow {
            field: "Payment Methods".to_string(),
            value: format!("{} configured", payment.len()),
        });
    }

    // Add API key info if present
    if let Some(key) = account.get("key").and_then(|k| k.as_object()) {
        if let Some(key_name) = key.get("name").and_then(|n| n.as_str()) {
            rows.push(DetailRow {
                field: "API Key".to_string(),
                value: key_name.to_string(),
            });
        }
        if let Some(allowed_ips) = key.get("allowedSourceIps").and_then(|i| i.as_array()) {
            let ips_str = allowed_ips
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            rows.push(DetailRow {
                field: "Allowed IPs".to_string(),
                value: ips_str,
            });
        }
    }

    if rows.is_empty() {
        println!("No account information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    output_with_pager(&table.to_string());
    Ok(())
}

/// Get payment methods
async fn get_payment_methods(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_account_payment_methods()
        .await
        .context("Failed to fetch payment methods")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_payment_methods_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// List supported regions
async fn list_regions(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    provider: Option<String>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_supported_regions(provider)
        .await
        .context("Failed to fetch regions")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_regions_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// List supported modules
async fn list_modules(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_supported_database_modules()
        .await
        .context("Failed to fetch modules")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_modules_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Get data persistence options
async fn get_persistence_options(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_data_persistence_options()
        .await
        .context("Failed to fetch persistence options")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_persistence_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Get system logs
async fn get_system_logs(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_account_system_logs(offset.map(|v| v as i32), limit.map(|v| v as i32))
        .await
        .context("Failed to fetch system logs")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_system_logs_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Get session logs
async fn get_session_logs(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_account_session_logs(offset.map(|v| v as i32), limit.map(|v| v as i32))
        .await
        .context("Failed to fetch session logs")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_session_logs_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Get search scaling factors
async fn get_search_scaling(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_supported_search_scaling_factors()
        .await
        .context("Failed to fetch search scaling factors")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_search_scaling_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}
