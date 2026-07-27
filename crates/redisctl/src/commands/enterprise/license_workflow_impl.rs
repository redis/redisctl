//! Implementation of enterprise license workflow commands

use anyhow::Result as AnyhowResult;
use serde_json::Value;

use crate::cli::OutputFormat;
use crate::connection::ConnectionManager;

use super::license_impl::{bytes_to_gb, calculate_days_remaining};

pub async fn license_audit(
    conn_mgr: &ConnectionManager,
    expiring_only: bool,
    expired_only: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let mut audit_results = Vec::new();

    // Get all enterprise profiles
    for (profile_name, profile) in conn_mgr.config.profiles.iter() {
        if profile.deployment_type != redisctl_core::DeploymentType::Enterprise {
            continue;
        }

        // Try to get license info for this profile
        match conn_mgr.create_enterprise_client(Some(profile_name)).await {
            Ok(client) => {
                match client.get::<Value>("/v1/license").await {
                    Ok(license) => {
                        let expired = license
                            .get("expired")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let expiration_date = license
                            .get("expiration_date")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let days_remaining = calculate_days_remaining(Some(expiration_date));
                        let is_expiring = (0..=30).contains(&days_remaining);

                        // Apply filters
                        if expired_only && !expired {
                            continue;
                        }
                        if expiring_only && !is_expiring && !expired {
                            continue;
                        }

                        audit_results.push(serde_json::json!({
                            "profile": profile_name,
                            "cluster_name": license.get("cluster_name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "expiration_date": expiration_date,
                            "days_remaining": days_remaining,
                            "expired": expired,
                            "expiring_soon": is_expiring,
                            "shards_limit": license.get("shards_limit"),
                            "ram_limit_gb": bytes_to_gb(
                                license.get("ram_limit").and_then(|v| v.as_i64()).unwrap_or(0)
                            ),
                            "status": if expired {
                                "EXPIRED"
                            } else if is_expiring {
                                "EXPIRING"
                            } else {
                                "OK"
                            }
                        }));
                    }
                    Err(e) => {
                        audit_results.push(serde_json::json!({
                            "profile": profile_name,
                            "error": format!("Failed to get license: {}", e),
                            "status": "ERROR"
                        }));
                    }
                }
            }
            Err(e) => {
                audit_results.push(serde_json::json!({
                    "profile": profile_name,
                    "error": format!("Failed to connect: {}", e),
                    "status": "ERROR"
                }));
            }
        }
    }

    let response = Value::Array(audit_results);
    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}

pub async fn bulk_update(
    conn_mgr: &ConnectionManager,
    profiles: &str,
    data: &str,
    dry_run: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let license_data = super::utils::read_json_data(data)?;

    // Determine which profiles to update
    let target_profiles: Vec<String> = if profiles == "all" {
        conn_mgr
            .config
            .profiles
            .iter()
            .filter(|(_, p)| p.deployment_type == redisctl_core::DeploymentType::Enterprise)
            .map(|(name, _)| name.clone())
            .collect()
    } else {
        profiles.split(',').map(|s| s.trim().to_string()).collect()
    };

    let mut update_results = Vec::new();

    for profile_name in target_profiles {
        if !conn_mgr.config.profiles.contains_key(&profile_name) {
            update_results.push(serde_json::json!({
                "profile": profile_name,
                "status": "SKIPPED",
                "message": "Profile not found"
            }));
            continue;
        }

        if dry_run {
            update_results.push(serde_json::json!({
                "profile": profile_name,
                "status": "DRY_RUN",
                "message": "Would update license"
            }));
        } else {
            match conn_mgr.create_enterprise_client(Some(&profile_name)).await {
                Ok(client) => match client.put::<_, Value>("/v1/license", &license_data).await {
                    Ok(_) => {
                        update_results.push(serde_json::json!({
                            "profile": profile_name,
                            "status": "SUCCESS",
                            "message": "License updated successfully"
                        }));
                    }
                    Err(e) => {
                        update_results.push(serde_json::json!({
                            "profile": profile_name,
                            "status": "FAILED",
                            "message": format!("Failed to update license: {}", e)
                        }));
                    }
                },
                Err(e) => {
                    update_results.push(serde_json::json!({
                        "profile": profile_name,
                        "status": "FAILED",
                        "message": format!("Failed to connect: {}", e)
                    }));
                }
            }
        }
    }

    let response = Value::Array(update_results);
    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response, output_format).map_err(|e| anyhow::anyhow!(e))
}

pub async fn license_report(
    conn_mgr: &ConnectionManager,
    format: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let mut report_data = Vec::new();

    for (profile_name, profile) in conn_mgr.config.profiles.iter() {
        if profile.deployment_type != redisctl_core::DeploymentType::Enterprise {
            continue;
        }

        match conn_mgr.create_enterprise_client(Some(profile_name)).await {
            Ok(client) => {
                // Get license info
                let license = client.get::<Value>("/v1/license").await.ok();
                // Get cluster info for usage
                let cluster = client.get::<Value>("/v1/cluster").await.ok();

                if let (Some(license), Some(cluster)) = (license, cluster) {
                    report_data.push(serde_json::json!({
                        "profile": profile_name,
                        "cluster_name": license.get("cluster_name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                        "activation_date": license.get("activation_date").and_then(|v| v.as_str()).unwrap_or("unknown"),
                        "expiration_date": license.get("expiration_date").and_then(|v| v.as_str()).unwrap_or("unknown"),
                        "days_remaining": calculate_days_remaining(
                            license.get("expiration_date").and_then(|v| v.as_str())
                        ),
                        "expired": license.get("expired").and_then(|v| v.as_bool()).unwrap_or(false),
                        "shards_limit": license.get("shards_limit").and_then(|v| v.as_i64()).unwrap_or(0),
                        "shards_used": cluster.get("shards_used").and_then(|v| v.as_i64()).unwrap_or(0),
                        "ram_limit_gb": bytes_to_gb(
                            license.get("ram_limit").and_then(|v| v.as_i64()).unwrap_or(0)
                        ),
                        "ram_used_gb": bytes_to_gb(
                            cluster.get("ram_used").and_then(|v| v.as_i64()).unwrap_or(0)
                        ),
                        "nodes_count": cluster.get("nodes_count").and_then(|v| v.as_i64()).unwrap_or(0),
                        "flash_enabled": license.get("flash_enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                        "rack_awareness": license.get("rack_awareness").and_then(|v| v.as_bool()).unwrap_or(false),
                    }));
                }
            }
            Err(_) => continue,
        }
    }

    // Format as CSV if requested
    if format == "csv" {
        if !report_data.is_empty() {
            println!(
                "profile,cluster_name,activation_date,expiration_date,days_remaining,expired,shards_limit,shards_used,ram_limit_gb,ram_used_gb,nodes_count,flash_enabled,rack_awareness"
            );
            for item in report_data {
                if let Some(obj) = item.as_object() {
                    println!(
                        "{},{},{},{},{},{},{},{},{:.2},{:.2},{},{},{}",
                        obj.get("profile").and_then(|v| v.as_str()).unwrap_or(""),
                        obj.get("cluster_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(""),
                        obj.get("activation_date")
                            .and_then(|v| v.as_str())
                            .unwrap_or(""),
                        obj.get("expiration_date")
                            .and_then(|v| v.as_str())
                            .unwrap_or(""),
                        obj.get("days_remaining")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(-1),
                        obj.get("expired")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        obj.get("shards_limit")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        obj.get("shards_used").and_then(|v| v.as_i64()).unwrap_or(0),
                        obj.get("ram_limit_gb")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        obj.get("ram_used_gb")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        obj.get("nodes_count").and_then(|v| v.as_i64()).unwrap_or(0),
                        obj.get("flash_enabled")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        obj.get("rack_awareness")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    );
                }
            }
            Ok(())
        } else {
            println!("No enterprise profiles found");
            Ok(())
        }
    } else {
        let response = Value::Array(report_data);
        let response = if let Some(q) = query {
            super::utils::apply_jmespath(&response, q)?
        } else {
            response
        };

        super::utils::print_formatted_output(response, output_format)
            .map_err(|e| anyhow::anyhow!(e))
    }
}

pub async fn license_monitor(
    conn_mgr: &ConnectionManager,
    warning_days: i64,
    fail_on_warning: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for (profile_name, profile) in conn_mgr.config.profiles.iter() {
        if profile.deployment_type != redisctl_core::DeploymentType::Enterprise {
            continue;
        }

        match conn_mgr.create_enterprise_client(Some(profile_name)).await {
            Ok(client) => match client.get::<Value>("/v1/license").await {
                Ok(license) => {
                    let expired = license
                        .get("expired")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let expiration_date = license
                        .get("expiration_date")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let days_remaining = calculate_days_remaining(Some(expiration_date));

                    if expired {
                        errors.push(serde_json::json!({
                                "profile": profile_name,
                                "cluster_name": license.get("cluster_name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                "message": format!("License EXPIRED on {}", expiration_date),
                                "severity": "ERROR"
                            }));
                    } else if days_remaining >= 0 && days_remaining <= warning_days {
                        warnings.push(serde_json::json!({
                                "profile": profile_name,
                                "cluster_name": license.get("cluster_name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                "message": format!("License expiring in {} days ({})", days_remaining, expiration_date),
                                "severity": "WARNING"
                            }));
                    }
                }
                Err(e) => {
                    errors.push(serde_json::json!({
                        "profile": profile_name,
                        "message": format!("Failed to check license: {}", e),
                        "severity": "ERROR"
                    }));
                }
            },
            Err(e) => {
                errors.push(serde_json::json!({
                    "profile": profile_name,
                    "message": format!("Failed to connect: {}", e),
                    "severity": "ERROR"
                }));
            }
        }
    }

    let response = serde_json::json!({
        "summary": {
            "total_profiles_checked": conn_mgr.config.profiles.iter().filter(|(_, p)| p.deployment_type == redisctl_core::DeploymentType::Enterprise).count(),
            "warnings_count": warnings.len(),
            "errors_count": errors.len(),
            "status": if !errors.is_empty() {
                "ERROR"
            } else if !warnings.is_empty() {
                "WARNING"
            } else {
                "OK"
            }
        },
        "warnings": warnings,
        "errors": errors
    });

    let response = if let Some(q) = query {
        super::utils::apply_jmespath(&response, q)?
    } else {
        response
    };

    super::utils::print_formatted_output(response.clone(), output_format)
        .map_err(|e| anyhow::anyhow!(e))?;

    // Exit with error code if requested
    if fail_on_warning && (!warnings.is_empty() || !errors.is_empty()) {
        std::process::exit(1);
    }

    Ok(())
}
