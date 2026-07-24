use crate::cli::OutputFormat;
use crate::commands::cloud::async_utils::{AsyncOperationArgs, handle_async_response};
use crate::commands::cloud::utils::{confirm_action, handle_output, print_formatted_output};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};

use anyhow::Context;
use colored::Colorize;
use redis_cloud::CloudClient;
use serde_json::{Value, json};
use tabled::builder::Builder;
use tabled::settings::Style;

/// Parameters for cloud account operations that support async operations
pub struct CloudAccountOperationParams<'a> {
    pub conn_mgr: &'a ConnectionManager,
    pub profile_name: Option<&'a str>,
    pub client: &'a CloudClient,
    pub async_ops: &'a AsyncOperationArgs,
    pub output_format: OutputFormat,
    pub query: Option<&'a str>,
}

/// Parameters for creating a cloud account
pub struct CreateParams<'a> {
    pub name: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub access_key_id: Option<&'a str>,
    pub access_secret_key: Option<&'a str>,
    pub console_username: Option<&'a str>,
    pub console_password: Option<&'a str>,
    pub sign_in_login_url: Option<&'a str>,
    pub data: Option<&'a str>,
}

/// Parameters for updating a cloud account
pub struct UpdateParams<'a> {
    pub name: Option<&'a str>,
    pub access_key_id: Option<&'a str>,
    pub access_secret_key: Option<&'a str>,
    pub console_username: Option<&'a str>,
    pub console_password: Option<&'a str>,
    pub sign_in_login_url: Option<&'a str>,
    pub data: Option<&'a str>,
}

/// Read JSON data from string or file (prefixed with @)
fn read_json_data(data: &str) -> CliResult<serde_json::Value> {
    let json_str = if let Some(file_path) = data.strip_prefix('@') {
        std::fs::read_to_string(file_path).map_err(|e| RedisCtlError::InvalidInput {
            message: format!("Failed to read file {}: {}", file_path, e),
        })?
    } else {
        data.to_string()
    };
    serde_json::from_str(&json_str).map_err(|e| RedisCtlError::InvalidInput {
        message: format!("Invalid JSON: {}", e),
    })
}

pub async fn handle_list(
    client: &CloudClient,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = client
        .get_raw("/cloud-accounts")
        .await
        .context("Failed to list cloud accounts")?;

    // For table output, create a formatted table
    if matches!(output_format, OutputFormat::Table | OutputFormat::Auto)
        && query.is_none()
        && let Some(accounts) = result.get("cloudAccounts").and_then(|a| a.as_array())
    {
        let mut builder = Builder::default();
        builder.push_record(["ID", "Name", "Provider", "Status", "Created"]);

        for account in accounts {
            let id = account.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let name = account.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let provider = account
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = account.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let created_timestamp = account
                .get("createdTimestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let status_str = match status {
                "active" => status.green().to_string(),
                "inactive" => status.red().to_string(),
                _ => status.to_string(),
            };

            builder.push_record([
                &id.to_string(),
                name,
                provider,
                &status_str,
                created_timestamp,
            ]);
        }

        println!("{}", builder.build().with(Style::blank()));
        return Ok(());
    }

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn handle_get(
    client: &CloudClient,
    account_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = client
        .get_raw(&format!("/cloud-accounts/{}", account_id))
        .await
        .context("Failed to get cloud account")?;

    // For table output, create a detailed view
    if matches!(output_format, OutputFormat::Table | OutputFormat::Auto) && query.is_none() {
        let mut builder = Builder::default();
        builder.push_record(["Field", "Value"]);

        if let Some(obj) = result.as_object() {
            for (key, value) in obj {
                // Mask sensitive fields
                let display_value =
                    if key.contains("secret") || key.contains("password") || key.contains("key") {
                        "***REDACTED***".to_string()
                    } else {
                        match value {
                            Value::String(s) => s.clone(),
                            _ => value.to_string(),
                        }
                    };
                builder.push_record([key.as_str(), &display_value]);
            }
        }

        println!("{}", builder.build().with(Style::blank()));
        return Ok(());
    }

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn handle_create(
    params: &CloudAccountOperationParams<'_>,
    create_params: &CreateParams<'_>,
) -> CliResult<()> {
    // Start with data from --data if provided, otherwise empty object
    let mut payload = if let Some(data_str) = create_params.data {
        let mut val = read_json_data(data_str)?;

        // If the input is a GCP service account JSON, convert it to the cloud account format
        if val.get("project_id").is_some() {
            // This is a GCP service account JSON
            let provider_payload = json!({
                "provider": "GCP",
                "name": val.get("client_email")
                    .and_then(|v| v.as_str())
                    .unwrap_or("GCP Cloud Account"),
                "serviceAccountJson": serde_json::to_string(&val)?
            });
            val = provider_payload;
        }
        val
    } else {
        serde_json::json!({})
    };

    let payload_obj = payload
        .as_object_mut()
        .ok_or_else(|| RedisCtlError::InvalidInput {
            message: "JSON data must be an object".to_string(),
        })?;

    // Apply first-class params (override JSON values)
    if let Some(n) = create_params.name {
        payload_obj.insert("name".to_string(), json!(n));
    }
    if let Some(p) = create_params.provider {
        payload_obj.insert("provider".to_string(), json!(p));
    }
    if let Some(aki) = create_params.access_key_id {
        payload_obj.insert("accessKeyId".to_string(), json!(aki));
    }
    if let Some(ask) = create_params.access_secret_key {
        payload_obj.insert("accessSecretKey".to_string(), json!(ask));
    }
    if let Some(cu) = create_params.console_username {
        payload_obj.insert("consoleUsername".to_string(), json!(cu));
    }
    if let Some(cp) = create_params.console_password {
        payload_obj.insert("consolePassword".to_string(), json!(cp));
    }
    if let Some(url) = create_params.sign_in_login_url {
        payload_obj.insert("signInLoginUrl".to_string(), json!(url));
    }

    // Validate required fields based on provider
    let provider = payload_obj
        .get("provider")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RedisCtlError::InvalidInput {
            message: "--provider is required (or provide via --data JSON)".to_string(),
        })?;

    match provider {
        "AWS" => {
            if payload_obj.get("accessKeyId").is_none() {
                return Err(RedisCtlError::InvalidInput {
                    message: "AWS provider requires --access-key-id".to_string(),
                });
            }
            if payload_obj.get("accessSecretKey").is_none() {
                return Err(RedisCtlError::InvalidInput {
                    message: "AWS provider requires --access-secret-key".to_string(),
                });
            }
        }
        "GCP" => {
            if payload_obj.get("serviceAccountJson").is_none() {
                return Err(RedisCtlError::InvalidInput {
                    message:
                        "GCP provider requires --data with service account JSON file (@filename)"
                            .to_string(),
                });
            }
        }
        "Azure" => {
            // Azure has different required fields - keep using JSON for now
            if payload_obj.get("subscriptionId").is_none() {
                return Err(RedisCtlError::InvalidInput {
                    message: "Azure provider requires 'subscriptionId' in --data JSON".to_string(),
                });
            }
        }
        _ => {
            return Err(RedisCtlError::InvalidInput {
                message: format!("Unknown provider: {}. Use AWS, GCP, or Azure", provider),
            });
        }
    }

    let response = params
        .client
        .post_raw("/cloud-accounts", payload)
        .await
        .context("Failed to create cloud account")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        response,
        params.async_ops,
        params.output_format,
        params.query,
        "cloud account creation",
    )
    .await
}

pub async fn handle_update(
    params: &CloudAccountOperationParams<'_>,
    account_id: i32,
    update_params: &UpdateParams<'_>,
) -> CliResult<()> {
    // Start with data from --data if provided, otherwise empty object
    let mut payload = if let Some(data_str) = update_params.data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let payload_obj = payload
        .as_object_mut()
        .ok_or_else(|| RedisCtlError::InvalidInput {
            message: "JSON data must be an object".to_string(),
        })?;

    // Apply first-class params (override JSON values)
    if let Some(n) = update_params.name {
        payload_obj.insert("name".to_string(), json!(n));
    }
    if let Some(aki) = update_params.access_key_id {
        payload_obj.insert("accessKeyId".to_string(), json!(aki));
    }
    if let Some(ask) = update_params.access_secret_key {
        payload_obj.insert("accessSecretKey".to_string(), json!(ask));
    }
    if let Some(cu) = update_params.console_username {
        payload_obj.insert("consoleUsername".to_string(), json!(cu));
    }
    if let Some(cp) = update_params.console_password {
        payload_obj.insert("consolePassword".to_string(), json!(cp));
    }
    if let Some(url) = update_params.sign_in_login_url {
        payload_obj.insert("signInLoginUrl".to_string(), json!(url));
    }

    // Validate that we have at least one field to update
    if payload_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--name, --access-key-id, --access-secret-key, --console-username, --console-password, --sign-in-login-url, or --data)".to_string(),
        });
    }

    let response = params
        .client
        .put_raw(&format!("/cloud-accounts/{}", account_id), payload)
        .await
        .context("Failed to update cloud account")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        response,
        params.async_ops,
        params.output_format,
        params.query,
        "cloud account update",
    )
    .await
}

pub async fn handle_delete(
    params: &CloudAccountOperationParams<'_>,
    account_id: i32,
    force: bool,
) -> CliResult<()> {
    if !force {
        let confirmed = confirm_action(&format!("delete cloud account {}", account_id))?;
        if !confirmed {
            println!("Operation cancelled");
            return Ok(());
        }
    }

    let response = params
        .client
        .delete_raw(&format!("/cloud-accounts/{}", account_id))
        .await
        .context("Failed to delete cloud account")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        response,
        params.async_ops,
        params.output_format,
        params.query,
        "cloud account deletion",
    )
    .await
}
