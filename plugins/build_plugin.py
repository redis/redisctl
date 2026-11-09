#!/usr/bin/env python3
"""Generate the experimental local plugin from canonical MCP skills (stdlib only)."""

import argparse
import json
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "plugins" / "redisctl-mcp"
SKILLS = ("redisctl-setup", "enterprise-health-check", "data-explorer")
POLICY = '''# Experimental plugin: read-only even if the user's default policy permits writes.
tier = "read-only"
deny_categories = ["destructive"]
deny = ["cloud_raw_api", "enterprise_raw_api", "redis_command", "profile_show", "redis_config_get", "get_enterprise_database"]

[tools]
preset = "essentials"
include = ["get_license_usage", "redis_memory_usage", "redis_json_get", "redis_smembers", "redis_zrange", "redis_lrange", "redis_xrange", "redis_hotkeys"]
'''

GUIDANCE = '''## Plugin safety and discovery

This is an experimental local plugin using an already-installed Rust redisctl-mcp.
Check the plugin server's available tools and call `show_policy` before this
workflow. Map the tool names below to the matching tools exposed by this plugin's
server; clients can namespace them. Do not use a similarly named tool from another
server or silently switch targets. If a tool is unavailable, explain the limitation
and stop that step; do not enable writes or bypass policy.

Use only the target and data scope the user approved. Read-only is not permission
to disclose data: do not sample values or report sensitive identifiers unless the
user requested that scope. Never request credentials in chat or tool arguments,
read credential files/environment values, or provision resources during setup.
For a new profile, guide the user through `redisctl profile init` in their own
terminal, then restart this MCP connection. The companion CLI is a prerequisite
for that flow. Installed plugin skills remain available if the server lacks MCP
resource/prompt support; do not claim the embedded skill index exists on an older
binary. Operational health results do not authorize subsequent changes.

'''

# Keep canonical setup guidance shared except for this explicit plugin-specific
# launch adaptation. Fail closed if the canonical section changes shape.
SETUP_LAUNCH = '''Profiles and connection caches are loaded when the MCP process starts. After creating or changing a profile, restart the MCP connection using the client's controls. Ask for user action when the agent cannot restart it. Select the appropriate profile and toolset in the MCP launch config and include app tools for setup/diagnostics, for example:

```json
{
  "command": "/absolute/path/to/redisctl-mcp",
  "args": ["--profile", "mydb", "--tools", "database,app"]
}
```

For Redis Cluster, include `--cluster` at startup. For multiple targets, use clearly named server configurations. Do not claim a startup-selected target can be changed by merely editing a profile.'''

PLUGIN_SETUP_LAUNCH = '''Profiles and connection caches are loaded when the MCP process starts. After creating or changing a profile, restart this plugin's MCP connection using the client's controls. Ask for user action when the agent cannot restart it.

Select an existing profile through the non-secret `REDISCTL_PROFILE` in the MCP server's environment, using the client's documented controls where supported, then restart the plugin connection. Do not assume a variable exported to the client was forwarded to its MCP server. Confirm the selected target through available non-secret metadata before operations; if it cannot be confirmed, stop. Do not read environment values through the agent. Do not edit the plugin's cached `mcp.json`/`.mcp.json`, replace its launch arguments or policy, or add a second manual MCP registration to complete this workflow. Keep the packaged toolset and explicit read-only policy intact.

This spike does not offer per-plugin target configuration or a Redis Cluster startup switch. If the requested target needs unsupported launch options, explain that limitation and stop; do not silently switch targets or bypass the packaged policy. Do not claim editing a profile changes an already-running connection.'''


def skill_body(name, body):
    if name == "redisctl-setup":
        if body.count(SETUP_LAUNCH) != 1:
            raise ValueError("Canonical setup launch guidance changed; review the plugin adaptation")
        body = body.replace(SETUP_LAUNCH, PLUGIN_SETUP_LAUNCH)
    return body.lstrip("\n")


def workspace_version(root):
    with (root / "Cargo.toml").open("rb") as source:
        version = tomllib.load(source).get("workspace", {}).get("package", {}).get("version")
    if not isinstance(version, str) or not version:
        raise ValueError("Workspace package version must be a non-empty string")
    return version


def unexpected_skill_entries(package):
    directory = package / "skills"
    if directory.is_symlink() or (directory.exists() and not directory.is_dir()):
        return [directory]
    allowed_files = {f"skills/{name}/SKILL.md" for name in SKILLS}
    allowed_directories = {f"skills/{name}" for name in SKILLS}
    return sorted(path for path in directory.rglob("*")
                  if path.is_symlink() or str(path.relative_to(package)).replace("\\", "/") not in
                  (allowed_directories if path.is_dir() else allowed_files))


def encode(value):
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


def server(root_variable):
    return {
        "type": "stdio",
        "command": "redisctl-mcp",
        "args": [
            "--transport", "stdio", "--read-only=true", "--policy",
            f"${{{root_variable}}}/read-only.toml",
            "--tools", "cloud,enterprise,database,app",
        ],
    }


def generated_files(root=ROOT):
    identity = {
        "name": "redisctl-mcp",
        "version": workspace_version(root),
        "description": "Experimental local Redis management plugin using the Rust redisctl-mcp server.",
        "author": {"name": "Redis", "url": "https://github.com/redis/redisctl"},
        "homepage": "https://redis.github.io/redisctl/mcp/",
        "repository": "https://github.com/redis/redisctl",
        "license": "MIT OR Apache-2.0",
    }
    files = {
        "plugin.json": encode({
            "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
            **identity,
            "extensions": {"com.openai": {
                "onboardingSkill": "./skills/redisctl-setup/SKILL.md",
                "interface": {"displayName": "redisctl MCP (experimental)",
                              "shortDescription": "Read-only Redis workflows with local profiles"},
            }},
        }),
        "mcp.json": encode({
            "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
            "mcpServers": {"redisctl": server("PLUGIN_ROOT")},
        }),
        ".claude-plugin/plugin.json": encode(identity),
        ".mcp.json": encode({"mcpServers": {"redisctl": server("CLAUDE_PLUGIN_ROOT")}}),
        "read-only.toml": POLICY,
    }
    for name in SKILLS:
        source = root / "crates" / "redisctl-mcp" / "skills" / name / "SKILL.md"
        markdown = source.read_text(encoding="utf-8")
        if not markdown.startswith("---\n") or "\n---\n" not in markdown[4:]:
            raise ValueError(f"Invalid skill frontmatter: {name}")
        header, body = markdown[4:].split("\n---\n", 1)
        if f"name: {name}" not in header.splitlines():
            raise ValueError(f"Skill identity mismatch: {name}")
        files[f"skills/{name}/SKILL.md"] = (
            "---\n" + header + "\n---\n\n"
            + f"<!-- Generated by plugins/build_plugin.py from crates/redisctl-mcp/skills/{name}/SKILL.md. -->\n\n"
            + GUIDANCE + skill_body(name, body)
        )
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if tracked generated files drift")
    args = parser.parse_args()
    drift = [str(path.relative_to(ROOT)) for path in unexpected_skill_entries(PACKAGE)]
    targets = {PACKAGE / relative: expected for relative, expected in generated_files().items()}
    # Keep the Rust regression self-contained when Cargo packages the crate.
    targets[ROOT / "crates/redisctl-mcp/tests/fixtures/plugin-read-only.toml"] = POLICY
    if drift and not args.check:
        parser.exit(1, "Unexpected plugin skill entries; reconcile them explicitly: " + ", ".join(drift) + "\n")
    for target, expected in targets.items():
        if args.check:
            if not target.is_file() or target.read_text(encoding="utf-8") != expected:
                drift.append(str(target.relative_to(ROOT)))
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(expected, encoding="utf-8")
    if drift:
        parser.exit(1, "Plugin generation drift: " + ", ".join(drift) + "\n")
    print("Plugin generated files are current" if args.check else "Generated experimental redisctl-mcp plugin")


if __name__ == "__main__":
    main()
