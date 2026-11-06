"""Metadata inspection environment contract; no installed client is started."""

from pathlib import Path
import unittest
from unittest.mock import Mock, patch

import inspect_codex as fixture


class CodexMetadataIsolationTests(unittest.TestCase):
    def test_inspector_uses_disposable_homes_and_cwd_without_forwarding_secrets(self):
        rpc = Mock()
        rpc.request.return_value = {"plugin": {
            "skills": [{"name": f"redisctl-mcp:{name}"} for name in
                       ("data-explorer", "enterprise-health-check", "redisctl-setup")],
            "mcpServers": ["redisctl"], "hooks": [], "marketplaceName": "synthetic",
        }}
        inherited = {"PATH": "/synthetic/bin", "LANG": "C", "HOME": "/operator",
                     "CODEX_HOME": "/operator/.codex", "OPENAI_API_KEY": "private-marker",
                     "REDIS_URL": "private-marker", "REDISCTL_PROFILE": "operator-target"}
        def start(command, cwd, env):
            self.assertTrue(Path(cwd).exists())
            for name in ("HOME", "CODEX_HOME", "XDG_CONFIG_HOME"):
                self.assertTrue(Path(env[name]).is_dir())
            return rpc

        with patch.dict(fixture.os.environ, inherited, clear=True), \
                patch.object(fixture.shutil, "which", return_value="/synthetic/codex"), \
                patch.object(fixture, "RpcProcess", side_effect=start) as create, \
                patch("builtins.print"):
            fixture.main()
            command, cwd, env = create.call_args.args
            self.assertEqual(command[0], "/synthetic/codex")
            self.assertNotEqual(env["HOME"], "/operator")
            self.assertNotEqual(env["CODEX_HOME"], "/operator/.codex")
            self.assertTrue(Path(env["HOME"]).is_relative_to(cwd))
            self.assertTrue(Path(env["CODEX_HOME"]).is_relative_to(cwd))
            self.assertEqual(set(env), {"PATH", "LANG", "HOME", "CODEX_HOME", "XDG_CONFIG_HOME"})
            self.assertNotIn("private-marker", str(env))
            self.assertNotIn("operator-target", str(env))
            self.assertNotIn("turn/start", [call.args[0] for call in rpc.request.call_args_list])
            self.assertNotIn("thread/start", [call.args[0] for call in rpc.request.call_args_list])
            rpc.close.assert_called_once()
        self.assertFalse(Path(cwd).exists())

    def test_missing_client_stops_before_protocol_start(self):
        with patch.object(fixture.shutil, "which", return_value=None), \
                patch.object(fixture, "RpcProcess") as create:
            with self.assertRaisesRegex(RuntimeError, "Codex CLI is required"):
                fixture.main()
            create.assert_not_called()


if __name__ == "__main__":
    unittest.main()
