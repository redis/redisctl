#!/usr/bin/env python3
"""No-credential MCP protocol smoke. macOS requires sandbox-exec; no unsafe fallback."""

import argparse
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time


class RpcProcess:
    def __init__(self, command, cwd, env):
        self.process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        text=True, bufsize=1)
        self.lines = queue.Queue()
        self.identifier = 0
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.process.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def send(self, method, params, identifier=None):
        payload = {"jsonrpc": "2.0", "method": method, "params": params}
        if identifier is not None:
            payload["id"] = identifier
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()

    def request(self, method, params, allow_error=False):
        self.identifier += 1
        identifier = self.identifier
        self.send(method, params, identifier)
        deadline = time.monotonic() + 20
        while True:
            line = self.lines.get(timeout=max(0.01, deadline - time.monotonic()))
            if line is None:
                raise RuntimeError("Protocol process ended before responding")
            message = json.loads(line)  # stdout must contain only JSON-RPC
            if message.get("id") == identifier:
                if "error" in message:
                    if allow_error:
                        return {"error": message["error"]}
                    raise RuntimeError(f"Protocol request failed: {method}")
                return message["result"]
            if time.monotonic() >= deadline:
                raise TimeoutError(f"Protocol request timed out: {method}")

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=5)
        self.process.stdout.close()


def isolated_command(binary, args, directory):
    env = {key: os.environ[key] for key in ("PATH", "SystemRoot", "WINDIR", "TMPDIR", "LANG") if key in os.environ}
    env.update({"XDG_CONFIG_HOME": str(directory), "APPDATA": str(directory), "LOCALAPPDATA": str(directory)})
    command = [str(binary), *args]
    if sys.platform == "darwin":
        # directories uses the OS home on macOS, not XDG_CONFIG_HOME. Block both
        # config locations and all network access instead of pretending isolation.
        paths = [Path.home() / ".config/redisctl", Path.home() / "Library/Application Support/com.redis.redisctl"]
        denied = " ".join(f"(subpath {json.dumps(str(path))})" for path in paths)
        profile = f"(version 1) (allow default) (deny network*) (deny file-read* {denied})"
        command = ["/usr/bin/sandbox-exec", "-p", profile, *command]
    elif not (sys.platform.startswith("linux") or sys.platform == "win32"):
        raise RuntimeError("No verified config isolation for this platform")
    return command, env


def smoke(binary, package, client):
    filename, variable = ("mcp.json", "PLUGIN_ROOT") if client == "codex" else (".mcp.json", "CLAUDE_PLUGIN_ROOT")
    with tempfile.TemporaryDirectory(prefix="redisctl plugin smoke ") as fixture:
        relocated = Path(fixture) / "plugin with spaces"
        shutil.copytree(package, relocated)
        entry = json.loads((relocated / filename).read_text())["mcpServers"]["redisctl"]
        args = [arg.replace("${" + variable + "}", str(relocated)) for arg in entry["args"]]
        command, env = isolated_command(binary, args, Path(fixture))
        rpc = RpcProcess(command, fixture, env)
        try:
            init = rpc.request("initialize", {"protocolVersion": "2025-03-26", "capabilities": {},
                                              "clientInfo": {"name": "plugin-smoke", "version": "1"}})
            rpc.send("notifications/initialized", {})
            tools = rpc.request("tools/list", {})["tools"]
            names = {tool["name"] for tool in tools}
            # Every mandatory tool in the three packaged workflows must be
            # available; discovering the skill text alone is not sufficient.
            required = {
                "show_policy", "list_available_tools", "profile_list", "profile_path",
                "profile_validate", "redis_ping", "get_account", "list_subscriptions",
                "get_cluster", "list_nodes", "get_node", "get_license", "get_license_usage",
                "list_alerts", "list_enterprise_databases", "redis_dbsize", "redis_info",
                "redis_scan", "redis_type", "redis_memory_usage", "redis_ttl", "redis_get",
                "redis_hgetall", "redis_json_get", "redis_smembers", "redis_scard",
                "redis_zrange", "redis_lrange", "redis_xrange",
            }
            assert required <= names, f"Missing workflow tools: {sorted(required - names)}"
            assert not {"profile_create", "profile_delete", "redis_set", "redis_command", "cloud_raw_api", "profile_show"} & names
            assert all(tool.get("annotations", {}).get("readOnlyHint") is True for tool in tools)
            policy = rpc.request("tools/call", {"name": "show_policy", "arguments": {}})
            summary = json.loads(policy["content"][0]["text"])
            assert summary["global_tier"] == "read-only"
            denied = rpc.request("tools/call", {"name": "profile_create", "arguments": {}}, allow_error=True)
            assert "error" in denied or denied.get("isError") is True
            resources = rpc.request("resources/list", {})["resources"]
            assert any(resource["uri"] == "redisctl://skills" for resource in resources)
            index = rpc.request("resources/read", {"uri": "redisctl://skills"})
            index = json.loads(index["contents"][0]["text"])
            assert {"redisctl-setup", "enterprise-health-check", "data-explorer"} <= {skill["name"] for skill in index["skills"]}
            print(json.dumps({"client_config": client, "server_version": init["serverInfo"]["version"],
                              "read_only_tools": len(tools), "skills": len(index["skills"]),
                              "write_denied": True, "operator_config_isolated": True}))
        finally:
            rpc.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--package", type=Path, default=Path(__file__).resolve().parents[1] / "redisctl-mcp")
    parser.add_argument("--client", choices=("claude", "codex"), required=True)
    options = parser.parse_args()
    smoke(options.binary.resolve(strict=True), options.package.resolve(strict=True), options.client)
