//! Node command router for Enterprise

use crate::cli::{EnterpriseNodeCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::node_impl;

pub async fn handle_node_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &EnterpriseNodeCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        // Node Operations
        EnterpriseNodeCommands::List => {
            node_impl::list_nodes(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseNodeCommands::Get { id } => {
            node_impl::get_node(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Add {
            address,
            username,
            password,
            data,
        } => {
            node_impl::add_node(
                conn_mgr,
                profile_name,
                address.as_deref(),
                username.as_deref(),
                password.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseNodeCommands::Remove { id, force } => {
            node_impl::remove_node(conn_mgr, profile_name, *id, *force, output_format, query).await
        }
        EnterpriseNodeCommands::Update {
            id,
            accept_servers,
            external_addr,
            rack_id,
            data,
        } => {
            node_impl::update_node(
                conn_mgr,
                profile_name,
                *id,
                *accept_servers,
                external_addr.clone(),
                rack_id.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }

        // Node Status & Health
        EnterpriseNodeCommands::Status { id } => {
            node_impl::get_node_status(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Stats { id } => {
            node_impl::get_node_stats(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Metrics { id, interval } => {
            node_impl::get_node_metrics(
                conn_mgr,
                profile_name,
                *id,
                interval.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseNodeCommands::Check { id } => {
            node_impl::check_node_health(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Alerts { id } => {
            node_impl::get_node_alerts(conn_mgr, profile_name, *id, output_format, query).await
        }

        // Node Maintenance
        EnterpriseNodeCommands::MaintenanceEnable { id } => {
            node_impl::enable_maintenance(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::MaintenanceDisable { id } => {
            node_impl::disable_maintenance(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Rebalance { id } => {
            node_impl::rebalance_node(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Drain { id } => {
            node_impl::drain_node(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Restart { id, force } => {
            node_impl::restart_node(conn_mgr, profile_name, *id, *force, output_format, query).await
        }

        // Node Configuration
        EnterpriseNodeCommands::GetConfig { id } => {
            node_impl::get_node_config(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::UpdateConfig {
            id,
            max_redis_servers,
            bigstore_driver,
            data,
        } => {
            node_impl::update_node_config(
                conn_mgr,
                profile_name,
                *id,
                *max_redis_servers,
                bigstore_driver.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseNodeCommands::GetRack { id } => {
            node_impl::get_node_rack(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::SetRack { id, rack } => {
            node_impl::set_node_rack(conn_mgr, profile_name, *id, rack, output_format, query).await
        }
        EnterpriseNodeCommands::GetRole { id } => {
            node_impl::get_node_role(conn_mgr, profile_name, *id, output_format, query).await
        }

        // Node Resources
        EnterpriseNodeCommands::Resources { id } => {
            node_impl::get_node_resources(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Memory { id } => {
            node_impl::get_node_memory(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Cpu { id } => {
            node_impl::get_node_cpu(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Storage { id } => {
            node_impl::get_node_storage(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseNodeCommands::Network { id } => {
            node_impl::get_node_network(conn_mgr, profile_name, *id, output_format, query).await
        }
    }
}
