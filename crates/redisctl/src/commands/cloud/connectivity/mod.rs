//! Cloud connectivity command implementations

pub mod private_link;
pub mod psc;
pub mod tgw;
pub mod vpc_peering;

use crate::cli::{CloudConnectivityCommands, OutputFormat};
use crate::commands::cloud::async_utils::AsyncOperationArgs;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use redis_cloud::CloudClient;

/// Common parameters for connectivity operations
pub struct ConnectivityOperationParams<'a> {
    pub conn_mgr: &'a ConnectionManager,
    pub profile_name: Option<&'a str>,
    pub client: &'a CloudClient,
    pub subscription_id: i32,
    pub async_ops: &'a AsyncOperationArgs,
    pub output_format: OutputFormat,
    pub query: Option<&'a str>,
}

/// Handle connectivity commands
pub async fn handle_connectivity_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudConnectivityCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        CloudConnectivityCommands::VpcPeering(vpc_cmd) => {
            vpc_peering::handle_vpc_peering_command(
                conn_mgr,
                profile_name,
                vpc_cmd,
                output_format,
                query,
            )
            .await
        }
        CloudConnectivityCommands::Psc(psc_cmd) => {
            psc::handle_psc_command(conn_mgr, profile_name, psc_cmd, output_format, query).await
        }
        CloudConnectivityCommands::Tgw(tgw_cmd) => {
            tgw::handle_tgw_command(conn_mgr, profile_name, tgw_cmd, output_format, query).await
        }
        CloudConnectivityCommands::PrivateLink(pl_cmd) => {
            private_link::handle_private_link_command(
                conn_mgr,
                profile_name,
                pl_cmd,
                output_format,
                query,
            )
            .await
        }
    }
}
