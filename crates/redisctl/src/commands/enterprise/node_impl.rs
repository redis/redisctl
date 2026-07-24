//! Node command implementations for Redis Enterprise

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use redis_enterprise::nodes::NodeHandler;
use serde_json::Value;
use tabled::{Table, Tabled, settings::Style};

use super::utils::*;

/// Node row for clean table display
#[derive(Tabled)]
struct NodeRow {
    #[tabled(rename = "UID")]
    uid: String,
    #[tabled(rename = "ADDRESS")]
    addr: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "SHARDS")]
    shards: String,
    #[tabled(rename = "MEMORY")]
    memory: String,
    #[tabled(rename = "RACK")]
    rack_id: String,
}

// Node Operations

pub async fn list_nodes(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    let nodes = handler.list().await?;
    let nodes_json = serde_json::to_value(nodes).context("Failed to serialize nodes")?;
    let data = handle_output(nodes_json, output_format, query)?;
    if matches!(resolve_auto(output_format), OutputFormat::Table) {
        print_nodes_table(&data)?;
    } else {
        print_formatted_output(data, output_format)?;
    }
    Ok(())
}

pub async fn get_node(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    let node = handler.get(id).await?;
    let node_json = serde_json::to_value(node).context("Failed to serialize node")?;
    let data = handle_output(node_json, output_format, query)?;
    if matches!(resolve_auto(output_format), OutputFormat::Table) {
        print_node_detail(&data)?;
    } else {
        print_formatted_output(data, output_format)?;
    }
    Ok(())
}

/// Print nodes in clean table format
fn print_nodes_table(data: &Value) -> CliResult<()> {
    let nodes = match data {
        Value::Array(arr) => arr.clone(),
        _ => {
            println!("No nodes found");
            return Ok(());
        }
    };

    if nodes.is_empty() {
        println!("No nodes found");
        return Ok(());
    }

    let mut rows = Vec::new();
    for node in &nodes {
        let total_mem = node.get("total_memory").and_then(|v| v.as_u64());
        let memory = total_mem
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());

        rows.push(NodeRow {
            uid: extract_field(node, "uid", "-"),
            addr: extract_field(node, "addr", "-"),
            status: format_status(extract_field(node, "status", "unknown")),
            shards: extract_field(node, "shard_count", "-"),
            memory,
            rack_id: extract_field(node, "rack_id", "-"),
        });
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    output_with_pager(&table.to_string());
    Ok(())
}

/// Print node detail in key-value format
fn print_node_detail(data: &Value) -> CliResult<()> {
    let mut rows = Vec::new();

    let fields = [
        ("UID", "uid"),
        ("Address", "addr"),
        ("Status", "status"),
        ("Rack ID", "rack_id"),
        ("OS Version", "os_version"),
        ("Software Version", "software_version"),
        ("Shard Count", "shard_count"),
        ("Accept Servers", "accept_servers"),
    ];

    for (label, key) in &fields {
        if let Some(val) = data.get(*key) {
            let display = match val {
                Value::Null => continue,
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                _ => val.to_string(),
            };
            rows.push(DetailRow {
                field: label.to_string(),
                value: display,
            });
        }
    }

    if let Some(mem) = data.get("total_memory").and_then(|v| v.as_u64()) {
        rows.push(DetailRow {
            field: "Total Memory".to_string(),
            value: format_bytes(mem),
        });
    }

    if rows.is_empty() {
        println!("No node information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    output_with_pager(&table.to_string());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn add_node(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    address: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut node_data = if let Some(data_str) = data {
        read_json_data(data_str).context("Failed to parse node data")?
    } else {
        serde_json::json!({})
    };

    let node_obj = node_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(addr) = address {
        node_obj.insert("address".to_string(), serde_json::json!(addr));
    }
    if let Some(user) = username {
        node_obj.insert("username".to_string(), serde_json::json!(user));
    }
    if let Some(pass) = password {
        node_obj.insert("password".to_string(), serde_json::json!(pass));
    }

    // Note: The actual add node operation typically requires cluster join operations
    // This is a placeholder for the actual implementation which would use cluster join
    let result = client.post_raw("/v1/nodes", node_data).await?;
    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn remove_node(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    force: bool,
    _output_format: OutputFormat,
    _query: Option<&str>,
) -> CliResult<()> {
    if !force && !confirm_action(&format!("Remove node {} from cluster?", id))? {
        println!("Operation cancelled");
        return Ok(());
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    handler.remove(id).await?;
    println!("Node {} removed successfully", id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_node(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    accept_servers: Option<bool>,
    external_addr: Option<Vec<String>>,
    rack_id: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    use crate::error::RedisCtlError;

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Start with JSON data if provided, otherwise empty object
    let mut request_obj: serde_json::Map<String, serde_json::Value> = if let Some(json_data) = data
    {
        let parsed = read_json_data(json_data).context("Failed to parse JSON data")?;
        parsed
            .as_object()
            .cloned()
            .unwrap_or_else(serde_json::Map::new)
    } else {
        serde_json::Map::new()
    };

    // Override with first-class parameters if provided
    if let Some(accept) = accept_servers {
        request_obj.insert("accept_servers".to_string(), serde_json::json!(accept));
    }
    if let Some(addrs) = &external_addr {
        request_obj.insert("external_addr".to_string(), serde_json::json!(addrs));
    }
    if let Some(rack) = rack_id {
        request_obj.insert("rack_id".to_string(), serde_json::json!(rack));
    }

    // Validate at least one update field is provided
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--accept-servers, --external-addr, --rack-id, or --data)".to_string(),
        });
    }

    let update_data = serde_json::Value::Object(request_obj);
    let updated = handler.update(id, update_data).await?;
    let updated_json = serde_json::to_value(updated).context("Failed to serialize updated node")?;
    let data = handle_output(updated_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

// Node Status & Health

pub async fn get_node_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    let status = handler.status(id).await?;
    let data = handle_output(status, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_stats(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    let stats = handler.stats(id).await?;
    let stats_json = serde_json::to_value(stats).context("Failed to serialize stats")?;
    let data = handle_output(stats_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_metrics(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    interval: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Metrics endpoint typically requires interval parameter
    let endpoint = if let Some(interval) = interval {
        format!("/v1/nodes/{}/metrics?interval={}", id, interval)
    } else {
        format!("/v1/nodes/{}/metrics", id)
    };

    let metrics = client.get_raw(&endpoint).await?;
    let data = handle_output(metrics, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn check_node_health(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Health check typically combines multiple status endpoints
    let handler = NodeHandler::new(client);
    let status = handler.status(id).await?;
    let data = handle_output(status, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_alerts(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    let alerts = handler.alerts_for(id).await?;
    let data = handle_output(alerts, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

// Node Maintenance

pub async fn enable_maintenance(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    let result = handler.execute_action(id, "maintenance_on").await?;
    let result_json = serde_json::to_value(result).context("Failed to serialize result")?;
    let data = handle_output(result_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn disable_maintenance(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);
    let result = handler.execute_action(id, "maintenance_off").await?;
    let result_json = serde_json::to_value(result).context("Failed to serialize result")?;
    let data = handle_output(result_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn rebalance_node(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Rebalance typically uses the rebalance action
    let result = handler.execute_action(id, "rebalance").await?;
    let result_json = serde_json::to_value(result).context("Failed to serialize result")?;
    let data = handle_output(result_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn drain_node(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Drain is typically done via the drain action
    let result = handler.execute_action(id, "drain").await?;
    let result_json = serde_json::to_value(result).context("Failed to serialize result")?;
    let data = handle_output(result_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn restart_node(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    if !force && !confirm_action(&format!("Restart node {} services?", id))? {
        println!("Operation cancelled");
        return Ok(());
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Restart typically uses the restart action
    let result = handler.execute_action(id, "restart").await?;
    let result_json = serde_json::to_value(result).context("Failed to serialize result")?;
    let data = handle_output(result_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

// Node Configuration

pub async fn get_node_config(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Configuration is typically part of the node details
    let handler = NodeHandler::new(client);
    let node = handler.get(id).await?;
    let node_json = serde_json::to_value(node).context("Failed to serialize node")?;
    let data = handle_output(node_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_node_config(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    max_redis_servers: Option<u32>,
    bigstore_driver: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Start with JSON from --data if provided, otherwise empty object
    let mut config_data = if let Some(data_str) = data {
        read_json_data(data_str).context("Failed to parse config data")?
    } else {
        serde_json::json!({})
    };

    let config_obj = config_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(max_servers) = max_redis_servers {
        config_obj.insert(
            "max_redis_servers".to_string(),
            serde_json::json!(max_servers),
        );
    }
    if let Some(driver) = bigstore_driver {
        config_obj.insert("bigstore_driver".to_string(), serde_json::json!(driver));
    }

    let updated = handler.update(id, config_data).await?;
    let updated_json = serde_json::to_value(updated).context("Failed to serialize updated node")?;
    let data = handle_output(updated_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_rack(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Rack info is part of node details
    let node = handler.get(id).await?;
    let node_json = serde_json::to_value(node).context("Failed to serialize node")?;
    let data = handle_output(node_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn set_node_rack(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    rack: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    let update_data = serde_json::json!({
        "rack_id": rack
    });

    let updated = handler.update(id, update_data).await?;
    let updated_json = serde_json::to_value(updated).context("Failed to serialize updated node")?;
    let data = handle_output(updated_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_role(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Role info is part of node details
    let node = handler.get(id).await?;
    let node_json = serde_json::to_value(node).context("Failed to serialize node")?;
    let data = handle_output(node_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

// Node Resources

pub async fn get_node_resources(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Resources are typically in stats
    let stats = handler.stats(id).await?;
    let stats_json = serde_json::to_value(stats).context("Failed to serialize stats")?;
    let data = handle_output(stats_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_memory(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Memory details are in stats
    let stats = handler.stats(id).await?;
    let stats_json = serde_json::to_value(stats).context("Failed to serialize stats")?;
    let data = handle_output(stats_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_cpu(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // CPU details are in stats
    let stats = handler.stats(id).await?;
    let stats_json = serde_json::to_value(stats).context("Failed to serialize stats")?;
    let data = handle_output(stats_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_storage(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Storage details are in stats
    let stats = handler.stats(id).await?;
    let stats_json = serde_json::to_value(stats).context("Failed to serialize stats")?;
    let data = handle_output(stats_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_node_network(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = NodeHandler::new(client);

    // Network stats are typically in stats
    let stats = handler.stats(id).await?;
    let stats_json = serde_json::to_value(stats).context("Failed to serialize stats")?;
    let data = handle_output(stats_json, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}
