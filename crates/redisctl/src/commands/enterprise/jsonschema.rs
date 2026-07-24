use crate::error::RedisCtlError;
use clap::Subcommand;

use crate::{cli::OutputFormat, connection::ConnectionManager, error::Result as CliResult};

pub async fn handle_jsonschema_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    jsonschema_cmd: JsonSchemaCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    jsonschema_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}

#[derive(Debug, Clone, Subcommand)]
pub enum JsonSchemaCommands {
    /// Get JSON schema for API validation
    Get,
}

impl JsonSchemaCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        handle_jsonschema_command_impl(conn_mgr, profile_name, self, output_format, query).await
    }
}

async fn handle_jsonschema_command_impl(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &JsonSchemaCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    match command {
        JsonSchemaCommands::Get => {
            let response: serde_json::Value = client
                .get("/v1/jsonschema")
                .await
                .map_err(RedisCtlError::from)?;

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
    fn test_jsonschema_command_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: JsonSchemaCommands,
        }

        // Test get command
        let cli = TestCli::parse_from(["test", "get"]);
        assert!(matches!(cli.cmd, JsonSchemaCommands::Get));
    }
}
