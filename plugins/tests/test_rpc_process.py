"""Deterministic RPC boundary tests: no child process, sleeps, or network."""

import json
import os
import queue
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

import smoke_stdio


class RpcProcessTests(unittest.TestCase):
    def rpc(self, *lines):
        rpc = smoke_stdio.RpcProcess.__new__(smoke_stdio.RpcProcess)
        rpc.identifier = 0
        rpc.send = Mock()
        rpc.lines = Mock()
        rpc.lines.get.side_effect = lines
        return rpc

    def test_silent_queue_has_contextual_timeout(self):
        rpc = self.rpc(queue.Empty)
        with patch.object(smoke_stdio.time, "monotonic", side_effect=[0, 1]):
            with self.assertRaisesRegex(TimeoutError, "Protocol request timed out: synthetic/read") as caught:
                rpc.request("synthetic/read", {})
        self.assertIsNone(caught.exception.__cause__)
        rpc.lines.get.assert_called_once_with(timeout=19)

    def test_expired_deadline_does_not_poll(self):
        rpc = self.rpc()
        with patch.object(smoke_stdio.time, "monotonic", side_effect=[0, 20]):
            with self.assertRaisesRegex(TimeoutError, "synthetic/expired"):
                rpc.request("synthetic/expired", {})
        rpc.lines.get.assert_not_called()

    def test_notification_traffic_cannot_extend_deadline(self):
        rpc = self.rpc(json.dumps({"method": "notifications/progress", "params": {}}))
        with patch.object(smoke_stdio.time, "monotonic", side_effect=[0, 1, 20]):
            with self.assertRaisesRegex(TimeoutError, "synthetic/traffic"):
                rpc.request("synthetic/traffic", {})
        rpc.lines.get.assert_called_once_with(timeout=19)

    def test_matching_result_is_returned(self):
        rpc = self.rpc(json.dumps({"id": 1, "result": {"value": "synthetic"}}))
        with patch.object(smoke_stdio.time, "monotonic", side_effect=[0, 1]):
            self.assertEqual(rpc.request("synthetic/read", {}), {"value": "synthetic"})
        rpc.send.assert_called_once_with("synthetic/read", {}, 1)

    def test_rpc_error_can_be_returned_as_data(self):
        error = {"code": -32000, "message": "synthetic detail"}
        rpc = self.rpc(json.dumps({"id": 1, "error": error}))
        with patch.object(smoke_stdio.time, "monotonic", side_effect=[0, 1]):
            self.assertEqual(rpc.request("synthetic/denied", {}, allow_error=True), {"error": error})

    def test_raised_rpc_error_does_not_include_raw_detail(self):
        rpc = self.rpc(json.dumps({"id": 1, "error": {"message": "synthetic private detail"}}))
        with patch.object(smoke_stdio.time, "monotonic", side_effect=[0, 1]):
            with self.assertRaisesRegex(RuntimeError, "Protocol request failed: synthetic/denied") as caught:
                rpc.request("synthetic/denied", {})
        self.assertNotIn("private detail", str(caught.exception))

    def test_eof_remains_a_protocol_failure(self):
        rpc = self.rpc(None)
        with patch.object(smoke_stdio.time, "monotonic", side_effect=[0, 1]):
            with self.assertRaisesRegex(RuntimeError, "ended before responding"):
                rpc.request("synthetic/read", {})

    def cleanup_rpc(self, waits):
        rpc = smoke_stdio.RpcProcess.__new__(smoke_stdio.RpcProcess)
        rpc.process = Mock()
        rpc.process.wait.side_effect = waits
        return rpc

    def test_cleanup_reaps_an_already_exited_child(self):
        rpc = self.cleanup_rpc([0])
        rpc.close()
        rpc.process.stdin.close.assert_called_once()
        rpc.process.wait.assert_called_once_with(timeout=5)
        rpc.process.terminate.assert_not_called()
        rpc.process.kill.assert_not_called()
        rpc.process.stdout.close.assert_called_once()

    def test_cleanup_terminates_a_child_that_ignores_eof(self):
        rpc = self.cleanup_rpc([subprocess.TimeoutExpired("synthetic", 5), 0])
        rpc.close()
        rpc.process.terminate.assert_called_once()
        rpc.process.kill.assert_not_called()
        self.assertEqual(rpc.process.wait.call_count, 2)
        rpc.process.stdout.close.assert_called_once()

    def test_cleanup_kills_and_reaps_a_child_that_ignores_termination(self):
        timeout = subprocess.TimeoutExpired("synthetic", 5)
        rpc = self.cleanup_rpc([timeout, timeout, -9])
        rpc.close()
        rpc.process.terminate.assert_called_once()
        rpc.process.kill.assert_called_once()
        self.assertEqual(rpc.process.wait.call_count, 3)
        rpc.process.stdout.close.assert_called_once()

    def test_broken_stdin_does_not_prevent_reaping(self):
        rpc = self.cleanup_rpc([0])
        rpc.process.stdin.close.side_effect = BrokenPipeError("synthetic")
        rpc.close()
        rpc.process.wait.assert_called_once_with(timeout=5)
        rpc.process.stdout.close.assert_called_once()

    def test_cleanup_failure_closes_streams_and_redacts_diagnostics(self):
        rpc = self.cleanup_rpc([OSError("synthetic private detail")])
        with self.assertRaisesRegex(RuntimeError, "Protocol cleanup failed") as caught:
            rpc.close()
        self.assertNotIn("private detail", str(caught.exception))
        rpc.process.stdout.close.assert_called_once()

    def test_cleanup_failure_does_not_mask_the_original_failure(self):
        rpc = self.cleanup_rpc([OSError("synthetic private detail")])
        with patch.object(smoke_stdio.sys, "stderr"), \
                self.assertRaisesRegex(ValueError, "original failure"):
            try:
                raise ValueError("original failure")
            finally:
                rpc.close()
        rpc.process.stdout.close.assert_called_once()

    @unittest.skipUnless(os.name == "posix", "requires POSIX signal handling")
    def test_real_sigterm_ignoring_child_is_killed_and_reaped(self):
        program = "import signal\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nprint('synthetic-ready', flush=True)\nwhile True: signal.pause()\n"
        with tempfile.TemporaryDirectory(prefix="rpc cleanup fixture ") as directory:
            rpc = smoke_stdio.RpcProcess([sys.executable, "-I", "-u", "-c", program], directory, {})
            try:
                self.assertEqual(rpc.lines.get(timeout=5).strip(), "synthetic-ready")
                rpc.close()
                self.assertIsNotNone(rpc.process.poll())
                self.assertTrue(rpc.process.stdin.closed)
                self.assertTrue(rpc.process.stdout.closed)
            finally:
                # Always reap this test's child, including when testing a broken cleanup.
                if rpc.process.poll() is None:
                    rpc.process.kill()
                    rpc.process.wait(timeout=5)
                rpc.process.stdin.close()
                rpc.process.stdout.close()


if __name__ == "__main__":
    unittest.main()
