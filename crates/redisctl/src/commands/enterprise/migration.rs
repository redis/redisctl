use anyhow::Context;
use clap::Subcommand;

use crate::error::RedisCtlError;
use crate::{cli::OutputFormat, connection::ConnectionManager, error::Result as CliResult};

pub async fn handle_migration_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    migration_cmd: MigrationCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    migration_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}

#[derive(Debug, Clone, Subcommand)]
pub enum MigrationCommands {
    /// List all migrations
    List,

    /// Get migration status
    Get {
        /// Migration UID
        uid: String,
    },

    /// Create a new migration
    #[command(after_help = "EXAMPLES:
    # Migrate from external Redis to internal database
    redisctl enterprise migration create --source-host redis.external.com --source-port 6379 \\
        --target-bdb 1

    # Migrate with authentication
    redisctl enterprise migration create --source-host redis.external.com --source-port 6379 \\
        --source-password secret123 --target-bdb 1

    # Migrate with SSL
    redisctl enterprise migration create --source-host redis.external.com --source-port 6379 \\
        --source-ssl --target-bdb 1

    # Migrate specific key pattern
    redisctl enterprise migration create --source-host redis.external.com --source-port 6379 \\
        --target-bdb 1 --key-pattern 'user:*'

    # Flush target before migration
    redisctl enterprise migration create --source-host redis.external.com --source-port 6379 \\
        --target-bdb 1 --flush-target

    # Using JSON for advanced configuration
    redisctl enterprise migration create --data @migration.json")]
    Create {
        /// Source hostname or IP address
        #[arg(long)]
        source_host: Option<String>,

        /// Source port number
        #[arg(long)]
        source_port: Option<u16>,

        /// Source endpoint type (redis, cluster, azure-cache)
        #[arg(long, default_value = "redis")]
        source_type: String,

        /// Source authentication password
        #[arg(long)]
        source_password: Option<String>,

        /// Use SSL for source connection
        #[arg(long)]
        source_ssl: bool,

        /// Target database UID
        #[arg(long)]
        target_bdb: Option<u32>,

        /// Target hostname (for external targets)
        #[arg(long)]
        target_host: Option<String>,

        /// Target port (for external targets)
        #[arg(long)]
        target_port: Option<u16>,

        /// Target authentication password
        #[arg(long)]
        target_password: Option<String>,

        /// Use SSL for target connection
        #[arg(long)]
        target_ssl: bool,

        /// Migration type (full, incremental)
        #[arg(long)]
        migration_type: Option<String>,

        /// Redis key pattern to migrate (supports wildcards like 'user:*')
        #[arg(long)]
        key_pattern: Option<String>,

        /// Flush target database before migration
        #[arg(long)]
        flush_target: bool,

        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Start a migration
    Start {
        /// Migration UID
        uid: String,
    },

    /// Pause a migration
    Pause {
        /// Migration UID
        uid: String,
    },

    /// Resume a migration
    Resume {
        /// Migration UID
        uid: String,
    },

    /// Cancel a migration
    Cancel {
        /// Migration UID
        uid: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Export database data (legacy command)
    Export {
        /// Database UID
        bdb_uid: u64,
    },

    /// Import database data (legacy command)
    #[command(after_help = "EXAMPLES:
    # Import with source URI
    redisctl enterprise migration import 1 --source-uri redis://external-redis:6379

    # Import with authentication
    redisctl enterprise migration import 1 --source-uri redis://external-redis:6379 --source-password secret

    # Import specific keys
    redisctl enterprise migration import 1 --source-uri redis://external-redis:6379 --key-pattern 'app:*'

    # Using JSON for advanced configuration
    redisctl enterprise migration import 1 --data @import.json")]
    Import {
        /// Database UID
        bdb_uid: u64,

        /// Source Redis URI (e.g., redis://host:port)
        #[arg(long)]
        source_uri: Option<String>,

        /// Source authentication password
        #[arg(long)]
        source_password: Option<String>,

        /// Redis key pattern to import (supports wildcards)
        #[arg(long)]
        key_pattern: Option<String>,

        /// Flush database before import
        #[arg(long)]
        flush_before: bool,

        /// Import data (use @filename or - for stdin) for advanced configuration
        #[arg(long)]
        data: Option<String>,
    },
}

impl MigrationCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        handle_migration_command_impl(conn_mgr, profile_name, self, output_format, query).await
    }
}

async fn handle_migration_command_impl(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &MigrationCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    match command {
        MigrationCommands::List => {
            let response: serde_json::Value = client
                .get("/v1/migrations")
                .await
                .context("Failed to list migrations")?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        MigrationCommands::Get { uid } => {
            let response: serde_json::Value = client
                .get(&format!("/v1/migrations/{}", uid))
                .await
                .context(format!("Failed to get migration {}", uid))?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        MigrationCommands::Create {
            source_host,
            source_port,
            source_type,
            source_password,
            source_ssl,
            target_bdb,
            target_host,
            target_port,
            target_password,
            target_ssl,
            migration_type,
            key_pattern,
            flush_target,
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

            // Build source endpoint if first-class params provided
            if source_host.is_some() || source_port.is_some() {
                let mut source = serde_json::Map::new();
                source.insert("endpoint_type".to_string(), serde_json::json!(source_type));
                if let Some(h) = source_host {
                    source.insert("host".to_string(), serde_json::json!(h));
                }
                if let Some(p) = source_port {
                    source.insert("port".to_string(), serde_json::json!(p));
                }
                if let Some(pw) = source_password {
                    source.insert("password".to_string(), serde_json::json!(pw));
                }
                if *source_ssl {
                    source.insert("ssl".to_string(), serde_json::json!(true));
                }
                request_obj.insert("source".to_string(), serde_json::Value::Object(source));
            }

            // Build target endpoint if first-class params provided
            if target_bdb.is_some() || target_host.is_some() {
                let mut target = serde_json::Map::new();
                if let Some(bdb) = target_bdb {
                    target.insert("endpoint_type".to_string(), serde_json::json!("redis"));
                    target.insert("bdb_uid".to_string(), serde_json::json!(bdb));
                } else {
                    target.insert("endpoint_type".to_string(), serde_json::json!("redis"));
                    if let Some(h) = target_host {
                        target.insert("host".to_string(), serde_json::json!(h));
                    }
                    if let Some(p) = target_port {
                        target.insert("port".to_string(), serde_json::json!(p));
                    }
                }
                if let Some(pw) = target_password {
                    target.insert("password".to_string(), serde_json::json!(pw));
                }
                if *target_ssl {
                    target.insert("ssl".to_string(), serde_json::json!(true));
                }
                request_obj.insert("target".to_string(), serde_json::Value::Object(target));
            }

            // Add other parameters
            if let Some(mt) = migration_type {
                request_obj.insert("migration_type".to_string(), serde_json::json!(mt));
            }
            if let Some(kp) = key_pattern {
                request_obj.insert("key_pattern".to_string(), serde_json::json!(kp));
            }
            if *flush_target {
                request_obj.insert("flush_target".to_string(), serde_json::json!(true));
            }

            // Validate required fields
            if !request_obj.contains_key("source") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--source-host is required when not using --data".to_string(),
                });
            }
            if !request_obj.contains_key("target") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--target-bdb or --target-host is required when not using --data"
                        .to_string(),
                });
            }

            let payload = serde_json::Value::Object(request_obj);
            let response: serde_json::Value = client
                .post("/v1/migrations", &payload)
                .await
                .context("Failed to create migration")?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        MigrationCommands::Start { uid } => {
            let response: serde_json::Value = client
                .post(
                    &format!("/v1/migrations/{}/start", uid),
                    &serde_json::Value::Null,
                )
                .await
                .context(format!("Failed to start migration {}", uid))?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        MigrationCommands::Pause { uid } => {
            let response: serde_json::Value = client
                .post(
                    &format!("/v1/migrations/{}/pause", uid),
                    &serde_json::Value::Null,
                )
                .await
                .context(format!("Failed to pause migration {}", uid))?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        MigrationCommands::Resume { uid } => {
            let response: serde_json::Value = client
                .post(
                    &format!("/v1/migrations/{}/resume", uid),
                    &serde_json::Value::Null,
                )
                .await
                .context(format!("Failed to resume migration {}", uid))?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        MigrationCommands::Cancel { uid, force } => {
            if !force {
                super::utils::confirm_action(&format!("Cancel migration {}?", uid))?;
            }

            client
                .delete(&format!("/v1/migrations/{}", uid))
                .await
                .context(format!("Failed to cancel migration {}", uid))?;

            println!("Migration {} cancelled successfully", uid);
        }

        MigrationCommands::Export { bdb_uid } => {
            let response: serde_json::Value = client
                .post(
                    &format!("/v1/bdbs/{}/actions/export", bdb_uid),
                    &serde_json::json!({}),
                )
                .await
                .context(format!("Failed to export database {}", bdb_uid))?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        MigrationCommands::Import {
            bdb_uid,
            source_uri,
            source_password,
            key_pattern,
            flush_before,
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
            if let Some(uri) = source_uri {
                request_obj.insert("source_uri".to_string(), serde_json::json!(uri));
            }
            if let Some(pw) = source_password {
                request_obj.insert("source_password".to_string(), serde_json::json!(pw));
            }
            if let Some(kp) = key_pattern {
                request_obj.insert("key_pattern".to_string(), serde_json::json!(kp));
            }
            if *flush_before {
                request_obj.insert("flush_before".to_string(), serde_json::json!(true));
            }

            // Validate at least some configuration is provided
            if request_obj.is_empty() {
                return Err(RedisCtlError::InvalidInput {
                    message: "At least one import configuration is required (--source-uri, --key-pattern, --flush-before, or --data)".to_string(),
                });
            }

            let payload = serde_json::Value::Object(request_obj);
            let response: serde_json::Value = client
                .post(&format!("/v1/bdbs/{}/actions/import", bdb_uid), &payload)
                .await
                .context(format!("Failed to import data to database {}", bdb_uid))?;

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
    fn test_migration_command_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: MigrationCommands,
        }

        // Test list command
        let cli = TestCli::parse_from(["test", "list"]);
        assert!(matches!(cli.cmd, MigrationCommands::List));

        // Test get command
        let cli = TestCli::parse_from(["test", "get", "mig-1"]);
        if let MigrationCommands::Get { uid } = cli.cmd {
            assert_eq!(uid, "mig-1");
        } else {
            panic!("Expected Get command");
        }

        // Test create command with first-class params
        let cli = TestCli::parse_from([
            "test",
            "create",
            "--source-host",
            "redis.external.com",
            "--source-port",
            "6379",
            "--target-bdb",
            "1",
            "--key-pattern",
            "user:*",
        ]);
        if let MigrationCommands::Create {
            source_host,
            source_port,
            target_bdb,
            key_pattern,
            ..
        } = cli.cmd
        {
            assert_eq!(source_host, Some("redis.external.com".to_string()));
            assert_eq!(source_port, Some(6379));
            assert_eq!(target_bdb, Some(1));
            assert_eq!(key_pattern, Some("user:*".to_string()));
        } else {
            panic!("Expected Create command");
        }

        // Test start command
        let cli = TestCli::parse_from(["test", "start", "mig-1"]);
        if let MigrationCommands::Start { uid } = cli.cmd {
            assert_eq!(uid, "mig-1");
        } else {
            panic!("Expected Start command");
        }

        // Test pause command
        let cli = TestCli::parse_from(["test", "pause", "mig-1"]);
        if let MigrationCommands::Pause { uid } = cli.cmd {
            assert_eq!(uid, "mig-1");
        } else {
            panic!("Expected Pause command");
        }

        // Test resume command
        let cli = TestCli::parse_from(["test", "resume", "mig-1"]);
        if let MigrationCommands::Resume { uid } = cli.cmd {
            assert_eq!(uid, "mig-1");
        } else {
            panic!("Expected Resume command");
        }

        // Test cancel command
        let cli = TestCli::parse_from(["test", "cancel", "mig-1", "--force"]);
        if let MigrationCommands::Cancel { uid, force } = cli.cmd {
            assert_eq!(uid, "mig-1");
            assert!(force);
        } else {
            panic!("Expected Cancel command");
        }

        // Test export command
        let cli = TestCli::parse_from(["test", "export", "2"]);
        if let MigrationCommands::Export { bdb_uid } = cli.cmd {
            assert_eq!(bdb_uid, 2);
        } else {
            panic!("Expected Export command");
        }

        // Test import command with first-class params
        let cli = TestCli::parse_from([
            "test",
            "import",
            "3",
            "--source-uri",
            "redis://external:6379",
            "--flush-before",
        ]);
        if let MigrationCommands::Import {
            bdb_uid,
            source_uri,
            flush_before,
            ..
        } = cli.cmd
        {
            assert_eq!(bdb_uid, 3);
            assert_eq!(source_uri, Some("redis://external:6379".to_string()));
            assert!(flush_before);
        } else {
            panic!("Expected Import command");
        }
    }
}
