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
2. **Provisions or discovers a database**: on a terminal the wizard asks where it
   comes from - Redis Cloud (the default; signs you in when needed), a local Redis
   on `localhost:6379` or a new Docker container, a pasted connection string, or
   "skip for now" (a `.env` placeholder to fill in later). When `.env` already holds
   a `REDIS_URL`, keeping it is the first option and the default, and "skip for now"
   is not offered. Without a terminal, or
   with `--defaults`, it keeps an existing `REDIS_URL` in `.env`, else reuses this
   project's container (restarted if stopped), else creates a local Docker
   container. `--url` (a pasted Redis Cloud connect command works verbatim) and
   `--cloud` answer the question up front.
3. **Wires the project**: `REDIS_URL` written to `.env` (an explicit database
   choice replaces an existing value), a committed `.env.example` placeholder, a `.gitignore` guard,
   the official Redis client for the detected runtime, and redis-cli when missing.
4. **Teaches the agents**: the official [redis/agent-skills](https://github.com/redis/agent-skills)
   via the standard skills CLI, a generated `redis-project-setup` skill carrying this
   project's specific facts, and a credential-free `redis` MCP server registration
   per agent config.
5. **Proves it works**: a live PING and SET/GET round trip.

Re-running with the same choices preserves the existing setup. An explicit database
choice replaces `REDIS_URL`; other env values are kept, including placeholders
that need filling. A `redis` entry in an agent's MCP
config that differs from the generated launcher is replaced (reported `updated`);
other MCP servers in the file survive
untouched.

## Options

| Flag | Meaning |
|---|---|
| `--url <redis-url>` | use an existing database instead of Docker (`rediss://` supported; accepts a pasted `redis-cli -u ...` command) |
| `--cloud` | take the database from Redis Cloud: connect to an existing database by `--name`, pick one interactively, or create one on the free Essentials plan. An account that already has databases needs `--name` when nothing can prompt (piped or `--defaults`). Not signed in yet? On a terminal it opens the browser to sign in first |
| `--cloud-subscription <id>` | create in this Essentials subscription instead of the free plan (requires `--cloud`) |
| `--name <label>` | database name: with `--cloud` it is the reuse key and the name of a new database; a database outside Docker is named by it in the generated project skill |
| `--agent <name>` | configure specific agents (`claude`, `cursor`, `vscode`, `codex`, `all`; repeatable or comma-separated). Default: on a terminal the wizard asks; otherwise the detected tools, or all four when none is detected |
| `--no-install-cli` | skip installing redis-cli when it is missing |
| `--skills-repo <dir>` | copy skills from a local redis/agent-skills checkout (offline-safe) |
| `--skills-global` | install the official skills for your user instead of this project |
| `--no-skills` | skip the official skills; the generated `redis-project-setup` skill is still written. The wizard offers the same as "Skip" |
| `--defaults` | take the defaults instead of asking: with no database flag that is a local Docker container. With `--cloud` and no Cloud sign-in, a terminal still opens the browser to sign in; piped stdin never prompts |
| `--dry-run` | print the full plan, write nothing |
| `--no-telemetry` | do not send the anonymous usage event for this run |
| `--agent-memory <endpoint>` | wire Agent Memory (with `--store <id>`); without `--api-key` or an exported `AGENT_MEMORY_API_KEY`, the key stays a `.env` placeholder to paste by hand |
| `--langcache <endpoint>` | wire LangCache (with `--cache <id>`) |
| `--context-retriever <endpoint>` | wire Context Retriever and bridge its MCP tools into the project agents |
| `--api-key <key>` | key for the one product being wired. A real key already in `.env` wins over it, and so does the product's own env var when `.env` has none; a placeholder in `.env` gives way to the flag |
| `--complete` | validate a product setup already present in `.env` (after filling the placeholders) |
| `--no-example` | skip the per-product example module |
| `--iris` | discovery-only: teach the agent to recommend the smallest Iris setup; adds no runtime |

## Credentials

`.env` is the only file that ever holds the connection string. The MCP configs and
the generated skill are credential-free and safe to commit: the MCP launcher reads
`REDIS_URL` from `.env` when the agent starts the server (without executing the
file) and hands the server the password through its environment, never on a command
line other users can see. Passwords are masked in all terminal output. A new `.env` is readable
only by you, `.gitignore` covers it before it is written, and a `.env` that git
already tracks is refused rather than handed a secret.

## Exit codes

Built on the standard `redisctl` [exit codes](../reference/agent-error-codes.md#exit-codes):

| Code | Meaning |
|---|---|
| 0 | success, including `--dry-run` and a run that ends with "Action required" |
| 1 | the run cannot finish: Docker unavailable or a `docker` command failed, no free port, a project file cannot be read or written, `.env` tracked by git while init would write a secret into it, a product key rejected, `--cloud` with no sign-in and nothing to prompt on, the OS keyring cannot store a new sign-in, an account with databases and no `--name` when nothing can prompt, a new Cloud database that never gets an endpoint |
| 2 | usage: an unknown flag or argument, `--cloud` together with `--url` or a pasted URL, `--cloud-subscription` without `--cloud` |
| 3 | the redisctl configuration cannot be used, for example `-p` names a profile that is not a Redis Cloud one |
| 4 | Redis Cloud rejected the API keys |
| 5 | Redis Cloud has no such resource |
| 6 | invalid input: no `redis://` URL in `--url`, a product flag missing its companion, `--complete` with nothing to complete, an invalid Cloud database name |
| 7 | Redis Cloud conflict: a paid `redisctl-<name>` subscription, the free plan already used up, a quota reached |
| 8 | any other Redis Cloud API or server error |
| 9 | Redis Cloud rate limit |
| 10 | the database cannot be reached (for a stale `.env` value, the message names it) |
| 12 | cancelled at a wizard question or the Cloud picker |

A failure inside the browser sign-in itself comes from `redisctl cloud auth login`
and keeps that command's exit codes.

## Telemetry

When a telemetry key is configured, each run sends one anonymous usage event: which paths get used (flags as booleans,
outcome, duration) - never paths, names, URLs, or credentials. The device id is a
random UUID in `~/.cache/redisctl/id`: it links the runs from one machine and nothing
else. A notice prints on
first send. Opt out with `--no-telemetry`, `REDISCTL_INIT_TELEMETRY=0`, or
`DO_NOT_TRACK=1`. The Amplitude key is read from `REDISCTL_INIT_AMPLITUDE_KEY` at
runtime, or from the build environment at compile time; a build without one sends
nothing. `REDISCTL_INIT_TELEMETRY_DEBUG=1` prints the event body and send outcome to
stderr, or that nothing was shared when no key is set.

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
