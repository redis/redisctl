//! Fixed database command implementations

use crate::cli::{CloudFixedDatabaseCommands, OutputFormat};
use crate::commands::cloud::async_utils::handle_async_response;
use crate::commands::cloud::utils::{confirm_action, handle_output, print_formatted_output};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use anyhow::Context;
use redis_cloud::fixed::databases::{
    DatabaseTagCreateRequest, DatabaseTagUpdateRequest, FixedDatabaseBackupRequest,
    FixedDatabaseCreateRequest, FixedDatabaseHandler, FixedDatabaseImportRequest,
    FixedDatabaseUpdateRequest,
};

/// Read JSON data from string or file
fn read_json_data(data: &str) -> CliResult<serde_json::Value> {
    let json_str = if let Some(file_path) = data.strip_prefix('@') {
        std::fs::read_to_string(file_path).map_err(|e| RedisCtlError::InvalidInput {
            message: format!("Failed to read file {}: {}", file_path, e),
        })?
    } else {
        data.to_string()
    };

    serde_json::from_str(&json_str).map_err(|e| RedisCtlError::InvalidInput {
        message: format!("Invalid JSON: {}", e),
    })
}

/// Parse tag string in key=value format
fn parse_tag(tag: &str) -> CliResult<(String, String)> {
    let parts: Vec<&str> = tag.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(RedisCtlError::InvalidInput {
            message: format!("Invalid tag format '{}'. Expected 'key=value' format", tag),
        });
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse database ID in format "subscription_id:database_id"
fn parse_fixed_database_id(id: &str) -> CliResult<(i32, i32)> {
    let parts: Vec<&str> = id.split(':').collect();
    if parts.len() != 2 {
        return Err(RedisCtlError::InvalidInput {
            message: format!(
                "Invalid database ID format: {}. Expected format: subscription_id:database_id",
                id
            ),
        });
    }

    let subscription_id = parts[0]
        .parse::<i32>()
        .with_context(|| format!("Invalid subscription ID: {}", parts[0]))?;
    let database_id = parts[1]
        .parse::<i32>()
        .with_context(|| format!("Invalid database ID: {}", parts[1]))?;

    Ok((subscription_id, database_id))
}

/// Handle fixed database commands
pub async fn handle_fixed_database_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudFixedDatabaseCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr
        .create_cloud_client(profile_name)
        .await
        .context("Failed to create Cloud client")?;

    let handler = FixedDatabaseHandler::new(client);

    match command {
        CloudFixedDatabaseCommands::List { subscription_id } => {
            let databases = handler
                .list(*subscription_id, None, None)
                .await
                .context("Failed to list fixed databases")?;

            let json_response =
                serde_json::to_value(databases).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::Get { id } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            let database = handler
                .get_by_id(subscription_id, database_id)
                .await
                .context("Failed to get fixed database")?;

            let json_response =
                serde_json::to_value(database).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::Create {
            subscription_id,
            name,
            password,
            enable_tls,
            eviction_policy,
            replication,
            data_persistence,
            data,
            async_ops,
        } => {
            // Start with JSON from --data if provided, otherwise empty object
            let mut request_value = if let Some(data_str) = data {
                read_json_data(data_str)?
            } else {
                serde_json::json!({})
            };

            let request_obj = request_value.as_object_mut().unwrap();

            // CLI parameters override JSON values
            if let Some(n) = name {
                request_obj.insert("name".to_string(), serde_json::json!(n));
            }
            if let Some(p) = password {
                request_obj.insert("password".to_string(), serde_json::json!(p));
            }
            if let Some(tls) = enable_tls {
                request_obj.insert("enableTls".to_string(), serde_json::json!(tls));
            }
            if let Some(eviction) = eviction_policy {
                request_obj.insert(
                    "dataEvictionPolicy".to_string(),
                    serde_json::json!(eviction),
                );
            }
            if let Some(repl) = replication {
                request_obj.insert("replication".to_string(), serde_json::json!(repl));
            }
            if let Some(persistence) = data_persistence {
                request_obj.insert(
                    "dataPersistence".to_string(),
                    serde_json::json!(persistence),
                );
            }

            // Validate required fields
            if !request_obj.contains_key("name") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--name is required (or provide via --data JSON)".to_string(),
                });
            }

            let request: FixedDatabaseCreateRequest =
                serde_json::from_value(request_value).context("Invalid database configuration")?;

            let result = handler
                .create(*subscription_id, &request)
                .await
                .context("Failed to create fixed database")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Fixed database created successfully",
            )
            .await
        }

        CloudFixedDatabaseCommands::Update {
            id,
            name,
            password,
            enable_tls,
            eviction_policy,
            replication,
            data_persistence,
            data,
            async_ops,
        } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            // Start with JSON from --data if provided, otherwise empty object
            let mut request_value = if let Some(data_str) = data {
                read_json_data(data_str)?
            } else {
                serde_json::json!({})
            };

            let request_obj = request_value.as_object_mut().unwrap();

            // CLI parameters override JSON values
            if let Some(n) = name {
                request_obj.insert("name".to_string(), serde_json::json!(n));
            }
            if let Some(p) = password {
                request_obj.insert("password".to_string(), serde_json::json!(p));
            }
            if let Some(tls) = enable_tls {
                request_obj.insert("enableTls".to_string(), serde_json::json!(tls));
            }
            if let Some(eviction) = eviction_policy {
                request_obj.insert(
                    "dataEvictionPolicy".to_string(),
                    serde_json::json!(eviction),
                );
            }
            if let Some(repl) = replication {
                request_obj.insert("replication".to_string(), serde_json::json!(repl));
            }
            if let Some(persistence) = data_persistence {
                request_obj.insert(
                    "dataPersistence".to_string(),
                    serde_json::json!(persistence),
                );
            }

            // Validate that we have at least one field to update
            if request_obj.is_empty() {
                return Err(RedisCtlError::InvalidInput {
                    message: "At least one update field is required (--name, --password, --enable-tls, --eviction-policy, --replication, --data-persistence, or --data)".to_string(),
                });
            }

            let request: FixedDatabaseUpdateRequest =
                serde_json::from_value(request_value).context("Invalid update configuration")?;

            let result = handler
                .update(subscription_id, database_id, &request)
                .await
                .context("Failed to update fixed database")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Fixed database updated successfully",
            )
            .await
        }

        CloudFixedDatabaseCommands::Delete {
            id,
            force,
            async_ops,
        } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            if !force {
                let prompt = format!("Delete fixed database {}:{}?", subscription_id, database_id);
                confirm_action(&prompt)?;
            }

            let result = handler
                .delete_by_id(subscription_id, database_id)
                .await
                .context("Failed to delete fixed database")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Fixed database deleted successfully",
            )
            .await
        }

        CloudFixedDatabaseCommands::BackupStatus { id } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            let status = handler
                .get_backup_status(subscription_id, database_id)
                .await
                .context("Failed to get backup status")?;

            let json_response =
                serde_json::to_value(status).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::Backup { id, async_ops } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            // Create a minimal backup request - most fields are optional
            let backup_request = FixedDatabaseBackupRequest {
                subscription_id: Some(subscription_id),
                database_id: Some(database_id),
                adhoc_backup_path: None,
                command_type: None,
            };

            let result = handler
                .backup(subscription_id, database_id, &backup_request)
                .await
                .context("Failed to initiate backup")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Backup initiated successfully",
            )
            .await
        }

        CloudFixedDatabaseCommands::ImportStatus { id } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            let status = handler
                .get_import_status(subscription_id, database_id)
                .await
                .context("Failed to get import status")?;

            let json_response =
                serde_json::to_value(status).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::Import {
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
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            // Start with JSON from --data if provided, otherwise empty object
            let mut request_value = if let Some(data_str) = data {
                read_json_data(data_str)?
            } else {
                serde_json::json!({})
            };

            let request_obj = request_value.as_object_mut().unwrap();

            // CLI parameters override JSON values
            if let Some(st) = source_type {
                request_obj.insert("sourceType".to_string(), serde_json::json!(st));
            }

            if let Some(uri) = import_from_uri {
                request_obj.insert("importFromUri".to_string(), serde_json::json!([uri]));
            }

            // AWS credentials
            if aws_access_key.is_some() || aws_secret_key.is_some() {
                let mut credentials = serde_json::json!({});
                if let Some(key) = aws_access_key {
                    credentials["accessKeyId"] = serde_json::json!(key);
                }
                if let Some(secret) = aws_secret_key {
                    credentials["accessSecretKey"] = serde_json::json!(secret);
                }
                request_obj.insert("credentials".to_string(), credentials);
            }

            // GCS credentials
            if gcs_client_email.is_some() || gcs_private_key.is_some() {
                let mut credentials = serde_json::json!({});
                if let Some(email) = gcs_client_email {
                    credentials["clientEmail"] = serde_json::json!(email);
                }
                if let Some(key) = gcs_private_key {
                    credentials["privateKey"] = serde_json::json!(key);
                }
                request_obj.insert("credentials".to_string(), credentials);
            }

            // Azure credentials
            if azure_account_name.is_some() || azure_account_key.is_some() {
                let mut credentials = serde_json::json!({});
                if let Some(name) = azure_account_name {
                    credentials["storageAccountName"] = serde_json::json!(name);
                }
                if let Some(key) = azure_account_key {
                    credentials["storageAccountKey"] = serde_json::json!(key);
                }
                request_obj.insert("credentials".to_string(), credentials);
            }

            // Validate required fields
            if !request_obj.contains_key("sourceType") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--source-type is required (or provide via --data JSON)".to_string(),
                });
            }

            if !request_obj.contains_key("importFromUri") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--import-from-uri is required (or provide via --data JSON)"
                        .to_string(),
                });
            }

            let request: FixedDatabaseImportRequest =
                serde_json::from_value(request_value).context("Invalid import configuration")?;

            let result = handler
                .import(subscription_id, database_id, &request)
                .await
                .context("Failed to initiate import")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Import initiated successfully",
            )
            .await
        }

        CloudFixedDatabaseCommands::SlowLog {
            id,
            limit: _,
            offset: _,
        } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            // Note: The API doesn't currently support limit/offset parameters
            let result = handler
                .get_slow_log(subscription_id, database_id)
                .await
                .context("Failed to get slow log")?;

            let json_result =
                serde_json::to_value(result).context("Failed to serialize response")?;
            let data = handle_output(json_result, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::ListTags { id } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            let tags = handler
                .get_tags(subscription_id, database_id)
                .await
                .context("Failed to get tags")?;

            let json_response =
                serde_json::to_value(tags).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::AddTag { id, key, value } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            let tag_request = DatabaseTagCreateRequest {
                subscription_id: Some(subscription_id),
                database_id: Some(database_id),
                command_type: None,
                key: key.clone(),
                value: value.clone(),
            };

            let result = handler
                .create_tag(subscription_id, database_id, &tag_request)
                .await
                .context("Failed to add tag")?;

            let json_result =
                serde_json::to_value(result).context("Failed to serialize response")?;
            let data = handle_output(json_result, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::UpdateTags { id, tags, data } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            // Start with JSON from --data if provided, otherwise empty object
            let mut request_value = if let Some(data_str) = data {
                read_json_data(data_str)?
            } else {
                serde_json::json!({})
            };

            let request_obj = request_value.as_object_mut().unwrap();

            // Build tags array from --tag parameters
            if !tags.is_empty() {
                let mut tag_array = Vec::new();
                for tag in tags {
                    let (key, value) = parse_tag(tag)?;
                    tag_array.push(serde_json::json!({
                        "key": key,
                        "value": value
                    }));
                }
                request_obj.insert("tags".to_string(), serde_json::json!(tag_array));
            }

            // Extract tags array from request
            let tags_vec =
                if let Some(tags_array) = request_obj.get("tags").and_then(|v| v.as_array()) {
                    tags_array.clone()
                } else {
                    return Err(RedisCtlError::InvalidInput {
                        message: "At least one --tag is required (or provide via --data JSON)"
                            .to_string(),
                    });
                };

            // Build the request with the proper structure
            let tags_request = serde_json::json!({
                "subscription_id": subscription_id,
                "database_id": database_id,
                "tags": tags_vec
            });

            // Use raw API call since the types don't match exactly
            let client = conn_mgr
                .create_cloud_client(profile_name)
                .await
                .context("Failed to create Cloud client")?;

            let result = client
                .put_raw(
                    &format!(
                        "/fixed/subscriptions/{}/databases/{}/tags",
                        subscription_id, database_id
                    ),
                    tags_request,
                )
                .await
                .context("Failed to update tags")?;

            let output_data = handle_output(result, output_format, query)?;
            print_formatted_output(output_data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::UpdateTag { id, key, value } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            let tag_request = DatabaseTagUpdateRequest {
                subscription_id: Some(subscription_id),
                database_id: Some(database_id),
                command_type: None,
                key: Some(key.clone()),
                value: value.clone(),
            };

            let result = handler
                .update_tag(subscription_id, database_id, key.clone(), &tag_request)
                .await
                .context("Failed to update tag")?;

            let json_result =
                serde_json::to_value(result).context("Failed to serialize response")?;
            let data = handle_output(json_result, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::DeleteTag { id, key } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            let _result = handler
                .delete_tag(subscription_id, database_id, key.clone())
                .await
                .context("Failed to delete tag")?;

            eprintln!("Tag '{}' deleted successfully", key);
            Ok(())
        }

        CloudFixedDatabaseCommands::AvailableVersions { id } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            let json_response = handler
                .get_available_target_versions(subscription_id, database_id)
                .await
                .context("Failed to get available versions")?;

            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::UpgradeStatus { id } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;
            let result = handler
                .get_upgrade_status(subscription_id, database_id)
                .await
                .context("Failed to get upgrade status")?;

            let data = handle_output(result, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedDatabaseCommands::UpgradeRedis {
            id,
            version,
            async_ops,
        } => {
            let (subscription_id, database_id) = parse_fixed_database_id(id)?;

            let result = handler
                .upgrade_redis_version(subscription_id, database_id, version)
                .await
                .context("Failed to upgrade Redis version")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                result,
                async_ops,
                output_format,
                query,
                "Redis version upgrade initiated",
            )
            .await
        }
    }
}
