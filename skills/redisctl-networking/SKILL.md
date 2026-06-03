---
name: redisctl-networking
description: Configure network connectivity for Redis Cloud deployments via the redisctl CLI. Use when setting up VPC peering, Transit Gateway, Private Service Connect, or PrivateLink.
---

## Overview

Set up private network connectivity between your infrastructure and Redis Cloud deployments. All networking commands live under `redisctl cloud connectivity`. Supports VPC Peering, Transit Gateway (AWS), Private Service Connect (GCP), and AWS PrivateLink.

## VPC Peering

VPC peering uses `--subscription <ID>` as a named flag. There is no generic `list` subcommand — use `get` to retrieve the current peering configuration, or `list-aa` for Active-Active subscriptions.

```bash
# Get VPC peering details for a subscription
redisctl cloud connectivity vpc-peering get --subscription 12345

# Create an AWS VPC peering connection
redisctl cloud connectivity vpc-peering create --subscription 12345 \
  --region us-east-1 \
  --aws-account-id 123456789012 \
  --vpc-id vpc-abc123 \
  --vpc-cidr 10.0.0.0/16

# Create a GCP VPC peering connection
redisctl cloud connectivity vpc-peering create --subscription 12345 \
  --gcp-project-id my-gcp-project \
  --gcp-network-name my-vpc-network

# Update an existing peering connection
redisctl cloud connectivity vpc-peering update --subscription 12345

# Delete a peering connection
redisctl cloud connectivity vpc-peering delete --subscription 12345 --peering-id 1

# Active-Active: list VPC peerings
redisctl cloud connectivity vpc-peering list-aa --subscription 12345
```

## Transit Gateway (AWS)

TGW commands take the subscription ID as a **positional argument** (not a flag).

```bash
# List TGW attachments
redisctl cloud connectivity tgw attachments-list 12345

# Create a TGW attachment
redisctl cloud connectivity tgw attachment-create 12345 \
  --aws-account-id 123456789012 \
  --tgw-id tgw-abc123 \
  --cidr 10.0.0.0/16

# Update TGW attachment CIDRs
redisctl cloud connectivity tgw attachment-update 12345

# Delete a TGW attachment
redisctl cloud connectivity tgw attachment-delete 12345

# List pending resource share invitations
redisctl cloud connectivity tgw invitations-list 12345

# Accept a TGW resource share invitation
redisctl cloud connectivity tgw invitation-accept 12345 <invitation-id>

# Reject a TGW resource share invitation
redisctl cloud connectivity tgw invitation-reject 12345 <invitation-id>
```

Active-Active TGW commands follow the same positional-arg pattern:

```bash
redisctl cloud connectivity tgw aa-attachments-list 12345
redisctl cloud connectivity tgw aa-attachment-create 12345 --tgw-id tgw-abc123
redisctl cloud connectivity tgw aa-invitations-list 12345
redisctl cloud connectivity tgw aa-invitation-accept 12345 <invitation-id>
```

## Private Service Connect (GCP)

PSC commands take the subscription ID as a **positional argument**. `endpoints-list` and `endpoint-create` also require `--psc-service-id`.

```bash
# Get PSC service details for a subscription
redisctl cloud connectivity psc service-get 12345

# Create a PSC service
redisctl cloud connectivity psc service-create 12345

# Delete a PSC service
redisctl cloud connectivity psc service-delete 12345

# List PSC endpoints
redisctl cloud connectivity psc endpoints-list 12345 --psc-service-id <service-id>

# Create a PSC endpoint
redisctl cloud connectivity psc endpoint-create 12345 \
  --psc-service-id <service-id> \
  --gcp-project-id my-project \
  --gcp-vpc-name my-vpc \
  --gcp-vpc-subnet-name my-subnet \
  --endpoint-connection-name redis-psc

# Get the endpoint creation script
redisctl cloud connectivity psc endpoint-creation-script 12345 <endpoint-id> \
  --psc-service-id <service-id>

# Get the endpoint deletion script
redisctl cloud connectivity psc endpoint-deletion-script 12345 <endpoint-id> \
  --psc-service-id <service-id>

# Delete a PSC endpoint
redisctl cloud connectivity psc endpoint-delete 12345 \
  --psc-service-id <service-id>
```

## AWS PrivateLink

PrivateLink uses `--subscription <ID>` as a named flag. There is no `list` subcommand — use `get` to retrieve the current configuration.

```bash
# Get PrivateLink configuration
redisctl cloud connectivity privatelink get --subscription 12345

# Create a PrivateLink service (with an initial principal)
redisctl cloud connectivity privatelink create --subscription 12345 \
  --share-name my-redis-share \
  --principal 123456789012 \
  --type aws-account

# Add additional AWS principals to an existing PrivateLink
redisctl cloud connectivity privatelink add-principal --subscription 12345 \
  --principal 123456789012 \
  --type aws-account

# Remove a principal from PrivateLink
redisctl cloud connectivity privatelink remove-principal --subscription 12345 \
  --principal 123456789012 \
  --type aws-account

# Get the VPC endpoint creation script
redisctl cloud connectivity privatelink get-script --subscription 12345

# Delete PrivateLink configuration
redisctl cloud connectivity privatelink delete --subscription 12345
```

## Cloud Provider Accounts

Required for some networking operations (e.g., AWS VPC peering, TGW):

```bash
# List provider accounts
redisctl cloud provider-account list

# Get provider account details
redisctl cloud provider-account get

# Create a provider account
redisctl cloud provider-account create --data '{...}'
```

## Tips

- VPC peering requires matching CIDR ranges — ensure no overlap with Redis Cloud VPC
- Transit Gateway and PrivateLink are AWS-only features
- Private Service Connect is GCP-only
- Most connectivity operations are async — use `redisctl cloud task wait` after creation
- Active-Active subscriptions have separate connectivity commands with an `aa-` prefix
- Use `--wait` on create/delete commands to block until the operation completes
- Use `--output json` or `--output table` to control output format
