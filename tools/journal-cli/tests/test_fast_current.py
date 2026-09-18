#!/usr/bin/python3
import base64
import json
from pathlib import Path
import tempfile
import unittest

from journal_cli.source import sync_source_current
from journal_cli.source.sync_source_current import audit_current, current_get, partition_reads, publish_current, pull_current, _read
from journal_cli.source.sync_source_adapter import canonical_request_body
from journal_cli.source.sync_source_model import CurrentReleaseReference, SyncSourceError, parse_current_release, sha256

ENDPOINT = "http://127.0.0.1:8192/interface"


class StoreAdapter:
    def __init__(self):
        self.store = {}; self.calls = []; self.index = 17; self.endpoint = ENDPOINT
        self.mutate = None

    @staticmethod
    def item(path, data):
        return {"path": list(path), "shape": "value", "contentBase64": base64.b64encode(data).decode(),
                "bytes": len(data), "sha256": sha256(data)}

    def invoke(self, command, request):
        self.calls.append((command, request))
        if command == "create-value":
            path = tuple(request["path"])
            if path in self.store: return {"outcome": "rejected"}
            data = Path(request["input"]).read_bytes(); self.store[path] = data
            return {"outcome": "accepted", "bytes": len(data), "sha256": sha256(data)}
        if command == "get-current":
            reads = request["reads"]
            if self.mutate: self.mutate(self, len([c for c, _ in self.calls if c == "get-current"]))
            values = [self.item(tuple(read["path"]), self.store[tuple(read["path"])]) for read in reads]
            return {"contract": "journal-cli-source-ops-v2", "version": 2, "operation": command,
                    "outcome": "retrieved", "requestSha256": sha256(canonical_request_body(request)), "results": values,
                    "currentEvidence": {"contentObservedLocator": None, "contentObservedIndex": None,
                    "committed": False, "routeContinuityObservations": 0, "useBatchDispatches": 1}}
        if command == "resolve-view":
            return {"outcome": "retrieved", "view": {"entryEndpoint": self.endpoint, "terminalEndpoint": self.endpoint,
                    "route": request["route"], "historyIndexes": [self.index] * (len(request["route"]) + 1),
                    "owner": request["owner"], "selectedIndex": self.index, "originIndex": self.index}, "results": [self.item(tuple(p), self.store[tuple(p)]) for p in request["paths"]]}
        raise AssertionError(command)


class FastCurrentTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); self.root = Path(self.tmp.name)
    def tearDown(self): self.tmp.cleanup()

    def source(self):
        p = self.root / "source"; p.mkdir(); (p / "a.txt").write_bytes(b"alpha"); return p

    def test_01_current_reference_roundtrip(self):
        value = CurrentReleaseReference(ENDPOINT, "owner", ("source", "p", "releases", "00000000-0000-4000-8000-000000000001", "release.scm"), 1, "0" * 64)
        self.assertEqual(parse_current_release(value.encode()), value)

    def test_02_v1_current_is_not_v11_current_release(self):
        with self.assertRaises(SyncSourceError): parse_current_release(b'(source-current-v1 (owner "owner") (journal "' + ENDPOINT.encode() + b'") (head-path ("source" "p" "head.scm")))\n')

    def test_03_partition_1024_small_paths(self):
        reads = [_read(("tree", f"p{i}"), 1, sha256(b"x")) for i in range(1024)]
        self.assertEqual([len(x) for x in partition_reads("owner", reads)], [1024])

    def test_04_partition_16384_is_at_least_16(self):
        reads = [_read(("tree", f"p{i}"), 1, sha256(b"x")) for i in range(16384)]
        batches = partition_reads("owner", reads)
        self.assertGreaterEqual(len(batches), 16); self.assertEqual(sum(map(len, batches)), 16384)

    def test_05_large_reads_require_more_than_path_count_batches(self):
        reads = [_read(("tree", f"p{i}"), 524288, "0" * 64) for i in range(17)]
        self.assertGreater(len(partition_reads("owner", reads)), 1)

    def test_06_current_evidence_null(self):
        adapter = StoreAdapter(); path = ("source", "p", "x"); adapter.store[path] = b"x"
        values, evidence, transcript = current_get(adapter, [], "owner", [_read(path, 1, sha256(b"x"))])
        self.assertEqual(values, [b"x"]); self.assertIsNone(evidence["contentObservedLocator"]); self.assertRegex(transcript["requestSha256"], r"^[0-9a-f]{64}$")

    def test_07_publish_marker_last_then_head_init(self):
        adapter = StoreAdapter(); receipt = self.root / "publish.json"
        head, result = publish_current(adapter, self.source(), route=[], owner="owner", endpoint=ENDPOINT,
            project_id="project", project="project", release_label="r", head_path=("source", "project", "current-head.scm"),
            receipt_path=receipt, expected_old_head=None, advance_head=lambda *_: self.fail("advance"),
            release_id="00000000-0000-4000-8000-000000000001")
        creates = [tuple(req["path"]) for cmd, req in adapter.calls if cmd == "create-value"]
        self.assertEqual(creates[-2][-1], "release.scm"); self.assertEqual(creates[-1][-1], "current-head.scm")
        self.assertTrue(result["markerObservedCurrent"]); self.assertEqual(adapter.store[creates[-1]], head.encode())

    def test_08_publish_advance_is_once_and_visibility_unobserved(self):
        adapter = StoreAdapter(); calls=[]
        def advance(owner, path, old, new): calls.append((owner,path,old,new)); adapter.store[path]=new; return {"outcome":"accepted"}
        advance.capability_validated = True
        _, result = publish_current(adapter, self.source(), route=[], owner="owner", endpoint=ENDPOINT,
            project_id="project", project="project", release_label="r", head_path=("source","project","current-head.scm"),
            receipt_path=self.root/"p.json", expected_old_head=b"old", advance_head=advance,
            release_id="00000000-0000-4000-8000-000000000002")
        self.assertEqual(len(calls),1); self.assertEqual(result["headVisibility"],"unobserved")

    def prepare(self):
        adapter=StoreAdapter(); head,_=publish_current(adapter,self.source(),route=[],owner="owner",endpoint=ENDPOINT,
            project_id="project",project="project",release_label="r",head_path=("source","project","current-head.scm"),
            receipt_path=self.root/"prep.json",expected_old_head=None,advance_head=lambda *_: None,
            release_id="00000000-0000-4000-8000-000000000003")
        return adapter,head

    def test_09_pull_captures_head_once_and_materializes(self):
        adapter,head=self.prepare(); dest=self.root/"dest"
        publication_marker_calls=[r for c,r in adapter.calls if c=="get-current" and r["reads"][0]["path"][-1]=="release.scm"]
        self.assertEqual(len(publication_marker_calls),1)
        result=pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=dest,receipt_path=self.root/"pull.json")
        self.assertEqual((dest/"a.txt").read_bytes(),b"alpha"); self.assertEqual(result["outcome"],"materialized-current")
        self.assertIsNone(result["contentObservedLocator"]); self.assertFalse(result["committed"])
        head_calls=[r for c,r in adapter.calls if c=="get-current" and r["reads"][0]["path"][-1]=="current-head.scm"]
        self.assertEqual(len(head_calls),1)  # Pull captures the head exactly once; publication reads only the descriptor marker.

    def test_10_pull_unequal_cutover_fails_without_destination(self):
        adapter,head=self.prepare(); chunk=next(p for p in adapter.store if "tree" in p)
        def mutate(store,count):
            if count>=3: store.store[chunk]=b"wrong"
        adapter.mutate=mutate; dest=self.root/"dest"
        with self.assertRaises((SyncSourceError,KeyError)): pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=dest,receipt_path=self.root/"pull.json")
        self.assertFalse(dest.exists())

    def test_11_equal_byte_cutover_still_content_only(self):
        adapter,head=self.prepare(); adapter.mutate=lambda store,count: None
        result=pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=self.root/"dest",receipt_path=self.root/"pull.json")
        self.assertFalse(result["committed"]); self.assertIsNone(result["selectedIndex"])

    def test_12_missing_resource_fails_no_final(self):
        adapter,head=self.prepare(); chunk=next(p for p in adapter.store if "tree" in p); del adapter.store[chunk]
        dest=self.root/"dest"
        with self.assertRaises(KeyError): pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=dest,receipt_path=self.root/"pull.json")
        self.assertFalse(dest.exists())

    def test_13_audit_no_head_and_emits_fixed(self):
        adapter,head=self.prepare(); dest=self.root/"dest"; pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=dest,receipt_path=self.root/"pull.json")
        before=len(adapter.calls); fixed,result=audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"audit.json",destination=dest)
        self.assertEqual(fixed.index,17); self.assertEqual(result["headReads"],0); self.assertTrue(result["committedAuditEstablished"])
        self.assertTrue(all(c!="get-current" for c,_ in adapter.calls[before:]))

    def test_14_audit_identity_mismatch_fails(self):
        adapter,head=self.prepare(); adapter.endpoint="http://127.0.0.1:8193/interface"
        with self.assertRaises(SyncSourceError): audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"audit.json")

    def test_15_no_mode_fallback(self):
        source=Path(sync_source_current.__file__).read_text()
        self.assertNotIn('adapter.invoke("resolve-view"', source.split("def audit_current",1)[0])
        self.assertNotIn('adapter.invoke("pin-view"', source)
        self.assertNotIn('adapter.invoke("unpin-retention"', source)

if __name__ == "__main__": unittest.main(verbosity=2)
