//! Networking and connectivity tools for Redis Cloud
//!
//! VPC Peering, Transit Gateway, Private Service Connect (PSC), and PrivateLink tools.

use redis_cloud::connectivity::psc::PscEndpointUpdateRequest;
use redis_cloud::connectivity::transit_gateway::TgwAttachmentRequest;
use redis_cloud::connectivity::vpc_peering::{
    ActiveActiveVpcPeeringCreateRequest, VpcPeeringCreateRequest, VpcPeeringUpdateAwsRequest,
};
use redis_cloud::{
    PrincipalType, PrivateLinkAddPrincipalRequest, PrivateLinkCreateRequest, PrivateLinkHandler,
    PscHandler, TransitGatewayHandler, VpcPeeringHandler,
};
use tower_mcp::{CallToolResult, ResultExt, ToolError};

use crate::tools::macros::{cloud_tool, mcp_module};

mcp_module! {
    get_vpc_peering => "get_vpc_peering",
    create_vpc_peering => "create_vpc_peering",
    update_vpc_peering => "update_vpc_peering",
    delete_vpc_peering => "delete_vpc_peering",
    get_aa_vpc_peering => "get_aa_vpc_peering",
    create_aa_vpc_peering => "create_aa_vpc_peering",
    update_aa_vpc_peering => "update_aa_vpc_peering",
    delete_aa_vpc_peering => "delete_aa_vpc_peering",
    get_tgw_attachments => "get_tgw_attachments",
    get_tgw_invitations => "get_tgw_invitations",
    accept_tgw_invitation => "accept_tgw_invitation",
    reject_tgw_invitation => "reject_tgw_invitation",
    create_tgw_attachment => "create_tgw_attachment",
    update_tgw_attachment_cidrs => "update_tgw_attachment_cidrs",
    delete_tgw_attachment => "delete_tgw_attachment",
    get_aa_tgw_attachments => "get_aa_tgw_attachments",
    get_aa_tgw_invitations => "get_aa_tgw_invitations",
    accept_aa_tgw_invitation => "accept_aa_tgw_invitation",
    reject_aa_tgw_invitation => "reject_aa_tgw_invitation",
    create_aa_tgw_attachment => "create_aa_tgw_attachment",
    update_aa_tgw_attachment_cidrs => "update_aa_tgw_attachment_cidrs",
    delete_aa_tgw_attachment => "delete_aa_tgw_attachment",
    get_psc_service => "get_psc_service",
    create_psc_service => "create_psc_service",
    delete_psc_service => "delete_psc_service",
    get_psc_endpoints => "get_psc_endpoints",
    create_psc_endpoint => "create_psc_endpoint",
    update_psc_endpoint => "update_psc_endpoint",
    delete_psc_endpoint => "delete_psc_endpoint",
    get_psc_creation_script => "get_psc_creation_script",
    get_psc_deletion_script => "get_psc_deletion_script",
    get_aa_psc_service => "get_aa_psc_service",
    create_aa_psc_service => "create_aa_psc_service",
    delete_aa_psc_service => "delete_aa_psc_service",
    get_aa_psc_endpoints => "get_aa_psc_endpoints",
    create_aa_psc_endpoint => "create_aa_psc_endpoint",
    update_aa_psc_endpoint => "update_aa_psc_endpoint",
    delete_aa_psc_endpoint => "delete_aa_psc_endpoint",
    get_aa_psc_creation_script => "get_aa_psc_creation_script",
    get_aa_psc_deletion_script => "get_aa_psc_deletion_script",
    get_private_link => "get_private_link",
    create_private_link => "create_private_link",
    delete_private_link => "delete_private_link",
    add_private_link_principals => "add_private_link_principals",
    remove_private_link_principals => "remove_private_link_principals",
    get_private_link_endpoint_script => "get_private_link_endpoint_script",
    get_aa_private_link => "get_aa_private_link",
    create_aa_private_link => "create_aa_private_link",
    add_aa_private_link_principals => "add_aa_private_link_principals",
    remove_aa_private_link_principals => "remove_aa_private_link_principals",
    get_aa_private_link_endpoint_script => "get_aa_private_link_endpoint_script",
}

// ============================================================================
// VPC Peering tools
// ============================================================================

cloud_tool!(read_only, get_vpc_peering, "get_vpc_peering",
    "Get VPC peering details.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .get(input.subscription_id)
            .await
            .tool_context("Failed to get VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_vpc_peering, "create_vpc_peering",
    "Create a VPC peering connection.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Cloud provider (AWS, GCP, Azure)
        #[serde(default)]
        pub provider: Option<String>,
        /// AWS VPC ID
        #[serde(default)]
        pub vpc_id: Option<String>,
        /// AWS region
        #[serde(default)]
        pub aws_region: Option<String>,
        /// AWS account ID
        #[serde(default)]
        pub aws_account_id: Option<String>,
        /// VPC CIDR block
        #[serde(default)]
        pub vpc_cidr: Option<String>,
        /// Multiple VPC CIDR blocks
        #[serde(default)]
        pub vpc_cidrs: Option<Vec<String>>,
        /// GCP project ID (for GCP peering)
        #[serde(default)]
        pub gcp_project_id: Option<String>,
        /// GCP network name (for GCP peering)
        #[serde(default)]
        pub network_name: Option<String>,
    } => |client, input| {
        let request = VpcPeeringCreateRequest {
            provider: input.provider,
            vpc_id: input.vpc_id,
            aws_region: input.aws_region,
            aws_account_id: input.aws_account_id,
            vpc_cidr: input.vpc_cidr,
            vpc_cidrs: input.vpc_cidrs,
            gcp_project_id: input.gcp_project_id,
            network_name: input.network_name,
            ..Default::default()
        };

        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .create(input.subscription_id, &request)
            .await
            .tool_context("Failed to create VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_vpc_peering, "update_vpc_peering",
    "Update a VPC peering connection.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// VPC Peering ID
        pub peering_id: i32,
        /// Cloud provider (AWS, GCP, Azure)
        #[serde(default)]
        pub provider: Option<String>,
        /// AWS VPC ID
        #[serde(default)]
        pub vpc_id: Option<String>,
        /// AWS region
        #[serde(default)]
        pub aws_region: Option<String>,
        /// AWS account ID
        #[serde(default)]
        pub aws_account_id: Option<String>,
        /// VPC CIDR block
        #[serde(default)]
        pub vpc_cidr: Option<String>,
        /// Multiple VPC CIDR blocks
        #[serde(default)]
        pub vpc_cidrs: Option<Vec<String>>,
        /// GCP project ID (for GCP peering)
        #[serde(default)]
        pub gcp_project_id: Option<String>,
        /// GCP network name (for GCP peering)
        #[serde(default)]
        pub network_name: Option<String>,
    } => |client, input| {
        let request = VpcPeeringCreateRequest {
            provider: input.provider,
            vpc_id: input.vpc_id,
            aws_region: input.aws_region,
            aws_account_id: input.aws_account_id,
            vpc_cidr: input.vpc_cidr,
            vpc_cidrs: input.vpc_cidrs,
            gcp_project_id: input.gcp_project_id,
            network_name: input.network_name,
            ..Default::default()
        };

        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .update(input.subscription_id, input.peering_id, &request)
            .await
            .tool_context("Failed to update VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_vpc_peering, "delete_vpc_peering",
    "DANGEROUS: Delete a VPC peering connection. Causes connectivity loss.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// VPC Peering ID
        pub peering_id: i32,
    } => |client, input| {
        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .delete(input.subscription_id, input.peering_id)
            .await
            .tool_context("Failed to delete VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Active-Active VPC Peering tools
// ============================================================================

cloud_tool!(read_only, get_aa_vpc_peering, "get_aa_vpc_peering",
    "Get Active-Active VPC peering details.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .get_active_active(input.subscription_id)
            .await
            .tool_context("Failed to get AA VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_aa_vpc_peering, "create_aa_vpc_peering",
    "Create an Active-Active VPC peering connection. Use aws_region for the source region and destination_region for the target region (AWS cross-region peering).",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Cloud provider (AWS, GCP, Azure)
        #[serde(default)]
        pub provider: Option<String>,
        /// AWS VPC ID
        #[serde(default)]
        pub vpc_id: Option<String>,
        /// AWS source region
        #[serde(default)]
        pub aws_region: Option<String>,
        /// AWS destination region for cross-region Active-Active VPC peering (AWS only)
        #[serde(default)]
        pub destination_region: Option<String>,
        /// AWS account ID
        #[serde(default)]
        pub aws_account_id: Option<String>,
        /// VPC CIDR block
        #[serde(default)]
        pub vpc_cidr: Option<String>,
        /// Multiple VPC CIDR blocks
        #[serde(default)]
        pub vpc_cidrs: Option<Vec<String>>,
        /// GCP project ID (for GCP peering)
        #[serde(default)]
        pub gcp_project_id: Option<String>,
        /// GCP network name (for GCP peering)
        #[serde(default)]
        pub network_name: Option<String>,
    } => |client, input| {
        // Active-Active peering uses a distinct request type upstream; map
        // `aws_region` onto `source_region` and the optional
        // `destination_region` for AWS cross-region peering.
        let request = ActiveActiveVpcPeeringCreateRequest {
            provider: input.provider,
            source_region: input.aws_region,
            destination_region: input.destination_region,
            aws_account_id: input.aws_account_id,
            vpc_id: input.vpc_id,
            vpc_cidr: input.vpc_cidr,
            vpc_cidrs: input.vpc_cidrs,
            gcp_project_id: input.gcp_project_id,
            network_name: input.network_name,
            ..Default::default()
        };

        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .create_active_active(input.subscription_id, &request)
            .await
            .tool_context("Failed to create AA VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_aa_vpc_peering, "update_aa_vpc_peering",
    "Update an Active-Active VPC peering connection.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// VPC Peering ID
        pub peering_id: i32,
        /// VPC CIDR block
        #[serde(default)]
        pub vpc_cidr: Option<String>,
        /// Multiple VPC CIDR blocks
        #[serde(default)]
        pub vpc_cidrs: Option<Vec<String>>,
    } => |client, input| {
        // Active-Active update only carries the CIDR list upstream.
        let request = VpcPeeringUpdateAwsRequest {
            subscription_id: None,
            vpc_peering_id: None,
            vpc_cidr: input.vpc_cidr,
            vpc_cidrs: input.vpc_cidrs,
            command_type: None,
        };

        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .update_active_active(input.subscription_id, input.peering_id, &request)
            .await
            .tool_context("Failed to update AA VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_aa_vpc_peering, "delete_aa_vpc_peering",
    "DANGEROUS: Delete an Active-Active VPC peering connection. Causes connectivity loss.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// VPC Peering ID
        pub peering_id: i32,
    } => |client, input| {
        let handler = VpcPeeringHandler::new(client);
        let result = handler
            .delete_active_active(input.subscription_id, input.peering_id)
            .await
            .tool_context("Failed to delete AA VPC peering")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Transit Gateway tools
// ============================================================================

cloud_tool!(read_only, get_tgw_attachments, "get_tgw_attachments",
    "Get Transit Gateway attachments.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .get_attachments(input.subscription_id)
            .await
            .tool_context("Failed to get TGW attachments")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_tgw_invitations, "get_tgw_invitations",
    "Get Transit Gateway shared invitations.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .get_shared_invitations(input.subscription_id)
            .await
            .tool_context("Failed to get TGW invitations")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, accept_tgw_invitation, "accept_tgw_invitation",
    "Accept a Transit Gateway resource share invitation.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Invitation ID
        pub invitation_id: String,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .accept_resource_share(input.subscription_id, input.invitation_id)
            .await
            .tool_context("Failed to accept TGW invitation")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, reject_tgw_invitation, "reject_tgw_invitation",
    "Reject a Transit Gateway resource share invitation.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Invitation ID
        pub invitation_id: String,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .reject_resource_share(input.subscription_id, input.invitation_id)
            .await
            .tool_context("Failed to reject TGW invitation")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_tgw_attachment, "create_tgw_attachment",
    "Create a Transit Gateway attachment.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Transit Gateway ID
        #[serde(default)]
        pub tgw_id: Option<String>,
    } => |client, input| {
        // The attachment route now requires the TGW ID in the path; the
        // generic `create_attachment` was removed upstream.
        let tgw_id = input
            .tgw_id
            .ok_or_else(|| ToolError::new("tgw_id is required".to_string()))?;

        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .create_attachment_with_id(input.subscription_id, &tgw_id)
            .await
            .tool_context("Failed to create TGW attachment")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_tgw_attachment_cidrs, "update_tgw_attachment_cidrs",
    "Update CIDRs for a Transit Gateway attachment.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Attachment ID
        pub attachment_id: String,
        /// AWS account ID
        #[serde(default)]
        pub aws_account_id: Option<String>,
        /// Transit Gateway ID
        #[serde(default)]
        pub tgw_id: Option<String>,
        /// CIDR blocks to route through the TGW
        #[serde(default)]
        pub cidrs: Option<Vec<String>>,
    } => |client, input| {
        let request = TgwAttachmentRequest {
            aws_account_id: input.aws_account_id,
            tgw_id: input.tgw_id,
            cidrs: input.cidrs,
        };

        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .update_attachment_cidrs(input.subscription_id, input.attachment_id, &request)
            .await
            .tool_context("Failed to update TGW attachment CIDRs")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_tgw_attachment, "delete_tgw_attachment",
    "DANGEROUS: Delete a Transit Gateway attachment. Causes connectivity loss.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Attachment ID
        pub attachment_id: String,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .delete_attachment(input.subscription_id, input.attachment_id)
            .await
            .tool_context("Failed to delete TGW attachment")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Active-Active Transit Gateway tools
// ============================================================================

cloud_tool!(read_only, get_aa_tgw_attachments, "get_aa_tgw_attachments",
    "Get Active-Active Transit Gateway attachments.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .get_attachments_active_active(input.subscription_id, input.region_id)
            .await
            .tool_context("Failed to get AA TGW attachments")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_aa_tgw_invitations, "get_aa_tgw_invitations",
    "Get Active-Active Transit Gateway shared invitations.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .get_shared_invitations_active_active(input.subscription_id, input.region_id)
            .await
            .tool_context("Failed to get AA TGW invitations")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, accept_aa_tgw_invitation, "accept_aa_tgw_invitation",
    "Accept an Active-Active Transit Gateway resource share invitation.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// Invitation ID
        pub invitation_id: String,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .accept_resource_share_active_active(
                input.subscription_id,
                input.region_id,
                input.invitation_id,
            )
            .await
            .tool_context("Failed to accept AA TGW invitation")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, reject_aa_tgw_invitation, "reject_aa_tgw_invitation",
    "Reject an Active-Active Transit Gateway resource share invitation.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// Invitation ID
        pub invitation_id: String,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .reject_resource_share_active_active(
                input.subscription_id,
                input.region_id,
                input.invitation_id,
            )
            .await
            .tool_context("Failed to reject AA TGW invitation")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_aa_tgw_attachment, "create_aa_tgw_attachment",
    "Create an Active-Active Transit Gateway attachment.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// AWS account ID
        #[serde(default)]
        pub aws_account_id: Option<String>,
        /// Transit Gateway ID
        #[serde(default)]
        pub tgw_id: Option<String>,
        /// CIDR blocks to route through the TGW
        #[serde(default)]
        pub cidrs: Option<Vec<String>>,
    } => |client, input| {
        // The attachment route now requires the TGW ID in the path.
        let tgw_id = input
            .tgw_id
            .clone()
            .ok_or_else(|| ToolError::new("tgw_id is required".to_string()))?;
        let request = TgwAttachmentRequest {
            aws_account_id: input.aws_account_id,
            tgw_id: input.tgw_id,
            cidrs: input.cidrs,
        };

        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .create_attachment_active_active(
                input.subscription_id,
                input.region_id,
                &tgw_id,
                &request,
            )
            .await
            .tool_context("Failed to create AA TGW attachment")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_aa_tgw_attachment_cidrs, "update_aa_tgw_attachment_cidrs",
    "Update CIDRs for an Active-Active Transit Gateway attachment.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// Attachment ID
        pub attachment_id: String,
        /// AWS account ID
        #[serde(default)]
        pub aws_account_id: Option<String>,
        /// Transit Gateway ID
        #[serde(default)]
        pub tgw_id: Option<String>,
        /// CIDR blocks to route through the TGW
        #[serde(default)]
        pub cidrs: Option<Vec<String>>,
    } => |client, input| {
        let request = TgwAttachmentRequest {
            aws_account_id: input.aws_account_id,
            tgw_id: input.tgw_id,
            cidrs: input.cidrs,
        };

        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .update_attachment_cidrs_active_active(
                input.subscription_id,
                input.region_id,
                input.attachment_id,
                &request,
            )
            .await
            .tool_context("Failed to update AA TGW attachment CIDRs")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_aa_tgw_attachment, "delete_aa_tgw_attachment",
    "DANGEROUS: Delete an Active-Active Transit Gateway attachment. Causes connectivity loss.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// Attachment ID
        pub attachment_id: String,
    } => |client, input| {
        let handler = TransitGatewayHandler::new(client);
        let result = handler
            .delete_attachment_active_active(
                input.subscription_id,
                input.region_id,
                input.attachment_id,
            )
            .await
            .tool_context("Failed to delete AA TGW attachment")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Private Service Connect (PSC) tools
// ============================================================================

cloud_tool!(read_only, get_psc_service, "get_psc_service",
    "Get Private Service Connect (PSC) service.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_service(input.subscription_id)
            .await
            .tool_context("Failed to get PSC service")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_psc_service, "create_psc_service",
    "Create a Private Service Connect (PSC) service.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .create_service(input.subscription_id)
            .await
            .tool_context("Failed to create PSC service")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_psc_service, "delete_psc_service",
    "DANGEROUS: Delete a PSC service. Disconnects all endpoints.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .delete_service(input.subscription_id)
            .await
            .tool_context("Failed to delete PSC service")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_psc_endpoints, "get_psc_endpoints",
    "Get Private Service Connect (PSC) endpoints.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_endpoints(input.subscription_id, input.psc_service_id)
            .await
            .tool_context("Failed to get PSC endpoints")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_psc_endpoint, "create_psc_endpoint",
    "Create a Private Service Connect (PSC) endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// PSC service ID (used internally by the request)
        pub psc_service_id: i32,
        /// Endpoint ID (used internally by the request)
        pub endpoint_id: i32,
        /// Google Cloud project ID
        #[serde(default)]
        pub gcp_project_id: Option<String>,
        /// Name of the Google Cloud VPC that hosts your application
        #[serde(default)]
        pub gcp_vpc_name: Option<String>,
        /// Name of your VPC's subnet of IP address ranges
        #[serde(default)]
        pub gcp_vpc_subnet_name: Option<String>,
        /// Prefix used to create PSC endpoints in the consumer application VPC
        #[serde(default)]
        pub endpoint_connection_name: Option<String>,
    } => |client, input| {
        let request = PscEndpointUpdateRequest {
            subscription_id: input.subscription_id,
            psc_service_id: input.psc_service_id,
            endpoint_id: input.endpoint_id,
            gcp_project_id: input.gcp_project_id,
            gcp_vpc_name: input.gcp_vpc_name,
            gcp_vpc_subnet_name: input.gcp_vpc_subnet_name,
            endpoint_connection_name: input.endpoint_connection_name,
        };

        let handler = PscHandler::new(client);
        let result = handler
            .create_endpoint(input.subscription_id, input.psc_service_id, &request)
            .await
            .tool_context("Failed to create PSC endpoint")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_psc_endpoint, "update_psc_endpoint",
    "Update a Private Service Connect (PSC) endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Google Cloud project ID
        #[serde(default)]
        pub gcp_project_id: Option<String>,
        /// Name of the Google Cloud VPC that hosts your application
        #[serde(default)]
        pub gcp_vpc_name: Option<String>,
        /// Name of your VPC's subnet of IP address ranges
        #[serde(default)]
        pub gcp_vpc_subnet_name: Option<String>,
        /// Prefix used to create PSC endpoints in the consumer application VPC
        #[serde(default)]
        pub endpoint_connection_name: Option<String>,
    } => |client, input| {
        let request = PscEndpointUpdateRequest {
            subscription_id: input.subscription_id,
            psc_service_id: input.psc_service_id,
            endpoint_id: input.endpoint_id,
            gcp_project_id: input.gcp_project_id,
            gcp_vpc_name: input.gcp_vpc_name,
            gcp_vpc_subnet_name: input.gcp_vpc_subnet_name,
            endpoint_connection_name: input.endpoint_connection_name,
        };

        let handler = PscHandler::new(client);
        let result = handler
            .update_endpoint(
                input.subscription_id,
                input.psc_service_id,
                input.endpoint_id,
                &request,
            )
            .await
            .tool_context("Failed to update PSC endpoint")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_psc_endpoint, "delete_psc_endpoint",
    "DANGEROUS: Delete a PSC endpoint. Causes connectivity loss.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .delete_endpoint(input.subscription_id, input.psc_service_id, input.endpoint_id)
            .await
            .tool_context("Failed to delete PSC endpoint")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_psc_creation_script, "get_psc_creation_script",
    "Get the creation script for a PSC endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_endpoint_creation_script(
                input.subscription_id,
                input.psc_service_id,
                input.endpoint_id,
            )
            .await
            .tool_context("Failed to get PSC creation script")?;

        Ok(CallToolResult::text(result))
    }
);

cloud_tool!(read_only, get_psc_deletion_script, "get_psc_deletion_script",
    "Get the deletion script for a PSC endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_endpoint_deletion_script(
                input.subscription_id,
                input.psc_service_id,
                input.endpoint_id,
            )
            .await
            .tool_context("Failed to get PSC deletion script")?;

        Ok(CallToolResult::text(result))
    }
);

// ============================================================================
// Active-Active PSC tools
// ============================================================================

cloud_tool!(read_only, get_aa_psc_service, "get_aa_psc_service",
    "Get Active-Active PSC service.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_service_active_active(input.subscription_id, input.region_id)
            .await
            .tool_context("Failed to get AA PSC service")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_aa_psc_service, "create_aa_psc_service",
    "Create an Active-Active PSC service.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .create_service_active_active(input.subscription_id, input.region_id)
            .await
            .tool_context("Failed to create AA PSC service")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_aa_psc_service, "delete_aa_psc_service",
    "DANGEROUS: Delete an Active-Active PSC service. Disconnects all endpoints.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .delete_service_active_active(input.subscription_id, input.region_id)
            .await
            .tool_context("Failed to delete AA PSC service")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_aa_psc_endpoints, "get_aa_psc_endpoints",
    "Get Active-Active PSC endpoints.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_endpoints_active_active(
                input.subscription_id,
                input.region_id,
                input.psc_service_id,
            )
            .await
            .tool_context("Failed to get AA PSC endpoints")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_aa_psc_endpoint, "create_aa_psc_endpoint",
    "Create an Active-Active PSC endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
        /// Google Cloud project ID
        #[serde(default)]
        pub gcp_project_id: Option<String>,
        /// Name of the Google Cloud VPC that hosts your application
        #[serde(default)]
        pub gcp_vpc_name: Option<String>,
        /// Name of your VPC's subnet of IP address ranges
        #[serde(default)]
        pub gcp_vpc_subnet_name: Option<String>,
        /// Prefix used to create PSC endpoints in the consumer application VPC
        #[serde(default)]
        pub endpoint_connection_name: Option<String>,
    } => |client, input| {
        let request = PscEndpointUpdateRequest {
            subscription_id: input.subscription_id,
            psc_service_id: input.psc_service_id,
            endpoint_id: input.endpoint_id,
            gcp_project_id: input.gcp_project_id,
            gcp_vpc_name: input.gcp_vpc_name,
            gcp_vpc_subnet_name: input.gcp_vpc_subnet_name,
            endpoint_connection_name: input.endpoint_connection_name,
        };

        let handler = PscHandler::new(client);
        let result = handler
            .create_endpoint_active_active(
                input.subscription_id,
                input.region_id,
                input.psc_service_id,
                &request,
            )
            .await
            .tool_context("Failed to create AA PSC endpoint")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, update_aa_psc_endpoint, "update_aa_psc_endpoint",
    "Update an Active-Active PSC endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Google Cloud project ID
        #[serde(default)]
        pub gcp_project_id: Option<String>,
        /// Name of the Google Cloud VPC that hosts your application
        #[serde(default)]
        pub gcp_vpc_name: Option<String>,
        /// Name of your VPC's subnet of IP address ranges
        #[serde(default)]
        pub gcp_vpc_subnet_name: Option<String>,
        /// Prefix used to create PSC endpoints in the consumer application VPC
        #[serde(default)]
        pub endpoint_connection_name: Option<String>,
    } => |client, input| {
        let request = PscEndpointUpdateRequest {
            subscription_id: input.subscription_id,
            psc_service_id: input.psc_service_id,
            endpoint_id: input.endpoint_id,
            gcp_project_id: input.gcp_project_id,
            gcp_vpc_name: input.gcp_vpc_name,
            gcp_vpc_subnet_name: input.gcp_vpc_subnet_name,
            endpoint_connection_name: input.endpoint_connection_name,
        };

        let handler = PscHandler::new(client);
        let result = handler
            .update_endpoint_active_active(
                input.subscription_id,
                input.region_id,
                input.psc_service_id,
                input.endpoint_id,
                &request,
            )
            .await
            .tool_context("Failed to update AA PSC endpoint")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_aa_psc_endpoint, "delete_aa_psc_endpoint",
    "DANGEROUS: Delete an Active-Active PSC endpoint. Causes connectivity loss.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .delete_endpoint_active_active(
                input.subscription_id,
                input.region_id,
                input.psc_service_id,
                input.endpoint_id,
            )
            .await
            .tool_context("Failed to delete AA PSC endpoint")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_aa_psc_creation_script, "get_aa_psc_creation_script",
    "Get the creation script for an Active-Active PSC endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_endpoint_creation_script_active_active(
                input.subscription_id,
                input.region_id,
                input.psc_service_id,
                input.endpoint_id,
            )
            .await
            .tool_context("Failed to get AA PSC creation script")?;

        Ok(CallToolResult::text(result))
    }
);

cloud_tool!(read_only, get_aa_psc_deletion_script, "get_aa_psc_deletion_script",
    "Get the deletion script for an Active-Active PSC endpoint.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// PSC service ID
        pub psc_service_id: i32,
        /// Endpoint ID
        pub endpoint_id: i32,
    } => |client, input| {
        let handler = PscHandler::new(client);
        let result = handler
            .get_endpoint_deletion_script_active_active(
                input.subscription_id,
                input.region_id,
                input.psc_service_id,
                input.endpoint_id,
            )
            .await
            .tool_context("Failed to get AA PSC deletion script")?;

        Ok(CallToolResult::text(result))
    }
);

// ============================================================================
// PrivateLink tools
// ============================================================================

fn parse_principal_type(s: &str) -> Result<PrincipalType, ToolError> {
    match s.to_lowercase().as_str() {
        "aws_account" | "awsaccount" => Ok(PrincipalType::AwsAccount),
        "organization" => Ok(PrincipalType::Organization),
        "organization_unit" | "organizationunit" => Ok(PrincipalType::OrganizationUnit),
        "iam_role" | "iamrole" => Ok(PrincipalType::IamRole),
        "iam_user" | "iamuser" => Ok(PrincipalType::IamUser),
        "service_principal" | "serviceprincipal" => Ok(PrincipalType::ServicePrincipal),
        _ => Err(ToolError::new(format!(
            "Invalid principal type: {}. Expected one of: aws_account, organization, organization_unit, iam_role, iam_user, service_principal",
            s
        ))),
    }
}

cloud_tool!(read_only, get_private_link, "get_private_link",
    "Get AWS PrivateLink configuration.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .get(input.subscription_id)
            .await
            .tool_context("Failed to get PrivateLink")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_private_link, "create_private_link",
    "Create an AWS PrivateLink configuration.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Share name for the PrivateLink service (max 64 characters)
        pub share_name: String,
        /// AWS principal (account ID, role ARN, etc.)
        pub principal: String,
        /// Principal type: aws_account, organization, organization_unit, iam_role, iam_user, service_principal
        pub principal_type: String,
        /// Optional alias for the PrivateLink
        #[serde(default)]
        pub alias: Option<String>,
    } => |client, input| {
        let principal_type = parse_principal_type(&input.principal_type)?;

        let request = PrivateLinkCreateRequest {
            share_name: input.share_name,
            principal: input.principal,
            principal_type,
            alias: input.alias,
        };

        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .create(input.subscription_id, &request)
            .await
            .tool_context("Failed to create PrivateLink")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(destructive, delete_private_link, "delete_private_link",
    "DANGEROUS: Delete an AWS PrivateLink configuration. Causes connectivity loss.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .delete(input.subscription_id)
            .await
            .tool_context("Failed to delete PrivateLink")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, add_private_link_principals, "add_private_link_principals",
    "Add AWS principals to a PrivateLink access list.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// AWS principal (account ID, role ARN, etc.)
        pub principal: String,
        /// Principal type: aws_account, organization, organization_unit, iam_role, iam_user, service_principal
        #[serde(default)]
        pub principal_type: Option<String>,
        /// Optional alias for the principal
        #[serde(default)]
        pub alias: Option<String>,
    } => |client, input| {
        let principal_type = input
            .principal_type
            .map(|pt| parse_principal_type(&pt))
            .transpose()?;

        let request = PrivateLinkAddPrincipalRequest {
            principal: input.principal,
            principal_type,
            alias: input.alias,
        };

        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .add_principals(input.subscription_id, &request)
            .await
            .tool_context("Failed to add PrivateLink principals")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, remove_private_link_principals, "remove_private_link_principals",
    "Remove AWS principals from a PrivateLink access list.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// AWS principal (account ID, role ARN, etc.) to remove
        pub principal: String,
        /// Principal type: aws_account, organization, organization_unit, iam_role, iam_user, service_principal
        #[serde(default)]
        pub principal_type: Option<String>,
        /// Alias of the principal
        #[serde(default)]
        pub alias: Option<String>,
    } => |client, input| {
        let principal_type = input
            .principal_type
            .map(|pt| parse_principal_type(&pt))
            .transpose()?;

        let request =
            redis_cloud::connectivity::private_link::PrivateLinkRemovePrincipalRequest {
                principal: input.principal,
                principal_type,
                alias: input.alias,
            };

        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .remove_principals(input.subscription_id, &request)
            .await
            .tool_context("Failed to remove PrivateLink principals")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_private_link_endpoint_script, "get_private_link_endpoint_script",
    "Get the endpoint creation script for an AWS PrivateLink.",
    {
        /// Subscription ID
        pub subscription_id: i32,
    } => |client, input| {
        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .get_endpoint_script(input.subscription_id)
            .await
            .tool_context("Failed to get PrivateLink endpoint script")?;

        CallToolResult::from_serialize(&result)
    }
);

// ============================================================================
// Active-Active PrivateLink tools
// ============================================================================

cloud_tool!(read_only, get_aa_private_link, "get_aa_private_link",
    "Get Active-Active AWS PrivateLink configuration.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
    } => |client, input| {
        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .get_active_active(input.subscription_id, input.region_id)
            .await
            .tool_context("Failed to get AA PrivateLink")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, create_aa_private_link, "create_aa_private_link",
    "Create an Active-Active AWS PrivateLink configuration.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// Share name for the PrivateLink service (max 64 characters)
        pub share_name: String,
        /// AWS principal (account ID, role ARN, etc.)
        pub principal: String,
        /// Principal type: aws_account, organization, organization_unit, iam_role, iam_user, service_principal
        pub principal_type: String,
        /// Optional alias for the PrivateLink
        #[serde(default)]
        pub alias: Option<String>,
    } => |client, input| {
        let principal_type = parse_principal_type(&input.principal_type)?;

        let request = PrivateLinkCreateRequest {
            share_name: input.share_name,
            principal: input.principal,
            principal_type,
            alias: input.alias,
        };

        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .create_active_active(input.subscription_id, input.region_id, &request)
            .await
            .tool_context("Failed to create AA PrivateLink")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, add_aa_private_link_principals, "add_aa_private_link_principals",
    "Add AWS principals to an Active-Active PrivateLink access list.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// AWS principal (account ID, role ARN, etc.)
        pub principal: String,
        /// Principal type: aws_account, organization, organization_unit, iam_role, iam_user, service_principal
        #[serde(default)]
        pub principal_type: Option<String>,
        /// Optional alias for the principal
        #[serde(default)]
        pub alias: Option<String>,
    } => |client, input| {
        let principal_type = input
            .principal_type
            .map(|pt| parse_principal_type(&pt))
            .transpose()?;

        let request = PrivateLinkAddPrincipalRequest {
            principal: input.principal,
            principal_type,
            alias: input.alias,
        };

        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .add_principals_active_active(input.subscription_id, input.region_id, &request)
            .await
            .tool_context("Failed to add AA PrivateLink principals")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(write, remove_aa_private_link_principals, "remove_aa_private_link_principals",
    "Remove AWS principals from an Active-Active PrivateLink access list.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
        /// AWS principal (account ID, role ARN, etc.) to remove
        pub principal: String,
        /// Principal type: aws_account, organization, organization_unit, iam_role, iam_user, service_principal
        #[serde(default)]
        pub principal_type: Option<String>,
        /// Alias of the principal
        #[serde(default)]
        pub alias: Option<String>,
    } => |client, input| {
        let principal_type = input
            .principal_type
            .map(|pt| parse_principal_type(&pt))
            .transpose()?;

        let request =
            redis_cloud::connectivity::private_link::PrivateLinkRemovePrincipalRequest {
                principal: input.principal,
                principal_type,
                alias: input.alias,
            };

        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .remove_principals_active_active(
                input.subscription_id,
                input.region_id,
                &request,
            )
            .await
            .tool_context("Failed to remove AA PrivateLink principals")?;

        CallToolResult::from_serialize(&result)
    }
);

cloud_tool!(read_only, get_aa_private_link_endpoint_script, "get_aa_private_link_endpoint_script",
    "Get the endpoint creation script for an Active-Active AWS PrivateLink.",
    {
        /// Subscription ID
        pub subscription_id: i32,
        /// Region ID
        pub region_id: i32,
    } => |client, input| {
        let handler = PrivateLinkHandler::new(client);
        let result = handler
            .get_endpoint_script_active_active(input.subscription_id, input.region_id)
            .await
            .tool_context("Failed to get AA PrivateLink endpoint script")?;

        CallToolResult::from_serialize(&result)
    }
);
