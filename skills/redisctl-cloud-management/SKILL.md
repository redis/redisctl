---
name: redisctl-cloud-management
description: Manage Redis Cloud subscriptions, databases, and resources via the redisctl CLI. Use when provisioning, updating, or monitoring Redis Cloud infrastructure.
---

## Overview

Manage the full lifecycle of Redis Cloud resources: subscriptions, databases, users, ACLs, and tasks.

## Subscriptions

```bash
# List all subscriptions
redisctl cloud subscription list

# Get subscription details
redisctl cloud subscription get 12345

# Create a subscription (JSON input)
redisctl cloud subscription create --data '{...}'

# Update a subscription
redisctl cloud subscription update 12345 --data '{...}'

# Delete a subscription
redisctl cloud subscription delete 12345
```

## Databases

```bash
# List databases (optionally filtered by subscription)
redisctl cloud database list
redisctl cloud database list --subscription 12345

# Get database details (format: subscription_id:database_id)
redisctl cloud database get 12345:67890

# Create a database
redisctl cloud database create --subscription 12345 --name mydb --memory 1

# Update a database
redisctl cloud database update 12345:67890 --memory 10

# Delete a database
redisctl cloud database delete 12345:67890

# Backup a database
redisctl cloud database backup 12345:67890
```

## Essentials / Fixed Tier

For smaller, fixed-price databases:

```bash
redisctl cloud fixed-subscription list
redisctl cloud fixed-database list 12345
redisctl cloud fixed-database create 12345 --name mydb
```

## Task Tracking

Cloud operations are async. Track tasks:

```bash
# List recent tasks
redisctl cloud task list

# Get task status
redisctl cloud task get <task-id>

# Wait for a task to complete
redisctl cloud task wait <task-id>
```

## Workflows

Multi-step operations:

```bash
# Complete subscription setup with optional database
redisctl cloud workflow subscription-setup
```

## Cost Reports

```bash
# Generate a cost report (FOCUS format)
redisctl cloud cost-report generate --start 2026-01-01 --end 2026-01-31

# Download a generated report
redisctl cloud cost-report download <report-id>

# Generate and download in one step
redisctl cloud cost-report export --start 2026-01-01 --end 2026-01-31
```

## Account Management

```bash
# Get account details
redisctl cloud account get

# List/manage users (no create — users are managed via the Redis Cloud console)
redisctl cloud user list
redisctl cloud user get <user-id>
redisctl cloud user update <user-id> --data '{...}'
redisctl cloud user delete <user-id>

# ACL management — granular subcommands
redisctl cloud acl list-redis-rules
redisctl cloud acl create-redis-rule --data '{...}'
redisctl cloud acl list-roles
redisctl cloud acl create-role --data '{...}'
redisctl cloud acl list-acl-users
redisctl cloud acl create-acl-user --data '{...}'
```

## Tips

- Use `--profile <name>` to target a specific Cloud profile
- Use `--output json` for machine-readable output
- Most create/update commands accept `--data` with a JSON payload
- Task IDs are returned from async operations — use `cloud task wait` to block until completion
- Database IDs use the format `subscription_id:database_id` for most operations
