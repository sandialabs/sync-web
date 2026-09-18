#!/usr/bin/python3
"""Inert Sync Web Source Code Publication v1 reference candidate."""

from __future__ import annotations

import sys
sys.dont_write_bytecode = True

import argparse
import base64
from dataclasses import replace
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import tempfile
import time
import uuid
from typing import Any, Callable, Iterable

from .sync_source_adapter import Adapter, ModuleAdapter
from .sync_source_fs import _open_absolute_directory, materialize, recover, snapshot_source, verify_tree
from .sync_source_model import (
    Chunk, CurrentReference, FileEntry, FixedReference, Release, SyncSourceError, LOWER_SHA256, LOWER_UUID,
    MAX_DESCRIPTOR_BYTES, canonical_endpoint, parse_reference, parse_release, sha256, tree_digest,
    validate_journal_component, validate_release_binding,
)

VERSION = "journal-cli-source-v2.0.0"
SPECIFICATION_SHA256 = "f8806ba73fdcd3b76da95fd0d43114cd4f016ba8449532e88bf5d11bbff93fae"
MAX_BATCH_PATHS = 5
MAX_PIN_BATCH_PATHS = 1
ROUTE_COMPONENT = re.compile(r"^[A-Za-z0-9_.*+!<>=?/-]{1,128}$")


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def require_adapter_endpoint(adapter: Adapter, expected: str) -> str:
    endpoint = canonical_endpoint(expected)
    try:
        active = canonical_endpoint(adapter.endpoint)
    except (AttributeError, SyncSourceError) as exc:
        raise SyncSourceError("adapter-framing", "Low-level adapter did not expose its active endpoint") from exc
    if active != endpoint:
        raise SyncSourceError("endpoint-mismatch", "Declared Source endpoint differs from the active Interface endpoint")
    return endpoint


def source_provenance(root: Path | None = None) -> str:
    root = root or Path(__file__).resolve().parent
    names = ["sync_source.py", "sync_source_current.py", "sync_source_model.py", "sync_source_fs.py", "sync_source_adapter.py", "ops.py"]
    rows = []
    for name in names:
        path = root / name
        if path.exists(): rows.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}\n")
    return hashlib.sha256("".join(rows).encode()).hexdigest()


def write_receipt(path: Path, value: dict[str, Any]) -> None:
    path = path.absolute(); path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    parent = _open_absolute_directory(path.parent)
    parent_info = os.fstat(parent)
    if parent_info.st_uid != os.geteuid() or parent_info.st_mode & 0o022:
        os.close(parent); raise SyncSourceError("unsafe-receipt", "Receipt parent must be owner-controlled")
    temporary = f".receipt-{uuid.uuid4().hex}"
    fd = -1
    try:
        fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        data = canonical_json(value); offset = 0
        while offset < len(data):
            count = os.write(fd, data[offset:])
            if count <= 0: raise OSError("receipt write made no progress")
            offset += count
        os.fsync(fd); os.close(fd); fd = -1
        os.rename(temporary, path.name, src_dir_fd=parent, dst_dir_fd=parent)
        os.fsync(parent)
    finally:
        if fd >= 0: os.close(fd)
        try: os.unlink(temporary, dir_fd=parent)
        except FileNotFoundError: pass
        os.close(parent)


def base_receipt(kind: str, operation_id: str) -> dict[str, Any]:
    return {
        "schema": "journal-cli-source-receipt-v2", "tool": VERSION, "toolProvenanceSha256": source_provenance(),
        "specificationSha256": SPECIFICATION_SHA256,
        "operationId": operation_id, "operation": kind,
        "attemptedAt": now(), "completedAt": None, "outcome": "attempted",
        "states": {"attempted": True, "writeAccepted": False, "retrieved": False, "verified": False, "materialized": False, "installed": False, "executed": False},
        "installed": False, "executed": False,
    }


def _decode_item(item: dict[str, Any], expected_path: tuple[str, ...]) -> bytes | None:
    if item.get("path") != list(expected_path): raise SyncSourceError("adapter-framing", "Low-level item path changed")
    if item.get("shape") == "nothing": return None
    if item.get("shape") != "value": raise SyncSourceError("adapter-framing", "Low-level item shape is invalid")
    try: data = base64.b64decode(item["contentBase64"], validate=True)
    except Exception as exc: raise SyncSourceError("adapter-framing", "Low-level content Base64 is invalid") from exc
    if item.get("bytes") != len(data) or item.get("sha256") != sha256(data):
        raise SyncSourceError("adapter-framing", "Low-level content evidence is inconsistent")
    return data


def resolve_view(
    adapter: Adapter, endpoint: str | None, route: list[str], owner: str,
    view: str | FixedReference, paths: list[tuple[str, ...]],
) -> tuple[dict[str, Any], list[tuple[dict[str, Any], bytes | None]]]:
    if isinstance(view, FixedReference):
        view_request = {"kind": "fixed", "index": view.index}
        if view.history_indexes:
            view_request["historyIndexes"] = list(view.history_indexes)
        expected_endpoint = view.endpoint
    else:
        view_request = {"kind": "current"}
        expected_endpoint = canonical_endpoint(endpoint) if endpoint is not None else None
    request = {
        "version": 2, "endpoint": expected_endpoint, "route": route, "owner": owner,
        "view": view_request, "paths": [list(path) for path in paths], "includePinned": True,
    }
    result = adapter.invoke("resolve-view", request)
    if result.get("outcome") != "retrieved":
        raise SyncSourceError(str(result.get("outcome")), "resolve-view did not retrieve")
    observed = result.get("view"); items = result.get("results")
    if not isinstance(observed, dict) or not isinstance(items, list) or len(items) != len(paths):
        raise SyncSourceError("adapter-framing", "resolve-view result is invalid")
    observed_endpoint = observed.get("terminalEndpoint")
    observed_route = observed.get("route")
    observed_history = observed.get("historyIndexes")
    index = observed.get("selectedIndex")
    if (not isinstance(observed_endpoint, str) or observed_route != route
            or not isinstance(observed_history, list) or len(observed_history) != len(route) + 1
            or any(type(item) is not int or item < 0 for item in observed_history)
            or type(index) is not int or index < 0 or observed_history[-1] != index
            or observed.get("owner") != owner):
        raise SyncSourceError("adapter-framing", "resolve-view locator evidence is invalid")
    canonical_endpoint(observed_endpoint)
    if expected_endpoint is not None and observed_endpoint != expected_endpoint:
        raise SyncSourceError("reference-mismatch", "Source endpoint changed")
    if isinstance(view, FixedReference):
        if index != view.index or (view.history_indexes and tuple(observed_history) != view.history_indexes):
            raise SyncSourceError("fixed-view-mismatch", "Selected fixed locator view changed")
    return observed, [(item, _decode_item(item, path)) for item, path in zip(items, paths)]


def create_value(adapter: Adapter, owner: str, path: tuple[str, ...], payload: Path) -> dict[str, Any]:
    result = adapter.invoke("create-value", {"version": 2, "owner": owner, "path": list(path), "input": str(payload.absolute()), "expected": "absent", "readback": False})
    if result.get("outcome") != "accepted": raise SyncSourceError(str(result.get("outcome")), f"create-value stopped: {result.get('error')}")
    data = payload.read_bytes()
    if result.get("bytes") != len(data) or result.get("sha256") != sha256(data):
        raise SyncSourceError("adapter-framing", "create-value evidence is inconsistent")
    return result


def _batches(values: list[Any], size: int = MAX_BATCH_PATHS) -> Iterable[list[Any]]:
    for start in range(0, len(values), size): yield values[start:start + size]


def _verify_remote_values(
    adapter: Adapter, route: list[str], fixed: FixedReference, expected: dict[tuple[str, ...], tuple[int, str]], *, require_pinned: bool = False,
) -> FixedReference:
    paths = list(expected)
    selected = fixed
    for batch in _batches(paths):
        observed, values = resolve_view(adapter, selected.endpoint, route, selected.owner, selected, batch)
        if not selected.history_indexes:
            selected = replace(selected, history_indexes=tuple(observed["historyIndexes"]))
        for (item, data), path in zip(values, batch):
            count, digest = expected[path]
            if data is None or len(data) != count or sha256(data) != digest: raise SyncSourceError("content-mismatch", "Fixed-view source value changed")
            if require_pinned and item.get("pinned") is not True: raise SyncSourceError("incomplete-retention", "Source path is not pinned at the fixed view")
    return selected


def _verify_resources_once(
    adapter: Adapter, endpoint: str | None, route: list[str], owner: str,
    expected: dict[tuple[str, ...], tuple[int, str] | None],
) -> dict[str, Any]:
    """Verify one complete expected set using one current selection and fixed continuation batches."""
    paths = list(expected)
    if not paths:
        raise SyncSourceError("invalid-release", "Resource verification requires at least one path")
    selected_view: dict[str, Any] | None = None
    for ordinal, batch in enumerate(_batches(paths)):
        if ordinal == 0:
            observed, values = resolve_view(adapter, endpoint, route, owner, "current", batch)
            selected_view = observed
        else:
            assert selected_view is not None
            fixed = FixedReference(
                selected_view["terminalEndpoint"], owner, selected_view["selectedIndex"],
                ("placeholder",), 1, "0" * 64, tuple(selected_view["historyIndexes"]),
            )
            observed, values = resolve_view(adapter, fixed.endpoint, route, owner, fixed, batch)
            if (observed["terminalEndpoint"], observed["route"], observed["historyIndexes"], observed["selectedIndex"]) != (
                    selected_view["terminalEndpoint"], selected_view["route"], selected_view["historyIndexes"], selected_view["selectedIndex"]):
                raise SyncSourceError("fixed-view-mismatch", "Resource verification changed fixed view")
        for (_, data), path in zip(values, batch):
            evidence = expected[path]
            if evidence is None:
                if data is not None:
                    raise SyncSourceError("conflict", "Ready marker was already present")
                continue
            count, digest = evidence
            if data is None or len(data) != count or sha256(data) != digest:
                raise SyncSourceError("content-mismatch", f"Resource committed verification failed at {'/'.join(path)}")
    assert selected_view is not None
    return selected_view


def _fixed_json(fixed: FixedReference) -> dict[str, Any]:
    return {
        "endpoint": fixed.endpoint, "owner": fixed.owner, "index": fixed.index,
        "descriptorPath": list(fixed.descriptor_path), "descriptorBytes": fixed.descriptor_bytes,
        "descriptorSha256": fixed.descriptor_sha256,
    }


def inspect_reference(adapter: Adapter, reference_bytes: bytes, route: list[str] | None = None) -> tuple[FixedReference, Release]:
    reference = parse_reference(reference_bytes)
    selected_route = validate_route(route or [])
    if isinstance(reference, CurrentReference):
        _, values = resolve_view(adapter, reference.endpoint, selected_route, reference.owner, "current", [reference.head_path])
        if values[0][1] is None:
            raise SyncSourceError("reference-mismatch", "Current reference did not resolve to its head")
        fixed = parse_reference(values[0][1])
        if (not isinstance(fixed, FixedReference) or fixed.endpoint != reference.endpoint
                or fixed.owner != reference.owner):
            raise SyncSourceError("reference-mismatch", "head.scm fixed reference changed publisher locator")
    else:
        fixed = reference
    observed, values = resolve_view(adapter, fixed.endpoint, selected_route, fixed.owner, fixed, [fixed.descriptor_path])
    fixed = replace(fixed, history_indexes=tuple(observed["historyIndexes"]))
    descriptor = values[0][1]
    if descriptor is None or len(descriptor) != fixed.descriptor_bytes or sha256(descriptor) != fixed.descriptor_sha256:
        raise SyncSourceError("reference-mismatch", "Descriptor does not match fixed reference")
    release = parse_release(descriptor)
    validate_release_binding(fixed, release)
    return fixed, release


def validate_settle_seconds(value: object) -> float:
    if (not isinstance(value, (int, float)) or isinstance(value, bool)
            or not math.isfinite(value) or not 0 <= value <= 300):
        raise SyncSourceError("invalid-settle-seconds", "settleSeconds must be finite and between 0 and 300")
    return float(value)


def publish(
    adapter: Adapter, source: Path, *, endpoint: str, route: list[str], owner: str, project_id: str,
    project: str, release_label: str, receipt_path: Path, release_id: str | None = None,
    settle_seconds: float = 0.0, sleep_fn: Callable[[float], None] | None = None,
) -> tuple[Release, dict[str, Any]]:
    settle_seconds = validate_settle_seconds(settle_seconds)
    if sleep_fn is None: sleep_fn = time.sleep
    route = validate_route(route)
    endpoint = require_adapter_endpoint(adapter, endpoint)
    validate_journal_component(owner); validate_journal_component(project_id)
    release_id = release_id or str(uuid.uuid4())
    if not LOWER_UUID.fullmatch(release_id): raise SyncSourceError("invalid-release-id", "Release ID must be a lowercase UUID")
    operation_id = uuid.uuid4().hex; receipt = base_receipt("publish", operation_id)
    receipt.update({
        "endpoint": endpoint, "route": route, "publisherOwner": owner, "releaseId": release_id,
        "resourceCreates": {"expected": 0, "acceptedLowerBound": 0},
        "resourcesCommittedExact": False, "readyMarkerWriteAccepted": False,
        "readyMarkerCommittedVisibility": "not-observed",
        "resourceVerificationScheduling": {
            "settleSeconds": settle_seconds, "classification": "scheduling-only",
            "evidence": False, "freshnessGuarantee": False, "waitCount": 0,
            "waitCompleted": False, "interrupted": None,
        },
    })
    write_receipt(receipt_path, receipt)
    with tempfile.TemporaryDirectory(prefix="sync-source-publish-") as temporary:
        spool = Path(temporary) / "snapshot"
        try:
            snapshot = snapshot_source(source.absolute(), spool, project, release_label)
            root = ("source", project_id, "releases", release_id)
            descriptor_path = (*root, "release.scm")
            descriptor = snapshot.release.encode(); descriptor_file = Path(temporary) / "release.scm"; descriptor_file.write_bytes(descriptor); os.chmod(descriptor_file, 0o600)
            expected: dict[tuple[str, ...], tuple[int, str] | None] = {}
            receipt["resourceCreates"]["expected"] = len(snapshot.release.chunks)
            write_receipt(receipt_path, receipt)
            for chunk in snapshot.release.chunks:
                full_path = (*root, *chunk.path)
                create_value(adapter, owner, full_path, snapshot.chunk_files[chunk.path])
                expected[full_path] = (chunk.bytes, chunk.sha256)
                receipt["states"]["writeAccepted"] = True
                receipt["resourceCreates"]["acceptedLowerBound"] += 1
                write_receipt(receipt_path, receipt)
            expected[descriptor_path] = None
            receipt["resourceVerificationScheduling"]["waitCount"] = 1
            write_receipt(receipt_path, receipt)
            try:
                sleep_fn(settle_seconds)
            except InterruptedError as error:
                receipt["resourceVerificationScheduling"]["interrupted"] = True
                receipt.update({"completedAt": now(), "outcome": "cancelled", "error": "Resource verification scheduling wait interrupted"})
                write_receipt(receipt_path, receipt)
                raise SyncSourceError("cancelled", "Resource verification scheduling wait interrupted") from error
            except (KeyboardInterrupt, SystemExit):
                receipt["resourceVerificationScheduling"]["interrupted"] = True
                receipt.update({"completedAt": now(), "outcome": "cancelled", "error": "Resource verification scheduling wait cancelled"})
                write_receipt(receipt_path, receipt)
                raise
            receipt["resourceVerificationScheduling"].update({"waitCompleted": True, "interrupted": False})
            write_receipt(receipt_path, receipt)
            verified_view = _verify_resources_once(adapter, endpoint, route, owner, expected)
            receipt.update({
                "resourcesCommittedExact": True,
                "resourceVerificationView": verified_view,
            })
            receipt["states"]["retrieved"] = True
            write_receipt(receipt_path, receipt)
            create_value(adapter, owner, descriptor_path, descriptor_file)
            receipt["states"]["writeAccepted"] = True
            receipt.update({
                "readyMarkerWriteAccepted": True, "readyMarkerCommittedVisibility": "not-observed",
                "operationLocalReadyInputs": {
                    "classification": "nonnormative-operation-local", "portable": False,
                    "endpoint": endpoint, "route": route, "owner": owner,
                    "readyMarkerPath": list(descriptor_path), "expectedDescriptorBytes": len(descriptor),
                    "expectedDescriptorSha256": sha256(descriptor),
                },
                "readyMarkerPath": list(descriptor_path), "readyMarkerBytes": len(descriptor),
                "readyMarkerSha256": sha256(descriptor), "entries": len(snapshot.release.entries),
                "chunks": len(snapshot.release.chunks), "aggregateBytes": snapshot.release.aggregate_bytes,
                "treeSha256": tree_digest(snapshot.release), "completedAt": now(),
                "outcome": "ready-marker-write-accepted",
            })
            write_receipt(receipt_path, receipt)
            return snapshot.release, receipt
        except Exception as error:
            receipt.update({"completedAt": now(), "outcome": error.code if isinstance(error, SyncSourceError) else "failed-or-ambiguous", "error": str(error)})
            write_receipt(receipt_path, receipt); raise


def observe_ready(
    adapter: Adapter, *, endpoint: str, route: list[str], owner: str,
    descriptor_path: tuple[str, ...], expected_descriptor_bytes: int,
    expected_descriptor_sha256: str, receipt_path: Path,
) -> tuple[FixedReference | None, Release | None, dict[str, Any]]:
    route = validate_route(route)
    endpoint = require_adapter_endpoint(adapter, endpoint)
    validate_journal_component(owner)
    if (len(descriptor_path) != 5 or descriptor_path[0] != "source" or descriptor_path[2] != "releases"
            or not LOWER_UUID.fullmatch(descriptor_path[3]) or descriptor_path[4] != "release.scm"):
        raise SyncSourceError("reference-layout", "Ready input does not name a v1 ready marker")
    for component in descriptor_path:
        validate_journal_component(component)
    if (not isinstance(expected_descriptor_bytes, int) or isinstance(expected_descriptor_bytes, bool)
            or not 1 <= expected_descriptor_bytes <= MAX_DESCRIPTOR_BYTES):
        raise SyncSourceError("invalid-ready-input", "Expected descriptor byte count is invalid")
    if not isinstance(expected_descriptor_sha256, str) or not LOWER_SHA256.fullmatch(expected_descriptor_sha256):
        raise SyncSourceError("invalid-ready-input", "Expected descriptor SHA-256 is invalid")
    release_id = descriptor_path[3]
    operation_inputs = {
        "classification": "nonnormative-operation-local", "portable": False,
        "endpoint": endpoint, "route": route, "owner": owner,
        "readyMarkerPath": list(descriptor_path), "expectedDescriptorBytes": expected_descriptor_bytes,
        "expectedDescriptorSha256": expected_descriptor_sha256,
    }
    operation_id = uuid.uuid4().hex; receipt = base_receipt("ready", operation_id)
    receipt.update({
        "endpoint": endpoint, "route": route, "publisherOwner": owner,
        "releaseId": release_id, "operationLocalReadyInputs": operation_inputs,
        "readyMarkerPath": list(descriptor_path), "observationCount": 1,
    })
    write_receipt(receipt_path, receipt)
    try:
        observed, values = resolve_view(adapter, endpoint, route, owner, "current", [descriptor_path])
        descriptor = values[0][1]
        receipt["observedView"] = observed
        receipt["states"]["retrieved"] = True
        if descriptor is None:
            receipt.update({"completedAt": now(), "outcome": "not-ready", "readyMarkerCommittedVisibility": "absent"})
            write_receipt(receipt_path, receipt)
            return None, None, receipt
        if len(descriptor) != expected_descriptor_bytes or sha256(descriptor) != expected_descriptor_sha256:
            raise SyncSourceError("content-mismatch", "Ready marker differs from operation-local binding")
        release = parse_release(descriptor)
        fixed = FixedReference(
            endpoint, owner, observed["selectedIndex"], descriptor_path, len(descriptor),
            sha256(descriptor), tuple(observed["historyIndexes"]),
        )
        validate_release_binding(fixed, release)
        expected = {descriptor_path: (len(descriptor), sha256(descriptor))}
        expected.update({(*descriptor_path[:-1], *chunk.path): (chunk.bytes, chunk.sha256) for chunk in release.chunks})
        fixed = _verify_remote_values(adapter, route, fixed, expected)
        receipt.update({
            "fixedReference": _fixed_json(fixed), "readyMarkerCommittedVisibility": "exact",
            "descriptorBytes": len(descriptor), "descriptorSha256": sha256(descriptor),
            "entries": len(release.entries), "chunks": len(release.chunks),
            "aggregateBytes": release.aggregate_bytes, "treeSha256": tree_digest(release),
            "completedAt": now(), "outcome": "verified",
        })
        receipt["states"]["verified"] = True
        write_receipt(receipt_path, receipt)
        return fixed, release, receipt
    except Exception as error:
        receipt.update({"completedAt": now(), "outcome": error.code if isinstance(error, SyncSourceError) else "failed-or-ambiguous", "error": str(error)})
        write_receipt(receipt_path, receipt); raise


def retain_paths(
    adapter: Adapter, route: list[str], fixed: FixedReference, release: Release,
    *, on_handle=lambda _handle: None,
) -> tuple[list[dict[str, Any]], bool, str]:
    route = validate_route(route)
    paths = [fixed.descriptor_path] + [(*fixed.descriptor_path[:-1], *chunk.path) for chunk in release.chunks]
    handles: list[dict[str, Any]] = []
    for batch in _batches(paths, MAX_PIN_BATCH_PATHS):
        proof_bound = 4_190_208
        result = adapter.invoke("pin-view", {
            "version": 2, "endpoint": fixed.endpoint, "route": route,
            "historyIndexes": list(fixed.history_indexes), "owner": fixed.owner, "index": fixed.index,
            "paths": [list(path) for path in batch], "proofEncodedBytesUpperBound": proof_bound,
        })
        batch_handles = result.get("handles") if isinstance(result.get("handles"), list) else []
        for handle in batch_handles:
            if isinstance(handle, dict): handles.append(handle); on_handle(handle)
        if result.get("outcome") != "accepted" or len(batch_handles) != len(batch): return handles, False, str(result.get("outcome"))
    expected = {fixed.descriptor_path: (fixed.descriptor_bytes, fixed.descriptor_sha256)}
    expected.update({(*fixed.descriptor_path[:-1], *chunk.path): (chunk.bytes, chunk.sha256) for chunk in release.chunks})
    _verify_remote_values(adapter, route, fixed, expected, require_pinned=True)
    return handles, True, "accepted"


def pull(
    adapter: Adapter, reference_bytes: bytes, *, route: list[str], destination: Path, receipt_path: Path, pin_policy: str = "none",
    cut=None,
) -> dict[str, Any]:
    route = validate_route(route)
    operation_id = uuid.uuid4().hex; receipt = base_receipt("pull", operation_id)
    receipt.update({"route": route, "destination": str(destination.absolute()), "pin": {"requested": pin_policy != "none", "policy": pin_policy, "outcome": "not-attempted", "handles": [], "complete": False}})
    write_receipt(receipt_path, receipt)
    try:
        fixed, release = inspect_reference(adapter, reference_bytes, route)
        receipt.update({
            "publisherEndpoint": fixed.endpoint, "publisherOwner": fixed.owner,
            "observerRoute": route, "observerHistoryIndexes": list(fixed.history_indexes),
            "fixedReference": _fixed_json(fixed),
            "descriptorBytes": fixed.descriptor_bytes, "descriptorSha256": fixed.descriptor_sha256,
            "entries": len(release.entries), "chunks": len(release.chunks), "aggregateBytes": release.aggregate_bytes,
        })
        receipt["states"]["retrieved"] = True
        if pin_policy not in {"none", "paths"}: raise SyncSourceError("unsupported-pin-policy", "Candidate supports explicit none or universal paths retention")
        if pin_policy == "paths":
            receipt["pin"]["outcome"] = "attempted"; write_receipt(receipt_path, receipt)
            def record_handle(handle: dict[str, Any]) -> None:
                receipt["pin"]["handles"].append(handle); write_receipt(receipt_path, receipt)
            handles, complete, pin_outcome = retain_paths(adapter, route, fixed, release, on_handle=record_handle)
            receipt["pin"].update({"handles": handles, "complete": complete, "outcome": pin_outcome})
            write_receipt(receipt_path, receipt)
            if not complete: raise SyncSourceError("incomplete-retention", "Retention did not complete")
        root = fixed.descriptor_path[:-1]

        def read_chunk(chunk: Chunk) -> bytes:
            full_path = (*root, *chunk.path)
            _, values = resolve_view(adapter, fixed.endpoint, route, fixed.owner, fixed, [full_path])
            item, data = values[0]
            if data is None or len(data) != chunk.bytes or sha256(data) != chunk.sha256:
                raise SyncSourceError("content-mismatch", "Chunk differs at fixed view")
            if pin_policy == "paths" and item.get("pinned") is not True: raise SyncSourceError("incomplete-retention", "Pinned state changed")
            return data

        result = materialize(destination, release, read_chunk, receipt_path, operation_id=operation_id, cut=cut)
        receipt.update({"treeSha256": result.tree_sha256, "stagingCleanup": "complete", "completedAt": now(), "outcome": "materialized"})
        receipt["states"].update({"verified": True, "materialized": True})
        write_receipt(receipt_path, receipt); return receipt
    except Exception as error:
        stage = destination.absolute().parent / f".sync-source-stage-{operation_id}"
        marker = stage.with_name(stage.name + ".json")
        receipt["stagingCleanup"] = "residue" if os.path.lexists(stage) or os.path.lexists(marker) else "complete-or-not-started"
        receipt.update({"completedAt": now(), "outcome": error.code if isinstance(error, SyncSourceError) else "failed-or-ambiguous", "error": str(error)})
        write_receipt(receipt_path, receipt); raise


def verify(descriptor_bytes: bytes, destination: Path) -> dict[str, Any]:
    release = parse_release(descriptor_bytes)
    return {"version": 2, "outcome": "verified", "treeSha256": verify_tree(destination.absolute(), release), "entries": len(release.entries), "chunks": len(release.chunks), "installed": False, "executed": False}


def validate_route(value: object) -> list[str]:
    if not isinstance(value, list) or len(value) > 16:
        raise SyncSourceError("invalid-route", "Route must be an array of at most 16 components")
    route: list[str] = []
    for item in value:
        if not isinstance(item, str) or not ROUTE_COMPONENT.fullmatch(item) or item in {".", ".."} or "/" in item:
            raise SyncSourceError("invalid-route", "Invalid route component")
        route.append(item)
    return route


def parse_route(value: str) -> list[str]:
    try: route = json.loads(value)
    except json.JSONDecodeError as exc: raise SyncSourceError("invalid-route", "Route must be a JSON array") from exc
    return validate_route(route)


def parse_ready_marker_path(value: str) -> tuple[str, ...]:
    try: path = json.loads(value)
    except json.JSONDecodeError as exc: raise SyncSourceError("invalid-ready-input", "Ready marker path must be a JSON array") from exc
    if not isinstance(path, list) or not all(isinstance(item, str) for item in path):
        raise SyncSourceError("invalid-ready-input", "Ready marker path must contain strings")
    return tuple(path)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", default=os.environ.get("PI_AGENT_CONFIG", "/etc/pi-agent/agent.json"))
    parser.add_argument("--state-dir", default="/var/lib/pi-agent/sync")
    sub = parser.add_subparsers(dest="command", required=True)
    inspect_p = sub.add_parser("inspect"); inspect_p.add_argument("--reference", type=Path, required=True); inspect_p.add_argument("--route", required=True)
    publish_p = sub.add_parser("publish")
    for name in ("source", "receipt"): publish_p.add_argument(f"--{name}", type=Path, required=True)
    for name in ("owner", "project-id", "project", "release"): publish_p.add_argument(f"--{name}", required=True)
    publish_p.add_argument("--route", required=True); publish_p.add_argument("--release-id")
    publish_p.add_argument("--settle-seconds", type=float, default=0.0)
    ready_p = sub.add_parser("ready")
    ready_p.add_argument("--receipt", type=Path, required=True); ready_p.add_argument("--route", required=True)
    ready_p.add_argument("--owner", required=True)
    ready_p.add_argument("--ready-marker-path", required=True)
    ready_p.add_argument("--expected-descriptor-bytes", type=int, required=True)
    ready_p.add_argument("--expected-descriptor-sha256", required=True)
    pull_p = sub.add_parser("pull"); pull_p.add_argument("--reference", type=Path, required=True); pull_p.add_argument("--route", required=True); pull_p.add_argument("--destination", type=Path, required=True); pull_p.add_argument("--receipt", type=Path, required=True); pull_p.add_argument("--pin", choices=["none", "paths"], default="none")
    publish_current_p = sub.add_parser("publish-current")
    for name in ("source", "receipt"): publish_current_p.add_argument(f"--{name}", type=Path, required=True)
    for name in ("endpoint", "owner", "project-id", "project", "release", "route", "head-path"): publish_current_p.add_argument(f"--{name}", required=True)
    publish_current_p.add_argument("--release-id"); publish_current_p.add_argument("--expected-old-head", type=Path)
    publish_current_p.add_argument("--head-helper", type=Path); publish_current_p.add_argument("--head-helper-sha256")
    pull_current_p = sub.add_parser("pull-current")
    for name in ("destination", "receipt"): pull_current_p.add_argument(f"--{name}", type=Path, required=True)
    for name in ("endpoint", "route", "owner", "head-path"): pull_current_p.add_argument(f"--{name}", required=True)
    audit_current_p = sub.add_parser("audit-current"); audit_current_p.add_argument("--current-reference", type=Path, required=True)
    audit_current_p.add_argument("--route", required=True); audit_current_p.add_argument("--receipt", type=Path, required=True); audit_current_p.add_argument("--destination", type=Path)
    audit_current_p.add_argument("--prior-current-receipt", type=Path); audit_current_p.add_argument("--prior-current-receipt-sha256")
    verify_p = sub.add_parser("verify"); verify_p.add_argument("--descriptor", type=Path, required=True); verify_p.add_argument("--destination", type=Path, required=True)
    recover_p = sub.add_parser("recover"); recover_p.add_argument("--parent", type=Path, required=True); recover_p.add_argument("--marker", required=True); recover_p.add_argument("--receipt", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "verify": result = verify(args.descriptor.read_bytes(), args.destination)
        elif args.command == "recover": result = {"version": 2, "outcome": "recovered" if recover(args.parent, args.marker, args.receipt) else "not-attempted"}
        else:
            adapter = ModuleAdapter(args.config, args.state_dir)
            if args.command == "inspect":
                fixed, release = inspect_reference(adapter, args.reference.read_bytes(), parse_route(args.route)); result = {"version": 2, "outcome": "verified", "fixedReference": _fixed_json(fixed), "treeSha256": tree_digest(release)}
            elif args.command == "publish":
                release, receipt = publish(adapter, args.source, endpoint=adapter.endpoint, route=parse_route(args.route), owner=args.owner, project_id=args.project_id, project=args.project, release_label=args.release, receipt_path=args.receipt, release_id=args.release_id, settle_seconds=args.settle_seconds)
                result = {"version": 2, "outcome": "ready-marker-write-accepted", "operationLocalReadyInputs": receipt["operationLocalReadyInputs"], "treeSha256": tree_digest(release), "receipt": receipt}
            elif args.command == "publish-current":
                from .sync_source_current import head_helper_mutator, publish_current, read_held_boundary
                old = read_held_boundary(args.expected_old_head) if args.expected_old_head else None
                if old is not None and (args.head_helper is None or args.head_helper_sha256 is None):
                    raise SyncSourceError("unsupported-capability", "Head advancement requires an exact qualified helper")
                mutator = head_helper_mutator(args.head_helper, args.head_helper_sha256) if old is not None else (lambda *_: {})
                head, receipt = publish_current(adapter, args.source, route=parse_route(args.route), owner=args.owner,
                    endpoint=args.endpoint, project_id=args.project_id, project=args.project, release_label=args.release,
                    head_path=parse_ready_marker_path(args.head_path), receipt_path=args.receipt,
                    expected_old_head=old, advance_head=mutator, release_id=args.release_id)
                result = {"version": 2, "outcome": receipt["outcome"], "currentReference": base64.b64encode(head.encode()).decode(), "receipt": receipt}
            elif args.command == "pull-current":
                from .sync_source_current import pull_current
                result = pull_current(adapter, endpoint=args.endpoint, route=parse_route(args.route), owner=args.owner,
                    head_path=parse_ready_marker_path(args.head_path), destination=args.destination, receipt_path=args.receipt)
            elif args.command == "audit-current":
                from .sync_source_current import audit_current
                prior_bytes = args.prior_current_receipt.read_bytes() if args.prior_current_receipt else None
                fixed, receipt = audit_current(adapter, args.current_reference.read_bytes(), route=parse_route(args.route),
                    receipt_path=args.receipt, destination=args.destination, prior_receipt_bytes=prior_bytes,
                    prior_receipt_sha256=args.prior_current_receipt_sha256)
                result = {"version": 2, "outcome": receipt["outcome"], "reference": base64.b64encode(fixed.encode()).decode(), "receipt": receipt}
            elif args.command == "ready":
                fixed, release, receipt = observe_ready(
                    adapter, route=parse_route(args.route), owner=args.owner,
                    endpoint=adapter.endpoint,
                    descriptor_path=parse_ready_marker_path(args.ready_marker_path),
                    expected_descriptor_bytes=args.expected_descriptor_bytes,
                    expected_descriptor_sha256=args.expected_descriptor_sha256,
                    receipt_path=args.receipt,
                )
                if fixed is None or release is None:
                    result = {"version": 2, "outcome": "not-ready", "receipt": receipt}
                else:
                    result = {"version": 2, "outcome": "verified", "fixedReference": _fixed_json(fixed), "reference": base64.b64encode(fixed.encode()).decode(), "treeSha256": tree_digest(release), "receipt": receipt}
            else: result = pull(adapter, args.reference.read_bytes(), route=parse_route(args.route), destination=args.destination, receipt_path=args.receipt, pin_policy=args.pin)
        sys_stdout(result); return 1 if result.get("outcome") == "not-ready" else 0
    except SyncSourceError as error:
        receipt_path = getattr(args, "receipt", None)
        if args.command in {"publish-current", "pull-current", "audit-current"} and isinstance(receipt_path, Path) and receipt_path.exists():
            try:
                exact = json.loads(receipt_path.read_bytes())
            except Exception:
                exact = None
            if isinstance(exact, dict):
                sys_stdout({"version": 2, "outcome": exact.get("outcome"), "receipt": exact}); return 1
        sys_stdout({"version": 2, "outcome": "not-attempted", "error": error.code, "message": str(error), "dispatchCount": 0}); return 1


def sys_stdout(value: dict[str, Any]) -> None:
    sys.stdout.buffer.write(canonical_json(value))


if __name__ == "__main__":
    raise SystemExit(main())
