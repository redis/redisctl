use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;

use crate::{cli::OutputFormat, connection::ConnectionManager, error::Result as CliResult};

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
        bdb_uid: u32,
    },
}

impl EndpointCommands {
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
            client
                .databases()
                .endpoint_availability(*bdb_uid)
                .await
                .map_err(RedisCtlError::from)
                .context(format!(
                    "Failed to check endpoint availability for database {}",
                    bdb_uid
                ))?;

            let response = serde_json::json!({ "bdb_uid": bdb_uid, "available": true });

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
}
