//! Cloud user command implementations

use super::async_utils::{AsyncOperationArgs, handle_async_response};
use super::utils::DetailRow;
use super::utils::*;
use crate::cli::{CloudUserCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use crate::output::print_output;
use anyhow::Context;
use colored::Colorize;
use serde_json::Value;
use tabled::{Table, Tabled, settings::Style};

/// Handle cloud user commands
pub async fn handle_user_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudUserCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        CloudUserCommands::List => list_users(conn_mgr, profile_name, output_format, query).await,
        CloudUserCommands::Get { id } => {
            get_user(conn_mgr, profile_name, *id, output_format, query).await
        }
        CloudUserCommands::Update {
            id,
            name,
            role,
            alerts_email,
            alerts_sms,
        } => {
            update_user(
                conn_mgr,
                profile_name,
                *id,
                name.as_deref(),
                role.as_deref(),
                *alerts_email,
                *alerts_sms,
                output_format,
                query,
            )
            .await
        }
        CloudUserCommands::Delete {
            id,
            force,
            async_ops,
        } => {
            delete_user(
                conn_mgr,
                profile_name,
                *id,
                *force,
                async_ops,
                output_format,
                query,
            )
            .await
        }
    }
}

/// List all cloud users with human-friendly output
async fn list_users(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Get raw user data
    let response = client
        .get_raw("/users")
        .await
        .context("Failed to fetch users")?;

    // Apply JMESPath query if provided
    let data = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    // Format output based on requested format
    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_users_table(&data)?;
        }
        OutputFormat::Json => {
            print_output(data, crate::output::OutputFormat::Json, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
        OutputFormat::Yaml => {
            print_output(data, crate::output::OutputFormat::Yaml, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
    }

    Ok(())
}

/// User row for clean table display
#[derive(Tabled)]
struct UserRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "EMAIL")]
    email: String,
    #[tabled(rename = "ROLE")]
    role: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "MFA")]
    mfa: String,
    #[tabled(rename = "LAST LOGIN")]
    last_login: String,
    #[tabled(rename = "CREATED")]
    created: String,
}

/// Print users in a clean table format
fn print_users_table(data: &Value) -> CliResult<()> {
    let users = if let Some(users_array) = data.get("users").and_then(|u| u.as_array()) {
        users_array.clone()
    } else if let Value::Array(arr) = data {
        arr.clone()
    } else if data.is_object() {
        vec![data.clone()]
    } else {
        println!("No users found");
        return Ok(());
    };

    if users.is_empty() {
        println!("No users found");
        return Ok(());
    }

    let mut rows = Vec::new();
    for user in users {
        rows.push(UserRow {
            id: extract_field(&user, "id", "—"),
            name: extract_user_name(&user),
            email: extract_field(&user, "email", "—"),
            role: format_role(&user),
            status: format_user_status(&user),
            mfa: format_mfa_status(&user),
            last_login: format_last_login(&user),
            created: format_date(extract_field(&user, "signUp", "")),
        });
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    output_with_pager(&table.to_string());
    Ok(())
}

/// Extract user name (first + last or username)
fn extract_user_name(user: &Value) -> String {
    let first = extract_field(user, "firstName", "");
    let last = extract_field(user, "lastName", "");

    if !first.is_empty() || !last.is_empty() {
        format!("{} {}", first, last).trim().to_string()
    } else {
        extract_field(user, "name", "—")
    }
}

/// Format user role with appropriate display
fn format_role(user: &Value) -> String {
    // Check for role field or roles array
    if let Some(role) = user.get("role").and_then(|r| r.as_str()) {
        match role.to_lowercase().as_str() {
            "owner" => role.to_uppercase().blue().to_string(),
            "admin" => role.capitalize().yellow().to_string(),
            "member" | "viewer" => role.capitalize(),
            _ => role.to_string(),
        }
    } else if let Some(roles) = user.get("roles").and_then(|r| r.as_array()) {
        roles
            .iter()
            .filter_map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "Member".to_string()
    }
}

/// Format user status with color coding
fn format_user_status(user: &Value) -> String {
    let status = extract_field(user, "status", "active");
    match status.to_lowercase().as_str() {
        "active" => status.green().to_string(),
        "inactive" | "disabled" => status.red().to_string(),
        "pending" | "invited" => status.yellow().to_string(),
        _ => status,
    }
}

/// Format MFA status
#[allow(clippy::collapsible_if)]
fn format_mfa_status(user: &Value) -> String {
    // Check in options.mfaEnabled
    if let Some(options) = user.get("options") {
        if let Some(mfa) = options.get("mfaEnabled").and_then(|m| m.as_bool()) {
            if mfa {
                return "✓".green().to_string();
            } else {
                return "✗".red().to_string();
            }
        }
    }

    // Fallback checks for other field names
    if let Some(mfa) = user.get("mfaEnabled").and_then(|m| m.as_bool()) {
        if mfa {
            "✓".green().to_string()
        } else {
            "✗".red().to_string()
        }
    } else if let Some(mfa) = user
        .get("twoFactorAuthentication")
        .and_then(|m| m.as_bool())
    {
        if mfa {
            "✓".green().to_string()
        } else {
            "✗".red().to_string()
        }
    } else {
        "—".to_string()
    }
}

/// Format last login time
fn format_last_login(user: &Value) -> String {
    let login_field = extract_field(user, "lastLoginTimestamp", "");
    if login_field.is_empty() {
        let alt_field = extract_field(user, "lastLogin", "");
        if !alt_field.is_empty() {
            return format_date(alt_field);
        }
        return "Never".dimmed().to_string();
    }
    format_date(login_field)
}

/// Get detailed user information
async fn get_user(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    user_id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    let response = client
        .get_raw(&format!("/users/{}", user_id))
        .await
        .map_err(|_| anyhow::Error::msg(format!("User {} not found", user_id)))?;

    let data = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_user_detail(&data)?;
        }
        OutputFormat::Json => {
            print_output(data, crate::output::OutputFormat::Json, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
        OutputFormat::Yaml => {
            print_output(data, crate::output::OutputFormat::Yaml, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
    }

    Ok(())
}

/// Print user detail in vertical format
fn print_user_detail(data: &Value) -> CliResult<()> {
    let mut rows = Vec::new();

    // Basic information
    if let Some(id) = data.get("id") {
        rows.push(DetailRow {
            field: "User ID".to_string(),
            value: id.to_string().trim_matches('"').to_string(),
        });
    }

    // Name (combine first and last if available)
    let first = extract_field(data, "firstName", "");
    let last = extract_field(data, "lastName", "");
    if !first.is_empty() || !last.is_empty() {
        rows.push(DetailRow {
            field: "Name".to_string(),
            value: format!("{} {}", first, last).trim().to_string(),
        });
    } else if let Some(name) = data.get("name").and_then(|n| n.as_str()) {
        rows.push(DetailRow {
            field: "Name".to_string(),
            value: name.to_string(),
        });
    }

    if let Some(email) = data.get("email").and_then(|e| e.as_str()) {
        rows.push(DetailRow {
            field: "Email".to_string(),
            value: email.to_string(),
        });
    }

    // Role and permissions
    if let Some(role) = data.get("role").and_then(|r| r.as_str()) {
        rows.push(DetailRow {
            field: "Role".to_string(),
            value: role.to_string(),
        });
    }

    if let Some(status) = data.get("status").and_then(|s| s.as_str()) {
        rows.push(DetailRow {
            field: "Status".to_string(),
            value: format_status_text(status),
        });
    }

    // Security settings
    #[allow(clippy::collapsible_if)]
    if let Some(options) = data.get("options") {
        if let Some(mfa) = options.get("mfaEnabled").and_then(|m| m.as_bool()) {
            rows.push(DetailRow {
                field: "MFA".to_string(),
                value: if mfa {
                    "✓ Enabled".to_string()
                } else {
                    "✗ Disabled".to_string()
                },
            });
        }
    }

    // API access
    if let Some(has_key) = data.get("hasApiKey").and_then(|h| h.as_bool()) {
        rows.push(DetailRow {
            field: "API Key".to_string(),
            value: if has_key {
                "✓ Configured".to_string()
            } else {
                "✗ Not configured".to_string()
            },
        });
    }

    // Account type
    if let Some(user_type) = data.get("userType").and_then(|t| t.as_str()) {
        rows.push(DetailRow {
            field: "Account Type".to_string(),
            value: user_type.to_string(),
        });
    }

    // Dates
    if let Some(last_login) = data.get("lastLoginTimestamp").and_then(|l| l.as_str()) {
        rows.push(DetailRow {
            field: "Last Login".to_string(),
            value: format_date(last_login.to_string()),
        });
    }

    if let Some(signup) = data.get("signUp").and_then(|s| s.as_str()) {
        rows.push(DetailRow {
            field: "Created".to_string(),
            value: format_date(signup.to_string()),
        });
    }

    // Notifications
    if let Some(alerts) = data.get("alertSettings")
        && let Some(emails) = alerts.get("alertEmails").and_then(|e| e.as_array())
        && !emails.is_empty()
    {
        rows.push(DetailRow {
            field: "Alert Emails".to_string(),
            value: format!("{} configured", emails.len()),
        });
    }

    if rows.is_empty() {
        println!("No user information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    output_with_pager(&table.to_string());
    Ok(())
}

/// Helper to capitalize first letter
trait Capitalize {
    fn capitalize(&self) -> String;
}

impl Capitalize for str {
    fn capitalize(&self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            }
        }
    }
}

/// Update user information
#[allow(clippy::too_many_arguments)]
async fn update_user(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    user_id: u32,
    name: Option<&str>,
    role: Option<&str>,
    alerts_email: Option<bool>,
    alerts_sms: Option<bool>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Build update payload with only specified fields
    let mut payload = serde_json::json!({});

    if let Some(name) = name {
        // Try to split name into first and last
        let parts: Vec<&str> = name.split_whitespace().collect();
        if parts.len() >= 2 {
            payload["firstName"] = serde_json::json!(parts[0]);
            payload["lastName"] = serde_json::json!(parts[1..].join(" "));
        } else {
            payload["firstName"] = serde_json::json!(name);
        }
    }

    if let Some(role) = role {
        // Validate role
        let valid_roles = ["owner", "manager", "viewer", "billing_admin"];
        if !valid_roles.contains(&role.to_lowercase().as_str()) {
            return Err(RedisCtlError::InvalidInput {
                message: format!(
                    "Invalid role '{}'. Valid roles are: owner, manager, viewer, billing_admin",
                    role
                ),
            });
        }
        payload["role"] = serde_json::json!(role.to_lowercase());
    }

    // Handle alert settings
    if alerts_email.is_some() || alerts_sms.is_some() {
        let mut alerts = serde_json::json!({});
        if let Some(email) = alerts_email {
            alerts["emailEnabled"] = serde_json::json!(email);
        }
        if let Some(sms) = alerts_sms {
            alerts["smsEnabled"] = serde_json::json!(sms);
        }
        payload["alertSettings"] = alerts;
    }

    if payload.as_object().unwrap().is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "No fields to update. Please specify at least one field to update."
                .to_string(),
        });
    }

    // Send update request
    let response = client
        .put_raw(&format!("/users/{}", user_id), payload)
        .await
        .context("Failed to update user")?;

    let data = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            println!("User {} updated successfully", user_id);
            print_user_detail(&data)?;
        }
        OutputFormat::Json => {
            print_output(data, crate::output::OutputFormat::Json, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
        OutputFormat::Yaml => {
            print_output(data, crate::output::OutputFormat::Yaml, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
    }

    Ok(())
}

/// Delete a user
async fn delete_user(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    user_id: u32,
    force: bool,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    // Confirm deletion unless forced
    if !force {
        print!("Are you sure you want to delete user {}? [y/N]: ", user_id);
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") && !input.trim().eq_ignore_ascii_case("yes") {
            println!("Operation cancelled");
            return Ok(());
        }
    }

    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Send delete request
    let response = client
        .delete_raw(&format!("/users/{}", user_id))
        .await
        .context("Failed to delete user")?;

    handle_async_response(
        conn_mgr,
        profile_name,
        response,
        async_ops,
        output_format,
        query,
        "user deletion",
    )
    .await
}
