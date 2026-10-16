#!/usr/bin/env python3
"""Native Claude install/health/uninstall fixture; macOS only, no model or network."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def snapshot(directory):
    return {
        str(path.relative_to(directory)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in directory.rglob("*") if path.is_file()
    }


def inspect(binary):
    if sys.platform != "darwin":
        raise RuntimeError("Native Claude fixture isolation is verified only on macOS")
    binary = binary.resolve(strict=True)
    if binary.name != "redisctl-mcp" or not binary.is_file():
        raise ValueError("Provide the already-built redisctl-mcp executable")
    claude = shutil.which("claude")
    if claude is None:
        raise RuntimeError("Claude Code CLI is required")
    with tempfile.TemporaryDirectory(prefix="redisctl Claude install ") as directory:
        # sandbox-exec subpath rules need canonical /private paths on macOS.
        fixture = Path(directory).resolve()
        marketplace = fixture / "marketplace with spaces"
        package = marketplace / "redisctl-mcp"
        shutil.copytree(ROOT / "plugins/redisctl-mcp", package)
        (marketplace / ".claude-plugin").mkdir()
        shutil.copyfile(ROOT / "plugins/.claude-plugin/marketplace.json",
                        marketplace / ".claude-plugin/marketplace.json")
        original = snapshot(marketplace)
        env = {key: os.environ[key] for key in ("PATH", "TMPDIR", "LANG") if key in os.environ}
        env["PATH"] = str(binary.parent) + os.pathsep + env.get("PATH", os.defpath)
        env["CLAUDE_CONFIG_DIR"] = str(fixture / "claude state")
        env["XDG_CONFIG_HOME"] = str(fixture / "xdg")
        blocked = [
            Path.home() / ".claude", Path.home() / ".claude.json", Path.home() / ".codex",
            Path.home() / ".config/redisctl",
            Path.home() / "Library/Application Support/com.redis.redisctl",
            Path.home() / "Library/Keychains",
        ]
        denied = " ".join(f"(subpath {json.dumps(str(path))})" for path in blocked)
        policy = (
            "(version 1) (allow default) (deny network*) "
            f"(deny file-read* {denied}) (deny file-write*) "
            f"(allow file-write* (subpath {json.dumps(str(fixture))})) "
            '(deny process-exec (literal "/usr/bin/security"))'
        )

        def run(args):
            result = subprocess.run(
                ["/usr/bin/sandbox-exec", "-p", policy, claude, "--bare", *args],
                cwd=fixture, env=env, capture_output=True, text=True, timeout=25,
            )
            if result.returncode != 0:
                # Do not publish stderr/credential-bearing diagnostic chains.
                raise RuntimeError(f"Claude fixture command failed: {args[0]} (exit {result.returncode})")
            return result.stdout

        version = run(["--version"]).strip()
        added = json.loads(run(["plugin", "marketplace", "add", str(marketplace), "--json"]))
        assert added["outcome"] == "ok"
        installed = json.loads(run(["plugin", "install", "redisctl-mcp@redisctl-experimental",
                                   "--scope", "user", "--json"]))
        assert installed["outcome"] == "ok"
        try:
            inventory = json.loads(run(["plugin", "list", "--json"]))
            assert len(inventory) == 1
            entry = inventory[0]
            assert entry["id"] == "redisctl-mcp@redisctl-experimental"
            assert entry["enabled"] is True
            assert set(entry["mcpServers"]) == {"redisctl"}
            assert Path(entry["installPath"]).is_relative_to(fixture)
            # A reported cache label is not evidence of loading a cache copy.
            source = Path(entry.get("readFromFolder", entry["installPath"]))
            assert source.resolve().is_relative_to(fixture)
            details = run(["plugin", "details", "redisctl-mcp@redisctl-experimental"])
            assert "Skills (3)" in details and "MCP servers (1)" in details
            assert all(name in details for name in ("data-explorer", "enterprise-health-check", "redisctl-setup"))
            # No --plugin-dir: the installed registration must drive startup.
            health = run(["mcp", "list"])
            assert "plugin:redisctl-mcp:redisctl:" in health
            assert "Connected" in health
            assert str(source / "read-only.toml") in health
            assert "--read-only=true" in health
        finally:
            run(["plugin", "uninstall", "redisctl-mcp@redisctl-experimental", "--scope", "user"])
        assert json.loads(run(["plugin", "list", "--json"])) == []
        assert snapshot(marketplace) == original
        print(json.dumps({
            "client": version, "fixture_install": True, "native_mcp_health": "connected",
            "skills": 3, "namespaced_server": "plugin:redisctl-mcp:redisctl",
            "local_source_in_place": source.resolve() == package.resolve(),
            "uninstalled": True, "source_preserved": True, "model_invocations": 0,
            "personal_state_blocked": True, "network_blocked": True,
        }))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    options = parser.parse_args()
    inspect(options.binary)
