# Common Features

Features shared across Redis Cloud and Redis Enterprise commands.

## The Four Layers

redisctl is built around four layers of functionality:

```mermaid
graph TB
    subgraph "Layer 4: Workflows"
        W[Multi-step orchestration]
    end
    subgraph "Layer 3: Human Commands"
        H[Type-safe wrappers]
    end
    subgraph "Layer 2: Raw API"
        R[Direct REST access]
    end
    subgraph "Layer 1: Profiles"
        P[Credential management]
    end

    P --> R --> H --> W

    style W fill:#61afef,color:#000
    style H fill:#98c379,color:#000
    style R fill:#e5c07b,color:#000
    style P fill:#dc382d,color:#fff
```

| Layer | When to Use |
|-------|-------------|
| **Profiles** | Always - manage credentials across environments |
| **Raw API** | Exploring, debugging, or accessing undocumented endpoints |
| **Human Commands** | Day-to-day operations with friendly output |
| **Workflows** | Complex multi-step provisioning tasks |

## Destructive Commands

Commands that delete, flush, reset, or otherwise make irreversible changes ask for confirmation
when run interactively. Use the long-only `--force` flag to bypass that prompt in automation:

```bash
redisctl cloud database delete 123456:789 --force
```

`--force` only skips the confirmation prompt; it does not change API validation, permissions, or
error handling. The older `--yes` and `-y` spellings remain as hidden compatibility aliases on the
commands that previously exposed them. They are deprecated, will be accepted throughout the 0.x
release series, and are scheduled for removal in 1.0.

## Features at a Glance

<div class="grid cards" markdown>

-   :material-account-key:{ .lg .middle } __Profiles__

    ---

    Manage credentials for multiple Redis deployments

    [:octicons-arrow-right-24: Learn more](profiles.md)

-   :material-code-json:{ .lg .middle } __Output Formats__

    ---

    JSON, YAML, or human-readable tables

    [:octicons-arrow-right-24: Learn more](output-formats.md)

-   :material-filter:{ .lg .middle } __JMESPath Queries__

    ---

    Filter and transform output with powerful queries

    [:octicons-arrow-right-24: Learn more](jmespath.md)

-   :material-sync:{ .lg .middle } __Async Operations__

    ---

    Wait for long-running operations to complete

    [:octicons-arrow-right-24: Learn more](async-operations.md)

-   :material-api:{ .lg .middle } __Raw API Access__

    ---

    Direct REST calls to any endpoint

    [:octicons-arrow-right-24: Learn more](raw-api.md)

</div>
