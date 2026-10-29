"""Deterministic RPC boundary tests: no child process, sleeps, or network."""

import json
import queue
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


if __name__ == "__main__":
    unittest.main()
