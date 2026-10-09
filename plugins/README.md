# redisctl-mcp plugin compatibility spike

Experimental work for [#1202](https://github.com/redis/redisctl/issues/1202).
This package wraps the existing Rust stdio server; it does not install binaries,
store credentials, provision Redis, or change the database backend/default.
It has not been published to a public plugin directory.

## What is packaged

- Portable `redisctl-mcp/plugin.json` and `mcp.json` for local Codex.
- Claude Code adapter: `.claude-plugin/plugin.json` and `.mcp.json`.
- An explicit read-only policy, with raw passthrough and credential display denied.
- Three native skills generated from `crates/redisctl-mcp/skills/`: setup,
  Enterprise health check, and data exploration. Their source bodies stay shared;
  a generated preamble adds plugin discovery, scope, and safety guidance.
- Separate experimental Claude and Codex marketplace catalogs under this directory.

There are no hooks, installers, agents, or background processes. The MCP process
is launched only by an enabled client connection. Native skills help clients
discover workflows; the server's effective policy, not skill text, enforces tool
access. A health-check GO result is not approval to change infrastructure.

## Prerequisites

Install `redisctl-mcp` through a trusted local flow and ensure the client can find
it on PATH. Use a build containing the embedded skill work from main for the full
setup/resource journey. The published `redisctl-mcp-v0.12.1` predates that work;
its identical version string is not proof that it contains the new feature.
The compatibility evidence below uses a build from source, not that release.

For new profiles, the companion `redisctl` CLI must also be installed. The user
enters credentials via `redisctl profile init` in their own terminal, never in
chat, a model-visible terminal, plugin files, or tool arguments. Use existing
profiles where possible, and restart the MCP connection after profile changes.
Secure-storage/helper improvements remain tracked separately in #1173.

The spike uses the existing default target selection. A user can explicitly set
the non-secret `REDISCTL_PROFILE` when starting their client. Inspect the target
and policy before operations. Do not enable the plugin alongside a manual
registration for the same target without reviewing possible duplicate servers.
Automatic migration/deduplication and per-client target selection UI are later
epic work, not part of this package.

## Claude Code: session-only trial

From the repository root, after reviewing the package:

```sh
claude plugin validate ./plugins/redisctl-mcp --strict
claude --plugin-dir ./plugins/redisctl-mcp plugin details redisctl-mcp
claude --plugin-dir ./plugins/redisctl-mcp
```

In that session, inspect `/mcp` and invoke `/redisctl-mcp:redisctl-setup`.
The other native skills are `/redisctl-mcp:enterprise-health-check` and
`/redisctl-mcp:data-explorer`. Do not invoke an operational skill until the user
has approved its target and read scope. Ending the session removes the inline
plugin; it does not delete profiles or credentials.

The optional Claude marketplace root is `./plugins`, with its catalog at
`plugins/.claude-plugin/marketplace.json`. Adding/installing from that marketplace
is a deliberate user action and has not been exercised as part of this spike.

## Local Codex: repository marketplace

The Codex marketplace root is also `./plugins`, not the repository root. Its
catalog is `plugins/.agents/plugins/marketplace.json`, and `source.path` resolves
relative to `plugins`. From the repository root, a user can deliberately add it:

```sh
codex plugin marketplace add ./plugins
codex plugin list --marketplace redisctl-experimental --available --json
codex plugin add redisctl-mcp@redisctl-experimental
```

These commands change the user's plugin configuration; the automated native
inspection below does not run them. Inspect `/mcp` in a new local session and
run the namespaced setup skill. Disable/uninstall using the client's plugin
controls; preserve existing Redis profiles and credentials. The spike has not
validated installed-cache relocation, upgrade, uninstall or Desktop/IDE UI.

This local stdio package is not a hosted ChatGPT plugin. Current OpenAI public
submission requires a remote HTTPS MCP endpoint or separately arranged local
support. Do not expose redisctl-mcp's unauthenticated HTTP listener to satisfy
that requirement. Hosted authentication and public distribution need separate
decisions.

## Regenerate and test

Python 3.10+ is used only for developer generation/tests, not to launch the
plugin's Rust server. Generated files are committed so consumers need no Python.

```sh
python3 plugins/build_plugin.py
python3 plugins/build_plugin.py --check
python3 -m unittest discover -s plugins/tests -p 'test_*.py' -v
cargo test -p redisctl-mcp --test plugin_package
```

Native metadata inspection does not install the plugin or run a model:

```sh
claude plugin validate ./plugins/redisctl-mcp --strict --json
claude plugin validate ./plugins/.claude-plugin/marketplace.json --strict --json
claude --plugin-dir ./plugins/redisctl-mcp plugin details redisctl-mcp
python3 plugins/tests/inspect_codex.py
```

To exercise the Rust protocol from both generated configurations:

```sh
cargo build -p redisctl-mcp
python3 plugins/tests/smoke_stdio.py --binary target/debug/redisctl-mcp --client claude
python3 plugins/tests/smoke_stdio.py --binary target/debug/redisctl-mcp --client codex
```

The harness relocates the package to a temporary path containing spaces, clears
credential/target environment values, isolates config, and calls only protocol
discovery, policy inspection, and an unavailable profile-creation tool with no
credentials. On macOS, sandbox-exec blocks both operator config locations and
all network access; it must not fall back to unsandboxed execution. Linux uses
a disposable XDG config root; Windows uses disposable APPDATA/LOCALAPPDATA.
Only macOS has local execution evidence so far. Neither harness connects to Redis
or exercises user credential storage. Inspect-Codex reads only package metadata
through app-server; it does not start a conversation or MCP connection.

## Evidence and limitations

Initial checkpoint, 2026-10-09, macOS Apple Silicon:

| Check | Result |
| --- | --- |
| Claude Code 2.1.294 strict manifest + marketplace validation | Pass |
| Claude inline component inventory | Three native skills, one MCP server, no hooks/agents |
| Codex CLI 0.160.1 local `plugin/read` | Three `redisctl-mcp:`-namespaced skills and `redisctl` server discovered |
| Rust main-based stdio smoke, both generated configs | 69 read-only tools, 11 embedded skills, profile creation rejected |
| Package moved to a directory containing spaces | Pass in both protocol smokes |
| Hermetic generation/adapter regression suite | Pass |
| Pure Rust package policy regression | Pass |

Native inventory and direct protocol launch are separate evidence: these checks
do **not** prove either client launched the MCP server from an installed plugin,
executed a skill through a model, or completed onboarding against a real target.
Fresh client installation, secure setup, real approved connectivity, namespace
tool invocation, missing-binary UX, installed-cache lifecycle, Linux/Windows,
independent review and current-head CI remain incomplete. Keep the PR draft.

## Sources

- [OpenAI plugin packaging](https://developers.openai.com/plugins/build/plugins)
- [Codex MCP configuration](https://developers.openai.com/codex/mcp)
- [Claude plugin components](https://code.claude.com/docs/en/plugins/components)
- [Agent Plugins schemas](https://agent-plugins.org/schemas/1.0.0/plugin.schema.json)
