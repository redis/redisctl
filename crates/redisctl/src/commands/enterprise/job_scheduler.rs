use clap::Subcommand;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};

#[derive(Debug, Clone, Subcommand)]
pub enum JobSchedulerCommands {
    /// Get the global job-scheduler configuration
    Get,

    /// Update the global job-scheduler configuration
    Update {
        /// Complete JSON configuration (file, stdin, or inline JSON)
        #[arg(long, value_name = "FILE|JSON")]
        data: String,
    },
}

impl JobSchedulerCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;
        let handler = client.job_scheduler();

        let response = match self {
            JobSchedulerCommands::Get => handler.get_config().await.map_err(RedisCtlError::from)?,
            JobSchedulerCommands::Update { data } => {
                let config = super::utils::read_json_data(data)?;
                if !config.is_object() {
                    return Err(RedisCtlError::InvalidInput {
                        message: "Job-scheduler configuration must be a JSON object".to_string(),
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

pub async fn handle_job_scheduler_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    job_scheduler_cmd: JobSchedulerCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    job_scheduler_cmd
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
        cmd: JobSchedulerCommands,
    }

    #[test]
    fn command_surface_only_exposes_global_configuration() {
        TestCli::command().debug_assert();
        assert!(matches!(
            TestCli::parse_from(["test", "get"]).cmd,
            JobSchedulerCommands::Get
        ));
        assert!(TestCli::try_parse_from(["test", "list"]).is_err());
        assert!(TestCli::try_parse_from(["test", "trigger", "job"]).is_err());
    }
}
