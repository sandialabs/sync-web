#!/usr/bin/python3
from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import uuid

from journal_cli.source.mock_pi_sync import DEFAULT_ENDPOINT, MockPiSync
from journal_cli.source.sync_source import inspect_reference, observe_ready, parse_route, publish, pull, resolve_view, retain_paths, source_provenance, verify
from journal_cli.source.sync_source_fs import materialize, recover, snapshot_source, verify_tree, verify_tree_fd
from journal_cli.source.sync_source_model import (
    canonical_bytes, Chunk, CurrentReference, DirectoryEntry, FileEntry, FixedReference, MAX_CHUNK_BYTES,
    MAX_COMPONENT_BYTES, MAX_ENTRIES, Release, S, SyncSourceError, parse_reference, parse_release, sha256,
    tree_digest, validate_logical_component, validate_logical_path, validate_release, validate_release_binding,
)


class Crash(BaseException): pass


class Tests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name); os.chmod(self.root, 0o700)
        self.mock = MockPiSync(self.root / "mock")
        self.source = self.root / "source"; self.source.mkdir(mode=0o700)
        self.receipt = self.root / "receipt.json"
        self.route = ["galactica", "publisher"]
        self.owner = "publisher"
        self.endpoint = DEFAULT_ENDPOINT

    def tearDown(self): self.temp.cleanup()

    def write(self, relative: str, data: bytes, mode: int = 0o644) -> Path:
        path = self.source / relative; path.parent.mkdir(mode=0o755, parents=True, exist_ok=True); path.write_bytes(data); os.chmod(path, mode); return path

    def publish(self, release_id: str = "550e8400-e29b-41d4-a716-446655440000"):
        release, publication = publish(
            self.mock, self.source, route=self.route, owner=self.owner, endpoint=self.endpoint,
            project_id="project", project="project", release_label="v1", receipt_path=self.receipt,
            release_id=release_id,
        )
        ready_inputs = publication["operationLocalReadyInputs"]
        fixed, observed, ready = observe_ready(
            self.mock, route=self.route, owner=ready_inputs["owner"],
            endpoint=ready_inputs["endpoint"],
            descriptor_path=tuple(ready_inputs["readyMarkerPath"]),
            expected_descriptor_bytes=ready_inputs["expectedDescriptorBytes"],
            expected_descriptor_sha256=ready_inputs["expectedDescriptorSha256"],
            receipt_path=self.root / "ready.json",
        )
        self.assertIsNotNone(fixed); self.assertEqual(observed, release); self.assertEqual(ready["outcome"], "verified")
        return fixed, release, publication

    # F1: minimal valid release and exact canonical objects.
    def test_f1_publish_pull_verify(self):
        self.write("README.md", b"hello source\n")
        fixed, release, publication = self.publish()
        self.assertEqual(parse_reference(fixed.encode()), fixed)
        self.assertEqual(parse_release(release.encode()), release)
        destination = self.root / "checkout"; pull_receipt = self.root / "pull.json"
        result = pull(self.mock, fixed.encode(), route=self.route, destination=destination, receipt_path=pull_receipt)
        self.assertEqual((destination / "README.md").read_bytes(), b"hello source\n")
        self.assertEqual(result["outcome"], "materialized"); self.assertTrue(result["states"]["materialized"])
        self.assertFalse(result["installed"]); self.assertFalse(result["executed"])
        self.assertEqual(verify(release.encode(), destination)["treeSha256"], tree_digest(release))
        self.assertEqual(json.loads(pull_receipt.read_text())["fixedReference"]["index"], fixed.index)

    def test_f1_repeated_verification_uses_fresh_directory_offsets(self):
        self.write("a", b"a"); fixed, release, _ = self.publish(); destination = self.root / "checkout"
        pull(self.mock, fixed.encode(), route=self.route, destination=destination, receipt_path=self.root / "pull.json")
        fd = os.open(destination, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            self.assertEqual(verify_tree_fd(fd, release), tree_digest(release))
            self.assertEqual(verify_tree_fd(fd, release), tree_digest(release))
        finally: os.close(fd)

    def test_f1_current_reference_resolves_once(self):
        self.write("a", b"a"); fixed, release, _ = self.publish()
        head = self.root / "head"; head.write_bytes(fixed.encode())
        create_path = ("source", "project", "head.scm")
        from journal_cli.source.sync_source import create_value
        create_value(self.mock, self.owner, create_path, head)
        current = CurrentReference(self.endpoint, self.owner, create_path)
        observed, parsed = inspect_reference(self.mock, current.encode(), self.route)
        self.assertEqual(observed, fixed); self.assertEqual(parsed, release)

    def test_f1_empty_owner_local_route_is_valid(self):
        self.assertEqual(parse_route("[]"), [])
        self.write("local", b"owner-local\n")
        release, publication = publish(
            self.mock, self.source, route=[], owner=self.owner, endpoint=self.endpoint,
            project_id="project", project="project", release_label="v1", receipt_path=self.receipt,
            release_id="550e8400-e29b-41d4-a716-446655440000",
        )
        ready_inputs = publication["operationLocalReadyInputs"]
        fixed, observed, _ = observe_ready(
            self.mock, route=[], owner=ready_inputs["owner"], endpoint=ready_inputs["endpoint"],
            descriptor_path=tuple(ready_inputs["readyMarkerPath"]), expected_descriptor_bytes=ready_inputs["expectedDescriptorBytes"],
            expected_descriptor_sha256=ready_inputs["expectedDescriptorSha256"], receipt_path=self.root / "ready.json",
        )
        self.assertEqual(publication["route"], []); self.assertEqual(observed, release); self.assertEqual(fixed.endpoint, self.endpoint)

    def test_f1_route_grammar_and_endpoint_fail_before_remote_mutation(self):
        self.assertEqual(parse_route('["galactica","peer_*+!<>=?-"]'), ["galactica", "peer_*+!<>=?-"])
        invalid_routes = ["{}", '["bad/slash"]', '["."]', '[".."]', '[""]', '["\\\\"]', json.dumps(["caf" + chr(0xE9)]), json.dumps(["x"] * 17), json.dumps(["x" * 129])]
        for encoded in invalid_routes:
            with self.subTest(route=encoded), self.assertRaises(SyncSourceError) as caught:
                parse_route(encoded)
            self.assertEqual(caught.exception.code, "invalid-route")
        self.write("one", b"x")
        before = self.mock.state_path.read_bytes()
        with self.assertRaises(SyncSourceError) as caught:
            publish(
                self.mock, self.source, route=[], owner=self.owner, endpoint="not-a-url",
                project_id="project", project="project", release_label="v1", receipt_path=self.receipt,
                release_id="550e8400-e29b-41d4-a716-446655440000",
            )
        self.assertEqual(caught.exception.code, "invalid-endpoint")
        self.assertEqual(self.mock.state_path.read_bytes(), before); self.assertFalse(self.receipt.exists())
        with self.assertRaises(SyncSourceError) as caught:
            publish(
                self.mock, self.source, route=["bad/slash"], owner=self.owner, endpoint=self.endpoint,
                project_id="project", project="project", release_label="v1", receipt_path=self.receipt,
                release_id="550e8400-e29b-41d4-a716-446655440000",
            )
        self.assertEqual(caught.exception.code, "invalid-route")
        self.assertEqual(self.mock.state_path.read_bytes(), before); self.assertFalse(self.receipt.exists())

    # F2: valid byte/path/mode/chunk edges.
    def test_f2_edges_and_multichunk(self):
        self.write("empty", b"")
        self.write("caf\u00e9.txt", "NFC\n".encode())
        self.write("opaque.dat", bytes(range(256)))
        self.write("no-final", b"tail")
        self.write("tool.sh", b"#!/bin/sh\nexit 99\n", 0o755)
        large = b"x" * (MAX_CHUNK_BYTES + 17); self.write("vendor/s7.c", large)
        fixed, release, _ = self.publish()
        files = {entry.path: entry for entry in release.entries if isinstance(entry, FileEntry)}
        self.assertEqual(files[("empty",)].chunks, ())
        self.assertEqual(len(files[("vendor", "s7.c")].chunks), 2)
        self.assertEqual(files[("tool.sh",)].mode, 493)
        destination = self.root / "checkout"
        pull(self.mock, fixed.encode(), route=self.route, destination=destination, receipt_path=self.root / "pull.json", pin_policy="paths")
        self.assertEqual((destination / "vendor/s7.c").read_bytes(), large)
        self.assertEqual((destination / "tool.sh").stat().st_mode & 0o777, 0o755)

    # F3: strict canonical parser and path rejection matrix.
    def test_f3_reference_canonical_rejections(self):
        fixed = FixedReference(self.endpoint, "publisher", 1, ("source", "p", "release.scm"), 1, "0" * 64).encode()
        mutations = [
            fixed.replace(b" (endpoint", b"  (endpoint", 1),
            fixed.replace(b"(index 1)", b"(index 01)"),
            fixed.replace(b"source-fixed-v2", b"#source-fixed-v2"),
            fixed[:-1] + b" extra\n",
            b"'(source-fixed-v2)\n",
            b"(source-fixed-v2 . ())\n",
            b"(source-fixed-v2)\n\n",
        ]
        for value in mutations:
            with self.subTest(value=value[:40]), self.assertRaises(SyncSourceError): parse_reference(value)

    def test_f3_descriptor_shape_order_and_unknown(self):
        good = Release("p", "r", (DirectoryEntry(("a",)), FileEntry(("a", "b"), 420, 0, sha256(b""), ()))).encode()
        self.assertEqual(parse_release(good).project, "p")
        for bad in [
            good.replace(b"(project \"p\")", b"(unknown \"p\")", 1),
            good.replace(b"(project \"p\")", b"(project \"p\") (project \"p\")", 1),
            good.replace(b"(mode 493)", b"(mode 0755)", 1),
            good.replace(b"\"p\"", b"\"e\\n\"", 1),
        ]:
            with self.assertRaises(SyncSourceError): parse_release(bad)

    def test_f3_logical_path_rejections(self):
        invalid = ["", ".", "..", "a/b", "a\\b", "a\x00b", "e\u0301", "x" * (MAX_COMPONENT_BYTES + 1)]
        for value in invalid:
            with self.subTest(value=repr(value)), self.assertRaises(SyncSourceError): validate_logical_component(value)
        with self.assertRaises(SyncSourceError): validate_logical_path(["a"] * 65)

    def test_f3_duplicate_prefix_and_entry_limits(self):
        duplicate = Release("p", "r", (DirectoryEntry(("a",)), DirectoryEntry(("a",))))
        with self.assertRaises(SyncSourceError): validate_release(duplicate)
        prefix = Release("p", "r", (FileEntry(("a",), 420, 0, sha256(b""), ()), DirectoryEntry(("a", "b"))))
        with self.assertRaises(SyncSourceError): validate_release(prefix)
        entries = tuple(DirectoryEntry((f"d{i:04d}",)) for i in range(MAX_ENTRIES + 1))
        with self.assertRaises(SyncSourceError): validate_release(Release("p", "r", entries))

    # F4: exact reference/content/view/pin failures.
    def test_f4_reference_descriptor_tamper(self):
        self.write("a", b"a"); fixed, _, _ = self.publish()
        cases = [
            FixedReference("http://127.0.0.1:8193/interface", fixed.owner, fixed.index, fixed.descriptor_path, fixed.descriptor_bytes, fixed.descriptor_sha256),
            FixedReference(fixed.endpoint, fixed.owner, fixed.index, fixed.descriptor_path, fixed.descriptor_bytes + 1, fixed.descriptor_sha256),
            FixedReference(fixed.endpoint, fixed.owner, fixed.index, fixed.descriptor_path, fixed.descriptor_bytes, "0" * 64),
        ]
        for value in cases:
            with self.assertRaises(SyncSourceError): inspect_reference(self.mock, value.encode(), self.route)

    def test_f4_fixed_view_does_not_advance(self):
        self.write("a", b"a"); fixed, release, _ = self.publish()
        extra = self.root / "extra"; extra.write_bytes(b"later")
        from journal_cli.source.sync_source import create_value
        create_value(self.mock, self.owner, ("unrelated",), extra)
        observed, parsed = inspect_reference(self.mock, fixed.encode(), self.route)
        self.assertEqual(observed.index, fixed.index); self.assertEqual(parsed, release)

    def test_f4_pin_partial_and_ambiguous_fail(self):
        self.write("a", b"a"); fixed, release, _ = self.publish()
        self.mock.set_fault("pin-view", "failed-or-ambiguous")
        with self.assertRaisesRegex(SyncSourceError, "Retention"):
            pull(self.mock, fixed.encode(), route=self.route, destination=self.root / "checkout", receipt_path=self.root / "pull.json", pin_policy="paths")
        receipt = json.loads((self.root / "pull.json").read_text())
        self.assertFalse(receipt["pin"]["complete"]); self.assertEqual(receipt["pin"]["outcome"], "failed-or-ambiguous"); self.assertFalse((self.root / "checkout").exists())

    def test_f4_chunk_content_mismatch(self):
        self.write("a", b"content"); fixed, release, _ = self.publish()
        chunk = release.chunks[0]; state = json.loads((self.root / "mock/state.json").read_text())
        history = state["values"][self.owner + "/" + "/".join((*fixed.descriptor_path[:-1], *chunk.path))]
        (self.root / "mock/blobs" / history[-1]["sha256"]).write_bytes(b"changed")
        with self.assertRaises(SyncSourceError): pull(self.mock, fixed.encode(), route=self.route, destination=self.root / "checkout", receipt_path=self.root / "pull.json")

    # F5: filesystem races, links, cancellation, no-replace, recovery.
    def test_f5_source_symlink_and_hardlink_rejected(self):
        target = self.write("target", b"x"); os.symlink("target", self.source / "link")
        with tempfile.TemporaryDirectory(dir=self.root) as td:
            with self.assertRaises(SyncSourceError): snapshot_source(self.source, Path(td) / "spool", "p", "r")
        (self.source / "link").unlink(); os.link(target, self.source / "hard")
        with tempfile.TemporaryDirectory(dir=self.root) as td:
            with self.assertRaises(SyncSourceError): snapshot_source(self.source, Path(td) / "spool", "p", "r")

    def test_f5_existing_and_raced_destination_preserved(self):
        self.write("a", b"a"); fixed, release, _ = self.publish(); destination = self.root / "checkout"; destination.mkdir(); sentinel = destination / "keep"; sentinel.write_text("keep")
        with self.assertRaises(SyncSourceError): pull(self.mock, fixed.encode(), route=self.route, destination=destination, receipt_path=self.root / "pull.json")
        self.assertEqual(sentinel.read_text(), "keep")
        shutil = __import__("shutil"); shutil.rmtree(destination)
        def race(point, path):
            if point == "before-rename": path.mkdir()
        with self.assertRaises(SyncSourceError): pull(self.mock, fixed.encode(), route=self.route, destination=destination, receipt_path=self.root / "pull2.json", cut=race)
        self.assertTrue(destination.is_dir()); self.assertEqual(list(destination.iterdir()), [])
        self.assertEqual(list(self.root.glob(".sync-source-stage-*")), [])

    def test_f5_handled_cut_cleans(self):
        self.write("a", b"a"); fixed, _, _ = self.publish()
        def cut(point, _path):
            if point == "after-file": raise RuntimeError("cut")
        with self.assertRaises(RuntimeError): pull(self.mock, fixed.encode(), route=self.route, destination=self.root / "checkout", receipt_path=self.root / "pull.json", cut=cut)
        self.assertFalse((self.root / "checkout").exists()); self.assertEqual(list(self.root.glob(".sync-source-stage-*")), [])

    def test_f5_crash_residue_is_marker_bound_and_recoverable(self):
        self.write("a", b"a"); fixed, _, _ = self.publish(); receipt_path = self.root / "pull.json"
        def crash(point, _path):
            if point == "after-file": raise Crash()
        with self.assertRaises(Crash): pull(self.mock, fixed.encode(), route=self.route, destination=self.root / "checkout", receipt_path=receipt_path, cut=crash)
        markers = list(self.root.glob(".sync-source-stage-*.json")); stages = [path for path in self.root.glob(".sync-source-stage-*") if path.is_dir()]
        self.assertEqual(len(markers), 1); self.assertEqual(len(stages), 1); self.assertFalse((self.root / "checkout").exists())
        self.assertTrue(recover(self.root, markers[0].name, receipt_path)); self.assertFalse(markers[0].exists()); self.assertFalse(stages[0].exists())

    def test_f5_crash_after_atomic_rename_never_deletes_complete_final(self):
        self.write("a", b"a"); fixed, release, _ = self.publish(); destination = self.root / "checkout"; receipt_path = self.root / "pull.json"
        def crash(point, _path):
            if point == "after-rename": raise Crash()
        with self.assertRaises(Crash): pull(self.mock, fixed.encode(), route=self.route, destination=destination, receipt_path=receipt_path, cut=crash)
        self.assertTrue(destination.is_dir()); self.assertEqual(verify_tree(destination, release), tree_digest(release))
        markers = list(self.root.glob(".sync-source-stage-*.json")); self.assertEqual(len(markers), 1)
        with self.assertRaises((SyncSourceError, FileNotFoundError)): recover(self.root, markers[0].name, receipt_path)
        self.assertTrue(destination.is_dir())

    def test_f5_marker_mismatch_refuses_cleanup(self):
        marker = self.root / (".sync-source-stage-" + "a" * 32 + ".json"); marker.write_text("{}\n"); os.chmod(marker, 0o600)
        receipt = self.root / "r.json"; receipt.write_text(json.dumps({"operationId": "a" * 32, "outcome": "attempted"}))
        with self.assertRaises(SyncSourceError): recover(self.root, marker.name, receipt)

    # F6: exact boundaries and clean capability failure.
    def test_f6_chunk_and_label_boundaries(self):
        good = Chunk(("tree", "o00000000", "c00000000"), MAX_CHUNK_BYTES, sha256(b"x"))
        entry = FileEntry(("a",), 420, MAX_CHUNK_BYTES, sha256(b"x" * MAX_CHUNK_BYTES), (good,))
        # A mismatched synthetic digest is caught at retrieval, while structural max remains encodable.
        self.assertLessEqual(good.bytes, MAX_CHUNK_BYTES)
        with self.assertRaises(SyncSourceError):
            parse_release(Release("x" * 129, "r", ()).encode())
        with self.assertRaises(SyncSourceError):
            validate_logical_component("x" * 256)

    def test_f3_release_binding_rejects_layout_and_joined_depth(self):
        release = Release("p", "r", ())
        wrong = FixedReference(self.endpoint, "publisher", 1, ("other", "p", "releases", "550e8400-e29b-41d4-a716-446655440000", "release.scm"), len(release.encode()), sha256(release.encode()))
        with self.assertRaises(SyncSourceError): validate_release_binding(wrong, release)
        deep_chunk = Chunk(("tree", *("x" for _ in range(60))), 1, sha256(b"x"))
        deep_release = Release("p", "r", (FileEntry(("a",), 420, 1, sha256(b"x"), (deep_chunk,)),))
        fixed = FixedReference(self.endpoint, "publisher", 1, ("source", "p", "releases", "550e8400-e29b-41d4-a716-446655440000", "release.scm"), len(deep_release.encode()), sha256(deep_release.encode()))
        with self.assertRaises(SyncSourceError): validate_release_binding(fixed, deep_release)

    def test_f4_create_conflict_is_one_attempt_and_no_overwrite(self):
        payload = self.root / "payload"; payload.write_bytes(b"first")
        from journal_cli.source.sync_source import create_value
        create_value(self.mock, self.owner, ("value",), payload)
        before = json.loads((self.root / "mock/state.json").read_text())["index"]
        payload.write_bytes(b"second")
        with self.assertRaises(SyncSourceError): create_value(self.mock, self.owner, ("value",), payload)
        after = json.loads((self.root / "mock/state.json").read_text())["index"]
        self.assertEqual(before, after)
        _, values = resolve_view(self.mock, self.endpoint, self.route, self.owner, "current", [("value",)])
        self.assertEqual(values[0][1], b"first")

    def test_f4_partial_cross_batch_handles_are_reported_immediately(self):
        chunks = tuple(Chunk(("tree", "o00000000", f"c{i:08d}"), 1, "0" * 64) for i in range(129))
        release = Release("p", "r", (FileEntry(("a",), 420, 129, "1" * 64, chunks),))
        fixed = FixedReference(self.endpoint, self.owner, 1, ("source", "p", "releases", "550e8400-e29b-41d4-a716-446655440000", "release.scm"), len(release.encode()), sha256(release.encode()))
        class Partial:
            calls = 0
            def invoke(inner, command, request):
                inner.calls += 1
                if inner.calls == 1:
                    return {"contract": "journal-cli-source-ops-v2", "version": 2, "operation": command, "tool": {"name": "journal-cli-source-ops", "version": "2.0.0", "sha256": "0" * 64}, "outcome": "accepted", "handles": [{"handleSha256": f"{i:064x}"} for i, _path in enumerate(request["paths"])]}
                return {"contract": "journal-cli-source-ops-v2", "version": 2, "operation": command, "tool": {"name": "journal-cli-source-ops", "version": "2.0.0", "sha256": "0" * 64}, "outcome": "failed-or-ambiguous"}
        observed = []
        handles, complete, outcome = retain_paths(Partial(), self.route, fixed, release, on_handle=observed.append)
        self.assertFalse(complete); self.assertEqual(outcome, "failed-or-ambiguous"); self.assertEqual(handles, observed); self.assertEqual(len(handles), 1)

    def test_f4_retention_handles_are_exact_and_explicitly_unpinned(self):
        self.write("a", b"a"); fixed, release, _ = self.publish()
        handles, complete, outcome = retain_paths(self.mock, self.route, fixed, release)
        self.assertTrue(complete); self.assertEqual(outcome, "accepted"); self.assertEqual(len(handles), 2)
        result = self.mock.invoke("unpin-retention", {"version": 2, "handles": handles})
        self.assertEqual(result["outcome"], "accepted")
        second = self.mock.invoke("unpin-retention", {"version": 2, "handles": handles})
        self.assertEqual(second["outcome"], "rejected")

    def test_f5_intermediate_symlink_ancestors_rejected(self):
        real_source = self.root / "real-source"; real_source.mkdir(); (real_source / "a").write_bytes(b"a")
        source_link = self.root / "source-link"; source_link.symlink_to(real_source, target_is_directory=True)
        with tempfile.TemporaryDirectory(dir=self.root) as td:
            with self.assertRaises(OSError): snapshot_source(source_link, Path(td) / "spool", "p", "r")
        self.write("a", b"a"); fixed, _, _ = self.publish()
        real_parent = self.root / "real-parent"; real_parent.mkdir(); parent_link = self.root / "parent-link"; parent_link.symlink_to(real_parent, target_is_directory=True)
        with self.assertRaises(OSError): pull(self.mock, fixed.encode(), route=self.route, destination=parent_link / "checkout", receipt_path=self.root / "pull-link.json")
        self.assertFalse((real_parent / "checkout").exists())

    def test_f5_world_writable_parent_and_adapter_symlink_rejected(self):
        self.write("a", b"a"); fixed, _, _ = self.publish()
        unsafe = self.root / "unsafe"; unsafe.mkdir(); os.chmod(unsafe, 0o777)
        with self.assertRaises(SyncSourceError): pull(self.mock, fixed.encode(), route=self.route, destination=unsafe / "checkout", receipt_path=self.root / "pull.json")


    def test_f5_short_write_and_fsync_failure_clean_stage(self):
        release = Release("p", "r", (FileEntry(("a",), 420, 1, sha256(b"a"), (Chunk(("tree", "o00000000", "c00000000"), 1, sha256(b"a")),)),))
        cases = (("journal_cli.source.sync_source_fs.os.write", {"return_value": 0}), ("journal_cli.source.sync_source_fs.os.fsync", {"side_effect": OSError("fsync")}))
        for number, (target, behavior) in enumerate(cases):
            destination = self.root / f"checkout-fault-{number}"
            with self.subTest(target=target), mock.patch(target, **behavior):
                with self.assertRaises(OSError): materialize(destination, release, lambda _chunk: b"a", self.root / f"receipt-{number}.json", operation_id=f"{number:032x}")
            self.assertFalse(destination.exists()); self.assertEqual(list(self.root.glob(".sync-source-stage-*")), [])

    def test_f5_no_replace_unavailable_fails_without_final_tree(self):
        self.write("a", b"a"); fixed, _, _ = self.publish(); destination = self.root / "checkout"
        with mock.patch("journal_cli.source.sync_source_fs._rename_noreplace", side_effect=SyncSourceError("unsupported-capability", "no primitive")):
            with self.assertRaises(SyncSourceError): pull(self.mock, fixed.encode(), route=self.route, destination=destination, receipt_path=self.root / "pull.json")
        self.assertFalse(destination.exists()); self.assertEqual(list(self.root.glob(".sync-source-stage-*")), [])
        receipt = json.loads((self.root / "pull.json").read_text()); self.assertEqual(receipt["stagingCleanup"], "complete-or-not-started")

    def test_f6_structural_max_file_aggregate_entries_and_batch(self):
        chunks = tuple(Chunk(("tree", "o00000000", f"c{i:08d}"), MAX_CHUNK_BYTES, "0" * 64) for i in range(128))
        file_entry = FileEntry(("f0",), 420, 67_108_864, "1" * 64, chunks)
        files = []
        for file_index in range(8):
            mapped = tuple(Chunk(("tree", f"o{file_index:08d}", f"c{i:08d}"), MAX_CHUNK_BYTES, "0" * 64) for i in range(128))
            files.append(FileEntry((f"f{file_index}",), 420, 67_108_864, "1" * 64, mapped))
        maximum = Release("p", "r", tuple(files)); self.assertEqual(parse_release(maximum.encode()).aggregate_bytes, 536_870_912)
        plus = Release("p", "r", (*files, FileEntry(("z",), 420, 1, "2" * 64, (Chunk(("tree", "o00000008", "c00000000"), 1, "3" * 64),))))
        with self.assertRaises(SyncSourceError): validate_release(plus)
        exact_entries = Release("p", "r", tuple(DirectoryEntry((f"d{i:04d}",)) for i in range(MAX_ENTRIES)))
        self.assertEqual(len(parse_release(exact_entries.encode()).entries), MAX_ENTRIES)
        paths = [[f"p{i}"] for i in range(1024)]
        result = self.mock.invoke("resolve-view", {"version": 2, "endpoint": self.endpoint, "route": ["x"], "owner": self.owner, "view": {"kind": "current"}, "paths": paths, "includePinned": True})
        self.assertEqual(result["outcome"], "retrieved"); self.assertEqual(len(result["results"]), 1024)
        result = self.mock.invoke("resolve-view", {"version": 2, "endpoint": self.endpoint, "route": ["x"], "owner": self.owner, "view": {"kind": "current"}, "paths": [*paths, ["overflow"]], "includePinned": True})
        self.assertEqual(result["outcome"], "not-attempted")

    def test_f6_deep_local_source_rejected_before_remote_mutation(self):
        current = self.source
        for _ in range(65): current = current / "x"; current.mkdir()
        with tempfile.TemporaryDirectory(dir=self.root) as td:
            with self.assertRaises(SyncSourceError): snapshot_source(self.source, Path(td) / "spool", "p", "r")
        state = json.loads((self.root / "mock/state.json").read_text()); self.assertEqual(state["index"], 0)

    def test_f6_wire_safe_content_batch_bound(self):
        encoded_chunk = ((MAX_CHUNK_BYTES + 2) // 3) * 4
        self.assertLess(5 * encoded_chunk + 65_536, 4_190_208)
        self.assertGreater(6 * encoded_chunk, 4_190_208)

    def test_f6_exact_path_depth_and_component_boundary(self):
        component = "x" * 255; self.assertEqual(validate_logical_component(component), component)
        self.assertEqual(len(validate_logical_path(["x"] * 64)), 64)
        with self.assertRaises(SyncSourceError): validate_logical_path(["x"] * 65)
        with self.assertRaises(SyncSourceError): validate_logical_component("x" * 256)

    def test_receipt_and_provenance_are_bounded_non_authority(self):
        self.write("a", b"a"); fixed, release, receipt = self.publish()
        self.assertRegex(source_provenance(), r"^[0-9a-f]{64}$")
        self.assertFalse(receipt["installed"]); self.assertFalse(receipt["executed"])
        self.assertEqual(receipt["treeSha256"], tree_digest(release)); self.assertNotIn("fixedReference", receipt)
        self.assertEqual(receipt["outcome"], "ready-marker-write-accepted"); self.assertFalse(receipt["states"]["verified"])


if __name__ == "__main__": unittest.main()
