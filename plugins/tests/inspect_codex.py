#!/usr/bin/env python3
"""Read local plugin metadata through Codex; no install, thread, model or MCP call."""

import json
import os
from pathlib import Path
import shutil
import tempfile

from smoke_stdio import RpcProcess


def main():
    root = Path(__file__).resolve().parents[2]
    codex = shutil.which("codex")
    if codex is None:
        raise RuntimeError("Codex CLI is required for native package inspection")
    with tempfile.TemporaryDirectory(prefix="redisctl codex metadata ") as fixture:
        directory = Path(fixture)
        env = {key: os.environ[key] for key in ("PATH", "TMPDIR", "SystemRoot", "LANG") if key in os.environ}
        env.update({"HOME": str(directory / "home"), "CODEX_HOME": str(directory / "codex"),
                    "XDG_CONFIG_HOME": str(directory / "config")})
        for name in ("HOME", "CODEX_HOME", "XDG_CONFIG_HOME"):
            Path(env[name]).mkdir()
        # No project cwd: do not load the operator's project-level Codex configuration.
        rpc = RpcProcess([codex, "app-server", "--stdio", "-c", "analytics.enabled=false"], directory, env)
        try:
            rpc.request("initialize", {"clientInfo": {"name": "redisctl-plugin-inspection", "version": "1"},
                                       "capabilities": {"experimentalApi": True}})
            rpc.send("initialized", {})
            result = rpc.request("plugin/read", {
                "pluginName": "redisctl-mcp",
                "marketplacePath": str(root / "plugins/.agents/plugins/marketplace.json"),
            })["plugin"]
            names = sorted(skill["name"] for skill in result["skills"])
            assert names == ["redisctl-mcp:data-explorer", "redisctl-mcp:enterprise-health-check", "redisctl-mcp:redisctl-setup"]
            assert result["mcpServers"] == ["redisctl"]
            assert not result["hooks"]
            print(json.dumps({"client": "Codex", "skills": names, "mcp_servers": result["mcpServers"],
                              "marketplace": result["marketplaceName"], "installed": False,
                              "model_or_mcp_invocations": 0}))
        finally:
            rpc.close()


if __name__ == "__main__":
    main()
