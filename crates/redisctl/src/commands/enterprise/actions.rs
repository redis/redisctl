use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;
use redis_enterprise::ActionHandler;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

#[derive(Debug, Clone, Subcommand)]
pub enum ActionCommands {
    /// List all actions (tasks) in the cluster
    List {
        /// Filter by action status (running, completed, failed)
        #[arg(long)]
        status: Option<String>,

        /// Filter by action type
        #[arg(long)]
        action_type: Option<String>,

        /// Use v2 API endpoint (default: v1)
        #[arg(long)]
        v2: bool,
    },

    /// Get details of a specific action by UID
    Get {
        /// Action UID
        uid: String,

        /// Use v2 API endpoint (default: v1)
        #[arg(long)]
        v2: bool,
    },

    /// Get the status of a specific action
    Status {
        /// Action UID
        uid: String,

        /// Use v2 API endpoint (default: v1)
        #[arg(long)]
        v2: bool,
    },

    /// List actions for a specific database
    #[command(name = "list-for-bdb")]
    ListForBdb {
        /// Database UID
        bdb_uid: u32,
    },
}

impl ActionCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;
        let handler = ActionHandler::new(client);

        match self {
            ActionCommands::List {
                status,
                action_type,
                v2,
            } => {
                let actions = if *v2 {
                    handler
                        .list_v2()
                        .await
                        .context("Failed to list actions (v2)")?
                } else {
                    handler.list().await.map_err(RedisCtlError::from)?
                };

                // Convert to JSON Value for filtering and output
                let mut response = serde_json::to_value(&actions)?;

                // Apply filters if provided
                if (status.is_some() || action_type.is_some())
                    && let Some(actions) = response.as_array_mut()
                {
                    actions.retain(|action| {
                        let mut keep = true;

                        if let Some(status_filter) = status {
                            if let Some(action_status) =
                                action.get("status").and_then(|s| s.as_str())
                            {
                                keep = keep && action_status.eq_ignore_ascii_case(status_filter);
                            } else {
                                keep = false;
                            }
                        }

                        if let Some(type_filter) = action_type {
                            if let Some(action_type) = action.get("type").and_then(|t| t.as_str()) {
                                keep = keep && action_type.eq_ignore_ascii_case(type_filter);
                            } else {
                                keep = false;
                            }
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

            ActionCommands::Get { uid, v2 } => {
                let action = if *v2 {
                    handler
                        .get_v2(uid)
                        .await
                        .context(format!("Failed to get action {} (v2)", uid))?
                } else {
                    handler
                        .get(uid)
                        .await
                        .context(format!("Failed to get action {}", uid))?
                };

                // Convert to JSON Value
                let response = serde_json::to_value(&action)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            ActionCommands::Status { uid, v2 } => {
                let action = if *v2 {
                    handler
                        .get_v2(uid)
                        .await
                        .context(format!("Failed to get action status {} (v2)", uid))?
                } else {
                    handler
                        .get(uid)
                        .await
                        .context(format!("Failed to get action status {}", uid))?
                };

                // Extract just the status information
                let response = serde_json::to_value(&action)?;
                let status_info = if let Some(obj) = response.as_object() {
                    let mut status = serde_json::Map::new();
                    if let Some(v) = obj.get("uid") {
                        status.insert("uid".to_string(), v.clone());
                    }
                    if let Some(v) = obj.get("status") {
                        status.insert("status".to_string(), v.clone());
                    }
                    if let Some(v) = obj.get("progress") {
                        status.insert("progress".to_string(), v.clone());
                    }
                    if let Some(v) = obj.get("error") {
                        status.insert("error".to_string(), v.clone());
                    }
                    if let Some(v) = obj.get("type") {
                        status.insert("type".to_string(), v.clone());
                    }
                    if let Some(v) = obj.get("description") {
                        status.insert("description".to_string(), v.clone());
                    }
                    serde_json::Value::Object(status)
                } else {
                    response
                };

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&status_info, q)?
                } else {
                    status_info
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            ActionCommands::ListForBdb { bdb_uid } => {
                let actions = handler
                    .list_for_bdb(*bdb_uid)
                    .await
                    .context(format!("Failed to list actions for database {}", bdb_uid))?;

                // Convert to JSON Value
                let response = serde_json::to_value(&actions)?;

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

pub async fn handle_action_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    action_cmd: ActionCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    action_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}
