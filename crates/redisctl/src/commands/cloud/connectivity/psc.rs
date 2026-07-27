//! Private Service Connect (PSC) command implementations

use super::ConnectivityOperationParams;
use crate::cli::{OutputFormat, PscCommands};
use crate::commands::cloud::async_utils::handle_async_response;
use crate::commands::cloud::utils::{
    confirm_action, handle_output, print_formatted_output, read_file_input,
};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use redis_cloud::CloudClient;
use redis_cloud::connectivity::psc::{PscEndpointUpdateRequest, PscHandler};

/// Parameters for PSC endpoint create/update operations
#[derive(Debug, Default)]
pub struct PscEndpointParams {
    pub gcp_project_id: Option<String>,
    pub gcp_vpc_name: Option<String>,
    pub gcp_vpc_subnet_name: Option<String>,
    pub endpoint_connection_name: Option<String>,
    pub psc_service_id: Option<i32>,
    pub data: Option<String>,
}

/// Handle PSC commands
pub async fn handle_psc_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &PscCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr
        .create_cloud_client(profile_name)
        .await
        .context("Failed to create Cloud client")?;

    match command {
        // Standard PSC Service operations
        PscCommands::ServiceGet { subscription_id } => {
            get_service(&client, *subscription_id, output_format, query).await
        }
        PscCommands::ServiceCreate {
            subscription_id,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            create_service(&params).await
        }
        PscCommands::ServiceDelete {
            subscription_id,
            yes,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            delete_service(&params, *yes).await
        }

        // Standard PSC Endpoint operations
        PscCommands::EndpointsList {
            subscription_id,
            psc_service_id,
        } => {
            get_endpoints(
                &client,
                *subscription_id,
                *psc_service_id,
                output_format,
                query,
            )
            .await
        }
        PscCommands::EndpointCreate {
            subscription_id,
            psc_service_id,
            gcp_project_id,
            gcp_vpc_name,
            gcp_vpc_subnet_name,
            endpoint_connection_name,
            data,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            let endpoint_params = PscEndpointParams {
                gcp_project_id: gcp_project_id.clone(),
                gcp_vpc_name: gcp_vpc_name.clone(),
                gcp_vpc_subnet_name: gcp_vpc_subnet_name.clone(),
                endpoint_connection_name: endpoint_connection_name.clone(),
                psc_service_id: Some(*psc_service_id),
                data: data.clone(),
            };
            create_endpoint(&params, *psc_service_id, &endpoint_params).await
        }
        PscCommands::EndpointUpdate {
            subscription_id,
            endpoint_id,
            psc_service_id,
            gcp_project_id,
            gcp_vpc_name,
            gcp_vpc_subnet_name,
            endpoint_connection_name,
            data,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            let endpoint_params = PscEndpointParams {
                gcp_project_id: gcp_project_id.clone(),
                gcp_vpc_name: gcp_vpc_name.clone(),
                gcp_vpc_subnet_name: gcp_vpc_subnet_name.clone(),
                endpoint_connection_name: endpoint_connection_name.clone(),
                psc_service_id: Some(*psc_service_id),
                data: data.clone(),
            };
            update_endpoint(&params, *psc_service_id, *endpoint_id, &endpoint_params).await
        }
        PscCommands::EndpointDelete {
            subscription_id,
            psc_service_id,
            endpoint_id,
            yes,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            delete_endpoint(&params, *psc_service_id, *endpoint_id, *yes).await
        }
        PscCommands::EndpointCreationScript {
            subscription_id,
            psc_service_id,
            endpoint_id,
        } => {
            get_endpoint_creation_script(&client, *subscription_id, *psc_service_id, *endpoint_id)
                .await
        }
        PscCommands::EndpointDeletionScript {
            subscription_id,
            psc_service_id,
            endpoint_id,
        } => {
            get_endpoint_deletion_script(&client, *subscription_id, *psc_service_id, *endpoint_id)
                .await
        }

        // Active-Active PSC Service operations
        PscCommands::AaServiceGet {
            subscription_id,
            region_id,
        } => get_service_aa(&client, *subscription_id, *region_id, output_format, query).await,
        PscCommands::AaServiceCreate {
            subscription_id,
            region_id,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            create_service_aa(&params, *region_id).await
        }
        PscCommands::AaServiceDelete {
            subscription_id,
            region_id,
            yes,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            delete_service_aa(&params, *region_id, *yes).await
        }

        // Active-Active PSC Endpoint operations
        PscCommands::AaEndpointsList {
            subscription_id,
            region_id,
            psc_service_id,
        } => {
            get_endpoints_aa(
                &client,
                *subscription_id,
                *region_id,
                *psc_service_id,
                output_format,
                query,
            )
            .await
        }
        PscCommands::AaEndpointCreate {
            subscription_id,
            region_id,
            psc_service_id,
            gcp_project_id,
            gcp_vpc_name,
            gcp_vpc_subnet_name,
            endpoint_connection_name,
            data,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            let endpoint_params = PscEndpointParams {
                gcp_project_id: gcp_project_id.clone(),
                gcp_vpc_name: gcp_vpc_name.clone(),
                gcp_vpc_subnet_name: gcp_vpc_subnet_name.clone(),
                endpoint_connection_name: endpoint_connection_name.clone(),
                psc_service_id: Some(*psc_service_id),
                data: data.clone(),
            };
            create_endpoint_aa(&params, *region_id, *psc_service_id, &endpoint_params).await
        }
        PscCommands::AaEndpointDelete {
            subscription_id,
            region_id,
            psc_service_id,
            endpoint_id,
            yes,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription_id,
                async_ops,
                output_format,
                query,
            };
            delete_endpoint_aa(&params, *region_id, *psc_service_id, *endpoint_id, *yes).await
        }
    }
}

// ============================================================================
// Standard PSC Service Operations
// ============================================================================

async fn get_service(
    client: &CloudClient,
    subscription_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = PscHandler::new(client.clone());
    let response = handler
        .get_service(subscription_id)
        .await
        .context("Failed to get PSC service")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn create_service(params: &ConnectivityOperationParams<'_>) -> CliResult<()> {
    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .create_service(params.subscription_id)
        .await
        .context("Failed to create PSC service")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "PSC service created successfully",
    )
    .await
}

async fn delete_service(params: &ConnectivityOperationParams<'_>, yes: bool) -> CliResult<()> {
    if !yes {
        let prompt = format!(
            "Delete PSC service for subscription {}?",
            params.subscription_id
        );
        confirm_action(&prompt)?;
    }

    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .delete_service(params.subscription_id)
        .await
        .context("Failed to delete PSC service")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "PSC service deleted successfully",
    )
    .await
}

// ============================================================================
// Standard PSC Endpoint Operations
// ============================================================================

async fn get_endpoints(
    client: &CloudClient,
    subscription_id: i32,
    psc_service_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = PscHandler::new(client.clone());
    let response = handler
        .get_endpoints(subscription_id, psc_service_id)
        .await
        .context("Failed to get PSC endpoints")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Build PSC endpoint request from parameters
fn build_psc_endpoint_request(
    subscription_id: i32,
    endpoint_id: i32,
    endpoint_params: &PscEndpointParams,
) -> CliResult<PscEndpointUpdateRequest> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &endpoint_params.data {
        let json_string = read_file_input(data)?;
        let mut request: PscEndpointUpdateRequest =
            serde_json::from_str(&json_string).context("Invalid PSC endpoint configuration")?;
        request.subscription_id = subscription_id;
        request.endpoint_id = endpoint_id;
        return Ok(request);
    }

    // Build from first-class parameters
    Ok(PscEndpointUpdateRequest {
        subscription_id,
        psc_service_id: endpoint_params.psc_service_id.unwrap_or(0),
        endpoint_id,
        gcp_project_id: endpoint_params.gcp_project_id.clone(),
        gcp_vpc_name: endpoint_params.gcp_vpc_name.clone(),
        gcp_vpc_subnet_name: endpoint_params.gcp_vpc_subnet_name.clone(),
        endpoint_connection_name: endpoint_params.endpoint_connection_name.clone(),
    })
}

async fn create_endpoint(
    params: &ConnectivityOperationParams<'_>,
    psc_service_id: i32,
    endpoint_params: &PscEndpointParams,
) -> CliResult<()> {
    let request = build_psc_endpoint_request(params.subscription_id, 0, endpoint_params)?;

    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .create_endpoint(params.subscription_id, psc_service_id, &request)
        .await
        .context("Failed to create PSC endpoint")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "PSC endpoint created successfully",
    )
    .await
}

async fn update_endpoint(
    params: &ConnectivityOperationParams<'_>,
    psc_service_id: i32,
    endpoint_id: i32,
    endpoint_params: &PscEndpointParams,
) -> CliResult<()> {
    let request = build_psc_endpoint_request(params.subscription_id, endpoint_id, endpoint_params)?;

    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .update_endpoint(
            params.subscription_id,
            psc_service_id,
            endpoint_id,
            &request,
        )
        .await
        .context("Failed to update PSC endpoint")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "PSC endpoint updated successfully",
    )
    .await
}

async fn delete_endpoint(
    params: &ConnectivityOperationParams<'_>,
    psc_service_id: i32,
    endpoint_id: i32,
    yes: bool,
) -> CliResult<()> {
    if !yes {
        let prompt = format!(
            "Delete PSC endpoint {} for subscription {}?",
            endpoint_id, params.subscription_id
        );
        confirm_action(&prompt)?;
    }

    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .delete_endpoint(params.subscription_id, psc_service_id, endpoint_id)
        .await
        .context("Failed to delete PSC endpoint")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "PSC endpoint deleted successfully",
    )
    .await
}

async fn get_endpoint_creation_script(
    client: &CloudClient,
    subscription_id: i32,
    psc_service_id: i32,
    endpoint_id: i32,
) -> CliResult<()> {
    let handler = PscHandler::new(client.clone());
    let script = handler
        .get_endpoint_creation_script(subscription_id, psc_service_id, endpoint_id)
        .await
        .context("Failed to get creation script")?;

    // Scripts are always returned as plain text
    println!("{}", script);
    Ok(())
}

async fn get_endpoint_deletion_script(
    client: &CloudClient,
    subscription_id: i32,
    psc_service_id: i32,
    endpoint_id: i32,
) -> CliResult<()> {
    let handler = PscHandler::new(client.clone());
    let script = handler
        .get_endpoint_deletion_script(subscription_id, psc_service_id, endpoint_id)
        .await
        .context("Failed to get deletion script")?;

    // Scripts are always returned as plain text
    println!("{}", script);
    Ok(())
}

// ============================================================================
// Active-Active PSC Service Operations
// ============================================================================

async fn get_service_aa(
    client: &CloudClient,
    subscription_id: i32,
    region_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = PscHandler::new(client.clone());
    let response = handler
        .get_service_active_active(subscription_id, region_id)
        .await
        .context("Failed to get Active-Active PSC service")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn create_service_aa(
    params: &ConnectivityOperationParams<'_>,
    region_id: i32,
) -> CliResult<()> {
    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .create_service_active_active(params.subscription_id, region_id)
        .await
        .context("Failed to create Active-Active PSC service")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active PSC service created successfully",
    )
    .await
}

async fn delete_service_aa(
    params: &ConnectivityOperationParams<'_>,
    region_id: i32,
    yes: bool,
) -> CliResult<()> {
    if !yes {
        let prompt = format!(
            "Delete Active-Active PSC service for subscription {}?",
            params.subscription_id
        );
        confirm_action(&prompt)?;
    }

    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .delete_service_active_active(params.subscription_id, region_id)
        .await
        .context("Failed to delete Active-Active PSC service")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active PSC service deleted successfully",
    )
    .await
}

// ============================================================================
// Active-Active PSC Endpoint Operations
// ============================================================================

async fn get_endpoints_aa(
    client: &CloudClient,
    subscription_id: i32,
    region_id: i32,
    psc_service_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = PscHandler::new(client.clone());
    let response = handler
        .get_endpoints_active_active(subscription_id, region_id, psc_service_id)
        .await
        .context("Failed to get Active-Active PSC endpoints")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn create_endpoint_aa(
    params: &ConnectivityOperationParams<'_>,
    region_id: i32,
    psc_service_id: i32,
    endpoint_params: &PscEndpointParams,
) -> CliResult<()> {
    let request = build_psc_endpoint_request(params.subscription_id, 0, endpoint_params)?;

    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .create_endpoint_active_active(params.subscription_id, region_id, psc_service_id, &request)
        .await
        .context("Failed to create Active-Active PSC endpoint")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active PSC endpoint created successfully",
    )
    .await
}

async fn delete_endpoint_aa(
    params: &ConnectivityOperationParams<'_>,
    region_id: i32,
    psc_service_id: i32,
    endpoint_id: i32,
    yes: bool,
) -> CliResult<()> {
    if !yes {
        let prompt = format!(
            "Delete Active-Active PSC endpoint {} in region {} for subscription {}?",
            endpoint_id, region_id, params.subscription_id
        );
        confirm_action(&prompt)?;
    }

    let handler = PscHandler::new(params.client.clone());
    let response = handler
        .delete_endpoint_active_active(
            params.subscription_id,
            region_id,
            psc_service_id,
            endpoint_id,
        )
        .await
        .context("Failed to delete Active-Active PSC endpoint")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active PSC endpoint deleted successfully",
    )
    .await
}
