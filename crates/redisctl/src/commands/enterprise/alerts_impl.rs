//! Implementation of enterprise alerts commands

use anyhow::{Context, Result as AnyhowResult};
use serde_json::{Map, Value};

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;

/// One alert source queried by `enterprise alerts list`.
struct AlertSource {
    /// Value accepted by `--filter-type`.
    filter: &'static str,
    /// Value written to each alert's `type` field.
    type_tag: &'static str,
    /// REST route listing every alert state for this source.
    path: &'static str,
    /// `true` when the response is keyed by entity UID, then alert name
    /// (`{"1": {"node_cpu_utilization": {...}}}`). `false` when it is keyed
    /// by alert name only (`{"cluster_node_down": {...}}`).
    per_entity: bool,
}

/// Response shapes follow the Redis Software REST API reference for
/// `GET /v1/cluster/alerts`, `GET /v1/nodes/alerts` and `GET /v1/bdbs/alerts`.
const ALERT_SOURCES: [AlertSource; 3] = [
    AlertSource {
        filter: "cluster",
        type_tag: "cluster",
        path: "/v1/cluster/alerts",
        per_entity: false,
    },
    AlertSource {
        filter: "node",
        type_tag: "node",
        path: "/v1/nodes/alerts",
        per_entity: true,
    },
    AlertSource {
        filter: "bdb",
        type_tag: "database",
        path: "/v1/bdbs/alerts",
        per_entity: true,
    },
];

/// Flatten one source's alert map into a list of alert records.
///
/// Each record carries `type`, the alert `name` (the map key) and, for node
/// and database alerts, `entity_uid` (the node or database UID map key),
/// followed by the alert state fields as the server returned them.
fn flatten_alerts(source: &AlertSource, response: Value) -> AnyhowResult<Vec<Value>> {
    let Value::Object(map) = response else {
        anyhow::bail!(
            "Unexpected response from {}: expected a JSON object, got {}",
            source.path,
            json_kind(&response)
        );
    };

    let mut alerts = Vec::new();
    if source.per_entity {
        for (entity_uid, entity_alerts) in map {
            let Value::Object(entity_alerts) = entity_alerts else {
                anyhow::bail!(
                    "Unexpected response from {}: expected an object of alerts for UID {}, got {}",
                    source.path,
                    entity_uid,
                    json_kind(&entity_alerts)
                );
            };
            for (name, state) in entity_alerts {
                alerts.push(alert_record(source, name, Some(&entity_uid), state)?);
            }
        }
    } else {
        for (name, state) in map {
            alerts.push(alert_record(source, name, None, state)?);
        }
    }
    Ok(alerts)
}

fn alert_record(
    source: &AlertSource,
    name: String,
    entity_uid: Option<&str>,
    state: Value,
) -> AnyhowResult<Value> {
    let Value::Object(fields) = state else {
        anyhow::bail!(
            "Unexpected response from {}: expected an object for alert {}, got {}",
            source.path,
            name,
            json_kind(&state)
        );
    };

    let mut record = Map::new();
    record.insert(
        "type".to_string(),
        Value::String(source.type_tag.to_string()),
    );
    record.insert("name".to_string(), Value::String(name));
    if let Some(uid) = entity_uid {
        record.insert("entity_uid".to_string(), Value::String(uid.to_string()));
    }
    for (key, value) in fields {
        record.entry(key).or_insert(value);
    }
    Ok(Value::Object(record))
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

pub async fn list_alerts(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    filter_type: Option<&str>,
    severity: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    if let Some(filter) = filter_type
        && !ALERT_SOURCES.iter().any(|source| source.filter == filter)
    {
        anyhow::bail!(
            "Unknown alert type '{}': expected one of cluster, node, bdb",
            filter
        );
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Get alerts from every selected source and combine them. A failed
    // request fails the command rather than silently dropping that source.
    let mut all_alerts = Vec::new();
    for source in &ALERT_SOURCES {
        if filter_type.is_some_and(|filter| filter != source.filter) {
            continue;
        }
        let response = client.get::<Value>(source.path).await.with_context(|| {
            format!(
                "Failed to list {} alerts from {}",
                source.type_tag, source.path
            )
        })?;
        all_alerts.extend(flatten_alerts(source, response)?);
    }

    // Filter by severity if specified
    if let Some(severity) = severity {
        all_alerts.retain(|alert| {
            alert
                .get("severity")
                .and_then(|s| s.as_str())
                .map(|s| s.eq_ignore_ascii_case(severity))
                .unwrap_or(false)
        });
    }

    let response = Value::Array(all_alerts);

    // Apply JMESPath query if provided
    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}

pub async fn get_alert(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: u64,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Try to find the alert in different endpoints
    // First try cluster alerts
    if let Ok(alerts) = client
        .get::<Value>(&format!("/v1/cluster/alerts/{}", uid))
        .await
    {
        let response = if let Some(q) = query {
            super::utils::apply_jmespath(&alerts, q)?
        } else {
            alerts
        };
        return super::utils::print_formatted_output(response, output_format)
            .map_err(|e| anyhow::anyhow!(e));
    }

    // Try node alerts
    if let Ok(alerts) = client
        .get::<Value>(&format!("/v1/nodes/alerts/{}", uid))
        .await
    {
        let response = if let Some(q) = query {
            super::utils::apply_jmespath(&alerts, q)?
        } else {
            alerts
        };
        return super::utils::print_formatted_output(response, output_format)
            .map_err(|e| anyhow::anyhow!(e));
    }

    // Try database alerts
    if let Ok(alerts) = client
        .get::<Value>(&format!("/v1/bdbs/alerts/{}", uid))
        .await
    {
        let response = if let Some(q) = query {
            super::utils::apply_jmespath(&alerts, q)?
        } else {
            alerts
        };
        return super::utils::print_formatted_output(response, output_format)
            .map_err(|e| anyhow::anyhow!(e));
    }

    anyhow::bail!("Alert with UID {} not found", uid)
}

pub async fn cluster_alerts(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    alert: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let endpoint = if let Some(alert_name) = alert {
        format!("/v1/cluster/alerts/{}", alert_name)
    } else {
        "/v1/cluster/alerts".to_string()
    };

    let response = client
        .get::<Value>(&endpoint)
        .await
        .map_err(RedisCtlError::from)?;

    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}

pub async fn node_alerts(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    node_uid: Option<u64>,
    alert: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let endpoint = match (node_uid, alert) {
        (Some(uid), Some(alert_name)) => format!("/v1/nodes/alerts/{}/{}", uid, alert_name),
        (Some(uid), None) => format!("/v1/nodes/alerts/{}", uid),
        (None, None) => "/v1/nodes/alerts".to_string(),
        (None, Some(_)) => anyhow::bail!("Cannot specify alert without node_uid"),
    };

    let response = client
        .get::<Value>(&endpoint)
        .await
        .map_err(RedisCtlError::from)?;

    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}

pub async fn database_alerts(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    bdb_uid: Option<u64>,
    alert: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    let endpoint = match (bdb_uid, alert) {
        (Some(uid), Some(alert_name)) => format!("/v1/bdbs/alerts/{}/{}", uid, alert_name),
        (Some(uid), None) => format!("/v1/bdbs/alerts/{}", uid),
        (None, None) => "/v1/bdbs/alerts".to_string(),
        (None, Some(_)) => anyhow::bail!("Cannot specify alert without bdb_uid"),
    };

    let response = client
        .get::<Value>(&endpoint)
        .await
        .map_err(RedisCtlError::from)?;

    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}

pub async fn get_alert_settings(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Alert settings are part of cluster configuration
    let response = client
        .get::<Value>("/v1/cluster")
        .await
        .map_err(RedisCtlError::from)?;

    // Extract alert_settings from the cluster config
    let alert_settings = response
        .get("alert_settings")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&alert_settings, q)?
    } else {
        alert_settings
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}

#[allow(clippy::too_many_arguments)]
pub async fn update_alert_settings(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cluster_alerts: Option<bool>,
    node_alerts: Option<bool>,
    bdb_alerts: Option<bool>,
    memory_threshold: Option<u32>,
    cpu_threshold: Option<u32>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Start with JSON from --data if provided, otherwise empty object
    let mut json_data = if let Some(data_str) = data {
        super::utils::read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let data_obj = json_data.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(ca) = cluster_alerts {
        data_obj.insert("cluster_alerts_enabled".to_string(), serde_json::json!(ca));
    }
    if let Some(na) = node_alerts {
        data_obj.insert("node_alerts_enabled".to_string(), serde_json::json!(na));
    }
    if let Some(ba) = bdb_alerts {
        data_obj.insert("bdb_alerts_enabled".to_string(), serde_json::json!(ba));
    }
    if let Some(mt) = memory_threshold {
        data_obj.insert("memory_threshold".to_string(), serde_json::json!(mt));
    }
    if let Some(ct) = cpu_threshold {
        data_obj.insert("cpu_threshold".to_string(), serde_json::json!(ct));
    }

    // Alert settings are updated through cluster configuration
    let update_payload = serde_json::json!({
        "alert_settings": json_data
    });

    let response = client
        .put::<_, Value>("/v1/cluster", &update_payload)
        .await
        .map_err(RedisCtlError::from)?;

    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}
