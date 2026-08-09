import json
import importlib.util
import os
import requests
import tempfile
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

    def test_get_activity_seconds_treats_zero_as_maximum_throughput(self):
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

    def test_user_names_layout_and_bounded_routes(self):
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
            self.assertEqual(
                run.bounded_routes(adjacency, "journal-0", 2),
                [
                    ["journal-1"],
                    ["journal-1", "journal-0"],
                    ["journal-1", "journal-2"],
                    ["journal-2"],
                    ["journal-2", "journal-0"],
                    ["journal-2", "journal-1"],
                ],
            )
            self.assertEqual(run.bounded_routes(adjacency, "journal-0", 0), [])
            longer = run.bounded_routes(adjacency, "journal-0", 3)
            self.assertIn(["journal-1", "journal-0", "journal-1"], longer)
            self.assertTrue(all(1 <= len(route) <= 3 for route in longer))

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

    def test_batch_is_optional_positive_and_bounded(self):
        with patch.dict(
            os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False
        ):
            os.environ.pop("BATCH", None)
            run = _load_run_module()
            self.assertIsNone(run.get_batch())
        for value, expected in (("1", 1), ("1024", 1024)):
            with patch.dict(
                os.environ,
                {"WORDS": "8", "NODE_NAME": "journal-0", "BATCH": value},
                clear=False,
            ):
                run = _load_run_module()
                self.assertEqual(run.get_batch(), expected)
        for value in ("0", "-1", "1025", "bad"):
            with patch.dict(
                os.environ,
                {"WORDS": "8", "NODE_NAME": "journal-0", "BATCH": value},
                clear=False,
            ):
                run = _load_run_module()
                with self.assertRaises(ValueError):
                    run.get_batch()

    def test_batch_selection_preserves_anchor_route_group_and_unique_paths(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            assignments = [
                {"route": ["journal-1"], "state_path": ["*state*", "alice", "data", "private", f"key-{i}"]}
                for i in range(3)
            ] + [
                {"route": [], "state_path": ["*state*", "alice", "data", "public", "key-0"]}
            ]
            groups = run.batch_groups(assignments)
            with patch.object(run, "choice", return_value=[1]):
                selected = run.select_batch_assignments(assignments, 0, 2, groups)
            self.assertIs(selected[0], assignments[0])
            self.assertEqual([item["route"] for item in selected], [["journal-1"], ["journal-1"]])
            self.assertEqual(len({tuple(item["state_path"]) for item in selected}), 2)
            self.assertTrue(all(item["state_path"][:-1] == assignments[0]["state_path"][:-1] for item in selected))

    def test_ordered_batch_contents_requires_exact_paths_and_order(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            paths = [["*state*", "a"], ["*state*", "b"]]
            values = [run.text_to_byte_vector("one"), run.text_to_byte_vector("two")]
            result = {"results": [
                {"path": paths[0], "content": values[0]},
                {"path": paths[1], "content": values[1]},
            ]}
            self.assertEqual(run.ordered_batch_contents(result, paths), values)
            result["results"].reverse()
            self.assertIsNone(run.ordered_batch_contents(result, paths))
            self.assertIsNone(run.ordered_batch_contents({"results": []}, paths))

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

    def test_call_batch_uses_gateway_batch_endpoint_and_counts_one_request(self):
        with patch.dict(
            os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False
        ):
            run = _load_run_module()
            metrics = MagicMock()
            run.METRICS = metrics
            nodes = {"journal-0": {"router_host": "router-0"}}
            payload = {"paths": [["*state*", "a"], ["*state*", "b"]]}
            response = {"results": []}
            with patch.object(run.requests, "post", return_value=_FakeResponse(response)) as post:
                self.assertEqual(
                    run.call(nodes, "get-batch", payload, token="user-token"), response
                )
            self.assertEqual(
                post.call_args.args[0], "http://router-0/api/v1/general/get-batch"
            )
            self.assertEqual(post.call_args.kwargs["json"], payload)
            self.assertEqual(
                post.call_args.kwargs["headers"]["authorization"], "Bearer user-token"
            )
            metrics.record_request.assert_called_once()
            self.assertEqual(metrics.record_request.call_args.args[0], "get-batch")

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
                "path_operations": 2, "path_operation_successes": 2,
            })
            self.assertEqual(snapshot["user_activity"]["bob"]["successes"], 1)
            metrics.record_cycle("alice", 1, 2, 8, 16)
            snapshot = metrics.snapshot()
            self.assertEqual(snapshot["activity_path_operations_total"], 20)
            self.assertEqual(snapshot["activity_path_operations_success_total"], 11)

    def test_prometheus_metrics_keep_requests_and_paths_separate(self):
        with patch.dict(os.environ, {"WORDS": "8", "NODE_NAME": "journal-0"}, clear=False):
            run = _load_run_module()
            run.METRICS = run.Metrics()
            run.METRICS.record_cycle("alice", 2, 2, 8, 8)
            with tempfile.TemporaryDirectory() as directory:
                run.METRICS_PATH = str(Path(directory) / "social.prom")
                run.write_metrics()
                text = Path(run.METRICS_PATH).read_text(encoding="utf-8")
            self.assertIn("social_agent_activity_requests_success_total 2", text)
            self.assertIn("social_agent_activity_path_operations_success_total 8", text)
            self.assertIn(
                'social_agent_user_activity_path_operations_success_total{user="alice"} 8',
                text,
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
                "activity_path_operations_total": 56,
                "activity_path_operations_success_total": 44,
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
                    "activity_path_operations_total": 32,
                    "activity_path_operations_success_total": 28,
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
            self.assertAlmostEqual(snapshot["activity_path_operations_per_second"], 8.0)
            self.assertAlmostEqual(snapshot["activity_path_operation_success_rate"], 200 / 3)
            self.assertAlmostEqual(snapshot["requests_per_second_lifetime"], 1.5)
            self.assertAlmostEqual(snapshot["activity_path_operations_per_second_lifetime"], 2.2)

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
                with patch.object(run, "call", return_value=True) as mock_call:
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

    def test_run_generates_exact_loop_rules_assignments_and_readiness(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "2",
                "SEGMENTS": "2", "ACTIVITY": "4", "CLIENTS": "1", "USERS": "1",
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
            rules = []
            activity_gets = []

            def _call(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation == "bridge" or operation == "set-admins":
                    return True
                if operation == "authorize":
                    rules.append(arguments["rule"])
                    return True
                if operation == "get" and client_id == "setup":
                    return {"*type/byte-vector*": "6f6e652074776f"}
                if operation == "get":
                    activity_gets.append(arguments)
                    raise KeyboardInterrupt()
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token", return_value="token"), \
                 patch.object(run, "ensure_local_identity"), \
                 patch.object(run, "wait_for_federation_ready") as ready, \
                 patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "Thread") as thread:
                run.run(nodes, edges)

            private_rules = [rule for rule in rules if rule["principal"] != ["*public*"]]
            self.assertEqual(
                [rule["principal"] for rule in private_rules],
                [
                    ["journal-1", "*state*", "alice"],
                    ["journal-1", "journal-0", "*state*", "alice"],
                ],
            )
            self.assertTrue(all(rule["path"] == ["data", "private"] for rule in private_rules))
            expected_path = ["*state*", "alice", "data", "private", "key-1"]
            self.assertEqual(
                ready.call_args.args[1],
                [
                    ("alice", ["journal-1", "journal-0"], expected_path),
                    ("alice", ["journal-1"], expected_path),
                ],
            )

            client_target = thread.call_args.kwargs["target"]
            with patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "randint", return_value=2), \
                 patch.object(run, "choice", return_value=1):
                with self.assertRaises(KeyboardInterrupt):
                    client_target()
            self.assertEqual(
                activity_gets,
                [{
                    "path": expected_path,
                    "$federation": {"route": ["journal-1", "journal-0"]},
                }],
            )

    def test_activity_pin_flow_does_not_pre_resolve_proof(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "1",
                "ACTIVITY": "4", "CLIENTS": "1", "USERS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}
            activity_calls = []

            def _call(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation in {"authorize", "set-admins"}:
                    return True
                if operation == "get" and client_id == "setup":
                    return {"*type/byte-vector*": "6f6e652074776f"}
                activity_calls.append((operation, arguments))
                if operation == "pin":
                    return True
                if operation == "unpin":
                    raise KeyboardInterrupt()
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token", return_value="token"), \
                 patch.object(run, "ensure_local_identity"), \
                 patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "Thread") as thread:
                run.run(nodes, edges)

            client_target = thread.call_args.kwargs["target"]
            with patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "randint", return_value=0), \
                 patch.object(run, "choice", return_value=0):
                with self.assertRaises(KeyboardInterrupt):
                    client_target()
            history_path = [-1, "*state*", "alice", "data", "public", "key-0"]
            self.assertEqual(
                activity_calls,
                [("pin", {"path": history_path}), ("unpin", {"path": history_path})],
            )

    def test_absent_batch_preserves_scalar_get_set_trace(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "1",
                "ACTIVITY": "4", "CLIENTS": "1", "USERS": "1",
            },
            clear=False,
        ):
            os.environ.pop("BATCH", None)
            run = _load_run_module()
            metrics = MagicMock()
            run.METRICS = metrics
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}
            calls = []
            state_path = ["*state*", "alice", "data", "public", "key-0"]

            def _call(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation in {"authorize", "set-admins"}:
                    return True
                if operation == "get" and client_id == "setup":
                    return run.text_to_byte_vector("one two")
                calls.append((operation, arguments))
                if operation == "get":
                    return run.text_to_byte_vector("one two")
                if operation == "set":
                    raise KeyboardInterrupt()
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token", return_value="token"), \
                 patch.object(run, "ensure_local_identity"), \
                 patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "Thread") as thread:
                run.run(nodes, edges)
            client_target = thread.call_args.kwargs["target"]
            with patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "randint", return_value=0), \
                 patch.object(run, "choice", side_effect=[1, "changed"]):
                with self.assertRaises(KeyboardInterrupt):
                    client_target()

            self.assertEqual(
                calls,
                [
                    ("get", {"path": state_path}),
                    ("set", {
                        "path": state_path,
                        "value": run.text_to_byte_vector("changed two"),
                    }),
                ],
            )
            metrics.record_cycle.assert_called_with("alice", 1, 2, 1, 2)

    def test_routed_batch_activity_uses_ordered_get_and_atomic_non_cas_set(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "4",
                "BATCH": "2", "ACTIVITY": "4", "CLIENTS": "1", "USERS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            metrics = MagicMock()
            run.METRICS = metrics
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}
            activity_calls = []
            paths = [
                ["*state*", "alice", "data", "public", "key-0"],
                ["*state*", "alice", "data", "public", "key-1"],
            ]

            def _call(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation in {"authorize", "set-admins"}:
                    return True
                if operation == "get" and client_id == "setup":
                    return run.text_to_byte_vector("one two")
                activity_calls.append((operation, arguments))
                if operation == "get-batch":
                    return {"results": [
                        {"path": path, "content": run.text_to_byte_vector("one two")}
                        for path in paths
                    ]}
                if operation == "set-batch":
                    raise KeyboardInterrupt()
                raise AssertionError(f"Unexpected operation {operation}")

            selected = [
                {"route": ["journal-1", "journal-2"], "state_path": path}
                for path in paths
            ]
            with patch.object(run, "acquire_api_token", return_value="token"), \
                 patch.object(run, "ensure_local_identity"), \
                 patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "Thread") as thread:
                run.run(nodes, edges)

            client_target = thread.call_args.kwargs["target"]
            with patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "select_batch_assignments", return_value=selected), \
                 patch.object(run, "randint", return_value=0), \
                 patch.object(run, "choice", side_effect=lambda source, *args, **kwargs: 1 if source == 2 else "changed"):
                with self.assertRaises(KeyboardInterrupt):
                    client_target()

            federation = {
                "$federation": {"route": ["journal-1", "journal-2"]}
            }
            self.assertEqual(
                activity_calls[0],
                ("get-batch", {"paths": paths, **federation}),
            )
            operation, arguments = activity_calls[1]
            self.assertEqual(operation, "set-batch")
            self.assertEqual(arguments["paths"], paths)
            self.assertEqual(arguments["$federation"], federation["$federation"])
            self.assertNotIn("expected", arguments)
            self.assertEqual(
                [run.byte_vector_text(value) for value in arguments["values"]],
                ["changed two", "changed two"],
            )
            metrics.record_cycle.assert_called_with("alice", 1, 2, 2, 4)
            metrics.record_inferred_hops.assert_called_once_with(
                [("journal-0", "journal-1"), ("journal-1", "journal-2")]
            )

    def test_batch_activity_pins_unique_latest_paths_without_resolve(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "4",
                "BATCH": "2", "ACTIVITY": "4", "CLIENTS": "1", "USERS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            run.METRICS = MagicMock()
            nodes = {"journal-0": {"router_host": "router-0"}}
            edges = {"journal-0": []}
            state_paths = [
                ["*state*", "alice", "data", "private", "key-2"],
                ["*state*", "alice", "data", "private", "key-3"],
            ]
            activity_calls = []

            def _call(_nodes, operation, arguments=None, client_id=None, token=None):
                if operation in {"authorize", "set-admins"}:
                    return True
                if operation == "get" and client_id == "setup":
                    return run.text_to_byte_vector("one two")
                activity_calls.append((operation, arguments))
                if operation == "pin-batch":
                    return True
                if operation == "unpin-batch":
                    raise KeyboardInterrupt()
                raise AssertionError(f"Unexpected operation {operation}")

            selected = [{"route": [], "state_path": path} for path in state_paths]
            with patch.object(run, "acquire_api_token", return_value="token"), \
                 patch.object(run, "ensure_local_identity"), \
                 patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "Thread") as thread:
                run.run(nodes, edges)
            client_target = thread.call_args.kwargs["target"]
            with patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "select_batch_assignments", return_value=selected), \
                 patch.object(run, "randint", return_value=2), \
                 patch.object(run, "choice", return_value=0):
                with self.assertRaises(KeyboardInterrupt):
                    client_target()

            history_paths = [[-1, *path] for path in state_paths]
            self.assertEqual(
                activity_calls,
                [
                    ("pin-batch", {"paths": history_paths}),
                    ("unpin-batch", {"paths": history_paths}),
                ],
            )
            self.assertNotIn("resolve-batch", [operation for operation, _ in activity_calls])

    def test_batch_fails_before_setup_when_access_group_is_too_small(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "4",
                "BATCH": "3", "ACTIVITY": "4", "CLIENTS": "1", "USERS": "1",
            },
            clear=False,
        ):
            run = _load_run_module()
            with patch.object(run, "acquire_api_token") as acquire:
                with self.assertRaisesRegex(ValueError, "route/access-group"):
                    run.run({"journal-0": {"router_host": "router-0"}}, {"journal-0": []})
            acquire.assert_not_called()

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
                if operation == "resolve":
                    return {"proof": [["c", 0, "00"]]}
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

    def test_run_starts_maximum_throughput_activity_when_interval_is_zero(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8", "NODE_NAME": "journal-0", "SIZE": "1",
                "ACTIVITY": "0", "ACTIVITY_DISABLED": "0", "CLIENTS": "1",
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
                if operation == "get":
                    return {"*type/byte-vector*": "6f6e652074776f"}
                raise AssertionError(f"Unexpected operation {operation}")

            with patch.object(run, "acquire_api_token", return_value="token"), \
                 patch.object(run, "ensure_local_identity"), \
                 patch.object(run, "wait_for_federation_ready"), \
                 patch.object(run, "call", side_effect=_call), \
                 patch.object(run, "Thread") as thread:
                run.run(nodes, edges)

            thread.assert_called_once()

    def test_run_disables_continuous_activity_with_explicit_flag(self):
        with patch.dict(
            os.environ,
            {
                "WORDS": "8",
                "NODE_NAME": "journal-0",
                "SIZE": "1",
                "ACTIVITY": "0",
                "ACTIVITY_DISABLED": "1",
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
                    return {"*type/byte-vector*": "6f6e652074776f"}
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
            self.assertEqual(
                [rule["principal"] for rule in private_rules],
                [
                    ["journal-1", "*state*", "alice"],
                    ["journal-1", "journal-0", "*state*", "alice"],
                ],
            )
            self.assertTrue(all(rule["key-index"] == [0, -1] for rule in private_rules))
            self.assertTrue(all(rule["path"] == ["data", "private"] for rule in private_rules))
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
                "ACTIVITY_DISABLED": "1",
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
