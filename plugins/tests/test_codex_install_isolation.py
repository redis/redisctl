"""Container harness boundary regressions, not native client execution evidence."""

from pathlib import Path
import unittest
from unittest.mock import patch

import inspect_codex_install as fixture


class CodexInstallIsolationTests(unittest.TestCase):
    def test_container_command_has_one_readonly_input_and_no_secret_forwarding(self):
        command = fixture.docker_command("/synthetic/docker", fixture.IMAGE, Path("/synthetic/plugins"))
        for flag, value in (("--pull", "never"), ("--network", "none"),
                            ("--cap-drop", "ALL"), ("--security-opt", "no-new-privileges")):
            self.assertEqual(command[command.index(flag) + 1], value)
        self.assertIn("--read-only", command)
        self.assertEqual(command.count("--mount"), 1)
        self.assertEqual(command[command.index("--mount") + 1],
                         "type=bind,src=/synthetic/plugins,dst=/input,readonly")
        self.assertEqual(command.count("--tmpfs"), 3)
        self.assertFalse({"--env", "--env-file", "-e", "--privileged", "-v"} & set(command))
        self.assertNotIn("HOME=", " ".join(command))
        self.assertNotIn("CODEX_HOME=", " ".join(command))
        self.assertNotIn("docker.sock", " ".join(command))

    def test_missing_docker_never_installs_on_host(self):
        with patch.object(fixture.shutil, "which", return_value=None), \
                patch("sys.argv", ["inspect_codex_install.py"]), \
                patch.object(fixture.subprocess, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "no host-install fallback"):
                fixture.main()
            run.assert_not_called()

    def test_native_guard_rejects_host_before_any_client_or_copy(self):
        with patch.object(fixture.Path, "is_file", return_value=False), \
                patch.object(fixture.subprocess, "run") as run, \
                patch.object(fixture.shutil, "copytree") as copy:
            with self.assertRaisesRegex(RuntimeError, "disposable Docker fixture"):
                fixture.native_trial()
            run.assert_not_called()
            copy.assert_not_called()

    def test_outer_runner_drops_operator_credentials_and_targets(self):
        inherited = {"PATH": "/synthetic/bin", "LANG": "C", "HOME": "/operator",
                     "CODEX_HOME": "/operator/.codex", "REDIS_URL": "synthetic-secret",
                     "OPENAI_API_KEY": "synthetic-secret", "REDISCTL_PROFILE": "operator-target"}
        with patch.dict(fixture.os.environ, inherited, clear=True), \
                patch.object(fixture.shutil, "which", return_value="/synthetic/docker"), \
                patch("sys.argv", ["inspect_codex_install.py"]), \
                patch.object(fixture.subprocess, "run") as run:
            fixture.main()
        run.assert_called_once()
        self.assertEqual(run.call_args.kwargs["env"], {"PATH": "/synthetic/bin", "LANG": "C"})
        self.assertNotIn("synthetic-secret", " ".join(run.call_args.args[0]))


if __name__ == "__main__":
    unittest.main()
