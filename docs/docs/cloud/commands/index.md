# Cloud Commands

All Redis Cloud CLI commands.

## Command Reference

| Command Group | Description |
|---------------|-------------|
| [subscription](subscriptions.md) | Manage Pro subscriptions |
| [database](databases.md) | Manage Pro databases |
| [fixed-subscription](fixed-subscriptions.md) | Manage Essentials subscriptions |
| [fixed-database](fixed-databases.md) | Manage Essentials databases |
| [user](access-control.md) | User management |
| [acl](access-control.md) | Access control lists |
| account | Account operations |
| payment-method | Payment method management |
| [cost-report](cost-report.md) | Cost reporting (Beta) |
| [connectivity](networking.md) | Network connectivity operations (VPC, PSC, TGW) |
| provider-account | Cloud provider account operations |
| [task](tasks.md) | Async task monitoring |
| workflow | Workflow operations for multi-step tasks |

## Getting Help

```bash
# List all cloud commands
redisctl cloud --help

# Help for specific command
redisctl cloud subscription --help
redisctl cloud database create --help
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
# List subscriptions as table
redisctl cloud subscription list

# List as JSON with query
redisctl cloud subscription list -o json -q '[].{id: id, name: name}'

# Use specific profile
redisctl --profile prod cloud database list --subscription-id 123456
```
