---
name: redisctl-setup
description: Install and configure redisctl CLI for Redis Cloud, Enterprise, and direct database access. Use when setting up redisctl for the first time, creating profiles, or configuring shell completions.
---

## Overview

redisctl is a unified CLI for managing Redis Cloud, Redis Enterprise, and direct Redis database connections. This skill covers installation, profile setup, and validation.

## Installation

Choose one method:

```bash
# Homebrew (macOS/Linux)
brew install redis-developer/tap/redisctl

# Cargo (from source)
cargo install redisctl

# Docker
docker run --rm -it ghcr.io/redis/redisctl --help
```

## Profile Setup

Profiles store connection credentials. Use the interactive wizard for guided setup:

```bash
redisctl profile init
```

Or create profiles directly:

### Redis Cloud Profile

```bash
redisctl profile set cloud-prod \
  --type cloud \
  --api-key "$REDIS_CLOUD_API_KEY" \
  --api-secret "$REDIS_CLOUD_API_SECRET"

redisctl profile default-cloud cloud-prod
```

### Redis Enterprise Profile

```bash
redisctl profile set ent-staging \
  --type enterprise \
  --url "https://cluster.internal:9443" \
  --username admin \
  --password "$RE_PASSWORD"

redisctl profile default-enterprise ent-staging
```

### Direct Database Profile

```bash
redisctl profile set local-redis \
  --type database \
  --url "redis://localhost:6379"

redisctl profile default-database local-redis
```

## Validation

After creating profiles, verify connectivity:

```bash
# List all profiles
redisctl profile list

# Show a specific profile
redisctl profile show cloud-prod

# Validate connectivity for all profiles
redisctl profile validate
```

## Shell Completions

Generate completions for your shell:

```bash
# Bash
redisctl completions bash > ~/.bash_completion.d/redisctl

# Zsh
redisctl completions zsh > ~/.zfunc/_redisctl

# Fish
redisctl completions fish > ~/.config/fish/completions/redisctl.fish
```

## Common Issues

- **Credential errors**: Check that environment variables are set and profile type matches the command namespace (cloud profile for cloud commands, etc.)
- **Enterprise TLS errors**: Use `--insecure` flag for self-signed certificates during setup
- **Config file location**: Run `redisctl profile path` to find the config file
