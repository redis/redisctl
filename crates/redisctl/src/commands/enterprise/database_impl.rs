//! Enterprise database command implementations

use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;
use tabled::{Table, Tabled, settings::Style};

use crate::cli::OutputFormat;
use crate::commands::cloud::async_utils::AsyncOperationArgs;
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};

use super::utils::*;

/// Database row for clean table display
#[derive(Tabled)]
struct DatabaseRow {
    #[tabled(rename = "UID")]
    uid: String,
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "MEMORY")]
    memory: String,
    #[tabled(rename = "SHARDS")]
    shards: String,
    #[tabled(rename = "REPL")]
    replication: String,
    #[tabled(rename = "ENDPOINT")]
    endpoint: String,
    #[tabled(rename = "PERSIST")]
    persistence: String,
}

/// Extract endpoint from database JSON
fn extract_endpoint(db: &Value) -> String {
    // Try endpoints[0].dns_name:port or endpoints[0].addr[0]:port
    if let Some(endpoints) = db.get("endpoints").and_then(|v| v.as_array())
        && let Some(ep) = endpoints.first()
    {
        let host = ep
            .get("dns_name")
            .and_then(|v| v.as_str())
            .or_else(|| {
                ep.get("addr")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("");
        let port = ep
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p.to_string())
            .unwrap_or_default();
        if !host.is_empty() && !port.is_empty() {
            return format!("{}:{}", host, port);
        } else if !host.is_empty() {
            return host.to_string();
        }
    }
    // Fallback: try top-level port
    if let Some(port) = db.get("port").and_then(|v| v.as_u64()) {
        return format!(":{}", port);
    }
    "-".to_string()
}

/// Print databases in clean table format
fn print_databases_table(data: &Value) -> CliResult<()> {
    let databases = match data {
        Value::Array(arr) => arr.clone(),
        _ => {
            println!("No databases found");
            return Ok(());
        }
    };

    if databases.is_empty() {
        println!("No databases found");
        return Ok(());
    }

    let mut rows = Vec::new();
    for db in &databases {
        let memory_size = db
            .get("memory_size")
            .and_then(|v| v.as_u64())
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());

        rows.push(DatabaseRow {
            uid: extract_field(db, "uid", "-"),
            name: truncate_string(&extract_field(db, "name", "-"), 25),
            status: format_status(extract_field(db, "status", "unknown")),
            memory: memory_size,
            shards: extract_field(db, "shards_count", "-"),
            replication: if db
                .get("replication")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                "yes".to_string()
            } else {
                "no".to_string()
            },
            endpoint: truncate_string(&extract_endpoint(db), 30),
            persistence: extract_field(db, "data_persistence", "-"),
        });
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    output_with_pager(&table.to_string());
    Ok(())
}

/// Print database detail in key-value format
fn print_database_detail(data: &Value) -> CliResult<()> {
    let mut rows = Vec::new();

    let fields = [
        ("UID", "uid"),
        ("Name", "name"),
        ("Status", "status"),
        ("Type", "type"),
        ("Port", "port"),
        ("Replication", "replication"),
        ("Data Persistence", "data_persistence"),
        ("Eviction Policy", "eviction_policy"),
        ("Shards Count", "shards_count"),
        ("Shards Placement", "shards_placement"),
        ("Proxy Policy", "proxy_policy"),
        ("OSS Cluster", "oss_cluster"),
        ("Version", "version"),
        ("Created Time", "created_time"),
        ("Last Changed Time", "last_changed_time"),
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

    // Memory size with formatting
    if let Some(mem) = data.get("memory_size").and_then(|v| v.as_u64()) {
        rows.push(DetailRow {
            field: "Memory Size".to_string(),
            value: format_bytes(mem),
        });
    }

    // Used memory if available
    if let Some(used) = data.get("used_memory").and_then(|v| v.as_u64()) {
        rows.push(DetailRow {
            field: "Used Memory".to_string(),
            value: format_bytes(used),
        });
    }

    // Endpoint
    let endpoint = extract_endpoint(data);
    if endpoint != "-" {
        rows.push(DetailRow {
            field: "Endpoint".to_string(),
            value: endpoint,
        });
    }

    if rows.is_empty() {
        println!("No database information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    output_with_pager(&table.to_string());
    Ok(())
}

/// Parse a module spec string into (name, version, args).
/// Format: `name[@version][:args]`
fn parse_module_spec(spec: &str) -> (&str, Option<&str>, Option<&str>) {
    let (name_and_version, module_args) = match spec.find(':') {
        Some(idx) => {
            let (nv, args) = spec.split_at(idx);
            (nv.trim(), Some(args[1..].trim()))
        }
        None => (spec.trim(), None),
    };
    let (module_name, module_version) = match name_and_version.find('@') {
        Some(idx) => {
            let (name, ver) = name_and_version.split_at(idx);
            (name.trim(), Some(ver[1..].trim()))
        }
        None => (name_and_version, None),
    };
    (module_name, module_version, module_args)
}

/// List all databases
pub async fn list_databases(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw("/v1/bdbs")
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    if matches!(resolve_auto(output_format), OutputFormat::Table) {
        print_databases_table(&data)?;
    } else {
        print_formatted_output(data, output_format)?;
    }
    Ok(())
}

/// Get database details
pub async fn get_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/bdbs/{}", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    if matches!(resolve_auto(output_format), OutputFormat::Table) {
        print_database_detail(&data)?;
    } else {
        print_formatted_output(data, output_format)?;
    }
    Ok(())
}

/// Create a new database
#[allow(clippy::too_many_arguments)]
pub async fn create_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    name: Option<&str>,
    memory: Option<u64>,
    port: Option<u16>,
    replication: bool,
    persistence: Option<&str>,
    eviction_policy: Option<&str>,
    sharding: bool,
    shards_count: Option<u32>,
    proxy_policy: Option<&str>,
    crdb: bool,
    redis_password: Option<&str>,
    modules: &[String],
    data: Option<&str>,
    dry_run: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(name_val) = name {
        request_obj.insert("name".to_string(), serde_json::json!(name_val));
    } else if data.is_none() {
        return Err(RedisCtlError::InvalidInput {
            message: "--name is required (unless using --data with complete configuration)"
                .to_string(),
        });
    }

    // Memory is highly recommended but not strictly required
    if let Some(mem) = memory {
        request_obj.insert("memory_size".to_string(), serde_json::json!(mem));
    }

    if let Some(p) = port {
        request_obj.insert("port".to_string(), serde_json::json!(p));
    }

    // Only set replication if true (false is default)
    if replication {
        request_obj.insert("replication".to_string(), serde_json::json!(true));
    }

    if let Some(persist) = persistence {
        request_obj.insert("persistence".to_string(), serde_json::json!(persist));
    }

    if let Some(evict) = eviction_policy {
        request_obj.insert("eviction_policy".to_string(), serde_json::json!(evict));
    }

    // Only set sharding if true (false is default)
    if sharding {
        request_obj.insert("sharding".to_string(), serde_json::json!(true));
    }

    if let Some(count) = shards_count {
        if !sharding
            && !request_obj
                .get("sharding")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            return Err(RedisCtlError::InvalidInput {
                message: "--shards-count requires --sharding to be enabled".to_string(),
            });
        }
        request_obj.insert("shards_count".to_string(), serde_json::json!(count));
    }

    if let Some(policy) = proxy_policy {
        request_obj.insert("proxy_policy".to_string(), serde_json::json!(policy));
    }

    // Only set crdb if true (false is default)
    if crdb {
        request_obj.insert("crdt".to_string(), serde_json::json!(true));
    }

    if let Some(password) = redis_password {
        request_obj.insert(
            "authentication_redis_pass".to_string(),
            serde_json::json!(password),
        );
    }

    // Handle module resolution if --module flags were provided
    if !modules.is_empty() {
        let module_handler = redis_enterprise::ModuleHandler::new(
            conn_mgr.create_enterprise_client(profile_name).await?,
        );
        let available_modules = module_handler.list().await.map_err(RedisCtlError::from)?;

        let mut module_list: Vec<Value> = Vec::new();

        for module_spec in modules {
            let (module_name, module_version, module_args) = parse_module_spec(module_spec);

            // Find matching module (case-insensitive)
            let matching: Vec<_> = available_modules
                .iter()
                .filter(|m| {
                    m.module_name
                        .as_ref()
                        .map(|n| n.eq_ignore_ascii_case(module_name))
                        .unwrap_or(false)
                })
                .collect();

            // If a version was specified, filter by exact version match
            let matching: Vec<_> = if let Some(version) = module_version {
                matching
                    .into_iter()
                    .filter(|m| {
                        m.semantic_version
                            .as_ref()
                            .map(|v| v == version)
                            .unwrap_or(false)
                    })
                    .collect()
            } else {
                matching
            };

            match matching.len() {
                0 => {
                    // No exact match - try partial match and suggest
                    let partial_matches: Vec<_> = available_modules
                        .iter()
                        .filter(|m| {
                            m.module_name
                                .as_ref()
                                .map(|n| n.to_lowercase().contains(&module_name.to_lowercase()))
                                .unwrap_or(false)
                        })
                        .collect();

                    if partial_matches.is_empty() {
                        return Err(RedisCtlError::InvalidInput {
                            message: format!(
                                "Module '{}' not found. Use 'enterprise module list' to see available modules.",
                                module_name
                            ),
                        });
                    } else {
                        let suggestions: Vec<_> = partial_matches
                            .iter()
                            .filter_map(|m| m.module_name.as_deref())
                            .collect();
                        return Err(RedisCtlError::InvalidInput {
                            message: format!(
                                "Module '{}' not found. Did you mean one of: {}?",
                                module_name,
                                suggestions.join(", ")
                            ),
                        });
                    }
                }
                1 => {
                    // Build module config using the actual module name from the API
                    let actual_name = matching[0].module_name.as_deref().unwrap_or(module_name);
                    let mut module_config = serde_json::json!({
                        "module_name": actual_name
                    });
                    if let Some(args) = module_args {
                        module_config["module_args"] = serde_json::json!(args);
                    }
                    module_list.push(module_config);
                }
                _ => {
                    // Multiple matches - show versions and suggest @version syntax
                    let versions: Vec<_> = matching
                        .iter()
                        .map(|m| {
                            format!(
                                "{}@{}",
                                m.module_name.as_deref().unwrap_or("unknown"),
                                m.semantic_version.as_deref().unwrap_or("unknown")
                            )
                        })
                        .collect();
                    return Err(RedisCtlError::InvalidInput {
                        message: format!(
                            "Multiple modules found matching '{}'. Specify a version with '{}@<version>':\n  {}",
                            module_name,
                            module_name,
                            versions.join("\n  ")
                        ),
                    });
                }
            }
        }

        // Add module_list to request (CLI modules override --data modules)
        request_obj.insert("module_list".to_string(), serde_json::json!(module_list));
    }

    let path = if dry_run {
        "/v1/bdbs?dry_run"
    } else {
        "/v1/bdbs"
    };

    let response = client
        .post_raw(path, request)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Update database configuration
#[allow(clippy::too_many_arguments)]
pub async fn update_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    name: Option<&str>,
    memory: Option<u64>,
    replication: Option<bool>,
    persistence: Option<&str>,
    eviction_policy: Option<&str>,
    shards_count: Option<u32>,
    proxy_policy: Option<&str>,
    redis_password: Option<&str>,
    data: Option<&str>,
    dry_run: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(name_val) = name {
        request_obj.insert("name".to_string(), serde_json::json!(name_val));
    }

    if let Some(mem) = memory {
        request_obj.insert("memory_size".to_string(), serde_json::json!(mem));
    }

    if let Some(repl) = replication {
        request_obj.insert("replication".to_string(), serde_json::json!(repl));
    }

    if let Some(persist) = persistence {
        request_obj.insert("data_persistence".to_string(), serde_json::json!(persist));
    }

    if let Some(eviction) = eviction_policy {
        request_obj.insert("eviction_policy".to_string(), serde_json::json!(eviction));
    }

    if let Some(shards) = shards_count {
        request_obj.insert("shards_count".to_string(), serde_json::json!(shards));
    }

    if let Some(proxy) = proxy_policy {
        request_obj.insert("proxy_policy".to_string(), serde_json::json!(proxy));
    }

    if let Some(password) = redis_password {
        request_obj.insert(
            "authentication_redis_pass".to_string(),
            serde_json::json!(password),
        );
    }

    // Validate that we have at least one field to update
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--name, --memory, --replication, --persistence, --eviction-policy, --shards-count, --proxy-policy, --redis-password, or --data)".to_string(),
        });
    }

    if dry_run {
        eprintln!("Would update database {} with:", id);
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&request).unwrap_or_default()
        );
        eprintln!();
        eprintln!("No changes were made.");
        return Ok(());
    }

    let response = client
        .put_raw(&format!("/v1/bdbs/{}", id), request)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Delete a database
pub async fn delete_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    force: bool,
    dry_run: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    if dry_run {
        eprintln!("Would delete database {}.", id);
        eprintln!();
        eprintln!("No changes were made.");
        return Ok(());
    }

    if !force && !confirm_action(&format!("Delete database {}?", id))? {
        println!("Operation cancelled");
        return Ok(());
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .delete_raw(&format!("/v1/bdbs/{}", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Watch database status changes
pub async fn watch_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    poll_interval: u64,
    query: Option<&str>,
) -> CliResult<()> {
    use futures::StreamExt;
    use tokio::signal;

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = redis_enterprise::BdbHandler::new(client);
    let mut stream = handler.watch_database(id, std::time::Duration::from_secs(poll_interval));

    println!("Watching database {} (Ctrl+C to stop)...\n", id);

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                println!("\nStopping database watch...");
                break;
            }
            result = stream.next() => {
                match result {
                    Some(Ok((db_info, prev_status))) => {
                        let current_status = db_info.status.as_deref().unwrap_or("unknown");
                        let timestamp = chrono::Utc::now().format("%H:%M:%S");

                        // Check if this is a status change
                        if let Some(old_status) = prev_status {
                            // Status transition detected
                            println!(
                                "[{}] Database {}: {} -> {} (TRANSITION)",
                                timestamp, id, old_status, current_status
                            );

                            // Show key metrics during transition
                            if let Some(memory_used) = db_info.memory_used
                                && let Some(memory_size) = db_info.memory_size {
                                    let usage_pct = (memory_used as f64 / memory_size as f64) * 100.0;
                                    println!("  Memory: {} / {} ({:.1}%)",
                                        format_bytes(memory_used),
                                        format_bytes(memory_size),
                                        usage_pct
                                    );
                                }

                            if let Some(shards) = db_info.shards_count {
                                println!("  Shards: {}", shards);
                            }
                        } else {
                            // Regular status update (no change)
                            print!("[{}] Database {}: {}", timestamp, id, current_status);

                            // Apply JMESPath query if provided
                            if query.is_some() {
                                let db_json = serde_json::to_value(&db_info)
                                    .map_err(|e| RedisCtlError::from(anyhow::anyhow!("Serialization error: {}", e)))?;
                                let filtered = handle_output(db_json, OutputFormat::Json, query)?;
                                print!(" | {}", serde_json::to_string(&filtered)?);
                            } else {
                                // Show brief metrics
                                if let Some(memory_used) = db_info.memory_used {
                                    print!(" | mem: {}", format_bytes(memory_used));
                                }
                                if let Some(shards) = db_info.shards_count {
                                    print!(" | shards: {}", shards);
                                }
                            }
                            println!();
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("Error watching database: {}", e);
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Format bytes into human-readable format
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2}{}", size, UNITS[unit_index])
}

/// Export database
#[allow(clippy::too_many_arguments)]
pub async fn export_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    location: Option<&str>,
    aws_access_key: Option<&str>,
    aws_secret_key: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(loc) = location {
        request_obj.insert("export_location".to_string(), serde_json::json!(loc));
    }
    if let Some(key) = aws_access_key {
        request_obj.insert("aws_access_key_id".to_string(), serde_json::json!(key));
    }
    if let Some(secret) = aws_secret_key {
        request_obj.insert(
            "aws_secret_access_key".to_string(),
            serde_json::json!(secret),
        );
    }

    // Validate required fields
    if !request_obj.contains_key("export_location") {
        return Err(RedisCtlError::InvalidInput {
            message: "--location is required (unless using --data with export_location)"
                .to_string(),
        });
    }

    let response = client
        .post_raw(&format!("/v1/bdbs/{}/export", id), request)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Import to database
#[allow(clippy::too_many_arguments)]
pub async fn import_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    location: Option<&str>,
    aws_access_key: Option<&str>,
    aws_secret_key: Option<&str>,
    flush: bool,
    data: Option<&str>,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Determine the import location - from --location or --data
    let import_location = if let Some(loc) = location {
        loc.to_string()
    } else if let Some(data_str) = data {
        let json = read_json_data(data_str)?;
        json.get("import_location")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| RedisCtlError::InvalidInput {
                message: "--location is required (unless using --data with import_location)"
                    .to_string(),
            })?
    } else {
        return Err(RedisCtlError::InvalidInput {
            message: "--location is required (unless using --data with import_location)"
                .to_string(),
        });
    };

    // Note: AWS credentials via --data are not currently supported by Layer 2 workflow
    // If AWS credentials are provided, we need to warn or use legacy path
    if aws_access_key.is_some() || aws_secret_key.is_some() {
        // Use legacy path with full JSON support for AWS credentials
        let mut request = if let Some(data_str) = data {
            read_json_data(data_str)?
        } else {
            serde_json::json!({})
        };

        let request_obj = request.as_object_mut().unwrap();
        request_obj.insert(
            "import_location".to_string(),
            serde_json::json!(import_location),
        );
        if let Some(key) = aws_access_key {
            request_obj.insert("aws_access_key_id".to_string(), serde_json::json!(key));
        }
        if let Some(secret) = aws_secret_key {
            request_obj.insert(
                "aws_secret_access_key".to_string(),
                serde_json::json!(secret),
            );
        }
        if flush {
            request_obj.insert("flush".to_string(), serde_json::json!(true));
        }

        let response = client
            .post_raw(&format!("/v1/bdbs/{}/import", id), request)
            .await
            .map_err(RedisCtlError::from)?;

        let data = handle_output(response, output_format, query)?;
        print_formatted_output(data, output_format)?;

        if async_ops.wait {
            eprintln!(
                "Note: --wait with AWS credentials requires manual polling. Check action status."
            );
        }
        return Ok(());
    }

    if async_ops.wait {
        // Use Layer 2 workflow with progress reporting
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg} [{elapsed_precise}]")
                .unwrap(),
        );
        pb.set_message(format!("Importing data to database {}", id));

        let timeout = Duration::from_secs(async_ops.wait_timeout);
        let progress_callback = {
            let pb = pb.clone();
            Some(Box::new(
                move |event: redisctl_core::enterprise::EnterpriseProgressEvent| match &event {
                    redisctl_core::enterprise::EnterpriseProgressEvent::Started { action_uid } => {
                        pb.set_message(format!("Import started: {}", action_uid));
                    }
                    redisctl_core::enterprise::EnterpriseProgressEvent::Polling {
                        status,
                        progress,
                        ..
                    } => {
                        if let Some(pct) = progress {
                            pb.set_message(format!("Import {}: {}%", status, pct));
                        } else {
                            pb.set_message(format!("Import status: {}", status));
                        }
                    }
                    redisctl_core::enterprise::EnterpriseProgressEvent::Completed { .. } => {
                        pb.finish_with_message("Import completed");
                    }
                    redisctl_core::enterprise::EnterpriseProgressEvent::Failed {
                        error, ..
                    } => {
                        pb.finish_with_message(format!("Import failed: {}", error));
                    }
                },
            )
                as redisctl_core::enterprise::EnterpriseProgressCallback)
        };

        redisctl_core::enterprise::import_database_and_wait(
            &client,
            id,
            &import_location,
            flush,
            timeout,
            progress_callback,
        )
        .await
        .map_err(RedisCtlError::from)?;

        match output_format {
            OutputFormat::Auto | OutputFormat::Table => {
                println!("Database {} import completed successfully", id);
            }
            OutputFormat::Json | OutputFormat::Yaml => {
                let result = serde_json::json!({
                    "status": "completed",
                    "database_id": id,
                    "import_location": import_location,
                    "message": "Import completed successfully"
                });
                print_formatted_output(result, output_format)?;
            }
        }
    } else {
        // Original behavior: trigger import and return immediately
        let mut request = if let Some(data_str) = data {
            read_json_data(data_str)?
        } else {
            serde_json::json!({})
        };

        let request_obj = request.as_object_mut().unwrap();
        request_obj.insert(
            "import_location".to_string(),
            serde_json::json!(import_location),
        );
        if flush {
            request_obj.insert("flush".to_string(), serde_json::json!(true));
        }

        let response = client
            .post_raw(&format!("/v1/bdbs/{}/import", id), request)
            .await
            .map_err(RedisCtlError::from)?;

        let data = handle_output(response, output_format, query)?;
        print_formatted_output(data, output_format)?;
    }

    Ok(())
}

/// Trigger database backup
pub async fn backup_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    async_ops: &AsyncOperationArgs,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    if async_ops.wait {
        // Use Layer 2 workflow with progress reporting
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg} [{elapsed_precise}]")
                .unwrap(),
        );
        pb.set_message(format!("Backing up database {}", id));

        let timeout = Duration::from_secs(async_ops.wait_timeout);
        let progress_callback = {
            let pb = pb.clone();
            Some(Box::new(
                move |event: redisctl_core::enterprise::EnterpriseProgressEvent| match &event {
                    redisctl_core::enterprise::EnterpriseProgressEvent::Started { action_uid } => {
                        pb.set_message(format!("Backup started: {}", action_uid));
                    }
                    redisctl_core::enterprise::EnterpriseProgressEvent::Polling {
                        status,
                        progress,
                        ..
                    } => {
                        if let Some(pct) = progress {
                            pb.set_message(format!("Backup {}: {}%", status, pct));
                        } else {
                            pb.set_message(format!("Backup status: {}", status));
                        }
                    }
                    redisctl_core::enterprise::EnterpriseProgressEvent::Completed { .. } => {
                        pb.finish_with_message("Backup completed");
                    }
                    redisctl_core::enterprise::EnterpriseProgressEvent::Failed {
                        error, ..
                    } => {
                        pb.finish_with_message(format!("Backup failed: {}", error));
                    }
                },
            )
                as redisctl_core::enterprise::EnterpriseProgressCallback)
        };

        redisctl_core::enterprise::backup_database_and_wait(
            &client,
            id,
            timeout,
            progress_callback,
        )
        .await
        .map_err(RedisCtlError::from)?;

        match output_format {
            OutputFormat::Auto | OutputFormat::Table => {
                println!("Database {} backup completed successfully", id);
            }
            OutputFormat::Json => {
                let result = serde_json::json!({
                    "status": "completed",
                    "database_id": id,
                    "message": "Backup completed successfully"
                });
                print_formatted_output(result, output_format)?;
            }
            OutputFormat::Yaml => {
                let result = serde_json::json!({
                    "status": "completed",
                    "database_id": id,
                    "message": "Backup completed successfully"
                });
                print_formatted_output(result, output_format)?;
            }
        }
    } else {
        // Original behavior: trigger backup and return immediately
        let response = client
            .post_raw(&format!("/v1/bdbs/{}/backup", id), Value::Null)
            .await
            .map_err(RedisCtlError::from)?;

        let data = handle_output(response, output_format, query)?;
        print_formatted_output(data, output_format)?;
    }

    Ok(())
}

/// Restore database
pub async fn restore_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    backup_uid: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(uid) = backup_uid {
        request_obj.insert("backup_uid".to_string(), serde_json::json!(uid));
    }

    let response = client
        .post_raw(&format!("/v1/bdbs/{}/restore", id), request)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Flush database data
pub async fn flush_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    if !force
        && !confirm_action(&format!(
            "Flush all data from database {}? This will delete all data!",
            id
        ))?
    {
        println!("Operation cancelled");
        return Ok(());
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .put_raw(&format!("/v1/bdbs/{}/flush", id), Value::Null)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get database shards
pub async fn get_database_shards(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/bdbs/{}/shards", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Update database shards
#[allow(clippy::too_many_arguments)]
pub async fn update_database_shards(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    shards_count: Option<u32>,
    shards_placement: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(count) = shards_count {
        request_obj.insert("shards_count".to_string(), serde_json::json!(count));
    }
    if let Some(placement) = shards_placement {
        request_obj.insert("shards_placement".to_string(), serde_json::json!(placement));
    }

    // Validate at least one field is provided
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--shards-count, --shards-placement, or --data)".to_string(),
        });
    }

    let response = client
        .put_raw(&format!("/v1/bdbs/{}/shards", id), request)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get database modules
///
/// Fetches the database object and extracts `module_list`, since the
/// `/v1/bdbs/{id}/modules` endpoint is not supported on all clusters.
pub async fn get_database_modules(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response: serde_json::Value = client
        .get_raw(&format!("/v1/bdbs/{}", id))
        .await
        .map_err(RedisCtlError::from)?;

    let modules = response
        .get("module_list")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));

    let data = handle_output(modules, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Update database modules
#[allow(clippy::too_many_arguments)]
pub async fn update_database_modules(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    add_modules: &[String],
    remove_modules: &[String],
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // Handle module additions
    if !add_modules.is_empty() {
        let mut module_list: Vec<serde_json::Value> = Vec::new();

        for module_spec in add_modules {
            let (module_name, _module_version, module_args) = parse_module_spec(module_spec);

            let mut module_config = serde_json::json!({
                "module_name": module_name
            });
            if let Some(args) = module_args {
                module_config["module_args"] = serde_json::json!(args);
            }
            module_list.push(module_config);
        }

        request_obj.insert("module_list".to_string(), serde_json::json!(module_list));
    }

    // Handle module removals
    if !remove_modules.is_empty() {
        request_obj.insert(
            "remove_modules".to_string(),
            serde_json::json!(remove_modules),
        );
    }

    // Validate at least one operation is provided
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message:
                "At least one operation is required (--add-module, --remove-module, or --data)"
                    .to_string(),
        });
    }

    let response = client
        .put_raw(&format!("/v1/bdbs/{}/modules", id), request)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get database ACL
pub async fn get_database_acl(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/bdbs/{}/acl", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Update database ACL
#[allow(clippy::too_many_arguments)]
pub async fn update_database_acl(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    acl_uid: Option<u32>,
    default_user: Option<bool>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut request = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let request_obj = request.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(uid) = acl_uid {
        request_obj.insert("acl_uid".to_string(), serde_json::json!(uid));
    }
    if let Some(default) = default_user {
        request_obj.insert("default_user".to_string(), serde_json::json!(default));
    }

    // Validate at least one field is provided
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--acl-uid, --default-user, or --data)"
                .to_string(),
        });
    }

    let response = client
        .put_raw(&format!("/v1/bdbs/{}/acl", id), request)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get database statistics
pub async fn get_database_stats(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/bdbs/{}/stats", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get database metrics
pub async fn get_database_metrics(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    interval: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let mut path = format!("/v1/bdbs/{}/metrics", id);
    if let Some(interval) = interval {
        path.push_str(&format!("?interval={}", interval));
    }

    let response = client.get_raw(&path).await.map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get database slowlog
pub async fn get_database_slowlog(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    limit: Option<u32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let mut path = format!("/v1/bdbs/{}/slowlog", id);
    if let Some(limit) = limit {
        path.push_str(&format!("?limit={}", limit));
    }

    let response = client.get_raw(&path).await.map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get connected clients
pub async fn get_database_clients(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/bdbs/{}/clients", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Upgrade database Redis version
#[allow(clippy::too_many_arguments)]
pub async fn upgrade_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    version: Option<&str>,
    preserve_roles: bool,
    force_restart: bool,
    may_discard_data: bool,
    force_discard: bool,
    keep_crdt_protocol_version: bool,
    parallel_shards_upgrade: Option<u32>,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    use redis_enterprise::bdb::{DatabaseHandler, DatabaseInfo, DatabaseUpgradeRequest};

    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Get current database info
    let db_handler = DatabaseHandler::new(client);
    let db: DatabaseInfo = db_handler.get(id).await?;
    let current_version = db.redis_version.as_deref().unwrap_or("unknown");

    // Determine target version
    let target_version = if let Some(v) = version {
        v.to_string()
    } else {
        // Get latest version from cluster - for now just use current
        // TODO: Get from cluster info when we add that endpoint
        current_version.to_string()
    };

    // Safety checks unless --force
    if !force {
        // Check if database is active
        if db.status.as_deref() != Some("active") {
            return Err(RedisCtlError::InvalidInput {
                message: format!(
                    "Database is not active (status: {}). Use --force to upgrade anyway.",
                    db.status.as_deref().unwrap_or("unknown")
                ),
            });
        }

        // Warn about persistence (check if persistence is disabled/none)
        let has_persistence = db
            .persistence
            .as_deref()
            .map(|p| p != "disabled")
            .unwrap_or(false);
        if !has_persistence && !may_discard_data {
            eprintln!("Warning: Database has no persistence enabled.");
            eprintln!("If upgrade fails, data may be lost.");
            eprintln!("Use --may-discard-data to proceed.");
            return Err(RedisCtlError::InvalidInput {
                message: "Upgrade cancelled for safety".to_string(),
            });
        }

        // Warn about replication (check if replication is enabled)
        let has_replication = db.replication.unwrap_or(false);
        if !has_replication {
            eprintln!("Warning: Database has no replication enabled.");
            eprintln!("Upgrade will cause downtime.");
            eprintln!("Use --force to proceed.");
            return Err(RedisCtlError::InvalidInput {
                message: "Upgrade cancelled for safety".to_string(),
            });
        }
    }

    // Display upgrade info
    if matches!(output_format, OutputFormat::Table | OutputFormat::Auto) {
        println!("Upgrading database '{}' (db:{})...", db.name, id);
        println!("  Current version: {}", current_version);
        println!("  Target version: {}", target_version);
    }

    // Build upgrade request
    let request = DatabaseUpgradeRequest {
        redis_version: Some(target_version.clone()),
        preserve_roles: Some(preserve_roles),
        force_restart: Some(force_restart),
        may_discard_data: Some(may_discard_data),
        force_discard: Some(force_discard),
        keep_crdt_protocol_version: Some(keep_crdt_protocol_version),
        parallel_shards_upgrade,
        modules: None,
    };

    // Call upgrade API
    let response = db_handler.upgrade_redis_version(id, request).await?;

    // Handle output
    match output_format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "database_id": id,
                "database_name": db.name,
                "old_version": current_version,
                "new_version": target_version,
                "action_uid": response.action_uid,
                "status": "upgrade_initiated"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Table | OutputFormat::Auto => {
            println!("Upgrade initiated (action_uid: {})", response.action_uid);
            println!(
                "Use 'redisctl enterprise database get {}' to check status",
                id
            );
        }
        _ => {
            let data = serde_json::to_value(&response)?;
            let filtered = handle_output(data, output_format, query)?;
            print_formatted_output(filtered, output_format)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_module_spec_name_only() {
        let (name, version, args) = parse_module_spec("search");
        assert_eq!(name, "search");
        assert_eq!(version, None);
        assert_eq!(args, None);
    }

    #[test]
    fn test_parse_module_spec_name_with_version() {
        let (name, version, args) = parse_module_spec("search@2.10.27");
        assert_eq!(name, "search");
        assert_eq!(version, Some("2.10.27"));
        assert_eq!(args, None);
    }

    #[test]
    fn test_parse_module_spec_name_with_args() {
        let (name, version, args) = parse_module_spec("search:ARGS");
        assert_eq!(name, "search");
        assert_eq!(version, None);
        assert_eq!(args, Some("ARGS"));
    }

    #[test]
    fn test_parse_module_spec_name_with_version_and_args() {
        let (name, version, args) = parse_module_spec("search@2.10.27:PARTITIONS=AUTO");
        assert_eq!(name, "search");
        assert_eq!(version, Some("2.10.27"));
        assert_eq!(args, Some("PARTITIONS=AUTO"));
    }
}
