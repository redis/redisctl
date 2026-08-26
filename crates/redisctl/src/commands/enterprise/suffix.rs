use anyhow::Context;
use clap::Subcommand;

use crate::error::RedisCtlError;
use crate::{cli::OutputFormat, connection::ConnectionManager, error::Result as CliResult};

pub async fn handle_suffix_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    suffix_cmd: SuffixCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    suffix_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}

#[derive(Debug, Clone, Subcommand)]
pub enum SuffixCommands {
    /// List all DNS suffixes
    List,

    /// Get a DNS suffix by name
    Get {
        /// DNS suffix name
        name: String,
    },

    /// Update a DNS suffix (Redis Software 8.0+)
    #[command(
        after_help = "Redis Software 7.x does not expose a suffix update method.

EXAMPLES:
    redisctl enterprise suffix update prod --dns-suffix redis.example.com
    redisctl enterprise suffix update prod --data @updates.json"
    )]
    Update {
        /// DNS suffix name
        name: String,
        /// New DNS suffix string
        #[arg(long)]
        dns_suffix: Option<String>,
        /// Use internal addresses
        #[arg(long)]
        use_internal_addr: Option<bool>,
        /// Use external addresses
        #[arg(long)]
        use_external_addr: Option<bool>,
        /// Update configuration (file, stdin, or inline JSON)
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete a DNS suffix
    Delete {
        /// DNS suffix name
        name: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

impl SuffixCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;

        let response = match self {
            SuffixCommands::List => serde_json::to_value(
                client
                    .suffixes()
                    .list()
                    .await
                    .map_err(RedisCtlError::from)?,
            )
            .context("Failed to serialize DNS suffixes")?,
            SuffixCommands::Get { name } => serde_json::to_value(
                client
                    .suffixes()
                    .get(name)
                    .await
                    .with_context(|| format!("Failed to get DNS suffix '{name}'"))?,
            )
            .context("Failed to serialize DNS suffix")?,
            SuffixCommands::Update {
                name,
                dns_suffix,
                use_internal_addr,
                use_external_addr,
                data,
            } => {
                let mut updates = if let Some(data) = data {
                    super::utils::read_json_data(data)?
                        .as_object()
                        .cloned()
                        .ok_or_else(|| RedisCtlError::InvalidInput {
                            message: "Suffix update must be a JSON object".to_string(),
                        })?
                } else {
                    serde_json::Map::new()
                };
                if let Some(dns_suffix) = dns_suffix {
                    updates.insert("dns_suffix".to_string(), serde_json::json!(dns_suffix));
                }
                if let Some(use_internal_addr) = use_internal_addr {
                    updates.insert(
                        "use_internal_addr".to_string(),
                        serde_json::json!(use_internal_addr),
                    );
                }
                if let Some(use_external_addr) = use_external_addr {
                    updates.insert(
                        "use_external_addr".to_string(),
                        serde_json::json!(use_external_addr),
                    );
                }
                if updates.is_empty() {
                    return Err(RedisCtlError::InvalidInput {
                        message: "Provide an update field or --data".to_string(),
                    });
                }

                client
                    .patch_raw(
                        &format!("/v1/suffix/{name}"),
                        serde_json::Value::Object(updates),
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to update DNS suffix '{name}'; suffix updates require Redis Software 8.0+"
                        )
                    })?
            }
            SuffixCommands::Delete { name, force } => {
                if !force {
                    super::utils::confirm_action(&format!("Delete DNS suffix '{name}'?"))?;
                }
                client
                    .suffixes()
                    .delete(name)
                    .await
                    .with_context(|| format!("Failed to delete DNS suffix '{name}'"))?;
                serde_json::json!({"deleted": true, "name": name})
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
        cmd: SuffixCommands,
    }

    #[test]
    fn suffix_creation_is_not_exposed() {
        TestCli::command().debug_assert();
        assert!(TestCli::try_parse_from(["test", "create"]).is_err());
        assert!(matches!(
            TestCli::parse_from([
                "test",
                "update",
                "prod",
                "--dns-suffix",
                "redis.example.com"
            ])
            .cmd,
            SuffixCommands::Update { .. }
        ));
    }
}
