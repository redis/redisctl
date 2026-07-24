//! Cloud database command implementations

use super::utils::DetailRow;
use super::utils::*;
use crate::cli::{CloudDatabaseCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use crate::output::print_output;
use anyhow::Context;
use serde_json::Value;
use tabled::{Table, Tabled, settings::Style};

/// Database row for clean table display
#[derive(Tabled)]
struct DatabaseRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "SUBSCRIPTION")]
    subscription: String,
    #[tabled(rename = "MEMORY")]
    memory: String,
    #[tabled(rename = "REGION")]
    region: String,
    #[tabled(rename = "ENDPOINT")]
    endpoint: String,
    #[tabled(rename = "CREATED")]
    created: String,
}

/// Handle cloud database commands
pub async fn handle_database_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudDatabaseCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        CloudDatabaseCommands::List { subscription } => {
            list_databases(conn_mgr, profile_name, *subscription, output_format, query).await
        }
        CloudDatabaseCommands::Get { id } => {
            get_database(conn_mgr, profile_name, id, output_format, query).await
        }
        CloudDatabaseCommands::Create {
            subscription,
            name,
            memory,
            dataset_size,
            protocol,
            replication,
            data_persistence,
            eviction_policy,
            redis_version,
            oss_cluster,
            port,
            data,
            dry_run,
            async_ops,
        } => {
            super::database_impl::create_database(
                conn_mgr,
                profile_name,
                *subscription,
                name.as_deref(),
                *memory,
                *dataset_size,
                protocol,
                *replication,
                data_persistence.as_deref(),
                eviction_policy,
                redis_version.as_deref(),
                *oss_cluster,
                *port,
                data.as_deref(),
                *dry_run,
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::Update {
            id,
            name,
            memory,
            replication,
            data_persistence,
            eviction_policy,
            oss_cluster,
            regex_rules,
            data,
            dry_run,
            async_ops,
        } => {
            super::database_impl::update_database(
                conn_mgr,
                profile_name,
                id,
                name.as_deref(),
                *memory,
                *replication,
                data_persistence.as_deref(),
                eviction_policy.as_deref(),
                *oss_cluster,
                regex_rules.as_deref(),
                data.as_deref(),
                *dry_run,
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::Delete {
            id,
            force,
            dry_run,
            async_ops,
        } => {
            super::database_impl::delete_database(
                conn_mgr,
                profile_name,
                id,
                *force,
                *dry_run,
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::BackupStatus { id } => {
            super::database_impl::get_backup_status(
                conn_mgr,
                profile_name,
                id,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::Backup { id, async_ops } => {
            super::database_impl::backup_database(
                conn_mgr,
                profile_name,
                id,
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::ImportStatus { id } => {
            super::database_impl::get_import_status(
                conn_mgr,
                profile_name,
                id,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::Import {
            id,
            source_type,
            import_from_uri,
            aws_access_key,
            aws_secret_key,
            gcs_client_email,
            gcs_private_key,
            azure_account_name,
            azure_account_key,
            data,
            async_ops,
        } => {
            super::database_impl::import_database(
                conn_mgr,
                profile_name,
                id,
                source_type.as_deref(),
                import_from_uri.as_deref(),
                aws_access_key.as_deref(),
                aws_secret_key.as_deref(),
                gcs_client_email.as_deref(),
                gcs_private_key.as_deref(),
                azure_account_name.as_deref(),
                azure_account_key.as_deref(),
                data.as_deref(),
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::GetCertificate { id } => {
            super::database_impl::get_certificate(conn_mgr, profile_name, id, output_format, query)
                .await
        }
        CloudDatabaseCommands::SlowLog { id, limit, offset } => {
            super::database_impl::get_slow_log(
                conn_mgr,
                profile_name,
                id,
                *limit,
                *offset,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::ListTags { id } => {
            super::database_impl::list_tags(conn_mgr, profile_name, id, output_format, query).await
        }
        CloudDatabaseCommands::AddTag { id, key, value } => {
            super::database_impl::add_tag(
                conn_mgr,
                profile_name,
                id,
                key,
                value,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::UpdateTags { id, tags, data } => {
            super::database_impl::update_tags(
                conn_mgr,
                profile_name,
                id,
                tags,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::UpdateTag { id, key, value } => {
            super::database_impl::update_tag(
                conn_mgr,
                profile_name,
                id,
                key,
                value,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::DeleteTag { id, key } => {
            super::database_impl::delete_tag(conn_mgr, profile_name, id, key, output_format, query)
                .await
        }
        CloudDatabaseCommands::Flush { id, force } => {
            super::database_impl::flush_database(
                conn_mgr,
                profile_name,
                id,
                *force,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::FlushCrdb { id, force } => {
            super::database_impl::flush_crdb(
                conn_mgr,
                profile_name,
                id,
                *force,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::UpdateAaRegions {
            id,
            name,
            memory,
            dataset_size,
            global_data_persistence,
            global_password,
            eviction_policy,
            enable_tls,
            oss_cluster,
            dry_run,
            data,
            async_ops,
        } => {
            super::database_impl::update_aa_regions(
                conn_mgr,
                profile_name,
                id,
                name.as_deref(),
                *memory,
                *dataset_size,
                global_data_persistence.as_deref(),
                global_password.as_deref(),
                eviction_policy.as_deref(),
                *enable_tls,
                *oss_cluster,
                *dry_run,
                data.as_deref(),
                async_ops,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::AvailableVersions { id } => {
            super::database_impl::get_available_versions(
                conn_mgr,
                profile_name,
                id,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::UpgradeStatus { id } => {
            super::database_impl::get_upgrade_status(
                conn_mgr,
                profile_name,
                id,
                output_format,
                query,
            )
            .await
        }
        CloudDatabaseCommands::UpgradeRedis { id, version } => {
            super::database_impl::upgrade_redis(
                conn_mgr,
                profile_name,
                id,
                version,
                output_format,
                query,
            )
            .await
        }
    }
}

/// List all databases
async fn list_databases(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    subscription_id: Option<u32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Fetch both flexible and fixed subscriptions
    let flex_response = client
        .get_raw("/subscriptions")
        .await
        .context("Failed to fetch flexible subscriptions")?;

    let fixed_response = client
        .get_raw("/fixed/subscriptions")
        .await
        .context("Failed to fetch fixed subscriptions")?;

    let mut all_databases = Vec::new();

    // Process flexible subscriptions
    if let Some(Value::Array(flex_subs)) = flex_response.get("subscriptions") {
        for sub in flex_subs {
            let sub_id = match sub.get("id").and_then(|i| i.as_u64()) {
                Some(id) => id as u32,
                None => continue,
            };

            // Skip if filtering by subscription and this isn't it
            if let Some(filter_id) = subscription_id
                && sub_id != filter_id
            {
                continue;
            }

            let sub_name = extract_field(sub, "name", "Unknown");

            // Fetch databases for this flexible subscription
            let db_response = client
                .get_raw(&format!("/subscriptions/{}/databases", sub_id))
                .await
                .ok();

            if let Some(Value::Array(databases)) = db_response {
                for db in databases {
                    let mut db_with_sub = db.clone();
                    if let Value::Object(ref mut map) = db_with_sub {
                        map.insert("subscriptionId".to_string(), Value::Number(sub_id.into()));
                        map.insert(
                            "subscriptionName".to_string(),
                            Value::String(sub_name.clone()),
                        );
                    }
                    all_databases.push(db_with_sub);
                }
            }
        }
    }

    // Process fixed subscriptions
    if let Some(Value::Array(fixed_subs)) = fixed_response.get("subscriptions") {
        for sub in fixed_subs {
            let sub_id = match sub.get("id").and_then(|i| i.as_u64()) {
                Some(id) => id as u32,
                None => continue,
            };

            // Skip if filtering by subscription and this isn't it
            if let Some(filter_id) = subscription_id
                && sub_id != filter_id
            {
                continue;
            }

            let sub_name = extract_field(sub, "name", "Unknown");

            // Fetch databases for this fixed subscription
            let db_response = client
                .get_raw(&format!("/fixed/subscriptions/{}/databases", sub_id))
                .await
                .ok();

            // Fixed subscriptions have a different response structure
            if let Some(sub_data) = db_response.and_then(|r| r.get("subscription").cloned())
                && let Some(Value::Array(databases)) = sub_data.get("databases")
            {
                for db in databases {
                    let mut db_with_sub = db.clone();
                    if let Value::Object(ref mut map) = db_with_sub {
                        map.insert("subscriptionId".to_string(), Value::Number(sub_id.into()));
                        map.insert(
                            "subscriptionName".to_string(),
                            Value::String(sub_name.clone()),
                        );
                    }
                    all_databases.push(db_with_sub);
                }
            }
        }
    }

    let data = if let Some(q) = query {
        apply_jmespath(&Value::Array(all_databases), q)?
    } else {
        Value::Array(all_databases)
    };

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_databases_table(&data)?;
        }
        OutputFormat::Json => {
            print_output(data, crate::output::OutputFormat::Json, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
        OutputFormat::Yaml => {
            print_output(data, crate::output::OutputFormat::Yaml, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
    }

    Ok(())
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
    for db in databases {
        let sub_id = extract_field(&db, "subscriptionId", "");
        let db_id = extract_field(&db, "databaseId", &extract_field(&db, "uid", "—"));
        let full_id = if !sub_id.is_empty() && db_id != "—" {
            format!("{}:{}", sub_id, db_id)
        } else {
            db_id.clone()
        };

        rows.push(DatabaseRow {
            id: full_id,
            name: truncate_string(&extract_field(&db, "name", "—"), 20),
            status: format_status(extract_field(&db, "status", "unknown")),
            subscription: truncate_string(
                &extract_field(
                    &db,
                    "subscriptionName",
                    &extract_field(&db, "subscriptionId", "—"),
                ),
                15,
            ),
            memory: format_database_memory(&db),
            region: extract_database_region(&db),
            endpoint: extract_database_endpoint(&db),
            created: format_date(extract_field(&db, "created", "")),
        });
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    output_with_pager(&table.to_string());
    Ok(())
}

/// Format database memory
fn format_database_memory(db: &Value) -> String {
    // Check for fixed subscription memory fields
    if let Some(memory_mb) = db.get("planMemoryLimit").and_then(|m| m.as_f64()) {
        let unit = extract_field(db, "memoryLimitMeasurementUnit", "MB");
        if unit == "GB" {
            return format_memory_size(memory_mb);
        } else if unit == "MB" {
            return format_memory_size(memory_mb / 1024.0);
        }
    }

    if let Some(memory_gb) = db.get("memoryLimitInGb").and_then(|m| m.as_f64()) {
        return format_memory_size(memory_gb);
    }
    "—".to_string()
}

/// Extract database region
fn extract_database_region(db: &Value) -> String {
    if let Some(region) = db.get("region").and_then(|r| r.as_str()) {
        // Just return the region without provider prefix for compactness
        return region.to_string();
    }
    "—".to_string()
}

/// Extract database endpoint
fn extract_database_endpoint(db: &Value) -> String {
    if let Some(public) = db.get("publicEndpoint").and_then(|e| e.as_str()) {
        // Extract just the subdomain and port for compactness
        if let Some(first_part) = public.split('.').next()
            && let Some(port) = public.rsplit(':').next()
        {
            return format!("{}...:{}", first_part, port);
        }
        return truncate_string(public, 35);
    }
    if let Some(private) = db.get("privateEndpoint").and_then(|e| e.as_str()) {
        return format!("[private] {}", truncate_string(private, 25));
    }
    "—".to_string()
}

/// Get detailed database information
async fn get_database(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    database_id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    // Parse database ID - could be "sub_id:db_id" for fixed or just "db_id" for flexible
    let (sub_id, db_id) = if database_id.contains(':') {
        let parts: Vec<&str> = database_id.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow::Error::msg(
                "Invalid database ID format. Use 'subscription_id:database_id'",
            )
            .into());
        }
        (Some(parts[0]), parts[1])
    } else {
        (None, database_id)
    };

    // Try to fetch the database
    let response = if let Some(subscription_id) = sub_id {
        // Fixed database path
        let fixed_response = client
            .get_raw(&format!(
                "/fixed/subscriptions/{}/databases/{}",
                subscription_id, db_id
            ))
            .await;

        match fixed_response {
            Ok(resp) => {
                // Fixed API returns the database nested in response
                if let Some(db) = resp.get("subscription").and_then(|s| s.get("database")) {
                    db.clone()
                } else {
                    resp
                }
            }
            Err(_) => {
                // Try flexible path as fallback
                client
                    .get_raw(&format!(
                        "/subscriptions/{}/databases/{}",
                        subscription_id, db_id
                    ))
                    .await
                    .map_err(|_| {
                        anyhow::Error::msg(format!("Database {} not found", database_id))
                    })?
            }
        }
    } else {
        // For flexible databases, we need to find the subscription first
        return Err(anyhow::Error::msg(
            "For flexible databases, please provide the full ID as 'subscription_id:database_id'",
        )
        .into());
    };

    let data = if let Some(q) = query {
        apply_jmespath(&response, q)?
    } else {
        response
    };

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_database_detail(&data)?;
        }
        OutputFormat::Json => {
            print_output(data, crate::output::OutputFormat::Json, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
        OutputFormat::Yaml => {
            print_output(data, crate::output::OutputFormat::Yaml, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
    }

    Ok(())
}

/// Print database detail in vertical format
fn print_database_detail(data: &Value) -> CliResult<()> {
    let mut rows = Vec::new();

    // Basic information
    if let Some(id) = data.get("databaseId").or_else(|| data.get("uid")) {
        rows.push(DetailRow {
            field: "Database ID".to_string(),
            value: id.to_string().trim_matches('"').to_string(),
        });
    }

    if let Some(name) = data.get("name").and_then(|n| n.as_str()) {
        rows.push(DetailRow {
            field: "Name".to_string(),
            value: name.to_string(),
        });
    }

    if let Some(status) = data.get("status").and_then(|s| s.as_str()) {
        rows.push(DetailRow {
            field: "Status".to_string(),
            value: format_status_text(status),
        });
    }

    // Connection info
    if let Some(endpoint) = data.get("publicEndpoint").and_then(|e| e.as_str()) {
        rows.push(DetailRow {
            field: "Public Endpoint".to_string(),
            value: endpoint.to_string(),
        });
    }

    if let Some(endpoint) = data.get("privateEndpoint").and_then(|e| e.as_str()) {
        rows.push(DetailRow {
            field: "Private Endpoint".to_string(),
            value: endpoint.to_string(),
        });
    }

    // Infrastructure
    if let Some(provider) = data.get("provider").and_then(|p| p.as_str()) {
        rows.push(DetailRow {
            field: "Provider".to_string(),
            value: provider.to_string(),
        });
    }

    if let Some(region) = data.get("region").and_then(|r| r.as_str()) {
        rows.push(DetailRow {
            field: "Region".to_string(),
            value: region.to_string(),
        });
    }

    // Memory and storage
    if let Some(memory) = data
        .get("planMemoryLimit")
        .or_else(|| data.get("memoryLimitInGb"))
    {
        let unit = extract_field(data, "memoryLimitMeasurementUnit", "MB");
        rows.push(DetailRow {
            field: "Memory Limit".to_string(),
            value: format!("{} {}", memory, unit),
        });
    }

    if let Some(used) = data.get("memoryUsedInMb") {
        rows.push(DetailRow {
            field: "Memory Used".to_string(),
            value: format!("{} MB", used),
        });
    }

    // Redis configuration
    if let Some(version) = data.get("redisVersion").and_then(|v| v.as_str()) {
        rows.push(DetailRow {
            field: "Redis Version".to_string(),
            value: version.to_string(),
        });
    }

    if let Some(eviction) = data.get("dataEvictionPolicy").and_then(|e| e.as_str()) {
        rows.push(DetailRow {
            field: "Eviction Policy".to_string(),
            value: eviction.to_string(),
        });
    }

    if let Some(persistence) = data.get("dataPersistence").and_then(|p| p.as_str()) {
        rows.push(DetailRow {
            field: "Persistence".to_string(),
            value: persistence.to_string(),
        });
    }

    // Modules
    if let Some(modules) = data.get("modules").and_then(|m| m.as_array())
        && !modules.is_empty()
    {
        let module_names: Vec<String> = modules
            .iter()
            .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
            .map(|s| s.to_string())
            .collect();

        rows.push(DetailRow {
            field: "Modules".to_string(),
            value: module_names.join(", "),
        });
    }

    // Security
    if let Some(security) = data.get("security") {
        if let Some(tls) = security.get("enableTls").and_then(|t| t.as_bool()) {
            rows.push(DetailRow {
                field: "TLS".to_string(),
                value: if tls {
                    "✓ Enabled".to_string()
                } else {
                    "✗ Disabled".to_string()
                },
            });
        }

        if let Some(source_ips) = security.get("sourceIps").and_then(|s| s.as_array()) {
            let ips: Vec<String> = source_ips
                .iter()
                .filter_map(|ip| ip.as_str())
                .map(|s| s.to_string())
                .collect();

            if !ips.is_empty() {
                rows.push(DetailRow {
                    field: "Allowed IPs".to_string(),
                    value: ips.join(", "),
                });
            }
        }
    }

    // Dates
    if let Some(created) = data
        .get("activatedOn")
        .and_then(|c| c.as_str())
        .or_else(|| data.get("created").and_then(|c| c.as_str()))
    {
        rows.push(DetailRow {
            field: "Created".to_string(),
            value: format_date(created.to_string()),
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
