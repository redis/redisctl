# Docker Usage

Run redisctl without installing anything.

## Quick Start

```bash
docker run ghcr.io/redis/redisctl --help
```

!!! note "Explicit prefixes in Docker"
    The examples on this page use explicit `cloud`/`enterprise` prefixes because Docker containers typically don't have a persistent config file. If you mount a config directory (see [Mount Config File](#mount-config-file) below), prefix inference works the same as a native install. See [Platform Inference](../common/profiles.md#platform-inference) for details.

## Passing Credentials

### Environment Variables

```bash
docker run --rm \
  -e REDIS_CLOUD_API_KEY \
  -e REDIS_CLOUD_SECRET_KEY \
  ghcr.io/redis/redisctl cloud subscription list
```

### Mount Config File

If you have a local configuration:

```bash
docker run --rm \
  -v ~/.config/redisctl:/root/.config/redisctl:ro \
  ghcr.io/redis/redisctl --profile prod cloud subscription list
```

## Convenient Aliases

Add to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.):

=== "Redis Cloud"

    ``` bash
    alias redisctl-cloud='docker run --rm \
      -e REDIS_CLOUD_API_KEY \
      -e REDIS_CLOUD_SECRET_KEY \
      ghcr.io/redis/redisctl'

    # Usage
    redisctl-cloud cloud subscription list
    ```

=== "Redis Enterprise"

    ``` bash
    alias redisctl-enterprise='docker run --rm \
      -e REDIS_ENTERPRISE_URL \
      -e REDIS_ENTERPRISE_USER \
      -e REDIS_ENTERPRISE_PASSWORD \
      -e REDIS_ENTERPRISE_INSECURE \
      ghcr.io/redis/redisctl'

    # Usage
    redisctl-enterprise enterprise cluster get
    ```

=== "With Config"

    ``` bash
    alias redisctl='docker run --rm \
      -v ~/.config/redisctl:/root/.config/redisctl:ro \
      ghcr.io/redis/redisctl'

    # Usage
    redisctl --profile prod cloud database list
    ```

## Saving Output to Files

Mount a volume to save command output:

```bash
docker run --rm \
  -e REDIS_ENTERPRISE_URL \
  -e REDIS_ENTERPRISE_USER \
  -e REDIS_ENTERPRISE_PASSWORD \
  -v $(pwd)/output:/output \
  ghcr.io/redis/redisctl \
  enterprise support-package cluster --output-dir /output
```

## MCP Server (Zero-Install)

The Docker image includes `redisctl-mcp`, so you can use the MCP server with your AI assistant without installing anything. Copy one of the `.mcp.json` snippets below and start chatting.

### Option 1: Environment Variables (Simplest)

Pass credentials as environment variables. The password is pulled from your host environment rather than hardcoded:

```json
{
  "mcpServers": {
    "redisctl": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "REDIS_ENTERPRISE_URL=https://cluster:9443",
        "-e", "REDIS_ENTERPRISE_USER=admin@redis.local",
        "-e", "REDIS_ENTERPRISE_PASSWORD",
        "ghcr.io/redis/redisctl",
        "redisctl-mcp"
      ]
    }
  }
}
```

### Option 2: Mounted Config

Mount your existing redisctl config directory for profile-based auth:

```json
{
  "mcpServers": {
    "redisctl": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-v", "~/.config/redisctl:/root/.config/redisctl:ro",
        "ghcr.io/redis/redisctl",
        "redisctl-mcp", "--profile", "my-profile"
      ]
    }
  }
}
```

### Option 3: Local Clusters (Host Networking)

For clusters running on localhost (e.g. Docker Compose demos), use `--network host` so the container can reach host ports:

```json
{
  "mcpServers": {
    "redisctl": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "--network", "host",
        "-e", "REDIS_ENTERPRISE_URL=https://localhost:9443",
        "-e", "REDIS_ENTERPRISE_USER=admin@redis.local",
        "-e", "REDIS_ENTERPRISE_PASSWORD",
        "-e", "REDIS_ENTERPRISE_INSECURE=true",
        "ghcr.io/redis/redisctl",
        "redisctl-mcp"
      ]
    }
  }
}
```

!!! note
    `--network host` is required on Linux. On macOS, Docker Desktop routes `host.docker.internal` to the host automatically, but `--network host` is the simplest cross-platform approach.

### Running from the CLI

You can also run the MCP server directly:

```bash
# With environment credentials
docker run -i --rm \
  -e REDIS_ENTERPRISE_URL \
  -e REDIS_ENTERPRISE_USER \
  -e REDIS_ENTERPRISE_PASSWORD \
  ghcr.io/redis/redisctl \
  redisctl-mcp

# With mounted config
docker run -i --rm \
  -v ~/.config/redisctl:/root/.config/redisctl:ro \
  ghcr.io/redis/redisctl \
  redisctl-mcp --profile my-profile
```

!!! tip
    Native installation is recommended for regular MCP usage since Docker adds latency to each tool call. Docker is best for quick trials and CI environments.

## Image Tags

| Tag | Description |
|-----|-------------|
| `latest` | Most recent release |
| `0.7.7` | Specific version |
| `0.7` | Latest patch in minor version |

```bash
# Pin to specific version
docker run ghcr.io/redis/redisctl:0.7.7 --version
```

## CI/CD Example

```yaml
# GitHub Actions
- name: List databases
  run: |
    docker run --rm \
      -e REDIS_CLOUD_API_KEY=${{ secrets.REDIS_CLOUD_API_KEY }} \
      -e REDIS_CLOUD_SECRET_KEY=${{ secrets.REDIS_CLOUD_SECRET_KEY }} \
      ghcr.io/redis/redisctl \
      cloud database list --subscription-id ${{ vars.SUBSCRIPTION_ID }} -o json
```
