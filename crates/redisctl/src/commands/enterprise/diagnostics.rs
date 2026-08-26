use clap::Subcommand;
use redis_enterprise::DiagnosticsHandler;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};

#[derive(Debug, Clone, Subcommand)]
pub enum DiagnosticsCommands {
    /// Get the global diagnostics configuration
    Get,

    /// Update the global diagnostics configuration
    #[command(after_help = "EXAMPLES:
    # Update one diagnostics collection target
    redisctl enterprise diagnostics update --data '{\"bdb_target\":{\"cron_expression\":\"*/15 * * * *\"}}'

    # Use JSON for the complete configuration
    redisctl enterprise diagnostics update --data @config.json")]
    Update {
        /// Complete JSON configuration (file, stdin, or inline JSON)
        #[arg(long, value_name = "FILE|JSON")]
        data: String,
    },
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
        let handler = DiagnosticsHandler::new(client);

        let response = match self {
            DiagnosticsCommands::Get => handler.get_config().await.map_err(RedisCtlError::from)?,
            DiagnosticsCommands::Update { data } => {
                let config = super::utils::read_json_data(data)?;
                if !config.is_object() {
                    return Err(RedisCtlError::InvalidInput {
                        message: "Diagnostics configuration must be a JSON object".to_string(),
                    });
                }

                handler
                    .update_config(config)
                    .await
                    .map_err(RedisCtlError::from)?
            }
        };

        let output = if let Some(query) = query {
            super::utils::apply_jmespath(&response, query)?
        } else {
            response
        };
        super::utils::print_formatted_output(output, output_format)
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: DiagnosticsCommands,
    }

    #[test]
    fn command_surface_only_exposes_global_configuration() {
        TestCli::command().debug_assert();
        assert!(matches!(
            TestCli::parse_from(["test", "get"]).cmd,
            DiagnosticsCommands::Get
        ));
        assert!(TestCli::try_parse_from(["test", "run"]).is_err());
        assert!(TestCli::try_parse_from(["test", "list-reports"]).is_err());
    }
}
