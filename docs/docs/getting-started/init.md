# Project Onboarding: `redisctl init`

`redisctl init` onboards the current project to Redis and makes its AI coding agents
(Claude Code, Cursor, VS Code, Codex) Redis-fluent - one command from an empty
directory to a validated database with the project wired.

```bash
cd your-project
redisctl init --dry-run    # see the plan first
redisctl init              # apply it
```

## What it does

1. **Detects the project**: runtime (Node, Python, Go, Rust, Java), package manager,
   framework, and which agent tools are present.
2. **Provisions or discovers a database**: an existing `REDIS_URL` in `.env`, a
   container from an earlier run (restarted if stopped), or a fresh local Docker
   container - or point it at any database with `--url` (a pasted Redis Cloud
   connect command works verbatim).
3. **Wires the project**: `REDIS_URL` appended to `.env` (never clobbering an
   existing value), a committed `.env.example` placeholder, a `.gitignore` guard,
   the official Redis client for the detected runtime, and redis-cli when missing.
4. **Teaches the agents**: the official [redis/agent-skills](https://github.com/redis/agent-skills)
   via the standard skills CLI, a generated `redis-project-setup` skill carrying this
   project's specific facts, and a credential-free `redis` MCP server registration
   per agent config.
5. **Proves it works**: a live PING and SET/GET round trip.

Re-running is safe: every line reports `unchanged` the second time. Env values,
`.gitignore` entries, and skill files are never overwritten - existing values are
reported `kept`. The one deliberate exception: a `redis` entry in an agent's MCP
config that differs from the generated launcher is replaced (reported `updated`,
with the old command masked in the note); other MCP servers in the file survive
untouched.

## Options

| Flag | Meaning |
|---|---|
| `--url <redis-url>` | use an existing database instead of Docker (`rediss://` supported; accepts a pasted `redis-cli -u ...` command) |
| `--cloud` | take the database from Redis Cloud: connect to an existing database by `--name`, pick one interactively, or create one on the free Essentials plan |
| `--cloud-subscription <id>` | create in this Essentials subscription instead of the free plan (requires `--cloud`) |
| `--name <label>` | database name, recorded in the generated project skill; with `--cloud` it is also the reuse key |
| `--agent <name>` | configure specific agents (`claude`, `cursor`, `vscode`, `codex`, `all`; repeatable or comma-separated). Default: detect installed tools |
| `--no-install-cli` | skip installing redis-cli when it is missing |
| `--skills-repo <dir>` | copy skills from a local redis/agent-skills checkout (offline-safe) |
| `--skills-global` | install the official skills for your user instead of this project |
| `--defaults` | take the defaults instead of asking; piped stdin never prompts either |
| `--dry-run` | print the full plan, write nothing |
| `--no-telemetry` | do not send the anonymous usage event for this run |
| `--agent-memory <endpoint>` | wire Agent Memory (with `--store <id>`); the key stays a human-pasted `.env` placeholder |
| `--langcache <endpoint>` | wire LangCache (with `--cache <id>`) |
| `--context-retriever <endpoint>` | wire Context Retriever and bridge its MCP tools into the project agents |
| `--api-key <key>` | key for the one product being wired; its env var or an existing `.env` value wins over it |
| `--complete` | validate a product setup already present in `.env` (after filling the placeholders) |
| `--no-example` | skip the per-product example module |
| `--iris` | discovery-only: teach the agent to recommend the smallest Iris setup; adds no runtime |

## Credentials

`.env` is the only file that ever holds the connection string. The MCP configs and
the generated skill are credential-free and safe to commit: the MCP launcher sources
`.env` when the agent starts the server, and passwords are masked in all terminal
output.

## Exit codes

Standard `redisctl` [exit codes](../common/troubleshooting.md): 0 on success, 6 for
invalid input, 10 when the database cannot be reached (the message names the stale
`.env` value to fix), 1 for local environment problems such as Docker being
unavailable.

## Telemetry

Each run sends one anonymous usage event: which paths get used (flags as booleans,
outcome, duration) - never paths, names, URLs, or credentials. The device id is a
random UUID in `~/.cache/redisctl/id`, traceable to nothing. A notice prints on
first send. Opt out with `--no-telemetry`, `REDISCTL_INIT_TELEMETRY=0`, or
`DO_NOT_TRACK=1`. Dev builds carry no key and send nothing;
`REDISCTL_INIT_TELEMETRY_DEBUG=1` echoes exactly what would be shared.

## Iris products

Product flags wire Redis Iris services into the project: env blocks in `.env` and
`.env.example` (never clobbering existing keys), the SDK for the detected runtime,
an example module nothing imports, product facts in the generated skill, and - for
Context Retriever - a project MCP bridge that reads the agent key from `.env` at
launch. The API key crosses the secret boundary by hand: without one the run ends
successfully with "Action required", you paste the key into `.env` yourself, and
`redisctl init --complete` validates the full setup live. `--iris` installs only
the recommendation guidance so an agent can propose the smallest setup, which you
approve before anything is added.
