//! Enterprise CRDB (Active-Active) command implementations

use crate::error::RedisCtlError;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use redis_enterprise::crdb_tasks::CrdbTasksHandler;
use serde_json::Value;

use super::utils::*;

/// List all CRDBs
pub async fn list_crdbs(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw("/v1/crdbs")
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get CRDB details
pub async fn get_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}", id))
        .await
        .context(format!("Failed to get CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Create a new CRDB
#[allow(clippy::too_many_arguments)]
pub async fn create_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    name: Option<&str>,
    memory_size: Option<u64>,
    default_db_name: Option<&str>,
    encryption: Option<bool>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut json_data = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let data_obj = json_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(n) = name {
        data_obj.insert("name".to_string(), serde_json::json!(n));
    }
    if let Some(mem) = memory_size {
        data_obj.insert("memory_size".to_string(), serde_json::json!(mem));
    }
    if let Some(db_name) = default_db_name {
        data_obj.insert("default_db_name".to_string(), serde_json::json!(db_name));
    }
    if let Some(enc) = encryption {
        data_obj.insert("encryption".to_string(), serde_json::json!(enc));
    }

    let response = client
        .post_raw("/v1/crdbs", json_data)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Update CRDB configuration
#[allow(clippy::too_many_arguments)]
pub async fn update_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    memory_size: Option<u64>,
    encryption: Option<bool>,
    data_persistence: Option<&str>,
    replication: Option<bool>,
    eviction_policy: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    use crate::error::RedisCtlError;

    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON data if provided, otherwise empty object
    let mut request_obj: serde_json::Map<String, serde_json::Value> = if let Some(json_data) = data
    {
        let parsed = read_json_data(json_data)?;
        parsed
            .as_object()
            .cloned()
            .unwrap_or_else(serde_json::Map::new)
    } else {
        serde_json::Map::new()
    };

    // Override with first-class parameters if provided
    if let Some(mem) = memory_size {
        request_obj.insert("memory_size".to_string(), serde_json::json!(mem));
    }
    if let Some(enc) = encryption {
        request_obj.insert("encryption".to_string(), serde_json::json!(enc));
    }
    if let Some(persist) = data_persistence {
        request_obj.insert("data_persistence".to_string(), serde_json::json!(persist));
    }
    if let Some(repl) = replication {
        request_obj.insert("replication".to_string(), serde_json::json!(repl));
    }
    if let Some(evict) = eviction_policy {
        request_obj.insert("eviction_policy".to_string(), serde_json::json!(evict));
    }

    // Validate at least one update field is provided
    if request_obj.is_empty() {
        return Err(RedisCtlError::InvalidInput {
            message: "At least one update field is required (--memory-size, --encryption, --data-persistence, --replication, --eviction-policy, or --data)".to_string(),
        });
    }

    let json_data = serde_json::Value::Object(request_obj);
    let response = client
        .put_raw(&format!("/v1/crdbs/{}", id), json_data)
        .await
        .context(format!("Failed to update CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Delete a CRDB
pub async fn delete_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    if !force {
        confirm_action(&format!("Delete CRDB {}?", id))?;
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .delete_raw(&format!("/v1/crdbs/{}", id))
        .await
        .context(format!("Failed to delete CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get participating clusters
pub async fn get_participating_clusters(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}/participating_clusters", id))
        .await
        .context(format!(
            "Failed to get participating clusters for CRDB {}",
            id
        ))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Add cluster to CRDB
#[allow(clippy::too_many_arguments)]
pub async fn add_cluster_to_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    url: Option<&str>,
    name: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    compression: Option<bool>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut json_data = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let data_obj = json_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(u) = url {
        data_obj.insert("url".to_string(), serde_json::json!(u));
    }
    if let Some(n) = name {
        data_obj.insert("name".to_string(), serde_json::json!(n));
    }
    if let Some(user) = username {
        let credentials = data_obj
            .entry("credentials")
            .or_insert(serde_json::json!({}));
        if let Some(cred_obj) = credentials.as_object_mut() {
            cred_obj.insert("username".to_string(), serde_json::json!(user));
        }
    }
    if let Some(pass) = password {
        let credentials = data_obj
            .entry("credentials")
            .or_insert(serde_json::json!({}));
        if let Some(cred_obj) = credentials.as_object_mut() {
            cred_obj.insert("password".to_string(), serde_json::json!(pass));
        }
    }
    if let Some(comp) = compression {
        data_obj.insert("compression".to_string(), serde_json::json!(comp));
    }

    let response = client
        .post_raw(
            &format!("/v1/crdbs/{}/participating_clusters", id),
            json_data,
        )
        .await
        .context(format!("Failed to add cluster to CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Remove cluster from CRDB
pub async fn remove_cluster_from_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    cluster_id: u32,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    if !force {
        confirm_action(&format!("Remove cluster {} from CRDB {}?", cluster_id, id))?;
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .delete_raw(&format!(
            "/v1/crdbs/{}/participating_clusters/{}",
            id, cluster_id
        ))
        .await
        .context(format!(
            "Failed to remove cluster {} from CRDB {}",
            cluster_id, id
        ))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get CRDB instances
pub async fn get_crdb_instances(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}/instances", id))
        .await
        .context(format!("Failed to get instances for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get specific CRDB instance
pub async fn get_crdb_instance(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    crdb_id: u32,
    instance_id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}/instances/{}", crdb_id, instance_id))
        .await
        .context(format!(
            "Failed to get instance {} for CRDB {}",
            instance_id, crdb_id
        ))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Update CRDB instance
#[allow(clippy::too_many_arguments)]
pub async fn update_crdb_instance(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    crdb_id: u32,
    instance_id: u32,
    memory_size: Option<u64>,
    port: Option<u16>,
    enabled: Option<bool>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut json_data = if let Some(data_str) = data {
        read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let data_obj = json_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(mem) = memory_size {
        data_obj.insert("memory_size".to_string(), serde_json::json!(mem));
    }
    if let Some(p) = port {
        data_obj.insert("port".to_string(), serde_json::json!(p));
    }
    if let Some(e) = enabled {
        data_obj.insert("enabled".to_string(), serde_json::json!(e));
    }

    let response = client
        .put_raw(
            &format!("/v1/crdbs/{}/instances/{}", crdb_id, instance_id),
            json_data,
        )
        .await
        .context(format!(
            "Failed to update instance {} for CRDB {}",
            instance_id, crdb_id
        ))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Flush CRDB instance data
pub async fn flush_crdb_instance(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    crdb_id: u32,
    instance_id: u32,
    force: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    if !force {
        confirm_action(&format!(
            "Flush data for instance {} in CRDB {}? This will delete all data!",
            instance_id, crdb_id
        ))?;
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .put_raw(
            &format!("/v1/crdbs/{}/instances/{}/flush", crdb_id, instance_id),
            Value::Null,
        )
        .await
        .context(format!(
            "Failed to flush instance {} for CRDB {}",
            instance_id, crdb_id
        ))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get replication status
pub async fn get_replication_status(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}/replication_status", id))
        .await
        .context(format!("Failed to get replication status for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get replication lag
pub async fn get_replication_lag(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}/lag", id))
        .await
        .context(format!("Failed to get replication lag for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Force sync CRDB
pub async fn force_sync_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    source_cluster: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let json_data = serde_json::json!({
        "source_cluster": source_cluster
    });

    let response = client
        .post_raw(&format!("/v1/crdbs/{}/sync", id), json_data)
        .await
        .context(format!("Failed to force sync CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Pause replication
pub async fn pause_replication(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .put_raw(&format!("/v1/crdbs/{}/replication/pause", id), Value::Null)
        .await
        .context(format!("Failed to pause replication for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Resume replication
pub async fn resume_replication(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .put_raw(&format!("/v1/crdbs/{}/replication/resume", id), Value::Null)
        .await
        .context(format!("Failed to resume replication for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get CRDB tasks
pub async fn get_crdb_tasks(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let crdb = client
        .get_raw(&format!("/v1/crdbs/{id}"))
        .await
        .with_context(|| format!("Failed to get CRDB {id}"))?;
    let guid = crdb
        .get("guid")
        .or_else(|| crdb.get("crdb_guid"))
        .and_then(Value::as_str)
        .with_context(|| format!("CRDB {id} response did not include its GUID"))?;
    let tasks = CrdbTasksHandler::new(client)
        .list_by_crdb(guid)
        .await
        .with_context(|| format!("Failed to get tasks for CRDB {id}"))?;
    let response = serde_json::to_value(tasks).context("Failed to serialize CRDB tasks")?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get specific CRDB task
pub async fn get_crdb_task(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    crdb_id: u32,
    task_id: String,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let task = CrdbTasksHandler::new(client)
        .get(&task_id)
        .await
        .with_context(|| format!("Failed to get task {task_id} for CRDB {crdb_id}"))?;
    let response = serde_json::to_value(task).context("Failed to serialize CRDB task")?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Cancel running CRDB task
pub async fn cancel_crdb_task(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    crdb_id: u32,
    task_id: String,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    CrdbTasksHandler::new(client)
        .cancel(&task_id)
        .await
        .with_context(|| format!("Failed to cancel task {task_id} for CRDB {crdb_id}"))?;
    let response = serde_json::json!({
        "cancelled": true,
        "crdb_id": crdb_id,
        "task_id": task_id,
    });

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get CRDB statistics
pub async fn get_crdb_stats(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}/stats", id))
        .await
        .context(format!("Failed to get statistics for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get CRDB metrics
pub async fn get_crdb_metrics(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    interval: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let mut path = format!("/v1/crdbs/{}/metrics", id);
    if let Some(interval) = interval {
        path.push_str(&format!("?interval={}", interval));
    }

    let response = client
        .get_raw(&path)
        .await
        .context(format!("Failed to get metrics for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Run health check on CRDB
pub async fn health_check_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let response = client
        .get_raw(&format!("/v1/crdbs/{}/health", id))
        .await
        .context(format!("Failed to run health check for CRDB {}", id))?;

    let data = handle_output(response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

// Additional missing functions

#[allow(clippy::too_many_arguments)]
pub async fn update_cluster_in_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    cluster_id: u32,
    url: Option<&str>,
    compression: Option<bool>,
    proxy_policy: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut update_data = if let Some(data_str) = data {
        read_json_data(data_str).context("Failed to parse update data")?
    } else {
        serde_json::json!({})
    };

    let data_obj = update_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(u) = url {
        data_obj.insert("url".to_string(), serde_json::json!(u));
    }
    if let Some(comp) = compression {
        data_obj.insert("compression".to_string(), serde_json::json!(comp));
    }
    if let Some(policy) = proxy_policy {
        data_obj.insert("proxy_policy".to_string(), serde_json::json!(policy));
    }

    let result = client
        .put_raw(
            &format!("/v1/crdbs/{}/participating_clusters/{}", id, cluster_id),
            update_data,
        )
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_conflicts(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    limit: Option<u32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let url = if let Some(limit) = limit {
        format!("/v1/crdbs/{}/conflicts?limit={}", id, limit)
    } else {
        format!("/v1/crdbs/{}/conflicts", id)
    };

    let result = client.get_raw(&url).await.map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_conflict_policy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let result = client
        .get_raw(&format!("/v1/crdbs/{}/conflict_policy", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_conflict_policy(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    policy: Option<&str>,
    source_id: Option<u32>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut policy_data = if let Some(data_str) = data {
        read_json_data(data_str).context("Failed to parse policy data")?
    } else {
        serde_json::json!({})
    };

    let data_obj = policy_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(p) = policy {
        data_obj.insert("policy".to_string(), serde_json::json!(p));
    }
    if let Some(src) = source_id {
        data_obj.insert("source_id".to_string(), serde_json::json!(src));
    }

    let result = client
        .put_raw(&format!("/v1/crdbs/{}/conflict_policy", id), policy_data)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn resolve_conflict(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    conflict_id: &str,
    resolution: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let resolution_data = serde_json::json!({
        "resolution": resolution
    });

    let result = client
        .post_raw(
            &format!("/v1/crdbs/{}/conflicts/{}/resolve", id, conflict_id),
            resolution_data,
        )
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_crdb_connections(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let result = client
        .get_raw(&format!("/v1/crdbs/{}/connections", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_crdb_throughput(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let result = client
        .get_raw(&format!("/v1/crdbs/{}/throughput", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn backup_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    location: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut backup_data = if let Some(data_str) = data {
        read_json_data(data_str).context("Failed to parse backup data")?
    } else {
        serde_json::json!({})
    };

    let backup_obj = backup_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(loc) = location {
        backup_obj.insert("location".to_string(), serde_json::json!(loc));
    }

    let result = client
        .post_raw(&format!("/v1/crdbs/{}/backup", id), backup_data)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn restore_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    backup_uid: Option<&str>,
    location: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut restore_data = if let Some(data_str) = data {
        read_json_data(data_str).context("Failed to parse restore data")?
    } else {
        serde_json::json!({})
    };

    let restore_obj = restore_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(uid) = backup_uid {
        restore_obj.insert("backup_uid".to_string(), serde_json::json!(uid));
    }
    if let Some(loc) = location {
        restore_obj.insert("location".to_string(), serde_json::json!(loc));
    }

    let result = client
        .post_raw(&format!("/v1/crdbs/{}/restore", id), restore_data)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn get_crdb_backups(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let result = client
        .get_raw(&format!("/v1/crdbs/{}/backups", id))
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

pub async fn export_crdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    id: u32,
    location: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut export_data = if let Some(data_str) = data {
        read_json_data(data_str).context("Failed to parse export data")?
    } else {
        serde_json::json!({})
    };

    let export_obj = export_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(loc) = location {
        export_obj.insert("location".to_string(), serde_json::json!(loc));
    }

    let result = client
        .post_raw(&format!("/v1/crdbs/{}/export", id), export_data)
        .await
        .map_err(RedisCtlError::from)?;

    let data = handle_output(result, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}
