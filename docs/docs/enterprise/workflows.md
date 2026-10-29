# Enterprise Workflows

Multi-step operations for Redis Enterprise.

## Init Cluster

Initialize a new cluster:

```bash
redisctl enterprise workflow init-cluster \
  --name production \
  --username admin@example.com \
  --password "$REDIS_ENTERPRISE_INIT_PASSWORD"
```

This:
1. Bootstraps the cluster with the given name and admin credentials
2. Waits until the API reports the bootstrap as completed
3. Creates a default database (`--skip-database` to skip; `--database-name` and
   `--database-memory-gb` adjust it)

To choose the database's Redis version, the workflow asks the cluster which versions its nodes support and requests the highest. Right after bootstrap the REST API can answer `401` for a few seconds while authentication comes up, so discovery retries for about 30 seconds before giving up; a cluster that keeps refusing is reported as an error rather than guessed at. If the cluster answers but advertises no supported version, the workflow prints a note and creates the database with the cluster default. Pass `--database-redis-version MAJOR.MINOR` to pin a version; an explicit value skips discovery and is validated strictly.

## When to Use Workflows

**Use workflows when:**
- Initial cluster setup
- Complex multi-step operations
- Need consistent orchestration

**Use individual commands when:**
- Day-to-day management
- Need fine-grained control
- Troubleshooting

## Manual Multi-Step Operations

For scenarios not covered by workflows:

### Database with Replication

```bash
#!/bin/bash
set -e

DB_NAME="production-cache"

# Create database
redisctl enterprise database create --data "{
  \"name\": \"$DB_NAME\",
  \"memory_size\": 2147483648,
  \"replication\": true,
  \"shards_count\": 2
}"

# Verify creation
redisctl enterprise database list -o json -q "[?name=='$DB_NAME'] | [0]"
```

### Cluster Health Check and Report

```bash
#!/bin/bash

echo "=== Cluster Health Workflow ==="

# Check cluster
CLUSTER=$(redisctl enterprise cluster get -o json)
echo "Cluster: $(echo $CLUSTER | jq -r '.name')"
echo "Status: $(echo $CLUSTER | jq -r '.status')"

# Check nodes
UNHEALTHY=$(redisctl enterprise node list -o json -q '[?status!=`active`] | length(@)')
if [ "$UNHEALTHY" -gt 0 ]; then
  echo "WARNING: $UNHEALTHY unhealthy nodes"
  redisctl enterprise support-package cluster --optimize
else
  echo "All nodes healthy"
fi
```

## Coming Soon

Additional workflows planned:
- Database migration
- Active-Active setup
- Cluster expansion

## Related

- [Cluster Commands](commands/cluster.md)
- [Database Commands](commands/databases.md)
- [Support Package](operations/support-package.md)
