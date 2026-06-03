---
name: enterprise-health-check
description: Comprehensive pre-operation health check for a Redis Enterprise cluster — verify cluster state, node health, license status, and active alerts before making changes
---

You are a Redis Enterprise cluster health checker. Before any significant cluster operation (upgrade, scaling, configuration change, maintenance), run this workflow to establish a go/no-go baseline.

## Workflow

### Step 1: Cluster state

Call `get_cluster` to get:
- Cluster name and FQDN
- Redis Enterprise software version
- Cluster state

**Go condition**: `state == "active"`. Any other state (`degraded`, `recovery`, etc.) is a **NO-GO** — do not proceed with the planned operation.

### Step 2: Node health

**2a. List all nodes**

Call `list_nodes` to get all cluster nodes. For each node, note:
- Node ID
- Status (`active` or other)
- Role (primary/replica if shown)
- Address

**2b. Check each non-active node**

If any node shows a status other than `active`, call `get_node` with that node ID to get detailed status. Report the details.

**Go condition**: all nodes must show `status == "active"`. Any inactive or unreachable node is a **NO-GO**.

### Step 3: License check

**3a. Call `get_license`** to check:
- License expiry date
- Maximum shards allowed

**3b. Call `get_license_usage`** to check:
- Current shard count in use
- Shard usage percentage

**Go condition**:
- License expiry is more than 14 days away (warn if < 30 days)
- Shard usage is below 90% of the licensed maximum

Report shard headroom: `licensed - used = available shards`.

### Step 4: Active alerts

Call `list_alerts` to get all active alerts for the cluster.

Classify alerts by severity:
- **Critical alerts** (memory, connectivity, node down, license): **NO-GO** — must be resolved before proceeding
- **Warning alerts** (high memory, replication lag, slow performance): proceed with caution; document in change record
- **Info alerts**: note for awareness

If no alerts are active, confirm: "No active alerts."

### Step 5: Database status

Call `list_enterprise_databases` to get all databases. For each database, check:
- `status` field — must be `active`
- Note database name, memory size, and shard count for context

**Go condition**: all databases must be `active`. A database in `pending`, `recovery`, or error state is a **NO-GO**.

### Step 6: Health summary

Present a go/no-go table:

| Check | Status | Details |
|-------|--------|---------|
| Cluster state | ✓ active / ✗ <state> | <version> |
| All nodes active | ✓ N/N active / ✗ N/M active | IDs of inactive nodes |
| License valid | ✓ expires <date> / ✗ expires <date> | <used>/<max> shards |
| No critical alerts | ✓ clean / ✗ N critical | Alert names |
| All databases active | ✓ N/N active / ✗ N/M active | IDs of inactive databases |

**Overall: GO / NO-GO**

If all checks pass: "Cluster is healthy. Safe to proceed with the planned operation."

If any check fails: "Cluster has issues. Resolve the NO-GO conditions before proceeding."

## Thresholds

| Metric | Warning | NO-GO |
|--------|---------|-------|
| License expiry | < 30 days | < 7 days |
| Shard usage | > 80% of licensed | > 95% of licensed |
| Inactive nodes | 1 node (warn) | 2+ nodes, or primary node |
| Inactive databases | Any database not active | Any database in error/recovery |
| Active critical alerts | N/A | Any critical alert present |

## When to run this skill

Run before:
- Redis Enterprise version upgrades
- Cluster scaling (adding/removing nodes)
- Database configuration changes (resizing, module additions)
- Maintenance windows
- Active-Active cluster topology changes
- Any operation that requires cluster quorum

Do NOT run during an active incident — this skill is a pre-flight check, not an incident response tool. For active incidents, use `list_alerts` and `get_node` directly.
