use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;

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

    /// Get a specific DNS suffix by name
    Get {
        /// DNS suffix name
        name: String,
    },

    /// Create a new DNS suffix
    #[command(after_help = "EXAMPLES:
    # Create a DNS suffix with basic settings
    redisctl enterprise suffix create --name prod --dns-suffix redis.prod.example.com

    # Create suffix using internal addresses only
    redisctl enterprise suffix create --name internal --dns-suffix redis.internal.local --use-internal-addr

    # Create suffix using external addresses only
    redisctl enterprise suffix create --name external --dns-suffix redis.external.example.com --use-external-addr

    # Using JSON for advanced configuration
    redisctl enterprise suffix create --data @suffix.json")]
    Create {
        /// Suffix name identifier (required unless using --data)
        #[arg(long)]
        name: Option<String>,

        /// DNS suffix string for database endpoints
        #[arg(long)]
        dns_suffix: Option<String>,

        /// Use internal addresses for this suffix
        #[arg(long)]
        use_internal_addr: bool,

        /// Use external addresses for this suffix
        #[arg(long)]
        use_external_addr: bool,

        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Update a DNS suffix
    #[command(after_help = "EXAMPLES:
    # Update DNS suffix string
    redisctl enterprise suffix update prod --dns-suffix redis.newprod.example.com

    # Enable internal addresses
    redisctl enterprise suffix update prod --use-internal-addr true

    # Enable external addresses
    redisctl enterprise suffix update prod --use-external-addr true

    # Using JSON for advanced configuration
    redisctl enterprise suffix update prod --data @updates.json")]
    Update {
        /// DNS suffix name to update
        name: String,

        /// New DNS suffix string
        #[arg(long)]
        dns_suffix: Option<String>,

        /// Use internal addresses for this suffix
        #[arg(long)]
        use_internal_addr: Option<bool>,

        /// Use external addresses for this suffix
        #[arg(long)]
        use_external_addr: Option<bool>,

        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete a DNS suffix
    Delete {
        /// DNS suffix name to delete
        name: String,

        /// Skip confirmation prompt
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
        handle_suffix_command_impl(conn_mgr, profile_name, self, output_format, query).await
    }
}

async fn handle_suffix_command_impl(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &SuffixCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    match command {
        SuffixCommands::List => {
            let response: serde_json::Value = client
                .get("/v1/suffixes")
                .await
                .map_err(RedisCtlError::from)?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        SuffixCommands::Get { name } => {
            let response: serde_json::Value = client
                .get(&format!("/v1/suffix/{}", name))
                .await
                .context(format!("Failed to get DNS suffix '{}'", name))?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        SuffixCommands::Create {
            name,
            dns_suffix,
            use_internal_addr,
            use_external_addr,
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
            if let Some(ds) = dns_suffix {
                request_obj.insert("dns_suffix".to_string(), serde_json::json!(ds));
            }
            if *use_internal_addr {
                request_obj.insert("use_internal_addr".to_string(), serde_json::json!(true));
            }
            if *use_external_addr {
                request_obj.insert("use_external_addr".to_string(), serde_json::json!(true));
            }

            // Validate required fields for create
            if !request_obj.contains_key("name") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--name is required when not using --data".to_string(),
                });
            }
            if !request_obj.contains_key("dns_suffix") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--dns-suffix is required when not using --data".to_string(),
                });
            }

            let payload = serde_json::Value::Object(request_obj);
            let response: serde_json::Value = client
                .post("/v1/suffix", &payload)
                .await
                .map_err(RedisCtlError::from)?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        SuffixCommands::Update {
            name,
            dns_suffix,
            use_internal_addr,
            use_external_addr,
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
            if let Some(ds) = dns_suffix {
                request_obj.insert("dns_suffix".to_string(), serde_json::json!(ds));
            }
            if let Some(uia) = use_internal_addr {
                request_obj.insert("use_internal_addr".to_string(), serde_json::json!(uia));
            }
            if let Some(uea) = use_external_addr {
                request_obj.insert("use_external_addr".to_string(), serde_json::json!(uea));
            }

            // Validate at least one update field is provided
            if request_obj.is_empty() {
                return Err(RedisCtlError::InvalidInput {
                    message: "At least one update field is required (--dns-suffix, --use-internal-addr, --use-external-addr, or --data)".to_string(),
                });
            }

            let payload = serde_json::Value::Object(request_obj);
            let response: serde_json::Value = client
                .put(&format!("/v1/suffix/{}", name), &payload)
                .await
                .context(format!("Failed to update DNS suffix '{}'", name))?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            super::utils::print_formatted_output(output_data, output_format)?;
        }

        SuffixCommands::Delete { name, force } => {
            if !force {
                super::utils::confirm_action(&format!("Delete DNS suffix '{}'?", name))?;
            }

            client
                .delete(&format!("/v1/suffix/{}", name))
                .await
                .context(format!("Failed to delete DNS suffix '{}'", name))?;

            println!("DNS suffix '{}' deleted successfully", name);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suffix_command_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: SuffixCommands,
        }

        // Test list command
        let cli = TestCli::parse_from(["test", "list"]);
        assert!(matches!(cli.cmd, SuffixCommands::List));

        // Test get command
        let cli = TestCli::parse_from(["test", "get", "example.redis.local"]);
        if let SuffixCommands::Get { name } = cli.cmd {
            assert_eq!(name, "example.redis.local");
        } else {
            panic!("Expected Get command");
        }

        // Test create command with first-class params
        let cli = TestCli::parse_from([
            "test",
            "create",
            "--name",
            "prod",
            "--dns-suffix",
            "redis.prod.example.com",
            "--use-internal-addr",
        ]);
        if let SuffixCommands::Create {
            name,
            dns_suffix,
            use_internal_addr,
            use_external_addr,
            data,
        } = cli.cmd
        {
            assert_eq!(name, Some("prod".to_string()));
            assert_eq!(dns_suffix, Some("redis.prod.example.com".to_string()));
            assert!(use_internal_addr);
            assert!(!use_external_addr);
            assert!(data.is_none());
        } else {
            panic!("Expected Create command");
        }

        // Test update command
        let cli = TestCli::parse_from([
            "test",
            "update",
            "prod",
            "--dns-suffix",
            "redis.newprod.example.com",
            "--use-external-addr",
            "true",
        ]);
        if let SuffixCommands::Update {
            name,
            dns_suffix,
            use_external_addr,
            ..
        } = cli.cmd
        {
            assert_eq!(name, "prod");
            assert_eq!(dns_suffix, Some("redis.newprod.example.com".to_string()));
            assert_eq!(use_external_addr, Some(true));
        } else {
            panic!("Expected Update command");
        }

        // Test delete command
        let cli = TestCli::parse_from(["test", "delete", "prod", "--force"]);
        if let SuffixCommands::Delete { name, force } = cli.cmd {
            assert_eq!(name, "prod");
            assert!(force);
        } else {
            panic!("Expected Delete command");
        }
    }
}
