use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use clap::Subcommand;

#[derive(Debug, Clone, Subcommand)]
pub enum LocalCommands {
    /// Get local node master healthcheck
    #[command(name = "healthcheck")]
    MasterHealthcheck,

    /// List local services
    Services,

    /// Update local services configuration
    #[command(
        name = "services-update",
        after_help = "EXAMPLES:
    # Update service action
    redisctl enterprise local services-update --action start --service cm_server

    # Using JSON for full configuration
    redisctl enterprise local services-update --data @services.json"
    )]
    ServicesUpdate {
        /// Service action (start, stop, restart)
        #[arg(long)]
        action: Option<String>,
        /// Service name
        #[arg(long)]
        service: Option<String>,
        /// Service configuration as JSON string or @file.json (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },
}

impl LocalCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;

        match self {
            LocalCommands::MasterHealthcheck => {
                let response: serde_json::Value = client
                    .get("/v1/local/node/master_healthcheck")
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            LocalCommands::Services => {
                let response: serde_json::Value = client
                    .get("/v1/local/services")
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            LocalCommands::ServicesUpdate {
                action,
                service,
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
                if let Some(a) = action {
                    data_obj.insert("action".to_string(), serde_json::json!(a));
                }
                if let Some(s) = service {
                    data_obj.insert("service".to_string(), serde_json::json!(s));
                }

                let response: serde_json::Value = client
                    .post("/v1/local/services", &json_data)
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
}

pub async fn handle_local_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    local_cmd: LocalCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    local_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}
