import json
import importlib.util
import os
import requests
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch


def _load_run_module():
    module_path = Path(__file__).resolve().parents[1] / "run.py"
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
    def test_get_activity_seconds_treats_empty_as_serial_mode(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "ACTIVITY": ""}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_activity_seconds(), 0.0)

    def test_get_activity_seconds_treats_zero_as_serial_mode(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "ACTIVITY": "0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_activity_seconds(), 0.0)

    def test_get_size_defaults_when_empty_or_nonpositive(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_size(), 32)

        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_size(), 32)

        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "12"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_size(), 12)

    def test_get_clients_defaults_when_empty_or_nonpositive(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_clients(), 1)

        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "CLIENTS": "0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_clients(), 1)

        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "CLIENTS": "4"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_clients(), 4)

    def test_call_size_uses_local_router_gateway_without_auth_header(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()
            fake_metrics = MagicMock()
            run.METRICS = fake_metrics
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "get", return_value=_FakeResponse(12)) as mock_get:
                result = run.call(nodes, "size")

            self.assertEqual(result, 12)
            kwargs = mock_get.call_args.kwargs
            self.assertEqual(kwargs["headers"], {"accept": "application/json"})
            self.assertEqual(kwargs["timeout"], run.REQUEST_TIMEOUT_SECONDS)
            self.assertEqual(mock_get.call_args.args[0], "http://router-0/api/v1/general/size")
            self.assertEqual(fake_metrics.record_request.call_args.args[0], "size")
            self.assertTrue(fake_metrics.record_request.call_args.args[2])

    def test_call_bridge_uses_auth_header(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "post", return_value=_FakeResponse(True)) as mock_post:
                run.call(
                    nodes,
                    "bridge",
                    {
                        "name": "journal-1",
                        "interface": {"*type/string*": "http://router-1/interface"},
                    },
                )

            headers = mock_post.call_args.kwargs["headers"]
            self.assertEqual(headers["accept"], "application/json")
            self.assertEqual(headers["x-sync-auth"], "pass")

    def test_call_set_uses_direct_json_arguments(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            payload = {
                "path": [["*state*", "data", "key-1"]],
                "value": {"*type/string*": "value-1"},
            }
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "post", return_value=_FakeResponse(True)) as mock_post:
                result = run.call(nodes, "set", payload)

            self.assertTrue(result)
            self.assertEqual(mock_post.call_args.args[0], "http://router-0/api/v1/general/set")
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
            self.assertEqual(kwargs["json"], payload)

    def test_call_honors_router_gateway_base_override(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SECRET": "pass",
                "ROUTER_GATEWAY_BASE": "http://router.local/custom/general",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "post", return_value=_FakeResponse({"ok": True})) as mock_post:
                run.call(nodes, "get", {"path": [["*state*", "x"]]})

            self.assertEqual(mock_post.call_args.args[0], "http://router.local/custom/general/get")

    def test_call_rewrites_indexed_get_to_resolve(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "post", return_value=_FakeResponse({"*type/string*": "x"})) as mock_post:
                result = run.call(
                    nodes,
                    "get",
                    {"path": [-1, ["*state*", "data", "key-0"]]},
                )

            self.assertEqual(result, {"*type/string*": "x"})
            self.assertEqual(
                mock_post.call_args.args[0],
                "http://router-0/api/v1/general/resolve",
            )
            self.assertEqual(
                mock_post.call_args.kwargs["json"],
                {
                    "path": [-1, ["*state*", "data", "key-0"]],
                    "pinned?": False,
                    "proof?": False,
                },
            )

    def test_make_benchmark_snapshot_includes_rates(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            stats = {
                "started": 100.0,
                "requests_total": 30,
                "requests_failed_total": 4,
                "get_latency_sum": 12.0,
                "get_latency_count": 20,
                "set_latency_sum": 5.0,
                "set_latency_count": 10,
                "activity_cycles_total": 9,
                "activity_requests_total": 14,
                "activity_requests_success_total": 11,
                "nodes": [],
                "inferred_hop_requests_total": {},
            }
            previous = {
                "timestamp": 118.0,
                "stats": {
                    "requests_total": 18,
                    "get_latency_count": 12,
                    "set_latency_count": 6,
                    "activity_cycles_total": 5,
                    "activity_requests_total": 9,
                    "activity_requests_success_total": 7,
                },
            }

            snapshot = run.make_benchmark_snapshot(stats, 120.0, previous=previous)

            self.assertEqual(snapshot["node_name"], "journal-0")
            self.assertEqual(snapshot["requests_succeeded_total"], 26)
            self.assertEqual(snapshot["get_latency_sum"], 12.0)
            self.assertEqual(snapshot["get_latency_count"], 20)
            self.assertEqual(snapshot["set_latency_sum"], 5.0)
            self.assertEqual(snapshot["set_latency_count"], 10)
            self.assertAlmostEqual(snapshot["average_get_latency_seconds"], 0.6)
            self.assertAlmostEqual(snapshot["average_set_latency_seconds"], 0.5)
            self.assertAlmostEqual(snapshot["requests_per_second"], 6.0)
            self.assertAlmostEqual(snapshot["get_requests_per_second"], 4.0)
            self.assertAlmostEqual(snapshot["set_requests_per_second"], 2.0)
            self.assertAlmostEqual(snapshot["activity_cycles_per_second"], 2.0)
            self.assertAlmostEqual(snapshot["activity_requests_per_second"], 2.0)
            self.assertAlmostEqual(snapshot["activity_request_success_rate"], 100.0)
            self.assertAlmostEqual(snapshot["requests_per_second_lifetime"], 1.5)

    def test_write_benchmark_snapshot_writes_json_file(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "BENCHMARK_OUTPUT": "/tmp/social-agent-benchmark.json",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            run.METRICS.snapshot.return_value = {
                "started": 100.0,
                "requests_total": 10,
                "requests_failed_total": 1,
                "get_latency_sum": 3.0,
                "get_latency_count": 5,
                "set_latency_sum": 2.0,
                "set_latency_count": 5,
                "activity_cycles_total": 4,
                "activity_requests_total": 6,
                "activity_requests_success_total": 5,
                "nodes": [],
                "inferred_hop_requests_total": {},
            }

            file_state = {}

            def _fake_open(path, mode="r", encoding=None):
                self.assertEqual(path, "/tmp/social-agent-benchmark.json.tmp")
                handle = MagicMock()
                buffer = []

                def _write(data):
                    buffer.append(data)
                    return len(data)

                handle.write.side_effect = _write
                handle.__enter__.return_value = handle
                def _flush():
                    file_state["content"] = "".join(buffer)

                handle.__exit__.side_effect = lambda *args: (_flush(), False)[1]
                return handle

            with patch.object(run.time, "time", return_value=110.0):
                with patch.object(run.os, "makedirs") as mock_makedirs:
                    with patch.object(run.os, "replace") as mock_replace:
                        with patch("builtins.open", side_effect=_fake_open):
                            previous = run.write_benchmark_snapshot()

            self.assertEqual(previous["timestamp"], 110.0)
            mock_makedirs.assert_called_once_with("/tmp", exist_ok=True)
            mock_replace.assert_called_once_with(
                "/tmp/social-agent-benchmark.json.tmp",
                "/tmp/social-agent-benchmark.json",
            )
            written = json.loads(file_state["content"])
            self.assertEqual(written["node_name"], "journal-0")
            self.assertEqual(written["requests_total"], 10)
            self.assertEqual(written["get_latency_sum"], 3.0)
            self.assertEqual(written["set_latency_count"], 5)

    def test_run_registers_bridges_with_wrapped_name_and_interface(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SECRET": "pass",
                "SIZE": "0",
                "ACTIVITY": "1.0",
                "CLIENTS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {
                "journal-0": {"router_host": "router-0"},
                "journal-1": {"router_host": "router-1"},
            }
            edges = {"journal-0": ["journal-1"], "journal-1": []}

            with patch.object(run, "call", side_effect=[True, KeyboardInterrupt()]) as mock_call:
                with patch.object(run, "Thread") as mock_thread:
                    mock_thread.side_effect = lambda target, daemon=True: type(
                        "_InlineThread",
                        (),
                        {"start": lambda self: target(), "join": lambda self: None},
                    )()
                    with self.assertRaises(KeyboardInterrupt):
                        run.run(nodes, edges)

            self.assertEqual(mock_call.call_args_list[0].args[1], "bridge")
            self.assertEqual(
                mock_call.call_args_list[0].args[2],
                {
                    "name": "journal-1",
                    "interface": {"*type/string*": "http://router-1/interface"},
                },
            )

    def test_call_raises_on_http_error_and_records_failure(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0", "SECRET": "pass"},
            clear=False,
        ):
            run = _load_run_module()
            fake_metrics = MagicMock()
            run.METRICS = fake_metrics
            nodes = {"journal-0": {"router_host": "router-0"}}

            class _ErrorResponse:
                def raise_for_status(self):
                    raise requests.HTTPError("boom")

                def json(self):
                    return {"error": "boom"}

            with patch.object(run.requests, "get", return_value=_ErrorResponse()):
                with self.assertRaises(requests.HTTPError):
                    run.call(nodes, "size")

            self.assertEqual(fake_metrics.record_request.call_args.args[0], "size")
            self.assertFalse(fake_metrics.record_request.call_args.args[2])

    def test_run_swallows_activity_cycle_errors(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SECRET": "pass",
                "SIZE": "1",
                "ACTIVITY": "4.0",
                "CLIENTS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            fake_metrics = MagicMock()
            run.METRICS = fake_metrics
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}
            set_calls = {"count": 0}

            def _call_side_effect(_nodes, operation, arguments=None):
                if operation == "set":
                    set_calls["count"] += 1
                    if set_calls["count"] == 1:
                        return True
                    raise requests.HTTPError("boom")
                if operation == "get":
                    return {"*type/string*": "one two"}
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "call", side_effect=_call_side_effect):
                with patch.object(run, "Thread") as mock_thread:
                    mock_thread.side_effect = lambda target, daemon=True: type(
                        "_InlineThread",
                        (),
                        {"start": lambda self: target(), "join": lambda self: None},
                    )()
                    with self.assertRaises(KeyboardInterrupt):
                        run.run(nodes, edges)

            self.assertTrue(fake_metrics.record_cycle.called)

    def test_run_uses_serial_mode_when_activity_is_zero(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SECRET": "pass",
                "SIZE": "0",
                "ACTIVITY": "0",
                "CLIENTS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}

            def _call_side_effect(_nodes, operation, arguments=None):
                if operation == "set":
                    raise KeyboardInterrupt()
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "call", side_effect=_call_side_effect):
                with patch.object(run, "Thread") as mock_thread:
                    mock_thread.side_effect = lambda target, daemon=True: type(
                        "_InlineThread",
                        (),
                        {"start": lambda self: target(), "join": lambda self: None},
                    )()
                    with self.assertRaises(KeyboardInterrupt):
                        run.run(nodes, edges)

            self.assertTrue(mock_thread.called)

    def test_run_defaults_size_when_size_is_zero(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SECRET": "pass",
                "SIZE": "0",
                "ACTIVITY": "4.0",
                "CLIENTS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            fake_metrics = MagicMock()
            run.METRICS = fake_metrics
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}

            def _call_side_effect(_nodes, operation, arguments=None):
                if operation == "set":
                    return True
                if operation == "get":
                    return {"*type/string*": "one two"}
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "randint", return_value=0):
                with patch.object(run, "call", side_effect=_call_side_effect) as mock_call:
                    with patch.object(run, "Thread") as mock_thread:
                        mock_thread.side_effect = lambda target, daemon=True: type(
                            "_InlineThread",
                            (),
                            {"start": lambda self: target(), "join": lambda self: None},
                        )()
                        with self.assertRaises(KeyboardInterrupt):
                            run.run(nodes, edges)

            get_call = next(call for call in mock_call.call_args_list if call.args[1] == "get")
            self.assertEqual(
                get_call.args[2]["path"][-1],
                ["*state*", "data", "key-0"],
            )
            fake_metrics.record_cycle.assert_called()

if __name__ == "__main__":
    unittest.main()
