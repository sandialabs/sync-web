#!/usr/bin/python3
import base64
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from journal_cli.source import sync_source, sync_source_current
from journal_cli.source.sync_source_adapter import canonical_request_body
from journal_cli.source.sync_source_current import (
    _canonical_json, _read, audit_current, current_get, head_helper_mutator,
    partition_reads, preflight_destination, publish_current, pull_current, read_held_boundary,
)
from journal_cli.source.sync_source_model import CurrentReleaseReference, SyncSourceError, parse_current_release, sha256
from tests.test_fast_current import ENDPOINT, StoreAdapter

VALID_RELEASE = "00000000-0000-4000-8000-000000000041"


class CountingFailureAdapter:
    def __init__(self, mode="reject"):
        self.endpoint=ENDPOINT; self.calls=[]; self.mode=mode
    def invoke(self, command, request):
        self.calls.append((command,request))
        if self.mode=="raise": raise RuntimeError("lost completion")
        if self.mode=="reject": return {"outcome":"rejected"}
        if self.mode=="malformed": return {"outcome":"retrieved","results":[],"currentEvidence":{}}
        raise AssertionError(self.mode)


class V041Tests(unittest.TestCase):
    def setUp(self): self.tmp=tempfile.TemporaryDirectory(); self.root=Path(self.tmp.name)
    def tearDown(self): self.tmp.cleanup()
    def source(self):
        p=self.root/"source";p.mkdir(exist_ok=True);(p/"a").write_bytes(b"a");return p
    def publish(self,adapter=None):
        adapter=adapter or StoreAdapter()
        head,receipt=publish_current(adapter,self.source(),route=[],owner="owner",endpoint=ENDPOINT,project_id="project",
            project="project",release_label="r",head_path=("source","project","current-head.scm"),receipt_path=self.root/"publish.json",
            expected_old_head=None,advance_head=lambda *_:{},release_id=VALID_RELEASE)
        return adapter,head,receipt

    def test_01_existing_destination_zero_dispatch(self):
        adapter=StoreAdapter();dest=self.root/"dest";dest.mkdir()
        with self.assertRaises(SyncSourceError): pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=dest,receipt_path=self.root/"r.json")
        self.assertEqual(adapter.calls,[])

    def test_02_unsafe_destination_parent_zero_dispatch(self):
        adapter=StoreAdapter();parent=self.root/"unsafe";parent.mkdir();os.chmod(parent,0o777)
        try:
            with self.assertRaises(SyncSourceError): pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=parent/"d",receipt_path=self.root/"r.json")
        finally: os.chmod(parent,0o700)
        self.assertEqual(adapter.calls,[])

    def test_03_invalid_fast_head_zero_mutation(self):
        adapter=StoreAdapter()
        with self.assertRaises(SyncSourceError): publish_current(adapter,self.source(),route=[],owner="owner",endpoint=ENDPOINT,project_id="project",project="p",release_label="r",head_path=("source","project","head.scm"),receipt_path=self.root/"r.json",expected_old_head=None,advance_head=lambda *_:{},release_id=VALID_RELEASE)
        self.assertEqual(adapter.calls,[])

    def test_04_unvalidated_advancer_zero_mutation(self):
        adapter=StoreAdapter()
        with self.assertRaises(SyncSourceError): publish_current(adapter,self.source(),route=[],owner="owner",endpoint=ENDPOINT,project_id="project",project="p",release_label="r",head_path=("source","project","current-head.scm"),receipt_path=self.root/"r.json",expected_old_head=b"old",advance_head=lambda *_:{"outcome":"accepted"},release_id=VALID_RELEASE)
        self.assertEqual(adapter.calls,[])

    def test_05_helper_symlink_rejected_without_pycache(self):
        source=Path(__file__).parent/"head-helper-qualification-v0.1.4/helper/head_expected_old_cas.py"
        link=self.root/"helper.py";link.symlink_to(source)
        with self.assertRaises(OSError): head_helper_mutator(link,hashlib.sha256(source.read_bytes()).hexdigest())
        self.assertFalse((self.root/"__pycache__").exists())

    def test_06_helper_held_bytes_load_without_pycache(self):
        source=Path(__file__).parent/"head-helper-qualification-v0.1.4/helper/head_expected_old_cas.py";copy=self.root/"helper.py";copy.write_bytes(source.read_bytes());os.chmod(copy,0o600)
        mutator=head_helper_mutator(copy,hashlib.sha256(copy.read_bytes()).hexdigest())
        self.assertTrue(mutator.capability_validated);self.assertFalse((self.root/"__pycache__").exists())

    def test_07_publish_rejection_receipt_truth(self):
        adapter=CountingFailureAdapter("reject")
        with self.assertRaises(SyncSourceError): publish_current(adapter,self.source(),route=[],owner="owner",endpoint=ENDPOINT,project_id="project",project="p",release_label="r",head_path=("source","project","current-head.scm"),receipt_path=self.root/"r.json",expected_old_head=None,advance_head=lambda *_:{},release_id=VALID_RELEASE)
        r=json.loads((self.root/"r.json").read_text());self.assertEqual((r["outcome"],r["dispatchCount"],r["mutationDispatchCount"],r["acceptedMutationPrefix"]),("rejected",1,1,0))

    def test_08_publish_ambiguity_not_not_attempted(self):
        adapter=CountingFailureAdapter("raise")
        with self.assertRaises(RuntimeError): publish_current(adapter,self.source(),route=[],owner="owner",endpoint=ENDPOINT,project_id="project",project="p",release_label="r",head_path=("source","project","current-head.scm"),receipt_path=self.root/"r.json",expected_old_head=None,advance_head=lambda *_:{},release_id=VALID_RELEASE)
        r=json.loads((self.root/"r.json").read_text());self.assertEqual(r["outcome"],"failed-or-ambiguous");self.assertEqual(r["dispatchCount"],1)

    def test_09_denied_current_records_one_dispatch(self):
        adapter=CountingFailureAdapter("reject")
        with self.assertRaises(SyncSourceError): pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=self.root/"d",receipt_path=self.root/"r.json")
        r=json.loads((self.root/"r.json").read_text());self.assertEqual((r["outcome"],r["dispatchCount"]),("rejected",1));self.assertFalse((self.root/"d").exists())

    def test_10_malformed_current_is_terminal_with_counter(self):
        adapter=CountingFailureAdapter("malformed")
        with self.assertRaises(SyncSourceError): pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=self.root/"d",receipt_path=self.root/"r.json")
        r=json.loads((self.root/"r.json").read_text());self.assertEqual(r["dispatchCount"],1);self.assertNotEqual(r["outcome"],"not-attempted")

    def test_11_request_and_result_digests_are_canonical_complete(self):
        adapter=StoreAdapter();path=("source","p","x");adapter.store[path]=b"x";reads=[_read(path,1,sha256(b"x"))]
        _,_,tx=current_get(adapter,["route"],"owner",reads);request=adapter.calls[-1][1]
        self.assertEqual(tx["requestSha256"],sha256(canonical_request_body(request)))
        item=adapter.item(path,b"x");evidence={"contentObservedLocator":None,"contentObservedIndex":None,"committed":False,"routeContinuityObservations":0,"useBatchDispatches":1}
        self.assertEqual(tx["resultEvidenceSha256"],sha256(_canonical_json({"results":[item],"currentEvidence":evidence})))

    def test_12_partition_exact_1024_and_16384(self):
        small=[_read(("tree",f"p{i}"),1,sha256(b"x")) for i in range(1024)]
        self.assertEqual([len(b) for b in partition_reads("owner",small)],[1024])
        large=[_read(("tree",f"p{i}"),1,sha256(b"x")) for i in range(16384)]
        batches=partition_reads("owner",large);self.assertEqual(sum(map(len,batches)),16384);self.assertTrue(all(len(b)<=1024 for b in batches))

    def test_13_current_object_malformed_matrix(self):
        good=CurrentReleaseReference(ENDPOINT,"owner",("source","p","releases",VALID_RELEASE,"release.scm"),1,"0"*64).encode()
        cases=[good.rstrip(b"\n"),good.replace(b"source-current-release-v2",b"unknown",1),good.replace(b'(owner "owner") ',b'',1),good.replace(b'(descriptor-bytes 1)',b'(descriptor-bytes 0)',1),good.replace(b'0'*64,b'A'*64,1)]
        for value in cases:
            with self.subTest(value=value[:40]):
                with self.assertRaises(SyncSourceError): parse_current_release(value)

    def test_14_audit_object_only_prior_null(self):
        adapter,head,_=self.publish();fixed,r=audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"a.json")
        self.assertIsNone(r["priorAssurance"]);self.assertIsNone(r["priorReceiptSha256"]);self.assertEqual(r["assuranceState"],"committed-audit-established")

    def test_15_audit_valid_prior_linkage(self):
        adapter,head,_=self.publish();pull=pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=self.root/"d",receipt_path=self.root/"pull.json")
        raw=(self.root/"pull.json").read_bytes();_,audit=audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"a.json",destination=self.root/"d",prior_receipt_bytes=raw,prior_receipt_sha256=sha256(raw))
        self.assertEqual(audit["priorAssurance"],"materialized-current");self.assertEqual(audit["priorReceiptSha256"],sha256(raw));self.assertEqual(pull["committedAuditStatus"],"committed-audit-pending")

    def test_16_audit_bad_prior_hash_zero_dispatch(self):
        adapter,head,_=self.publish();before=len(adapter.calls)
        with self.assertRaises(SyncSourceError): audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"a.json",prior_receipt_bytes=b"{}\n",prior_receipt_sha256="0"*64)
        self.assertEqual(len(adapter.calls),before)

    def test_17_audit_prior_false_claim_rejected(self):
        adapter,head,_=self.publish();raw=_canonical_json({"outcome":"verified","committed":False,"headSha256":sha256(head.encode()),"contentObservedLocator":None,"selectedIndex":None})
        with self.assertRaises(SyncSourceError): audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"a.json",prior_receipt_bytes=raw,prior_receipt_sha256=sha256(raw))

    def test_18_audit_failure_is_separate_state(self):
        adapter,head,_=self.publish();adapter.endpoint="http://127.0.0.1:8193/interface"
        with self.assertRaises(SyncSourceError): audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"a.json")
        r=json.loads((self.root/"a.json").read_text());self.assertEqual(r["assuranceState"],"audit-failure");self.assertFalse(r["committedAuditEstablished"])

    def test_19_five_states_are_nonrewriting_labels(self):
        states=["materialized-current","committed-audit-pending","committed-audit-established","retained","audit-failure"]
        self.assertEqual(len(states),len(set(states)))
        source=Path(sync_source_current.__file__).read_text();self.assertNotIn("write_text",source)

    def test_20_two_normalized_runs_equal(self):
        normalized=[]
        for suffix in ("a","b"):
            adapter,head,_=self.publish();dest=self.root/f"d{suffix}";receipt=self.root/f"r{suffix}.json"
            value=pull_current(adapter,route=[],owner="owner",endpoint=ENDPOINT,head_path=("source","project","current-head.scm"),destination=dest,receipt_path=receipt)
            for key in ("operationId","attemptedAt","completedAt","destination"): value.pop(key,None)
            normalized.append(value)
        self.assertEqual(normalized[0],normalized[1])

    def test_21_package_lint_rejects_bytecode(self):
        bad=[p for p in Path(__file__).parent.rglob("*") if p.is_file() and (p.suffix in {".pyc",".pyo"} or "__pycache__" in p.parts)]
        self.assertEqual(bad,[])

    def test_22_marker_denial_never_attempts_head(self):
        class MarkerDeny(StoreAdapter):
            def invoke(inner,command,request):
                if command=="get-current": inner.calls.append((command,request));return {"outcome":"rejected"}
                return super(MarkerDeny,inner).invoke(command,request)
        adapter=MarkerDeny()
        with self.assertRaises(SyncSourceError): publish_current(adapter,self.source(),route=[],owner="owner",endpoint=ENDPOINT,project_id="project",project="p",release_label="r",head_path=("source","project","current-head.scm"),receipt_path=self.root/"r.json",expected_old_head=None,advance_head=lambda *_:{},release_id=VALID_RELEASE)
        self.assertFalse(any(c=="create-value" and r["path"][-1]=="current-head.scm" for c,r in adapter.calls))
        receipt=json.loads((self.root/"r.json").read_text());self.assertEqual(receipt["outcome"],"rejected");self.assertTrue(receipt["markerAccepted"]);self.assertFalse(receipt["headMutationAccepted"])

    def test_25_prior_receipt_duplicate_and_noncanonical_rejected(self):
        adapter,head,_=self.publish();duplicate=b'{"outcome":"materialized-current","outcome":"materialized-current"}\n'
        for raw in (duplicate,b'{ "outcome": "materialized-current" }\n'):
            before=len(adapter.calls)
            with self.assertRaises(SyncSourceError): audit_current(adapter,head.encode(),route=[],receipt_path=self.root/"a.json",prior_receipt_bytes=raw,prior_receipt_sha256=sha256(raw))
            self.assertEqual(len(adapter.calls),before)

    def test_26_retention_is_absent_from_fast_operations(self):
        source=Path(sync_source_current.__file__).read_text()
        self.assertNotIn('adapter.invoke("pin-view"',source);self.assertNotIn('adapter.invoke("unpin-retention"',source)
        self.assertIn('"retained"',Path(__file__).read_text())

    def test_27_current_reference_duplicate_reordered_unknown_rejected(self):
        good=CurrentReleaseReference(ENDPOINT,"owner",("source","p","releases",VALID_RELEASE,"release.scm"),1,"0"*64).encode()
        duplicate=good.replace(b'(owner "owner")',b'(owner "owner") (owner "owner")',1)
        reordered=good.replace(b'(owner "owner") (journal ',b'(journal ',1).replace(b') (descriptor-path',b') (owner "owner") (descriptor-path',1)
        unknown=good.replace(b'(descriptor-bytes 1)',b'(extra "x") (descriptor-bytes 1)',1)
        for raw in (duplicate,reordered,unknown):
            with self.assertRaises(SyncSourceError): parse_current_release(raw)

    def test_28_expected_old_boundary_is_held_nofollow(self):
        target=self.root/"old";target.write_bytes(b"old");os.chmod(target,0o600);link=self.root/"link";link.symlink_to(target)
        self.assertEqual(read_held_boundary(target),b"old")
        with self.assertRaises(OSError): read_held_boundary(link)

if __name__=="__main__": unittest.main(verbosity=2)
