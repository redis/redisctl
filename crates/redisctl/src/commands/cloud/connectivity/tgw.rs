//! Transit Gateway (TGW) command implementations

#![allow(dead_code)]

use super::ConnectivityOperationParams;
use crate::cli::{OutputFormat, TgwCommands};
use crate::commands::cloud::async_utils::handle_async_response;
use crate::commands::cloud::utils::{
    confirm_action, handle_output, print_formatted_output, read_file_input,
};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use redis_cloud::CloudClient;
use redis_cloud::connectivity::transit_gateway::{TgwAttachmentRequest, TransitGatewayHandler};

/// Parameters for TGW attachment create/update operations
#[derive(Debug, Default)]
pub struct TgwAttachmentParams {
    pub aws_account_id: Option<String>,
    pub tgw_id: Option<String>,
    pub cidrs: Vec<String>,
    pub data: Option<String>,
}

/// Handle TGW commands
pub async fn handle_tgw_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &TgwCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr
        .create_cloud_client(profile_name)
        .await
        .context("Failed to create Cloud client")?;

    match command {
        // Standard TGW operations
        TgwCommands::AttachmentsList { subscription_id } => {
            list_attachments(&client, *subscription_id, output_format, query).await
        }
        TgwCommands::AttachmentCreate {
            subscription_id,
            aws_account_id,
            tgw_id,
            cidrs,
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
            let attachment_params = TgwAttachmentParams {
                aws_account_id: aws_account_id.clone(),
                tgw_id: tgw_id.clone(),
                cidrs: cidrs.clone(),
                data: data.clone(),
            };
            create_attachment(&params, &attachment_params).await
        }
        TgwCommands::AttachmentCreateWithId {
            subscription_id,
            tgw_id,
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
            create_attachment_with_id(&params, tgw_id).await
        }
        TgwCommands::AttachmentUpdate {
            subscription_id,
            attachment_id,
            cidrs,
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
            let attachment_params = TgwAttachmentParams {
                aws_account_id: None,
                tgw_id: None,
                cidrs: cidrs.clone(),
                data: data.clone(),
            };
            update_attachment_cidrs(&params, attachment_id, &attachment_params).await
        }
        TgwCommands::AttachmentDelete {
            subscription_id,
            attachment_id,
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
            delete_attachment(&params, attachment_id, *yes).await
        }
        TgwCommands::InvitationsList { subscription_id } => {
            list_invitations(&client, *subscription_id, output_format, query).await
        }
        TgwCommands::InvitationAccept {
            subscription_id,
            invitation_id,
        } => {
            accept_invitation(
                &client,
                *subscription_id,
                invitation_id,
                output_format,
                query,
            )
            .await
        }
        TgwCommands::InvitationReject {
            subscription_id,
            invitation_id,
        } => {
            reject_invitation(
                &client,
                *subscription_id,
                invitation_id,
                output_format,
                query,
            )
            .await
        }

        // Active-Active TGW operations
        TgwCommands::AaAttachmentsList {
            subscription_id,
            region_id,
        } => list_attachments_aa(&client, *subscription_id, *region_id, output_format, query).await,
        TgwCommands::AaAttachmentCreate {
            subscription_id,
            region_id,
            aws_account_id,
            tgw_id,
            cidrs,
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
            let attachment_params = TgwAttachmentParams {
                aws_account_id: aws_account_id.clone(),
                tgw_id: tgw_id.clone(),
                cidrs: cidrs.clone(),
                data: data.clone(),
            };
            create_attachment_aa(&params, *region_id, &attachment_params).await
        }
        TgwCommands::AaAttachmentUpdate {
            subscription_id,
            region_id,
            attachment_id,
            cidrs,
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
            let attachment_params = TgwAttachmentParams {
                aws_account_id: None,
                tgw_id: None,
                cidrs: cidrs.clone(),
                data: data.clone(),
            };
            update_attachment_cidrs_aa(&params, *region_id, attachment_id, &attachment_params).await
        }
        TgwCommands::AaAttachmentDelete {
            subscription_id,
            region_id,
            attachment_id,
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
            delete_attachment_aa(&params, *region_id, attachment_id, *yes).await
        }
        TgwCommands::AaInvitationsList {
            subscription_id,
            region_id,
        } => list_invitations_aa(&client, *subscription_id, *region_id, output_format, query).await,
        TgwCommands::AaInvitationAccept {
            subscription_id,
            region_id,
            invitation_id,
        } => {
            accept_invitation_aa(
                &client,
                *subscription_id,
                *region_id,
                invitation_id,
                output_format,
                query,
            )
            .await
        }
        TgwCommands::AaInvitationReject {
            subscription_id,
            region_id,
            invitation_id,
        } => {
            reject_invitation_aa(
                &client,
                *subscription_id,
                *region_id,
                invitation_id,
                output_format,
                query,
            )
            .await
        }
    }
}

// ============================================================================
// Standard TGW Operations
// ============================================================================

async fn list_attachments(
    client: &CloudClient,
    subscription_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .get_attachments(subscription_id)
        .await
        .context("Failed to get TGW attachments")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Build TGW attachment request from parameters
fn build_tgw_attachment_request(
    attachment_params: &TgwAttachmentParams,
) -> CliResult<TgwAttachmentRequest> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &attachment_params.data {
        let json_string = read_file_input(data)?;
        let request: TgwAttachmentRequest =
            serde_json::from_str(&json_string).context("Invalid TGW attachment configuration")?;
        return Ok(request);
    }

    // Build from first-class parameters
    let cidrs = if attachment_params.cidrs.is_empty() {
        None
    } else {
        Some(attachment_params.cidrs.clone())
    };

    Ok(TgwAttachmentRequest {
        aws_account_id: attachment_params.aws_account_id.clone(),
        tgw_id: attachment_params.tgw_id.clone(),
        cidrs,
    })
}

async fn create_attachment(
    params: &ConnectivityOperationParams<'_>,
    attachment_params: &TgwAttachmentParams,
) -> CliResult<()> {
    // The generic create_attachment route was removed upstream; the TGW ID is
    // now required and is carried in the path.
    let tgw_id = attachment_params
        .tgw_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--tgw-id is required"))?;

    let handler = TransitGatewayHandler::new(params.client.clone());
    let response = handler
        .create_attachment_with_id(params.subscription_id, &tgw_id)
        .await
        .context("Failed to create TGW attachment")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "TGW attachment created successfully",
    )
    .await
}

async fn create_attachment_with_id(
    params: &ConnectivityOperationParams<'_>,
    tgw_id: &str,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(params.client.clone());
    let response = handler
        .create_attachment_with_id(params.subscription_id, tgw_id)
        .await
        .context("Failed to create TGW attachment")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "TGW attachment created successfully",
    )
    .await
}

async fn update_attachment_cidrs(
    params: &ConnectivityOperationParams<'_>,
    attachment_id: &str,
    attachment_params: &TgwAttachmentParams,
) -> CliResult<()> {
    let request = build_tgw_attachment_request(attachment_params)?;

    let handler = TransitGatewayHandler::new(params.client.clone());
    let response = handler
        .update_attachment_cidrs(params.subscription_id, attachment_id.to_string(), &request)
        .await
        .context("Failed to update TGW attachment CIDRs")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "TGW attachment updated successfully",
    )
    .await
}

async fn delete_attachment(
    params: &ConnectivityOperationParams<'_>,
    attachment_id: &str,
    yes: bool,
) -> CliResult<()> {
    if !yes {
        let prompt = format!(
            "Delete TGW attachment {} for subscription {}?",
            attachment_id, params.subscription_id
        );
        if !confirm_action(&prompt)? {
            eprintln!("Operation cancelled");
            return Ok(());
        }
    }

    let handler = TransitGatewayHandler::new(params.client.clone());
    handler
        .delete_attachment(params.subscription_id, attachment_id.to_string())
        .await
        .context("Failed to delete TGW attachment")?;

    eprintln!("TGW attachment deleted successfully");
    Ok(())
}

async fn list_invitations(
    client: &CloudClient,
    subscription_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .get_shared_invitations(subscription_id)
        .await
        .context("Failed to get TGW invitations")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn accept_invitation(
    client: &CloudClient,
    subscription_id: i32,
    invitation_id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .accept_resource_share(subscription_id, invitation_id.to_string())
        .await
        .context("Failed to accept TGW invitation")?;

    // Convert response to JSON and check for task ID
    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;
    if let Some(task_id) = json_response.get("taskId").and_then(|v| v.as_str()) {
        eprintln!("TGW invitation acceptance initiated. Task ID: {}", task_id);
        eprintln!(
            "Use 'redisctl cloud task wait {}' to monitor progress",
            task_id
        );
    }

    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn reject_invitation(
    client: &CloudClient,
    subscription_id: i32,
    invitation_id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .reject_resource_share(subscription_id, invitation_id.to_string())
        .await
        .context("Failed to reject TGW invitation")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

// ============================================================================
// Active-Active TGW Operations
// ============================================================================

async fn list_attachments_aa(
    client: &CloudClient,
    subscription_id: i32,
    region_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .get_attachments_active_active(subscription_id, region_id)
        .await
        .context("Failed to get Active-Active TGW attachments")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn create_attachment_aa(
    params: &ConnectivityOperationParams<'_>,
    region_id: i32,
    attachment_params: &TgwAttachmentParams,
) -> CliResult<()> {
    // The attachment route now requires the TGW ID in the path.
    let tgw_id = attachment_params
        .tgw_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--tgw-id is required"))?;
    let request = build_tgw_attachment_request(attachment_params)?;

    let handler = TransitGatewayHandler::new(params.client.clone());
    let response = handler
        .create_attachment_active_active(params.subscription_id, region_id, &tgw_id, &request)
        .await
        .context("Failed to create Active-Active TGW attachment")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active TGW attachment created successfully",
    )
    .await
}

async fn update_attachment_cidrs_aa(
    params: &ConnectivityOperationParams<'_>,
    region_id: i32,
    attachment_id: &str,
    attachment_params: &TgwAttachmentParams,
) -> CliResult<()> {
    let request = build_tgw_attachment_request(attachment_params)?;

    let handler = TransitGatewayHandler::new(params.client.clone());
    let response = handler
        .update_attachment_cidrs_active_active(
            params.subscription_id,
            region_id,
            attachment_id.to_string(),
            &request,
        )
        .await
        .context("Failed to update Active-Active TGW attachment CIDRs")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active TGW attachment updated successfully",
    )
    .await
}

async fn delete_attachment_aa(
    params: &ConnectivityOperationParams<'_>,
    region_id: i32,
    attachment_id: &str,
    yes: bool,
) -> CliResult<()> {
    if !yes {
        let prompt = format!(
            "Delete Active-Active TGW attachment {} in region {} for subscription {}?",
            attachment_id, region_id, params.subscription_id
        );
        if !confirm_action(&prompt)? {
            eprintln!("Operation cancelled");
            return Ok(());
        }
    }

    let handler = TransitGatewayHandler::new(params.client.clone());
    handler
        .delete_attachment_active_active(
            params.subscription_id,
            region_id,
            attachment_id.to_string(),
        )
        .await
        .context("Failed to delete Active-Active TGW attachment")?;

    eprintln!("Active-Active TGW attachment deleted successfully");
    Ok(())
}

async fn list_invitations_aa(
    client: &CloudClient,
    subscription_id: i32,
    region_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .get_shared_invitations_active_active(subscription_id, region_id)
        .await
        .context("Failed to get Active-Active TGW invitations")?;

    let json_response = serde_json::to_value(response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn accept_invitation_aa(
    client: &CloudClient,
    subscription_id: i32,
    region_id: i32,
    invitation_id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .accept_resource_share_active_active(subscription_id, region_id, invitation_id.to_string())
        .await
        .context("Failed to accept Active-Active TGW invitation")?;

    // Convert response to JSON and check for task ID
    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;
    if let Some(task_id) = json_response.get("taskId").and_then(|v| v.as_str()) {
        eprintln!(
            "Active-Active TGW invitation acceptance initiated. Task ID: {}",
            task_id
        );
        eprintln!(
            "Use 'redisctl cloud task wait {}' to monitor progress",
            task_id
        );
    }

    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

async fn reject_invitation_aa(
    client: &CloudClient,
    subscription_id: i32,
    region_id: i32,
    invitation_id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let handler = TransitGatewayHandler::new(client.clone());
    let response = handler
        .reject_resource_share_active_active(subscription_id, region_id, invitation_id.to_string())
        .await
        .context("Failed to reject Active-Active TGW invitation")?;

    let json_response = serde_json::to_value(&response).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}
