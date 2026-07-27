//! Enterprise alerts command handler

use crate::cli::{EnterpriseAlertsCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};

use super::alerts_impl;

/// Handle enterprise alerts commands
pub async fn handle_alerts_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &EnterpriseAlertsCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = match command {
        EnterpriseAlertsCommands::List {
            filter_type,
            severity,
        } => {
            alerts_impl::list_alerts(
                conn_mgr,
                profile_name,
                filter_type.as_deref(),
                severity.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseAlertsCommands::Get { uid } => {
            alerts_impl::get_alert(conn_mgr, profile_name, *uid, output_format, query).await
        }
        EnterpriseAlertsCommands::Cluster { alert } => {
            alerts_impl::cluster_alerts(
                conn_mgr,
                profile_name,
                alert.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseAlertsCommands::Node { node_uid, alert } => {
            alerts_impl::node_alerts(
                conn_mgr,
                profile_name,
                *node_uid,
                alert.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseAlertsCommands::Database { bdb_uid, alert } => {
            alerts_impl::database_alerts(
                conn_mgr,
                profile_name,
                *bdb_uid,
                alert.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseAlertsCommands::SettingsGet => {
            alerts_impl::get_alert_settings(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseAlertsCommands::SettingsUpdate {
            cluster_alerts,
            node_alerts,
            bdb_alerts,
            memory_threshold,
            cpu_threshold,
            data,
        } => {
            alerts_impl::update_alert_settings(
                conn_mgr,
                profile_name,
                *cluster_alerts,
                *node_alerts,
                *bdb_alerts,
                *memory_threshold,
                *cpu_threshold,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
    };

    result.map_err(RedisCtlError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alerts_command_structure() {
        // Test that all alerts commands can be constructed

        // List command
        let _cmd = EnterpriseAlertsCommands::List {
            filter_type: None,
            severity: None,
        };

        let _cmd = EnterpriseAlertsCommands::List {
            filter_type: Some("cluster".to_string()),
            severity: Some("error".to_string()),
        };

        // Get command
        let _cmd = EnterpriseAlertsCommands::Get { uid: 1 };

        // Cluster alerts
        let _cmd = EnterpriseAlertsCommands::Cluster { alert: None };
        let _cmd = EnterpriseAlertsCommands::Cluster {
            alert: Some("test_alert".to_string()),
        };

        // Node alerts
        let _cmd = EnterpriseAlertsCommands::Node {
            node_uid: None,
            alert: None,
        };
        let _cmd = EnterpriseAlertsCommands::Node {
            node_uid: Some(1),
            alert: Some("test".to_string()),
        };

        // Database alerts
        let _cmd = EnterpriseAlertsCommands::Database {
            bdb_uid: None,
            alert: None,
        };
        let _cmd = EnterpriseAlertsCommands::Database {
            bdb_uid: Some(1),
            alert: Some("test".to_string()),
        };

        // Settings commands
        let _cmd = EnterpriseAlertsCommands::SettingsGet;
        let _cmd = EnterpriseAlertsCommands::SettingsUpdate {
            cluster_alerts: Some(true),
            node_alerts: None,
            bdb_alerts: None,
            memory_threshold: None,
            cpu_threshold: None,
            data: None,
        };
    }
}
