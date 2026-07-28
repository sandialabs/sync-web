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
        self.ok = True
        self.status_code = 200
        self.text = json.dumps(payload)

    def raise_for_status(self):
        return None

    def json(self):
        return self._payload


class SocialAgentRunTests(unittest.TestCase):
    def test_get_activity_seconds_defaults_empty_to_a_controlled_interval(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "ACTIVITY": ""}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_activity_seconds(), 4.0)

    def test_get_activity_seconds_treats_zero_as_disabled(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "ACTIVITY": "0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_activity_seconds(), 0.0)

    def test_get_size_accepts_zero_and_defaults_when_empty_or_negative(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_size(), 32)

        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_size(), 0)

        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "12"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.get_size(), 12)

    def test_user_names_layout_and_simple_routes(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(run.user_names(0), [])
            self.assertEqual(run.user_names(2), ["alice", "bob"])
            self.assertEqual(len(run.user_names(26)), 26)
            self.assertEqual(run.user_names(27)[-1], "alice-2")
            self.assertEqual([len(bucket["keys"]) for bucket in run.build_user_layout(0)], [0, 0])
            self.assertEqual([len(bucket["keys"]) for bucket in run.build_user_layout(1)], [1, 0])
            self.assertEqual([len(bucket["keys"]) for bucket in run.build_user_layout(4)], [2, 2])
            self.assertEqual(
                [len(bucket["keys"]) for bucket in run.build_user_layout(5)],
                [3, 2],
            )
            adjacency = {
                "journal-0": ["journal-1", "journal-2"],
                "journal-1": ["journal-0", "journal-2"],
                "journal-2": ["journal-0", "journal-1"],
            }
            routes = run.simple_routes(adjacency, "journal-0", 2)
            self.assertIn(["journal-1"], routes)
            self.assertIn(["journal-1", "journal-2"], routes)
            self.assertNotIn(["journal-1", "journal-0"], routes)
            long_bound = run.simple_routes(adjacency, "journal-0", 8)
            self.assertTrue(all(len(route) == len(set(route)) for route in long_bound))
            self.assertTrue(all("journal-0" not in route for route in long_bound))

    def test_identity_creation_is_idempotent_and_uses_valid_fixture_password(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-2"}, clear=False):
            run = _load_run_module()
            missing = _FakeResponse([])
            created = _FakeResponse({"id": "identity"})
            created.status_code = 201
            with patch.object(run.requests, "get", return_value=missing), \
                 patch.object(run.requests, "post", return_value=created) as post:
                run.ensure_local_identity("alice")
            expect_body = {
                "schema_id": "default",
                "traits": {"username": "alice"},
                "credentials": {"password": {"config": {"password": "alice-pass"}}},
            }
            self.assertEqual(run.fixture_password("bob"), "bob-pass")
            self.assertGreaterEqual(len(run.fixture_password("bob")), 8)
            self.assertEqual(post.call_args.kwargs["json"], expect_body)
            self.assertEqual(post.call_args.args[0], "http://identity-provider-2:4434/admin/identities")

            with patch.object(run.requests, "get", return_value=_FakeResponse([{"id": "identity"}])), \
                 patch.object(run.requests, "post") as existing_post:
                run.ensure_local_identity("alice")
            existing_post.assert_not_called()

    def test_route_helpers_build_terminal_principal_and_origin_pin_path(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            self.assertEqual(
                run.route_principal(["journal-1", "journal-2"], "alice"),
                ["journal-1", "journal-2", "*state*", "alice"],
            )
            self.assertEqual(
                run.reverse_access_route("journal-0", ["journal-1", "journal-2"]),
                ["journal-1", "journal-0"],
            )
            self.assertEqual(
                run.local_proof_path(
                    ["journal-1", "journal-0"],
                    ["*state*", "admin", "data", "journal-2:journal-1", "key-0"],
                    12,
                ),
                [12, "journal-1", -1, "journal-0", -1,
                 "*state*", "admin", "data", "journal-2:journal-1", "key-0"],
            )

    def test_users_and_segments_accept_zero_and_configured_values(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0", "USERS": "27", "SEGMENTS": "0"},
            clear=False,
        ):
            run = _load_run_module()
            self.assertEqual(run.get_users(), 27)
            self.assertEqual(run.get_segments(), 0)

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
            {"WORDS": "8", "NODE_NAME": "journal-0"},
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
            {"WORDS": "8", "NODE_NAME": "journal-0"},
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            run.API_TOKEN = "test-token"
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "post", return_value=_FakeResponse(True)) as mock_post:
                run.call(
                    nodes,
                    "bridge",
                    {
                        "name": "journal-1",
                        "interface": {"*type/string*": "http://router-1/api/v1/journal/interface"},
                        "remote-name": "journal-0",
                    },
                )

            headers = mock_post.call_args.kwargs["headers"]
            self.assertEqual(headers["accept"], "application/json")
            self.assertEqual(headers["authorization"], "Bearer test-token")

    def test_call_set_uses_direct_json_arguments(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0"},
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            run.API_TOKEN = "test-token"
            payload = {
                "path": ["*state*", "data", "key-1"],
                "value": {"*type/byte-vector*": "76616c75652d31"},
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
                    "authorization": "Bearer test-token",
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
                "ROUTER_GATEWAY_BASE": "http://router.local/custom/general",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "post", return_value=_FakeResponse({"ok": True})) as mock_post:
                run.call(nodes, "get", {"path": ["*state*", "x"]})

            self.assertEqual(mock_post.call_args.args[0], "http://router.local/custom/general/get")

    def test_call_rewrites_indexed_get_to_resolve(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0"},
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}

            with patch.object(run.requests, "post", return_value=_FakeResponse({"*type/string*": "x"})) as mock_post:
                result = run.call(
                    nodes,
                    "get",
                    {"path": [-1, "*state*", "data", "key-0"]},
                )

            self.assertEqual(result, {"*type/string*": "x"})
            self.assertEqual(
                mock_post.call_args.args[0],
                "http://router-0/api/v1/general/resolve",
            )
            self.assertEqual(
                mock_post.call_args.kwargs["json"],
                {
                    "path": [-1, "*state*", "data", "key-0"],
                    "pinned?": False,
                    "proof?": False,
                },
            )

    def test_metrics_account_for_each_user_independently(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            metrics = run.Metrics()
            metrics.record_cycle("alice", 2, 2)
            metrics.record_cycle("bob", 1, 2)
            snapshot = metrics.snapshot()
            self.assertEqual(snapshot["user_activity"]["alice"], {
                "cycles": 1, "requests": 2, "successes": 2,
            })
            self.assertEqual(snapshot["user_activity"]["bob"]["successes"], 1)

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
            self.assertAlmostEqual(snapshot["activity_request_success_rate"], 80.0)
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
            edges = {"journal-0": [{"node": "journal-1", "mode": "push"}], "journal-1": []}

            with patch.object(run, "acquire_api_token"), patch.object(run, "ensure_local_identity"):
                with patch.object(run, "call", side_effect=[True, True, True, True, KeyboardInterrupt()]) as mock_call:
                    with patch.object(run, "Thread") as mock_thread:
                        mock_thread.side_effect = lambda target, daemon=True: type(
                            "_InlineThread",
                            (),
                            {"start": lambda self: target(), "join": lambda self: None},
                        )()
                        run.run(nodes, edges)

            bridge_call = next(call for call in mock_call.call_args_list if call.args[1] == "bridge")
            self.assertEqual(
                bridge_call.args[2],
                {
                    "name": "journal-1",
                    "interface": {"*type/string*": "http://router-1/api/v1/journal/interface"},
                    "remote-name": "journal-0",
                },
            )

    def test_wait_for_federation_ready_retries_without_activity_metrics(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0"},
            clear=False,
        ):
            run = _load_run_module()
            run.API_TOKEN = "token"
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}
            waiting = MagicMock(ok=False, status_code=400)
            ready = MagicMock(ok=True, status_code=200)

            with patch.object(run.requests, "post", side_effect=[waiting, ready]) as post:
                with patch.object(run.time, "sleep") as sleep:
                    run.wait_for_federation_ready(
                        nodes,
                        [
                            (
                                "alice",
                                ["journal-1"],
                                ["*state*", "alice", "data", "public", "key-0"],
                            )
                        ],
                        {"alice": "token"},
                    )

            self.assertEqual(post.call_count, 2)
            self.assertEqual(
                post.call_args.kwargs["json"],
                {
                    "path": ["*state*", "alice", "data", "public", "key-0"],
                    "$federation": {"route": ["journal-1"]},
                },
            )
            sleep.assert_called_once_with(1)
            run.METRICS.record_request.assert_not_called()

    def test_call_raises_on_http_error_and_records_failure(self):
        with patch.dict(
            os.environ,
            {"WORDS": "8", "NODE_NAME": "journal-0"},
            clear=False,
        ):
            run = _load_run_module()
            fake_metrics = MagicMock()
            run.METRICS = fake_metrics
            nodes = {"journal-0": {"router_host": "router-0"}}

            class _ErrorResponse:
                ok = False
                status_code = 500
                text = "boom"

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

            def _call_side_effect(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation in {"authorize", "set-admins"}:
                    return True
                if operation == "set":
                    set_calls["count"] += 1
                    if set_calls["count"] == 1:
                        return True
                    raise requests.HTTPError("boom")
                if operation == "get":
                    if client_id == "setup":
                        return ["nothing"]
                    return {"*type/byte-vector*": "6f6e652074776f"}
                if operation in {"pin", "unpin"}:
                    raise KeyboardInterrupt()
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token"), patch.object(run, "ensure_local_identity"):
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

    def test_run_disables_continuous_activity_when_activity_is_zero(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SIZE": "0",
                "ACTIVITY": "0",
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
            edges = {"journal-0": [], "journal-1": ["journal-0"]}
            set_calls = {"count": 0}
            authorization_rules = []

            def _call_side_effect(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation == "authorize":
                    authorization_rules.append(arguments["rule"])
                    return True
                if operation == "set-admins":
                    return True
                if operation == "get":
                    return ["nothing"]
                if operation == "set":
                    set_calls["count"] += 1
                    return True
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token"), patch.object(run, "ensure_local_identity"):
                with patch.object(run, "call", side_effect=_call_side_effect):
                    with patch.object(run, "Thread") as mock_thread:
                        run.run(nodes, edges)

            self.assertEqual(set_calls["count"], 0)
            public_rule = next(
                rule for rule in authorization_rules if rule["principal"] == ["*public*"]
            )
            self.assertEqual(public_rule["path"], ["data", "public"])
            self.assertTrue(public_rule["get"])
            self.assertFalse(public_rule["set!"])
            private_rules = [
                rule for rule in authorization_rules if rule["principal"] != ["*public*"]
            ]
            self.assertTrue(private_rules)
            self.assertTrue(all(rule["key-index"] == [0, -1] for rule in private_rules))
            self.assertNotIn(["data"], [rule["path"] for rule in authorization_rules])
            mock_thread.assert_not_called()

    def test_restart_setup_preserves_existing_fixture_values(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SIZE": "2",
                "ACTIVITY": "0",
                "CLIENTS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}

            def _call_side_effect(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation in {"authorize", "set-admins"}:
                    return True
                if operation == "get":
                    return {"*type/byte-vector*": "6f6e652074776f"}
                if operation == "set":
                    raise AssertionError("restart setup must not overwrite an existing fixture key")
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token"), patch.object(run, "ensure_local_identity"):
                with patch.object(run, "call", side_effect=_call_side_effect):
                    run.run(nodes, edges)

    def test_run_accepts_size_zero_without_activity_threads(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "0",
                "ACTIVITY": "4.0", "CLIENTS": "1", "USERS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}

            def _call(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation in {"authorize", "set-admins"}:
                    return True
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token", return_value="token"), \
                 patch.object(run, "ensure_local_identity"), \
                 patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "Thread") as thread:
                run.run(nodes, edges)

            thread.assert_not_called()

if __name__ == "__main__":
    unittest.main()
