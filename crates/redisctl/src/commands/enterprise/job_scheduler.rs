use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

#[derive(Debug, Clone, Subcommand)]
pub enum JobSchedulerCommands {
    /// List all scheduled jobs
    List,

    /// Get a specific scheduled job
    Get {
        /// Job ID
        job_id: String,
    },

    /// Create a new scheduled job
    #[command(after_help = "EXAMPLES:
    # Create a backup job running daily at 2am
    redisctl enterprise job-scheduler create --name daily-backup --job-type backup --schedule '0 2 * * *'

    # Create an enabled cleanup job
    redisctl enterprise job-scheduler create --name cleanup --job-type cleanup --schedule '0 0 * * 0' --enabled

    # Create job with parameters
    redisctl enterprise job-scheduler create --name rotation --job-type rotation --schedule '0 3 * * *' \\
        --params '{\"retain_days\": 30}'

    # Using JSON for advanced configuration
    redisctl enterprise job-scheduler create --data @job.json")]
    Create {
        /// Job name (required unless using --data)
        #[arg(long)]
        name: Option<String>,

        /// Job type (backup, cleanup, rotation, etc.)
        #[arg(long)]
        job_type: Option<String>,

        /// Cron-style schedule expression (e.g., '0 2 * * *' for daily at 2am)
        #[arg(long)]
        schedule: Option<String>,

        /// Enable the job immediately
        #[arg(long)]
        enabled: bool,

        /// Job-specific parameters as JSON
        #[arg(long)]
        params: Option<String>,

        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Update a scheduled job
    #[command(after_help = "EXAMPLES:
    # Update job schedule
    redisctl enterprise job-scheduler update my-job --schedule '0 4 * * *'

    # Enable/disable a job
    redisctl enterprise job-scheduler update my-job --enabled false

    # Update job name
    redisctl enterprise job-scheduler update my-job --name new-job-name

    # Update job parameters
    redisctl enterprise job-scheduler update my-job --params '{\"retain_days\": 60}'

    # Using JSON for advanced configuration
    redisctl enterprise job-scheduler update my-job --data @updates.json")]
    Update {
        /// Job ID
        job_id: String,

        /// New job name
        #[arg(long)]
        name: Option<String>,

        /// New job type
        #[arg(long)]
        job_type: Option<String>,

        /// New cron-style schedule expression
        #[arg(long)]
        schedule: Option<String>,

        /// Enable/disable the job
        #[arg(long)]
        enabled: Option<bool>,

        /// Job-specific parameters as JSON
        #[arg(long)]
        params: Option<String>,

        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete a scheduled job
    Delete {
        /// Job ID
        job_id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Trigger immediate execution of a scheduled job
    Trigger {
        /// Job ID
        job_id: String,
    },

    /// Get execution history for a job
    History {
        /// Job ID
        job_id: String,
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

        match self {
            JobSchedulerCommands::List => {
                let response: serde_json::Value = client
                    .get("/v1/job_scheduler")
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            JobSchedulerCommands::Get { job_id } => {
                let response: serde_json::Value = client
                    .get(&format!("/v1/job_scheduler/{}", job_id))
                    .await
                    .context(format!("Failed to get job '{}'", job_id))?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            JobSchedulerCommands::Create {
                name,
                job_type,
                schedule,
                enabled,
                params,
                data,
            } => {
                // Start with JSON data if provided, otherwise empty object
                let mut request_obj: serde_json::Map<String, serde_json::Value> =
                    if let Some(json_data) = data {
                        let parsed = super::utils::read_json_data(json_data)?;
                        parsed
                            .as_object()
                            .cloned()
                            .unwrap_or_else(serde_json::Map::new)
                    } else {
                        serde_json::Map::new()
                    };

                // Override with first-class parameters if provided
                if let Some(n) = name {
                    request_obj.insert("name".to_string(), serde_json::json!(n));
                }
                if let Some(jt) = job_type {
                    request_obj.insert("job_type".to_string(), serde_json::json!(jt));
                }
                if let Some(s) = schedule {
                    request_obj.insert("schedule".to_string(), serde_json::json!(s));
                }
                if *enabled {
                    request_obj.insert("enabled".to_string(), serde_json::json!(true));
                }
                if let Some(p) = params {
                    let params_value: serde_json::Value =
                        serde_json::from_str(p).context("Invalid JSON for --params")?;
                    request_obj.insert("params".to_string(), params_value);
                }

                // Validate required fields for create
                if !request_obj.contains_key("name") {
                    return Err(RedisCtlError::InvalidInput {
                        message: "--name is required when not using --data".to_string(),
                    });
                }
                if !request_obj.contains_key("job_type") {
                    return Err(RedisCtlError::InvalidInput {
                        message: "--job-type is required when not using --data".to_string(),
                    });
                }
                if !request_obj.contains_key("schedule") {
                    return Err(RedisCtlError::InvalidInput {
                        message: "--schedule is required when not using --data".to_string(),
                    });
                }

                let payload = serde_json::Value::Object(request_obj);
                let response: serde_json::Value = client
                    .post("/v1/job_scheduler", &payload)
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            JobSchedulerCommands::Update {
                job_id,
                name,
                job_type,
                schedule,
                enabled,
                params,
                data,
            } => {
                // Start with JSON data if provided, otherwise empty object
                let mut request_obj: serde_json::Map<String, serde_json::Value> =
                    if let Some(json_data) = data {
                        let parsed = super::utils::read_json_data(json_data)?;
                        parsed
                            .as_object()
                            .cloned()
                            .unwrap_or_else(serde_json::Map::new)
                    } else {
                        serde_json::Map::new()
                    };

                // Override with first-class parameters if provided
                if let Some(n) = name {
                    request_obj.insert("name".to_string(), serde_json::json!(n));
                }
                if let Some(jt) = job_type {
                    request_obj.insert("job_type".to_string(), serde_json::json!(jt));
                }
                if let Some(s) = schedule {
                    request_obj.insert("schedule".to_string(), serde_json::json!(s));
                }
                if let Some(e) = enabled {
                    request_obj.insert("enabled".to_string(), serde_json::json!(e));
                }
                if let Some(p) = params {
                    let params_value: serde_json::Value =
                        serde_json::from_str(p).context("Invalid JSON for --params")?;
                    request_obj.insert("params".to_string(), params_value);
                }

                // Validate at least one update field is provided
                if request_obj.is_empty() {
                    return Err(RedisCtlError::InvalidInput {
                        message: "At least one update field is required (--name, --job-type, --schedule, --enabled, --params, or --data)".to_string(),
                    });
                }

                let payload = serde_json::Value::Object(request_obj);
                let response: serde_json::Value = client
                    .put(&format!("/v1/job_scheduler/{}", job_id), &payload)
                    .await
                    .context(format!("Failed to update job '{}'", job_id))?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            JobSchedulerCommands::Delete { job_id, force } => {
                if !force {
                    super::utils::confirm_action(&format!("Delete scheduled job '{}'?", job_id))?;
                }

                client
                    .delete(&format!("/v1/job_scheduler/{}", job_id))
                    .await
                    .context(format!("Failed to delete job '{}'", job_id))?;

                println!("Scheduled job '{}' deleted successfully", job_id);
            }

            JobSchedulerCommands::Trigger { job_id } => {
                let response: serde_json::Value = client
                    .post(
                        &format!("/v1/job_scheduler/{}/trigger", job_id),
                        &serde_json::Value::Null,
                    )
                    .await
                    .context(format!("Failed to trigger job '{}'", job_id))?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            JobSchedulerCommands::History { job_id } => {
                let response: serde_json::Value = client
                    .get(&format!("/v1/job_scheduler/{}/history", job_id))
                    .await
                    .context(format!("Failed to get history for job '{}'", job_id))?;

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

    #[test]
    fn test_job_scheduler_command_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: JobSchedulerCommands,
        }

        // Test list command
        let cli = TestCli::parse_from(["test", "list"]);
        assert!(matches!(cli.cmd, JobSchedulerCommands::List));

        // Test get command
        let cli = TestCli::parse_from(["test", "get", "my-job"]);
        if let JobSchedulerCommands::Get { job_id } = cli.cmd {
            assert_eq!(job_id, "my-job");
        } else {
            panic!("Expected Get command");
        }

        // Test create command with first-class params
        let cli = TestCli::parse_from([
            "test",
            "create",
            "--name",
            "backup-job",
            "--job-type",
            "backup",
            "--schedule",
            "0 2 * * *",
            "--enabled",
        ]);
        if let JobSchedulerCommands::Create {
            name,
            job_type,
            schedule,
            enabled,
            params,
            data,
        } = cli.cmd
        {
            assert_eq!(name, Some("backup-job".to_string()));
            assert_eq!(job_type, Some("backup".to_string()));
            assert_eq!(schedule, Some("0 2 * * *".to_string()));
            assert!(enabled);
            assert!(params.is_none());
            assert!(data.is_none());
        } else {
            panic!("Expected Create command");
        }

        // Test update command
        let cli = TestCli::parse_from([
            "test",
            "update",
            "my-job",
            "--schedule",
            "0 3 * * *",
            "--enabled",
            "false",
        ]);
        if let JobSchedulerCommands::Update {
            job_id,
            schedule,
            enabled,
            ..
        } = cli.cmd
        {
            assert_eq!(job_id, "my-job");
            assert_eq!(schedule, Some("0 3 * * *".to_string()));
            assert_eq!(enabled, Some(false));
        } else {
            panic!("Expected Update command");
        }

        // Test delete command
        let cli = TestCli::parse_from(["test", "delete", "my-job", "--force"]);
        if let JobSchedulerCommands::Delete { job_id, force } = cli.cmd {
            assert_eq!(job_id, "my-job");
            assert!(force);
        } else {
            panic!("Expected Delete command");
        }

        // Test trigger command
        let cli = TestCli::parse_from(["test", "trigger", "my-job"]);
        if let JobSchedulerCommands::Trigger { job_id } = cli.cmd {
            assert_eq!(job_id, "my-job");
        } else {
            panic!("Expected Trigger command");
        }

        // Test history command
        let cli = TestCli::parse_from(["test", "history", "my-job"]);
        if let JobSchedulerCommands::History { job_id } = cli.cmd {
            assert_eq!(job_id, "my-job");
        } else {
            panic!("Expected History command");
        }
    }
}
