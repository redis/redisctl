# Connect and explore with a Redis agent

In this walkthrough, your agent connects to a disposable Redis instance, reads
two synthetic keys and explains why it cannot write. You prepare the fixture;
the agent does not install software, create infrastructure or change policy.

Allow about 10 minutes after the prerequisites are installed. This recipe uses
macOS or Linux, stdio transport and the existing `legacy` database backend.
It does not require Search or JSON tools, a plugin or a Cloud account.

## Check the version and prerequisites

The reference tool contract is redisctl `0.12.1`, source revision
[`8d383a1`](https://github.com/redis/redisctl/tree/8d383a1e4eb6d81bc0760a66b36784751d7ebf55).
The local fixture uses Redis `8.10.2` standalone. Record your actual versions:

```bash
redisctl-mcp --version
redisctl-mcp --help
redis-server --version
```

Use an already installed build with the options shown below. If an option is
missing, stop and check the [installation guide](../../getting-started/installation.md)
rather than dropping a safety or target-selection option. A version number alone
does not identify a source revision; record the build revision when available.

Your MCP client must support a new project's server configuration and per-server
environment settings. Use an empty demo project, not an application's existing
client configuration. Do not change a global client file to follow this recipe.

## Start an isolated fixture

In a terminal, create a fresh directory and start Redis in the foreground:

```bash
demo_dir="$(mktemp -d "${TMPDIR:-/tmp}/redisctl-demo.XXXXXX")"
mkdir -p "$demo_dir/home/.config/redisctl" "$demo_dir/config"
printf '%s\n' "$demo_dir"
redis-server --port 0 \
  --unixsocket "$demo_dir/redis.sock" --unixsocketperm 700 \
  --save "" --appendonly no --dir "$demo_dir"
```

Leave that terminal open. TCP is disabled; the socket is private to your user.
Persistence is disabled, so stopping this Redis instance discards its data.

In a second terminal, set `demo_dir` to the exact directory printed above. Do
not substitute a socket belonging to another Redis instance. Seed only this
new fixture with two small synthetic values:

```bash
demo_dir=/absolute/path/printed/by/the/first/terminal
redis-cli -s "$demo_dir/redis.sock" PING
redis-cli -s "$demo_dir/redis.sock" SET demo:catalog:1 'Synthetic headphones' EX 1800
redis-cli -s "$demo_dir/redis.sock" SET demo:session:1 'Synthetic session' EX 1800
redis-cli -s "$demo_dir/redis.sock" DBSIZE
```

Expect `PONG`, two `OK` replies and a key count of `2`. If those checks fail,
stop before connecting the agent. The keys expire after 30 minutes; an expired
fixture is not a connection or tool failure. Only you seed the fixture; do not
ask the read-only agent to recreate missing data.

## Configure only the demo server

In your editor, create `read-only.toml` inside the demo directory:

```toml
tier = "read-only"
allow = []
deny = ["redis_set", "redis_del", "redis_flushdb"]
```

Add this server to the new demo project's MCP configuration. Replace every
`/ABSOLUTE/DEMO` placeholder with the printed directory, and the command with the
absolute path to your installed executable. Do not put credentials in this file.

```json
{
  "mcpServers": {
    "redisctl-demo": {
      "command": "/ABSOLUTE/PATH/redisctl-mcp",
      "args": [
        "--transport", "stdio",
        "--database-backend", "legacy",
        "--database-url", "redis+unix:///ABSOLUTE/DEMO/redis.sock",
        "--tools", "database:server,database:keys",
        "--policy", "/ABSOLUTE/DEMO/read-only.toml"
      ],
      "env": {
        "HOME": "/ABSOLUTE/DEMO/home",
        "XDG_CONFIG_HOME": "/ABSOLUTE/DEMO/config"
      }
    }
  }
}
```

The home and config paths apply only to this MCP child process, not your shell
or global client settings. The explicit socket selects the fixture without
using an existing profile. The explicit policy takes precedence over policy
auto-discovery and `--read-only`; do not add allow rules or change its tier.

Load the project server using your client's normal project-scoped flow. If the
client cannot isolate this configuration, stop here. Do not replace a working
personal server configuration or copy credentials into the conversation.

## Discover before calling tools

Start with this prompt:

> Use only the redisctl-demo server for this walkthrough. Inspect the callable
> tools and read `redisctl://skills`. If available, call `show_policy` and
> `list_available_tools`. Confirm that this server is read-only and has the
> database tools needed below. Do not call unavailable tools, change policy,
> enable writes or supply per-call `url` or `profile` overrides. Stop if these
> checks cannot be completed.

Confirm that `show_policy` reports the explicit `read-only.toml`, a `read-only`
tier and an empty allow list. Check for `redis_ping`, `redis_info`,
`redis_dbsize`, `redis_type`, `redis_ttl` and `redis_get` in the callable catalog.

`list_available_tools` describes preset visibility: its `active` list may include
tools the policy does not permit. Use the client's callable tool list and the
permission-filtered `available_tools` in `redisctl://skills` to determine what
can actually run. Visibility is not permission. If resources are unavailable in
your client, stop this walkthrough rather than guessing from a skill's text.

## Connect and identify the server

> Call `redis_ping`, then `redis_info` with `section` set to `server`. Report the
> observed response, Redis version and deployment mode. Do not infer installed
> modules or other reachable targets from the version number.

Expect a `PONG` response and standalone server information. The INFO input is:

```json
{"section": "server"}
```

If connection or authorization fails, stop and check the local fixture and
project configuration. Do not try another profile, port or backend automatically.

## Inspect a known small sample

> Call `redis_dbsize`, then `redis_type` for `demo:catalog:1`. Explain the scope
> of each result. Do not scan the database or claim a complete type distribution.

Expect `2` keys in this tiny standalone fixture and a `string` type for the named
key. The TYPE input is:

```json
{"key": "demo:catalog:1"}
```

> Call `redis_ttl` for `demo:session:1`, then `redis_get` for `demo:catalog:1`.
> Explain the remaining lifetime and quote only the synthetic catalog value.
> Do not read other values or inspect client, ACL or slow-command records.

Expect a positive TTL no greater than 1,800 seconds and `Synthetic headphones`.
These observations describe only the named keys. They do not establish database
health or a representative production sample.

Read-only does not mean zero cost or permission to disclose data. In particular,
the legacy `redis_scan` tool's `limit` caps returned keys, not the total work of
its internal iteration. Redis [SCAN](https://redis.io/docs/latest/commands/scan/)
uses incremental calls and treats `COUNT` as a hint. Keep this walkthrough on
known, small fixture keys instead of copying it into a production-wide scan.

## Explain the write boundary without crossing it

> Check the callable catalog and the skills index for `redis_set`. Explain why
> a request to change `demo:catalog:1` cannot be carried out in this session.
> Do not attempt the write, invoke a hidden tool, switch policy or suggest that
> this walkthrough needs writes enabled.

`redis_set` should be absent from the callable catalog and permission-filtered
skill index, even if the preset visibility inventory calls it active. This
demonstrates non-exposure, not a live write attempt or an audit-log rejection.
Read-only tools remain useful without granting the agent permission to modify
the fixture.

## Finish and choose a next step

Ask the agent to summarize the observed version, two known-key findings and
policy boundary, distinguishing facts from suggested next investigations.
Do not generate a whole-database inventory from these two samples.

Disconnect the project's MCP server, then stop the fixture with Ctrl-C in its
original terminal. The instance has no persisted dataset; the temporary folder
contains only demo configuration. No shared Redis instance needs cleanup.

For more context, read [MCP configuration](../configuration.md) and the
[skill catalog](../skills.md). Search, JSON, Cloud and Enterprise workflows need
their own available tools, target capabilities and authorization; this demo does
not enable them. A capability-aware guided tour and domain expert references
are separate follow-ups, not prerequisites for this walkthrough.
