# MCP Server

redisctl includes a built-in [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server that turns your AI assistant into a Redis power tool -- for working with data, exploring databases, and managing infrastructure.

## What is MCP?

The Model Context Protocol is an open standard that allows AI systems to securely interact with external tools and data sources. The redisctl MCP server exposes Redis operations as tools that AI assistants like Claude, Cursor, and others can discover and invoke.

## What Can You Do?

With the MCP server, you can:

- **Explore your data** - "What keys are in my database?" or "Show me the top 10 leaderboard scores"
- **Work with any data structure** - Hashes, sets, sorted sets, lists, streams, JSON documents
- **Search and index** - "Create a search index on my products" or "Search for wireless headphones"
- **Debug and diagnose** - "What are the slowest queries?" or "Which keys use the most memory?"
- **Monitor health** - "How much memory is Redis using?" or "Show me connected clients"
- **Query infrastructure** - "List all my Cloud subscriptions" or "Show cluster health"
- **Manage deployments** - "Create a new database" or "Backup database 67890"

## Key Features

<div class="grid cards" markdown>

-   :material-shield-check:{ .lg .middle } **Secure by Default**

    ---

    Read-only mode by default. Credentials stay in your profiles, never exposed to AI.

-   :material-cloud:{ .lg .middle } **Broad Coverage**

    ---

    Hundreds of cataloged tools covering Redis Cloud, Redis Enterprise, and direct database operations.

-   :material-cog:{ .lg .middle } **IDE Integration**

    ---

    Works with Claude Desktop, Claude Code, Cursor, Windsurf, VS Code, and Zed.

-   :material-chart-line:{ .lg .middle } **Advanced Analytics**

    ---

    Combine with JMESPath MCP server for complex queries and reporting.

</div>

## Quick Example

Once configured, interact naturally with your Redis data:

> **You**: What's in my database?
>
> **Claude**: You have 1,247 keys. Here's the breakdown:
>
> | Type | Count | Examples |
> |------|-------|---------|
> | hash | 892 | `user:*`, `session:*` |
> | string | 203 | `config:*`, `cache:*` |
> | sorted set | 98 | `leaderboard:*` |
> | set | 42 | `tags:*` |

> **You**: Show me the top 5 leaderboard scores
>
> **Claude**: Top 5 in `leaderboard:global`:
>
> | Rank | Member | Score |
> |------|--------|-------|
> | 1 | player:alice | 15,230 |
> | 2 | player:bob | 14,890 |
> | 3 | player:carol | 12,450 |

> **You**: Which keys are using the most memory?
>
> **Claude**: Largest keys by memory usage:
>
> | Key | Type | Memory |
> |-----|------|--------|
> | `session:index` | sorted set | 4.2 MB |
> | `user:cache` | hash | 2.8 MB |
> | `events:main` | stream | 1.9 MB |

## Getting Started

Ready to set up MCP? Choose your path:

- **[Database Quickstart](database-quickstart.md)** - Connect to any Redis instance and start exploring
- **[Cloud Quickstart](cloud-quickstart.md)** - Redis Cloud users: manage subscriptions and databases
- **[Enterprise Quickstart](enterprise-quickstart.md)** - Redis Enterprise users: includes multi-cluster setup
- **[Getting Started](getting-started.md)** - Full installation and configuration guide
- **[Configuration](configuration.md)** - Tool selection, safety tiers, and presets

Want to see what's possible? Check out [Advanced Usage](advanced-usage.md) for JMESPath integration and complex analytics pipelines.

For operators, see [Architecture](architecture.md) for how the server works internally, and [Audit Logging](audit-logging.md) for compliance and operational visibility.
