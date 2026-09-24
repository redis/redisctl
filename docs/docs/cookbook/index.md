# Cookbook

Step-by-step guides for common tasks.

## Redis Cloud Recipes

| Recipe | Description |
|--------|-------------|
| [Create Your First Database](cloud/first-database.md) | From zero to connected in 5 minutes |
| [From Sign-In to REDIS_URL](cloud/quick-database.md) | Sign in, provision a free database, connect — the agent path |
| [Set Up VPC Peering](cloud/vpc-peering.md) | Connect your VPC to Redis Cloud |
| [Configure ACLs](cloud/acls.md) | Secure database access with ACL rules |
| [Backup and Restore](cloud/backup-restore.md) | Manage database backups |

## Redis Enterprise Recipes

| Recipe | Description |
|--------|-------------|
| [Create a Database](enterprise/create-database.md) | Provision a new database |
| [Generate Support Package](enterprise/support-package.md) | Collect diagnostics for Redis Support |
| [Cluster Health Monitoring](enterprise/cluster-health.md) | Monitor cluster status and alerts |
| [Node Management](enterprise/node-management.md) | Add, remove, and maintain nodes |

## Quick Reference

### Common Patterns

**List and filter:**
```bash
redisctl cloud subscription list -o json -q '[?status == `active`].name'
```

**Create and wait:**
```bash
redisctl cloud database create --subscription-id 123 --name mydb --wait
```

**Export to file:**
```bash
redisctl enterprise cluster get -o json > cluster-backup.json
```

**Loop over results:**
```bash
for id in $(redisctl enterprise database list -o json -q '[].uid' | jq -r '.[]'); do
  echo "Database $id:"
  redisctl enterprise database get "$id"
done
```
