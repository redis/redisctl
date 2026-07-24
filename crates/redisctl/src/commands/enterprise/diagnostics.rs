use crate::error::RedisCtlError;
use clap::Subcommand;
use redis_enterprise::{DiagnosticsHandler, RestError};

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

#[derive(Debug, Clone, Subcommand)]
pub enum DiagnosticsCommands {
    /// Get diagnostics configuration
    Get,

    /// Update diagnostics configuration
    #[command(after_help = "EXAMPLES:
    # Enable diagnostics
    redisctl enterprise diagnostics update --enabled true

    # Set collection interval
    redisctl enterprise diagnostics update --interval 3600

    # Using JSON for full configuration
    redisctl enterprise diagnostics update --data @config.json")]
    Update {
        /// Enable/disable diagnostics collection
        #[arg(long)]
        enabled: Option<bool>,
        /// Collection interval in seconds
        #[arg(long)]
        interval: Option<u32>,
        /// JSON data for configuration update (optional)
        #[arg(short, long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Run diagnostic checks
    Run {
        /// Specific diagnostic checks to run (comma-separated)
        #[arg(long)]
        checks: Option<String>,

        /// Node UIDs to run diagnostics on (comma-separated)
        #[arg(long)]
        nodes: Option<String>,

        /// Database UIDs to run diagnostics on (comma-separated)
        #[arg(long)]
        databases: Option<String>,
    },

    /// List available diagnostic checks
    #[command(name = "list-checks")]
    ListChecks,

    /// Get the last diagnostic report
    #[command(name = "last-report")]
    LastReport,

    /// Get a specific diagnostic report by ID
    #[command(name = "get-report")]
    GetReport {
        /// Report ID
        report_id: String,
    },

    /// List all diagnostic reports
    #[command(name = "list-reports")]
    ListReports,
}

impl DiagnosticsCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;
        let handler = DiagnosticsHandler::new(client.clone());

        match self {
            DiagnosticsCommands::Get => {
                let config = handler.get_config().await.map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&config, q)?
                } else {
                    config
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            DiagnosticsCommands::Update {
                enabled,
                interval,
                data,
            } => {
                // Start with JSON from --data if provided, otherwise empty object
                let mut json_data = if let Some(data_str) = data {
                    super::utils::read_json_data(data_str)?
                } else {
                    serde_json::json!({})
                };

                let data_obj = json_data.as_object_mut().unwrap();

                // CLI parameters override JSON values
                if let Some(e) = enabled {
                    data_obj.insert("enabled".to_string(), serde_json::json!(e));
                }
                if let Some(i) = interval {
                    data_obj.insert("interval".to_string(), serde_json::json!(i));
                }

                let result = handler
                    .update_config(json_data)
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&result, q)?
                } else {
                    result
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            DiagnosticsCommands::Run {
                checks,
                nodes,
                databases,
            } => {
                // Create the request directly as JSON
                let mut request = serde_json::json!({});

                if let Some(checks_list) = parse_comma_separated(checks) {
                    request["checks"] = serde_json::json!(checks_list);
                }

                if let Some(nodes_list) = parse_comma_separated_u32(nodes) {
                    request["node_uids"] = serde_json::json!(nodes_list);
                }

                if let Some(databases_list) = parse_comma_separated_u32(databases) {
                    request["bdb_uids"] = serde_json::json!(databases_list);
                }

                // Use the raw POST method
                let report: serde_json::Value = client
                    .post("/v1/diagnostics", &request)
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&report, q)?
                } else {
                    report
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            DiagnosticsCommands::ListChecks => {
                let checks = handler
                    .list_checks()
                    .await
                    .map_err(|e| map_endpoint_error(e, "list-checks", "/v1/diagnostics/checks"))?;

                // Convert to JSON Value for output
                let response = serde_json::to_value(&checks)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            DiagnosticsCommands::LastReport => {
                let report = handler
                    .get_last_report()
                    .await
                    .map_err(|e| map_endpoint_error(e, "last-report", "/v1/diagnostics/last"))?;

                // Convert to JSON Value for output
                let response = serde_json::to_value(&report)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            DiagnosticsCommands::GetReport { report_id } => {
                let endpoint = format!("/v1/diagnostics/reports/{}", report_id);
                let report = handler
                    .get_report(report_id)
                    .await
                    .map_err(|e| map_endpoint_error(e, "get-report", &endpoint))?;

                // Convert to JSON Value for output
                let response = serde_json::to_value(&report)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            DiagnosticsCommands::ListReports => {
                let reports = handler.list_reports().await.map_err(|e| {
                    map_endpoint_error(e, "list-reports", "/v1/diagnostics/reports")
                })?;

                // Convert to JSON Value for output
                let response = serde_json::to_value(&reports)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }
        }

        Ok(())
    }
}

pub async fn handle_diagnostics_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    diagnostics_cmd: DiagnosticsCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    diagnostics_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}

/// Map an error from a diagnostics endpoint into a `RedisCtlError`.
///
/// A 404 is turned into a clear, actionable message naming the specific
/// endpoint, since these diagnostics endpoints may not be supported by all
/// Redis Enterprise versions. Any other error is converted normally.
fn map_endpoint_error(err: RestError, command: &str, endpoint: &str) -> RedisCtlError {
    if err.is_not_found() {
        RedisCtlError::ApiError {
            message: format!(
                "The '{command}' endpoint ({endpoint}) is not available on this cluster. \
                 This feature may not be supported by your Redis Enterprise version."
            ),
        }
    } else {
        RedisCtlError::from(err)
    }
}

// Helper functions
fn parse_comma_separated(input: &Option<String>) -> Option<Vec<String>> {
    input.as_ref().map(|s| {
        s.split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    })
}

fn parse_comma_separated_u32(input: &Option<String>) -> Option<Vec<u32>> {
    input.as_ref().and_then(|s| {
        let values: Result<Vec<u32>, _> = s
            .split(',')
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(|item| item.parse::<u32>())
            .collect();
        values.ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_maps_to_actionable_endpoint_message() {
        let err = map_endpoint_error(RestError::NotFound, "list-checks", "/v1/diagnostics/checks");

        match err {
            RedisCtlError::ApiError { message } => {
                assert!(
                    message.contains("/v1/diagnostics/checks"),
                    "message should name the endpoint, got: {message}"
                );
                assert!(
                    message.contains("not available"),
                    "message should explain it is not available, got: {message}"
                );
                assert!(
                    message.contains("list-checks"),
                    "message should name the command, got: {message}"
                );
            }
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    #[test]
    fn api_error_404_also_maps_to_actionable_message() {
        let err = map_endpoint_error(
            RestError::ApiError {
                code: 404,
                message: "not found".to_string(),
            },
            "list-reports",
            "/v1/diagnostics/reports",
        );

        match err {
            RedisCtlError::ApiError { message } => {
                assert!(message.contains("/v1/diagnostics/reports"));
                assert!(message.contains("not available"));
            }
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    #[test]
    fn non_404_error_passes_through() {
        let err = map_endpoint_error(
            RestError::ServerError("boom".to_string()),
            "list-checks",
            "/v1/diagnostics/checks",
        );

        match err {
            RedisCtlError::ApiError { message } => {
                // Should be the generic server-error conversion, not the
                // "not available" endpoint message.
                assert!(
                    !message.contains("not available"),
                    "non-404 errors must not be rewritten, got: {message}"
                );
                assert!(message.contains("boom"));
            }
            other => panic!("expected ApiError from server error, got: {other:?}"),
        }
    }
}
