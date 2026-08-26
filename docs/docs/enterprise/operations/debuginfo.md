# Debug Info

Collect detailed debugging information from Redis Enterprise clusters.

## Overview

Debug info provides lower-level diagnostic data than support packages. Use it for:

- Quick diagnostics without full support package
- Targeted debugging of specific components
- Automated collection in scripts

For support tickets, use [support-package](support-package.md) instead.

## Commands

### Cluster Debug Info

```bash
redisctl enterprise debug-info all
```

### Node Debug Info

```bash
# All nodes
redisctl enterprise debug-info node

# Specific node
redisctl enterprise debug-info node --node-uid 1
```

### Database Debug Info

```bash
redisctl enterprise debug-info database 1
```

## Options

| Option | Description |
|--------|-------------|
| `-f, --file` | Output file path |

## Examples

### Save to File

```bash
redisctl enterprise debug-info all --file cluster-debug.tar.gz
```

### Collect for Specific Issue

```bash
# Database issue
redisctl enterprise debug-info database 1 --file db1-debug.tar.gz

# Node issue
redisctl enterprise debug-info node --node-uid 2 --file node2-debug.tar.gz
```

## Raw API Access

```bash
# Cluster debug info
redisctl api enterprise get /v1/cluster/debuginfo

# Node debug info
redisctl api enterprise get /v1/nodes/1/debuginfo
```

## Debug Info vs Support Package

| Feature | Debug Info | Support Package |
|---------|-----------|-----------------|
| Size | Smaller | Larger (more complete) |
| Optimization | No | Yes (`--optimize`) |
| Upload | No | Yes (`--upload`) |
| Progress | Basic | Enhanced |
| Use case | Quick debug | Support tickets |

## Related

- [Support Package](support-package.md) - Full diagnostic packages for support
- [Cluster Health](../../cookbook/enterprise/cluster-health.md) - Health monitoring
