# Database Quickstart

Get up and running with MCP for Redis database operations in 5 minutes. No Redis Cloud or Enterprise account required -- just a Redis instance.

## Prerequisites

- A running Redis instance (local, Docker, or remote)
- redisctl installed ([Installation Guide](../getting-started/installation.md))

For a first agent conversation without customer data, try the
[synthetic connect and explore demo](demo/index.md). It uses a
disposable local fixture and an explicit read-only policy.

## Step 1: Create a Database Profile

=== "Profile (Recommended)"

    ```bash
    redisctl profile set mydb --type database \
      --host localhost \
      --port 6379
    ```

    For authenticated or TLS connections:

    ```bash
    redisctl profile set mydb --type database \
      --host redis.example.com \
      --port 6380 \
      --username default \
      --password YOUR_PASSWORD \
      --tls
    ```

=== "Direct URL"

    Skip the profile and pass a URL directly in your MCP config (Step 2).

## Step 2: Configure Your AI Assistant

Add to your MCP configuration (read-only by default for safe exploration):

=== "Claude Desktop"

    **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`

    ```json
    {
      "mcpServers": {
        "redisctl": {
          "command": "redisctl-mcp",
          "args": ["--profile", "mydb", "--tools", "database"]
        }
      }
    }
    ```

=== "Claude Code"

    Add to `.mcp.json` in your project:

    ```json
    {
      "mcpServers": {
        "redisctl": {
          "command": "redisctl-mcp",
          "args": ["--profile", "mydb", "--tools", "database"]
        }
      }
    }
    ```

=== "Cursor"

    **macOS**: `~/.cursor/mcp.json`

    ```json
    {
      "mcpServers": {
        "redisctl": {
          "command": "redisctl-mcp",
          "args": ["--profile", "mydb", "--tools", "database"]
        }
      }
    }
    ```

!!! tip "Direct URL alternative"
    If you skipped the profile, replace `"--profile", "mydb"` with `"--database-url", "redis://localhost:6379"`.

Restart your AI assistant to load the new configuration.

## Step 3: Try These Prompts

Start a conversation with your AI assistant and try these examples:

### Explore Your Data

| Prompt | What it does |
|--------|--------------|
| "What keys are in my database?" | Scans keys with pattern matching |
| "How many keys do I have?" | Returns the DBSIZE count |
| "What type is the key `users:1`?" | Shows the data type of a key |
| "Show me the health of this Redis instance" | Runs a comprehensive health check |

### Work with Data Structures

| Prompt | What it does |
|--------|--------------|
| "Show me all fields in the hash `user:42`" | Returns HGETALL for the hash |
| "What are the top 10 scores in the `leaderboard` sorted set?" | Returns ZRANGE with scores |
| "List the members of the `active-users` set" | Returns SMEMBERS |
| "Show the last 5 items in the `events` stream" | Returns XRANGE for recent entries |

### Diagnostics and Performance

| Prompt | What it does |
|--------|--------------|
| "What are the slowest queries?" | Shows the slow log |
| "How much memory is Redis using?" | Returns memory stats |
| "Which keys are using the most memory?" | Samples keys by memory size |
| "Show me the connected clients" | Lists client connections |

### RedisJSON (requires JSON module)

| Prompt | What it does |
|--------|--------------|
| "Get the JSON document at key `product:100`" | Returns JSON.GET result |
| "What are the object keys in `config`?" | Returns JSON.OBJKEYS |

### RediSearch (requires Search module)

| Prompt | What it does |
|--------|--------------|
| "List all search indexes" | Returns FT._LIST |
| "Show me the schema for index `idx:products`" | Returns FT.INFO |
| "Search `idx:products` for 'wireless headphones'" | Runs FT.SEARCH |

## Optional: Enable Write Operations

By default, the MCP server runs in **read-only mode** -- you can explore freely without risk of modifying anything. When you're ready for write operations, update your config:

```json
{
  "mcpServers": {
    "redisctl": {
      "command": "redisctl-mcp",
      "args": ["--profile", "mydb", "--tools", "database", "--read-only=false"]
    }
  }
}
```

Write prompts to try:

| Prompt | What it does |
|--------|--------------|
| "Set the key `greeting` to 'hello world'" | Sets a string value |
| "Add user data to the hash `user:99`" | Creates/updates hash fields |
| "Store this JSON document as `doc:1`" | Uses JSON.SET to store structured data |
| "Create a search index on my user hashes" | Uses FT.CREATE to define an index |

## Example Conversation

> **You**: What's in my database?
>
> **AI**: Your database has 1,247 keys. Here's a breakdown by type:
>
> | Type | Count | Example Keys |
> |------|-------|-------------|
> | hash | 892 | `user:*`, `session:*` |
> | string | 203 | `config:*`, `cache:*` |
> | sorted set | 98 | `leaderboard:*`, `timeline:*` |
> | set | 42 | `tags:*`, `active-users` |
> | stream | 12 | `events:*` |

> **You**: Show me the top 5 leaderboard scores
>
> **AI**: Here are the top 5 entries in `leaderboard:global`:
>
> | Rank | Member | Score |
> |------|--------|-------|
> | 1 | player:alice | 15,230 |
> | 2 | player:bob | 14,890 |
> | 3 | player:carol | 12,450 |
> | 4 | player:dave | 11,200 |
> | 5 | player:eve | 10,890 |

> **You**: What's the memory breakdown?
>
> **AI**: Redis is using **48.2 MB** of memory:
>
> - Dataset: 32.1 MB (66.6%)
> - Overhead: 12.4 MB (25.7%)
> - Lua: 3.7 MB (7.7%)
> - Peak usage: 52.8 MB

## Troubleshooting

### "Connection refused"

- Verify Redis is running: `redis-cli ping`
- Check the host and port in your profile match your Redis instance
- For Docker Redis, ensure the port is published (`-p 6379:6379`)

### "Authentication required" or "NOAUTH"

- Add `--password` to your profile or include credentials in the URL
- For Redis 6+ ACLs, ensure the user has appropriate permissions

### Tools not appearing

- Restart your AI assistant after config changes
- Check that `redisctl-mcp` is in your PATH
- Verify the `--tools database` flag is set

### RedisJSON/RediSearch tools not working

- These tools require the corresponding Redis modules to be loaded
- Check with `redis-cli MODULE LIST`
- Use [Redis Stack](https://redis.io/docs/latest/operate/oss_and_stack/install/install-stack/) for an all-in-one install

## Next Steps

- [Tools Reference](tools-reference.md) - Complete list of database tools
- [Advanced Usage](advanced-usage.md) - Complex queries with JMESPath
- [Cloud Quickstart](cloud-quickstart.md) - If you also use Redis Cloud
- [Enterprise Quickstart](enterprise-quickstart.md) - If you also use Redis Enterprise
