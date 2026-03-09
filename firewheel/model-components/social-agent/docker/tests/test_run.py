import importlib.util
import os
import requests
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch


def _load_run_module():
    module_path = (
        Path(__file__).resolve().parents[1] / "run.py"
    )
    spec = importlib.util.spec_from_file_location("social_agent_run", module_path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class _FakeResponse:
    def __init__(self, payload):
        self._payload = payload

    def raise_for_status(self):
        return None

    def json(self):
        return self._payload


class SocialAgentRunTests(unittest.TestCase):
    def test_call_size_uses_gateway_get_without_auth_header(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "JOURNAL": "journal.net", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()

            fake_metrics = MagicMock()
            run.METRICS = fake_metrics

            with patch.object(run.requests, "get", return_value=_FakeResponse(12)) as mock_get:
                result = run.call("size")

            self.assertEqual(result, 12)
            mock_get.assert_called_once()
            kwargs = mock_get.call_args.kwargs
            self.assertEqual(kwargs["headers"], {"accept": "application/json"})
            self.assertEqual(kwargs["timeout"], run.REQUEST_TIMEOUT_SECONDS)
            self.assertEqual(
                mock_get.call_args.args[0], "http://journal.net/api/v1/general/size"
            )
            self.assertEqual(fake_metrics.record_request.call_count, 1)
            self.assertEqual(fake_metrics.record_request.call_args.args[0], "size")
            self.assertTrue(fake_metrics.record_request.call_args.args[2])

    def test_call_peers_uses_gateway_get_with_auth_header(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "JOURNAL": "journal.net", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()

            run.METRICS = MagicMock()

            with patch.object(run.requests, "get", return_value=_FakeResponse([])) as mock_get:
                run.call("peers")

            headers = mock_get.call_args.kwargs["headers"]
            self.assertEqual(headers["accept"], "application/json")
            self.assertEqual(headers["x-sync-auth"], "pass")

    def test_call_set_uses_gateway_post_with_arguments_body(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "JOURNAL": "journal.net", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()

            run.METRICS = MagicMock()
            payload = {
                "path": [["*state*", "data", "key-1"]],
                "value": {"*type/string*": "value-1"},
            }

            with patch.object(run.requests, "post", return_value=_FakeResponse(True)) as mock_post:
                result = run.call("set", payload)

            self.assertTrue(result)
            self.assertEqual(
                mock_post.call_args.args[0], "http://journal.net/api/v1/general/set"
            )
            kwargs = mock_post.call_args.kwargs
            self.assertEqual(
                kwargs["headers"],
                {
                    "accept": "application/json",
                    "x-sync-auth": "pass",
                    "content-type": "application/json",
                },
            )
            self.assertEqual(kwargs["timeout"], run.REQUEST_TIMEOUT_SECONDS)
            self.assertEqual(kwargs["json"], {"arguments": payload})

    def test_call_honors_gateway_base_override(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "JOURNAL": "journal.net",
                "SECRET": "pass",
                "JOURNAL_GATEWAY_BASE": "http://router.local/custom/general",
            },
            clear=False,
        ):
            run = _load_run_module()

            run.METRICS = MagicMock()

            with patch.object(run.requests, "post", return_value=_FakeResponse({"ok": True})) as mock_post:
                run.call("get", {"path": [["*state*", "x"]]})

            self.assertEqual(
                mock_post.call_args.args[0], "http://router.local/custom/general/get"
            )

    def test_metrics_tracks_set_operation_name(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "JOURNAL": "journal.net", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()

        metrics = run.Metrics()
        metrics.record_request("set", 0.25, True)
        self.assertEqual(metrics.set_latency_count, 1)
        self.assertEqual(metrics.requests_total, 1)

    def test_call_raises_on_http_error_and_records_failure(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "JOURNAL": "journal.net", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()

            fake_metrics = MagicMock()
            run.METRICS = fake_metrics

            class _ErrorResponse:
                def raise_for_status(self):
                    raise requests.HTTPError("boom")

                def json(self):
                    return {"error": "boom"}

            with patch.object(run.requests, "get", return_value=_ErrorResponse()):
                with self.assertRaises(requests.HTTPError):
                    run.call("size")

            self.assertEqual(fake_metrics.record_request.call_count, 1)
            self.assertEqual(fake_metrics.record_request.call_args.args[0], "size")
            self.assertFalse(fake_metrics.record_request.call_args.args[2])


if __name__ == "__main__":
    unittest.main()
