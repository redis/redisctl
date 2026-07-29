# Networking Commands

VPC peering and private connectivity for Redis Cloud.

## VPC Peering

### List Peerings

```bash
redisctl cloud connectivity vpc-peering get --subscription 123456
```

### Create Peering

#### AWS VPC Peering

```bash
# Using first-class parameters (recommended)
redisctl cloud connectivity vpc-peering create --subscription 123456 \
  --region us-east-1 \
  --aws-account-id 123456789012 \
  --vpc-id vpc-abc123 \
  --wait

# With multiple CIDR blocks
redisctl cloud connectivity vpc-peering create --subscription 123456 \
  --region us-east-1 \
  --aws-account-id 123456789012 \
  --vpc-id vpc-abc123 \
  --vpc-cidr 10.0.0.0/16 \
  --vpc-cidr 10.1.0.0/16 \
  --wait
```

#### GCP VPC Peering

```bash
redisctl cloud connectivity vpc-peering create --subscription 123456 \
  --gcp-project-id my-gcp-project \
  --gcp-network-name my-network \
  --wait
```

#### Using JSON (escape hatch for advanced options)

```bash
redisctl cloud connectivity vpc-peering create --subscription 123456 \
  --data '{
    "region": "us-east-1",
    "awsAccountId": "123456789012",
    "vpcId": "vpc-abc123",
    "vpcCidr": "10.0.0.0/16"
  }' --wait
```

### Update Peering

```bash
# Update CIDR blocks
redisctl cloud connectivity vpc-peering update --subscription 123456 --peering-id 789 \
  --vpc-cidr 10.0.0.0/16 \
  --vpc-cidr 10.1.0.0/16 \
  --wait
```

### Delete Peering

```bash
redisctl cloud connectivity vpc-peering delete --subscription 123456 --peering-id 789 --wait
```

## AWS PrivateLink

### Get PrivateLink Status

```bash
redisctl cloud connectivity privatelink get --subscription 123456
```

### Create PrivateLink Service

```bash
# Using first-class parameters
redisctl cloud connectivity privatelink create --subscription 123456 \
  --share-name my-privatelink-share \
  --wait

# Using JSON
redisctl cloud connectivity privatelink create --subscription 123456 \
  --data '{"shareName": "my-privatelink-share"}' \
  --wait
```

### Add Principal (Allow AWS Account)

```bash
# Using first-class parameters
redisctl cloud connectivity privatelink add-principal --subscription 123456 \
  --principal 123456789012 \
  --type aws-account \
  --wait

# Using JSON
redisctl cloud connectivity privatelink add-principal --subscription 123456 \
  --data '{"principal": "123456789012", "principalType": "aws_account"}' \
  --wait
```

### Remove Principal

```bash
redisctl cloud connectivity privatelink remove-principal --subscription 123456 \
  --principal 123456789012 \
  --type aws-account \
  --wait
```

### Delete PrivateLink

```bash
redisctl cloud connectivity privatelink delete --subscription 123456 --force --wait
```

## Private Service Connect (GCP)

### Get PSC Service

```bash
redisctl cloud connectivity psc service-get 123456
```

### Create PSC Service

```bash
redisctl cloud connectivity psc service-create 123456 --wait
```

### List PSC Endpoints

```bash
redisctl cloud connectivity psc endpoints-list 123456
```

### Create PSC Endpoint

```bash
# Using first-class parameters (recommended)
redisctl cloud connectivity psc endpoint-create 123456 \
  --gcp-project-id my-gcp-project \
  --gcp-vpc-name my-vpc \
  --gcp-vpc-subnet-name my-subnet \
  --endpoint-connection-name redis-psc \
  --wait

# Using JSON
redisctl cloud connectivity psc endpoint-create 123456 \
  --data '{
    "gcpProjectId": "my-gcp-project",
    "gcpVpcName": "my-vpc",
    "gcpVpcSubnetName": "my-subnet"
  }' --wait
```

### Update PSC Endpoint

```bash
redisctl cloud connectivity psc endpoint-update 123456 \
  --endpoint-id 789 \
  --psc-service-id 456 \
  --gcp-vpc-subnet-name new-subnet \
  --wait
```

### Delete PSC Endpoint

```bash
redisctl cloud connectivity psc endpoint-delete 123456 --endpoint-id 789 --force --wait
```

## Transit Gateway (AWS)

### List Attachments

```bash
redisctl cloud connectivity tgw attachments-list 123456
```

### Create Attachment

```bash
# Using first-class parameters (recommended)
redisctl cloud connectivity tgw attachment-create 123456 \
  --aws-account-id 123456789012 \
  --tgw-id tgw-abc123 \
  --wait

# With CIDR blocks
redisctl cloud connectivity tgw attachment-create 123456 \
  --aws-account-id 123456789012 \
  --tgw-id tgw-abc123 \
  --cidr 10.0.0.0/16 \
  --cidr 10.1.0.0/16 \
  --wait

# Using JSON file
redisctl cloud connectivity tgw attachment-create 123456 \
  --data @tgw-config.json \
  --wait
```

### Update Attachment CIDRs

```bash
redisctl cloud connectivity tgw attachment-update 123456 \
  --attachment-id att-abc123 \
  --cidr 10.0.0.0/16 \
  --cidr 10.2.0.0/16 \
  --wait
```

### Delete Attachment

```bash
redisctl cloud connectivity tgw attachment-delete 123456 att-abc123 --force --wait
```

### List Invitations

```bash
redisctl cloud connectivity tgw invitations-list 123456
```

### Accept/Reject Invitation

```bash
redisctl cloud connectivity tgw invitation-accept 123456 inv-abc123
redisctl cloud connectivity tgw invitation-reject 123456 inv-abc123
```

## Common Patterns

### Check Peering Status

```bash
redisctl cloud connectivity vpc-peering get --subscription 123456 -o json -q '[].{
  id: id,
  status: status,
  vpcId: vpcId
}'
```

### Wait for Peering Active

```bash
redisctl cloud connectivity vpc-peering create \
  --subscription 123456 \
  --region us-east-1 \
  --aws-account-id 123456789012 \
  --vpc-id vpc-abc123 \
  --wait \
  --wait-timeout 600
```

## Raw API Access

```bash
# VPC Peerings
redisctl api cloud get /subscriptions/123456/peerings

# PrivateLink
redisctl api cloud get /subscriptions/123456/privateLink

# PSC
redisctl api cloud get /subscriptions/123456/private-service-connect

# Transit Gateway
redisctl api cloud get /subscriptions/123456/transitGateways
```

## Related

- [Subscriptions](subscriptions.md) - Subscription management
- [VPC Peering Cookbook](../../cookbook/cloud/vpc-peering.md) - Step-by-step guide
