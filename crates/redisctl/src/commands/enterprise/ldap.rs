use crate::cli::OutputFormat;
use crate::commands::enterprise::utils;
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;
use serde_json::Value;

#[derive(Debug, Clone, Subcommand)]
pub enum LdapCommands {
    /// Get LDAP configuration
    Get,

    /// Update LDAP configuration
    #[command(after_help = "EXAMPLES:
    # Update LDAP server URLs
    redisctl enterprise ldap update --server-urls ldap://ldap.example.com:389

    # Update bind credentials
    redisctl enterprise ldap update --bind-dn 'CN=admin,DC=example,DC=com' --bind-pass secret

    # Update search settings
    redisctl enterprise ldap update --user-dn 'OU=Users,DC=example,DC=com' \\
        --group-dn 'OU=Groups,DC=example,DC=com'

    # Using JSON for full configuration
    redisctl enterprise ldap update --data @ldap.json")]
    Update {
        /// LDAP server URLs (comma-separated)
        #[arg(long)]
        server_urls: Option<String>,
        /// Bind DN for LDAP connection
        #[arg(long)]
        bind_dn: Option<String>,
        /// Bind password for LDAP connection
        #[arg(long)]
        bind_pass: Option<String>,
        /// Base DN for user searches
        #[arg(long)]
        user_dn: Option<String>,
        /// Base DN for group searches
        #[arg(long)]
        group_dn: Option<String>,
        /// Enable/disable LDAP
        #[arg(long)]
        enabled: Option<bool>,
        /// JSON data for LDAP configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete LDAP configuration
    Delete,

    /// Test LDAP connection
    Test {
        /// Optional test configuration JSON
        #[arg(long)]
        data: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum LdapMappingsCommands {
    /// List all LDAP mappings
    List,

    /// Get specific LDAP mapping
    Get {
        /// Mapping UID
        uid: u64,
    },

    /// Create new LDAP mapping
    #[command(after_help = "EXAMPLES:
    # Create LDAP mapping with required fields
    redisctl enterprise ldap-mappings create --name engineers --dn 'CN=Engineers,OU=Groups,DC=example,DC=com' --role db_viewer

    # Create with email alerts
    redisctl enterprise ldap-mappings create --name admins --dn 'CN=Admins,OU=Groups,DC=example,DC=com' --role admin --email alerts@example.com

    # Using JSON for advanced configuration
    redisctl enterprise ldap-mappings create --data '{\"name\":\"ops\",\"dn\":\"CN=Ops,OU=Groups,DC=example,DC=com\",\"role\":\"db_member\"}'")]
    Create {
        /// Mapping name
        #[arg(long)]
        name: Option<String>,
        /// LDAP group distinguished name
        #[arg(long)]
        dn: Option<String>,
        /// Role identifier
        #[arg(long)]
        role: Option<String>,
        /// Email address for alerts
        #[arg(long)]
        email: Option<String>,
        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Update existing LDAP mapping
    #[command(after_help = "EXAMPLES:
    # Update mapping name
    redisctl enterprise ldap-mappings update 1 --name new-name

    # Update role
    redisctl enterprise ldap-mappings update 1 --role admin

    # Update email
    redisctl enterprise ldap-mappings update 1 --email newalerts@example.com

    # Using JSON for advanced configuration
    redisctl enterprise ldap-mappings update 1 --data '{\"role_uids\":[1,2,3]}'")]
    Update {
        /// Mapping UID
        uid: u64,
        /// Mapping name
        #[arg(long)]
        name: Option<String>,
        /// LDAP group distinguished name
        #[arg(long)]
        dn: Option<String>,
        /// Role identifier
        #[arg(long)]
        role: Option<String>,
        /// Email address for alerts
        #[arg(long)]
        email: Option<String>,
        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete LDAP mapping
    Delete {
        /// Mapping UID
        uid: u64,
    },
}

pub async fn handle_ldap_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: LdapCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    match cmd {
        LdapCommands::Get => handle_ldap_get(conn_mgr, profile_name, output_format, query).await,
        LdapCommands::Update {
            server_urls,
            bind_dn,
            bind_pass,
            user_dn,
            group_dn,
            enabled,
            data,
        } => {
            handle_ldap_update(
                conn_mgr,
                profile_name,
                server_urls.as_deref(),
                bind_dn.as_deref(),
                bind_pass.as_deref(),
                user_dn.as_deref(),
                group_dn.as_deref(),
                enabled,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        LdapCommands::Delete => {
            handle_ldap_delete(conn_mgr, profile_name, output_format, query).await
        }
        LdapCommands::Test { data } => {
            handle_ldap_test(
                conn_mgr,
                profile_name,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
    }
}

pub async fn handle_ldap_mappings_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: LdapMappingsCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    match cmd {
        LdapMappingsCommands::List => {
            handle_mappings_list(conn_mgr, profile_name, output_format, query).await
        }
        LdapMappingsCommands::Get { uid } => {
            handle_mappings_get(conn_mgr, profile_name, uid, output_format, query).await
        }
        LdapMappingsCommands::Create {
            name,
            dn,
            role,
            email,
            data,
        } => {
            handle_mappings_create(
                conn_mgr,
                profile_name,
                name.as_deref(),
                dn.as_deref(),
                role.as_deref(),
                email.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        LdapMappingsCommands::Update {
            uid,
            name,
            dn,
            role,
            email,
            data,
        } => {
            handle_mappings_update(
                conn_mgr,
                profile_name,
                uid,
                name.as_deref(),
                dn.as_deref(),
                role.as_deref(),
                email.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        LdapMappingsCommands::Delete { uid } => {
            handle_mappings_delete(conn_mgr, profile_name, uid, output_format, query).await
        }
    }
}

async fn handle_ldap_get(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .get::<Value>("/v1/cluster/ldap")
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

#[allow(clippy::too_many_arguments)]
async fn handle_ldap_update(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    server_urls: Option<&str>,
    bind_dn: Option<&str>,
    bind_pass: Option<&str>,
    user_dn: Option<&str>,
    group_dn: Option<&str>,
    enabled: Option<bool>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut payload = if let Some(data_str) = data {
        utils::read_json_data(data_str).context("Invalid JSON data for LDAP configuration")?
    } else {
        serde_json::json!({})
    };

    let payload_obj = payload.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(urls) = server_urls {
        payload_obj.insert(
            "uris".to_string(),
            serde_json::json!(urls.split(',').collect::<Vec<_>>()),
        );
    }
    if let Some(dn) = bind_dn {
        payload_obj.insert("bind_dn".to_string(), serde_json::json!(dn));
    }
    if let Some(pass) = bind_pass {
        payload_obj.insert("bind_pass".to_string(), serde_json::json!(pass));
    }
    if let Some(dn) = user_dn {
        payload_obj.insert("user_dn".to_string(), serde_json::json!(dn));
    }
    if let Some(dn) = group_dn {
        payload_obj.insert("group_dn".to_string(), serde_json::json!(dn));
    }
    if let Some(en) = enabled {
        payload_obj.insert("enabled".to_string(), serde_json::json!(en));
    }

    let response = client
        .put_raw("/v1/cluster/ldap", payload)
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_ldap_delete(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .delete_raw("/v1/cluster/ldap")
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_ldap_test(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let payload = if let Some(d) = data {
        serde_json::from_str(d).context("Invalid JSON data for LDAP test")?
    } else {
        serde_json::json!({})
    };

    let response = client
        .post_raw("/v1/cluster/ldap/test", payload)
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

// LDAP Mappings handlers
async fn handle_mappings_list(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .get::<Value>("/v1/ldap_mappings")
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_mappings_get(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: u64,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let endpoint = format!("/v1/ldap_mappings/{}", uid);
    let response = client
        .get::<Value>(&endpoint)
        .await
        .context(format!("Failed to get LDAP mapping {}", uid))?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

#[allow(clippy::too_many_arguments)]
async fn handle_mappings_create(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    name: Option<&str>,
    dn: Option<&str>,
    role: Option<&str>,
    email: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON data if provided, otherwise empty object
    let mut request_obj: serde_json::Map<String, serde_json::Value> = if let Some(json_data) = data
    {
        let parsed: Value =
            serde_json::from_str(json_data).context("Invalid JSON data for LDAP mapping")?;
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
    if let Some(d) = dn {
        request_obj.insert("dn".to_string(), serde_json::json!(d));
    }
    if let Some(r) = role {
        request_obj.insert("role".to_string(), serde_json::json!(r));
    }
    if let Some(e) = email {
        request_obj.insert("email".to_string(), serde_json::json!(e));
    }

    // Validate required fields for create
    if !request_obj.contains_key("name") {
        return Err(RedisCtlError::InvalidInput {
            message: "--name is required when not using --data".to_string(),
        });
    }
    if !request_obj.contains_key("dn") {
        return Err(RedisCtlError::InvalidInput {
            message: "--dn is required when not using --data".to_string(),
        });
    }
    if !request_obj.contains_key("role") {
        return Err(RedisCtlError::InvalidInput {
            message: "--role is required when not using --data".to_string(),
        });
    }

    let payload = serde_json::Value::Object(request_obj);
    let response = client
        .post_raw("/v1/ldap_mappings", payload)
        .await
        .map_err(RedisCtlError::from)?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

#[allow(clippy::too_many_arguments)]
async fn handle_mappings_update(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: u64,
    name: Option<&str>,
    dn: Option<&str>,
    role: Option<&str>,
    email: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON data if provided, otherwise empty object
    let mut request_obj: serde_json::Map<String, serde_json::Value> = if let Some(json_data) = data
    {
        let parsed: Value =
            serde_json::from_str(json_data).context("Invalid JSON data for LDAP mapping update")?;
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
    if let Some(d) = dn {
        request_obj.insert("dn".to_string(), serde_json::json!(d));
    }
    if let Some(r) = role {
        request_obj.insert("role".to_string(), serde_json::json!(r));
    }
    if let Some(e) = email {
        request_obj.insert("email".to_string(), serde_json::json!(e));
    }

    // Validate at least one update field is provided
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message:
                "At least one update field is required (--name, --dn, --role, --email, or --data)"
                    .to_string(),
        });
    }

    let payload = serde_json::Value::Object(request_obj);
    let endpoint = format!("/v1/ldap_mappings/{}", uid);
    let response = client
        .put_raw(&endpoint, payload)
        .await
        .context(format!("Failed to update LDAP mapping {}", uid))?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

async fn handle_mappings_delete(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: u64,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let endpoint = format!("/v1/ldap_mappings/{}", uid);
    let response = client
        .delete_raw(&endpoint)
        .await
        .context(format!("Failed to delete LDAP mapping {}", uid))?;

    let result = if let Some(q) = query {
        utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    utils::print_formatted_output(result, output_format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ldap_commands() {
        use clap::CommandFactory;

        #[derive(clap::Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: LdapCommands,
        }

        TestCli::command().debug_assert();
    }

    #[test]
    fn test_ldap_mappings_commands() {
        use clap::CommandFactory;

        #[derive(clap::Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: LdapMappingsCommands,
        }

        TestCli::command().debug_assert();
    }
}
