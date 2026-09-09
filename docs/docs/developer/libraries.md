# Libraries

Use the Redis API client libraries in your own Rust or Python projects.

The documented public Rust APIs of `redisctl-core` and `redisctl-mcp` become supported SemVer
surfaces at the redisctl 1.0 baseline. See the
[Compatibility and Support Policy](../reference/compatibility.md) for the boundary between stable
library APIs and implementation details.

## Available Crates

| Crate | Description | docs.rs | Repository |
|-------|-------------|---------|------------|
| `redis-cloud` | Redis Cloud API client | [docs](https://docs.rs/redis-cloud) | [redis-cloud-rs](https://github.com/redis-developer/redis-cloud-rs) |
| `redis-enterprise` | Redis Enterprise API client | [docs](https://docs.rs/redis-enterprise) | [redis-enterprise-rs](https://github.com/redis-developer/redis-enterprise-rs) |
| `redisctl-core` | Profile and credential management | [docs](https://docs.rs/redisctl-core) | [redisctl](https://github.com/redis/redisctl) |
| `redisctl-mcp` | Embeddable MCP server and tool routers | [docs](https://docs.rs/redisctl-mcp) | [redisctl](https://github.com/redis/redisctl) |

**Note:** `redis-cloud` and `redis-enterprise` are maintained in separate repositories and also provide Python bindings via PyPI.

## redis-cloud

Redis Cloud API client with full type coverage. Maintained at [github.com/redis-developer/redis-cloud-rs](https://github.com/redis-developer/redis-cloud-rs).

### Installation

**Rust:**
```toml
[dependencies]
redis-cloud = "0.10"
tokio = { version = "1", features = ["full"] }
```

**Python:**
```bash
pip install redis-cloud
```

### Example

```rust
use redis_cloud::RedisCloudClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = RedisCloudClient::new(
        std::env::var("REDIS_CLOUD_API_KEY")?,
        std::env::var("REDIS_CLOUD_SECRET_KEY")?,
    );

    // List subscriptions
    let subscriptions = client.subscriptions().list().await?;
    for sub in subscriptions {
        println!("{}: {}", sub.id, sub.name);
    }

    // Get databases
    let databases = client.databases().list(subscription_id).await?;

    Ok(())
}
```

## redis-enterprise

Redis Enterprise REST API client. Maintained at [github.com/redis-developer/redis-enterprise-rs](https://github.com/redis-developer/redis-enterprise-rs).

### Installation

**Rust:**
```toml
[dependencies]
redis-enterprise = "0.9"
tokio = { version = "1", features = ["full"] }
```

**Python:**
```bash
pip install redis-enterprise
```

### Example

```rust
use redis_enterprise::RedisEnterpriseClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = RedisEnterpriseClient::builder()
        .url("https://cluster.example.com:9443")
        .username("admin@cluster.local")
        .password("password")
        .insecure(true)  // For self-signed certs
        .build()?;

    // Get cluster info
    let cluster = client.cluster().get().await?;
    println!("Cluster: {} ({})", cluster.name, cluster.version);

    // List databases
    let databases = client.databases().list().await?;
    for db in databases {
        println!("  {}: {}", db.uid, db.name);
    }

    Ok(())
}
```

## redisctl-core

Core library for config, workflows, and shared logic.

### Installation

```toml
[dependencies]
redisctl-core = "0.11"
```

### Example

```rust
use redisctl_core::{Config, Profile};

fn main() -> anyhow::Result<()> {
    // Load config
    let config = Config::load()?;

    // Get default profile
    let profile = config.default_profile()?;

    // Access credentials
    if let Some(key) = profile.cloud_api_key() {
        println!("Cloud API key: {}...", &key[..8]);
    }

    Ok(())
}
```

## Use Cases

### Custom Backup Tool

```rust
use redis_enterprise::RedisEnterpriseClient;
use std::fs::File;

async fn backup_config(client: &RedisEnterpriseClient) -> anyhow::Result<()> {
    let cluster = client.cluster().get().await?;
    let databases = client.databases().list().await?;

    let backup = serde_json::json!({
        "cluster": cluster,
        "databases": databases,
        "timestamp": chrono::Utc::now(),
    });

    let file = File::create("backup.json")?;
    serde_json::to_writer_pretty(file, &backup)?;

    Ok(())
}
```

### Monitoring Integration

```rust
use redis_enterprise::RedisEnterpriseClient;

async fn collect_metrics(client: &RedisEnterpriseClient) -> anyhow::Result<()> {
    let nodes = client.nodes().list().await?;

    for node in nodes {
        prometheus::gauge!("redis_node_shards", node.shard_count as f64,
            "node" => node.uid.to_string());
    }

    Ok(())
}
```

## API Coverage

### redis-cloud

- Subscriptions (CRUD)
- Databases (CRUD)
- Tasks (list, get, wait)
- VPC Peering
- Users and ACLs
- And more...

### redis-enterprise

- Cluster (get, update, stats)
- Databases (CRUD, stats)
- Nodes (list, get, stats)
- Users and Roles
- LDAP configuration
- Support packages
- And more...

## Links

**Rust:**
- [docs.rs/redis-cloud](https://docs.rs/redis-cloud)
- [docs.rs/redis-enterprise](https://docs.rs/redis-enterprise)
- [crates.io/crates/redisctl](https://crates.io/crates/redisctl)

**Python:**
- [pypi.org/project/redis-cloud](https://pypi.org/project/redis-cloud/)
- [pypi.org/project/redis-enterprise](https://pypi.org/project/redis-enterprise/)

**GitHub:**
- [redis/redisctl](https://github.com/redis/redisctl)
- [redis-developer/redis-cloud-rs](https://github.com/redis-developer/redis-cloud-rs)
- [redis-developer/redis-enterprise-rs](https://github.com/redis-developer/redis-enterprise-rs)
