# Embedded Skills

redisctl-mcp embeds its workflow library in the binary and Rust library. No
separate skill download or directory beside the executable is needed. Skills
provide instructions, not additional permissions: tool selection, visibility
presets, and the active safety policy still apply.

## Discover and use a skill

After connecting your agent, ask it to read the `redisctl://skills` MCP resource.
This JSON index contains:

- `schema_version`: the index format version (currently `1`).
- `enabled_toolsets` and `available_tools`: the startup selection and the tools
  actually available under the current visibility and policy settings.
- `skills`: sorted entries with `name`, `description`, `uri`, `prompt`, and
  `source` (`bundled` or `custom`).

Read an entry's URI, for example `redisctl://skills/redisctl-setup`, to retrieve its
complete `SKILL.md` as `text/markdown`. Each skill is also available as an MCP
prompt of the same name. Both entry points use the same workflow; prompts add
a reminder to check tool availability and policy before proceeding.

Resource support varies by client. Some clients require you to select or attach
a resource yourself. If your agent cannot read resources, select the
`redisctl-setup` prompt through your client's prompt controls. Merely connecting
the server does not guarantee the client will automatically load a workflow.

The library includes setup, data exploration and modeling, Search index advice,
query tuning, index audits and migrations, Cloud provisioning, and Enterprise
health checks. The entire catalog stays discoverable even when its tools are
not loaded; the index lets the agent recognize that a workflow is unavailable
without attempting denied operations.

Initialization contains compact pointers rather than a tool catalog or complete
workflow bodies. Read only the relevant skill, on demand. This does not hide
tool schemas: clients still discover the selected, policy-filtered tools through
`tools/list`. Use `--tools` or visibility presets to reduce that separate surface.

## Resource namespace

`redisctl://` identifies redisctl metadata and guidance, not a Redis database
connection. With app tools loaded, the server exposes `redisctl://config/path`,
`redisctl://profiles`, and `redisctl://help`. The profile resource contains names
and defaults, not credential values; configuration errors use a non-secret summary.
The previously published `redis://config/path`, `redis://profiles`, and
`redis://help` addresses remain compatible aliases. Existing Redis connection
URLs continue to use `redis://` or `rediss://`.

## First-run setup

Ask your agent:

> Read redisctl://skills/redisctl-setup and help me configure Redis access safely.

The workflow distinguishes Cloud management API access, Enterprise REST API
access, and direct database access. It inspects existing profiles when app tools
are available, guides local credential entry, validates connectivity, and ends
with a read-only operation. It never asks for secrets in chat or requests full
Redis permissions merely to configure a profile.

!!! note "Current onboarding boundary"
    Credential entry still requires a trusted local user flow, such as
    `redisctl profile init` in the user's own terminal. Current `profile_create`
    arguments are not a secure interactive credential-entry mechanism. After
    creating or changing a profile, restart the MCP connection so its startup
    configuration and connection caches refresh. Fully agent-led secure profile
    creation and in-process refresh are tracked in
    [#1173](https://github.com/redis/redisctl/issues/1173).

Include `app` when explicitly selecting toolsets to keep profile diagnostics
available, for example `--tools database,app`. Setup resources and prompts
remain available even if app tools are disabled by policy.

## Custom skills

Use `--skills-dir /absolute/path/to/skills` or `REDISCTL_MCP_SKILLS_DIR` to load
custom skills at startup. Rust embedders use
`McpServerBuilder::with_skills_dir(Some(path))`.

The directory layout is:

```text
skills/
  my-workflow/
    SKILL.md
```

Each file needs frontmatter with a lowercase, hyphenated `name`, a nonempty
`description`, and a Markdown body. Names allow lowercase ASCII letters,
digits, and single hyphens, up to 64 characters, with no leading or trailing
hyphen. Custom skills replace bundled skills with the same name in both
resources and prompts; other bundled skills remain available. New names extend
the catalog. Only immediate child directories containing `SKILL.md` are read.

An explicitly configured missing/unreadable directory, malformed skill, or
duplicate custom name fails startup instead of silently falling back. Custom
skill content is trusted operator configuration; review it before loading.
Skills cannot bypass policy restrictions.

Keep `name` and `description` on single frontmatter lines. For example:

```markdown
---
name: my-workflow
description: Validate an existing Redis profile before using it
---

Read redisctl://skills and check tool availability and policy first.
If profile_validate is available, use it with connect=true.
Never request credentials in chat or relax policy to run this workflow.
```

Reference the exact MCP tool names rather than CLI command names for tool calls.
State prerequisites, require approval for writes, and explain unavailable-tool
fallbacks. The repository's catalog tests validate bundled metadata and tool
references; custom skills are operator-reviewed guidance, not executable policy.

Previously, the binary could discover a `skills` directory beside itself. That
implicit lookup is replaced by embedded defaults. Set `--skills-dir` explicitly
to retain local customizations. Changes to that directory require a restart.
