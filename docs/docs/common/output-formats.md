# Output Formats

Control how redisctl displays results.

For automation guarantees—including stable fields, upstream-controlled payloads, stderr behavior,
and exit codes—see the [Compatibility and Support Policy](../reference/compatibility.md).

## Available Formats

| Format | Flag | Best For |
|--------|------|----------|
| Table | `-o table` (default) | Human reading |
| JSON | `-o json` | Scripting, piping to jq |
| YAML | `-o yaml` | Configuration files |

## Table Output (Default)

Human-readable tables with aligned columns:

```bash
redisctl enterprise database list
```

```
ID  Name           Memory     Status  Endpoints
1   session-cache  1.0 GB     active  redis-12345.cluster.local:12000
2   user-data      2.0 GB     active  redis-12346.cluster.local:12001
3   analytics      4.0 GB     active  redis-12347.cluster.local:12002
```

## JSON Output

Structured data for scripting and automation:

```bash
redisctl enterprise database list -o json
```

```json
[
  {
    "uid": 1,
    "name": "session-cache",
    "memory_size": 1073741824,
    "status": "active",
    "endpoints": [
      {
        "addr": ["redis-12345.cluster.local:12000"]
      }
    ]
  }
]
```

### Pretty vs Compact

JSON is pretty-printed by default. For compact output, pipe through `jq -c`:

```bash
redisctl cloud subscription list -o json | jq -c '.[]'
```

## YAML Output

```bash
redisctl enterprise cluster get -o yaml
```

```yaml
name: production-cluster
uid: abc123
nodes:
  - uid: 1
    addr: 10.0.0.1
    status: active
  - uid: 2
    addr: 10.0.0.2
    status: active
```

## Combining with JMESPath

Use `-q` to filter before output formatting:

```bash
# Get just names as JSON array
redisctl cloud subscription list -o json -q '[].name'
```

```json
["prod-sub", "dev-sub", "staging-sub"]
```

See [JMESPath Queries](jmespath.md) for more examples.

## Scripting Examples

### Extract Single Value

```bash
# Get cluster name
CLUSTER=$(redisctl enterprise cluster get -o json -q 'name')
echo "Connected to: $CLUSTER"
```

### Loop Over Results

```bash
# Process each database
redisctl enterprise database list -o json -q '[].uid' | jq -r '.[]' | while read uid; do
  echo "Processing database $uid"
  redisctl enterprise database get "$uid" -o json > "db-$uid.json"
done
```

### Conditional Logic

```bash
# Check if any database is inactive
INACTIVE=$(redisctl enterprise database list -o json -q "[?status!='active'] | length(@)")
if [ "$INACTIVE" -gt 0 ]; then
  echo "Warning: $INACTIVE databases are not active"
fi
```

## CI/CD Integration

JSON output is essential for CI/CD pipelines:

```yaml
# GitHub Actions example
- name: Get database endpoint
  id: db
  run: |
    ENDPOINT=$(redisctl cloud database get $SUB_ID $DB_ID -o json -q 'publicEndpoint')
    echo "endpoint=$ENDPOINT" >> $GITHUB_OUTPUT

- name: Use endpoint
  run: |
    redis-cli -u redis://${{ steps.db.outputs.endpoint }} PING
```
