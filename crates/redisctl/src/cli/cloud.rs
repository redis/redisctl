//! Cloud CLI command definitions

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum CloudConnectivityCommands {
    /// VPC Peering operations
    #[command(subcommand, name = "vpc-peering")]
    VpcPeering(VpcPeeringCommands),
    /// Private Service Connect operations
    #[command(subcommand, name = "psc")]
    Psc(PscCommands),
    /// Transit Gateway operations
    #[command(subcommand, name = "tgw")]
    Tgw(TgwCommands),
    /// AWS PrivateLink operations
    #[command(subcommand, name = "privatelink")]
    PrivateLink(PrivateLinkCommands),
}

/// VPC Peering Commands
#[derive(Subcommand, Debug)]
pub enum VpcPeeringCommands {
    /// Get VPC peering details
    Get {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
    },
    /// Create VPC peering
    #[command(after_help = "EXAMPLES:
    # AWS VPC peering
    redisctl cloud connectivity vpc-peering create --subscription 123 \\
      --region us-east-1 --aws-account-id 123456789012 --vpc-id vpc-abc123

    # AWS VPC peering with CIDR blocks
    redisctl cloud connectivity vpc-peering create --subscription 123 \\
      --region us-east-1 --aws-account-id 123456789012 --vpc-id vpc-abc123 \\
      --vpc-cidr 10.0.0.0/16 --vpc-cidr 10.1.0.0/16

    # GCP VPC peering
    redisctl cloud connectivity vpc-peering create --subscription 123 \\
      --gcp-project-id my-project --gcp-network-name my-network

    # Using JSON data (escape hatch for advanced options)
    redisctl cloud connectivity vpc-peering create --subscription 123 \\
      --data '{\"region\": \"us-east-1\", \"awsAccountId\": \"123456789012\", \"vpcId\": \"vpc-abc123\"}'
")]
    Create {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,

        // AWS-specific parameters
        /// AWS region (e.g., us-east-1)
        #[arg(long, required_unless_present_any = ["gcp_project_id", "data"])]
        region: Option<String>,

        /// AWS account ID (12-digit number)
        #[arg(long, required_unless_present_any = ["gcp_project_id", "data"])]
        aws_account_id: Option<String>,

        /// AWS VPC ID (e.g., vpc-abc123)
        #[arg(long, required_unless_present_any = ["gcp_project_id", "data"])]
        vpc_id: Option<String>,

        // GCP-specific parameters
        /// GCP project ID (for GCP VPC peering)
        #[arg(long, required_unless_present_any = ["region", "data"])]
        gcp_project_id: Option<String>,

        /// GCP network name (for GCP VPC peering)
        #[arg(long, required_unless_present_any = ["region", "data"])]
        gcp_network_name: Option<String>,

        // Common optional parameters
        /// VPC CIDR block (can be specified multiple times)
        #[arg(long = "vpc-cidr", value_name = "CIDR")]
        vpc_cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json (overrides other params)
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update VPC peering
    #[command(after_help = "EXAMPLES:
    # Update VPC CIDR blocks
    redisctl cloud connectivity vpc-peering update --subscription 123 --peering-id 456 \\
      --vpc-cidr 10.0.0.0/16 --vpc-cidr 10.1.0.0/16

    # Using JSON data
    redisctl cloud connectivity vpc-peering update --subscription 123 --peering-id 456 \\
      --data '{\"vpcCidrs\": [\"10.0.0.0/16\", \"10.1.0.0/16\"]}'
")]
    Update {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Peering ID
        #[arg(long)]
        peering_id: i32,

        /// VPC CIDR block (can be specified multiple times)
        #[arg(long = "vpc-cidr", value_name = "CIDR")]
        vpc_cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json (overrides other params)
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete VPC peering
    Delete {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Peering ID
        #[arg(long)]
        peering_id: i32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// List Active-Active VPC peerings
    #[command(name = "list-aa")]
    ListActiveActive {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
    },
    /// Create Active-Active VPC peering
    #[command(
        name = "create-aa",
        after_help = "EXAMPLES:
    # AWS Active-Active VPC peering
    redisctl cloud connectivity vpc-peering create-aa --subscription 123 \\
      --source-region us-east-1 --destination-region us-west-2 \\
      --aws-account-id 123456789012 --vpc-id vpc-abc123

    # AWS Active-Active with CIDR blocks
    redisctl cloud connectivity vpc-peering create-aa --subscription 123 \\
      --source-region us-east-1 --destination-region us-west-2 \\
      --aws-account-id 123456789012 --vpc-id vpc-abc123 \\
      --vpc-cidr 10.0.0.0/16

    # GCP Active-Active VPC peering
    redisctl cloud connectivity vpc-peering create-aa --subscription 123 \\
      --source-region us-central1 --gcp-project-id my-project --gcp-network-name my-network

    # Using JSON data
    redisctl cloud connectivity vpc-peering create-aa --subscription 123 \\
      --data @peering-config.json
"
    )]
    CreateActiveActive {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,

        /// Source region (Redis Cloud region)
        #[arg(long, required_unless_present = "data")]
        source_region: Option<String>,

        // AWS-specific parameters
        /// Destination region for AWS (customer VPC region)
        #[arg(long, required_unless_present_any = ["gcp_project_id", "data"])]
        destination_region: Option<String>,

        /// AWS account ID (12-digit number)
        #[arg(long, required_unless_present_any = ["gcp_project_id", "data"])]
        aws_account_id: Option<String>,

        /// AWS VPC ID (e.g., vpc-abc123)
        #[arg(long, required_unless_present_any = ["gcp_project_id", "data"])]
        vpc_id: Option<String>,

        // GCP-specific parameters
        /// GCP project ID (for GCP VPC peering)
        #[arg(long, required_unless_present_any = ["destination_region", "data"])]
        gcp_project_id: Option<String>,

        /// GCP network name (for GCP VPC peering)
        #[arg(long, required_unless_present_any = ["destination_region", "data"])]
        gcp_network_name: Option<String>,

        // Common optional parameters
        /// VPC CIDR block (can be specified multiple times)
        #[arg(long = "vpc-cidr", value_name = "CIDR")]
        vpc_cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json (overrides other params)
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update Active-Active VPC peering
    #[command(
        name = "update-aa",
        after_help = "EXAMPLES:
    # Update VPC CIDR blocks
    redisctl cloud connectivity vpc-peering update-aa --subscription 123 --peering-id 456 \\
      --vpc-cidr 10.0.0.0/16 --vpc-cidr 10.1.0.0/16

    # Using JSON data
    redisctl cloud connectivity vpc-peering update-aa --subscription 123 --peering-id 456 \\
      --data '{\"vpcCidrs\": [\"10.0.0.0/16\"]}'
"
    )]
    UpdateActiveActive {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Peering ID
        #[arg(long)]
        peering_id: i32,

        /// VPC CIDR block (can be specified multiple times)
        #[arg(long = "vpc-cidr", value_name = "CIDR")]
        vpc_cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json (overrides other params)
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete Active-Active VPC peering
    #[command(name = "delete-aa")]
    DeleteActiveActive {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Peering ID
        #[arg(long)]
        peering_id: i32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

/// Private Service Connect (PSC) Commands
#[derive(Subcommand, Debug)]
pub enum PscCommands {
    // Standard PSC Service operations
    /// Get PSC service details
    #[command(name = "service-get")]
    ServiceGet {
        /// Subscription ID
        subscription_id: i32,
    },
    /// Create PSC service
    #[command(name = "service-create")]
    ServiceCreate {
        /// Subscription ID
        subscription_id: i32,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete PSC service
    #[command(name = "service-delete")]
    ServiceDelete {
        /// Subscription ID
        subscription_id: i32,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    // Standard PSC Endpoint operations
    /// List PSC endpoints
    #[command(name = "endpoints-list")]
    EndpointsList {
        /// Subscription ID
        subscription_id: i32,
        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,
    },
    /// Create PSC endpoint
    #[command(
        name = "endpoint-create",
        after_help = "EXAMPLES:
    # Create PSC endpoint with all parameters
    redisctl cloud connectivity psc endpoint-create 123 \\
      --gcp-project-id my-project \\
      --gcp-vpc-name my-vpc \\
      --gcp-vpc-subnet-name my-subnet \\
      --endpoint-connection-name redis-psc

    # Using JSON file
    redisctl cloud connectivity psc endpoint-create 123 --data @endpoint.json
"
    )]
    EndpointCreate {
        /// Subscription ID
        subscription_id: i32,

        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,

        /// GCP project ID
        #[arg(long, required_unless_present = "data")]
        gcp_project_id: Option<String>,

        /// GCP VPC name
        #[arg(long, required_unless_present = "data")]
        gcp_vpc_name: Option<String>,

        /// GCP VPC subnet name
        #[arg(long, required_unless_present = "data")]
        gcp_vpc_subnet_name: Option<String>,

        /// Endpoint connection name prefix
        #[arg(long)]
        endpoint_connection_name: Option<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update PSC endpoint
    #[command(
        name = "endpoint-update",
        after_help = "EXAMPLES:
    # Update PSC endpoint connection name
    redisctl cloud connectivity psc endpoint-update 123 --endpoint-id 456 \\
      --endpoint-connection-name new-redis-psc

    # Update with JSON
    redisctl cloud connectivity psc endpoint-update 123 --endpoint-id 456 \\
      --data '{\"endpointConnectionName\": \"new-name\"}'
"
    )]
    EndpointUpdate {
        /// Subscription ID
        subscription_id: i32,
        /// Endpoint ID
        #[arg(long)]
        endpoint_id: i32,
        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,

        /// GCP project ID
        #[arg(long)]
        gcp_project_id: Option<String>,

        /// GCP VPC name
        #[arg(long)]
        gcp_vpc_name: Option<String>,

        /// GCP VPC subnet name
        #[arg(long)]
        gcp_vpc_subnet_name: Option<String>,

        /// Endpoint connection name prefix
        #[arg(long)]
        endpoint_connection_name: Option<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete PSC endpoint
    #[command(name = "endpoint-delete")]
    EndpointDelete {
        /// Subscription ID
        subscription_id: i32,
        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,
        /// Endpoint ID
        endpoint_id: i32,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Get PSC endpoint creation script
    #[command(name = "endpoint-creation-script")]
    EndpointCreationScript {
        /// Subscription ID
        subscription_id: i32,
        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,
        /// Endpoint ID
        endpoint_id: i32,
    },
    /// Get PSC endpoint deletion script
    #[command(name = "endpoint-deletion-script")]
    EndpointDeletionScript {
        /// Subscription ID
        subscription_id: i32,
        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,
        /// Endpoint ID
        endpoint_id: i32,
    },

    // Active-Active PSC Service operations
    /// Get Active-Active PSC service details
    #[command(name = "aa-service-get")]
    AaServiceGet {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
    },
    /// Create Active-Active PSC service
    #[command(name = "aa-service-create")]
    AaServiceCreate {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete Active-Active PSC service
    #[command(name = "aa-service-delete")]
    AaServiceDelete {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    // Active-Active PSC Endpoint operations
    /// List Active-Active PSC endpoints
    #[command(name = "aa-endpoints-list")]
    AaEndpointsList {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,
    },
    /// Create Active-Active PSC endpoint
    #[command(
        name = "aa-endpoint-create",
        after_help = "EXAMPLES:
    # Create Active-Active PSC endpoint
    redisctl cloud connectivity psc aa-endpoint-create 123 \\
      --gcp-project-id my-project \\
      --gcp-vpc-name my-vpc \\
      --gcp-vpc-subnet-name my-subnet \\
      --endpoint-connection-name redis-psc

    # Using JSON file
    redisctl cloud connectivity psc aa-endpoint-create 123 --data @endpoint.json
"
    )]
    AaEndpointCreate {
        /// Subscription ID
        subscription_id: i32,

        /// Region ID
        #[arg(long)]
        region_id: i32,

        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,

        /// GCP project ID
        #[arg(long, required_unless_present = "data")]
        gcp_project_id: Option<String>,

        /// GCP VPC name
        #[arg(long, required_unless_present = "data")]
        gcp_vpc_name: Option<String>,

        /// GCP VPC subnet name
        #[arg(long, required_unless_present = "data")]
        gcp_vpc_subnet_name: Option<String>,

        /// Endpoint connection name prefix
        #[arg(long)]
        endpoint_connection_name: Option<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete Active-Active PSC endpoint
    #[command(name = "aa-endpoint-delete")]
    AaEndpointDelete {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
        /// PSC Service ID
        #[arg(long)]
        psc_service_id: i32,
        /// Endpoint ID
        endpoint_id: i32,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

/// Transit Gateway (TGW) Commands
#[derive(Subcommand, Debug)]
pub enum TgwCommands {
    // Standard TGW Attachment operations
    /// List TGW attachments
    #[command(name = "attachments-list")]
    AttachmentsList {
        /// Subscription ID
        subscription_id: i32,
    },
    /// Create TGW attachment
    #[command(
        name = "attachment-create",
        after_help = "EXAMPLES:
    # Create TGW attachment with AWS account and TGW ID
    redisctl cloud connectivity tgw attachment-create 123 \\
      --aws-account-id 123456789012 --tgw-id tgw-abc123

    # Create with CIDR blocks
    redisctl cloud connectivity tgw attachment-create 123 \\
      --aws-account-id 123456789012 --tgw-id tgw-abc123 \\
      --cidr 10.0.0.0/16 --cidr 10.1.0.0/16

    # Using JSON file
    redisctl cloud connectivity tgw attachment-create 123 --data @attachment.json
"
    )]
    AttachmentCreate {
        /// Subscription ID
        subscription_id: i32,

        /// AWS account ID
        #[arg(long, required_unless_present = "data")]
        aws_account_id: Option<String>,

        /// Transit Gateway ID
        #[arg(long, required_unless_present = "data")]
        tgw_id: Option<String>,

        /// CIDR blocks to route through TGW (can be specified multiple times)
        #[arg(long = "cidr", value_name = "CIDR")]
        cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Create TGW attachment with ID in path
    #[command(name = "attachment-create-with-id")]
    AttachmentCreateWithId {
        /// Subscription ID
        subscription_id: i32,
        /// Transit Gateway ID
        tgw_id: String,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update TGW attachment CIDRs
    #[command(
        name = "attachment-update",
        after_help = "EXAMPLES:
    # Update TGW attachment CIDR blocks
    redisctl cloud connectivity tgw attachment-update 123 --attachment-id att-abc123 \\
      --cidr 10.0.0.0/16 --cidr 10.1.0.0/16

    # Using JSON
    redisctl cloud connectivity tgw attachment-update 123 --attachment-id att-abc123 \\
      --data '{\"cidrs\": [\"10.0.0.0/16\"]}'
"
    )]
    AttachmentUpdate {
        /// Subscription ID
        subscription_id: i32,
        /// Attachment ID
        #[arg(long)]
        attachment_id: String,

        /// CIDR blocks to route through TGW (can be specified multiple times)
        #[arg(long = "cidr", value_name = "CIDR")]
        cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete TGW attachment
    #[command(name = "attachment-delete")]
    AttachmentDelete {
        /// Subscription ID
        subscription_id: i32,
        /// Attachment ID
        attachment_id: String,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    // Standard TGW Invitation operations
    /// List TGW resource share invitations
    #[command(name = "invitations-list")]
    InvitationsList {
        /// Subscription ID
        subscription_id: i32,
    },
    /// Accept TGW resource share invitation
    #[command(name = "invitation-accept")]
    InvitationAccept {
        /// Subscription ID
        subscription_id: i32,
        /// Invitation ID
        invitation_id: String,
    },
    /// Reject TGW resource share invitation
    #[command(name = "invitation-reject")]
    InvitationReject {
        /// Subscription ID
        subscription_id: i32,
        /// Invitation ID
        invitation_id: String,
    },

    // Active-Active TGW Attachment operations
    /// List Active-Active TGW attachments
    #[command(name = "aa-attachments-list")]
    AaAttachmentsList {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
    },
    /// Create Active-Active TGW attachment
    #[command(
        name = "aa-attachment-create",
        after_help = "EXAMPLES:
    # Create Active-Active TGW attachment
    redisctl cloud connectivity tgw aa-attachment-create 123 --region-id 1 \\
      --aws-account-id 123456789012 --tgw-id tgw-abc123

    # With CIDR blocks
    redisctl cloud connectivity tgw aa-attachment-create 123 --region-id 1 \\
      --aws-account-id 123456789012 --tgw-id tgw-abc123 \\
      --cidr 10.0.0.0/16

    # Using JSON file
    redisctl cloud connectivity tgw aa-attachment-create 123 --region-id 1 --data @attachment.json
"
    )]
    AaAttachmentCreate {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        #[arg(long)]
        region_id: i32,

        /// AWS account ID
        #[arg(long, required_unless_present = "data")]
        aws_account_id: Option<String>,

        /// Transit Gateway ID
        #[arg(long, required_unless_present = "data")]
        tgw_id: Option<String>,

        /// CIDR blocks to route through TGW (can be specified multiple times)
        #[arg(long = "cidr", value_name = "CIDR")]
        cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update Active-Active TGW attachment CIDRs
    #[command(
        name = "aa-attachment-update",
        after_help = "EXAMPLES:
    # Update Active-Active TGW attachment CIDR blocks
    redisctl cloud connectivity tgw aa-attachment-update 123 --region-id 1 --attachment-id att-abc123 \\
      --cidr 10.0.0.0/16 --cidr 10.1.0.0/16

    # Using JSON
    redisctl cloud connectivity tgw aa-attachment-update 123 --region-id 1 --attachment-id att-abc123 \\
      --data '{\"cidrs\": [\"10.0.0.0/16\"]}'
"
    )]
    AaAttachmentUpdate {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        #[arg(long)]
        region_id: i32,
        /// Attachment ID
        #[arg(long)]
        attachment_id: String,

        /// CIDR blocks to route through TGW (can be specified multiple times)
        #[arg(long = "cidr", value_name = "CIDR")]
        cidrs: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete Active-Active TGW attachment
    #[command(name = "aa-attachment-delete")]
    AaAttachmentDelete {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
        /// Attachment ID
        attachment_id: String,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    // Active-Active TGW Invitation operations
    /// List Active-Active TGW resource share invitations
    #[command(name = "aa-invitations-list")]
    AaInvitationsList {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
    },
    /// Accept Active-Active TGW resource share invitation
    #[command(name = "aa-invitation-accept")]
    AaInvitationAccept {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
        /// Invitation ID
        invitation_id: String,
    },
    /// Reject Active-Active TGW resource share invitation
    #[command(name = "aa-invitation-reject")]
    AaInvitationReject {
        /// Subscription ID
        subscription_id: i32,
        /// Region ID
        region_id: i32,
        /// Invitation ID
        invitation_id: String,
    },
}

/// AWS PrivateLink Commands
#[derive(Subcommand, Debug)]
pub enum PrivateLinkCommands {
    /// Get PrivateLink configuration
    Get {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Region ID (for Active-Active databases)
        #[arg(long)]
        region: Option<i32>,
    },
    /// Create PrivateLink
    #[command(after_help = "EXAMPLES:
    # Create PrivateLink with AWS account principal
    redisctl cloud connectivity privatelink create --subscription 123 \\
      --share-name my-redis-share --principal 123456789012 --type aws-account

    # Create PrivateLink with alias
    redisctl cloud connectivity privatelink create --subscription 123 \\
      --share-name my-redis-share --principal 123456789012 \\
      --type aws-account --alias 'Production Account'

    # Create for Active-Active subscription
    redisctl cloud connectivity privatelink create --subscription 123 --region 1 \\
      --share-name my-redis-share --principal 123456789012 --type aws-account

    # Using JSON data (escape hatch)
    redisctl cloud connectivity privatelink create --subscription 123 \\
      --data '{\"shareName\": \"my-share\", \"principal\": \"123456789012\", \"type\": \"aws_account\"}'
")]
    Create {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Region ID (for Active-Active databases)
        #[arg(long)]
        region: Option<i32>,

        /// Share name for the PrivateLink service
        #[arg(long, required_unless_present = "data")]
        share_name: Option<String>,

        /// AWS principal (account ID or ARN)
        #[arg(long, required_unless_present = "data")]
        principal: Option<String>,

        /// Principal type (aws-account, iam-role, etc.)
        #[arg(long = "type", value_name = "TYPE", required_unless_present = "data")]
        principal_type: Option<String>,

        /// Alias for the principal (optional)
        #[arg(long)]
        alias: Option<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Add principals to PrivateLink
    #[command(
        name = "add-principal",
        after_help = "EXAMPLES:
    # Add an AWS account as principal
    redisctl cloud connectivity privatelink add-principal --subscription 123 \\
      --principal 123456789012 --type aws-account

    # Add with alias
    redisctl cloud connectivity privatelink add-principal --subscription 123 \\
      --principal 123456789012 --type aws-account --alias 'Dev Account'

    # Add to Active-Active subscription
    redisctl cloud connectivity privatelink add-principal --subscription 123 --region 1 \\
      --principal 123456789012 --type aws-account
"
    )]
    AddPrincipal {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Region ID (for Active-Active databases)
        #[arg(long)]
        region: Option<i32>,

        /// AWS principal (account ID or ARN)
        #[arg(long, required_unless_present = "data")]
        principal: Option<String>,

        /// Principal type (aws-account, iam-role, etc.)
        #[arg(long = "type", value_name = "TYPE")]
        principal_type: Option<String>,

        /// Alias for the principal (optional)
        #[arg(long)]
        alias: Option<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },
    /// Remove principals from PrivateLink
    #[command(
        name = "remove-principal",
        after_help = "EXAMPLES:
    # Remove an AWS account principal
    redisctl cloud connectivity privatelink remove-principal --subscription 123 \\
      --principal 123456789012 --type aws-account

    # Remove from Active-Active subscription
    redisctl cloud connectivity privatelink remove-principal --subscription 123 --region 1 \\
      --principal 123456789012 --type aws-account
"
    )]
    RemovePrincipal {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Region ID (for Active-Active databases)
        #[arg(long)]
        region: Option<i32>,

        /// AWS principal (account ID or ARN)
        #[arg(long, required_unless_present = "data")]
        principal: Option<String>,

        /// Principal type (aws-account, iam-role, etc.)
        #[arg(long = "type", value_name = "TYPE")]
        principal_type: Option<String>,

        /// Alias for the principal (optional)
        #[arg(long)]
        alias: Option<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },
    /// Get VPC endpoint creation script
    #[command(name = "get-script")]
    GetScript {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Region ID (for Active-Active databases)
        #[arg(long)]
        region: Option<i32>,
    },
    /// Delete PrivateLink configuration
    Delete {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

/// Cloud Task Commands
#[derive(Subcommand, Debug)]
pub enum CloudTaskCommands {
    /// List all tasks for this account
    #[command(alias = "ls")]
    List,
    /// Get task status and details
    Get {
        /// Task ID (UUID format)
        id: String,
    },
    /// Wait for task to complete
    Wait {
        /// Task ID (UUID format)
        id: String,
        /// Maximum time to wait in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Polling interval in seconds
        #[arg(long, default_value = "2")]
        interval: u64,
    },
    /// Poll task status with live updates
    Poll {
        /// Task ID (UUID format)
        id: String,
        /// Polling interval in seconds
        #[arg(long, default_value = "2")]
        interval: u64,
        /// Maximum number of polls (0 = unlimited)
        #[arg(long, default_value = "0")]
        max_polls: u64,
    },
}

/// Cloud Fixed Database Commands
#[derive(Subcommand, Debug)]
pub enum CloudFixedDatabaseCommands {
    /// List all databases in a fixed subscription
    List {
        /// Subscription ID
        subscription_id: i32,
    },
    /// Get details of a specific fixed database
    Get {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },
    /// Create a new database in a fixed subscription
    #[command(after_help = "EXAMPLES:
    # Simple database with name
    redisctl cloud fixed-database create 123456 --name mydb --wait

    # Database with password
    redisctl cloud fixed-database create 123456 \\
      --name mydb \\
      --password mysecret \\
      --wait

    # Advanced: Use JSON for full configuration
    redisctl cloud fixed-database create 123456 \\
      --data @database.json
")]
    Create {
        /// Subscription ID
        subscription_id: i32,

        /// Database name
        #[arg(long)]
        name: Option<String>,

        /// Database password
        #[arg(long)]
        password: Option<String>,

        /// Enable TLS/SSL
        #[arg(long)]
        enable_tls: Option<bool>,

        /// Data eviction policy
        #[arg(long)]
        eviction_policy: Option<String>,

        /// Enable replication
        #[arg(long)]
        replication: Option<bool>,

        /// Data persistence setting
        #[arg(long)]
        data_persistence: Option<String>,

        /// Advanced: Full database configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update fixed database configuration
    #[command(after_help = "EXAMPLES:
    # Update database name
    redisctl cloud fixed-database update 123:456 --name new-name

    # Change password
    redisctl cloud fixed-database update 123:456 --password newpassword

    # Enable TLS
    redisctl cloud fixed-database update 123:456 --enable-tls true

    # Multiple changes
    redisctl cloud fixed-database update 123:456 \\
      --eviction-policy allkeys-lru \\
      --data-persistence aof-every-1-second \\
      --wait
")]
    Update {
        /// Database ID (format: subscription_id:database_id)
        id: String,

        /// New database name
        #[arg(long)]
        name: Option<String>,

        /// New password
        #[arg(long)]
        password: Option<String>,

        /// Enable TLS/SSL
        #[arg(long)]
        enable_tls: Option<bool>,

        /// Data eviction policy
        #[arg(long)]
        eviction_policy: Option<String>,

        /// Enable replication
        #[arg(long)]
        replication: Option<bool>,

        /// Data persistence setting
        #[arg(long)]
        data_persistence: Option<String>,

        /// Advanced: Full update configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete a fixed database
    Delete {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Get backup status for fixed database
    #[command(name = "backup-status")]
    BackupStatus {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },
    /// Trigger manual backup
    Backup {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Get import status
    #[command(name = "import-status")]
    ImportStatus {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },
    /// Import data into fixed database
    #[command(after_help = "EXAMPLES:
    # Import from S3
    redisctl cloud fixed-database import 123:456 \\
      --source-type s3 \\
      --import-from-uri s3://bucket/backup.rdb \\
      --wait

    # Import from HTTP
    redisctl cloud fixed-database import 123:456 \\
      --source-type http \\
      --import-from-uri https://example.com/backup.rdb

    # Import with AWS credentials
    redisctl cloud fixed-database import 123:456 \\
      --source-type aws-s3 \\
      --import-from-uri s3://bucket/backup.rdb \\
      --aws-access-key AKIA... \\
      --aws-secret-key secret
")]
    Import {
        /// Database ID (format: subscription_id:database_id)
        id: String,

        /// Source type: http, redis, ftp, aws-s3, gcs, azure-blob-storage
        #[arg(long)]
        source_type: Option<String>,

        /// URI to import from
        #[arg(long)]
        import_from_uri: Option<String>,

        /// AWS access key ID (for aws-s3)
        #[arg(long)]
        aws_access_key: Option<String>,

        /// AWS secret access key (for aws-s3)
        #[arg(long)]
        aws_secret_key: Option<String>,

        /// GCS client email (for gcs)
        #[arg(long)]
        gcs_client_email: Option<String>,

        /// GCS private key (for gcs)
        #[arg(long)]
        gcs_private_key: Option<String>,

        /// Azure storage account name
        #[arg(long)]
        azure_account_name: Option<String>,

        /// Azure storage account key
        #[arg(long)]
        azure_account_key: Option<String>,

        /// Advanced: Full import configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Get slow query log
    #[command(name = "slow-log")]
    SlowLog {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Maximum number of entries to return
        #[arg(long, default_value = "100")]
        limit: i32,
        /// Offset for pagination
        #[arg(long, default_value = "0")]
        offset: i32,
    },
    /// List tags for fixed database
    #[command(name = "list-tags")]
    ListTags {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },
    /// Add a tag
    #[command(name = "add-tag")]
    AddTag {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Tag key
        #[arg(long)]
        key: String,
        /// Tag value
        #[arg(long)]
        value: String,
    },
    /// Update all tags
    #[command(
        name = "update-tags",
        after_help = "EXAMPLES:
    # Set multiple tags
    redisctl cloud fixed-database update-tags 123:456 \\
      --tag env=production \\
      --tag team=backend

    # Replace all tags using JSON
    redisctl cloud fixed-database update-tags 123:456 \\
      --data '{\"tags\": [{\"key\": \"env\", \"value\": \"prod\"}]}'
"
    )]
    UpdateTags {
        /// Database ID (format: subscription_id:database_id)
        id: String,

        /// Tag in key=value format (repeatable)
        #[arg(long = "tag", value_name = "KEY=VALUE")]
        tags: Vec<String>,

        /// Tags as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },
    /// Update specific tag
    #[command(name = "update-tag")]
    UpdateTag {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Tag key
        #[arg(long)]
        key: String,
        /// Tag value
        #[arg(long)]
        value: String,
    },
    /// Delete a tag
    #[command(name = "delete-tag")]
    DeleteTag {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Tag key
        #[arg(long)]
        key: String,
    },
    /// Get available Redis versions for upgrade
    #[command(name = "available-versions")]
    AvailableVersions {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },
    /// Get Redis version upgrade status
    #[command(name = "upgrade-status")]
    UpgradeStatus {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },
    /// Upgrade Redis version
    #[command(name = "upgrade-redis")]
    UpgradeRedis {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Target Redis version
        #[arg(long)]
        version: String,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

/// Cloud Fixed Subscription Commands
#[derive(Subcommand, Debug)]
pub enum CloudFixedSubscriptionCommands {
    /// List all available fixed subscription plans
    #[command(name = "list-plans")]
    ListPlans {
        /// Filter by cloud provider (AWS, GCP, Azure)
        #[arg(long)]
        provider: Option<String>,
    },
    /// Get plans for a specific subscription
    #[command(name = "get-plans")]
    GetPlans {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
    },
    /// Get details of a specific plan
    #[command(name = "get-plan")]
    GetPlan {
        /// Plan ID
        id: i32,
    },
    /// List all fixed subscriptions
    List,
    /// Get details of a fixed subscription
    Get {
        /// Subscription ID
        id: i32,
    },
    /// Create a new fixed subscription
    #[command(after_help = "EXAMPLES:
    # Create subscription with name and plan ID
    redisctl cloud fixed-subscription create --name my-cache --plan-id 12345 --wait

    # Create with specific payment method
    redisctl cloud fixed-subscription create \\
      --name prod-cache \\
      --plan-id 12345 \\
      --payment-method credit-card \\
      --payment-method-id 67890

    # Create using JSON for full control
    redisctl cloud fixed-subscription create \\
      --data '{\"name\": \"my-cache\", \"planId\": 12345}'
")]
    Create {
        /// Subscription name
        #[arg(long)]
        name: Option<String>,

        /// Plan ID from list-plans
        #[arg(long)]
        plan_id: Option<i32>,

        /// Payment method (credit-card or marketplace)
        #[arg(long)]
        payment_method: Option<String>,

        /// Payment method ID (required for credit-card)
        #[arg(long)]
        payment_method_id: Option<i32>,

        /// JSON data (string or @filename)
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update fixed subscription
    #[command(after_help = "EXAMPLES:
    # Rename subscription
    redisctl cloud fixed-subscription update 123456 --name new-name

    # Change plan
    redisctl cloud fixed-subscription update 123456 --plan-id 67890 --wait

    # Change payment method
    redisctl cloud fixed-subscription update 123456 \\
      --payment-method credit-card \\
      --payment-method-id 11111

    # Update using JSON
    redisctl cloud fixed-subscription update 123456 \\
      --data '{\"name\": \"new-name\"}'
")]
    Update {
        /// Subscription ID
        id: i32,

        /// New subscription name
        #[arg(long)]
        name: Option<String>,

        /// New plan ID
        #[arg(long)]
        plan_id: Option<i32>,

        /// Payment method (credit-card or marketplace)
        #[arg(long)]
        payment_method: Option<String>,

        /// Payment method ID (required for credit-card)
        #[arg(long)]
        payment_method_id: Option<i32>,

        /// JSON data (string or @filename)
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete a fixed subscription
    Delete {
        /// Subscription ID
        id: i32,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Get available Redis versions for fixed subscription
    #[command(name = "redis-versions")]
    RedisVersions {
        /// Subscription ID
        #[arg(long)]
        subscription: i32,
    },
}

/// Cloud Provider Account Commands
#[derive(Subcommand, Debug)]
pub enum CloudProviderAccountCommands {
    /// List all cloud provider accounts
    List,
    /// Get cloud provider account details
    Get {
        /// Cloud account ID
        account_id: i32,
    },
    /// Create a new cloud provider account
    #[command(after_help = "EXAMPLES:
    # Create AWS cloud account with credentials
    redisctl cloud provider-account create \\
      --name 'Production AWS' \\
      --provider AWS \\
      --access-key-id AKIA... \\
      --access-secret-key secret \\
      --console-username admin@example.com \\
      --console-password mypassword \\
      --sign-in-login-url https://console.aws.amazon.com

    # Create using JSON file (for GCP service account)
    redisctl cloud provider-account create --data @gcp-service-account.json

    # Create with JSON string
    redisctl cloud provider-account create \\
      --data '{\"name\": \"My Account\", \"provider\": \"AWS\", ...}'
")]
    Create {
        /// Account display name
        #[arg(long)]
        name: Option<String>,

        /// Cloud provider (AWS, GCP, Azure)
        #[arg(long)]
        provider: Option<String>,

        /// Cloud provider access key ID
        #[arg(long)]
        access_key_id: Option<String>,

        /// Cloud provider secret access key
        #[arg(long)]
        access_secret_key: Option<String>,

        /// Cloud provider console username
        #[arg(long)]
        console_username: Option<String>,

        /// Cloud provider console password
        #[arg(long)]
        console_password: Option<String>,

        /// Cloud provider console login URL
        #[arg(long)]
        sign_in_login_url: Option<String>,

        /// JSON data (string or @filename) - use for GCP service account JSON
        #[arg(long)]
        data: Option<String>,

        /// Async operation arguments
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Update a cloud provider account
    #[command(after_help = "EXAMPLES:
    # Update account name
    redisctl cloud provider-account update 123 --name 'New Name'

    # Update credentials
    redisctl cloud provider-account update 123 \\
      --access-key-id AKIA... \\
      --access-secret-key newsecret

    # Update using JSON
    redisctl cloud provider-account update 123 --data @updated-config.json
")]
    Update {
        /// Cloud account ID
        account_id: i32,

        /// New account display name
        #[arg(long)]
        name: Option<String>,

        /// New access key ID
        #[arg(long)]
        access_key_id: Option<String>,

        /// New secret access key
        #[arg(long)]
        access_secret_key: Option<String>,

        /// New console username
        #[arg(long)]
        console_username: Option<String>,

        /// New console password
        #[arg(long)]
        console_password: Option<String>,

        /// New console login URL
        #[arg(long)]
        sign_in_login_url: Option<String>,

        /// JSON data (string or @filename)
        #[arg(long)]
        data: Option<String>,

        /// Async operation arguments
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
    /// Delete a cloud provider account
    Delete {
        /// Cloud account ID
        account_id: i32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Async operation arguments
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

/// Cloud-specific commands
#[derive(Subcommand, Debug)]
#[command(after_long_help = "\
COMMAND GROUPS:
  Core:       database (db), subscription (sub), fixed-database (fixed-db),
              fixed-subscription (fixed-sub)
  Access:     user, acl
  Billing:    account (acct), payment-method, cost-report
  Networking: connectivity (conn), provider-account
  Operations: task, workflow

Aliases shown in parentheses (e.g. 'cloud db list' instead of 'cloud database list').")]
pub enum CloudCommands {
    // -- Authentication --
    /// Log in to Redis Cloud and manage the session (browser or device flow)
    #[command(subcommand, display_order = 0)]
    Auth(CloudAuthCommands),

    // -- Core --
    /// Database operations
    #[command(subcommand, display_order = 1, visible_alias = "db")]
    Database(CloudDatabaseCommands),

    /// Subscription operations
    #[command(subcommand, display_order = 2, visible_alias = "sub")]
    Subscription(CloudSubscriptionCommands),

    /// Fixed database operations (Essentials)
    #[command(
        subcommand,
        name = "fixed-database",
        display_order = 3,
        visible_alias = "fixed-db"
    )]
    FixedDatabase(CloudFixedDatabaseCommands),

    /// Fixed subscription operations (Essentials)
    #[command(
        subcommand,
        name = "fixed-subscription",
        display_order = 4,
        visible_alias = "fixed-sub"
    )]
    FixedSubscription(CloudFixedSubscriptionCommands),

    // -- Access Control --
    /// User operations
    #[command(subcommand, display_order = 10)]
    User(CloudUserCommands),

    /// ACL (Access Control List) operations
    #[command(subcommand, display_order = 11)]
    Acl(CloudAclCommands),

    // -- Billing --
    /// Account operations
    #[command(subcommand, display_order = 20, visible_alias = "acct")]
    Account(CloudAccountCommands),

    /// Payment method operations
    #[command(subcommand, name = "payment-method", display_order = 21)]
    PaymentMethod(CloudPaymentMethodCommands),

    /// Cost report operations (Beta)
    #[command(subcommand, name = "cost-report", display_order = 22)]
    CostReport(CloudCostReportCommands),

    // -- Networking --
    /// Network connectivity operations (VPC, PSC, TGW)
    #[command(subcommand, display_order = 30, visible_alias = "conn")]
    Connectivity(CloudConnectivityCommands),

    /// Cloud provider account operations
    #[command(subcommand, name = "provider-account", display_order = 31)]
    ProviderAccount(CloudProviderAccountCommands),

    // -- Operations --
    /// Task operations
    #[command(subcommand, display_order = 40)]
    Task(CloudTaskCommands),

    /// Workflow operations for multi-step tasks
    #[command(subcommand, display_order = 41)]
    Workflow(CloudWorkflowCommands),
}
/// Authentication subcommands for Redis Cloud.
#[derive(Debug, Subcommand)]
pub enum CloudAuthCommands {
    /// Log in to Redis Cloud and store credentials (mints a CAPI key).
    ///
    /// Opens a browser by default; use --device for a headless URL + code flow.
    Login {
        /// Use the device-authorization flow (print a URL + code) instead of opening a browser.
        #[arg(long)]
        device: bool,
        /// Store credentials as plaintext in the config file when no OS keyring is available.
        #[arg(long)]
        allow_plaintext: bool,
        /// Block until the device flow is approved (one-shot). Without this, the device flow
        /// returns the code immediately and `auth status --wait` completes the login — the
        /// agent-friendly split. The browser (loopback) flow always blocks.
        #[arg(long)]
        wait: bool,
    },
    /// Report whether the profile is authenticated to Redis Cloud.
    Status {
        /// Block until a pending device-authorization login completes (or times out / is
        /// denied), then run the exchange and persist credentials.
        #[arg(long)]
        wait: bool,
        /// Max seconds to wait when --wait is set.
        #[arg(long, default_value = "600")]
        timeout: u64,
    },
    /// Remove the locally stored Redis Cloud credentials for the profile.
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum CloudWorkflowCommands {
    /// List available workflows
    List,
    /// Complete subscription setup with optional database
    #[command(name = "subscription-setup")]
    SubscriptionSetup(crate::workflows::cloud::subscription_setup::SubscriptionSetupArgs),
}

/// Cloud Cost Report Commands (Beta)
#[derive(Debug, Clone, Subcommand)]
pub enum CloudCostReportCommands {
    /// Generate a cost report in FOCUS format
    #[command(after_help = "EXAMPLES:
    # Generate a cost report for January 2025
    redisctl cloud cost-report generate --start-date 2025-01-01 --end-date 2025-01-31

    # Generate CSV report filtered by subscription
    redisctl cloud cost-report generate --start-date 2025-01-01 --end-date 2025-01-31 \\
      --subscription 123 --format csv

    # Generate JSON report filtered by region and tags
    redisctl cloud cost-report generate --start-date 2025-01-01 --end-date 2025-01-31 \\
      --format json --region us-east-1 --tag team:marketing

    # Generate report for Pro subscriptions only
    redisctl cloud cost-report generate --start-date 2025-01-01 --end-date 2025-01-31 \\
      --subscription-type pro

NOTE: The maximum date range is 40 days. Cost reports are generated asynchronously.
      Use --wait to wait for completion, or use 'cloud task get' to check status.
      Once complete, use 'cloud cost-report download' with the costReportId from the task.
")]
    Generate {
        /// Start date (YYYY-MM-DD format)
        #[arg(long)]
        start_date: String,

        /// End date (YYYY-MM-DD format, max 40 days from start)
        #[arg(long)]
        end_date: String,

        /// Output format (csv or json)
        #[arg(long, value_parser = ["csv", "json"], default_value = "csv")]
        format: String,

        /// Filter by subscription IDs (can be specified multiple times)
        #[arg(long = "subscription", value_name = "ID")]
        subscription_ids: Vec<i32>,

        /// Filter by database IDs (can be specified multiple times)
        #[arg(long = "database", value_name = "ID")]
        database_ids: Vec<i32>,

        /// Filter by subscription type (pro or essentials)
        #[arg(long, value_parser = ["pro", "essentials"])]
        subscription_type: Option<String>,

        /// Filter by regions (can be specified multiple times)
        #[arg(long = "region", value_name = "REGION")]
        regions: Vec<String>,

        /// Filter by tags (format: key:value, can be specified multiple times)
        #[arg(long = "tag", value_name = "KEY:VALUE")]
        tags: Vec<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Download a generated cost report
    #[command(after_help = "EXAMPLES:
    # Download cost report to stdout
    redisctl cloud cost-report download cost-report-12345-abcdef

    # Download cost report to a file
    redisctl cloud cost-report download cost-report-12345-abcdef --file report.csv

NOTE: The costReportId is returned in the task response after the generation completes.
      Check task status with 'redisctl cloud task get <task-id>' to get the costReportId.
")]
    Download {
        /// Cost report ID (from the completed generation task)
        cost_report_id: String,

        /// Output file path (defaults to stdout if not specified)
        #[arg(long = "file", short = 'f')]
        file: Option<String>,
    },

    /// Generate and download a cost report in one step
    #[command(after_help = "EXAMPLES:
    # Export January 2025 costs to CSV file
    redisctl cloud cost-report export --start-date 2025-01-01 --end-date 2025-01-31 \\
      --file january-costs.csv

    # Export as JSON to stdout
    redisctl cloud cost-report export --start-date 2025-01-01 --end-date 2025-01-31 \\
      --format json

    # Export filtered by subscription and tags
    redisctl cloud cost-report export --start-date 2025-01-01 --end-date 2025-01-31 \\
      --subscription 12345 --tag team:platform --file team-costs.csv

NOTE: This command combines 'generate --wait' and 'download' into a single operation.
      The maximum date range is 40 days.
")]
    Export {
        /// Start date (YYYY-MM-DD format)
        #[arg(long)]
        start_date: String,

        /// End date (YYYY-MM-DD format, max 40 days from start)
        #[arg(long)]
        end_date: String,

        /// Output format (csv or json)
        #[arg(long, value_parser = ["csv", "json"], default_value = "csv")]
        format: String,

        /// Output file path (defaults to stdout if not specified)
        #[arg(long = "file", short = 'f')]
        file: Option<String>,

        /// Filter by subscription IDs (can be specified multiple times)
        #[arg(long = "subscription", value_name = "ID")]
        subscription_ids: Vec<i32>,

        /// Filter by database IDs (can be specified multiple times)
        #[arg(long = "database", value_name = "ID")]
        database_ids: Vec<i32>,

        /// Filter by subscription type (pro or essentials)
        #[arg(long, value_parser = ["pro", "essentials"])]
        subscription_type: Option<String>,

        /// Filter by regions (can be specified multiple times)
        #[arg(long = "region", value_name = "REGION")]
        regions: Vec<String>,

        /// Filter by tags (format: key:value, can be specified multiple times)
        #[arg(long = "tag", value_name = "KEY:VALUE")]
        tags: Vec<String>,

        /// Maximum time to wait for report generation in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
    },
}

/// Enterprise workflow commands
#[derive(Subcommand, Debug)]
pub enum CloudAccountCommands {
    /// Get account information
    Get,

    /// Get payment methods configured for the account
    GetPaymentMethods,

    /// List supported regions
    ListRegions {
        /// Filter by cloud provider (aws, gcp, azure)
        #[arg(long)]
        provider: Option<String>,
    },

    /// List supported Redis modules
    ListModules,

    /// Get data persistence options
    GetPersistenceOptions,

    /// Get system logs
    GetSystemLogs {
        /// Maximum number of logs to return
        #[arg(long, default_value = "100")]
        limit: Option<u32>,

        /// Offset for pagination
        #[arg(long, default_value = "0")]
        offset: Option<u32>,
    },

    /// Get session/audit logs
    GetSessionLogs {
        /// Maximum number of logs to return
        #[arg(long, default_value = "100")]
        limit: Option<u32>,

        /// Offset for pagination
        #[arg(long, default_value = "0")]
        offset: Option<u32>,
    },

    /// Get search module scaling factors
    GetSearchScaling,
}

#[derive(Subcommand, Debug)]
pub enum CloudPaymentMethodCommands {
    /// List payment methods configured for the account
    List,
}

#[derive(Subcommand, Debug)]
pub enum CloudSubscriptionCommands {
    /// List all subscriptions
    List,

    /// Get detailed subscription information
    Get {
        /// Subscription ID
        id: u32,
    },

    /// Create a new subscription
    #[command(after_help = "EXAMPLES:
    # Simple subscription - just name, provider, and region via --data
    redisctl cloud subscription create --name prod-subscription \\
      --data '{\"cloudProviders\":[{\"regions\":[{\"region\":\"us-east-1\"}]}],\"databases\":[{\"name\":\"db1\",\"memoryLimitInGb\":1}]}'

    # With payment method
    redisctl cloud subscription create --name dev-subscription \\
      --payment-method marketplace \\
      --data '{\"cloudProviders\":[{\"regions\":[{\"region\":\"us-west-2\"}]}],\"databases\":[{\"name\":\"db1\",\"memoryLimitInGb\":1}]}'

    # With auto-tiering (RAM+Flash)
    redisctl cloud subscription create --name large-subscription \\
      --memory-storage ram-and-flash \\
      --data '{\"cloudProviders\":[{\"provider\":\"AWS\",\"regions\":[{\"region\":\"eu-west-1\"}]}],\"databases\":[{\"name\":\"db1\",\"memoryLimitInGb\":10}]}'

    # Complete configuration from file
    redisctl cloud subscription create --data @subscription.json

    # Dry run to preview deployment
    redisctl cloud subscription create --dry-run --data @subscription.json

NOTE: Subscription creation requires complex nested structures for cloud providers,
      regions, and databases. Use --data for the required cloudProviders and databases
      arrays. First-class parameters (--name, --payment-method, etc.) override values
      in --data when both are provided.")]
    Create {
        /// Subscription name
        #[arg(long)]
        name: Option<String>,

        /// Dry run - create deployment plan without provisioning resources
        #[arg(long)]
        dry_run: bool,

        /// Deployment type: single-region or active-active
        #[arg(long, value_parser = ["single-region", "active-active"])]
        deployment_type: Option<String>,

        /// Payment method: credit-card or marketplace
        #[arg(long, value_parser = ["credit-card", "marketplace"], default_value = "credit-card")]
        payment_method: String,

        /// Payment method ID (required if payment-method is credit-card)
        #[arg(long)]
        payment_method_id: Option<i32>,

        /// Memory storage: ram or ram-and-flash (Auto Tiering)
        #[arg(long, value_parser = ["ram", "ram-and-flash"], default_value = "ram")]
        memory_storage: String,

        /// Persistent storage encryption: cloud-provider-managed-key or customer-managed-key
        #[arg(long, value_parser = ["cloud-provider-managed-key", "customer-managed-key"], default_value = "cloud-provider-managed-key")]
        persistent_storage_encryption: String,

        /// Advanced: Full subscription configuration as JSON string or @file.json
        /// REQUIRED: Must include cloudProviders array with regions and databases array
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Update subscription configuration
    #[command(after_help = "EXAMPLES:
    # Update subscription name
    redisctl cloud subscription update 123 --name new-name

    # Update payment method
    redisctl cloud subscription update 123 --payment-method marketplace

    # Using JSON for advanced options
    redisctl cloud subscription update 123 --data '{\"name\": \"new-name\"}'
")]
    Update {
        /// Subscription ID
        id: u32,

        /// Updated subscription name
        #[arg(long)]
        name: Option<String>,

        /// Payment method: credit-card or marketplace
        #[arg(long, value_parser = ["credit-card", "marketplace"])]
        payment_method: Option<String>,

        /// Payment method ID (required if payment-method is credit-card)
        #[arg(long)]
        payment_method_id: Option<i32>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Dry run - validate without applying changes
        #[arg(long)]
        dry_run: bool,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Delete a subscription
    Delete {
        /// Subscription ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Dry run - show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Get available Redis versions
    RedisVersions {
        /// Filter by subscription ID (optional)
        #[arg(long)]
        subscription: Option<u32>,
    },

    /// Get subscription pricing information
    GetPricing {
        /// Subscription ID
        id: u32,
    },

    /// Get CIDR allowlist
    GetCidrAllowlist {
        /// Subscription ID
        id: u32,
    },

    /// Update CIDR allowlist
    #[command(after_help = "EXAMPLES:
    # Set CIDR blocks
    redisctl cloud subscription update-cidr-allowlist 123 \\
      --cidr 10.0.0.0/24 --cidr 192.168.1.0/24

    # Set AWS security groups
    redisctl cloud subscription update-cidr-allowlist 123 \\
      --security-group sg-12345678

    # Mix both
    redisctl cloud subscription update-cidr-allowlist 123 \\
      --cidr 10.0.0.0/24 --security-group sg-12345678

    # Using JSON
    redisctl cloud subscription update-cidr-allowlist 123 \\
      --data '{\"cidrIps\": [\"10.0.0.0/24\"], \"securityGroupIds\": [\"sg-12345678\"]}'
")]
    UpdateCidrAllowlist {
        /// Subscription ID
        id: u32,

        /// CIDR block to allow (can be specified multiple times)
        #[arg(long = "cidr", value_name = "CIDR")]
        cidrs: Vec<String>,

        /// AWS Security Group ID to allow (can be specified multiple times)
        #[arg(long = "security-group", value_name = "SG_ID")]
        security_groups: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Get maintenance windows
    GetMaintenanceWindows {
        /// Subscription ID
        id: u32,
    },

    /// Update maintenance windows
    #[command(after_help = "EXAMPLES:
    # Set automatic maintenance
    redisctl cloud subscription update-maintenance-windows 123 --mode automatic

    # Set manual maintenance windows (up to 7 windows)
    redisctl cloud subscription update-maintenance-windows 123 \\
      --mode manual \\
      --window 'monday:03:00-monday:07:00' \\
      --window 'thursday:03:00-thursday:07:00'

    # Using JSON for complex configurations
    redisctl cloud subscription update-maintenance-windows 123 \\
      --data '{\"mode\": \"manual\", \"windows\": [{\"startHour\": 3, \"durationInHours\": 4, \"days\": [\"Monday\"]}]}'
")]
    UpdateMaintenanceWindows {
        /// Subscription ID
        id: u32,

        /// Maintenance mode: automatic or manual
        #[arg(long, value_parser = ["automatic", "manual"])]
        mode: Option<String>,

        /// Maintenance window in format 'day:HH:MM-day:HH:MM' (can be specified multiple times, up to 7)
        /// Example: 'monday:03:00-monday:07:00'
        #[arg(long = "window", value_name = "WINDOW")]
        windows: Vec<String>,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// List Active-Active regions
    ListAaRegions {
        /// Subscription ID
        id: u32,
    },

    /// Add region to Active-Active subscription
    #[command(after_help = "EXAMPLES:
    # Add a new region with required parameters
    redisctl cloud subscription add-aa-region 123 \\
      --region us-west-2 \\
      --deployment-cidr 10.1.0.0/24

    # Add region with existing VPC
    redisctl cloud subscription add-aa-region 123 \\
      --region eu-west-1 \\
      --deployment-cidr 10.2.0.0/24 \\
      --vpc-id vpc-abc123

    # Using JSON for advanced options
    redisctl cloud subscription add-aa-region 123 \\
      --data @region-config.json
")]
    AddAaRegion {
        /// Subscription ID
        id: u32,

        /// Cloud provider region name (e.g., us-west-2, eu-west-1)
        #[arg(long, required_unless_present = "data")]
        region: Option<String>,

        /// Deployment CIDR block (must be /24, e.g., 10.1.0.0/24)
        #[arg(long, required_unless_present = "data")]
        deployment_cidr: Option<String>,

        /// Existing VPC ID to use (optional, creates new VPC if not specified)
        #[arg(long)]
        vpc_id: Option<String>,

        /// RESP version (must be compatible with Redis version)
        #[arg(long)]
        resp_version: Option<String>,

        /// Dry run - create deployment plan without provisioning
        #[arg(long)]
        dry_run: bool,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Delete regions from Active-Active subscription
    #[command(after_help = "EXAMPLES:
    # Delete a single region
    redisctl cloud subscription delete-aa-regions 123 --region us-west-2

    # Delete multiple regions
    redisctl cloud subscription delete-aa-regions 123 \\
      --region us-west-2 --region eu-west-1

    # Dry run to preview deletion
    redisctl cloud subscription delete-aa-regions 123 \\
      --region us-west-2 --dry-run

    # Using JSON
    redisctl cloud subscription delete-aa-regions 123 \\
      --data '{\"regions\": [{\"region\": \"us-west-2\"}]}'
")]
    DeleteAaRegions {
        /// Subscription ID
        id: u32,

        /// Region to delete (can be specified multiple times)
        #[arg(long = "region", value_name = "REGION")]
        regions: Vec<String>,

        /// Dry run - create deletion plan without deleting
        #[arg(long)]
        dry_run: bool,

        /// Advanced: Full configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum CloudDatabaseCommands {
    /// List all databases across subscriptions
    List {
        /// Filter by subscription ID
        #[arg(long)]
        subscription: Option<u32>,
    },

    /// Get detailed database information
    Get {
        /// Database ID (format: subscription_id:database_id for fixed, or just database_id for flexible)
        id: String,
    },

    /// Create a new database
    #[command(after_help = "EXAMPLES:
    # Simple database - just name and size
    redisctl cloud database create --subscription 123 --name mydb --memory 1

    # Production database with high availability
    redisctl cloud database create \\
      --subscription 123 \\
      --name prod-cache \\
      --memory 10 \\
      --replication \\
      --data-persistence aof-every-1-second

    # Advanced: Mix flags with JSON for rare options
    redisctl cloud database create \\
      --subscription 123 \\
      --name mydb \\
      --memory 5 \\
      --data '{\"modules\": [{\"name\": \"RedisJSON\"}]}'
")]
    Create {
        /// Subscription ID
        #[arg(long)]
        subscription: u32,

        /// Database name (required unless using --data)
        /// Limited to 40 characters: letters, digits, hyphens
        /// Must start with letter, end with letter or digit
        #[arg(long)]
        name: Option<String>,

        /// Memory limit in GB (e.g., 1, 5, 10, 50)
        /// Alternative to --dataset-size
        #[arg(long, conflicts_with = "dataset_size")]
        memory: Option<f64>,

        /// Dataset size in GB (alternative to --memory)
        /// If replication enabled, total memory will be 2x this value
        #[arg(long, conflicts_with = "memory")]
        dataset_size: Option<f64>,

        /// Database protocol
        #[arg(long, value_parser = ["redis", "memcached"], default_value = "redis")]
        protocol: String,

        /// Enable replication for high availability
        #[arg(long)]
        replication: bool,

        /// Data persistence policy
        /// Options: none, aof-every-1-second, aof-every-write, snapshot-every-1-hour,
        ///          snapshot-every-6-hours, snapshot-every-12-hours
        #[arg(long)]
        data_persistence: Option<String>,

        /// Data eviction policy when memory limit reached
        /// Options: volatile-lru, volatile-ttl, volatile-random, allkeys-lru,
        ///          allkeys-lfu, allkeys-random, noeviction, volatile-lfu
        #[arg(long, default_value = "volatile-lru")]
        eviction_policy: String,

        /// Redis version (e.g., "7.2", "7.0", "6.2")
        #[arg(long)]
        redis_version: Option<String>,

        /// Enable OSS Cluster API support
        #[arg(long)]
        oss_cluster: bool,

        /// TCP port (10000-19999, auto-assigned if not specified)
        #[arg(long)]
        port: Option<i32>,

        /// Advanced: Full database configuration as JSON string or @file.json
        /// CLI flags take precedence over values in JSON
        #[arg(long)]
        data: Option<String>,

        /// Dry run - validate without creating the database
        #[arg(long)]
        dry_run: bool,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Update database configuration
    #[command(after_help = "EXAMPLES:
    # Update database name
    redisctl cloud database update 123:456 --name new-db-name

    # Increase memory
    redisctl cloud database update 123:456 --memory 10

    # Change eviction policy
    redisctl cloud database update 123:456 --eviction-policy allkeys-lru

    # Enable replication
    redisctl cloud database update 123:456 --replication

    # Multiple changes at once
    redisctl cloud database update 123:456 \\
      --memory 20 \\
      --data-persistence aof-every-1-second \\
      --wait

    # Advanced: Use JSON for complex updates
    redisctl cloud database update 123:456 \\
      --data '{\"alerts\": [{\"name\": \"dataset-size\", \"value\": 80}]}'
")]
    Update {
        /// Database ID (format: subscription_id:database_id)
        id: String,

        /// New database name
        #[arg(long)]
        name: Option<String>,

        /// Memory limit in GB
        #[arg(long)]
        memory: Option<f64>,

        /// Enable replication for high availability
        #[arg(long)]
        replication: Option<bool>,

        /// Data persistence policy
        /// Options: none, aof-every-1-second, aof-every-write, snapshot-every-1-hour,
        ///          snapshot-every-6-hours, snapshot-every-12-hours
        #[arg(long)]
        data_persistence: Option<String>,

        /// Data eviction policy when memory limit reached
        /// Options: volatile-lru, volatile-ttl, volatile-random, allkeys-lru,
        ///          allkeys-lfu, allkeys-random, noeviction, volatile-lfu
        #[arg(long)]
        eviction_policy: Option<String>,

        /// Enable OSS Cluster API support
        #[arg(long)]
        oss_cluster: Option<bool>,

        /// Regular expression for allowed keys
        #[arg(long)]
        regex_rules: Option<String>,

        /// Advanced: Full update configuration as JSON string or @file.json
        /// CLI flags take precedence over values in JSON
        #[arg(long)]
        data: Option<String>,

        /// Dry run - validate without applying changes
        #[arg(long)]
        dry_run: bool,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Delete a database
    Delete {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Dry run - show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Get database backup status
    BackupStatus {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },

    /// Trigger manual database backup
    Backup {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Get database import status
    ImportStatus {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },

    /// Import data into database
    #[command(after_help = "EXAMPLES:
    # Import from S3
    redisctl cloud database import 123:456 \\
      --source-type s3 \\
      --import-from-uri s3://bucket/backup.rdb \\
      --wait

    # Import from FTP
    redisctl cloud database import 123:456 \\
      --source-type ftp \\
      --import-from-uri ftp://user:pass@server/backup.rdb

    # Import from HTTP
    redisctl cloud database import 123:456 \\
      --source-type http \\
      --import-from-uri https://example.com/backup.rdb

    # Import from AWS S3 with credentials
    redisctl cloud database import 123:456 \\
      --source-type aws-s3 \\
      --import-from-uri s3://bucket/backup.rdb \\
      --aws-access-key AKIA... \\
      --aws-secret-key secret

    # Import from Google Cloud Storage
    redisctl cloud database import 123:456 \\
      --source-type gcs \\
      --import-from-uri gs://bucket/backup.rdb

    # Advanced: Use JSON for complex configurations
    redisctl cloud database import 123:456 \\
      --data @import-config.json
")]
    Import {
        /// Database ID (format: subscription_id:database_id)
        id: String,

        /// Source type: http, redis, ftp, aws-s3, gcs, azure-blob-storage
        #[arg(long)]
        source_type: Option<String>,

        /// URI to import from (S3 URL, HTTP URL, FTP URL, etc.)
        #[arg(long)]
        import_from_uri: Option<String>,

        /// AWS access key ID (for aws-s3 source type)
        #[arg(long)]
        aws_access_key: Option<String>,

        /// AWS secret access key (for aws-s3 source type)
        #[arg(long)]
        aws_secret_key: Option<String>,

        /// GCS client email (for gcs source type)
        #[arg(long)]
        gcs_client_email: Option<String>,

        /// GCS private key (for gcs source type)
        #[arg(long)]
        gcs_private_key: Option<String>,

        /// Azure storage account name (for azure-blob-storage source type)
        #[arg(long)]
        azure_account_name: Option<String>,

        /// Azure storage account key (for azure-blob-storage source type)
        #[arg(long)]
        azure_account_key: Option<String>,

        /// Advanced: Full import configuration as JSON string or @file.json
        /// CLI flags take precedence over values in JSON
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Get database certificate
    GetCertificate {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },

    /// Get slow query log
    SlowLog {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Maximum number of entries to return
        #[arg(long, default_value = "100")]
        limit: u32,
        /// Offset for pagination
        #[arg(long, default_value = "0")]
        offset: u32,
    },

    /// List database tags
    ListTags {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },

    /// Add a tag to database
    AddTag {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Tag key
        #[arg(long)]
        key: String,
        /// Tag value
        #[arg(long)]
        value: String,
    },

    /// Update database tags
    #[command(after_help = "EXAMPLES:
    # Set multiple tags at once
    redisctl cloud database update-tags 123:456 \\
      --tag env=production \\
      --tag team=backend \\
      --tag cost-center=12345

    # Replace all tags using JSON
    redisctl cloud database update-tags 123:456 \\
      --data '{\"tags\": [{\"key\": \"env\", \"value\": \"prod\"}]}'
")]
    UpdateTags {
        /// Database ID (format: subscription_id:database_id)
        id: String,

        /// Tag in key=value format (repeatable)
        #[arg(long = "tag", value_name = "KEY=VALUE")]
        tags: Vec<String>,

        /// Tags as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Update a single tag value
    UpdateTag {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Tag key
        #[arg(long)]
        key: String,
        /// Tag value
        #[arg(long)]
        value: String,
    },

    /// Delete a tag from database
    DeleteTag {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Tag key to delete
        #[arg(long)]
        key: String,
    },

    /// Flush database (deletes all data)
    Flush {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Flush Active-Active database
    FlushCrdb {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Update Active-Active database regions
    #[command(
        name = "update-aa-regions",
        after_help = "EXAMPLES:
    # Update database name
    redisctl cloud database update-aa-regions 123:456 --name new-db-name --wait

    # Update memory limit
    redisctl cloud database update-aa-regions 123:456 --memory 10 --wait

    # Update global password for all regions
    redisctl cloud database update-aa-regions 123:456 --global-password newsecret

    # Update data persistence
    redisctl cloud database update-aa-regions 123:456 \\
      --global-data-persistence aof-every-1-second

    # Update eviction policy
    redisctl cloud database update-aa-regions 123:456 \\
      --eviction-policy volatile-lru

    # Enable TLS
    redisctl cloud database update-aa-regions 123:456 --enable-tls true

    # Dry run to validate changes
    redisctl cloud database update-aa-regions 123:456 --name test --dry-run

    # Use JSON for complex per-region settings
    redisctl cloud database update-aa-regions 123:456 \\
      --data @regions.json
"
    )]
    UpdateAaRegions {
        /// Database ID (format: subscription_id:database_id)
        id: String,

        /// New database name
        #[arg(long)]
        name: Option<String>,

        /// Memory limit in GB (total including replication)
        #[arg(long)]
        memory: Option<f64>,

        /// Dataset size in GB (alternative to --memory)
        #[arg(long)]
        dataset_size: Option<f64>,

        /// Data persistence for all regions (disabled, aof-every-1-second, aof-every-write, snapshot-every-1-hour, snapshot-every-6-hours, snapshot-every-12-hours)
        #[arg(long)]
        global_data_persistence: Option<String>,

        /// Password for all regions
        #[arg(long)]
        global_password: Option<String>,

        /// Data eviction policy
        #[arg(long)]
        eviction_policy: Option<String>,

        /// Enable/disable TLS
        #[arg(long)]
        enable_tls: Option<bool>,

        /// Enable OSS Cluster API
        #[arg(long)]
        oss_cluster: Option<bool>,

        /// Dry run - validate without applying changes
        #[arg(long)]
        dry_run: bool,

        /// JSON data for complex per-region settings (string or @filename)
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Get available Redis versions for upgrade
    AvailableVersions {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },

    /// Get Redis version upgrade status
    UpgradeStatus {
        /// Database ID (format: subscription_id:database_id)
        id: String,
    },

    /// Upgrade Redis version
    UpgradeRedis {
        /// Database ID (format: subscription_id:database_id)
        id: String,
        /// Target Redis version
        #[arg(long)]
        version: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum CloudUserCommands {
    /// List all users
    List,

    /// Get detailed user information
    Get {
        /// User ID
        id: u32,
    },

    /// Update user information
    Update {
        /// User ID
        id: u32,
        /// New name for the user
        #[arg(long)]
        name: Option<String>,
        /// New role for the user (owner, manager, viewer, billing_admin)
        #[arg(long)]
        role: Option<String>,
        /// Enable/disable email alerts
        #[arg(long)]
        alerts_email: Option<bool>,
        /// Enable/disable SMS alerts
        #[arg(long)]
        alerts_sms: Option<bool>,
    },

    /// Delete a user
    Delete {
        /// User ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Async operation arguments
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum CloudAclCommands {
    // Redis ACL Rules
    /// List all Redis ACL rules
    #[command(name = "list-redis-rules")]
    ListRedisRules,

    /// Create a new Redis ACL rule
    #[command(name = "create-redis-rule")]
    CreateRedisRule {
        /// Rule name
        #[arg(long)]
        name: String,
        /// Redis ACL rule (e.g., "+@read")
        #[arg(long)]
        rule: String,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Update an existing Redis ACL rule
    #[command(name = "update-redis-rule")]
    UpdateRedisRule {
        /// Rule ID
        id: i32,
        /// New rule name
        #[arg(long)]
        name: Option<String>,
        /// New Redis ACL rule
        #[arg(long)]
        rule: Option<String>,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Delete a Redis ACL rule
    #[command(name = "delete-redis-rule")]
    DeleteRedisRule {
        /// Rule ID
        id: i32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    // ACL Roles
    /// List all ACL roles
    #[command(name = "list-roles")]
    ListRoles,

    /// Create a new ACL role
    #[command(name = "create-role")]
    CreateRole {
        /// Role name
        #[arg(long)]
        name: String,
        /// Redis rules (JSON array or single rule ID)
        #[arg(long, value_name = "JSON|ID")]
        redis_rules: String,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Update an existing ACL role
    #[command(name = "update-role")]
    UpdateRole {
        /// Role ID
        id: i32,
        /// New role name
        #[arg(long)]
        name: Option<String>,
        /// New Redis rules (JSON array or single rule ID)
        #[arg(long, value_name = "JSON|ID")]
        redis_rules: Option<String>,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Delete an ACL role
    #[command(name = "delete-role")]
    DeleteRole {
        /// Role ID
        id: i32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    // ACL Users
    /// List all ACL users
    #[command(name = "list-acl-users")]
    ListAclUsers,

    /// Get ACL user details
    #[command(name = "get-acl-user")]
    GetAclUser {
        /// ACL user ID
        id: i32,
    },

    /// Create a new ACL user
    #[command(name = "create-acl-user")]
    CreateAclUser {
        /// Username
        #[arg(long)]
        name: String,
        /// Role name
        #[arg(long)]
        role: String,
        /// Password
        #[arg(long)]
        password: String,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Update an ACL user
    #[command(name = "update-acl-user")]
    UpdateAclUser {
        /// ACL user ID
        id: i32,
        /// New username
        #[arg(long)]
        name: Option<String>,
        /// New role name
        #[arg(long)]
        role: Option<String>,
        /// New password
        #[arg(long)]
        password: Option<String>,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Delete an ACL user
    #[command(name = "delete-acl-user")]
    DeleteAclUser {
        /// ACL user ID
        id: i32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}
