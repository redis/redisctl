# Presentation

Presentation materials for introducing redisctl to your team.

## Slide Deck

<a href="slides.html" target="_blank" class="md-button md-button--primary">
  :material-presentation-play: View Slides
</a>

The presentation covers:

- The problem: No CLI tooling for Redis Cloud/Enterprise
- The solution: redisctl's four-layer architecture
- Demo: Profile setup, Cloud operations, Enterprise operations
- Raw API access for any endpoint
- Support package automation
- Output formats and JMESPath queries
- Async operation handling
- Use cases by persona
- Installation and getting started

## Using the Slides

| Key | Action |
|-----|--------|
| Arrow keys | Navigate slides |
| `Space` | Next slide |
| `Esc` | Overview mode |
| `S` | Speaker notes |
| `F` | Fullscreen |
| `?` | Help |

## Sharing

Direct link to slides: `https://redis-field-engineering.github.io/redisctl-docs/presentation/slides.html`

## Customizing

The slides use [reveal.js](https://revealjs.com/). To customize:

1. Edit `docs/presentation/slides.html`
2. Slides are in `<section>` tags
3. Use standard HTML and reveal.js features

## Quick Pitch

Need a one-liner?

> **redisctl** manages Redis Cloud, Redis Enterprise, and Redis databases from one tool -- as a CLI for humans or an MCP server for AI agents.

## Demo Script

### 1. Setup (30 seconds)

```bash
# Show installation
brew install redis/homebrew-tap/redisctl

# Configure profile
redisctl profile set demo \
  --type enterprise \
  --url "https://cluster:9443" \
  --username "admin@cluster.local" \
  --password "$PASSWORD"
```

### 2. Basic Commands (1 minute)

```bash
# Cluster info
redisctl enterprise cluster get

# List databases
redisctl enterprise database list

# JSON output with filtering
redisctl enterprise database list -o json -q '[].{name: name, memory: memory_size}'
```

### 3. License & Cluster Management (1 minute)

```bash
# Check license status
redisctl enterprise license get

# Check license usage against limits
redisctl enterprise license usage

# View cluster policy
redisctl enterprise cluster get-policy
```

### 4. Support Package (30 seconds)

```bash
# The killer feature
redisctl enterprise support-package cluster --optimize --upload
```

### 5. Raw API Access (30 seconds)

```bash
# Any endpoint
redisctl api enterprise get /v1/nodes

# Compare to curl
curl -k -u "user:pass" https://cluster:9443/v1/nodes | jq
```

Total demo time: ~4 minutes

---

## MCP Demo Script (AI Integration)

For customers interested in AI-driven automation:

### 1. Start MCP Server

```bash
# Start with Enterprise profile (read-only by default)
redisctl-mcp --profile demo

# Or with write operations enabled
redisctl-mcp --profile demo --read-only=false
```

### 2. Configure Claude Desktop / Cursor / etc.

```json
{
  "mcpServers": {
    "redisctl": {
      "command": "redisctl-mcp",
      "args": ["--profile", "demo"]
    }
  }
}
```

### 3. Example Prompts to Demo

- "What's our license status and when does it expire?"
- "Show me all databases and their memory usage"
- "Which nodes have the most shards?"
- "Are there any active alerts on the cluster?"
- "Check the cluster policy settings"

### 4. Write Operations (if enabled)

- "Enable maintenance mode for tonight's upgrade"
- "Update the cluster policy to use sparse shard placement"
- "Create a new 2GB database called test-cache"

Total MCP demo time: ~5 minutes

---

## Multi-Cluster Management Demo

For customers managing multiple Redis Enterprise clusters:

### The Problem

The Redis Enterprise UI manages ONE cluster at a time. Customers with multiple clusters need to:
- Log into each UI separately
- Manually aggregate information across clusters
- No unified view of license usage, database inventory, or alerts

### The Solution

redisctl provides unified multi-cluster management through profiles:

### 1. Configure Multiple Clusters

```bash
# Add each cluster as a profile
redisctl profile set cluster-west --type enterprise \
  --url https://west.example.com:9443 \
  --username admin@redis.local \
  --password "$PASSWORD"

redisctl profile set cluster-east --type enterprise \
  --url https://east.example.com:9443 \
  --username admin@redis.local \
  --password "$PASSWORD"

redisctl profile set cluster-central --type enterprise \
  --url https://central.example.com:9443 \
  --username admin@redis.local \
  --password "$PASSWORD"
```

### 2. Cross-Cluster CLI Queries

```bash
# Check all clusters at once
for p in cluster-west cluster-east cluster-central; do
  echo "=== $p ==="
  redisctl -p $p enterprise cluster get -o json -q '{name: name, nodes: nodes_count}'
done

# Total memory across all clusters
total=0
for p in cluster-west cluster-east cluster-central; do
  mem=$(redisctl -p $p enterprise database list -o json -q '[].memory_size | sum(@)')
  echo "$p: $((mem / 1024 / 1024)) MB"
  total=$((total + mem))
done
echo "Total: $((total / 1024 / 1024)) MB"

# License check across clusters
for p in cluster-west cluster-east cluster-central; do
  echo "=== $p ==="
  redisctl -p $p enterprise license get -o json -q '{expired: expired, expiration: expiration_date}'
done
```

### 3. MCP Multi-Cluster Prompts

With MCP, the AI can query across clusters conversationally:

- "List all my configured profiles"
- "Show me the cluster info for each of my Enterprise profiles"
- "What's my total memory allocation across all clusters?"
- "Check license status on all clusters - are any expiring soon?"
- "Which databases across all clusters don't have persistence enabled?"
- "Total up all the databases across my fleet"

The AI uses `profile_list` to discover clusters and targets each one via the `profile` parameter on tool calls, then aggregates the results.

### Key Differentiators

| Capability | RE UI | redisctl |
|------------|-------|----------|
| Single cluster view | ✅ | ✅ |
| Multi-cluster view | ❌ | ✅ |
| Cross-cluster queries | ❌ | ✅ |
| License aggregation | ❌ | ✅ |
| Scripted operations | ❌ | ✅ |
| AI-driven management | ❌ | ✅ (MCP) |

Total multi-cluster demo time: ~5 minutes
