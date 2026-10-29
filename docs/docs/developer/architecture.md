# Architecture

How redisctl is structured and designed.

## Four-Layer Design

```
┌─────────────────────────────────────────────────────────────────┐
│                    Layer 3: Consumers                           │
│           CLI (redisctl)        MCP (redisctl-mcp)             │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                 Layer 2: redisctl-core                          │
│  - Unified errors (CoreError)                                   │
│  - Progress callbacks (poll_task, poll_action)                  │
│  - Configuration (profiles, credentials)                        │
│  - Workflows (create_and_wait, backup_and_wait, etc.)          │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│               Layer 1: Typed API Clients                        │
│         redis-cloud              redis-enterprise               │
│     (DatabaseHandler,         (databases(), nodes(),            │
│      SubscriptionHandler)      cluster(), etc.)                 │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│               Layer 0: Raw HTTP                                 │
│         CloudClient.get()      EnterpriseClient.get()          │
└─────────────────────────────────────────────────────────────────┘
```

| Layer | Purpose | Example |
|-------|---------|---------|
| **Layer 0** | Raw HTTP requests | `client.get("/subscriptions")` |
| **Layer 1** | Typed handlers | `DatabaseHandler::create()` returns `TaskStateUpdate` |
| **Layer 2** | Workflows + composition | `create_database_and_wait()` returns `Database` |
| **Layer 3** | User interfaces | CLI commands, MCP tools |

## Workspace Structure

```
redisctl/
├── crates/
│   ├── redisctl-core/      # Layer 2: workflows, config, unified errors
│   │   ├── src/cloud/      # Cloud workflows and params
│   │   ├── src/enterprise/ # Enterprise workflows and progress
│   │   ├── src/config/     # Profile and credential management
│   │   ├── src/error.rs    # CoreError unifying both platforms
│   │   └── src/progress.rs # Cloud task polling
│   ├── redisctl/           # CLI application (Layer 3)
│   │   ├── src/commands/   # Command implementations
│   │   ├── src/workflows/  # Multi-step CLI workflows
│   │   └── tests/          # CLI tests
│   └── redisctl-mcp/       # MCP server (Layer 3)
└── docs/                   # Documentation (you're reading it)
```

**External Dependencies (Layer 1):**

The API client libraries are maintained in separate repositories:

- [redis-cloud](https://github.com/redis-developer/redis-cloud-rs) - Redis Cloud API client
- [redis-enterprise](https://github.com/redis-developer/redis-enterprise-rs) - Redis Enterprise API client

## Layer 2: redisctl-core

The core library provides:

### Unified Error Handling

```rust
use redisctl_core::{CoreError, Result};

// CoreError wraps both platform errors
pub enum CoreError {
    Cloud(CloudError),
    Enterprise(RestError),
    TaskTimeout(Duration),
    TaskFailed(String),
    Validation(String),
    Config(String),
}
```

### Progress Callbacks

```rust
use redisctl_core::{poll_task, ProgressEvent, ProgressCallback};

// Cloud: poll task by ID
let callback: ProgressCallback = Box::new(|event| {
    if let ProgressEvent::Polling { status, elapsed, .. } = event {
        println!("Status: {} ({:.0}s)", status, elapsed.as_secs());
    }
});

let completed = poll_task(&client, &task_id, timeout, interval, Some(callback)).await?;
```

```rust
use redisctl_core::enterprise::{poll_action, EnterpriseProgressEvent};

// Enterprise: poll action by UID
let completed = poll_action(&client, &action_uid, timeout, interval, None).await?;
```

### Workflows

Workflows compose Layer 1 operations with progress tracking:

```rust
use redisctl_core::cloud::{create_database_and_wait, backup_database_and_wait};

// Create database and wait for completion
let database = create_database_and_wait(
    &client,
    subscription_id,
    &request,
    Duration::from_secs(600),
    Some(progress_callback),
).await?;

// Backup with progress
backup_database_and_wait(&client, sub_id, db_id, None, timeout, callback).await?;
```

### Configuration

```rust
use redisctl_core::{Config, Profile, DeploymentType};

let config = Config::load()?;
let profile = config.profiles.get("production")?;
let (api_key, api_secret, url) = profile.resolve_cloud_credentials()?;
```

## Library-First Design

The CLI is a thin layer over the libraries:

```rust
// Layer 1: redis-cloud crate
let client = CloudClient::builder()
    .api_key(api_key)
    .api_secret(secret_key)
    .build()?;

// Layer 2: redisctl-core workflow
let database = redisctl_core::cloud::create_database_and_wait(
    &client, sub_id, &request, timeout, None
).await?;
```

This enables:
- Terraform providers
- Backup tools
- Migration scripts
- Monitoring dashboards
- Custom automation

## API Client Architecture

### Redis Cloud

- **Auth:** `x-api-key` and `x-api-secret-key` headers
- **Base URL:** `https://api.redislabs.com/v1`
- **Async operations:** Most operations return task IDs (poll with `poll_task`)

### Redis Enterprise

- **Auth:** Basic auth (username/password)
- **Base URL:** `https://cluster:9443/v1`
- **TLS:** Self-signed certs common (`--insecure`)
- **Async operations:** Some operations return action UIDs (poll with `poll_action`)

## Error Handling

- **Layer 1:** Use `thiserror` for typed errors (`CloudError`, `RestError`)
- **Layer 2:** `CoreError` wraps both platforms
- **Layer 3:** CLI uses `anyhow` for context-rich messages

```rust
// Layer 1 error
#[derive(thiserror::Error, Debug)]
pub enum CloudError {
    #[error("API error: {0}")]
    Api(String),
}

// Layer 2 wraps it
pub enum CoreError {
    Cloud(CloudError),
    // ...
}

// Layer 3 adds context
let result = create_database_and_wait(&client, sub_id, &req, timeout, None)
    .await
    .context("Failed to create database")?;
```

## Output System

All commands support multiple output formats:

```rust
match output_format {
    OutputFormat::Table => print_table(&data),
    OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&data)?),
    OutputFormat::Yaml => println!("{}", serde_norway::to_string(&data)?),
}
```

JMESPath queries are applied before formatting.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime |
| `reqwest` | HTTP client |
| `clap` | CLI parsing |
| `serde` | Serialization |
| `jmespath` | Query filtering |
| `thiserror` | Typed errors |
| `tower-mcp` | MCP server framework |
