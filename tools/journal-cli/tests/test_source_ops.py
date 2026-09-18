#!/usr/bin/python3
import base64
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from journal_cli.source import ops

S = ops.Symbol
IDENTITY = bytes(range(32))
IDENTITY_TEXT = base64.b64encode(IDENTITY).decode()


def entry(key, value):
    return [S(key), value]


def info(identity=IDENTITY):
    return [entry("identity", [entry("id", identity), entry("nonce", bytes(reversed(identity)))])]


def resolve_response(path, content, pinned=None, indexes=None):
    if indexes is None:
        marker = path.index("*state*") if "*state*" in path else len(path)
        indexes = path[:marker:2]
    item = [entry("content", content), entry("indexes", indexes)]
    if pinned is not None:
        item.append(entry("pinned?", pinned))
    return [entry("results", [item])]


class FakeClient:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []
        self.response_limits = []
        self.endpoint = "http://127.0.0.1:8192/interface"

    def observe_public_endpoint(self, expected):
        return expected

    def authentication(self, identity=None):
        return "(authentication ((identity (*state* grace)) (credentials \"redacted\")))"

    def federated_invocation(self, identity, route):
        return "((identity grace) (route-source ()) (route-target (" + " ".join(route) + ")) (credentials \"redacted\"))"

    def post(self, expression, response_limit=ops.MAX_RESPONSE_BYTES):
        self.calls.append(expression)
        self.response_limits.append(response_limit)
        if not self.responses:
            raise AssertionError("unexpected Interface call")
        response = self.responses.pop(0)
        if isinstance(response, Exception):
            raise response
        return response


class SourceOpsTests(unittest.TestCase):
    def setUp(self):
        self.config = {"version": 1, "id": "grace", "sync": {"localInterface": "http://127.0.0.1:8192/interface"}}

    def source(self, responses):
        client = FakeClient(responses)
        return ops.SourceOps(self.config, "/unused", client), client

    def test_scheme_parser_bounds_known_shapes(self):
        value = ops.parse_sexp('((results (((content #u(0 1 255)) (pinned? #t)))) (missing (nothing)))')
        parsed = ops.alist(value, "root")
        self.assertEqual(ops.alist(parsed["results"][0], "item")["content"], b"\x00\x01\xff")
        self.assertEqual(parsed["missing"], [S("nothing")])
        self.assertRaises(ValueError, ops.parse_sexp, "(#u(256))")
        self.assertRaises(ValueError, ops.parse_sexp, "(#t) trailing")

    def test_local_current_resolve_returns_selected_exact_index(self):
        source, client = self.source([resolve_response([5, "*state*", "grace", "head.scm"], b"fixed", True)])
        result = source.resolve_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "owner": "grace", "view": {"kind": "current"},
                                      "paths": [["head.scm"]], "includePinned": True})
        self.assertEqual(result["outcome"], "retrieved")
        self.assertEqual(result["view"]["selectedIndex"], 5)
        self.assertEqual(result["results"][0]["canonicalCommittedPath"], [5, "*state*", "grace", "head.scm"])
        self.assertEqual(result["results"][0]["contentBase64"], base64.b64encode(b"fixed").decode())
        self.assertTrue(result["results"][0]["pinned"])
        self.assertIn("(paths ((-1 *state* grace head.scm)))", client.calls[-1])
        self.assertIn("(index? #t)", client.calls[-1])

    def test_remote_current_is_frozen_into_full_exact_path(self):
        roots1 = [IDENTITY, bytes([40]) * 32]
        roots2 = [IDENTITY, bytes([40]) * 32, bytes([50]) * 32]
        route1 = [entry("terminal-index", 20), entry("roots", roots1)]
        route2 = [entry("terminal-index", 30), entry("roots", roots2)]
        canonical = [8, "galactica", 20, "ester", 30, "*state*", "ester", "head.scm"]
        source, client = self.source([resolve_response(canonical, b"head")])
        result = source.resolve_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": ["galactica", "ester"], "owner": "ester",
                                      "view": {"kind": "current"}, "paths": [["head.scm"]]})
        self.assertEqual(result["view"]["selectedIndex"], 30)
        self.assertEqual(result["view"]["historyIndexes"], [8, 20, 30])
        self.assertEqual(result["view"]["terminalEndpoint"], "http://127.0.0.1:8192/interface")
        self.assertEqual(result["results"][0]["canonicalCommittedPath"], canonical)
        self.assertEqual(len(client.calls), 1)
        self.assertIn("(-1 galactica -1 ester -1 *state* ester head.scm)", client.calls[0])

    def test_exact_fixed_route_mismatch_is_rejected(self):
        route1 = [entry("terminal-index", 20), entry("roots", [IDENTITY, bytes([40]) * 32])]
        mismatch = [entry("history-index", 31), entry("roots", [IDENTITY, bytes([40]) * 32, bytes([50]) * 32])]
        source, _ = self.source([ops.ExplicitReject("fixed-index-mismatch", "fixed route unavailable")])
        with self.assertRaises(ops.ExplicitReject) as caught:
            source.resolve_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": ["galactica", "ester"], "owner": "ester",
                                 "view": {"kind": "fixed", "index": 30, "historyIndexes": [9, 20, 30]}, "paths": [["release.scm"]]})
        self.assertEqual(caught.exception.code, "fixed-index-mismatch")

    def test_projected_directory_is_explicitly_incomplete(self):
        directory = [S("directory"), [[S("release.scm"), S("value")]], False]
        path = [4, "*state*", "ester", "releases"]
        source, _ = self.source([resolve_response(path, directory, True)])
        result = source.resolve_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "owner": "ester", "view": {"kind": "fixed", "index": 4, "historyIndexes": [4]},
                                      "paths": [["releases"]], "includePinned": True})
        self.assertEqual(result["results"][0]["shape"], "directory")
        self.assertFalse(result["results"][0]["complete"])
        self.assertTrue(result["results"][0]["pinned"])

    def test_create_value_uses_expected_nothing_and_readback(self):
        with tempfile.TemporaryDirectory() as directory:
            value = Path(directory) / "value"
            value.write_bytes(b"opaque\x00bytes")
            source, client = self.source([True, b"opaque\x00bytes"])
            result = source.create_value({"version": 2, "owner": "grace", "path": ["chunk"], "input": str(value),
                                          "expected": "absent", "readback": True})
        self.assertEqual(result["outcome"], "accepted")
        self.assertEqual(result["readback"]["outcome"], "retrieved")
        self.assertIn("(expected (nothing))", client.calls[0])
        self.assertIn("(expression? #f)", client.calls[0])

    def test_create_conflict_is_explicit_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            value = Path(directory) / "value"
            value.write_bytes(b"x")
            source, _ = self.source([False])
            with self.assertRaises(ops.ExplicitReject) as caught:
                source.create_value({"version": 2, "owner": "grace", "path": ["chunk"], "input": str(value),
                                     "expected": "absent"})
        self.assertEqual(caught.exception.code, "conflict")

    def test_create_ambiguous_is_not_retried(self):
        with tempfile.TemporaryDirectory() as directory:
            value = Path(directory) / "value"
            value.write_bytes(b"x")
            source, client = self.source([ops.AmbiguousFailure("lost response")])
            with self.assertRaises(ops.AmbiguousFailure):
                source.create_value({"version": 2, "owner": "grace", "path": ["chunk"], "input": str(value),
                                     "expected": "absent"})
        self.assertEqual(len(client.calls), 1)

    def test_scalar_pin_returns_exact_handle_without_completeness_claim(self):
        source, client = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), True])
        request = {"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7, "paths": [["release", "chunk"]],
                   "proofEncodedBytesUpperBound": 10000}
        result = source.pin_view(request)
        self.assertEqual(result["outcome"], "accepted")
        self.assertEqual(result["completeness"], "unverified")
        handle = result["handles"][0]
        self.assertEqual(handle["canonicalCommittedPath"], [7, "*state*", "grace", "release", "chunk"])
        self.assertIn("pin-batch!", client.calls[-1])

    def test_bounded_pin_preserves_all_exact_handles(self):
        source, _ = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), True])
        paths = [[f"c{index}"] for index in range(4)]
        result = source.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7, "paths": paths,
                                  "proofEncodedBytesUpperBound": ops.MAX_RESPONSE_BYTES})
        self.assertEqual([item["path"] for item in result["handles"]], paths)

    def test_oversized_root_pin_fails_before_any_dispatch(self):
        source, client = self.source([])
        with self.assertRaises(ops.RequestError) as caught:
            source.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7,
                             "paths": [["release-root"]], "proofEncodedBytesUpperBound": ops.MAX_RESPONSE_BYTES + 1})
        self.assertEqual(caught.exception.code, "unsupported-capability")
        self.assertEqual(client.calls, [])

    def test_pin_ambiguous_is_one_attempt_and_returns_no_handle(self):
        source, client = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), ops.AmbiguousFailure("lost pin response")])
        with self.assertRaises(ops.AmbiguousFailure):
            source.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7, "paths": [["chunk"]],
                             "proofEncodedBytesUpperBound": 1000})
        self.assertEqual(len([call for call in client.calls if "pin-batch!" in call]), 1)

    def test_cross_batch_partial_keeps_only_established_handles(self):
        first, _ = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), True])
        accepted = first.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7,
                                   "paths": [["chunk-a"]], "proofEncodedBytesUpperBound": 1000})
        second, client = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), ops.AmbiguousFailure("lost second response")])
        with self.assertRaises(ops.AmbiguousFailure):
            second.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7,
                             "paths": [["chunk-b"]], "proofEncodedBytesUpperBound": 1000})
        self.assertEqual(len(accepted["handles"]), 1)
        self.assertEqual(accepted["completeness"], "unverified")
        self.assertEqual(len([call for call in client.calls if "pin-batch!" in call]), 1)

    def test_exact_unpin_handle_does_not_rediscover_route(self):
        pin_source, _ = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), True])
        handle = pin_source.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7,
                                      "paths": [["chunk"]], "proofEncodedBytesUpperBound": 1000})["handles"][0]
        unpin_source, client = self.source([True])
        result = unpin_source.unpin_retention({"version": 2, "handles": [handle]})
        self.assertEqual(result["outcome"], "accepted")
        self.assertEqual(len(client.calls), 1)
        self.assertIn("unpin-batch!", client.calls[0])
        self.assertNotIn("function route", client.calls[0])

    def test_tampered_unpin_handle_fails_before_dispatch(self):
        pin_source, _ = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), True])
        handle = pin_source.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7,
                                      "paths": [["chunk"]], "proofEncodedBytesUpperBound": 1000})["handles"][0]
        handle["publisherIndex"] = 8
        unpin_source, client = self.source([])
        with self.assertRaises(ops.RequestError):
            unpin_source.unpin_retention({"version": 2, "handles": [handle]})
        self.assertEqual(client.calls, [])

    def test_rehashed_inconsistent_handle_still_fails_before_dispatch(self):
        pin_source, _ = self.source([resolve_response([7, "*state*", "grace", "chunk"], b"x"), True])
        handle = pin_source.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "historyIndexes": [7], "owner": "grace", "index": 7,
                                      "paths": [["chunk"]], "proofEncodedBytesUpperBound": 1000})["handles"][0]
        handle["publisherIndex"] = 8
        body = dict(handle)
        body.pop("handleSha256")
        handle["handleSha256"] = ops.hashlib.sha256(ops.canonical_json(body)).hexdigest()
        unpin_source, client = self.source([])
        with self.assertRaises(ops.RequestError):
            unpin_source.unpin_retention({"version": 2, "handles": [handle]})
        self.assertEqual(client.calls, [])

    def test_run_command_maps_primary_outcomes_and_exit_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = root / "config.json"
            config.write_text(json.dumps(self.config))
            request = root / "request.json"
            request.write_text(json.dumps({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "owner": "grace", "view": {"kind": "current"},
                                           "paths": [["head.scm"]]}))
            client = FakeClient([resolve_response([1, "*state*", "grace", "head.scm"], b"x")])
            result = ops.run_command("resolve-view", str(request), str(config), "/unused", client)
            self.assertEqual(result["outcome"], "retrieved")
            bad = root / "bad.json"
            bad.write_text('{"version":1,"version":1}')
            result = ops.run_command("resolve-view", str(bad), str(config), "/unused", FakeClient([]))
            self.assertEqual(result["outcome"], "not-attempted")
            nonfinite = root / "nonfinite.json"
            nonfinite.write_text('{"version":1,"route":[],"owner":"grace","view":{"kind":"fixed","index":NaN},"paths":[["x"]]}')
            result = ops.run_command("resolve-view", str(nonfinite), str(config), "/unused", FakeClient([]))
            self.assertEqual(result["outcome"], "not-attempted")
            client = FakeClient([2, ops.AmbiguousFailure("lost resolve response")])
            result = ops.run_command("resolve-view", str(request), str(config), "/unused", client)
            self.assertEqual(result["outcome"], "failed-or-ambiguous")
            self.assertEqual(len([call for call in client.calls if "retrieve-batch" in call]), 1)
            malformed = FakeClient([2, b"wrong-shape"])
            result = ops.run_command("resolve-view", str(request), str(config), "/unused", malformed)
            self.assertEqual(result["outcome"], "failed-or-ambiguous")
            self.assertEqual(result["error"]["code"], "malformed-response")

    def test_get_current_result_contract_distinguishes_terminal_classes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = root / "config.json"
            config.write_text(json.dumps(self.config))
            request = root / "request.json"
            request.write_text(json.dumps({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "owner": "grace",
                                           "reads": [{"path": ["marker.scm"], "maxBytes": 10}],
                                           "rawResponseBytesUpperBound": 70000, "responseBytesUpperBound": 70000}))
            nothing = [[S("nothing")]]
            result = ops.run_command("get-current", str(request), str(config), "/unused", FakeClient([nothing]))
            self.assertEqual(result["outcome"], "retrieved")
            self.assertEqual(result["results"][0]["shape"], "nothing")
            result = ops.run_command("get-current", str(request), str(config), "/unused",
                                     FakeClient([ops.ExplicitReject("interface-rejected", "denied")]))
            self.assertEqual((result["outcome"], result["error"]["code"]),
                             ("rejected", "interface-rejected"))
            result = ops.run_command("get-current", str(request), str(config), "/unused",
                                     FakeClient([ops.AmbiguousFailure("transport")]))
            self.assertEqual((result["outcome"], result["error"]["code"]),
                             ("failed-or-ambiguous", "completion-unestablished"))
            result = ops.run_command("get-current", str(request), str(config), "/unused",
                                     FakeClient([[b"x", b"extra"]]))
            self.assertEqual((result["outcome"], result["error"]["code"]),
                             ("failed-or-ambiguous", "malformed-response"))
            self.assertEqual(result["error"]["message"], "use-batch! result cardinality mismatch")

    def test_released_s7_symbol_lexical_boundary_fails_closed(self):
        self.assertEqual(ops.scheme_symbol("galactica"), "galactica")
        self.assertEqual(ops.scheme_symbol("head.scm"), "head.scm")
        self.assertEqual(ops.scheme_symbol("6b1bd0e2-54e5-4779-bd88-1d550c860d87"),
                         "6b1bd0e2-54e5-4779-bd88-1d550c860d87")
        self.assertEqual(ops.scheme_path([7, "*state*", "grace", "head.scm"], {1}),
                         "(7 *state* grace head.scm)")
        for value in ("123", "1.0", "1e3", "+inf.0", "-nan.0", ".", "..",
                      "#t", "galactica)", "a b", "|galactica|", "a\\b", "a;b"):
            with self.subTest(value=value):
                with self.assertRaises(ops.RequestError) as caught:
                    ops.scheme_symbol(value)
                self.assertEqual(caught.exception.code, "unsupported-capability")
        source, client = self.source([])
        with self.assertRaises(ops.RequestError) as caught:
            source.resolve_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": ["123"], "owner": "grace",
                                 "view": {"kind": "current"}, "paths": [["head.scm"]]})
        self.assertEqual(caught.exception.code, "unsupported-capability")
        self.assertEqual(client.calls, [])

    def test_reserved_implementation_symbols_fail_before_dispatch_across_five_commands(self):
        reserved = {
            "*admins-get*", "*admins-set*", "*autoload-hook*", "*bridge*", "*call*", "*crypto*",
            "*error-hook*", "*init*", "*journal*", "*load-hook*", "*missing-close-paren-hook*",
            "*periodic*", "*public*", "*read-error-hook*", "*removed*", "*rootlet-redefinition-hook*",
            "*s7*", "*secret*", "*set-query*", "*set-step*", "*state*", "*step*", "*sync-state*",
            "*time*", "*transition*", "*unbound-variable-hook*", "*window-set*",
        }
        self.assertEqual(ops.RESERVED_S7_SYMBOLS, reserved)
        for token in sorted(reserved):
            with self.subTest(operation="get-current", token=token):
                source, client = self.source([])
                with self.assertRaises(ops.RequestError) as caught:
                    source.get_current({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [token], "owner": "grace",
                                        "reads": [{"path": ["head.scm"], "maxBytes": 10}],
                                        "rawResponseBytesUpperBound": 70000, "responseBytesUpperBound": 70000})
                self.assertEqual(caught.exception.code, "unsupported-capability")
                self.assertEqual(client.calls, [])
            with self.subTest(operation="resolve-view", token=token):
                source, client = self.source([])
                with self.assertRaises(ops.RequestError) as caught:
                    source.resolve_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [token], "owner": "grace",
                                         "view": {"kind": "current"}, "paths": [["head.scm"]]})
                self.assertEqual(caught.exception.code, "unsupported-capability")
                self.assertEqual(client.calls, [])
            with self.subTest(operation="create-value", token=token):
                source, client = self.source([])
                with self.assertRaises(ops.RequestError) as caught:
                    source.create_value({"version": 2, "owner": "grace", "path": [token],
                                         "input": "/not-opened", "expected": "absent"})
                self.assertEqual(caught.exception.code, "unsupported-capability")
                self.assertEqual(client.calls, [])
            with self.subTest(operation="pin-view", token=token):
                source, client = self.source([])
                with self.assertRaises(ops.RequestError) as caught:
                    source.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [token], "historyIndexes": [8, 7], "owner": "grace", "index": 7,
                                     "paths": [["chunk"]], "proofEncodedBytesUpperBound": 1000})
                self.assertEqual(caught.exception.code, "unsupported-capability")
                self.assertEqual(client.calls, [])
            with self.subTest(operation="unpin-retention", token=token):
                source, client = self.source([])
                body = {"handleVersion": 2, "originOwner": "grace", "entryEndpoint": "http://127.0.0.1:8192/interface", "publisherEndpoint": "http://127.0.0.1:8192/interface", "publisherOwner": "grace",
                        "routeObservation": [token], "historyIndexes": [8, 7], "publisherIndex": 7, "path": ["chunk"],
                        "canonicalCommittedPath": [8, token, 7, "*state*", "grace", "chunk"]}
                handle = dict(body)
                handle["handleSha256"] = ops.hashlib.sha256(ops.canonical_json(body)).hexdigest()
                with self.assertRaises(ops.RequestError) as caught:
                    source.unpin_retention({"version": 2, "handles": [handle]})
                self.assertEqual(caught.exception.code, "unsupported-capability")
                self.assertEqual(client.calls, [])

    def test_supported_external_symbols_are_bare_across_five_commands(self):
        route = ["galactica", "6b1bd0e2-54e5-4779-bd88-1d550c860d87"]
        relative = ["objects", "tree", "o00000001"]
        indexes = [9, 10, 11, 12, 13, 14]
        route_responses = [
            [entry("terminal-index", indexes[position + 1]),
             entry("roots", [bytes([position + 1]) * 32])]
            for position in range(len(route))
        ]
        canonical = [indexes[0]]
        for alias, index in zip(route, indexes[1:]):
            canonical.extend([alias, index])
        canonical.extend(["*state*", "grace", *relative])

        getter, get_client = self.source([[b"x"]])
        getter.get_current({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": route, "owner": "grace",
                            "reads": [{"path": relative, "maxBytes": 10}],
                            "rawResponseBytesUpperBound": 70000, "responseBytesUpperBound": 70000})
        get_text = get_client.calls[-1]
        for value in route + relative + ["grace"]:
            self.assertIn(value, get_text)
        self.assertNotIn("|", get_text)
        self.assertIn("(function use-batch!)", get_text)
        self.assertNotIn("resolve", get_text)
        self.assertEqual(len(get_client.calls), 1)

        resolver, resolve_client = self.source([resolve_response(canonical, b"x")])
        resolver.resolve_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": route, "owner": "grace",
                               "view": {"kind": "current"}, "paths": [relative]})
        resolve_text = "\n".join(resolve_client.calls)
        for value in route + relative + ["grace"]:
            self.assertIn(value, resolve_text)
        self.assertIn("*state*", resolve_client.calls[-1])
        self.assertNotIn("|", resolve_text)
        self.assertIn("(function retrieve-batch)", resolve_client.calls[-1])
        self.assertNotIn("|resolve-batch|", resolve_client.calls[-1])

        with tempfile.TemporaryDirectory() as directory:
            value_file = Path(directory) / "value"
            value_file.write_bytes(b"x")
            creator, create_client = self.source([True])
            creator.create_value({"version": 2, "owner": "grace", "path": relative,
                                  "input": str(value_file), "expected": "absent"})
        for value in relative + ["grace"]:
            self.assertIn(value, create_client.calls[-1])
        self.assertNotIn("|", create_client.calls[-1])

        fixed_route_responses = route_responses[:-1] + [
            [entry("history-index", 14), entry("roots", [bytes([5]) * 32])]
        ]
        pinner, pin_client = self.source([resolve_response(canonical, b"x", indexes=[9, 10, 14]), True])
        pin_result = pinner.pin_view({"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": route, "historyIndexes": [9, 10, 14], "owner": "grace", "index": 14,
                                      "paths": [relative], "proofEncodedBytesUpperBound": 1000})
        pin_text = "\n".join(pin_client.calls)
        for value in route + relative + ["grace"]:
            self.assertIn(value, pin_text)
        self.assertNotIn("|", pin_text)

        unpinner, unpin_client = self.source([True])
        unpinner.unpin_retention({"version": 2, "handles": pin_result["handles"]})
        for value in route + relative + ["grace"]:
            self.assertIn(value, unpin_client.calls[-1])
        self.assertNotIn("|", unpin_client.calls[-1])

    def current_request(self, reads, raw=70000, encoded=70000, route=None, owner="grace"):
        return {"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": route or [], "owner": owner, "reads": reads,
                "rawResponseBytesUpperBound": raw, "responseBytesUpperBound": encoded}

    def test_get_current_batches_once_without_identity_probe(self):
        first = b"marker"
        second = b"descriptor"
        staged = [["*state*", "hermione", "release.scm"],
                  ["*state*", "hermione", "descriptor.scm"]]
        response = [first, second]
        source, client = self.source([response])
        request = self.current_request([
            {"path": ["release.scm"], "maxBytes": 64,
             "expected": {"bytes": len(first), "sha256": ops.hashlib.sha256(first).hexdigest()}},
            {"path": ["descriptor.scm"], "maxBytes": 64},
        ], raw=80000, encoded=80000, route=["galactica", "hermione"], owner="hermione")
        result = source.get_current(request)
        self.assertEqual(len(client.calls), 1)
        self.assertEqual([item["path"] for item in result["results"]],
                         [["release.scm"], ["descriptor.scm"]])
        self.assertEqual(result["results"][0]["sha256"], ops.hashlib.sha256(first).hexdigest())
        evidence = result["currentEvidence"]
        self.assertEqual(evidence["requestedRoute"], ["galactica", "hermione"])
        self.assertEqual(evidence["requestedOwner"], "hermione")
        self.assertIsNone(evidence["contentObservedLocator"])
        self.assertIsNone(evidence["contentObservedIndex"])
        self.assertFalse(evidence["committed"])
        self.assertEqual((evidence["routeContinuityObservations"], evidence["useBatchDispatches"]), (0, 1))
        self.assertNotIn("observedTerminalJournal", evidence)
        self.assertNotIn("expectedTerminalJournal", evidence)
        call = client.calls[0]
        self.assertIn("(function use-batch!)", call)
        self.assertIn("(expression? #f)", call)
        self.assertIn("(route-target (galactica hermione))", call)
        self.assertNotIn("(function route)", call)
        self.assertNotIn("(function info)", call)
        self.assertNotIn("resolve", call)
        self.assertEqual(client.response_limits, [80000])

    def test_get_current_nothing_is_ordered_and_local(self):
        response = [[S("nothing")]]
        source, client = self.source([response])
        result = source.get_current(self.current_request([{"path": ["marker.scm"], "maxBytes": 128}]))
        self.assertEqual(result["results"], [{"path": ["marker.scm"], "shape": "nothing"}])
        self.assertEqual(len(client.calls), 1)
        self.assertIn("(authentication ", client.calls[0])
        self.assertNotIn("(invocation ", client.calls[0])

    def test_get_current_mismatch_and_malformed_never_retry(self):
        staged = ["*state*", "grace", "chunk"]
        source, client = self.source([[b"wrong"]])
        with self.assertRaises(ops.ExplicitReject) as caught:
            source.get_current(self.current_request([{"path": ["chunk"], "maxBytes": 10,
                                                               "expected": {"bytes": 5, "sha256": "0" * 64}}]))
        self.assertEqual(caught.exception.code, "content-mismatch")
        self.assertEqual(len(client.calls), 1)
        source, client = self.source([[]])
        with self.assertRaises(ValueError):
            source.get_current(self.current_request([{"path": ["chunk"], "maxBytes": 10}]))
        self.assertEqual(len(client.calls), 1)

    def test_get_current_exact_1024_small_paths_is_one_batch(self):
        reads = [{"path": [f"c{index}"], "maxBytes": 0} for index in range(ops.MAX_PATHS)]
        items = [[S("nothing")] for _index in range(ops.MAX_PATHS)]
        source, client = self.source([items])
        request = self.current_request(reads, raw=ops.MAX_RESPONSE_BYTES, encoded=ops.MAX_RESPONSE_BYTES)
        result = source.get_current(request)
        self.assertEqual(len(result["results"]), ops.MAX_PATHS)
        self.assertEqual(len(client.calls), 1)
        self.assertIn("(function use-batch!)", client.calls[0])

    def test_raw_vector_bound_is_exact_and_separate_from_encoded(self):
        self.assertEqual(ops.released_s7_byte_vector_bound(0), 4)
        self.assertEqual(ops.released_s7_byte_vector_bound(ops.MAX_VALUE_BYTES),
                         4 * ops.MAX_VALUE_BYTES + 3)
        two = [{"path": [f"x{index}"], "maxBytes": ops.MAX_VALUE_BYTES} for index in range(2)]
        raw, encoded = ops.current_response_bound_proofs("grace", two)
        self.assertGreater(raw, ops.MAX_RESPONSE_BYTES)
        self.assertLess(encoded, ops.MAX_RESPONSE_BYTES)
        source, client = self.source([])
        with self.assertRaises(ops.RequestError) as caught:
            source.get_current(self.current_request(two, raw=ops.MAX_RESPONSE_BYTES,
                                                    encoded=ops.MAX_RESPONSE_BYTES))
        self.assertEqual(caught.exception.code, "unsupported-capability")
        self.assertEqual(client.calls, [])

    def test_all_255_maximum_vector_and_actual_caps(self):
        content = bytes([255]) * ops.MAX_VALUE_BYTES
        read = {"path": ["maximum"], "maxBytes": len(content)}
        raw, encoded = ops.current_response_bound_proofs("grace", [read])
        source, client = self.source([[content]])
        result = source.get_current(self.current_request([read], raw=raw, encoded=encoded))
        self.assertEqual(result["results"][0]["bytes"], ops.MAX_VALUE_BYTES)
        self.assertEqual(client.response_limits, [raw])

        class OversizedResponse:
            def __enter__(self):
                return self
            def __exit__(self, *unused):
                return False
            def read(self, count):
                return b" " * count

        interface = ops.InterfaceClient("http://local/interface", "redacted")
        with mock.patch.object(ops.URL_OPENER, "open", return_value=OversizedResponse()):
            with self.assertRaises(ops.AmbiguousFailure):
                interface.post("((function get-batch))", response_limit=10)

        small = {"path": ["small"], "maxBytes": 1}
        response = [b"x"]
        source, _ = self.source([response])
        with mock.patch.object(ops, "canonical_json", return_value=b"x" * 70001):
            with self.assertRaises(ops.AmbiguousFailure):
                source.get_current(self.current_request([small]))

    def test_largest_global_batch_passes_and_first_prefix_rejects(self):
        reads = []
        while True:
            candidate = reads + [{"path": [f"chunk{len(reads)}"], "maxBytes": 65536}]
            raw, encoded = ops.current_response_bound_proofs("grace", candidate)
            if raw > ops.MAX_RESPONSE_BYTES or encoded > ops.MAX_RESPONSE_BYTES:
                rejected = candidate
                break
            reads = candidate
        self.assertGreater(len(reads), 0)
        items = [b"" for _index in range(len(reads))]
        source, client = self.source([items])
        result = source.get_current(self.current_request(
            reads, raw=ops.MAX_RESPONSE_BYTES, encoded=ops.MAX_RESPONSE_BYTES))
        self.assertEqual(len(result["results"]), len(reads))
        self.assertEqual(len(client.calls), 1)
        source, client = self.source([])
        with self.assertRaises(ops.RequestError) as caught:
            source.get_current(self.current_request(
                rejected, raw=ops.MAX_RESPONSE_BYTES, encoded=ops.MAX_RESPONSE_BYTES))
        self.assertEqual(caught.exception.code, "unsupported-capability")
        self.assertEqual(client.calls, [])

    def test_declared_first_reject_includes_wrapper_path_overhead(self):
        read = {"path": ["p" * 128, "q" * 128], "maxBytes": 4096}
        raw, encoded = ops.current_response_bound_proofs("grace", [read])
        response = [bytes([255]) * read["maxBytes"]]
        source, client = self.source([response])
        result = source.get_current(self.current_request([read], raw=raw, encoded=encoded))
        self.assertEqual(result["results"][0]["bytes"], 4096)
        self.assertEqual(client.response_limits, [raw])
        for field, value in (("raw", raw - 1), ("encoded", encoded - 1)):
            source, client = self.source([])
            request = self.current_request([read], raw=raw, encoded=encoded)
            request["rawResponseBytesUpperBound" if field == "raw" else "responseBytesUpperBound"] = value
            with self.assertRaises(ops.RequestError) as caught:
                source.get_current(request)
            self.assertEqual(caught.exception.code, "unsupported-capability")
            self.assertEqual(client.calls, [])

    def test_bound_overflow_and_invalid_inputs_fail_before_dispatch(self):
        with self.assertRaises(ops.RequestError):
            ops.checked_bound_add(ops.MAX_BOUND_INTEGER, 1)
        with self.assertRaises(ops.RequestError):
            ops.checked_bound_multiply(ops.MAX_BOUND_INTEGER, 2)
        invalid = [
            {"version": 2, "endpoint": "http://127.0.0.1:8192/interface", "route": [], "owner": "grace",
             "reads": [{"path": ["chunk"], "maxBytes": 0} for _ in range(ops.MAX_PATHS + 1)],
             "rawResponseBytesUpperBound": ops.MAX_RESPONSE_BYTES, "responseBytesUpperBound": ops.MAX_RESPONSE_BYTES},
            self.current_request([]),
            self.current_request([{"path": ["duplicate"], "maxBytes": 0},
                                  {"path": ["duplicate"], "maxBytes": 0}]),
            self.current_request([{"path": ["chunk"], "maxBytes": ops.MAX_VALUE_BYTES + 1}]),
            self.current_request([{"path": ["chunk"], "maxBytes": 10,
                                   "expected": {"bytes": 11, "sha256": "0" * 64}}]),
            {**self.current_request([{"path": ["chunk"], "maxBytes": 10}]),
             "expectedTerminalJournal": IDENTITY_TEXT},
        ]
        for request in invalid:
            source, client = self.source([])
            with self.assertRaises(ops.RequestError):
                source.get_current(request)
            self.assertEqual(client.calls, [])

    def test_get_current_value_bound_and_ordered_path_fail_once(self):
        source, client = self.source([[b"too-long"]])
        with self.assertRaises(ops.AmbiguousFailure):
            source.get_current(self.current_request([{"path": ["chunk"], "maxBytes": 2}]))
        self.assertEqual(len(client.calls), 1)
        source, client = self.source([[b"x", b"extra"]])
        with self.assertRaises(ValueError):
            source.get_current(self.current_request([{"path": ["chunk"], "maxBytes": 2}]))
        self.assertEqual(len(client.calls), 1)

    def test_cli_exposes_exact_five_commands_and_no_shell_surface(self):
        source_path = Path(ops.__file__)
        result = subprocess.run([sys.executable, str(source_path), "--help"], check=True, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        command_line = next(line for line in result.stdout.splitlines()
                            if line.strip().startswith("{get-current,") and not line.strip().endswith("..."))
        self.assertEqual(command_line.strip(),
                         "{get-current,resolve-view,create-value,pin-view,unpin-retention}")
        source = source_path.read_text()
        for forbidden in ("import subprocess", "os.system(", "shell=True", "eval(", "exec("):
            self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
