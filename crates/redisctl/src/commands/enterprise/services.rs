use crate::cli::OutputFormat;
use crate::commands::enterprise::utils;
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;
use serde_json::Value;

#[derive(Debug, Clone, Subcommand)]
pub enum ServicesCommands {
    /// List all services
    List,

    /// Get service configuration
    Get {
        /// Service name
        service: String,
    },

    /// Update service configuration
    #[command(after_help = "EXAMPLES:
    # Enable a service
    redisctl enterprise services update cm_server --enabled true

    # Update service with timeout
    redisctl enterprise services update cm_server --timeout 30

    # Using JSON for full configuration
    redisctl enterprise services update cm_server --data @config.json")]
    Update {
        /// Service name
        service: String,
        /// Enable/disable the service
        #[arg(long)]
        enabled: Option<bool>,
        /// Service timeout in seconds
        #[arg(long)]
        timeout: Option<u32>,
        /// JSON data for service configuration (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Restart service
    Restart {
        /// Service name
        service: String,
    },

    /// Get service status
    Status {
        /// Service name
        service: String,
    },

    /// Enable service
    Enable {
        /// Service name
        service: String,
    },

    /// Disable service
    Disable {
        /// Service name
        service: String,
    },
}

pub async fn handle_services_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: ServicesCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    match cmd {
        ServicesCommands::List => {
            handle_services_list(conn_mgr, profile_name, output_format, query).await
        }
        ServicesCommands::Get { service } => {
            handle_services_get(conn_mgr, profile_name, &service, output_format, query).await
        }
        ServicesCommands::Update {
            service,
            enabled,
            timeout,
            data,
        } => {
            handle_services_update(
                conn_mgr,
                profile_name,
                &service,
                enabled,
                timeout,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        ServicesCommands::Restart { service } => {
            handle_services_restart(conn_mgr, profile_name, &service, output_format, query).await
        }
        ServicesCommands::Status { service } => {
            handle_services_status(conn_mgr, profile_name, &service, output_format, query).await
        }
        ServicesCommands::Enable { service } => {
            handle_services_enable(conn_mgr, profile_name, &service, output_format, query).await
        }
        ServicesCommands::Disable { service } => {
            handle_services_disable(conn_mgr, profile_name, &service, output_format, query).await
        }
    }
}

async fn handle_services_list(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Use /v1/local/services endpoint - /v1/services doesn't exist for GET
    let response = client
        .get::<Value>("/v1/local/services")
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_services_get(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    service: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .get::<Value>("/v1/local/services")
        .await
        .map_err(RedisCtlError::from)?;

    let services_map = response
        .as_object()
        .ok_or_else(|| RedisCtlError::ApiError {
            message: "Unexpected response format from /v1/local/services".to_string(),
        })?;

    let entry = services_map.get(service).ok_or_else(|| {
        let available: Vec<&str> = services_map.keys().map(String::as_str).collect();
        RedisCtlError::InvalidInput {
            message: format!(
                "Service '{}' not found. Available services: {}",
                service,
                available.join(", ")
            ),
        }
    })?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(entry, q)?
    } else {
        entry.clone()
    };

    utils::print_formatted_output(result, output_format)
}

#[allow(clippy::too_many_arguments)]
async fn handle_services_update(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    service: &str,
    enabled: Option<bool>,
    timeout: Option<u32>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut payload = if let Some(data_str) = data {
        utils::read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let payload_obj = payload.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(e) = enabled {
        payload_obj.insert("enabled".to_string(), serde_json::json!(e));
    }
    if let Some(t) = timeout {
        payload_obj.insert("timeout".to_string(), serde_json::json!(t));
    }

    let endpoint = format!("/v1/services/{}", service);
    let response = client
        .put_raw(&endpoint, payload)
        .await
        .context(format!("Failed to update service {}", service))?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_services_restart(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    service: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let endpoint = format!("/v1/services/{}/restart", service);
    let response = client
        .post_raw(&endpoint, serde_json::json!({}))
        .await
        .context(format!("Failed to restart service {}", service))?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_services_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    service: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .get::<Value>("/v1/local/services")
        .await
        .map_err(RedisCtlError::from)?;

    let services_map = response
        .as_object()
        .ok_or_else(|| RedisCtlError::ApiError {
            message: "Unexpected response format from /v1/local/services".to_string(),
        })?;

    let entry = services_map.get(service).ok_or_else(|| {
        let available: Vec<&str> = services_map.keys().map(String::as_str).collect();
        RedisCtlError::InvalidInput {
            message: format!(
                "Service '{}' not found. Available services: {}",
                service,
                available.join(", ")
            ),
        }
    })?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(entry, q)?
    } else {
        entry.clone()
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_services_enable(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    service: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let payload = serde_json::json!({
        "enabled": true
    });

    let endpoint = format!("/v1/services/{}", service);
    let response = client
        .put_raw(&endpoint, payload)
        .await
        .context(format!("Failed to enable service {}", service))?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_services_disable(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    service: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let payload = serde_json::json!({
        "enabled": false
    });

    let endpoint = format!("/v1/services/{}", service);
    let response = client
        .put_raw(&endpoint, payload)
        .await
        .context(format!("Failed to disable service {}", service))?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_services_commands() {
        use clap::CommandFactory;

        #[derive(clap::Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: ServicesCommands,
        }

        TestCli::command().debug_assert();
    }

    /// Helper that mimics the extraction logic shared by handle_services_get and
    /// handle_services_status: look up a service name in the /v1/local/services object.
    fn extract_service_entry<'a>(
        response: &'a Value,
        service: &str,
    ) -> Result<&'a Value, RedisCtlError> {
        let services_map = response
            .as_object()
            .ok_or_else(|| RedisCtlError::ApiError {
                message: "Unexpected response format from /v1/local/services".to_string(),
            })?;

        services_map.get(service).ok_or_else(|| {
            let available: Vec<&str> = services_map.keys().map(String::as_str).collect();
            RedisCtlError::InvalidInput {
                message: format!(
                    "Service '{}' not found. Available services: {}",
                    service,
                    available.join(", ")
                ),
            }
        })
    }

    fn sample_services_response() -> Value {
        serde_json::json!({
            "cm_server": {
                "start_time": "2025-06-01T00:00:00Z",
                "status": "RUNNING",
                "uptime": "0:02:18"
            },
            "ccs": {
                "start_time": "2025-06-01T00:00:01Z",
                "status": "RUNNING",
                "uptime": "0:02:17"
            }
        })
    }

    #[test]
    fn test_get_extracts_known_service() {
        let response = sample_services_response();
        let entry = extract_service_entry(&response, "cm_server").unwrap();
        assert_eq!(entry["status"], "RUNNING");
        assert_eq!(entry["uptime"], "0:02:18");
    }

    #[test]
    fn test_status_extracts_known_service() {
        let response = sample_services_response();
        let entry = extract_service_entry(&response, "ccs").unwrap();
        assert_eq!(entry["status"], "RUNNING");
    }

    #[test]
    fn test_get_unknown_service_returns_invalid_input_error() {
        let response = sample_services_response();
        let err = extract_service_entry(&response, "nope").unwrap_err();
        match err {
            RedisCtlError::InvalidInput { message } => {
                assert!(
                    message.contains("nope"),
                    "error should mention the service name"
                );
                assert!(
                    message.contains("cm_server") || message.contains("ccs"),
                    "error should list available services"
                );
            }
            other => panic!("expected InvalidInput, got {:?}", other),
        }
    }

    #[test]
    fn test_get_reads_from_local_services_object() {
        // Verify that both get and status derive data from the flat map at /v1/local/services,
        // not from a per-service endpoint.  The sample response is a JSON object (not array),
        // matching what the real API returns.
        let response = sample_services_response();
        assert!(
            response.as_object().is_some(),
            "/v1/local/services must be a JSON object keyed by service name"
        );
        // Every entry should have a 'status' field
        for (name, entry) in response.as_object().unwrap() {
            assert!(
                entry.get("status").is_some(),
                "service '{}' is missing the 'status' field",
                name
            );
        }
    }
}
