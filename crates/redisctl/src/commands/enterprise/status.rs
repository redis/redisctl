//! Comprehensive status command implementation for Redis Enterprise
//!
//! Provides a single command to view cluster, nodes, databases, and shards status,
//! similar to `rladmin status extra all`.

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use colored::Colorize;
use redis_enterprise::bdb::BdbHandler;
use redis_enterprise::cluster::ClusterHandler;
use redis_enterprise::nodes::NodeHandler;
use redis_enterprise::shards::ShardHandler;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tabled::builder::Builder;
use tabled::settings::Style;

use super::utils::*;

/// Comprehensive cluster status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    /// Cluster information
    pub cluster: Value,
    /// List of nodes
    pub nodes: Value,
    /// List of databases
    pub databases: Value,
    /// List of shards
    pub shards: Value,
    /// Summary statistics
    pub summary: StatusSummary,
}

/// Summary statistics for cluster health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSummary {
    /// Total number of nodes
    pub total_nodes: usize,
    /// Number of healthy nodes
    pub healthy_nodes: usize,
    /// Total number of databases
    pub total_databases: usize,
    /// Number of active databases
    pub active_databases: usize,
    /// Total number of shards
    pub total_shards: usize,
    /// Cluster health status
    pub cluster_health: String,
}

/// Sections to display in status output
#[derive(Debug, Clone, Default)]
pub struct StatusSections {
    /// Show cluster information
    pub cluster: bool,
    /// Show nodes information
    pub nodes: bool,
    /// Show databases information
    pub databases: bool,
    /// Show shards information
    pub shards: bool,
}

impl StatusSections {
    /// Create sections showing all information
    pub fn all() -> Self {
        Self {
            cluster: true,
            nodes: true,
            databases: true,
            shards: true,
        }
    }

    /// Check if any section is enabled
    pub fn any_enabled(&self) -> bool {
        self.cluster || self.nodes || self.databases || self.shards
    }
}

/// Get comprehensive cluster status
pub async fn get_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    sections: StatusSections,
    brief: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Use provided sections, or default to all if none specified
    let sections = if sections.any_enabled() {
        sections
    } else {
        StatusSections::all()
    };

    // Collect cluster info
    let cluster_result = if sections.cluster {
        ClusterHandler::new(client.clone())
            .info()
            .await
            .map(|v| serde_json::to_value(v).unwrap_or(json!({})))
            .context("Failed to get cluster info")?
    } else {
        json!({})
    };

    // Collect nodes
    let nodes_result = if sections.nodes {
        NodeHandler::new(client.clone())
            .list()
            .await
            .map(|v| serde_json::to_value(v).unwrap_or(json!([])))
            .context("Failed to list nodes")?
    } else {
        json!([])
    };

    // Collect databases
    let databases_result = if sections.databases {
        BdbHandler::new(client.clone())
            .list()
            .await
            .map(|v| serde_json::to_value(v).unwrap_or(json!([])))
            .context("Failed to list databases")?
    } else {
        json!([])
    };

    // Collect shards
    let shards_result = if sections.shards {
        ShardHandler::new(client.clone())
            .list()
            .await
            .map(|v| serde_json::to_value(v).unwrap_or(json!([])))
            .context("Failed to list shards")?
    } else {
        json!([])
    };

    // Calculate summary statistics
    let summary = calculate_summary(&nodes_result, &databases_result, &shards_result);

    // Brief mode: print compact health summary and return
    if brief {
        let warnings = collect_warnings(&cluster_result, &nodes_result, &databases_result);
        print_brief_summary(&summary, &warnings);
        return Ok(());
    }

    // Table/Auto format without query: print colored tables
    if matches!(output_format, OutputFormat::Table | OutputFormat::Auto) && query.is_none() {
        print_status_tables(
            &sections,
            &cluster_result,
            &nodes_result,
            &databases_result,
            &shards_result,
            &summary,
        );
        return Ok(());
    }

    // Build comprehensive status for JSON/YAML/query output
    let status = ClusterStatus {
        cluster: cluster_result,
        nodes: nodes_result,
        databases: databases_result,
        shards: shards_result,
        summary,
    };

    let status_json = serde_json::to_value(status).context("Failed to serialize cluster status")?;

    // Apply query if provided
    let data = handle_output(status_json, output_format, query)?;

    // Format and display
    print_formatted_output(data, output_format)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Brief summary
// ---------------------------------------------------------------------------

/// Print a compact one-line health summary with counts and warnings
fn print_brief_summary(summary: &StatusSummary, warnings: &[String]) {
    let health_label = match summary.cluster_health.as_str() {
        "healthy" => "HEALTHY".green().bold(),
        "degraded" => "DEGRADED".yellow().bold(),
        _ => "CRITICAL".red().bold(),
    };

    println!(
        "Cluster: {}  |  Nodes: {}/{}  |  Databases: {}/{}  |  Shards: {}",
        health_label,
        summary.healthy_nodes,
        summary.total_nodes,
        summary.active_databases,
        summary.total_databases,
        summary.total_shards,
    );

    if !warnings.is_empty() {
        println!();
        for w in warnings {
            println!("  {} {}", "!".yellow().bold(), w);
        }
    }
}

/// Collect actionable warnings from cluster data
fn collect_warnings(cluster: &Value, nodes: &Value, databases: &Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let empty_vec = vec![];

    // License expiry
    if let Some(exp) = cluster.get("license_expire_time").and_then(|v| v.as_str())
        && exp != "N/A"
        && !exp.is_empty()
    {
        warnings.push(format!("License expires: {exp}"));
    }

    // Unhealthy nodes
    let nodes_array = nodes.as_array().unwrap_or(&empty_vec);
    let unhealthy: Vec<String> = nodes_array
        .iter()
        .filter(|n| {
            n.get("status")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s != "active" && s != "ok")
        })
        .map(|n| {
            let uid = n.get("uid").and_then(|v| v.as_u64()).unwrap_or(0);
            let status = n
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Node {uid} is {status}")
        })
        .collect();
    warnings.extend(unhealthy);

    // High memory usage on databases
    let databases_array = databases.as_array().unwrap_or(&empty_vec);
    for db in databases_array {
        let name = db.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let used = db
            .get("memory_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let limit = db
            .get("memory_limit")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if limit > 0.0 {
            let pct = used / limit * 100.0;
            if pct > 90.0 {
                warnings.push(format!("Database '{name}' memory at {pct:.0}% (critical)"));
            } else if pct > 75.0 {
                warnings.push(format!("Database '{name}' memory at {pct:.0}%"));
            }
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// Table output
// ---------------------------------------------------------------------------

/// Print status as colored, sectioned tables
fn print_status_tables(
    sections: &StatusSections,
    cluster: &Value,
    nodes: &Value,
    databases: &Value,
    shards: &Value,
    summary: &StatusSummary,
) {
    if sections.cluster {
        print_cluster_table(cluster);
    }
    if sections.nodes {
        print_nodes_table(nodes);
    }
    if sections.databases {
        print_databases_table(databases);
    }
    if sections.shards {
        print_shards_table(shards);
    }
    print_summary_line(summary);
}

fn print_cluster_table(cluster: &Value) {
    println!("{}", "CLUSTER".bold());
    let mut builder = Builder::default();
    builder.push_record(["Field", "Value"]);

    let name = cluster.get("name").and_then(|v| v.as_str()).unwrap_or("-");
    builder.push_record(["Name", name]);

    if let Some(status) = cluster.get("status").and_then(|v| v.as_str()) {
        builder.push_record(["Status", &status_colored(status)]);
    }

    if let Some(rack_aware) = cluster.get("rack_aware").and_then(|v| v.as_bool()) {
        let label = if rack_aware { "Yes" } else { "No" };
        builder.push_record(["Rack Aware", label]);
    }

    if let Some(exp) = cluster.get("license_expire_time").and_then(|v| v.as_str()) {
        builder.push_record(["License Expires", exp]);
    }

    println!("{}", builder.build().with(Style::blank()));
    println!();
}

fn print_nodes_table(nodes: &Value) {
    let empty_vec = vec![];
    let nodes_array = nodes.as_array().unwrap_or(&empty_vec);
    println!("{}", "NODES".bold());
    let mut builder = Builder::default();
    builder.push_record(["UID", "Address", "Status", "Shards", "Memory", "Rack ID"]);

    for node in nodes_array {
        let uid = node
            .get("uid")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or_default();
        let addr = node.get("addr").and_then(|v| v.as_str()).unwrap_or("-");
        let status = node
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let shard_count = node
            .get("shard_count")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or("-".to_string());
        let total_memory = node
            .get("total_memory")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let rack_id = node.get("rack_id").and_then(|v| v.as_str()).unwrap_or("-");

        builder.push_record([
            uid.as_str(),
            addr,
            &status_colored(status),
            &shard_count,
            &format_bytes(total_memory),
            rack_id,
        ]);
    }

    println!("{}", builder.build().with(Style::blank()));
    println!();
}

fn print_databases_table(databases: &Value) {
    let empty_vec = vec![];
    let databases_array = databases.as_array().unwrap_or(&empty_vec);
    println!("{}", "DATABASES".bold());
    let mut builder = Builder::default();
    builder.push_record([
        "UID",
        "Name",
        "Status",
        "Memory (used/limit)",
        "Shards",
        "Replication",
        "Endpoint",
    ]);

    for db in databases_array {
        let uid = db
            .get("uid")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or_default();
        let name = db.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let status = db
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let mem_used = db
            .get("memory_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let mem_limit = db
            .get("memory_limit")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let memory = format!("{} / {}", format_bytes(mem_used), format_bytes(mem_limit));
        let shard_count = db
            .get("shards_count")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or("-".to_string());
        let replication = db
            .get("replication")
            .and_then(|v| v.as_bool())
            .map(|v| if v { "Yes" } else { "No" })
            .unwrap_or("-");

        // Try common endpoint field patterns
        let endpoint = db
            .get("endpoints")
            .and_then(|v| v.as_array())
            .and_then(|eps| eps.first())
            .and_then(|ep| {
                let host = ep.get("dns_name").and_then(|v| v.as_str()).or_else(|| {
                    ep.get("addr")
                        .and_then(|addrs| addrs.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                });
                let port = ep.get("port").and_then(|v| v.as_u64());
                match (host, port) {
                    (Some(h), Some(p)) => Some(format!("{h}:{p}")),
                    (Some(h), None) => Some(h.to_string()),
                    _ => None,
                }
            })
            .unwrap_or_else(|| "-".to_string());

        builder.push_record([
            uid.as_str(),
            name,
            &status_colored(status),
            &memory,
            &shard_count,
            replication,
            &endpoint,
        ]);
    }

    println!("{}", builder.build().with(Style::blank()));
    println!();
}

fn print_shards_table(shards: &Value) {
    let empty_vec = vec![];
    let shards_array = shards.as_array().unwrap_or(&empty_vec);
    println!("{}", "SHARDS".bold());
    let mut builder = Builder::default();
    builder.push_record(["UID", "DB", "Node", "Role", "Status"]);

    for shard in shards_array {
        let uid = shard
            .get("uid")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .unwrap_or_default();
        let bdb_uid = shard
            .get("bdb_uid")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or("-".to_string());
        let node_uid = shard
            .get("node_uid")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .unwrap_or("-".to_string());
        let role = shard.get("role").and_then(|v| v.as_str()).unwrap_or("-");
        let status = shard
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        builder.push_record([
            uid.as_str(),
            &bdb_uid,
            &node_uid,
            role,
            &status_colored(status),
        ]);
    }

    println!("{}", builder.build().with(Style::blank()));
    println!();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return a colored string based on status value
fn status_colored(status: &str) -> String {
    match status.to_lowercase().as_str() {
        "active" | "ok" | "healthy" => status.green().to_string(),
        "degraded" | "pending" | "importing" | "recovery" => status.yellow().to_string(),
        "critical" | "failed" | "error" | "inactive" | "down" => status.red().to_string(),
        _ => status.to_string(),
    }
}

/// Format byte count as human-readable string
fn format_bytes(bytes: f64) -> String {
    if bytes <= 0.0 {
        return "-".to_string();
    }
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else {
        format!("{:.1} MB", bytes / MB)
    }
}

/// Print a colored one-line summary footer
fn print_summary_line(summary: &StatusSummary) {
    let health_label = match summary.cluster_health.as_str() {
        "healthy" => "HEALTHY".green().bold(),
        "degraded" => "DEGRADED".yellow().bold(),
        _ => "CRITICAL".red().bold(),
    };

    println!(
        "Status: {}  |  Nodes: {}/{}  |  Databases: {}/{}  |  Shards: {}",
        health_label,
        summary.healthy_nodes,
        summary.total_nodes,
        summary.active_databases,
        summary.total_databases,
        summary.total_shards,
    );
}

/// Calculate summary statistics from collected data
fn calculate_summary(nodes: &Value, databases: &Value, shards: &Value) -> StatusSummary {
    let empty_vec = vec![];
    let nodes_array = nodes.as_array().unwrap_or(&empty_vec);
    let databases_array = databases.as_array().unwrap_or(&empty_vec);
    let shards_array = shards.as_array().unwrap_or(&empty_vec);

    let total_nodes = nodes_array.len();
    let healthy_nodes = nodes_array
        .iter()
        .filter(|n| {
            n.get("status")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s == "active" || s == "ok")
        })
        .count();

    let total_databases = databases_array.len();
    let active_databases = databases_array
        .iter()
        .filter(|db| {
            db.get("status")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s == "active")
        })
        .count();

    let total_shards = shards_array.len();

    // Determine cluster health
    let cluster_health = if healthy_nodes == total_nodes && active_databases == total_databases {
        "healthy".to_string()
    } else if healthy_nodes == 0 || active_databases == 0 {
        "critical".to_string()
    } else {
        "degraded".to_string()
    };

    StatusSummary {
        total_nodes,
        healthy_nodes,
        total_databases,
        active_databases,
        total_shards,
        cluster_health,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0.0), "-");
        assert_eq!(format_bytes(-1.0), "-");
        assert_eq!(format_bytes(1_048_576.0), "1.0 MB");
        assert_eq!(format_bytes(536_870_912.0), "512.0 MB");
        assert_eq!(format_bytes(1_073_741_824.0), "1.0 GB");
        assert_eq!(format_bytes(2_684_354_560.0), "2.5 GB");
    }

    #[test]
    fn test_status_colored() {
        // Just verify these don't panic
        let _ = status_colored("active");
        let _ = status_colored("degraded");
        let _ = status_colored("critical");
        let _ = status_colored("something-else");
    }

    #[test]
    fn test_calculate_summary_healthy() {
        let nodes = json!([
            {"status": "active", "uid": 1},
            {"status": "active", "uid": 2},
        ]);
        let dbs = json!([
            {"status": "active", "uid": 1},
        ]);
        let shards = json!([
            {"uid": "1:1"},
            {"uid": "1:2"},
        ]);

        let summary = calculate_summary(&nodes, &dbs, &shards);
        assert_eq!(summary.cluster_health, "healthy");
        assert_eq!(summary.total_nodes, 2);
        assert_eq!(summary.healthy_nodes, 2);
        assert_eq!(summary.total_databases, 1);
        assert_eq!(summary.active_databases, 1);
        assert_eq!(summary.total_shards, 2);
    }

    #[test]
    fn test_calculate_summary_degraded() {
        let nodes = json!([
            {"status": "active", "uid": 1},
            {"status": "down", "uid": 2},
        ]);
        let dbs = json!([{"status": "active", "uid": 1}]);
        let shards = json!([]);

        let summary = calculate_summary(&nodes, &dbs, &shards);
        assert_eq!(summary.cluster_health, "degraded");
        assert_eq!(summary.healthy_nodes, 1);
    }

    #[test]
    fn test_calculate_summary_critical() {
        let nodes = json!([
            {"status": "down", "uid": 1},
        ]);
        let dbs = json!([{"status": "active", "uid": 1}]);
        let shards = json!([]);

        let summary = calculate_summary(&nodes, &dbs, &shards);
        assert_eq!(summary.cluster_health, "critical");
    }

    #[test]
    fn test_collect_warnings_memory() {
        let cluster = json!({});
        let nodes = json!([]);
        let dbs = json!([
            {"name": "db1", "memory_size": 910.0, "memory_limit": 1000.0},
            {"name": "db2", "memory_size": 800.0, "memory_limit": 1000.0},
            {"name": "db3", "memory_size": 500.0, "memory_limit": 1000.0},
        ]);

        let warnings = collect_warnings(&cluster, &nodes, &dbs);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("db1"));
        assert!(warnings[0].contains("critical"));
        assert!(warnings[1].contains("db2"));
    }

    #[test]
    fn test_collect_warnings_unhealthy_node() {
        let cluster = json!({});
        let nodes = json!([
            {"uid": 1, "status": "active"},
            {"uid": 2, "status": "down"},
        ]);
        let dbs = json!([]);

        let warnings = collect_warnings(&cluster, &nodes, &dbs);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Node 2"));
    }
}
