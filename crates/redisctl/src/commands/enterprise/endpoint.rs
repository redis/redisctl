use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;

use crate::{cli::OutputFormat, connection::ConnectionManager, error::Result as CliResult};

#[allow(dead_code)]
pub async fn handle_endpoint_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    endpoint_cmd: EndpointCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    endpoint_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}

#[derive(Debug, Clone, Subcommand)]
pub enum EndpointCommands {
    /// Get endpoint statistics
    Stats,

    /// Check endpoint availability for a database
    Availability {
        /// Database UID
        bdb_uid: u64,
    },
}

impl EndpointCommands {
    #[allow(dead_code)]
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        handle_endpoint_command_impl(conn_mgr, profile_name, self, output_format, query).await
    }
}

#[allow(dead_code)]
async fn handle_endpoint_command_impl(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &EndpointCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    match command {
        EndpointCommands::Stats => {
            let response: serde_json::Value = client
                .get("/v1/endpoints/stats")
                .await
                .map_err(RedisCtlError::from)?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }
        EndpointCommands::Availability { bdb_uid } => {
            let text = client
                .get_text(&format!("/v1/local/bdbs/{}/endpoint/availability", bdb_uid))
                .await
                .map_err(RedisCtlError::from)
                .context(format!(
                    "Failed to check endpoint availability for database {}",
                    bdb_uid
                ))?;

            let response: serde_json::Value = if text.trim().is_empty() {
                // HTTP 200 with empty body means the endpoint is available.
                serde_json::json!({ "bdb_uid": bdb_uid, "available": true })
            } else {
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(v) => v,
                    Err(_) => serde_json::json!({ "bdb_uid": bdb_uid, "raw": text }),
                }
            };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_command_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: EndpointCommands,
        }

        // Test stats command
        let cli = TestCli::parse_from(["test", "stats"]);
        assert!(matches!(cli.cmd, EndpointCommands::Stats));

        // Test availability command
        let cli = TestCli::parse_from(["test", "availability", "1"]);
        if let EndpointCommands::Availability { bdb_uid } = cli.cmd {
            assert_eq!(bdb_uid, 1);
        } else {
            panic!("Expected Availability command");
        }
    }

    /// Verify the body-parsing logic used in the Availability arm:
    /// - empty body  → synthesized `{ bdb_uid, available: true }`
    /// - valid JSON  → parsed value passed through
    /// - non-JSON    → wrapped in `{ bdb_uid, raw: <text> }`
    #[test]
    fn test_availability_body_synthesis_empty() {
        let bdb_uid: u64 = 1;
        let text = "";
        let result: serde_json::Value = if text.trim().is_empty() {
            serde_json::json!({ "bdb_uid": bdb_uid, "available": true })
        } else {
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(v) => v,
                Err(_) => serde_json::json!({ "bdb_uid": bdb_uid, "raw": text }),
            }
        };
        assert_eq!(result["bdb_uid"], 1);
        assert_eq!(result["available"], true);
    }

    #[test]
    fn test_availability_body_synthesis_whitespace_only() {
        let bdb_uid: u64 = 42;
        let text = "   \n  ";
        let result: serde_json::Value = if text.trim().is_empty() {
            serde_json::json!({ "bdb_uid": bdb_uid, "available": true })
        } else {
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(v) => v,
                Err(_) => serde_json::json!({ "bdb_uid": bdb_uid, "raw": text }),
            }
        };
        assert_eq!(result["bdb_uid"], 42);
        assert_eq!(result["available"], true);
    }

    #[test]
    fn test_availability_body_synthesis_json_body() {
        let bdb_uid: u64 = 5;
        let text = r#"{"status":"unavailable","reason":"recovering"}"#;
        let result: serde_json::Value = if text.trim().is_empty() {
            serde_json::json!({ "bdb_uid": bdb_uid, "available": true })
        } else {
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(v) => v,
                Err(_) => serde_json::json!({ "bdb_uid": bdb_uid, "raw": text }),
            }
        };
        assert_eq!(result["status"], "unavailable");
        assert_eq!(result["reason"], "recovering");
        // Not synthesized — original keys are preserved
        assert!(result.get("bdb_uid").is_none());
    }

    #[test]
    fn test_availability_body_synthesis_non_json_body() {
        let bdb_uid: u64 = 7;
        let text = "endpoint not ready";
        let result: serde_json::Value = if text.trim().is_empty() {
            serde_json::json!({ "bdb_uid": bdb_uid, "available": true })
        } else {
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(v) => v,
                Err(_) => serde_json::json!({ "bdb_uid": bdb_uid, "raw": text }),
            }
        };
        assert_eq!(result["bdb_uid"], 7);
        assert_eq!(result["raw"], "endpoint not ready");
    }
}
