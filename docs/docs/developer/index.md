# Developer Guide

Build with redisctl and contribute to the project.

## Using the API Libraries

The Redis API client libraries are available for both Rust and Python:

### Rust Crates

| Crate | Description | docs.rs |
|-------|-------------|---------|
| `redis-cloud` | Redis Cloud API client | [docs](https://docs.rs/redis-cloud) |
| `redis-enterprise` | Redis Enterprise API client | [docs](https://docs.rs/redis-enterprise) |
| `redisctl-core` | Profile and credential management | [docs](https://docs.rs/redisctl-core) |

### Python Packages

| Package | Description | PyPI |
|---------|-------------|------|
| `redis-cloud` | Redis Cloud API client | [pypi](https://pypi.org/project/redis-cloud/) |
| `redis-enterprise` | Redis Enterprise API client | [pypi](https://pypi.org/project/redis-enterprise/) |

### Example: Using redis-cloud (Rust)

```rust
use redis_cloud::CloudClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CloudClient::builder()
        .api_key("your-api-key")
        .api_secret("your-secret-key")
        .build()?;

    let subscriptions = client.subscription().list().await?;
    for sub in subscriptions {
        println!("{}: {}", sub.id, sub.name);
    }

    Ok(())
}
```

[:octicons-arrow-right-24: Rust Libraries Guide](libraries.md)

### Example: Using Python

```python
from redis_cloud import CloudClient

client = CloudClient.from_env()

subscriptions = client.subscriptions_sync()
for sub in subscriptions:
    print(f"{sub['id']}: {sub['name']}")
```

[:octicons-arrow-right-24: Python Bindings Guide](python.md)

## Architecture

Understand how redisctl is structured:

- Four-layer design (Profiles, Raw API, Human Commands, Workflows)
- Workspace organization
- Error handling patterns
- Output formatting

[:octicons-arrow-right-24: Architecture](architecture.md)

## Contributing

We welcome contributions:

- Bug reports and feature requests
- Documentation improvements
- Code contributions

[:octicons-arrow-right-24: Contributing Guide](contributing.md)

## Links

- [GitHub Repository](https://github.com/redis/redisctl)
- [Issue Tracker](https://github.com/redis/redisctl/issues)
- [Releases](https://github.com/redis/redisctl/releases)
