use anyhow::Result as AnyhowResult;
use clap::Subcommand;
use redisctl_core::Config;
use serde_json::Value;

use crate::cli::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum LicenseWorkflowCommands {
    /// Audit licenses across all configured profiles
    Audit {
        /// Only show profiles with expiring licenses (within 30 days)
        #[arg(long)]
        expiring: bool,
        /// Only show profiles with expired licenses
        #[arg(long)]
        expired: bool,
    },
    /// Update license across multiple profiles
    #[command(name = "bulk-update")]
    BulkUpdate {
        /// Profiles to update (comma-separated, or 'all' for all enterprise profiles)
        #[arg(long)]
        profiles: String,
        /// License data as JSON string or @file.json
        #[arg(long)]
        data: String,
        /// Dry run - show what would be updated without making changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate license compliance report
    Report {
        /// Output format for report (csv for spreadsheet export)
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// Monitor license expiration and send alerts
    Monitor {
        /// Days before expiration to trigger warning
        #[arg(long, default_value = "30")]
        warning_days: i64,
        /// Exit with error code if any licenses are expiring
        #[arg(long)]
        fail_on_warning: bool,
    },
}

impl LicenseWorkflowCommands {
    pub async fn execute(
        &self,
        config: &Config,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> AnyhowResult<()> {
        match self {
            Self::Audit { expiring, expired } => {
                handle_license_audit(config, *expiring, *expired, output_format, query).await
            }
            Self::BulkUpdate {
                profiles,
                data,
                dry_run,
            } => handle_bulk_update(config, profiles, data, *dry_run, output_format, query).await,
            Self::Report { format } => {
                handle_license_report(config, format, output_format, query).await
            }
            Self::Monitor {
                warning_days,
                fail_on_warning,
            } => {
                handle_license_monitor(
                    config,
                    *warning_days,
                    *fail_on_warning,
                    output_format,
                    query,
                )
                .await
            }
        }
    }
}

async fn handle_license_audit(
    config: &Config,
    expiring_only: bool,
    expired_only: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let mut audit_results = Vec::new();
    let conn_manager = crate::connection::ConnectionManager::new(config.clone());

    // Get all enterprise profiles
    for (profile_name, profile) in config.profiles.iter() {
        if profile.deployment_type != redisctl_core::DeploymentType::Enterprise {
            continue;
        }

        // Try to get license info for this profile
        match conn_manager
            .create_enterprise_client(Some(profile_name))
            .await
        {
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
                        let days_remaining =
                            super::license::calculate_days_remaining(Some(expiration_date));
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
                            "ram_limit_gb": super::license::bytes_to_gb(
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

async fn handle_bulk_update(
    config: &Config,
    profiles: &str,
    data: &str,
    dry_run: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let conn_manager = crate::connection::ConnectionManager::new(config.clone());
    let license_data = super::utils::read_json_data(data)?;

    // Determine which profiles to update
    let target_profiles: Vec<String> = if profiles == "all" {
        config
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
        if !config.profiles.contains_key(&profile_name) {
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
            match conn_manager
                .create_enterprise_client(Some(&profile_name))
                .await
            {
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

async fn handle_license_report(
    config: &Config,
    format: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let conn_manager = crate::connection::ConnectionManager::new(config.clone());
    let mut report_data = Vec::new();

    for (profile_name, profile) in config.profiles.iter() {
        if profile.deployment_type != redisctl_core::DeploymentType::Enterprise {
            continue;
        }

        match conn_manager
            .create_enterprise_client(Some(profile_name))
            .await
        {
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
                        "days_remaining": super::license::calculate_days_remaining(
                            license.get("expiration_date").and_then(|v| v.as_str())
                        ),
                        "expired": license.get("expired").and_then(|v| v.as_bool()).unwrap_or(false),
                        "shards_limit": license.get("shards_limit").and_then(|v| v.as_i64()).unwrap_or(0),
                        "shards_used": cluster.get("shards_used").and_then(|v| v.as_i64()).unwrap_or(0),
                        "ram_limit_gb": super::license::bytes_to_gb(
                            license.get("ram_limit").and_then(|v| v.as_i64()).unwrap_or(0)
                        ),
                        "ram_used_gb": super::license::bytes_to_gb(
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

async fn handle_license_monitor(
    config: &Config,
    warning_days: i64,
    fail_on_warning: bool,
    output_format: OutputFormat,
    query: Option<&str>,
) -> AnyhowResult<()> {
    let conn_manager = crate::connection::ConnectionManager::new(config.clone());
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for (profile_name, profile) in config.profiles.iter() {
        if profile.deployment_type != redisctl_core::DeploymentType::Enterprise {
            continue;
        }

        match conn_manager
            .create_enterprise_client(Some(profile_name))
            .await
        {
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
                    let days_remaining =
                        super::license::calculate_days_remaining(Some(expiration_date));

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
            "total_profiles_checked": config.profiles.iter().filter(|(_, p)| p.deployment_type == redisctl_core::DeploymentType::Enterprise).count(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_license_workflow_command_structure() {
        // Test that all workflow commands can be constructed

        // Audit command
        let _cmd = LicenseWorkflowCommands::Audit {
            expiring: false,
            expired: false,
        };

        // Bulk update command
        let _cmd = LicenseWorkflowCommands::BulkUpdate {
            profiles: "all".to_string(),
            data: "{}".to_string(),
            dry_run: true,
        };

        // Report command
        let _cmd = LicenseWorkflowCommands::Report {
            format: "csv".to_string(),
        };

        // Monitor command
        let _cmd = LicenseWorkflowCommands::Monitor {
            warning_days: 30,
            fail_on_warning: false,
        };
    }
}
