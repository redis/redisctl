"""Synthetic smoke-harness boundary tests; no installed client/server is started."""

import importlib.util
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("smoke_stdio", ROOT / "plugins/tests/smoke_stdio.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class SmokeIsolationTests(unittest.TestCase):
    def test_supported_platforms_drop_credentials_and_targets(self):
        inherited = {
            "PATH": "/synthetic/bin", "LANG": "C", "TMPDIR": "/synthetic/tmp",
            "REDISCTL_PROFILE": "operator-target", "REDIS_URL": "synthetic-secret",
            "REDIS_CLOUD_API_KEY": "synthetic-secret", "REDISCTL_CONFIG_FILE": "/operator/config",
            "AWS_SECRET_ACCESS_KEY": "synthetic-secret", "ANTHROPIC_API_KEY": "synthetic-secret",
        }
        for platform in ("linux", "win32", "darwin"):
            with self.subTest(platform=platform), patch.dict(smoke.os.environ, inherited, clear=True), \
                    patch.object(smoke.sys, "platform", platform), \
                    patch.object(smoke.Path, "home", return_value=PurePosixPath("/synthetic/operator")):
                command, env = smoke.isolated_command(Path("/synthetic/server"), ["--transport", "stdio"], Path("/fixture"))
                self.assertEqual(set(env), {"PATH", "LANG", "TMPDIR", "XDG_CONFIG_HOME", "APPDATA", "LOCALAPPDATA"})
                for key in ("XDG_CONFIG_HOME", "APPDATA", "LOCALAPPDATA"):
                    self.assertEqual(env[key], str(Path("/fixture")))
                self.assertNotIn("synthetic-secret", " ".join(command))
                self.assertNotIn("operator-target", " ".join(command))
                self.assertNotIn("HOME", env)
                self.assertNotIn("CODEX_HOME", env)

    def test_macos_sandbox_blocks_config_and_network_without_fallback(self):
        with patch.object(smoke.sys, "platform", "darwin"), \
                patch.object(smoke.Path, "home", return_value=PurePosixPath("/synthetic/operator")):
            command, env = smoke.isolated_command(Path("/synthetic/server"), [], Path("/fixture"))
        self.assertEqual(command[:2], ["/usr/bin/sandbox-exec", "-p"])
        self.assertIn("(deny network*)", command[2])
        self.assertIn("/synthetic/operator/.config/redisctl", command[2])
        self.assertIn("/synthetic/operator/Library/Application Support/com.redis.redisctl", command[2])
        with patch.object(smoke.subprocess, "Popen", side_effect=FileNotFoundError("synthetic missing sandbox")) as start:
            with self.assertRaises(FileNotFoundError):
                smoke.RpcProcess(command, "/fixture", env)
            start.assert_called_once()  # never retry without the sandbox

    def test_unverified_platform_is_rejected(self):
        with patch.object(smoke.sys, "platform", "synthetic-unsupported"):
            with self.assertRaisesRegex(RuntimeError, "No verified config isolation"):
                smoke.isolated_command(Path("/synthetic/server"), [], Path("/fixture"))

    def test_missing_binary_fails_before_protocol_start(self):
        with tempfile.TemporaryDirectory(prefix="missing plugin binary ") as directory:
            missing = Path(directory) / "redisctl-mcp"
            result = subprocess.run([
                sys.executable, str(ROOT / "plugins/tests/smoke_stdio.py"),
                "--binary", str(missing), "--client", "codex",
            ], cwd=directory, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("FileNotFoundError", result.stderr)
            self.assertNotIn('"read_only_tools"', result.stdout)
            self.assertEqual(list(Path(directory).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
