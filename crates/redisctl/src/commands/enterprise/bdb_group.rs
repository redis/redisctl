use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

#[derive(Debug, Clone, Subcommand)]
pub enum BdbGroupCommands {
    /// List all database groups
    List,

    /// Get database group details
    Get {
        /// Database group UID
        uid: u32,
    },

    /// Create a new database group
    #[command(after_help = "EXAMPLES:
    # Create a database group with name
    redisctl enterprise bdb-group create --name my-group

    # Create a database group with memory size limit
    redisctl enterprise bdb-group create --name pool-group --memory-size 10737418240

    # Using JSON for advanced configuration
    redisctl enterprise bdb-group create --data @group.json")]
    Create {
        /// Group name (required unless using --data)
        #[arg(long)]
        name: Option<String>,

        /// Memory pool size limit in bytes for all databases in the group
        #[arg(long)]
        memory_size: Option<u64>,

        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Update a database group
    #[command(after_help = "EXAMPLES:
    # Update group name
    redisctl enterprise bdb-group update 1 --name new-group-name

    # Update memory size limit
    redisctl enterprise bdb-group update 1 --memory-size 21474836480

    # Using JSON for advanced configuration
    redisctl enterprise bdb-group update 1 --data @updates.json")]
    Update {
        /// Database group UID
        uid: u32,

        /// New group name
        #[arg(long)]
        name: Option<String>,

        /// New memory pool size limit in bytes
        #[arg(long)]
        memory_size: Option<u64>,

        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete a database group
    Delete {
        /// Database group UID
        uid: u32,
        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Add database to group
    AddDatabase {
        /// Database group UID
        group_uid: u32,
        /// Database UID to add
        #[arg(long)]
        database: u32,
    },

    /// Remove database from group
    RemoveDatabase {
        /// Database group UID
        group_uid: u32,
        /// Database UID to remove
        #[arg(long)]
        database: u32,
    },

    /// List databases in group
    ListDatabases {
        /// Database group UID
        group_uid: u32,
    },
}

impl BdbGroupCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;

        match self {
            BdbGroupCommands::List => {
                let response: serde_json::Value = client
                    .get("/v1/bdb_groups")
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            BdbGroupCommands::Get { uid } => {
                let response: serde_json::Value = client
                    .get(&format!("/v1/bdb_groups/{}", uid))
                    .await
                    .context(format!("Failed to get database group {}", uid))?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            BdbGroupCommands::Create {
                name,
                memory_size,
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
                if let Some(ms) = memory_size {
                    request_obj.insert("memory_size".to_string(), serde_json::json!(ms));
                }

                // Validate required fields for create
                if !request_obj.contains_key("name") {
                    return Err(RedisCtlError::InvalidInput {
                        message: "--name is required when not using --data".to_string(),
                    });
                }

                let payload = serde_json::Value::Object(request_obj);
                let response: serde_json::Value = client
                    .post("/v1/bdb_groups", &payload)
                    .await
                    .map_err(RedisCtlError::from)?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            BdbGroupCommands::Update {
                uid,
                name,
                memory_size,
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
                if let Some(ms) = memory_size {
                    request_obj.insert("memory_size".to_string(), serde_json::json!(ms));
                }

                // Validate at least one update field is provided
                if request_obj.is_empty() {
                    return Err(RedisCtlError::InvalidInput {
                        message:
                            "At least one update field is required (--name, --memory-size, or --data)"
                                .to_string(),
                    });
                }

                let payload = serde_json::Value::Object(request_obj);
                let response: serde_json::Value = client
                    .put(&format!("/v1/bdb_groups/{}", uid), &payload)
                    .await
                    .context(format!("Failed to update database group {}", uid))?;

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            BdbGroupCommands::Delete { uid, force } => {
                if !force {
                    super::utils::confirm_action(&format!("Delete database group {}?", uid))?;
                }

                client
                    .delete(&format!("/v1/bdb_groups/{}", uid))
                    .await
                    .context(format!("Failed to delete database group {}", uid))?;

                println!("Database group {} deleted successfully", uid);
            }

            BdbGroupCommands::AddDatabase {
                group_uid,
                database,
            } => {
                // Get current group data
                let mut group_data: serde_json::Value = client
                    .get(&format!("/v1/bdb_groups/{}", group_uid))
                    .await
                    .context(format!("Failed to get database group {}", group_uid))?;

                // Add database to the group
                if let Some(bdbs) = group_data["bdbs"].as_array_mut() {
                    if !bdbs.contains(&serde_json::json!(database)) {
                        bdbs.push(serde_json::json!(database));
                    } else {
                        println!("Database {} is already in group {}", database, group_uid);
                        return Ok(());
                    }
                } else {
                    group_data["bdbs"] = serde_json::json!([database]);
                }

                // Update the group
                let response: serde_json::Value = client
                    .put(&format!("/v1/bdb_groups/{}", group_uid), &group_data)
                    .await
                    .context(format!(
                        "Failed to add database {} to group {}",
                        database, group_uid
                    ))?;

                println!("Database {} added to group {}", database, group_uid);

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            BdbGroupCommands::RemoveDatabase {
                group_uid,
                database,
            } => {
                // Get current group data
                let mut group_data: serde_json::Value = client
                    .get(&format!("/v1/bdb_groups/{}", group_uid))
                    .await
                    .context(format!("Failed to get database group {}", group_uid))?;

                // Remove database from the group
                if let Some(bdbs) = group_data["bdbs"].as_array_mut() {
                    if let Some(pos) = bdbs.iter().position(|x| x == &serde_json::json!(database)) {
                        bdbs.remove(pos);
                    } else {
                        println!("Database {} is not in group {}", database, group_uid);
                        return Ok(());
                    }
                } else {
                    println!("Group {} has no databases", group_uid);
                    return Ok(());
                }

                // Update the group
                let response: serde_json::Value = client
                    .put(&format!("/v1/bdb_groups/{}", group_uid), &group_data)
                    .await
                    .context(format!(
                        "Failed to remove database {} from group {}",
                        database, group_uid
                    ))?;

                println!("Database {} removed from group {}", database, group_uid);

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&response, q)?
                } else {
                    response
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }

            BdbGroupCommands::ListDatabases { group_uid } => {
                let response: serde_json::Value = client
                    .get(&format!("/v1/bdb_groups/{}", group_uid))
                    .await
                    .context(format!("Failed to get database group {}", group_uid))?;

                // Extract just the databases list
                let databases = &response["bdbs"];

                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(databases, q)?
                } else {
                    databases.clone()
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            }
        }

        Ok(())
    }
}

pub async fn handle_bdb_group_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    bdb_group_cmd: BdbGroupCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    bdb_group_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bdb_group_command_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BdbGroupCommands,
        }

        // Test list command
        let cli = TestCli::parse_from(["test", "list"]);
        assert!(matches!(cli.cmd, BdbGroupCommands::List));

        // Test get command
        let cli = TestCli::parse_from(["test", "get", "1"]);
        if let BdbGroupCommands::Get { uid } = cli.cmd {
            assert_eq!(uid, 1);
        } else {
            panic!("Expected Get command");
        }

        // Test create command with first-class params
        let cli = TestCli::parse_from([
            "test",
            "create",
            "--name",
            "my-group",
            "--memory-size",
            "10737418240",
        ]);
        if let BdbGroupCommands::Create {
            name,
            memory_size,
            data,
        } = cli.cmd
        {
            assert_eq!(name, Some("my-group".to_string()));
            assert_eq!(memory_size, Some(10737418240));
            assert!(data.is_none());
        } else {
            panic!("Expected Create command");
        }

        // Test update command
        let cli = TestCli::parse_from(["test", "update", "1", "--name", "new-name"]);
        if let BdbGroupCommands::Update { uid, name, .. } = cli.cmd {
            assert_eq!(uid, 1);
            assert_eq!(name, Some("new-name".to_string()));
        } else {
            panic!("Expected Update command");
        }

        // Test delete command
        let cli = TestCli::parse_from(["test", "delete", "1", "--force"]);
        if let BdbGroupCommands::Delete { uid, force } = cli.cmd {
            assert_eq!(uid, 1);
            assert!(force);
        } else {
            panic!("Expected Delete command");
        }

        // Test add-database command
        let cli = TestCli::parse_from(["test", "add-database", "1", "--database", "2"]);
        if let BdbGroupCommands::AddDatabase {
            group_uid,
            database,
        } = cli.cmd
        {
            assert_eq!(group_uid, 1);
            assert_eq!(database, 2);
        } else {
            panic!("Expected AddDatabase command");
        }

        // Test remove-database command
        let cli = TestCli::parse_from(["test", "remove-database", "1", "--database", "2"]);
        if let BdbGroupCommands::RemoveDatabase {
            group_uid,
            database,
        } = cli.cmd
        {
            assert_eq!(group_uid, 1);
            assert_eq!(database, 2);
        } else {
            panic!("Expected RemoveDatabase command");
        }

        // Test list-databases command
        let cli = TestCli::parse_from(["test", "list-databases", "1"]);
        if let BdbGroupCommands::ListDatabases { group_uid } = cli.cmd {
            assert_eq!(group_uid, 1);
        } else {
            panic!("Expected ListDatabases command");
        }
    }
}
