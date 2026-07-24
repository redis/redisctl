use crate::cli::OutputFormat;
use crate::commands::enterprise::utils;
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;
use serde_json::Value;

#[derive(Debug, Clone, Subcommand)]
pub enum BootstrapCommands {
    /// Get bootstrap status
    Status,

    /// Bootstrap new cluster
    #[command(
        name = "create-cluster",
        after_help = "EXAMPLES:
    # Create cluster with name
    redisctl enterprise bootstrap create-cluster --name mycluster --license 'LICENSE_KEY'

    # Using JSON for full configuration
    redisctl enterprise bootstrap create-cluster --data @cluster.json"
    )]
    CreateCluster {
        /// Cluster name
        #[arg(long)]
        name: Option<String>,
        /// License key
        #[arg(long)]
        license: Option<String>,
        /// Admin username
        #[arg(long)]
        username: Option<String>,
        /// Admin password
        #[arg(long)]
        password: Option<String>,
        /// JSON data for cluster creation (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Join existing cluster
    #[command(
        name = "join-cluster",
        after_help = "EXAMPLES:
    # Join cluster with URL
    redisctl enterprise bootstrap join-cluster --cluster-url https://cluster.example.com:9443

    # Using JSON for full configuration
    redisctl enterprise bootstrap join-cluster --data @join.json"
    )]
    JoinCluster {
        /// Cluster URL to join
        #[arg(long)]
        cluster_url: Option<String>,
        /// Admin username
        #[arg(long)]
        username: Option<String>,
        /// Admin password
        #[arg(long)]
        password: Option<String>,
        /// JSON data for joining cluster (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Validate bootstrap configuration
    #[command(after_help = "EXAMPLES:
    # Validate create cluster config
    redisctl enterprise bootstrap validate create_cluster --name mycluster

    # Validate with JSON
    redisctl enterprise bootstrap validate join_cluster --data @config.json")]
    Validate {
        /// Action to validate (create_cluster, join_cluster)
        action: String,
        /// Cluster name (for create_cluster)
        #[arg(long)]
        name: Option<String>,
        /// Cluster URL (for join_cluster)
        #[arg(long)]
        cluster_url: Option<String>,
        /// JSON data to validate (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },
}

pub async fn handle_bootstrap_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: BootstrapCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    match cmd {
        BootstrapCommands::Status => {
            handle_bootstrap_status(conn_mgr, profile_name, output_format, query).await
        }
        BootstrapCommands::CreateCluster {
            name,
            license,
            username,
            password,
            data,
        } => {
            handle_create_cluster(
                conn_mgr,
                profile_name,
                name.as_deref(),
                license.as_deref(),
                username.as_deref(),
                password.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        BootstrapCommands::JoinCluster {
            cluster_url,
            username,
            password,
            data,
        } => {
            handle_join_cluster(
                conn_mgr,
                profile_name,
                cluster_url.as_deref(),
                username.as_deref(),
                password.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        BootstrapCommands::Validate {
            action,
            name,
            cluster_url,
            data,
        } => {
            handle_validate_bootstrap(
                conn_mgr,
                profile_name,
                &action,
                name.as_deref(),
                cluster_url.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
    }
}

async fn handle_bootstrap_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let response = client
        .get::<Value>("/v1/bootstrap")
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
async fn handle_create_cluster(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    name: Option<&str>,
    license: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut payload = if let Some(data_str) = data {
        utils::read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let payload_obj = payload.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(n) = name {
        payload_obj.insert("name".to_string(), serde_json::json!(n));
    }
    if let Some(l) = license {
        payload_obj.insert("license".to_string(), serde_json::json!(l));
    }
    if let Some(u) = username {
        let credentials = payload_obj
            .entry("credentials")
            .or_insert(serde_json::json!({}));
        if let Some(cred_obj) = credentials.as_object_mut() {
            cred_obj.insert("username".to_string(), serde_json::json!(u));
        }
    }
    if let Some(p) = password {
        let credentials = payload_obj
            .entry("credentials")
            .or_insert(serde_json::json!({}));
        if let Some(cred_obj) = credentials.as_object_mut() {
            cred_obj.insert("password".to_string(), serde_json::json!(p));
        }
    }

    let response = client
        .post_raw("/v1/bootstrap/create_cluster", payload)
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
async fn handle_join_cluster(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cluster_url: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut payload = if let Some(data_str) = data {
        utils::read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let payload_obj = payload.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(url) = cluster_url {
        payload_obj.insert("cluster_url".to_string(), serde_json::json!(url));
    }
    if let Some(u) = username {
        let credentials = payload_obj
            .entry("credentials")
            .or_insert(serde_json::json!({}));
        if let Some(cred_obj) = credentials.as_object_mut() {
            cred_obj.insert("username".to_string(), serde_json::json!(u));
        }
    }
    if let Some(p) = password {
        let credentials = payload_obj
            .entry("credentials")
            .or_insert(serde_json::json!({}));
        if let Some(cred_obj) = credentials.as_object_mut() {
            cred_obj.insert("password".to_string(), serde_json::json!(p));
        }
    }

    let response = client
        .post_raw("/v1/bootstrap/join_cluster", payload)
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
async fn handle_validate_bootstrap(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    action: &str,
    name: Option<&str>,
    cluster_url: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> Result<(), RedisCtlError> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut payload = if let Some(data_str) = data {
        utils::read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let payload_obj = payload.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(n) = name {
        payload_obj.insert("name".to_string(), serde_json::json!(n));
    }
    if let Some(url) = cluster_url {
        payload_obj.insert("cluster_url".to_string(), serde_json::json!(url));
    }

    let endpoint = format!("/v1/bootstrap/validate/{}", action);
    let response = client
        .post_raw(&endpoint, payload)
        .await
        .context(format!("Failed to validate {} configuration", action))?;

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
    fn test_bootstrap_commands() {
        use clap::CommandFactory;

        #[derive(clap::Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BootstrapCommands,
        }

        TestCli::command().debug_assert();
    }
}
