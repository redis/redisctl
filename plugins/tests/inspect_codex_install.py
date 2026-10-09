#!/usr/bin/env python3
"""Native Codex install/launch/uninstall in Docker only; no model or Redis calls."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

from smoke_stdio import RpcProcess

IMAGE = "redisctl-plugin-codex-test:0.160.1"
PLUGIN_ID = "redisctl-mcp@redisctl-experimental"
SKILLS = {"redisctl-mcp:data-explorer", "redisctl-mcp:enterprise-health-check",
          "redisctl-mcp:redisctl-setup"}


def docker_command(docker, image, plugins):
    # No host home, client config, Docker socket, credentials, or writable bind.
    # Default home belongs to this disposable container; do not override HOME.
    return [docker, "run", "--rm", "--pull", "never", "--platform", "linux/amd64",
            "--network", "none", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
            "--read-only", "--memory", "512m", "--pids-limit", "256",
            "--tmpfs", "/root:rw,size=128m", "--tmpfs", "/tmp:rw,exec,size=128m",
            "--tmpfs", "/fixture:rw,exec,size=128m",
            "--mount", f"type=bind,src={plugins},dst=/input,readonly",
            image, "python3", "/input/tests/inspect_codex_install.py", "--inside-container"]


def container_guard():
    if not Path("/.dockerenv").is_file():
        raise RuntimeError("Native installation requires the disposable Docker fixture")
    # Docker Desktop can expose dormant tunnel interfaces even with --network
    # none. Reject active non-loopback interfaces and any non-loopback route.
    active = {path.name for path in Path("/sys/class/net").iterdir()
              if (path / "flags").is_file() and int((path / "flags").read_text(), 16) & 1}
    ipv4 = Path("/proc/net/route").read_text().splitlines()[1:]
    ipv6 = Path("/proc/net/ipv6_route").read_text().splitlines()
    if active - {"lo"} or ipv4 or any(line.split()[-1] != "lo" for line in ipv6):
        raise RuntimeError("Fixture network must be disabled")
    mounts = {line.split()[1]: set(line.split()[3].split(","))
              for line in Path("/proc/mounts").read_text().splitlines()}
    if "ro" not in mounts.get("/input", set()) or "ro" not in mounts.get("/", set()):
        raise RuntimeError("Fixture input and root must be read-only")
    if Path("/root/.codex").exists() or Path("/root/.config/redisctl").exists():
        raise RuntimeError("Fixture must start without existing client or Redis state")


def snapshot(directory):
    return {str(path.relative_to(directory)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in directory.rglob("*") if path.is_file()}


def cli(*args):
    env = {key: os.environ[key] for key in ("PATH", "LANG") if key in os.environ}
    result = subprocess.run(["codex", *args], cwd="/fixture", env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=35)
    if result.returncode:
        raise RuntimeError(f"Native Codex command failed: {args[0]}")
    return result.stdout


def connect():
    env = {key: os.environ[key] for key in ("PATH", "LANG") if key in os.environ}
    rpc = RpcProcess(["codex", "app-server", "--stdio", "-c", "analytics.enabled=false"],
                     "/fixture", env)
    try:
        init = rpc.request("initialize", {"clientInfo": {"name": "redisctl-plugin-fixture", "version": "1"},
                                          "capabilities": {"experimentalApi": True}})
        assert init["codexHome"] == "/root/.codex"
        rpc.send("initialized", {})
        return rpc
    except BaseException:
        rpc.close()
        raise


def native_trial():
    container_guard()  # Fail before any client invocation or mutation.
    source = Path("/input/redisctl-mcp")
    original = snapshot(source)
    marketplace = Path("/fixture/marketplace with spaces")
    shutil.copytree("/input", marketplace)
    version = cli("--version").strip()
    assert version == "codex-cli 0.160.1"
    added = json.loads(cli("plugin", "marketplace", "add", str(marketplace), "--json"))
    assert added["marketplaceName"] == "redisctl-experimental"
    installed = json.loads(cli("plugin", "add", PLUGIN_ID, "--json"))
    cache = Path(installed["installedPath"])
    assert cache.is_relative_to(Path("/root/.codex/plugins/cache"))
    assert snapshot(cache) == original
    rpc = None
    try:
        rows = json.loads(cli("plugin", "list", "--json"))["installed"]
        assert len(rows) == 1 and rows[0]["pluginId"] == PLUGIN_ID and rows[0]["enabled"]
        rpc = connect()
        skills = rpc.request("skills/list", {"cwds": ["/fixture"], "forceReload": True})["data"][0]
        assert not skills["errors"]
        owned = [skill for skill in skills["skills"] if skill.get("pluginId") == PLUGIN_ID]
        assert {skill["name"] for skill in owned} == SKILLS
        assert all(skill["enabled"] and Path(skill["path"]).is_relative_to(cache) for skill in owned)
        # Starting an ephemeral thread initializes MCP; never send turn/start.
        thread = rpc.request("thread/start", {"cwd": "/fixture", "ephemeral": True})["thread"]["id"]
        status = rpc.request("mcpServerStatus/list", {"threadId": thread, "detail": "toolsAndAuthOnly"})
        assert not status.get("nextCursor")
        assert len(status["data"]) == 1
        server = status["data"][0]
        assert server["name"] == "redisctl" and server["pluginId"] == PLUGIN_ID
        assert server["runtimeStatus"] == "connected"
        tools = server["tools"]
        assert {"show_policy", "profile_list", "redis_ping", "get_license_usage"} <= tools.keys()
        assert not {"profile_create", "profile_delete", "redis_set", "redis_command",
                    "cloud_raw_api", "enterprise_raw_api", "profile_show"} & tools.keys()
        assert all(tool.get("annotations", {}).get("readOnlyHint") is True for tool in tools.values())
        result = rpc.request("mcpServer/tool/call", {"threadId": thread, "server": server["name"],
                                                    "tool": "show_policy", "arguments": {}})
        policy = json.loads(result["content"][0]["text"])
        assert policy["global_tier"] == "read-only"
        assert policy["source"] == f"file: {cache}/read-only.toml"
        denied = rpc.request("mcpServer/tool/call", {"threadId": thread, "server": server["name"],
                                                    "tool": "profile_create", "arguments": {}}, allow_error=True)
        assert "error" in denied or denied.get("isError") is True
        server_version = server["serverInfo"]["version"]
        tool_count = len(tools)
    finally:
        if rpc is not None:
            rpc.close()
        cli("plugin", "remove", PLUGIN_ID, "--json")
    assert json.loads(cli("plugin", "list", "--json"))["installed"] == []
    assert not cache.exists()
    rpc = connect()
    try:
        thread = rpc.request("thread/start", {"cwd": "/fixture", "ephemeral": True})["thread"]["id"]
        assert rpc.request("mcpServerStatus/list", {"threadId": thread})["data"] == []
        rows = rpc.request("skills/list", {"cwds": ["/fixture"], "forceReload": True})["data"]
        assert all(skill.get("pluginId") != PLUGIN_ID for row in rows for skill in row["skills"])
    finally:
        rpc.close()
    assert snapshot(source) == original and snapshot(marketplace / "redisctl-mcp") == original
    print(json.dumps({"client": version, "platform": "Linux x86_64 container",
                      "server_release": server_version, "native_skills": len(owned),
                      "read_only_tools": tool_count, "cached_policy_used": True,
                      "uninstalled_cache_and_runtime": True, "source_preserved": True,
                      "model_requests": 0, "redis_connections": 0}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default=IMAGE)
    parser.add_argument("--inside-container", action="store_true", help=argparse.SUPPRESS)
    options = parser.parse_args()
    if options.inside_container:
        native_trial()
    else:
        docker = shutil.which("docker")
        if docker is None:
            raise RuntimeError("Docker is required; no host-install fallback")
        plugins = Path(__file__).resolve().parents[1]
        env = {key: os.environ[key] for key in ("PATH", "LANG", "SystemRoot") if key in os.environ}
        subprocess.run(docker_command(docker, options.image, plugins), env=env, check=True, timeout=180)


if __name__ == "__main__":
    main()
