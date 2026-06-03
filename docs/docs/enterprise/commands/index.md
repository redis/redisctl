# Enterprise Commands

All Redis Enterprise CLI commands.

## Command Reference

### Core

| Command Group | Description |
|---------------|-------------|
| [database](databases.md) | Database management |
| [cluster](cluster.md) | Cluster configuration and operations |
| [node](nodes.md) | Node operations |
| [shard](databases.md) | Shard management operations |
| [endpoint](databases.md) | Endpoint operations |

### Access

| Command Group | Description |
|---------------|-------------|
| [user](access-control.md) | User management |
| [role](access-control.md) | Role management |
| [acl](access-control.md) | ACL operations |
| [ldap](access-control.md) | LDAP integration |
| [ldap-mappings](access-control.md) | LDAP mappings management |
| [auth](access-control.md) | Authentication and sessions |

### Monitoring

| Command Group | Description |
|---------------|-------------|
| [stats](monitoring.md) | Statistics and metrics |
| [status](monitoring.md) | Comprehensive cluster status (cluster, nodes, databases, shards) |
| [alerts](monitoring.md) | Alert management |
| [logs](monitoring.md) | Log operations |
| [diagnostics](monitoring.md) | Diagnostics operations |
| [debug-info](monitoring.md) | Debug info collection |

### Admin

| Command Group | Description |
|---------------|-------------|
| [license](cluster.md) | License management |
| [module](databases.md) | Module management |
| [proxy](cluster.md) | Proxy management |
| [services](cluster.md) | Service management |
| [cm-settings](cluster.md) | Cluster manager settings |
| [suffix](cluster.md) | DNS suffix management |

### Advanced

| Command Group | Description |
|---------------|-------------|
| [crdb](active-active.md) | Active-Active database (CRDB) operations |
| [crdb-task](active-active.md) | CRDB task operations |
| [bdb-group](databases.md) | Database group operations |
| [migration](databases.md) | Migration operations |
| [bootstrap](cluster.md) | Bootstrap and initialization operations |
| [job-scheduler](cluster.md) | Job scheduler operations |

### Troubleshooting

| Command Group | Description |
|---------------|-------------|
| [support-package](monitoring.md) | Support package generation for troubleshooting |
| [ocsp](cluster.md) | OCSP certificate validation |
| [usage-report](monitoring.md) | Usage report operations |
| [local](cluster.md) | Local node operations |

### Other

| Command Group | Description |
|---------------|-------------|
| [action](tasks.md) | Action (task) operations |
| [workflow](tasks.md) | Workflow operations for multi-step tasks |
| [jsonschema](cluster.md) | JSON schema operations |

## Getting Help

```bash
# List all enterprise commands
redisctl enterprise --help

# Help for specific command
redisctl enterprise cluster --help
redisctl enterprise database create --help
```

## Common Options

All commands support:

| Option | Description |
|--------|-------------|
| `-o, --output` | Output format: `table`, `json`, `yaml` |
| `-q, --query` | JMESPath query to filter output |
| `--profile` | Use specific profile |

## Examples

```bash
# Get cluster info
redisctl enterprise cluster get

# List databases as JSON
redisctl enterprise database list -o json

# Filter with JMESPath
redisctl enterprise node list -o json -q '[].{id: uid, status: status}'
```
