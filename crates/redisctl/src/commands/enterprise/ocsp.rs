use crate::cli::OutputFormat;
use crate::commands::enterprise::utils;
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;
use serde_json::Value;

#[derive(Debug, Clone, Subcommand)]
pub enum OcspCommands {
    /// Get OCSP configuration
    Get,

    /// Update OCSP configuration
    #[command(after_help = "EXAMPLES:
    # Enable OCSP with responder URL
    redisctl enterprise ocsp update --enabled true --responder-url https://ocsp.example.com

    # Set response timeout
    redisctl enterprise ocsp update --response-timeout 5000

    # Using JSON for full configuration
    redisctl enterprise ocsp update --data @ocsp.json")]
    Update {
        /// Enable/disable OCSP validation
        #[arg(long)]
        enabled: Option<bool>,
        /// OCSP responder URL
        #[arg(long)]
        responder_url: Option<String>,
        /// Response timeout in milliseconds
        #[arg(long)]
        response_timeout: Option<u32>,
        /// Query frequency in seconds
        #[arg(long)]
        query_frequency: Option<u32>,
        /// JSON data for OCSP configuration (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Get OCSP status
    Status,

    /// Test OCSP validation
    Test {
        /// Optional test configuration JSON
        #[arg(long)]
        data: Option<String>,
    },

    /// Enable OCSP validation
    Enable,

    /// Disable OCSP validation
    Disable,
}

pub async fn handle_ocsp_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: OcspCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    match cmd {
        OcspCommands::Get => handle_ocsp_get(conn_mgr, profile_name, output_format, query).await,
        OcspCommands::Update {
            enabled,
            responder_url,
            response_timeout,
            query_frequency,
            data,
        } => {
            handle_ocsp_update(
                conn_mgr,
                profile_name,
                enabled,
                responder_url.as_deref(),
                response_timeout,
                query_frequency,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        OcspCommands::Status => {
            handle_ocsp_status(conn_mgr, profile_name, output_format, query).await
        }
        OcspCommands::Test { data } => {
            handle_ocsp_test(
                conn_mgr,
                profile_name,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        OcspCommands::Enable => {
            handle_ocsp_enable(conn_mgr, profile_name, output_format, query).await
        }
        OcspCommands::Disable => {
            handle_ocsp_disable(conn_mgr, profile_name, output_format, query).await
        }
    }
}

async fn handle_ocsp_get(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .get::<Value>("/v1/ocsp")
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

#[allow(clippy::too_many_arguments)]
async fn handle_ocsp_update(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    enabled: Option<bool>,
    responder_url: Option<&str>,
    response_timeout: Option<u32>,
    query_frequency: Option<u32>,
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
    if let Some(url) = responder_url {
        payload_obj.insert("responder_url".to_string(), serde_json::json!(url));
    }
    if let Some(rt) = response_timeout {
        payload_obj.insert("response_timeout".to_string(), serde_json::json!(rt));
    }
    if let Some(qf) = query_frequency {
        payload_obj.insert("query_frequency".to_string(), serde_json::json!(qf));
    }

    let response = client
        .put_raw("/v1/ocsp", payload)
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_ocsp_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .get::<Value>("/v1/ocsp/status")
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_ocsp_test(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let payload = if let Some(d) = data {
        serde_json::from_str(d).context("Invalid JSON data for OCSP test")?
    } else {
        serde_json::json!({})
    };

    let response = client
        .post_raw("/v1/ocsp/test", payload)
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_ocsp_enable(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let payload = serde_json::json!({
        "enabled": true
    });

    let response = client
        .put_raw("/v1/ocsp", payload)
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_ocsp_disable(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let payload = serde_json::json!({
        "enabled": false
    });

    let response = client
        .put_raw("/v1/ocsp", payload)
        .await
        .map_err(RedisCtlError::from)?;

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
    fn test_ocsp_commands() {
        use clap::CommandFactory;

        #[derive(clap::Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: OcspCommands,
        }

        TestCli::command().debug_assert();
    }
}
