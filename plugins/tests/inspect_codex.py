#!/usr/bin/env python3
"""Read local plugin metadata through Codex; no install, thread, model or MCP call."""

import json
import os
from pathlib import Path
import shutil

from smoke_stdio import RpcProcess


def main():
    root = Path(__file__).resolve().parents[2]
    codex = shutil.which("codex")
    if codex is None:
        raise RuntimeError("Codex CLI is required for native package inspection")
    env = {key: os.environ[key] for key in ("PATH", "TMPDIR", "SystemRoot", "LANG") if key in os.environ}
    rpc = RpcProcess([codex, "app-server", "--stdio", "-c", "analytics.enabled=false"], root / "plugins", env)
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
