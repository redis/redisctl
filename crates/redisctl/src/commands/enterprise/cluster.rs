//! Cluster command router for Enterprise

use crate::cli::{EnterpriseClusterCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::cluster_impl;

pub async fn handle_cluster_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &EnterpriseClusterCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        // Cluster Configuration
        EnterpriseClusterCommands::Get => {
            cluster_impl::get_cluster(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::Update {
            name,
            email_alerts,
            rack_aware,
            data,
        } => {
            cluster_impl::update_cluster(
                conn_mgr,
                profile_name,
                name.as_deref(),
                *email_alerts,
                *rack_aware,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseClusterCommands::GetPolicy => {
            cluster_impl::get_cluster_policy(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::UpdatePolicy {
            default_shards_placement,
            rack_aware,
            default_redis_version,
            persistent_node_removal,
            data,
        } => {
            cluster_impl::update_cluster_policy(
                conn_mgr,
                profile_name,
                default_shards_placement.as_deref(),
                *rack_aware,
                default_redis_version.as_deref(),
                *persistent_node_removal,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseClusterCommands::GetLicense => {
            cluster_impl::get_cluster_license(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::UpdateLicense { license } => {
            cluster_impl::update_cluster_license(
                conn_mgr,
                profile_name,
                license,
                output_format,
                query,
            )
            .await
        }

        // Cluster Operations
        EnterpriseClusterCommands::Bootstrap {
            cluster_name,
            username,
            password,
            data,
            dry_run,
        } => {
            cluster_impl::bootstrap_cluster(
                conn_mgr,
                profile_name,
                cluster_name.as_deref(),
                username.as_deref(),
                password.as_deref(),
                data.as_deref(),
                *dry_run,
                output_format,
                query,
            )
            .await
        }
        EnterpriseClusterCommands::Join {
            nodes,
            username,
            password,
            data,
        } => {
            cluster_impl::join_cluster(
                conn_mgr,
                profile_name,
                nodes,
                username.as_deref(),
                password.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseClusterCommands::Recover { data } => {
            cluster_impl::recover_cluster(
                conn_mgr,
                profile_name,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseClusterCommands::Reset { force } => {
            cluster_impl::reset_cluster(conn_mgr, profile_name, *force, output_format, query).await
        }

        // Cluster Monitoring
        EnterpriseClusterCommands::Stats => {
            cluster_impl::get_cluster_stats(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::Metrics { interval } => {
            cluster_impl::get_cluster_metrics(
                conn_mgr,
                profile_name,
                interval.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseClusterCommands::Alerts => {
            cluster_impl::get_cluster_alerts(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::Events { limit } => {
            cluster_impl::get_cluster_events(conn_mgr, profile_name, *limit, output_format, query)
                .await
        }
        EnterpriseClusterCommands::AuditLog { from } => {
            cluster_impl::get_audit_log(
                conn_mgr,
                profile_name,
                from.as_deref(),
                output_format,
                query,
            )
            .await
        }

        // Cluster Maintenance
        EnterpriseClusterCommands::MaintenanceModeEnable => {
            cluster_impl::enable_maintenance_mode(conn_mgr, profile_name, output_format, query)
                .await
        }
        EnterpriseClusterCommands::MaintenanceModeDisable => {
            cluster_impl::disable_maintenance_mode(conn_mgr, profile_name, output_format, query)
                .await
        }
        EnterpriseClusterCommands::CheckStatus => {
            cluster_impl::check_cluster_status(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::Health => {
            cluster_impl::cluster_health(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::VerifyBalance => {
            cluster_impl::verify_balance(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::VerifyRackAwareness => {
            cluster_impl::verify_rack_awareness(conn_mgr, profile_name, output_format, query).await
        }

        // Certificates & Security
        EnterpriseClusterCommands::GetCertificates => {
            cluster_impl::get_cluster_certificates(conn_mgr, profile_name, output_format, query)
                .await
        }
        EnterpriseClusterCommands::UpdateCertificates {
            name,
            certificate,
            key,
            data,
        } => {
            cluster_impl::update_cluster_certificates(
                conn_mgr,
                profile_name,
                name.as_deref(),
                certificate.as_deref(),
                key.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseClusterCommands::RotateCertificates => {
            cluster_impl::rotate_certificates(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::GetOcsp => {
            cluster_impl::get_ocsp_config(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseClusterCommands::UpdateOcsp {
            enabled,
            responder_url,
            response_timeout,
            query_frequency,
            recovery_frequency,
            recovery_max_tries,
            data,
        } => {
            cluster_impl::update_ocsp_config(
                conn_mgr,
                profile_name,
                *enabled,
                responder_url.as_deref(),
                *response_timeout,
                *query_frequency,
                *recovery_frequency,
                *recovery_max_tries,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
    }
}
