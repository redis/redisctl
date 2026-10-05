---
name: redisctl-setup
description: Set up Redis access after connecting an agent - choose a deployment, configure credentials locally, validate connectivity, and run a first read-only operation
---

Guide a first-time redisctl user from a connected MCP client to working Redis access. This skill is available before any profile exists. Keep passwords and API keys out of chat, tool arguments, and logs.

## 1. Inspect the connected server

Read `redisctl://skills` for the toolsets and tools actually available. Call `show_policy` and `list_available_tools` when listed as available. If app/profile tools are available, use `profile_list` and `profile_path` to identify existing configuration. An existing profile may already satisfy the request; do not replace or delete it. Do not invoke unavailable tools or relax policy to complete setup.

## 2. Identify the user's goal

Ask which workflow they need:

- Cloud infrastructure: subscriptions, databases, networking. Requires Cloud API credentials.
- Enterprise infrastructure: cluster and database administration. Requires an Enterprise REST API endpoint and account.
- Database access: keys, data and diagnostics on any Redis instance, including Cloud-hosted databases. Requires Redis database credentials, not Cloud API keys.

Gather only non-secret details: profile name, hostname, port, username, TLS/CA requirements, network/VPN access, and whether the endpoint is Redis Cluster. Prefer a test environment for initial exploration.

## 3. Configure credentials locally

Do not ask the user to paste a password, token, API key, or credential-bearing URL into the conversation. Do not call `profile_create` with raw credentials. Do not read configuration files or environment values through the agent's shell, even when they might contain only credential references.

When an agent has terminal access, it can guide or open the user's local credential-entry workflow. The user must enter secrets directly into their own terminal or trusted credential UI, not an agent-visible terminal session. The supported fallback for clients without secure interactive input is for the user to run:

```sh
redisctl profile init
```

The wizard prompts for deployment details and credentials and offers connectivity testing. The agent handles the surrounding explanation and diagnostics. Prefer OS keyring storage when the local setup offers it. Confirm the saved profile name without requesting secret values.

This fallback still needs a local user action: current MCP profile creation accepts secrets as arguments and is hidden by the default read-only policy. Do not enable full Redis access to work around that restriction.

## 4. Validate and refresh

Use `profile_validate` with `connect=true` when available. If app tools are not loaded, guide the user to run `redisctl profile validate --connect` locally and share only the non-secret status/error summary.

Diagnose connection failures in order: endpoint/port, network/VPN/firewall, TLS/CA, username, credential source, then permissions. Retain TLS verification; do not silently suggest insecure mode. Database profiles enable TLS by default; plaintext development instances require an explicit choice. An Enterprise REST endpoint (usually port 9443) is different from a Redis database endpoint.

Profiles and connection caches are loaded when the MCP process starts. After creating or changing a profile, restart the MCP connection using the client's controls. Ask for user action when the agent cannot restart it. Select the appropriate profile and toolset in the MCP launch config and include app tools for setup/diagnostics, for example:

```json
{
  "command": "/absolute/path/to/redisctl-mcp",
  "args": ["--profile", "mydb", "--tools", "database,app"]
}
```

For Redis Cluster, include `--cluster` at startup. For multiple targets, use clearly named server configurations. Do not claim a startup-selected target can be changed by merely editing a profile.

## 5. Prove a first read-only call

Check `show_policy` again when available and invoke one tool available in this session:

- Database: `redis_ping`, without reading values. Optionally use `redis_dbsize` only when available and supported for the selected deployment.
- Cloud: `get_account` or `list_subscriptions`.
- Enterprise: `get_cluster`.

Report the target/profile and the successful operation, without credentials or raw sensitive output. Then offer a relevant workflow from the skill index. Writes or destructive operations require a separate user request and appropriately configured policy; this setup workflow does not grant them.
