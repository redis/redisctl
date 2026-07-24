//! Raw API access commands for direct REST endpoint calls

use crate::cli::{HttpMethod, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use crate::output::print_output;
use anyhow::Context;
use redisctl_core::{Config, DeploymentType};
use serde_json::Value;

/// Parameters for API command execution
pub struct ApiCommandParams {
    pub config: Config,
    pub config_path: Option<std::path::PathBuf>,
    pub profile_name: Option<String>,
    pub deployment: DeploymentType,
    pub method: HttpMethod,
    pub path: String,
    pub data: Option<String>,
    pub query: Option<String>,
    pub output_format: OutputFormat,
    pub curl: bool,
}

/// Handle raw API commands
pub async fn handle_api_command(params: ApiCommandParams) -> CliResult<()> {
    let connection_manager = ConnectionManager::with_config_path(params.config, params.config_path);

    match params.deployment {
        DeploymentType::Cloud => {
            handle_cloud_api(
                connection_manager,
                params.profile_name.as_deref(),
                params.method,
                params.path,
                params.data,
                params.query,
                params.output_format,
                params.curl,
            )
            .await
        }
        DeploymentType::Enterprise => {
            handle_enterprise_api(
                connection_manager,
                params.profile_name.as_deref(),
                params.method,
                params.path,
                params.data,
                params.query,
                params.output_format,
                params.curl,
            )
            .await
        }
        DeploymentType::Database => Err(anyhow::anyhow!(
            "Raw API access is not supported for database profiles. Database profiles are for direct Redis connections."
        ).into()),
    }
}

/// Handle Cloud API calls
#[allow(dead_code, clippy::too_many_arguments)] // Used by binary target
async fn handle_cloud_api(
    connection_manager: ConnectionManager,
    profile_name: Option<&str>,
    method: HttpMethod,
    path: String,
    data: Option<String>,
    query: Option<String>,
    output_format: OutputFormat,
    curl: bool,
) -> CliResult<()> {
    // Ensure path starts with /
    let normalized_path = if path.starts_with('/') {
        path
    } else {
        format!("/{}", path)
    };

    // Parse request body if provided
    let body: Option<Value> = parse_body(data)?;

    if curl {
        let info = connection_manager.resolve_cloud_connection(profile_name)?;
        let cmd = super::curl::format_cloud_curl(&info, &method, &normalized_path, body.as_ref());
        println!("{}", cmd);
        return Ok(());
    }

    let client = connection_manager.create_cloud_client(profile_name).await?;

    // Execute the API call based on HTTP method
    let result: std::result::Result<Value, _> = match method {
        HttpMethod::Get => client.get_raw(&normalized_path).await,
        HttpMethod::Post => {
            let body = body.unwrap_or(serde_json::json!({}));
            client.post_raw(&normalized_path, body).await
        }
        HttpMethod::Put => {
            let body = body.unwrap_or(serde_json::json!({}));
            client.put_raw(&normalized_path, body).await
        }
        HttpMethod::Patch => {
            let body = body.unwrap_or(serde_json::json!({}));
            client.patch_raw(&normalized_path, body).await
        }
        HttpMethod::Delete => client.delete_raw(&normalized_path).await,
    };

    match result {
        Ok(response) => {
            // Raw API responses aren't structured for tables, resolve Auto to Json
            let format = match output_format {
                OutputFormat::Auto => OutputFormat::Json,
                other => other,
            };

            print_output(response, format, query.as_deref()).map_err(|e| {
                crate::error::RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
            Ok(())
        }
        Err(e) => {
            // Format error nicely
            eprintln!("API Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Handle Enterprise API calls
#[allow(dead_code, clippy::too_many_arguments)] // Used by binary target
async fn handle_enterprise_api(
    connection_manager: ConnectionManager,
    profile_name: Option<&str>,
    method: HttpMethod,
    path: String,
    data: Option<String>,
    query: Option<String>,
    output_format: OutputFormat,
    curl: bool,
) -> CliResult<()> {
    // Normalize path with smart v1 prefixing for Enterprise
    let normalized_path = normalize_enterprise_path(path);

    // Parse request body if provided
    let body: Option<Value> = parse_body(data)?;

    if curl {
        let info = connection_manager.resolve_enterprise_connection(profile_name)?;
        let cmd =
            super::curl::format_enterprise_curl(&info, &method, &normalized_path, body.as_ref());
        println!("{}", cmd);
        return Ok(());
    }

    let client = connection_manager
        .create_enterprise_client(profile_name)
        .await?;

    // Execute the API call based on HTTP method
    let result: std::result::Result<Value, _> = match method {
        HttpMethod::Get => client.get_raw(&normalized_path).await,
        HttpMethod::Post => {
            let body = body.unwrap_or(serde_json::json!({}));
            client.post_raw(&normalized_path, body).await
        }
        HttpMethod::Put => {
            let body = body.unwrap_or(serde_json::json!({}));
            client.put_raw(&normalized_path, body).await
        }
        HttpMethod::Patch => {
            let body = body.unwrap_or(serde_json::json!({}));
            client.patch_raw(&normalized_path, body).await
        }
        HttpMethod::Delete => client.delete_raw(&normalized_path).await,
    };

    match result {
        Ok(response) => {
            // Raw API responses aren't structured for tables, resolve Auto to Json
            let format = match output_format {
                OutputFormat::Auto => OutputFormat::Json,
                other => other,
            };

            print_output(response, format, query.as_deref()).map_err(|e| {
                crate::error::RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
            Ok(())
        }
        Err(e) => {
            // Format error nicely
            eprintln!("API Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Parse request body from a JSON string or @file reference.
fn parse_body(data: Option<String>) -> Result<Option<Value>, crate::error::RedisCtlError> {
    let Some(data_str) = data else {
        return Ok(None);
    };
    if let Some(file_path) = data_str.strip_prefix('@') {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path))?;
        Ok(Some(serde_json::from_str(&content).with_context(|| {
            format!("Failed to parse JSON from file: {}", file_path)
        })?))
    } else {
        Ok(Some(
            serde_json::from_str(&data_str).context("Failed to parse JSON from data parameter")?,
        ))
    }
}

/// Normalize an Enterprise API path with smart v1 prefixing.
fn normalize_enterprise_path(path: String) -> String {
    if path.starts_with('/') {
        if path.starts_with("/v")
            && path
                .chars()
                .nth(2)
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            path
        } else if path == "/" {
            "/v1".to_string()
        } else {
            format!("/v1{}", path)
        }
    } else if path.starts_with("v")
        && path
            .chars()
            .nth(1)
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    {
        format!("/{}", path)
    } else {
        format!("/v1/{}", path)
    }
}
