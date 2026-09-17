from __future__ import annotations

import importlib.util
import sys
import types
import unittest
from pathlib import Path
from unittest.mock import patch


if importlib.util.find_spec("requests") is None:
    requests_stub = types.ModuleType("requests")
    requests_stub.exceptions = types.SimpleNamespace(JSONDecodeError=ValueError)
    sys.modules["requests"] = requests_stub

MODULE_PATH = Path(__file__).with_name("single_node_journey.py")
SPEC = importlib.util.spec_from_file_location("single_node_journey", MODULE_PATH)
assert SPEC and SPEC.loader
journey = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = journey
SPEC.loader.exec_module(journey)


class FakeResponse:
    def __init__(self, status_code: int, value: object = None) -> None:
        self.status_code = status_code
        self.value = value
        self.text = "" if value is None else str(value)

    def json(self) -> object:
        return self.value


class RunAuthorizationJourneyTests(unittest.TestCase):
    def test_grant_run_readback_revoke_and_admin_fallback(self) -> None:
        responses = iter(
            [
                FakeResponse(400),
                FakeResponse(200),
                FakeResponse(200, "journey-value"),
                FakeResponse(200, "journey-value"),
                FakeResponse(200),
                FakeResponse(400),
                FakeResponse(200, "journey-value"),
            ]
        )
        requests: list[tuple[str, str, str, dict[str, object]]] = []

        def fake_post(base: str, operation: str, token: str, body: dict[str, object]) -> FakeResponse:
            requests.append((base, operation, token, body))
            return next(responses)

        program_path = ["*state*", "alice", "release-qa", "program"]
        result_path = ["*state*", "alice", "release-qa", "result"]
        with patch.object(journey, "post_json", side_effect=fake_post):
            journey.exercise_run_authorization(
                "http://gateway",
                "alice",
                "user-token",
                "admin-token",
                program_path,
                result_path,
            )

        self.assertEqual(
            [request[1] for request in requests],
            ["run", "authorize", "run", "use", "deauthorize", "run", "run"],
        )
        self.assertEqual(
            [request[2] for request in requests],
            ["user-token"] * 6 + ["admin-token"],
        )
        exact_rule = {
            "principal": ["*state*", "alice"],
            "path": ["release-qa"],
            "put!": False,
            "use!": {"read-only?": True},
            "run!": True,
            "retrieve": False,
        }
        exact_envelope = {"user": ["*state*", "alice"], "rule": exact_rule}
        self.assertEqual(requests[1][3], exact_envelope)
        self.assertEqual(requests[4][3], exact_envelope)
        self.assertEqual(
            requests[0][3],
            {"path": program_path, "arguments": [result_path, "journey-value"]},
        )
        self.assertEqual(
            requests[3][3],
            {"path": result_path, "read-only?": True, "expression?": True},
        )


if __name__ == "__main__":
    unittest.main()
