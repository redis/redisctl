---
name: redisctl-tour
description: Give a conversational read-only introduction to an already selected standalone Redis fixture, adapting to callable tools and policy without seeding or changing targets
---

# Read only Redis tour

Show a first-time user what this connection can do, one or two tool calls at a
time. Offer connection facts or a tiny, known synthetic sample, explain each
result, then let the user choose whether to continue. This first tour covers
core standalone Redis only; Search, JSON, Cloud and Enterprise tracks are
separate workflows, not automatic branches.

## Establish the session boundary

Use only the server the user selected for this tour. Confirm its non-secret
name and intended target; never inspect credential files or ask for passwords,
tokens or credential-bearing URLs. Do not switch targets or supply per-call
`url` or `profile` overrides. If the target is uncertain, stop with advice only
and offer `redisctl-setup` from the skill index for a separate local setup task.

Read `redisctl://skills` and compare its permission-filtered `available_tools`
with the client's current callable tool list. Call `show_policy` only when it
is callable, and inspect the global tier, per-toolset overrides and allow/deny
lists. If the index, callable catalog or policy is unavailable, inconsistent
or denied, stop with advice only. A skill's text is not evidence that a tool
is installed or permitted.

Call `list_available_tools` when callable to explain preset visibility. Its
`active` list is visibility only, not permission. A global read-only tier alone
does not settle explicit allows or toolset overrides. If writes are exposed,
say: "This tour only reads; the server may permit other operations." Never
claim all writes are blocked from the global label alone.

Do not change policy, enable writes, invoke a hidden tool, use a raw-command
workaround or sweep profiles with connectivity validation. Missing tools are
a reason to narrow the offer or stop, not to enable them. These instructions
are workflow guidance, not an additional runtime permission boundary.

## Offer one supported choice

Offer only choices supported by the current callable catalog and target:

- **Connection facts** needs `redis_ping` and `redis_info`.
- **Known synthetic sample** also needs `redis_dbsize`, `redis_type`,
  `redis_ttl` and `redis_get`, plus the already prepared fixture below.

Ask one preference question. If neither choice is supported, explain the
missing capabilities without database calls. If only one is supported, offer
that choice without suggesting unavailable tracks. Do not infer module support
from a Redis version or promise Search/JSON commands because a skill is indexed.

For the sample, the user must already have prepared a disposable standalone
fixture with only two tiny strings: `demo:catalog:1` = `Synthetic headphones`
and `demo:session:1` = `Synthetic session`, each initially expiring in 1,800
seconds. Use the [connect and explore recipe](https://github.com/redis/redisctl/pull/1246)
for the preparation plan; that linked draft is not assumed to be released.
Never seed, recreate expired keys or read arbitrary customer values in this tour.
An empty database does not authorize writes. If the fixture is not confirmed,
offer connection facts instead.

## Show connection facts

After the choice, make at most these two database calls, without target overrides:

```json
{"name":"redis_ping","arguments":{}}
```

```json
{"name":"redis_info","arguments":{"section":"server"}}
```

Explain the observed PONG, Redis version and deployment mode. On a connection
or authorization error, stop; do not retry another target, profile or backend.
Do not infer installed modules, account access or database health from PONG
or the version. If deployment mode is clustered or uncertain, stop the sample
track here. Never describe a node-scoped count as a cluster-wide inventory.

For connection facts alone, summarize these observations and offer the next
step below. Otherwise ask whether to continue with the confirmed tiny fixture.

## Inspect only the known sample

Use these MCP call envelopes, not Redis CLI arguments. First run one pair:

```json
{"name":"redis_dbsize","arguments":{}}
```

```json
{"name":"redis_type","arguments":{"key":"demo:catalog:1"}}
```

Explain that DBSIZE describes the selected database on this standalone target,
not a representative production survey. Expect two keys and a string type.
If the key is missing, the type is not string, the fixture is empty or the
observations differ from the agreed fixture, stop the sample. Do not replace
it with a broader scan, another key or a write. Ask whether to continue only
when the fixture still matches.

Then run the final pair:

```json
{"name":"redis_ttl","arguments":{"key":"demo:session:1"}}
```

```json
{"name":"redis_get","arguments":{"key":"demo:catalog:1"}}
```

Check the TTL result before GET: if the session key is missing, has no expiry
or has a lifetime outside the agreed fixture, stop without reading a value.
Expect a positive lifetime no greater than 1,800 seconds and the known synthetic
catalog string. TTL decreases normally; missing or expired keys are not proof
of a connection failure. An empty string is not a missing key. A key may expire
between calls; explain that observation without retrying, repairing or expanding
the sample. Quote only the agreed synthetic value, not unexpected content.

Read-only is not a cost or disclosure budget. Do not run keyspace scans,
`redis_keys`, `redis_scan`, `redis_hgetall`, `redis_smembers`, client/ACL/slowlog
inspection or Search profiling in this tour. In particular, a returned-key
limit does not bound all work inside the legacy scan tool. Keep the sample
on known, small fixture keys rather than substituting production-wide discovery.

## Explain the boundary and offer a next step

For a request to change, delete or seed data, decline within this tour. Explain
the separate authorization and policy requirements, without attempting a write,
changing configuration or suggesting that writes must be enabled to finish.
If a write tool is absent from the callable catalog and skill index, describe
non-exposure; do not claim an attempted denial or an audit event that did not occur.

Summarize observed facts, skipped steps and the reason for each stop. Do not
turn two named keys into a whole-database assessment. Offer relevant next skills
only when listed in the current index: `redisctl-setup` for separate setup,
`data-modeling-advisor` for modeling advice, or `expert-search` for a later
Search question. Indexed guidance does not establish executable capability;
recheck tools, target capabilities and cost before a separate workflow. Do not
automatically run broader exploration or diagnostics after the summary.

The user disconnects the demo server and stops their disposable fixture using
the preparation recipe. Do not send cleanup or shutdown commands to Redis.
