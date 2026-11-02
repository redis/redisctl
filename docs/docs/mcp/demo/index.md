# Redis agent demos

See how an agent uses Redis tools, explains the results and respects the active
policy. Start with [Connect and explore](connect-and-explore.md): a short
walkthrough using two synthetic keys in a disposable local Redis instance.
It needs no Cloud account or customer data.

The walkthrough covers connection checks, a small data sample and the safety
boundary between visible and callable tools. The agent reads the fixture; you
prepare it locally before the conversation. Keep the same fixture and prompts
when demonstrating the workflow to someone else.

You need an installed `redisctl-mcp`, `redis-server`, `redis-cli` and an MCP client
that supports project-local server configuration. Follow the
[MCP getting started guide](../getting-started.md) for installation and client
prerequisites. The local socket recipe supports macOS and Linux.

For an existing, authorized Redis target instead of a disposable demo, use the
[database quickstart](../database-quickstart.md). Do not point this fixture recipe
at an existing database or copy real records into it.
