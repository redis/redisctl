//! AWS PrivateLink command implementations

#![allow(dead_code)]

use super::ConnectivityOperationParams;
use crate::cli::{OutputFormat, PrivateLinkCommands};
use crate::commands::cloud::async_utils::handle_async_response;
use crate::commands::cloud::utils::{handle_output, print_formatted_output, read_file_input};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use redis_cloud::PrivateLinkHandler;
use redis_cloud::connectivity::private_link::{
    PrincipalType, PrivateLinkAddPrincipalRequest, PrivateLinkCreateRequest,
    PrivateLinkRemovePrincipalRequest,
};

/// Parameters for PrivateLink create operation
#[derive(Debug, Default)]
pub struct PrivateLinkCreateParams {
    pub share_name: Option<String>,
    pub principal: Option<String>,
    pub principal_type: Option<String>,
    pub alias: Option<String>,
    pub data: Option<String>,
}

/// Parameters for PrivateLink principal operations
#[derive(Debug, Default)]
pub struct PrivateLinkPrincipalParams {
    pub principal: Option<String>,
    pub principal_type: Option<String>,
    pub alias: Option<String>,
    pub data: Option<String>,
}

/// Handle PrivateLink commands
pub async fn handle_private_link_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &PrivateLinkCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = PrivateLinkHandler::new(client.clone());

    match command {
        PrivateLinkCommands::Get {
            subscription,
            region,
        } => handle_get(&handler, *subscription, *region, output_format, query).await,
        PrivateLinkCommands::Create {
            subscription,
            region,
            share_name,
            principal,
            principal_type,
            alias,
            data,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription,
                async_ops,
                output_format,
                query,
            };
            let create_params = PrivateLinkCreateParams {
                share_name: share_name.clone(),
                principal: principal.clone(),
                principal_type: principal_type.clone(),
                alias: alias.clone(),
                data: data.clone(),
            };
            handle_create(&handler, &params, *region, &create_params).await
        }
        PrivateLinkCommands::AddPrincipal {
            subscription,
            region,
            principal,
            principal_type,
            alias,
            data,
        } => {
            let principal_params = PrivateLinkPrincipalParams {
                principal: principal.clone(),
                principal_type: principal_type.clone(),
                alias: alias.clone(),
                data: data.clone(),
            };
            handle_add_principal(
                &handler,
                *subscription,
                *region,
                &principal_params,
                output_format,
                query,
            )
            .await
        }
        PrivateLinkCommands::RemovePrincipal {
            subscription,
            region,
            principal,
            principal_type,
            alias,
            data,
        } => {
            let principal_params = PrivateLinkPrincipalParams {
                principal: principal.clone(),
                principal_type: principal_type.clone(),
                alias: alias.clone(),
                data: data.clone(),
            };
            handle_remove_principal(
                &handler,
                *subscription,
                *region,
                &principal_params,
                output_format,
                query,
            )
            .await
        }
        PrivateLinkCommands::GetScript {
            subscription,
            region,
        } => handle_get_script(&handler, *subscription, *region, output_format, query).await,
        PrivateLinkCommands::Delete {
            subscription,
            force,
            async_ops,
        } => {
            let params = ConnectivityOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                subscription_id: *subscription,
                async_ops,
                output_format,
                query,
            };
            handle_delete(&handler, &params, *force).await
        }
    }
}

/// Get PrivateLink configuration
async fn handle_get(
    handler: &PrivateLinkHandler,
    subscription_id: i32,
    region_id: Option<i32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = if let Some(region) = region_id {
        handler
            .get_active_active(subscription_id, region)
            .await
            .context("Failed to get Active-Active PrivateLink configuration")?
    } else {
        handler
            .get(subscription_id)
            .await
            .context("Failed to get PrivateLink configuration")?
    };

    let json_response = serde_json::to_value(result).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Parse principal type string into PrincipalType enum
fn parse_principal_type(s: &str) -> CliResult<PrincipalType> {
    // Normalize: convert dashes to underscores and lowercase
    let normalized = s.to_lowercase().replace('-', "_");
    match normalized.as_str() {
        "aws_account" | "awsaccount" => Ok(PrincipalType::AwsAccount),
        "organization" => Ok(PrincipalType::Organization),
        "organization_unit" | "organizationunit" => Ok(PrincipalType::OrganizationUnit),
        "iam_role" | "iamrole" => Ok(PrincipalType::IamRole),
        "iam_user" | "iamuser" => Ok(PrincipalType::IamUser),
        "service_principal" | "serviceprincipal" => Ok(PrincipalType::ServicePrincipal),
        _ => Err(anyhow::anyhow!(
            "Invalid principal type '{}'. Valid types: aws-account, organization, organization-unit, iam-role, iam-user, service-principal",
            s
        ).into()),
    }
}

/// Build PrivateLink create request from parameters
fn build_create_request(
    create_params: &PrivateLinkCreateParams,
) -> CliResult<PrivateLinkCreateRequest> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &create_params.data {
        let content = read_file_input(data)?;
        return Ok(serde_json::from_str(&content).context("Failed to parse JSON input")?);
    }

    // Build request from first-class parameters
    let share_name = create_params
        .share_name
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--share-name is required"))?;
    let principal = create_params
        .principal
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--principal is required"))?;
    let principal_type = create_params
        .principal_type
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--principal-type is required"))?;

    Ok(PrivateLinkCreateRequest {
        share_name,
        principal,
        principal_type: parse_principal_type(principal_type)?,
        alias: create_params.alias.clone(),
    })
}

/// Build PrivateLink add principal request from parameters
fn build_add_principal_request(
    principal_params: &PrivateLinkPrincipalParams,
) -> CliResult<PrivateLinkAddPrincipalRequest> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &principal_params.data {
        let content = read_file_input(data)?;
        return Ok(serde_json::from_str(&content).context("Failed to parse JSON input")?);
    }

    // Build request from first-class parameters
    let principal = principal_params
        .principal
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--principal is required"))?;

    let principal_type = principal_params
        .principal_type
        .as_ref()
        .map(|t| parse_principal_type(t))
        .transpose()?;

    Ok(PrivateLinkAddPrincipalRequest {
        principal,
        principal_type,
        alias: principal_params.alias.clone(),
    })
}

/// Build PrivateLink remove principal request from parameters
fn build_remove_principal_request(
    principal_params: &PrivateLinkPrincipalParams,
) -> CliResult<PrivateLinkRemovePrincipalRequest> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &principal_params.data {
        let content = read_file_input(data)?;
        return Ok(serde_json::from_str(&content).context("Failed to parse JSON input")?);
    }

    // Build request from first-class parameters
    let principal = principal_params
        .principal
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--principal is required"))?;

    let principal_type = principal_params
        .principal_type
        .as_ref()
        .map(|t| parse_principal_type(t))
        .transpose()?;

    Ok(PrivateLinkRemovePrincipalRequest {
        principal,
        principal_type,
        alias: principal_params.alias.clone(),
    })
}

/// Create PrivateLink
async fn handle_create(
    handler: &PrivateLinkHandler,
    params: &ConnectivityOperationParams<'_>,
    region_id: Option<i32>,
    create_params: &PrivateLinkCreateParams,
) -> CliResult<()> {
    let request = build_create_request(create_params)?;

    let result = if let Some(region) = region_id {
        handler
            .create_active_active(params.subscription_id, region, &request)
            .await
            .context("Failed to create Active-Active PrivateLink")?
    } else {
        handler
            .create(params.subscription_id, &request)
            .await
            .context("Failed to create PrivateLink")?
    };

    let json_response = serde_json::to_value(result).context("Failed to serialize response")?;
    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Create PrivateLink",
    )
    .await
}

/// Add principals to PrivateLink
async fn handle_add_principal(
    handler: &PrivateLinkHandler,
    subscription_id: i32,
    region_id: Option<i32>,
    principal_params: &PrivateLinkPrincipalParams,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let request = build_add_principal_request(principal_params)?;

    let result = if let Some(region) = region_id {
        handler
            .add_principals_active_active(subscription_id, region, &request)
            .await
            .context("Failed to add principals to Active-Active PrivateLink")?
    } else {
        handler
            .add_principals(subscription_id, &request)
            .await
            .context("Failed to add principals to PrivateLink")?
    };

    let json_response = serde_json::to_value(result).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Remove principals from PrivateLink
async fn handle_remove_principal(
    handler: &PrivateLinkHandler,
    subscription_id: i32,
    region_id: Option<i32>,
    principal_params: &PrivateLinkPrincipalParams,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let request = build_remove_principal_request(principal_params)?;

    let result = if let Some(region) = region_id {
        handler
            .remove_principals_active_active(subscription_id, region, &request)
            .await
            .context("Failed to remove principals from Active-Active PrivateLink")?
    } else {
        handler
            .remove_principals(subscription_id, &request)
            .await
            .context("Failed to remove principals from PrivateLink")?
    };

    let json_response = serde_json::to_value(result).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Get VPC endpoint creation script
async fn handle_get_script(
    handler: &PrivateLinkHandler,
    subscription_id: i32,
    region_id: Option<i32>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = if let Some(region) = region_id {
        handler
            .get_endpoint_script_active_active(subscription_id, region)
            .await
            .context("Failed to get Active-Active endpoint script")?
    } else {
        handler
            .get_endpoint_script(subscription_id)
            .await
            .context("Failed to get endpoint script")?
    };

    let json_response = serde_json::to_value(result).context("Failed to serialize response")?;
    let data = handle_output(json_response, output_format, query)?;
    print_formatted_output(data, output_format)?;
    Ok(())
}

/// Delete PrivateLink configuration
async fn handle_delete(
    handler: &PrivateLinkHandler,
    params: &ConnectivityOperationParams<'_>,
    force: bool,
) -> CliResult<()> {
    // Confirmation prompt unless --force is used
    if !force
        && !crate::commands::cloud::utils::confirm_action(&format!(
            "Are you sure you want to delete PrivateLink for subscription {}?",
            params.subscription_id
        ))?
    {
        println!("Delete operation cancelled");
        return Ok(());
    }

    let result = handler
        .delete(params.subscription_id)
        .await
        .context("Failed to delete PrivateLink")?;

    let json_response = serde_json::to_value(result).context("Failed to serialize response")?;
    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        json_response,
        params.async_ops,
        params.output_format,
        params.query,
        "Delete PrivateLink",
    )
    .await
}
