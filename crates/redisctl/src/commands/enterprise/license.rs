//! Enterprise license command handler

use crate::cli::{EnterpriseLicenseCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};

use super::license_impl;

/// Handle enterprise license commands
pub async fn handle_license_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &EnterpriseLicenseCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = match command {
        EnterpriseLicenseCommands::Get => {
            license_impl::get_license(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseLicenseCommands::Update { license_key, data } => {
            license_impl::update_license(
                conn_mgr,
                profile_name,
                license_key.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseLicenseCommands::Upload { file } => {
            license_impl::upload_license(conn_mgr, profile_name, file, output_format, query).await
        }
        EnterpriseLicenseCommands::Validate { license_key, data } => {
            license_impl::validate_license(
                conn_mgr,
                profile_name,
                license_key.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseLicenseCommands::Expiry => {
            license_impl::license_expiry(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseLicenseCommands::Features => {
            license_impl::license_features(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseLicenseCommands::Usage => {
            license_impl::license_usage(conn_mgr, profile_name, output_format, query).await
        }
    };

    result.map_err(RedisCtlError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_license_command_structure() {
        // Test that all license commands can be constructed

        // Get command
        let _cmd = EnterpriseLicenseCommands::Get;

        // Update command
        let _cmd = EnterpriseLicenseCommands::Update {
            license_key: Some("ABC123".to_string()),
            data: None,
        };

        // Upload command
        let _cmd = EnterpriseLicenseCommands::Upload {
            file: "/path/to/license".to_string(),
        };

        // Validate command
        let _cmd = EnterpriseLicenseCommands::Validate {
            license_key: Some("ABC123".to_string()),
            data: None,
        };

        // Expiry command
        let _cmd = EnterpriseLicenseCommands::Expiry;

        // Features command
        let _cmd = EnterpriseLicenseCommands::Features;

        // Usage command
        let _cmd = EnterpriseLicenseCommands::Usage;
    }
}
