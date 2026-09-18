#!/usr/bin/python3
from __future__ import annotations

import base64
import json
import multiprocessing
import os
from pathlib import Path
import tempfile
import time
import unittest

from journal_cli.source.mock_pi_sync import DEFAULT_ENDPOINT, MockPiSync
from journal_cli.source.sync_source import inspect_reference, observe_ready, publish
from journal_cli.source.sync_source_model import CurrentReference, MAX_CHUNK_BYTES, SyncSourceError

RELEASE_ID = "550e8400-e29b-41d4-a716-446655440000"
MARKER = ["source", "project", "releases", RELEASE_ID, "release.scm"]


def abrupt_publish_child(root_text: str):
    root = Path(root_text)
    private_tmp = root / "tmp"; private_tmp.mkdir(mode=0o700)
    tempfile.tempdir = str(private_tmp)
    source = root / "source"
    adapter = MockPiSync(root / "mock")
    def block_forever(_seconds):
        while True: time.sleep(3600)
    publish(
        adapter, source, route=["galactica", "publisher"], owner="publisher",
        endpoint=DEFAULT_ENDPOINT, project_id="project", project="project", release_label="v1",
        receipt_path=root / "publish.json", release_id=RELEASE_ID,
        settle_seconds=60, sleep_fn=block_forever,
    )


class RecordingAdapter:
    def __init__(self, delegate):
        self.delegate = delegate
        self.endpoint = delegate.endpoint
        self.calls = []

    def invoke(self, command, request):
        result = self.delegate.invoke(command, request)
        self.calls.append((command, json.loads(json.dumps(request)), json.loads(json.dumps(result))))
        return result


class HideMarkerAdapter(RecordingAdapter):
    def invoke(self, command, request):
        result = self.delegate.invoke(command, request)
        if command == "resolve-view" and request["view"] == {"kind": "current"} and request["paths"] == [MARKER]:
            item = result["results"][0]
            result["results"][0] = {
                "path": MARKER, "shape": "nothing", "canonicalCommittedPath": item["canonicalCommittedPath"],
                **({"pinned": item["pinned"]} if "pinned" in item else {}),
            }
        self.calls.append((command, json.loads(json.dumps(request)), json.loads(json.dumps(result))))
        return result


class WrongResourceAdapter(RecordingAdapter):
    def invoke(self, command, request):
        result = self.delegate.invoke(command, request)
        if command == "resolve-view":
            for item in result.get("results", []):
                if item.get("shape") == "value" and item.get("path") != MARKER:
                    item["contentBase64"] = "eA=="
                    item["bytes"] = 1
                    item["sha256"] = "0" * 64
                    break
        self.calls.append((command, json.loads(json.dumps(request)), json.loads(json.dumps(result))))
        return result


class ReadyMarkerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name); os.chmod(self.root, 0o700)
        self.source = self.root / "source"; self.source.mkdir(mode=0o700)
        self.mock = MockPiSync(self.root / "mock")

    def tearDown(self):
        self.temp.cleanup()

    def publish(self, adapter=None, **kwargs):
        release, receipt = publish(
            adapter or self.mock, self.source, route=["galactica", "publisher"], owner="publisher",
            endpoint=DEFAULT_ENDPOINT, project_id="project", project="project", release_label="v1",
            receipt_path=self.root / "publish.json", release_id=RELEASE_ID, **kwargs,
        )
        self.ready_inputs = receipt["operationLocalReadyInputs"]
        return release, receipt

    def ready(self, adapter=None, **overrides):
        values = {
            "owner": self.ready_inputs["owner"],
            "endpoint": self.ready_inputs["endpoint"],
            "descriptor_path": tuple(self.ready_inputs["readyMarkerPath"]),
            "expected_descriptor_bytes": self.ready_inputs["expectedDescriptorBytes"],
            "expected_descriptor_sha256": self.ready_inputs["expectedDescriptorSha256"],
        }
        values.update(overrides)
        return observe_ready(
            adapter or self.mock, route=["galactica", "publisher"], receipt_path=self.root / "ready.json", **values,
        )

    def marker_present(self):
        state = json.loads((self.root / "mock/state.json").read_text())
        return "publisher/" + "/".join(MARKER) in state["values"]

    def test_publication_uses_one_current_then_same_fixed_view_and_marker_last(self):
        (self.source / "large").write_bytes(b"x" * (5 * MAX_CHUNK_BYTES + 1))
        adapter = RecordingAdapter(self.mock)
        release, receipt = self.publish(adapter)
        self.assertEqual(len(release.chunks), 6)
        self.assertEqual(receipt["outcome"], "ready-marker-write-accepted")
        self.assertTrue(receipt["resourcesCommittedExact"])
        self.assertTrue(receipt["readyMarkerWriteAccepted"])
        self.assertEqual(receipt["readyMarkerCommittedVisibility"], "not-observed")
        self.assertEqual(receipt["resourceVerificationScheduling"], {
            "settleSeconds": 0.0, "classification": "scheduling-only", "evidence": False,
            "freshnessGuarantee": False, "waitCount": 1, "waitCompleted": True, "interrupted": False,
        })
        self.assertFalse(receipt["states"]["verified"])
        self.assertNotIn("fixedReference", receipt)
        self.assertNotIn("currentReference", receipt)
        self.assertNotIn("reference", receipt)
        self.assertEqual(set(self.ready_inputs), {
            "classification", "portable", "endpoint", "route", "owner", "readyMarkerPath",
            "expectedDescriptorBytes", "expectedDescriptorSha256",
        })
        self.assertEqual(self.ready_inputs["classification"], "nonnormative-operation-local")
        self.assertIs(self.ready_inputs["portable"], False)
        self.assertEqual(self.ready_inputs["owner"], "publisher")
        self.assertEqual(self.ready_inputs["endpoint"], DEFAULT_ENDPOINT)
        self.assertEqual(self.ready_inputs["readyMarkerPath"], MARKER)
        self.assertEqual(self.ready_inputs["expectedDescriptorBytes"], receipt["readyMarkerBytes"])
        self.assertEqual(self.ready_inputs["expectedDescriptorSha256"], receipt["readyMarkerSha256"])

        commands = [command for command, _, _ in adapter.calls]
        self.assertEqual(commands[-1], "create-value")
        self.assertEqual(adapter.calls[-1][1]["path"], MARKER)
        resolves = [request for command, request, _ in adapter.calls if command == "resolve-view"]
        self.assertEqual(sum(request["view"] == {"kind": "current"} for request in resolves), 1)
        fixed = [request["view"]["index"] for request in resolves if request["view"].get("kind") == "fixed"]
        self.assertTrue(fixed); self.assertEqual(len(set(fixed)), 1)
        self.assertEqual(fixed[0], receipt["resourceVerificationView"]["selectedIndex"])
        self.assertEqual(sum(len(request["paths"]) for request in resolves), 7)

    def test_settle_wait_occurs_exactly_once_and_is_scheduling_only(self):
        (self.source / "a").write_bytes(b"a")
        adapter = RecordingAdapter(self.mock)
        sleeps = []
        calls_at_sleep = []
        def sleep_once(seconds):
            sleeps.append(seconds); calls_at_sleep.append(len(adapter.calls))
        _, receipt = self.publish(adapter, settle_seconds=2.5, sleep_fn=sleep_once)
        self.assertEqual(sleeps, [2.5])
        self.assertEqual(receipt["resourceVerificationScheduling"]["waitCount"], 1)
        self.assertEqual(receipt["resourceVerificationScheduling"]["classification"], "scheduling-only")
        self.assertFalse(receipt["resourceVerificationScheduling"]["evidence"])
        self.assertFalse(receipt["resourceVerificationScheduling"]["freshnessGuarantee"])
        self.assertTrue(all(command == "create-value" for command, _, _ in adapter.calls[:calls_at_sleep[0]]))
        self.assertTrue(any(command == "resolve-view" for command, _, _ in adapter.calls[calls_at_sleep[0]:]))

    def test_zero_default_still_performs_one_nonbusy_wait(self):
        sleeps = []
        self.publish(sleep_fn=sleeps.append)
        self.assertEqual(sleeps, [0.0])

    def test_settle_bound_rejected_before_receipt_or_mutation(self):
        (self.source / "a").write_bytes(b"a")
        for value in (-1, 300.0001, float("inf"), float("nan"), True, "1"):
            adapter = RecordingAdapter(self.mock)
            with self.subTest(value=value), self.assertRaises(SyncSourceError) as caught:
                self.publish(adapter, settle_seconds=value)
            self.assertEqual(caught.exception.code, "invalid-settle-seconds")
            self.assertEqual(adapter.calls, [])
            self.assertFalse((self.root / "publish.json").exists())
        state = json.loads((self.root / "mock/state.json").read_text())
        self.assertEqual(state["index"], 0)

    def test_wait_interruption_stops_safely_before_marker(self):
        cases = [
            (InterruptedError("interrupted"), SyncSourceError),
            (KeyboardInterrupt(), KeyboardInterrupt),
            (SystemExit(9), SystemExit),
        ]
        for ordinal, (error, expected_error) in enumerate(cases):
            with self.subTest(error=type(error).__name__), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); os.chmod(root, 0o700)
                source = root / "source"; source.mkdir(mode=0o700); (source / "a").write_bytes(b"a")
                delegate = MockPiSync(root / "mock"); adapter = RecordingAdapter(delegate); receipt_path = root / "publish.json"
                def interrupt(_seconds, raised=error): raise raised
                with self.assertRaises(expected_error):
                    publish(
                        adapter, source, route=["galactica", "publisher"], owner="publisher",
                        endpoint=DEFAULT_ENDPOINT, project_id="project", project="project", release_label="v1",
                        receipt_path=receipt_path, release_id=f"550e8400-e29b-41d4-a716-4466554400{ordinal:02d}",
                        settle_seconds=1, sleep_fn=interrupt,
                    )
                receipt = json.loads(receipt_path.read_text())
                self.assertEqual(receipt["outcome"], "cancelled")
                self.assertEqual(receipt["resourceCreates"]["acceptedLowerBound"], 1)
                self.assertEqual(receipt["resourceVerificationScheduling"]["waitCount"], 1)
                self.assertFalse(receipt["resourceVerificationScheduling"]["waitCompleted"])
                self.assertTrue(receipt["resourceVerificationScheduling"]["interrupted"])
                creates = [request for command, request, _ in adapter.calls if command == "create-value"]
                self.assertEqual(len(creates), 1); self.assertNotEqual(creates[0]["path"], MARKER)
                self.assertFalse(any(command == "resolve-view" for command, _, _ in adapter.calls))

    def test_abrupt_process_termination_preserves_unknown_interruption_truth(self):
        root = self.root / "abrupt"; root.mkdir(mode=0o700)
        source = root / "source"; source.mkdir(mode=0o700); (source / "a").write_bytes(b"a")
        process = multiprocessing.get_context("fork").Process(target=abrupt_publish_child, args=(str(root),))
        process.start()
        try:
            receipt_path = root / "publish.json"; deadline = time.monotonic() + 10
            observed = None
            while time.monotonic() < deadline:
                if receipt_path.exists():
                    try: candidate = json.loads(receipt_path.read_text())
                    except (OSError, json.JSONDecodeError): candidate = None
                    scheduling = candidate.get("resourceVerificationScheduling", {}) if isinstance(candidate, dict) else {}
                    accepted = candidate.get("resourceCreates", {}).get("acceptedLowerBound") if isinstance(candidate, dict) else None
                    if accepted == 1 and scheduling.get("waitCount") == 1:
                        observed = candidate; break
                if not process.is_alive(): self.fail(f"child exited before blocking wait: {process.exitcode}")
                time.sleep(0.01)
            self.assertIsNotNone(observed)
            scheduling = observed["resourceVerificationScheduling"]
            self.assertFalse(scheduling["waitCompleted"])
            self.assertIsNone(scheduling["interrupted"])
            process.kill(); process.join(5)
            self.assertFalse(process.is_alive()); self.assertLess(process.exitcode, 0)
            durable = json.loads(receipt_path.read_text())
            self.assertEqual(durable["resourceCreates"]["acceptedLowerBound"], 1)
            self.assertEqual(durable["resourceVerificationScheduling"]["waitCount"], 1)
            self.assertFalse(durable["resourceVerificationScheduling"]["waitCompleted"])
            self.assertIsNone(durable["resourceVerificationScheduling"]["interrupted"])
            state = json.loads((root / "mock/state.json").read_text())
            self.assertEqual(state["index"], 1)
            self.assertNotIn("publisher/" + "/".join(MARKER), state["values"])
        finally:
            if process.is_alive(): process.kill(); process.join(5)

    def test_missing_resource_stops_before_marker_without_retry(self):
        (self.source / "a").write_bytes(b"content")
        adapter = WrongResourceAdapter(self.mock)
        with self.assertRaises(SyncSourceError):
            self.publish(adapter)
        self.assertFalse(self.marker_present())
        creates = [request for command, request, _ in adapter.calls if command == "create-value"]
        self.assertEqual(len(creates), 1)
        self.assertNotEqual(creates[0]["path"], MARKER)
        receipt = json.loads((self.root / "publish.json").read_text())
        self.assertFalse(receipt["readyMarkerWriteAccepted"])

    def test_marker_acceptance_is_not_readiness_and_one_shot_absence_does_not_repeat(self):
        (self.source / "a").write_bytes(b"a")
        _, publication = self.publish()
        self.assertTrue(self.marker_present())
        self.assertEqual(publication["readyMarkerCommittedVisibility"], "not-observed")
        adapter = HideMarkerAdapter(self.mock)
        fixed, release, receipt = self.ready(adapter)
        self.assertIsNone(fixed); self.assertIsNone(release)
        self.assertEqual(receipt["outcome"], "not-ready")
        self.assertEqual(receipt["observationCount"], 1)
        self.assertNotIn("fixedReference", receipt)
        self.assertEqual(len(adapter.calls), 1)
        self.assertEqual(adapter.calls[0][1]["view"], {"kind": "current"})

    def test_ready_check_emits_reference_only_after_marker_and_all_resources_exact(self):
        (self.source / "a").write_bytes(b"a")
        expected_release, _ = self.publish()
        adapter = RecordingAdapter(self.mock)
        fixed, release, receipt = self.ready(adapter)
        self.assertEqual(release, expected_release)
        self.assertEqual(receipt["outcome"], "verified")
        self.assertTrue(receipt["states"]["verified"])
        self.assertEqual(receipt["fixedReference"]["index"], fixed.index)
        self.assertEqual(adapter.calls[0][1]["view"], {"kind": "current"})
        for command, request, _ in adapter.calls[1:]:
            self.assertEqual(command, "resolve-view")
            self.assertEqual(request["view"], {"kind": "fixed", "index": fixed.index,
                                                "historyIndexes": list(fixed.history_indexes)})
        self.assertFalse(any(command == "create-value" for command, _, _ in adapter.calls))

    def test_ready_rejects_terminal_descriptor_and_path_mismatch(self):
        (self.source / "a").write_bytes(b"a")
        self.publish()
        wrong_endpoint = "http://127.0.0.1:8193/interface"
        with self.assertRaises(SyncSourceError):
            self.ready(endpoint=wrong_endpoint)
        with self.assertRaises(SyncSourceError):
            self.ready(expected_descriptor_bytes=self.ready_inputs["expectedDescriptorBytes"] + 1)
        with self.assertRaises(SyncSourceError):
            self.ready(expected_descriptor_sha256="0" * 64)
        adapter = RecordingAdapter(self.mock)
        with self.assertRaises(SyncSourceError):
            self.ready(adapter, descriptor_path=("source", "project", "head.scm"))
        self.assertEqual(adapter.calls, [])

    def test_existing_current_reference_rejects_direct_marker_overload(self):
        (self.source / "a").write_bytes(b"a")
        self.publish()
        overloaded = CurrentReference(DEFAULT_ENDPOINT, "publisher", tuple(MARKER))
        with self.assertRaises(SyncSourceError):
            inspect_reference(self.mock, overloaded.encode(), ["galactica", "publisher"])

    def test_ready_check_wrong_resource_stops_without_mutation(self):
        (self.source / "a").write_bytes(b"a")
        self.publish()
        before = json.loads((self.root / "mock/state.json").read_text())["index"]
        adapter = WrongResourceAdapter(self.mock)
        with self.assertRaises(SyncSourceError):
            self.ready(adapter)
        after = json.loads((self.root / "mock/state.json").read_text())["index"]
        self.assertEqual(before, after)
        self.assertFalse(any(command == "create-value" for command, _, _ in adapter.calls))

    def test_marker_ambiguous_create_is_one_attempt_and_never_ready(self):
        self.mock.set_fault("create-value", "failed-or-ambiguous")
        adapter = RecordingAdapter(self.mock)
        with self.assertRaises(SyncSourceError):
            self.publish(adapter)
        creates = [request for command, request, _ in adapter.calls if command == "create-value"]
        self.assertEqual(creates, [{
            "version": 2, "owner": "publisher", "path": MARKER,
            "input": creates[0]["input"], "expected": "absent", "readback": False,
        }])
        self.assertFalse(self.marker_present())
        receipt = json.loads((self.root / "publish.json").read_text())
        self.assertFalse(receipt["readyMarkerWriteAccepted"])
        self.assertFalse(receipt["states"]["verified"])

    def test_ready_explicit_rejection_stops_one_shot_without_mutation(self):
        (self.source / "a").write_bytes(b"a")
        self.publish()
        before = json.loads((self.root / "mock/state.json").read_text())["index"]
        self.mock.set_fault("resolve-view", "rejected")
        adapter = RecordingAdapter(self.mock)
        with self.assertRaises(SyncSourceError):
            self.ready(adapter)
        self.assertEqual(len(adapter.calls), 1)
        self.assertEqual(adapter.calls[0][0], "resolve-view")
        after = json.loads((self.root / "mock/state.json").read_text())["index"]
        self.assertEqual(before, after)

    def test_empty_resource_release_uses_marker_absence_as_coherent_precheck(self):
        adapter = RecordingAdapter(self.mock)
        release, receipt = self.publish(adapter)
        self.assertEqual(release.chunks, ())
        resolves = [request for command, request, _ in adapter.calls if command == "resolve-view"]
        self.assertEqual(len(resolves), 1)
        self.assertEqual(resolves[0]["view"], {"kind": "current"})
        self.assertEqual(resolves[0]["paths"], [MARKER])
        self.assertEqual(adapter.calls[-1][0], "create-value")
        self.assertEqual(adapter.calls[-1][1]["path"], MARKER)
        self.assertEqual(receipt["resourceCreates"], {"expected": 0, "acceptedLowerBound": 0})


if __name__ == "__main__":
    unittest.main()
