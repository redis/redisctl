---
name: redisctl-enterprise-ops
description: Day-to-day Redis Enterprise cluster operations via the redisctl CLI. Use when checking cluster status, managing databases, viewing stats and logs, or monitoring Active-Active deployments.
---

## Overview

Manage Redis Enterprise clusters, databases, and operational monitoring through the CLI.

## Cluster Status

```bash
# Comprehensive status overview
redisctl enterprise status

# With specific sections
redisctl enterprise status --cluster --nodes --databases --shards

# Brief summary
redisctl enterprise status --brief
```

## Database Management

```bash
# List all databases
redisctl enterprise database list

# Get database details
redisctl enterprise database get --id 1

# Create a database
redisctl enterprise database create --data '{...}'

# Update a database
redisctl enterprise database update --id 1 --data '{...}'

# Delete a database
redisctl enterprise database delete --id 1
```

## Active-Active (CRDB)

```bash
# List CRDBs
redisctl enterprise crdb list

# Get CRDB details
redisctl enterprise crdb get --id <guid>

# Create a CRDB
redisctl enterprise crdb create --data '{...}'

# Update a CRDB
redisctl enterprise crdb update --id <guid> --data '{...}'

# Check CRDB task status
redisctl enterprise crdb-task list
redisctl enterprise crdb-task get --id <task-id>
```

## Stats and Metrics

```bash
# Cluster-level stats
redisctl enterprise stats cluster

# Per-node stats
redisctl enterprise stats node

# Per-database stats
redisctl enterprise stats database

# Per-shard stats
redisctl enterprise stats shard
```

## Logs and Alerts

```bash
# View cluster logs
redisctl enterprise logs cluster

# View node-specific logs
redisctl enterprise logs node

# List alerts
redisctl enterprise alerts list

# Get alert details
redisctl enterprise alerts get --id <alert-id>
```

## Module Management

```bash
# List installed modules
redisctl enterprise module list

# Get module details
redisctl enterprise module get --id <module-id>
```

## Nodes and Shards

```bash
# List nodes
redisctl enterprise node list

# Get node details
redisctl enterprise node get --id 1

# List shards
redisctl enterprise shard list

# Get shard details
redisctl enterprise shard get --id 1
```

## Tips

- Use `--profile <name>` to target a specific Enterprise cluster
- Manage multiple clusters by creating separate profiles and switching with `--profile`
- Use `redisctl enterprise status --brief` for a quick health check
- Database IDs are numeric (BDB IDs), CRDB IDs are GUIDs
