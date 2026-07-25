//! Implementation of enterprise alerts commands

use anyhow::Result as AnyhowResult;
use serde_json::Value;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;

pub async fn list_alerts(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    filter_type: Option<&str>,
    severity: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    // Get alerts from all sources and combine
    let mut all_alerts = Vec::new();

    // Get cluster alerts
    if (filter_type.is_none() || filter_type == Some("cluster"))
        && let Ok(cluster_alerts) = client.get::<Value>("/v1/cluster/alerts").await
        && let Some(alerts) = cluster_alerts.as_array()
    {
        for alert in alerts {
            let mut alert = alert.clone();
            if let Some(obj) = alert.as_object_mut() {
                obj.insert("type".to_string(), Value::String("cluster".to_string()));
            }
            all_alerts.push(alert);
        }
    }

    // Get node alerts
    if (filter_type.is_none() || filter_type == Some("node"))
        && let Ok(node_alerts) = client.get::<Value>("/v1/nodes/alerts").await
        && let Some(alerts) = node_alerts.as_array()
    {
        for alert in alerts {
            let mut alert = alert.clone();
            if let Some(obj) = alert.as_object_mut() {
                obj.insert("type".to_string(), Value::String("node".to_string()));
            }
            all_alerts.push(alert);
        }
    }

    // Get database alerts
    if (filter_type.is_none() || filter_type == Some("bdb"))
        && let Ok(bdb_alerts) = client.get::<Value>("/v1/bdbs/alerts").await
        && let Some(alerts) = bdb_alerts.as_array()
    {
        for alert in alerts {
            let mut alert = alert.clone();
            if let Some(obj) = alert.as_object_mut() {
                obj.insert("type".to_string(), Value::String("database".to_string()));
            }
            all_alerts.push(alert);
        }
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
