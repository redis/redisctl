use crate::error::RedisCtlError;
use clap::Subcommand;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

#[derive(Debug, Clone, Subcommand)]
pub enum CrdbTaskCommands {
    /// List all CRDB tasks
    List {
        /// Filter by task status
        #[arg(long)]
        status: Option<String>,

        /// Filter by task type
        #[arg(long, name = "type")]
        task_type: Option<String>,

        /// Filter by CRDB UID
        #[arg(long)]
        crdb_uid: Option<u32>,
    },

    /// Get CRDB task details
    Get {
        /// Task ID
        task_id: String,
    },

    /// Get task status
    Status {
        /// Task ID
        task_id: String,
    },

    /// Get task progress information
    Progress {
        /// Task ID
        task_id: String,
    },

    /// Get task logs
    Logs {
        /// Task ID
        task_id: String,
    },

    /// List tasks for a specific CRDB
    #[command(name = "list-by-crdb")]
    ListByCrdb {
        /// CRDB UID
        crdb_uid: u32,

        /// Filter by task status
        #[arg(long)]
        status: Option<String>,

        /// Filter by task type
        #[arg(long, name = "type")]
        task_type: Option<String>,
    },

    /// Cancel a running task
    Cancel {
        /// Task ID
        task_id: String,

        /// Force cancellation without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Retry a failed task
    Retry {
        /// Task ID
        task_id: String,
    },

    /// Pause a running task
    Pause {
        /// Task ID
        task_id: String,
    },

    /// Resume a paused task
    Resume {
        /// Task ID
        task_id: String,
    },
}

impl CrdbTaskCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;

        match self {
            CrdbTaskCommands::List {
                status,
                task_type,
                crdb_uid,
            } => {
                // Get all CRDB tasks
                let mut response: serde_json::Value = client
                    .get("/crdb_tasks")
                    .await
                    .map_err(RedisCtlError::from)?;

                // Apply filters if provided
                if (status.is_some() || task_type.is_some() || crdb_uid.is_some())
                    && let Some(tasks) = response["tasks"].as_array_mut()
                {
                    tasks.retain(|task| {
                        let mut keep = true;

                        if let Some(s) = status {
                            keep = keep && task["status"].as_str() == Some(s);
                        }

                        if let Some(t) = task_type {
                            keep = keep && task["type"].as_str() == Some(t);
                        }

                        if let Some(uid) = crdb_uid {
                            keep = keep
                                && task["crdb_guid"]
                                    .as_str()
                                    .and_then(|guid| guid.split('-').nth(0))
                                    .and_then(|id| id.parse::<u32>().ok())
                                    == Some(*uid);
                        }

                        keep
                    });
                }

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            CrdbTaskCommands::Get { task_id } => {
                let response: serde_json::Value = client
                    .get(&format!("/crdb_tasks/{}", task_id))
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            CrdbTaskCommands::Status { task_id } => {
                let response: serde_json::Value = client
                    .get(&format!("/crdb_tasks/{}", task_id))
                    .await
                    .map_err(RedisCtlError::from)?;

                // Extract just the status field
                let status = &response["status"];

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(status, q)?
                } else {
                    status.clone()
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            CrdbTaskCommands::Progress { task_id } => {
                let response: serde_json::Value = client
                    .get(&format!("/crdb_tasks/{}", task_id))
                    .await
                    .map_err(RedisCtlError::from)?;

                // Extract progress information
                let progress = serde_json::json!({
                    "status": response["status"],
                    "progress": response["progress"],
                    "progress_percent": response["progress_percent"],
                    "start_time": response["start_time"],
                    "end_time": response["end_time"],
                });

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&progress, q)?
                } else {
                    progress
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            CrdbTaskCommands::Logs { task_id } => {
                let response: serde_json::Value = client
                    .get(&format!("/crdb_tasks/{}", task_id))
                    .await
                    .map_err(RedisCtlError::from)?;

                // Extract logs if available
                let logs = if response["logs"].is_null() {
                    serde_json::json!({
                        "task_id": task_id,
                        "logs": "No logs available",
                        "status": response["status"]
                    })
                } else {
                    response["logs"].clone()
                };

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&logs, q)?
                } else {
                    logs
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            CrdbTaskCommands::ListByCrdb {
                crdb_uid,
                status,
                task_type,
            } => {
                // Get all CRDB tasks
                let mut response: serde_json::Value = client
                    .get("/crdb_tasks")
                    .await
                    .map_err(RedisCtlError::from)?;

                // Filter by CRDB UID and optional status/type
                if let Some(tasks) = response["tasks"].as_array_mut() {
                    tasks.retain(|task| {
                        let mut keep = task["crdb_guid"]
                            .as_str()
                            .and_then(|guid| guid.split('-').nth(0))
                            .and_then(|id| id.parse::<u32>().ok())
                            == Some(*crdb_uid);

                        if let Some(s) = status {
                            keep = keep && task["status"].as_str() == Some(s);
                        }

                        if let Some(t) = task_type {
                            keep = keep && task["type"].as_str() == Some(t);
                        }

                        keep
                    });
                }

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            CrdbTaskCommands::Cancel { task_id, force } => {
                if !force {
                    super::utils::confirm_action(&format!("Cancel task {}?", task_id))?;
                }

                // Cancel the task
                let _: serde_json::Value = client
                    .post(
                        &format!("/crdb_tasks/{}/actions/cancel", task_id),
                        &serde_json::json!({}),
                    )
                    .await
                    .map_err(RedisCtlError::from)?;

                println!("Task {} cancelled successfully", task_id);
            }

            CrdbTaskCommands::Retry { task_id } => {
                // Note: Retry endpoint may not exist in all versions
                // Try to retry the task
                let result: Result<serde_json::Value, _> = client
                    .post(
                        &format!("/crdb_tasks/{}/actions/retry", task_id),
                        &serde_json::json!({}),
                    )
                    .await;

                match result {
                    Ok(_) => println!("Task {} retry initiated", task_id),
                    Err(_) => {
                        // If retry endpoint doesn't exist, provide alternative
                        println!("Retry operation not available for task {}", task_id);
                        println!("Consider creating a new task with the same configuration");
                    }
                }
            }

            CrdbTaskCommands::Pause { task_id } => {
                // Note: Pause endpoint may not exist in all versions
                let result: Result<serde_json::Value, _> = client
                    .post(
                        &format!("/crdb_tasks/{}/actions/pause", task_id),
                        &serde_json::json!({}),
                    )
                    .await;

                match result {
                    Ok(_) => println!("Task {} paused", task_id),
                    Err(_) => {
                        println!("Pause operation not available for task {}", task_id);
                        println!("Task pause may not be supported for this task type");
                    }
                }
            }

            CrdbTaskCommands::Resume { task_id } => {
                // Note: Resume endpoint may not exist in all versions
                let result: Result<serde_json::Value, _> = client
                    .post(
                        &format!("/crdb_tasks/{}/actions/resume", task_id),
                        &serde_json::json!({}),
                    )
                    .await;

                match result {
                    Ok(_) => println!("Task {} resumed", task_id),
                    Err(_) => {
                        println!("Resume operation not available for task {}", task_id);
                        println!("Task resume may not be supported for this task type");
                    }
                }
            }
        }

        Ok(())
    }
}

pub async fn handle_crdb_task_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    crdb_task_cmd: CrdbTaskCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    crdb_task_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}
