"""Policy evidence must distinguish authorization from validation/handler errors."""

import unittest

from smoke_stdio import PROFILE_PROBE_ARGUMENTS, assert_profile_create_denied


class PolicyProbeTests(unittest.TestCase):
    def test_rpc_authorization_denial_is_accepted(self):
        assert_profile_create_denied({"error": {"code": -32001,
                                               "message": "Unauthorized: profile_create"}})

    def test_wrapped_mcp_authorization_denial_is_accepted(self):
        assert_profile_create_denied({"isError": True, "content": [{"type": "text",
                                     "text": "MCP error -32001: Unauthorized: profile_create"}]})

    def test_non_authorization_failures_and_successes_are_rejected(self):
        for result in ({}, {"content": [{"type": "text", "text": "Unauthorized: profile_create"}]},
                       {"error": {"message": "missing field name"}},
                       {"error": {"message": "Invalid profile type"}},
                       {"error": {"message": "Unauthorized: profile_delete"}},
                       {"isError": True, "content": [{"type": "text", "text": "invalid arguments"}]}):
            with self.subTest(result=result), self.assertRaisesRegex(AssertionError, "explicit policy denial"):
                assert_profile_create_denied(result)

    def test_probe_has_required_fields_but_cannot_write_if_policy_is_removed(self):
        self.assertEqual(set(PROFILE_PROBE_ARGUMENTS), {"name", "profile_type"})
        self.assertNotIn(PROFILE_PROBE_ARGUMENTS["profile_type"].lower(),
                         {"cloud", "enterprise", "database", "db"})


if __name__ == "__main__":
    unittest.main()
