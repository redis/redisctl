//! Enterprise license workflow command handler

use crate::cli::{EnterpriseLicenseWorkflowCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};

use super::license_workflow_impl;

/// Handle enterprise license workflow commands
pub async fn handle_license_workflow_command(
    conn_mgr: &ConnectionManager,
    command: &EnterpriseLicenseWorkflowCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = match command {
        EnterpriseLicenseWorkflowCommands::Audit { expiring, expired } => {
            license_workflow_impl::license_audit(
                conn_mgr,
                *expiring,
                *expired,
                output_format,
                query,
            )
            .await
        }
        EnterpriseLicenseWorkflowCommands::BulkUpdate {
            profiles,
            data,
            dry_run,
        } => {
            license_workflow_impl::bulk_update(
                conn_mgr,
                profiles,
                data,
                *dry_run,
                output_format,
                query,
            )
            .await
        }
        EnterpriseLicenseWorkflowCommands::Report { format } => {
            license_workflow_impl::license_report(conn_mgr, format, output_format, query).await
        }
        EnterpriseLicenseWorkflowCommands::Monitor {
            warning_days,
            fail_on_warning,
        } => {
            license_workflow_impl::license_monitor(
                conn_mgr,
                *warning_days,
                *fail_on_warning,
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
    async fn test_license_workflow_command_structure() {
        // Test that all workflow commands can be constructed

        // Audit command
        let _cmd = EnterpriseLicenseWorkflowCommands::Audit {
            expiring: false,
            expired: false,
        };

        // Bulk update command
        let _cmd = EnterpriseLicenseWorkflowCommands::BulkUpdate {
            profiles: "all".to_string(),
            data: "{}".to_string(),
            dry_run: true,
        };

        // Report command
        let _cmd = EnterpriseLicenseWorkflowCommands::Report {
            format: "csv".to_string(),
        };

        // Monitor command
        let _cmd = EnterpriseLicenseWorkflowCommands::Monitor {
            warning_days: 30,
            fail_on_warning: false,
        };
    }
}
