#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("sticky_readiness.py")
SPEC = importlib.util.spec_from_file_location("sticky_readiness", MODULE_PATH)
assert SPEC and SPEC.loader
sticky = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = sticky
SPEC.loader.exec_module(sticky)


PEERS = {
    "nodes": {f"journal-{index}": {"router_host": f"router-{index}"} for index in range(4)},
    "edges": {
        "journal-0": [{"node": "journal-2"}, {"node": "journal-3"}, {"node": "journal-1"}],
        "journal-1": [{"node": "journal-3"}],
        "journal-2": [{"node": "journal-3"}],
        "journal-3": [],
    },
}


class StickyReadinessUnitTests(unittest.TestCase):
    def test_routes_match_four_node_qualification_topology(self) -> None:
        routes = sticky.readiness_routes(PEERS, 2)
        self.assertEqual(len(routes), 26)
        self.assertEqual(len(set(routes)), 26)
        self.assertIn(sticky.Route("journal-3", ("journal-1", "journal-0")), routes)
        self.assertIn(sticky.Route("journal-3", ("journal-2",)), routes)
        self.assertNotIn(sticky.Route("journal-0", ("journal-2", "journal-0")), routes)

    def test_proof_path_has_an_index_for_origin_and_each_hop(self) -> None:
        route = sticky.Route("journal-3", ("journal-1", "journal-0"))
        self.assertEqual(
            sticky.proof_path(route, ["*state*", "alice", "data", "private", "key-4"]),
            [-1, "journal-1", -1, "journal-0", -1, "*state*", "alice", "data", "private", "key-4"],
        )

    def test_structural_proof_requires_typed_content_root_and_closed_refs(self) -> None:
        body = {
            "content": {"*type/byte-vector*": "00ff"},
            "pinned?": False,
            "proof": {"n-1": ["n-2", "n-0"], "n-2": ["c", "n-0", "n-0"]},
        }
        checked = sticky.structural_proof_check(body)
        self.assertTrue(checked["ok"])
        self.assertEqual(checked["proof_nodes"], 2)
        self.assertEqual(checked["missing_refs"], [])
        self.assertIsNotNone(checked["proof_sha256"])
        self.assertIsNotNone(checked["root_node_sha256"])

        body["proof"]["n-1"] = ["n-3", "n-0"]
        checked = sticky.structural_proof_check(body)
        self.assertFalse(checked["ok"])
        self.assertEqual(checked["missing_refs"], ["n-3"])

    def test_post_ready_failure_is_terminal_and_cannot_later_pass(self) -> None:
        self.assertEqual(sticky.readiness_transition(False, True), "retry")
        self.assertEqual(sticky.readiness_transition(False, False), "ready")
        self.assertEqual(sticky.readiness_transition(True, False), "continue")
        self.assertEqual(sticky.readiness_transition(True, True), "terminal")

    def test_gateway_failure_counter_ignores_success(self) -> None:
        metrics = "\n".join(
            (
                'sync_gateway_journal_requests_total{function="get",result="success"} 11',
                'sync_gateway_journal_requests_total{function="get",result="error"} 2',
                'sync_gateway_journal_requests_total{function="resolve",result="error"} 3',
            )
        )
        self.assertEqual(sticky.metric_failures(metrics), 5)

    def test_frozen_serializer_oracle_has_base_fail_candidate_pass(self) -> None:
        provenance = json.loads(MODULE_PATH.with_name("sticky-oracle-provenance.json").read_text())
        raw = MODULE_PATH.with_name("sticky-oracle-raw.txt").read_bytes()
        probe = MODULE_PATH.with_name("sticky-oracle-probe.scm").read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(), provenance["serializer_oracle"]["raw_output_sha256"])
        self.assertEqual(hashlib.sha256(probe).hexdigest(), provenance["serializer_oracle"]["probe_sha256"])
        parsed = sticky.parse_serializer_oracle(raw.decode())
        self.assertEqual(provenance["base"]["result"], "fail")
        self.assertEqual(provenance["candidate"]["result"], "pass")
        self.assertEqual(provenance["serializer_oracle"]["hydrated_before_serialization"], "byte-vector")
        self.assertEqual(provenance["serializer_oracle"]["plain_full_subtree_roundtrip"], "unknown")
        self.assertEqual(provenance["serializer_oracle"]["explicit_route_field_trace_roundtrip"], "byte-vector")
        self.assertTrue(provenance["serializer_oracle"]["plain_digest_equal"])
        self.assertTrue(provenance["serializer_oracle"]["traced_digest_equal"])
        self.assertEqual(parsed["kinds"], {"before": "byte-vector", "plain": "unknown", "traced": "byte-vector"})
        self.assertEqual(set(parsed["digests"].values()), {provenance["serializer_oracle"]["object_digest_hex"]})


if __name__ == "__main__":
    unittest.main()
