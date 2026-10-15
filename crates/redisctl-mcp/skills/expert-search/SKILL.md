---
name: expert-search
description: Explain Redis Search schemas, query syntax and aggregation inputs using the tools actually available, with version and diagnostic cost checks
---

# Redis Search expertise

Use this reference before writing Search queries, explaining empty results, or
recommending an index. Start with the question and actual schema, then choose a
supported tool. For a full workflow, use [index-advisor](../index-advisor/SKILL.md)
or [query-tuning](../query-tuning/SKILL.md); neither a workflow nor this reference
grants permission to run commands.

## Check the target and available capabilities

Read `redisctl://skills` and compare `available_tools` with the current callable
tools. Call `show_policy` when available. If a required tool or Search capability
is absent or denied, explain the gap and continue with advice only. Do not enable
writes, switch targets, or use a raw-command path to bypass a denial.

Confirm the intended target and index with the user, using configuration selected
locally. Never request credentials in chat. Record the actual Redis and Search
build when known; do not infer module support or query dialect from a Redis major
version alone. Inspect the index's attributes, aliases, document type, indexing
status and failures before proposing a query. Avoid broad key scans or reading
whole documents just to discover a schema.

## Match field types to the question

| Need | Field type | Query form |
| ---- | ---------- | ---------- |
| Words and phrases with full-text behavior | TEXT | `@title:wireless` |
| Category or identifier membership | TAG | `@category:{audio}` |
| Numeric interval | NUMERIC | `@price:[50 200]` |

Choose by semantics, not cardinality alone: replacing TAG with TEXT changes
tokenization and matching. Read the existing schema rather than guessing a type
from a field's name. Use declared aliases for JSON paths, such as `tags` for
`$.tags[*] AS tags TAG`.

TAG matching is case-insensitive by default; `CASESENSITIVE` changes that.
Hash TAG fields split on commas by default, but JSON strings do not. A JSON
value `"red,blue"` is one tag unless an explicit separator is configured; a JSON
array with a `[*]` path represents separate values. These distinctions often
explain empty results. Check the actual field options. See [TAG behavior](https://redis.io/docs/latest/develop/ai/search-and-query/advanced-concepts/tags/).

Escape punctuation at the query-string layer and then at the transport layer.
The literal TAG `usb-c` becomes `usb\-c` in the query and `usb\\-c` inside a JSON
string. Do not interpolate untrusted values as query syntax. Whitespace and
operator grouping depend on dialect; inspect parsing rather than guessing.

## Respect the wrapper and dialect limits

Use the current tool schema, not the Redis command reference, as the list of MCP
input fields. In this wrapper:

- `redis_ft_search` has pagination, sorting and result-shaping inputs, but no
  `dialect`, `params` or `timeout` input.
- `redis_ft_aggregate` accepts `raw_args` for pipeline tokens, not top-level
  `group_by` or `reduce` inputs. Older builds can silently ignore unknown fields;
  apparent success does not prove those fields were used.
- `redis_ft_explain` accepts `index` and `query`, without a dialect override.
- `redis_ft_profile` requires `index`, `command` and `query`. Its command is
  `SEARCH` or `AGGREGATE`, not `HYBRID`. It exposes no pagination, timeout or
  dialect override.

`DIALECT` is a command option, not text to append inside `query`. Do not invent
fields or change the global `DEFAULT_DIALECT` to make an example work. The
[dialect reference](https://redis.io/docs/latest/develop/ai/search-and-query/advanced-concepts/dialects/)
describes multiple dialects. In the pinned Search **2.10.31** source,
[the initial default is 1](https://github.com/RediSearch/RediSearch/blob/989e8372dabb03328a98a1f478b3862d1b5704d0/src/config.h#L305);
that is not evidence of any particular target's configured default. If a query
needs explicit dialect selection or parameter binding that this tool cannot
express, stop at a proposed query and explain the missing capability. Do not
silently substitute literal vector data or a different execution path.

## Inspect and query a small agreed dataset

The following are MCP call envelopes, not Redis CLI commands. They assume an
already-created, tiny synthetic index `idx:expert_demo`, with `title` TEXT,
`category` and `tags` TAG, and `price` NUMERIC. They do not create data or indexes.
Only run them on the agreed target after checking availability, policy and cost.

Inspect metadata first:

```json
{"name":"redis_ft_info","arguments":{"index":"idx:expert_demo"}}
```

Retrieve at most five documents and only the required fields:

```json
{"name":"redis_ft_search","arguments":{"index":"idx:expert_demo","query":"@tags:{usb\\-c} @price:[50 200]","limit_offset":0,"limit_num":5,"return_fields":["title","price"]}}
```

This limits returned rows, not all server work. Search may still inspect many
candidates or sort them. Do not claim a latency bound from `limit_num`.

For a parsing question, inspect the expression before executing more searches:

```json
{"name":"redis_ft_explain","arguments":{"index":"idx:expert_demo","query":"@tags:{usb\\-c} @price:[50 200]"}}
```

## Build aggregation inputs explicitly

For counts per category, load only the required attribute, group, reduce, sort
and then cap the output. Stage order changes the calculation. In this example,
`GROUPBY 1` consumes one property, `COUNT 0` takes no reducer arguments, and
`SORTBY 2` consumes the property and direction tokens. See [FT.AGGREGATE](https://redis.io/docs/latest/commands/ft.aggregate/).

```json
{"name":"redis_ft_aggregate","arguments":{"index":"idx:expert_demo","query":"@price:[50 200]","load_fields":["@category"],"raw_args":["GROUPBY","1","@category","REDUCE","COUNT","0","AS","count","SORTBY","2","@count","DESC"],"limit_offset":0,"limit_num":5}}
```

Read the named `count` output as the per-group count. Do not interpret the
wrapper's leading `Total results` label as a business count or proof the intended
pipeline ran. If rows are blank or fields are missing, check the accepted inputs
and pipeline before drawing conclusions. Neither `LIMIT` nor loading one field
makes aggregation cheap on a large dataset; avoid `LOAD *`.

## Profile only after a separate cost check

`FT.PROFILE` **executes** the search or aggregation; it is not a dry run. Check
the expected candidate set and obtain agreement on the diagnostic work. If cost
cannot be bounded appropriately, explain what evidence is needed and do not run
it. Read-only permission alone is insufficient. The [command reference](https://redis.io/docs/latest/commands/ft.profile/)
describes execution and profiling output; this wrapper exposes fewer options.

Only on the tiny synthetic fixture, after that check:

```json
{"name":"redis_ft_profile","arguments":{"index":"idx:expert_demo","command":"SEARCH","query":"@tags:{usb\\-c} @price:[50 200]"}}
```

Report observed timing and iterator counts with the build, schema and query.
Do not promise a speedup from textual filter order or replace an index based on
one timing sample. Index creation, alterations, alias changes and drop/rebuild
are separate write operations requiring an explicit plan and authorization;
never perform them as an automatic diagnostic step.
