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
| `--name <label>` | database name, recorded in the generated project skill |
| `--agent <name>` | configure specific agents (`claude`, `cursor`, `vscode`, `codex`, `all`; repeatable or comma-separated). Default: detect installed tools |
| `--no-install-cli` | skip installing redis-cli when it is missing |
| `--skills-repo <dir>` | copy skills from a local redis/agent-skills checkout (offline-safe) |
| `--skills-global` | install the official skills for your user instead of this project |
| `--dry-run` | print the full plan, write nothing |

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
