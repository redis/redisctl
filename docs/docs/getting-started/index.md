# Getting Started

Get up and running with redisctl in minutes.

## Choose Your Path

<div class="grid cards" markdown>

-   :material-docker:{ .lg .middle } __Try with Docker__

    ---

    No installation required. Run commands immediately.

    ``` bash
    docker run ghcr.io/redis/redisctl --help
    ```

    [:octicons-arrow-right-24: Docker guide](docker.md)

-   :material-download:{ .lg .middle } __Install Locally__

    ---

    Homebrew, Cargo, or download binaries.

    [:octicons-arrow-right-24: Installation](installation.md)

</div>

## Quick Example

=== "Redis Cloud"

    ``` bash
    # Set credentials
    export REDIS_CLOUD_API_KEY="your-api-key"
    export REDIS_CLOUD_SECRET_KEY="your-secret-key"

    # List your subscriptions
    redisctl cloud subscription list

    # Get databases in a subscription
    redisctl cloud database list --subscription-id 123456
    ```

=== "Redis Enterprise"

    ``` bash
    # Set credentials
    export REDIS_ENTERPRISE_URL="https://cluster.example.com:9443"
    export REDIS_ENTERPRISE_USER="admin@cluster.local"
    export REDIS_ENTERPRISE_PASSWORD="your-password"

    # Get cluster info
    redisctl enterprise cluster get

    # List databases
    redisctl enterprise database list
    ```

## Next Steps

1. **[Installation](installation.md)** - Install redisctl on your system
2. **[Quick Start](quickstart.md)** - Run your first commands
3. **[Authentication](authentication.md)** - Set up profiles for multiple environments
