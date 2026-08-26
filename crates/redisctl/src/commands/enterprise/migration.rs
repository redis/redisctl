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
    /// Get a migration by UID
    Get {
        /// Migration UID
        uid: String,
    },

    /// Export database data through the database action API
    Export {
        /// Database UID
        bdb_uid: u32,
    },

    /// Import database data through the database action API
    #[command(after_help = "EXAMPLES:
    redisctl enterprise migration import 1 --source-uri redis://external-redis:6379
    redisctl enterprise migration import 1 --data @import.json")]
    Import {
        /// Database UID
        bdb_uid: u32,
        /// Source Redis URI (for example, redis://host:port)
        #[arg(long)]
        source_uri: Option<String>,
        /// Source authentication password
        #[arg(long)]
        source_password: Option<String>,
        /// Redis key pattern to import
        #[arg(long)]
        key_pattern: Option<String>,
        /// Flush the database before import
        #[arg(long)]
        flush_before: bool,
        /// Import configuration (file, stdin, or inline JSON)
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
        let client = conn_mgr.create_enterprise_client(profile_name).await?;

        let response = match self {
            MigrationCommands::Get { uid } => {
                let migration = client
                    .migrations()
                    .get(uid)
                    .await
                    .with_context(|| format!("Failed to get migration {uid}"))?;
                serde_json::to_value(migration).context("Failed to serialize migration")?
            }
            MigrationCommands::Export { bdb_uid } => client
                .post(
                    &format!("/v1/bdbs/{bdb_uid}/actions/export"),
                    &serde_json::json!({}),
                )
                .await
                .with_context(|| format!("Failed to export database {bdb_uid}"))?,
            MigrationCommands::Import {
                bdb_uid,
                source_uri,
                source_password,
                key_pattern,
                flush_before,
                data,
            } => {
                let mut request = if let Some(data) = data {
                    super::utils::read_json_data(data)?
                        .as_object()
                        .cloned()
                        .ok_or_else(|| RedisCtlError::InvalidInput {
                            message: "Import configuration must be a JSON object".to_string(),
                        })?
                } else {
                    serde_json::Map::new()
                };

                if let Some(source_uri) = source_uri {
                    request.insert("source_uri".to_string(), serde_json::json!(source_uri));
                }
                if let Some(source_password) = source_password {
                    request.insert(
                        "source_password".to_string(),
                        serde_json::json!(source_password),
                    );
                }
                if let Some(key_pattern) = key_pattern {
                    request.insert("key_pattern".to_string(), serde_json::json!(key_pattern));
                }
                if *flush_before {
                    request.insert("flush_before".to_string(), serde_json::json!(true));
                }
                if request.is_empty() {
                    return Err(RedisCtlError::InvalidInput {
                        message: "Provide --source-uri, --key-pattern, --flush-before, or --data"
                            .to_string(),
                    });
                }

                client
                    .post(
                        &format!("/v1/bdbs/{bdb_uid}/actions/import"),
                        &serde_json::Value::Object(request),
                    )
                    .await
                    .with_context(|| format!("Failed to import data to database {bdb_uid}"))?
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: MigrationCommands,
    }

    #[test]
    fn retired_migration_routes_are_not_exposed() {
        TestCli::command().debug_assert();
        assert!(matches!(
            TestCli::parse_from(["test", "get", "migration-1"]).cmd,
            MigrationCommands::Get { .. }
        ));
        assert!(TestCli::try_parse_from(["test", "list"]).is_err());
        assert!(TestCli::try_parse_from(["test", "start", "migration-1"]).is_err());
        assert!(TestCli::try_parse_from(["test", "cancel", "migration-1"]).is_err());
    }
}
