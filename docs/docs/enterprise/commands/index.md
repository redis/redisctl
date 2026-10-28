# Enterprise Commands

All Redis Enterprise CLI commands.

## Command Reference

### Core

| Command Group | Description |
|---------------|-------------|
| [database](databases.md) | Database management |
| [cluster](cluster.md) | Cluster configuration and operations |
| [node](nodes.md) | Node operations |
| shard | Shard management operations |
| endpoint | Endpoint operations |

### Access

| Command Group | Description |
|---------------|-------------|
| [user](access-control.md) | User management |
| [role](access-control.md) | Role management |
| [acl](access-control.md) | ACL operations |
| ldap | LDAP integration |
| ldap-mappings | LDAP mappings management |
| auth | Authentication and sessions |

### Monitoring

| Command Group | Description |
|---------------|-------------|
| [alerts](monitoring.md) | Alert management |
| stats | Statistics and metrics |
| status | Comprehensive cluster status (cluster, nodes, databases, shards) |
| [logs](monitoring.md) | Log operations |
| [diagnostics](monitoring.md) | Diagnostics operations |
| debug-info | Debug info collection |

### Admin

| Command Group | Description |
|---------------|-------------|
| license | License management |
| module | Module management |
| proxy | Proxy management |
| services | Service management |
| cm-settings | Cluster manager settings |
| suffix | DNS suffix management |

### Advanced

| Command Group | Description |
|---------------|-------------|
| [crdb](active-active.md) | Active-Active database (CRDB) operations |
| crdb-task | CRDB task operations |
| bdb-group | Database group operations |
| migration | Migration operations |
| bootstrap | Bootstrap and initialization operations |
| job-scheduler | Job scheduler operations |

### Troubleshooting

| Command Group | Description |
|---------------|-------------|
| support-package | Support package generation for troubleshooting |
| ocsp | OCSP certificate validation |
| usage-report | Usage report operations |
| local | Local node operations |

### Other

| Command Group | Description |
|---------------|-------------|
| action | Action (task) operations |
| workflow | Workflow operations for multi-step tasks |
| jsonschema | JSON schema operations |

## Module archives

`enterprise module package` writes a ZIP containing `module.json` and the module
binary's basename at the archive root, with Deflate compression. Input contents
are preserved; compressed archive bytes and size can change between dependency
versions. `enterprise module inspect` reads the archive without extracting or
loading its entries. Its structural `valid` result is not a security assessment
or proof that a module can be loaded by Redis Enterprise.

The ZIP 9 reader prefers UTF-8 when filename bytes form valid UTF-8, otherwise
falling back to CP437. An older archive with ambiguous non-ASCII bytes and no
UTF-8 flag can therefore display a different filename than with ZIP 6. Correctly
encoded UTF-8 and ordinary CP437 names remain supported. Some invalid
Unicode-path extra fields previously rejected by ZIP 6 are ignored by ZIP 9.
For stable automation, use ASCII entry names or explicitly encoded UTF-8 names;
do not depend on malformed or ambiguous name metadata being interpreted the
same way across reader versions. This does not enable archive extraction.

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
