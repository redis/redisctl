---
hide:
  - navigation
---

# redisctl

**The CLI and MCP Server for Redis**

---

<div class="grid cards" markdown>

-   :material-rocket-launch:{ .lg .middle } __Get Started in 60 Seconds__

    ---

    Install with Homebrew or Docker and run your first command

    [:octicons-arrow-right-24: Quick Start](getting-started/quickstart.md)

-   :material-cloud:{ .lg .middle } __Redis Cloud__

    ---

    Manage subscriptions, databases, networking, and access control

    [:octicons-arrow-right-24: Cloud Commands](cloud/index.md)

-   :material-server:{ .lg .middle } __Redis Enterprise__

    ---

    Control clusters, nodes, databases, and generate support packages

    [:octicons-arrow-right-24: Enterprise Commands](enterprise/index.md)

-   :material-robot:{ .lg .middle } __MCP Server__

    ---

    AI-powered Redis operations -- explore data, debug performance, manage infrastructure

    [:octicons-arrow-right-24: Database Quickstart](mcp/database-quickstart.md)

-   :material-book-open-variant:{ .lg .middle } __Cookbook__

    ---

    Step-by-step guides for common tasks and workflows

    [:octicons-arrow-right-24: Recipes](cookbook/index.md)

</div>

---

## What is redisctl?

redisctl is a command-line tool and MCP server for Redis. It covers Redis Cloud and Redis Enterprise management, plus direct database operations with 90+ tools for data exploration, diagnostics, and module support (RedisJSON, RediSearch). The built-in MCP server lets AI assistants work with your Redis data through natural language.

```bash
# Before redisctl
curl -s -X POST "https://api.redislabs.com/v1/subscriptions/123/databases" \
  -H "x-api-key: $KEY" -H "x-api-secret-key: $SECRET" \
  -d '{"name": "mydb", ...}'
# Then poll for status... parse JSON... hope nothing changes...

# With redisctl
redisctl database create --subscription 123 --name mydb --wait
```

## Key Features

<div class="grid" markdown>

:material-shield-check: **Type-Safe API Clients**
:   Catch errors at compile time, not at 3am

:material-sync: **Async Operation Handling**
:   No more polling loops - just add `--wait`

:material-package-variant: **Support Package Automation**
:   10+ minutes of manual work in 30 seconds

:material-account-key: **Profile Management**
:   Secure credential storage with OS keyring support

:material-code-json: **Structured Output**
:   JSON, YAML, or tables with JMESPath queries

:material-puzzle: **Library-First Architecture**
:   Built on standalone [`redis-cloud`](https://crates.io/crates/redis-cloud) and [`redis-enterprise`](https://crates.io/crates/redis-enterprise) crates

</div>

## The Four Layers

redisctl provides four layers of functionality:

```mermaid
graph LR
    A[Profiles] --> B[Raw API]
    B --> C[Human Commands]
    C --> D[Workflows]

    style A fill:#dc382d,color:#fff
    style B fill:#e5c07b,color:#000
    style C fill:#98c379,color:#000
    style D fill:#61afef,color:#000
```

| Layer | Purpose | Example |
|-------|---------|---------|
| **Profiles** | Credential management | `redisctl profile set prod --api-key $KEY` |
| **Raw API** | Direct REST access | `redisctl api cloud get /subscriptions` |
| **Human Commands** | Type-safe wrappers | `redisctl database list` |
| **Workflows** | Multi-step operations | `redisctl cloud workflow subscription-setup` |

## Quick Install

=== "Homebrew"

    ``` bash
    brew install redis/homebrew-tap/redisctl
    ```

=== "Docker"

    ``` bash
    docker run ghcr.io/redis/redisctl --help
    ```

=== "Cargo"

    ``` bash
    cargo install redisctl
    ```

=== "Binary"

    ``` bash
    # Download from GitHub Releases
    curl -L https://github.com/redis/redisctl/releases/latest/download/redisctl-x86_64-unknown-linux-gnu.tar.gz | tar xz
    ```

[:octicons-arrow-right-24: Full installation guide](getting-started/installation.md)

## Who Uses redisctl?

!!! example "Support Engineers"
    Generate and upload support packages in seconds instead of minutes

!!! example "DevOps / SRE"
    Automate database provisioning in CI/CD pipelines

!!! example "Platform Engineers"
    Build self-service portals on top of redisctl libraries

!!! example "Solutions Architects"
    Quickly spin up demo environments and PoCs

!!! example "Developers"
    Explore and debug Redis data through AI-assisted natural language queries

---

<div class="grid cards" markdown>

-   :fontawesome-brands-github:{ .lg } __Open Source__

    ---

    MIT licensed. Contributions welcome.

    [GitHub](https://github.com/redis/redisctl)

-   :material-book:{ .lg } __API Docs__

    ---

    Rust library documentation on docs.rs

    [docs.rs/redisctl](https://docs.rs/redisctl)

</div>
