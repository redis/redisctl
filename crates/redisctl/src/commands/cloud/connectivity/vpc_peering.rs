//! VPC Peering command implementations

use super::ConnectivityOperationParams;
use crate::cli::{OutputFormat, VpcPeeringCommands};
use crate::commands::cloud::async_utils::handle_async_response;
use crate::commands::cloud::utils::{
    confirm_action, handle_output, print_formatted_output, read_file_input,
};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use redis_cloud::CloudClient;
use serde_json::Value;

/// Parameters for VPC peering create operation
#[derive(Debug, Default)]
pub struct VpcPeeringCreateParams {
    pub region: Option<String>,
    pub aws_account_id: Option<String>,
    pub vpc_id: Option<String>,
    pub gcp_project_id: Option<String>,
    pub gcp_network_name: Option<String>,
    pub vpc_cidrs: Vec<String>,
    pub data: Option<String>,
}

/// Parameters for Active-Active VPC peering create operation
#[derive(Debug, Default)]
pub struct VpcPeeringCreateAaParams {
    pub source_region: Option<String>,
    pub destination_region: Option<String>,
    pub aws_account_id: Option<String>,
    pub vpc_id: Option<String>,
    pub gcp_project_id: Option<String>,
    pub gcp_network_name: Option<String>,
    pub vpc_cidrs: Vec<String>,
    pub data: Option<String>,
}

/// Parameters for VPC peering update operation
#[derive(Debug, Default)]
pub struct VpcPeeringUpdateParams {
    pub vpc_cidrs: Vec<String>,
    pub data: Option<String>,
}

/// Handle VPC peering commands
pub async fn handle_vpc_peering_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &VpcPeeringCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    match command {
        VpcPeeringCommands::Get { subscription } => {
            handle_get(&client, *subscription, output_format, query).await
        }
        VpcPeeringCommands::Create {
            subscription,
            region,
            aws_account_id,
            vpc_id,
            gcp_project_id,
            gcp_network_name,
            vpc_cidrs,
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
            let create_params = VpcPeeringCreateParams {
                region: region.clone(),
                aws_account_id: aws_account_id.clone(),
                vpc_id: vpc_id.clone(),
                gcp_project_id: gcp_project_id.clone(),
                gcp_network_name: gcp_network_name.clone(),
                vpc_cidrs: vpc_cidrs.clone(),
                data: data.clone(),
            };
            handle_create(&params, &create_params).await
        }
        VpcPeeringCommands::Update {
            subscription,
            peering_id,
            vpc_cidrs,
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
            let update_params = VpcPeeringUpdateParams {
                vpc_cidrs: vpc_cidrs.clone(),
                data: data.clone(),
            };
            handle_update(&params, *peering_id, &update_params).await
        }
        VpcPeeringCommands::Delete {
            subscription,
            peering_id,
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
            handle_delete(&params, *peering_id, *force).await
        }
        VpcPeeringCommands::ListActiveActive { subscription } => {
            handle_list_active_active(&client, *subscription, output_format, query).await
        }
        VpcPeeringCommands::CreateActiveActive {
            subscription,
            source_region,
            destination_region,
            aws_account_id,
            vpc_id,
            gcp_project_id,
            gcp_network_name,
            vpc_cidrs,
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
            let create_params = VpcPeeringCreateAaParams {
                source_region: source_region.clone(),
                destination_region: destination_region.clone(),
                aws_account_id: aws_account_id.clone(),
                vpc_id: vpc_id.clone(),
                gcp_project_id: gcp_project_id.clone(),
                gcp_network_name: gcp_network_name.clone(),
                vpc_cidrs: vpc_cidrs.clone(),
                data: data.clone(),
            };
            handle_create_active_active(&params, &create_params).await
        }
        VpcPeeringCommands::UpdateActiveActive {
            subscription,
            peering_id,
            vpc_cidrs,
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
            let update_params = VpcPeeringUpdateParams {
                vpc_cidrs: vpc_cidrs.clone(),
                data: data.clone(),
            };
            handle_update_active_active(&params, *peering_id, &update_params).await
        }
        VpcPeeringCommands::DeleteActiveActive {
            subscription,
            peering_id,
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
            handle_delete_active_active(&params, *peering_id, *force).await
        }
    }
}

/// Get VPC peering details
async fn handle_get(
    client: &CloudClient,
    subscription_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = client
        .get_raw(&format!("/subscriptions/{}/peerings/vpc", subscription_id))
        .await
        .context("Failed to get VPC peering")?;

    let data = handle_output(result, output_format, query)?;

    if matches!(output_format, OutputFormat::Table) && query.is_none() {
        print_vpc_peering_table(&data)?;
    } else {
        print_formatted_output(data, output_format)?;
    }

    Ok(())
}

/// Build VPC peering create payload from parameters
fn build_create_payload(create_params: &VpcPeeringCreateParams) -> CliResult<Value> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &create_params.data {
        let content = read_file_input(data)?;
        return Ok(serde_json::from_str(&content).context("Failed to parse JSON input")?);
    }

    // Build payload from first-class parameters
    let mut payload = serde_json::Map::new();

    // Determine if this is AWS or GCP based on provided parameters
    if create_params.gcp_project_id.is_some() {
        // GCP VPC peering
        if let Some(project_id) = &create_params.gcp_project_id {
            payload.insert(
                "vpcProjectUid".to_string(),
                Value::String(project_id.clone()),
            );
        }
        if let Some(network_name) = &create_params.gcp_network_name {
            payload.insert(
                "vpcNetworkName".to_string(),
                Value::String(network_name.clone()),
            );
        }
    } else {
        // AWS VPC peering
        if let Some(region) = &create_params.region {
            payload.insert("region".to_string(), Value::String(region.clone()));
        }
        if let Some(account_id) = &create_params.aws_account_id {
            payload.insert(
                "awsAccountId".to_string(),
                Value::String(account_id.clone()),
            );
        }
        if let Some(vpc_id) = &create_params.vpc_id {
            payload.insert("vpcId".to_string(), Value::String(vpc_id.clone()));
        }
    }

    // Add VPC CIDRs if provided (works for both AWS and GCP)
    if !create_params.vpc_cidrs.is_empty() {
        let cidrs: Vec<Value> = create_params
            .vpc_cidrs
            .iter()
            .map(|c| Value::String(c.clone()))
            .collect();
        payload.insert("vpcCidrs".to_string(), Value::Array(cidrs));
    }

    Ok(Value::Object(payload))
}

/// Build VPC peering update payload from parameters
fn build_update_payload(update_params: &VpcPeeringUpdateParams) -> CliResult<Value> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &update_params.data {
        let content = read_file_input(data)?;
        return Ok(serde_json::from_str(&content).context("Failed to parse JSON input")?);
    }

    // Build payload from first-class parameters
    let mut payload = serde_json::Map::new();

    // Add VPC CIDRs if provided
    if !update_params.vpc_cidrs.is_empty() {
        let cidrs: Vec<Value> = update_params
            .vpc_cidrs
            .iter()
            .map(|c| Value::String(c.clone()))
            .collect();
        payload.insert("vpcCidrs".to_string(), Value::Array(cidrs));
    }

    Ok(Value::Object(payload))
}

/// Build Active-Active VPC peering create payload from parameters
fn build_create_aa_payload(create_params: &VpcPeeringCreateAaParams) -> CliResult<Value> {
    // If --data is provided, use it as the base (escape hatch)
    if let Some(data) = &create_params.data {
        let content = read_file_input(data)?;
        return Ok(serde_json::from_str(&content).context("Failed to parse JSON input")?);
    }

    // Build payload from first-class parameters
    let mut payload = serde_json::Map::new();

    // Source region is common to both AWS and GCP
    if let Some(source_region) = &create_params.source_region {
        payload.insert(
            "sourceRegion".to_string(),
            Value::String(source_region.clone()),
        );
    }

    // Determine if this is AWS or GCP based on provided parameters
    if create_params.gcp_project_id.is_some() {
        // GCP VPC peering
        if let Some(project_id) = &create_params.gcp_project_id {
            payload.insert(
                "vpcProjectUid".to_string(),
                Value::String(project_id.clone()),
            );
        }
        if let Some(network_name) = &create_params.gcp_network_name {
            payload.insert(
                "vpcNetworkName".to_string(),
                Value::String(network_name.clone()),
            );
        }
    } else {
        // AWS VPC peering
        if let Some(dest_region) = &create_params.destination_region {
            payload.insert(
                "destinationRegion".to_string(),
                Value::String(dest_region.clone()),
            );
        }
        if let Some(account_id) = &create_params.aws_account_id {
            payload.insert(
                "awsAccountId".to_string(),
                Value::String(account_id.clone()),
            );
        }
        if let Some(vpc_id) = &create_params.vpc_id {
            payload.insert("vpcId".to_string(), Value::String(vpc_id.clone()));
        }
    }

    // Add VPC CIDRs if provided (works for both AWS and GCP)
    if !create_params.vpc_cidrs.is_empty() {
        let cidrs: Vec<Value> = create_params
            .vpc_cidrs
            .iter()
            .map(|c| Value::String(c.clone()))
            .collect();
        payload.insert("vpcCidrs".to_string(), Value::Array(cidrs));
    }

    Ok(Value::Object(payload))
}

/// Create VPC peering
async fn handle_create(
    params: &ConnectivityOperationParams<'_>,
    create_params: &VpcPeeringCreateParams,
) -> CliResult<()> {
    let payload = build_create_payload(create_params)?;

    let result = params
        .client
        .post_raw(
            &format!("/subscriptions/{}/peerings/vpc", params.subscription_id),
            payload,
        )
        .await
        .context("Failed to create VPC peering")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        result,
        params.async_ops,
        params.output_format,
        params.query,
        "VPC peering created successfully",
    )
    .await
}

/// Update VPC peering
async fn handle_update(
    params: &ConnectivityOperationParams<'_>,
    peering_id: i32,
    update_params: &VpcPeeringUpdateParams,
) -> CliResult<()> {
    let payload = build_update_payload(update_params)?;

    let result = params
        .client
        .put_raw(
            &format!(
                "/subscriptions/{}/peerings/vpc/{}",
                params.subscription_id, peering_id
            ),
            payload,
        )
        .await
        .context("Failed to update VPC peering")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        result,
        params.async_ops,
        params.output_format,
        params.query,
        "VPC peering updated successfully",
    )
    .await
}

/// Delete VPC peering
async fn handle_delete(
    params: &ConnectivityOperationParams<'_>,
    peering_id: i32,
    force: bool,
) -> CliResult<()> {
    if !force {
        confirm_action(&format!(
            "delete VPC peering {} for subscription {}",
            peering_id, params.subscription_id
        ))?;
    }

    let result = params
        .client
        .delete_raw(&format!(
            "/subscriptions/{}/peerings/vpc/{}",
            params.subscription_id, peering_id
        ))
        .await
        .context("Failed to delete VPC peering")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        result,
        params.async_ops,
        params.output_format,
        params.query,
        "VPC peering deleted successfully",
    )
    .await
}

/// List Active-Active VPC peerings
async fn handle_list_active_active(
    client: &CloudClient,
    subscription_id: i32,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let result = client
        .get_raw(&format!(
            "/subscriptions/{}/peerings/vpc/active-active",
            subscription_id
        ))
        .await
        .context("Failed to list Active-Active VPC peerings")?;

    let data = handle_output(result, output_format, query)?;

    if matches!(output_format, OutputFormat::Table) && query.is_none() {
        print_vpc_peering_list_table(&data)?;
    } else {
        print_formatted_output(data, output_format)?;
    }

    Ok(())
}

/// Create Active-Active VPC peering
async fn handle_create_active_active(
    params: &ConnectivityOperationParams<'_>,
    create_params: &VpcPeeringCreateAaParams,
) -> CliResult<()> {
    let payload = build_create_aa_payload(create_params)?;

    let result = params
        .client
        .post_raw(
            &format!(
                "/subscriptions/{}/peerings/vpc/active-active",
                params.subscription_id
            ),
            payload,
        )
        .await
        .context("Failed to create Active-Active VPC peering")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        result,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active VPC peering created successfully",
    )
    .await
}

/// Update Active-Active VPC peering
async fn handle_update_active_active(
    params: &ConnectivityOperationParams<'_>,
    peering_id: i32,
    update_params: &VpcPeeringUpdateParams,
) -> CliResult<()> {
    // Reuse the same update payload builder since update params are the same
    let payload = build_update_payload(update_params)?;

    let result = params
        .client
        .put_raw(
            &format!(
                "/subscriptions/{}/peerings/vpc/active-active/{}",
                params.subscription_id, peering_id
            ),
            payload,
        )
        .await
        .context("Failed to update Active-Active VPC peering")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        result,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active VPC peering updated successfully",
    )
    .await
}

/// Delete Active-Active VPC peering
async fn handle_delete_active_active(
    params: &ConnectivityOperationParams<'_>,
    peering_id: i32,
    force: bool,
) -> CliResult<()> {
    if !force {
        confirm_action(&format!(
            "delete Active-Active VPC peering {} for subscription {}",
            peering_id, params.subscription_id
        ))?;
    }

    let result = params
        .client
        .delete_raw(&format!(
            "/subscriptions/{}/peerings/vpc/active-active/{}",
            params.subscription_id, peering_id
        ))
        .await
        .context("Failed to delete Active-Active VPC peering")?;

    handle_async_response(
        params.conn_mgr,
        params.profile_name,
        result,
        params.async_ops,
        params.output_format,
        params.query,
        "Active-Active VPC peering deleted successfully",
    )
    .await
}

/// Print VPC peering details in table format
fn print_vpc_peering_table(data: &Value) -> CliResult<()> {
    use super::super::utils::DetailRow;
    use tabled::{Table, settings::Style};

    let mut rows = Vec::new();

    // Basic info
    if let Some(id) = data.get("peeringId") {
        rows.push(DetailRow {
            field: "Peering ID".to_string(),
            value: id.to_string().trim_matches('"').to_string(),
        });
    }

    if let Some(status) = data.get("status").and_then(|s| s.as_str()) {
        rows.push(DetailRow {
            field: "Status".to_string(),
            value: super::super::utils::format_status_text(status),
        });
    }

    // AWS VPC info
    if let Some(vpc_id) = data.get("awsVpcId").and_then(|v| v.as_str()) {
        rows.push(DetailRow {
            field: "AWS VPC ID".to_string(),
            value: vpc_id.to_string(),
        });
    }

    if let Some(account_id) = data.get("awsAccountId").and_then(|a| a.as_str()) {
        rows.push(DetailRow {
            field: "AWS Account ID".to_string(),
            value: account_id.to_string(),
        });
    }

    if let Some(region) = data.get("region").and_then(|r| r.as_str()) {
        rows.push(DetailRow {
            field: "Region".to_string(),
            value: region.to_string(),
        });
    }

    // CIDR blocks
    if let Some(cidrs) = data.get("vpcCidrs").and_then(|c| c.as_array()) {
        let cidr_list: Vec<String> = cidrs
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !cidr_list.is_empty() {
            rows.push(DetailRow {
                field: "VPC CIDRs".to_string(),
                value: cidr_list.join(", "),
            });
        }
    }

    // Connection details
    if let Some(connection_id) = data.get("connectionId").and_then(|c| c.as_str()) {
        rows.push(DetailRow {
            field: "Connection ID".to_string(),
            value: connection_id.to_string(),
        });
    }

    if rows.is_empty() {
        println!("No VPC peering information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    println!("{}", table);
    Ok(())
}

/// Print VPC peering list in table format
fn print_vpc_peering_list_table(data: &Value) -> CliResult<()> {
    use colored::Colorize;
    use tabled::builder::Builder;
    use tabled::settings::Style;

    let peerings = if let Some(arr) = data.as_array() {
        arr.clone()
    } else if let Some(peerings) = data.get("peerings").and_then(|p| p.as_array()) {
        peerings.clone()
    } else {
        println!("No VPC peerings found");
        return Ok(());
    };

    if peerings.is_empty() {
        println!("No VPC peerings found");
        return Ok(());
    }

    let mut builder = Builder::default();
    builder.push_record(["ID", "Status", "VPC ID", "Account ID", "Region", "CIDRs"]);

    for peering in peerings {
        let id = peering
            .get("peeringId")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let status = peering
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let vpc_id = peering
            .get("awsVpcId")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let account_id = peering
            .get("awsAccountId")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let region = peering.get("region").and_then(|v| v.as_str()).unwrap_or("");

        let cidrs = if let Some(cidr_array) = peering.get("vpcCidrs").and_then(|c| c.as_array()) {
            cidr_array
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            String::new()
        };

        let status_str = match status.to_lowercase().as_str() {
            "active" => status.green().to_string(),
            "pending" => status.yellow().to_string(),
            "failed" | "error" => status.red().to_string(),
            _ => status.to_string(),
        };

        builder.push_record([
            &id.to_string(),
            &status_str,
            vpc_id,
            account_id,
            region,
            &cidrs,
        ]);
    }

    println!("{}", builder.build().with(Style::blank()));
    Ok(())
}
