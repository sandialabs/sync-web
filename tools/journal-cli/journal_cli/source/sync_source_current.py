#!/usr/bin/python3
"""Fail-closed fast-current v1.1 publication, materialization, and fixed audit."""

from __future__ import annotations

import base64
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import os
import stat
import sys
import tempfile
import types
import uuid
from typing import Any, Callable

from .ops import MAX_RESPONSE_BYTES, RequestError, current_response_bound_proofs, scheme_symbol
from .sync_source_adapter import Adapter, PreDispatchAdapterError, canonical_request_body, require_outcome
from .sync_source_fs import _open_absolute_directory, materialize, snapshot_source, verify_tree
from .sync_source_model import (
    Chunk, CurrentReleaseReference, FixedReference, LOWER_UUID, SyncSourceError,
    canonical_endpoint, parse_current_release, parse_release, sha256, tree_digest,
    validate_journal_component, validate_release_binding,
)
from .sync_source import base_receipt, create_value, now, require_adapter_endpoint, resolve_view, validate_route, write_receipt

MAX_BATCH_PATHS = 1024
MAX_VALUE_BYTES = 524_288


def _canonical_json(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True, separators=(",", ":")) + "\n").encode()


@dataclass(frozen=True)
class DestinationBinding:
    destination: Path
    parent: Path
    name: str
    parent_dev: int
    parent_ino: int


def preflight_destination(destination: Path) -> DestinationBinding:
    """Prove safe parent and absence before any adapter dispatch."""
    destination = destination.absolute()
    if destination.name in {"", ".", ".."}:
        raise SyncSourceError("unsafe-destination", "Destination name is invalid")
    parent = destination.parent
    fd = _open_absolute_directory(parent)
    try:
        info = os.fstat(fd)
        if info.st_uid != os.geteuid() or info.st_mode & 0o022:
            raise SyncSourceError("unsafe-destination", "Destination parent is not owner-controlled")
        try:
            os.stat(destination.name, dir_fd=fd, follow_symlinks=False)
        except FileNotFoundError:
            pass
        else:
            raise SyncSourceError("destination-exists", "Destination already exists")
        return DestinationBinding(destination, parent, destination.name, info.st_dev, info.st_ino)
    finally:
        os.close(fd)


def recheck_destination(binding: DestinationBinding) -> None:
    fd = _open_absolute_directory(binding.parent)
    try:
        info = os.fstat(fd)
        if (info.st_dev, info.st_ino) != (binding.parent_dev, binding.parent_ino):
            raise SyncSourceError("destination-race", "Destination parent binding changed")
        try:
            os.stat(binding.name, dir_fd=fd, follow_symlinks=False)
        except FileNotFoundError:
            return
        raise SyncSourceError("destination-exists", "Destination appeared before materialization")
    finally:
        os.close(fd)


class TrackingAdapter:
    def __init__(self, adapter: Adapter):
        self.adapter = adapter; self.endpoint = adapter.endpoint
        self.dispatch_count = 0; self.mutation_dispatch_count = 0
        self.accepted_prefix = 0; self.phase = "preflight"

    def invoke(self, command: str, request: dict[str, Any]) -> dict[str, Any]:
        counted = command in {"get-current", "resolve-view", "pin-view", "unpin-retention", "create-value"}
        mutation = command == "create-value"
        if counted: self.dispatch_count += 1
        if mutation: self.mutation_dispatch_count += 1
        try:
            result = self.adapter.invoke(command, request)
        except PreDispatchAdapterError:
            if counted: self.dispatch_count -= 1
            if mutation: self.mutation_dispatch_count -= 1
            raise
        if mutation and result.get("outcome") == "accepted": self.accepted_prefix += 1
        return result


def _truth(receipt: dict[str, Any], tracked: TrackingAdapter, phase: str) -> None:
    receipt.update({"phase": phase, "dispatchCount": tracked.dispatch_count,
                    "mutationDispatchCount": tracked.mutation_dispatch_count,
                    "acceptedMutationPrefix": tracked.accepted_prefix})


def _failure_outcome(error: Exception, tracked: TrackingAdapter) -> str:
    if isinstance(error, PreDispatchAdapterError) and tracked.dispatch_count == 0:
        return "not-attempted"
    return error.code if isinstance(error, SyncSourceError) else "failed-or-ambiguous"


def _decode_result(item: Any, path: tuple[str, ...]) -> bytes:
    if not isinstance(item, dict) or item.get("path") != list(path) or item.get("shape") != "value":
        raise SyncSourceError("content-mismatch", "Current result path or shape differs")
    try:
        data = base64.b64decode(item["contentBase64"], validate=True)
    except Exception as exc:
        raise SyncSourceError("adapter-framing", "Current result Base64 is invalid") from exc
    if item.get("bytes") != len(data) or item.get("sha256") != sha256(data):
        raise SyncSourceError("content-mismatch", "Current result byte evidence differs")
    return data


def current_get(adapter: Adapter, route: list[str], owner: str, reads: list[dict[str, Any]]) -> tuple[list[bytes], dict[str, Any], dict[str, Any]]:
    route = validate_route(route); validate_journal_component(owner)
    request = {"version": 2, "route": route, "owner": owner, "reads": reads,
               "rawResponseBytesUpperBound": MAX_RESPONSE_BYTES, "responseBytesUpperBound": MAX_RESPONSE_BYTES}
    request_bytes = canonical_request_body(request)
    request_digest = sha256(request_bytes)
    result = require_outcome(adapter.invoke("get-current", request), {"retrieved"})
    if result.get("requestSha256") != request_digest:
        raise SyncSourceError("adapter-framing", "Low-level request digest is missing or differs")
    items = result.get("results")
    if not isinstance(items, list) or len(items) != len(reads):
        raise SyncSourceError("adapter-framing", "Current result count differs")
    evidence = result.get("currentEvidence")
    if (not isinstance(evidence, dict) or evidence.get("contentObservedLocator") is not None
            or evidence.get("contentObservedIndex") is not None or evidence.get("committed") is not False
            or evidence.get("routeContinuityObservations") != 0 or evidence.get("useBatchDispatches") != 1):
        raise SyncSourceError("adapter-framing", "Current evidence overclaims assurance")
    values = [_decode_result(item, tuple(read["path"])) for item, read in zip(items, reads)]
    result_evidence = {"results": items, "currentEvidence": evidence}
    transcript = {"count": len(reads), "requestBytes": len(request_bytes),
                  "requestSha256": sha256(request_bytes),
                  "resultEvidenceBytes": len(_canonical_json(result_evidence)),
                  "resultEvidenceSha256": sha256(_canonical_json(result_evidence))}
    return values, evidence, transcript


def _read(path: tuple[str, ...], maximum: int, expected_sha: str | None = None) -> dict[str, Any]:
    value: dict[str, Any] = {"path": list(path), "maxBytes": maximum}
    if expected_sha is not None:
        value["expected"] = {"bytes": maximum, "sha256": expected_sha}
    return value


def partition_reads(owner: str, reads: list[dict[str, Any]]) -> list[list[dict[str, Any]]]:
    result: list[list[dict[str, Any]]] = []; offset = 0
    while offset < len(reads):
        best = offset; stop = min(len(reads), offset + MAX_BATCH_PATHS)
        for end in range(offset + 1, stop + 1):
            raw, encoded = current_response_bound_proofs(owner, reads[offset:end])
            if raw > MAX_RESPONSE_BYTES or encoded > MAX_RESPONSE_BYTES: break
            best = end
        if best == offset:
            raise SyncSourceError("unsupported-capability", "One current read cannot fit bounded response proofs")
        result.append(reads[offset:best]); offset = best
    return result


def validate_fast_head_path(owner: str, head_path: tuple[str, ...]) -> str:
    validate_journal_component(owner)
    if not isinstance(head_path, tuple) or len(head_path) != 3 or head_path[0] != "source" or head_path[2] != "current-head.scm":
        raise SyncSourceError("unsupported-capability", "Fast head must be source/<project>/current-head.scm")
    for component in (owner, *head_path):
        validate_journal_component(component)
        try: scheme_symbol(component)
        except RequestError as exc: raise SyncSourceError("unsupported-capability", "Fast head component is not safely representable") from exc
    return head_path[1]


def read_held_boundary(path: Path) -> bytes:
    path = path.absolute(); parent_fd = _open_absolute_directory(path.parent); fd = -1
    try:
        parent_info = os.fstat(parent_fd)
        if parent_info.st_uid != os.geteuid() or parent_info.st_mode & 0o022:
            raise SyncSourceError("unsafe-input", "Boundary parent is unsafe")
        fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0), dir_fd=parent_fd)
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid() or before.st_nlink != 1 or before.st_mode & 0o022 or before.st_size > MAX_VALUE_BYTES:
            raise SyncSourceError("unsafe-input", "Boundary must be an owner-controlled bounded single-link file")
        data = b""
        while len(data) <= MAX_VALUE_BYTES:
            part = os.read(fd, min(65536, MAX_VALUE_BYTES + 1 - len(data)))
            if not part: break
            data += part
        after = os.fstat(fd)
        if len(data) > MAX_VALUE_BYTES or (before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns,before.st_ctime_ns)!=(after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns):
            raise SyncSourceError("source-changed", "Boundary changed while held")
        return data
    finally:
        if fd >= 0: os.close(fd)
        os.close(parent_fd)


def _load_held_helper(helper_path: Path, expected_sha256: str) -> types.ModuleType:
    helper_path = helper_path.absolute(); parent_fd = _open_absolute_directory(helper_path.parent)
    fd = -1
    try:
        parent_info = os.fstat(parent_fd)
        if parent_info.st_uid != os.geteuid() or parent_info.st_mode & 0o022:
            raise SyncSourceError("unsafe-helper", "Head helper parent is unsafe")
        flags = os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0)
        fd = os.open(helper_path.name, flags, dir_fd=parent_fd)
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid() or before.st_nlink != 1 or before.st_mode & 0o022:
            raise SyncSourceError("unsafe-helper", "Head helper must be an owner-controlled single-link file")
        pieces=[]
        while True:
            part=os.read(fd,65536)
            if not part: break
            pieces.append(part)
        after=os.fstat(fd); data=b"".join(pieces)
        if (before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns,before.st_ctime_ns)!=(after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns):
            raise SyncSourceError("source-changed", "Head helper changed while held")
        if hashlib.sha256(data).hexdigest()!=expected_sha256:
            raise SyncSourceError("unsupported-capability", "Head helper source hash differs")
        module=types.ModuleType("qualified_head_expected_old_cas_v014");module.__file__=str(helper_path)
        old=sys.dont_write_bytecode;sys.dont_write_bytecode=True
        try: exec(compile(data,str(helper_path),"exec"),module.__dict__)
        finally: sys.dont_write_bytecode=old
        return module
    finally:
        if fd>=0: os.close(fd)
        os.close(parent_fd)


def head_helper_mutator(helper_path: Path, expected_sha256: str) -> Callable[[str, tuple[str, ...], bytes, bytes], dict[str, Any]]:
    module = _load_held_helper(helper_path, expected_sha256)
    def mutate(owner: str, path: tuple[str, ...], old: bytes, new: bytes) -> dict[str, Any]:
        with tempfile.TemporaryDirectory(prefix="sync-source-head-cas-") as directory:
            old_file=Path(directory)/"old";new_file=Path(directory)/"new"
            old_file.write_bytes(old);new_file.write_bytes(new);os.chmod(old_file,0o600);os.chmod(new_file,0o600)
            request={"owner":owner,"path":list(path),"oldFile":str(old_file),"newFile":str(new_file)}
            return module.run_helper(request,module.LocalInterfaceTransport(Path(module.DEFAULT_CONFIG),Path(module.DEFAULT_STATE)))
    setattr(mutate,"capability_validated",True)
    return mutate


def pull_current(adapter: Adapter, *, endpoint: str, route: list[str], owner: str,
                 head_path: tuple[str, ...], destination: Path, receipt_path: Path, cut=None) -> dict[str, Any]:
    endpoint=canonical_endpoint(endpoint);route=validate_route(route);validate_journal_component(owner)
    validate_fast_head_path(owner, head_path)
    binding=preflight_destination(destination)
    operation_id=uuid.uuid4().hex;receipt=base_receipt("pull-current",operation_id);tracked=TrackingAdapter(adapter)
    receipt.update({"requestedEndpoint":endpoint,"requestedRoute":route,"requestedOwner":owner,
                    "referenceDeclaredOwner":owner,"contentObservedLocator":None,"selectedIndex":None,
                    "committed":False,"destination":str(binding.destination),"destinationPreflight":"absent-safe-bound",
                    "installed":False,"executed":False,"batchEvidence":[]})
    _truth(receipt,tracked,"preflight-complete");write_receipt(receipt_path,receipt)
    try:
        head_values,head_evidence,head_tx=current_get(tracked,route,owner,[_read(head_path,MAX_VALUE_BYTES)])
        _truth(receipt,tracked,"head-captured");head=parse_current_release(head_values[0])
        if head.endpoint!=endpoint or head.owner!=owner:
            raise SyncSourceError("reference-mismatch","Current head declared locator differs")
        frozen_head_b64 = base64.b64encode(head_values[0]).decode("ascii")
        if base64.b64decode(frozen_head_b64, validate=True) != head_values[0]:
            raise SyncSourceError("adapter-framing", "Frozen current object Base64 roundtrip differs")
        receipt.update({"headPath":list(head_path),"headBytes":len(head_values[0]),"headSha256":sha256(head_values[0]),
                        "frozenCurrentObjectBase64":frozen_head_b64,"headEvidence":head_evidence,"headTranscript":head_tx})
        descriptor_values,descriptor_evidence,descriptor_tx=current_get(tracked,route,owner,[_read(head.descriptor_path,head.descriptor_bytes,head.descriptor_sha256)])
        _truth(receipt,tracked,"descriptor-captured");descriptor=descriptor_values[0];release=parse_release(descriptor)
        validate_release_binding(FixedReference(endpoint,owner,0,head.descriptor_path,head.descriptor_bytes,head.descriptor_sha256),release)
        root=head.descriptor_path[:-1];reads=[_read((*root,*chunk.path),chunk.bytes,chunk.sha256) for chunk in release.chunks];chunks={}
        for batch in partition_reads(owner,reads):
            values,evidence,transcript=current_get(tracked,route,owner,batch)
            for read,data in zip(batch,values):
                path=tuple(read["path"])
                if path in chunks: raise SyncSourceError("descriptor-shape","Chunk path repeats")
                chunks[path]=data
            receipt["batchEvidence"].append({**transcript,"evidence":evidence});_truth(receipt,tracked,"resources-capturing");write_receipt(receipt_path,receipt)
        recheck_destination(binding)
        result=materialize(binding.destination,release,lambda chunk:chunks[(*root,*chunk.path)],receipt_path,operation_id=operation_id,cut=cut)
        receipt.update({"descriptorPath":list(head.descriptor_path),"descriptorBytes":len(descriptor),"descriptorSha256":sha256(descriptor),
                        "descriptorEvidence":descriptor_evidence,"descriptorTranscript":descriptor_tx,"entries":len(release.entries),
                        "chunks":len(release.chunks),"aggregateBytes":release.aggregate_bytes,"treeSha256":result.tree_sha256,
                        "batchCount":len(receipt["batchEvidence"]),"stagingCleanup":"complete","assuranceState":"materialized-current",
                        "committedAuditStatus":"committed-audit-pending","outcome":"materialized-current","completedAt":now()})
        receipt["states"].update({"retrieved":True,"verified":True,"materialized":True});_truth(receipt,tracked,"materialized");write_receipt(receipt_path,receipt);return receipt
    except Exception as error:
        outcome=_failure_outcome(error,tracked)
        receipt.update({"completedAt":now(),"outcome":outcome,"error":str(error)});_truth(receipt,tracked,"terminal-failure");write_receipt(receipt_path,receipt);raise


def publish_current(adapter: Adapter, source: Path, *, endpoint: str, route: list[str], owner: str,
                    project_id: str, project: str, release_label: str, head_path: tuple[str, ...],
                    receipt_path: Path, expected_old_head: bytes | None,
                    advance_head: Callable[[str,tuple[str,...],bytes,bytes],dict[str,Any]],
                    release_id: str | None=None) -> tuple[CurrentReleaseReference,dict[str,Any]]:
    endpoint=require_adapter_endpoint(adapter,endpoint);route=validate_route(route)
    if validate_fast_head_path(owner,head_path) != project_id:
        raise SyncSourceError("unsupported-capability","Fast head project differs from publication project")
    if expected_old_head is not None:
        if not isinstance(expected_old_head,bytes) or len(expected_old_head)>MAX_VALUE_BYTES: raise SyncSourceError("unsupported-capability","Expected old head boundary is invalid")
        if not getattr(advance_head,"capability_validated",False): raise SyncSourceError("unsupported-capability","Expected-old helper capability is unvalidated")
    release_id=release_id or str(uuid.uuid4())
    if not LOWER_UUID.fullmatch(release_id): raise SyncSourceError("invalid-release-id","Release ID must be a lowercase UUID")
    operation_id=uuid.uuid4().hex;receipt=base_receipt("publish-current",operation_id);tracked=TrackingAdapter(adapter)
    receipt.update({"requestedRoute":route,"requestedOwner":owner,"headPath":list(head_path),"markerAccepted":False,
                    "markerObservedCurrent":False,"headMutationAccepted":False,"headVisibility":"unobserved"})
    _truth(receipt,tracked,"preflight-complete");write_receipt(receipt_path,receipt)
    try:
        with tempfile.TemporaryDirectory(prefix="sync-source-current-publish-") as temporary:
            snapshot=snapshot_source(source.absolute(),Path(temporary)/"snapshot",project,release_label)
            root=("source",project_id,"releases",release_id);descriptor_path=(*root,"release.scm");descriptor=snapshot.release.encode()
            descriptor_file=Path(temporary)/"release.scm";descriptor_file.write_bytes(descriptor);os.chmod(descriptor_file,0o600)
            for chunk in snapshot.release.chunks:
                create_value(tracked,owner,(*root,*chunk.path),snapshot.chunk_files[chunk.path]);_truth(receipt,tracked,"resources-creating");write_receipt(receipt_path,receipt)
            create_value(tracked,owner,descriptor_path,descriptor_file);receipt["markerAccepted"]=True;_truth(receipt,tracked,"marker-accepted");write_receipt(receipt_path,receipt)
            values,marker_evidence,marker_tx=current_get(tracked,route,owner,[_read(descriptor_path,len(descriptor),sha256(descriptor))])
            if values[0]!=descriptor: raise SyncSourceError("content-mismatch","Current marker differs")
            receipt.update({"markerObservedCurrent":True,"markerEvidence":marker_evidence,"markerTranscript":marker_tx});_truth(receipt,tracked,"marker-observed")
            head=CurrentReleaseReference(endpoint,owner,descriptor_path,len(descriptor),sha256(descriptor));head_bytes=head.encode()
            if expected_old_head is None:
                head_file=Path(temporary)/"current-head.scm";head_file.write_bytes(head_bytes);os.chmod(head_file,0o600);mutation=create_value(tracked,owner,head_path,head_file)
            else:
                tracked.dispatch_count+=1;tracked.mutation_dispatch_count+=1
                mutation=advance_head(owner,head_path,expected_old_head,head_bytes)
                if mutation.get("outcome")=="accepted": tracked.accepted_prefix+=1
                if mutation.get("outcome")!="accepted": raise SyncSourceError(str(mutation.get("outcome")),"Head advancement did not complete")
            receipt.update({"headMutationAccepted":True,"headMutation":mutation,"headVisibility":"unobserved","releaseId":release_id,
                            "descriptorPath":list(descriptor_path),"descriptorBytes":len(descriptor),"descriptorSha256":sha256(descriptor),
                            "entries":len(snapshot.release.entries),"chunks":len(snapshot.release.chunks),"aggregateBytes":snapshot.release.aggregate_bytes,
                            "treeSha256":tree_digest(snapshot.release),"outcome":"current-marker-observed-head-write-accepted","completedAt":now()})
            _truth(receipt,tracked,"head-mutation-accepted");write_receipt(receipt_path,receipt);return head,receipt
    except Exception as error:
        outcome=_failure_outcome(error,tracked)
        receipt.update({"completedAt":now(),"outcome":outcome,"error":str(error)});_truth(receipt,tracked,"terminal-failure");write_receipt(receipt_path,receipt);raise


def _validate_prior_receipt(prior_bytes: bytes, prior_sha256: str, current_bytes: bytes) -> dict[str,Any]:
    if sha256(prior_bytes)!=prior_sha256: raise SyncSourceError("prior-receipt-mismatch","Prior receipt hash differs")
    def pairs(items):
        value = {}
        for key, item in items:
            if key in value: raise ValueError("duplicate field")
            value[key] = item
        return value
    try:
        value=json.loads(prior_bytes,object_pairs_hook=pairs,
                         parse_constant=lambda _value: (_ for _ in ()).throw(ValueError("non-finite")))
    except Exception as exc: raise SyncSourceError("prior-receipt-mismatch","Prior receipt is malformed") from exc
    if _canonical_json(value) != prior_bytes:
        raise SyncSourceError("prior-receipt-mismatch","Prior receipt is noncanonical")
    if not isinstance(value,dict) or value.get("outcome")!="materialized-current" or value.get("committed") is not False:
        raise SyncSourceError("prior-receipt-mismatch","Prior receipt is not immutable materialized-current evidence")
    if value.get("headSha256")!=sha256(current_bytes) or value.get("contentObservedLocator") is not None or value.get("selectedIndex") is not None:
        raise SyncSourceError("prior-receipt-mismatch","Prior receipt current binding differs")
    return value


def audit_current(adapter: Adapter,current_bytes: bytes,*,route:list[str],receipt_path:Path,destination:Path|None=None,
                  prior_receipt_bytes:bytes|None=None,prior_receipt_sha256:str|None=None)->tuple[FixedReference,dict[str,Any]]:
    current=parse_current_release(current_bytes);route=validate_route(route);tracked=TrackingAdapter(adapter)
    if (prior_receipt_bytes is None)!=(prior_receipt_sha256 is None): raise SyncSourceError("prior-receipt-mismatch","Prior receipt bytes and hash must be supplied together")
    prior=None if prior_receipt_bytes is None else _validate_prior_receipt(prior_receipt_bytes,prior_receipt_sha256,current_bytes)
    operation_id=uuid.uuid4().hex;receipt=base_receipt("audit-current",operation_id)
    receipt.update({"currentReferenceSha256":sha256(current_bytes),"headReads":0,"priorAssurance":None if prior is None else "materialized-current",
                    "priorReceiptSha256":prior_receipt_sha256,"committedAuditEstablished":False,"assuranceState":"committed-audit-pending"});_truth(receipt,tracked,"preflight-complete");write_receipt(receipt_path,receipt)
    try:
        observed,values=resolve_view(tracked,current.endpoint,route,current.owner,"current",[current.descriptor_path]);descriptor=values[0][1]
        if descriptor is None or len(descriptor)!=current.descriptor_bytes or sha256(descriptor)!=current.descriptor_sha256: raise SyncSourceError("audit-mismatch","Committed descriptor differs")
        release=parse_release(descriptor);fixed=FixedReference(current.endpoint,current.owner,observed["selectedIndex"],current.descriptor_path,len(descriptor),sha256(descriptor),tuple(observed["historyIndexes"]));validate_release_binding(fixed,release);root=current.descriptor_path[:-1]
        for chunk in release.chunks:
            _,chunk_values=resolve_view(tracked,fixed.endpoint,route,current.owner,fixed,[(*root,*chunk.path)]);data=chunk_values[0][1]
            if data is None or len(data)!=chunk.bytes or sha256(data)!=chunk.sha256: raise SyncSourceError("audit-mismatch","Committed chunk differs")
        if destination is not None and verify_tree(destination,release)!=tree_digest(release): raise SyncSourceError("audit-mismatch","Materialized destination differs")
        receipt.update({"fixedReference":{"endpoint":fixed.endpoint,"owner":fixed.owner,"index":fixed.index,"descriptorPath":list(fixed.descriptor_path),"descriptorBytes":fixed.descriptor_bytes,"descriptorSha256":fixed.descriptor_sha256},
                        "observedView":observed,
                        "fixedReferenceBytesBase64":base64.b64encode(fixed.encode()).decode(),"committedAuditEstablished":True,
                        "assuranceState":"committed-audit-established","treeSha256":tree_digest(release),"outcome":"committed-audit-established","completedAt":now()})
        receipt["states"].update({"retrieved":True,"verified":True});_truth(receipt,tracked,"audit-established");write_receipt(receipt_path,receipt);return fixed,receipt
    except Exception as error:
        outcome=_failure_outcome(error,tracked);receipt.update({"completedAt":now(),"outcome":outcome,"assuranceState":"audit-failure","error":str(error)});_truth(receipt,tracked,"terminal-failure");write_receipt(receipt_path,receipt);raise
