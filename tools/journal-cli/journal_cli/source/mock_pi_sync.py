#!/usr/bin/python3
"""Inert local mock for resolve-view/create-value/pin-view/unpin-retention."""

from __future__ import annotations

import sys
sys.dont_write_bytecode = True

import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile
import uuid
from typing import Any

from .sync_source_model import SyncSourceError, validate_journal_component, validate_journal_path

DEFAULT_ENDPOINT = "http://127.0.0.1:8192/interface"
COMMANDS = {"resolve-view", "create-value", "pin-view", "unpin-retention"}
CONTRACT = "journal-cli-source-ops-v2"


def mock_source_sha256() -> str:
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


class MockPiSync:
    def __init__(self, root: Path, endpoint: str = DEFAULT_ENDPOINT):
        self.endpoint = endpoint
        self.root = root.absolute()
        self.root.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(self.root, 0o700)
        self.state_path = self.root / "state.json"
        self.lock_path = self.root / "lock"
        self.blobs = self.root / "blobs"
        self.blobs.mkdir(mode=0o700, exist_ok=True)
        os.chmod(self.blobs, 0o700)
        if not self.state_path.exists():
            self._write_state({"version": 2, "endpoint": endpoint, "index": 0, "values": {}, "retentions": {}, "faults": {}})

    def _write_state(self, state: dict[str, Any]) -> None:
        fd, name = tempfile.mkstemp(prefix=".state-", dir=self.root)
        try:
            os.fchmod(fd, 0o600)
            with os.fdopen(fd, "wb", closefd=False) as handle:
                handle.write(canonical_json(state)); handle.flush(); os.fsync(handle.fileno())
            os.close(fd); fd = -1
            os.replace(name, self.state_path)
            parent = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            try: os.fsync(parent)
            finally: os.close(parent)
        finally:
            if fd >= 0: os.close(fd)
            try: os.unlink(name)
            except FileNotFoundError: pass

    def _locked(self):
        fd = os.open(self.lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        os.fchmod(fd, 0o600); fcntl.flock(fd, fcntl.LOCK_EX)
        return fd

    def _read_state(self) -> dict[str, Any]:
        return json.loads(self.state_path.read_text("utf-8"))

    @staticmethod
    def _key(owner: str, path: list[str] | tuple[str, ...]) -> str:
        validate_journal_component(owner); values = validate_journal_path(path)
        return owner + "/" + "/".join(values)

    @staticmethod
    def _value_at(history: list[dict[str, Any]] | None, index: int) -> dict[str, Any] | None:
        if not history: return None
        selected = None
        for item in history:
            if item["index"] <= index: selected = item
            else: break
        return selected

    def set_fault(self, command: str, outcome: str) -> None:
        fd = self._locked()
        try:
            state = self._read_state(); state["faults"][command] = outcome; self._write_state(state)
        finally: os.close(fd)

    def _result(self, command: str, value: dict[str, Any]) -> dict[str, Any]:
        return {"contract": CONTRACT, "version": 2, "operation": command, "tool": {"name": "pi-sync-source-ops", "version": "2.0.0", "sha256": mock_source_sha256()}, "requestSha256": None, **value}

    def invoke(self, command: str, request: dict[str, Any]) -> dict[str, Any]:
        if command not in COMMANDS:
            return self._result(command, {"outcome": "not-attempted", "error": {"code": "unsupported-capability", "message": "unsupported"}})
        fd = self._locked()
        try:
            state = self._read_state()
            fault = state.get("faults", {}).pop(command, None)
            if fault:
                self._write_state(state)
                return self._result(command, {"outcome": fault, "error": {"code": "injected-fault", "message": "injected fault"}})
            try:
                result = getattr(self, command.replace("-", "_"))(state, request)
            except SyncSourceError as exc:
                return self._result(command, {"outcome": "not-attempted", "error": {"code": exc.code, "message": str(exc)}})
            self._write_state(state)
            return self._result(command, result)
        finally:
            os.close(fd)

    def resolve_view(self, state: dict[str, Any], request: dict[str, Any]) -> dict[str, Any]:
        owner = validate_journal_component(request.get("owner"))
        paths = request.get("paths")
        if not isinstance(paths, list) or not 1 <= len(paths) <= 1024:
            raise SyncSourceError("invalid-request", "resolve-view paths are invalid")
        view = request.get("view")
        route = request.get("route", [])
        if request.get("endpoint") not in {None, self.endpoint}:
            raise SyncSourceError("endpoint-mismatch", "Requested endpoint differs")
        if view == {"kind": "current"}:
            selected = state["index"]; indexes = [selected] * (len(route) + 1)
        elif (isinstance(view, dict) and set(view).issubset({"kind", "index", "historyIndexes"})
              and set(view) >= {"kind", "index"} and view.get("kind") == "fixed"
              and isinstance(view.get("index"), int) and not isinstance(view.get("index"), bool)
              and 0 <= view["index"] <= state["index"]):
            selected = view["index"]
            indexes = view.get("historyIndexes", [selected] * (len(route) + 1))
            if len(indexes) != len(route) + 1 or indexes[-1] != selected:
                raise SyncSourceError("fixed-view-mismatch", "Requested fixed history is unavailable")
        else: raise SyncSourceError("fixed-view-mismatch", "Requested fixed view is unavailable")
        include_pinned = request.get("includePinned", False)
        items = []
        for raw_path in paths:
            path = validate_journal_path(raw_path); key = self._key(owner, path)
            value = self._value_at(state["values"].get(key), selected)
            pinned = any(
                retention["index"] == selected and retention["owner"] == owner
                and (tuple(path) == tuple(retention["path"]) or tuple(path[:len(retention["path"])]) == tuple(retention["path"]))
                for retention in state["retentions"].values()
            )
            if value is None:
                items.append({"path": list(path), "shape": "nothing", "canonicalCommittedPath": [selected, "*state*", owner, *path], **({"pinned": pinned} if include_pinned else {})})
            else:
                data = (self.blobs / value["sha256"]).read_bytes()
                items.append({
                    "path": list(path), "shape": "value", "contentBase64": base64.b64encode(data).decode("ascii"),
                    "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                    "canonicalCommittedPath": [selected, "*state*", owner, *path], **({"pinned": pinned} if include_pinned else {}),
                })
        return {"outcome": "retrieved", "view": {"entryEndpoint": self.endpoint, "terminalEndpoint": self.endpoint, "route": route, "historyIndexes": indexes, "owner": owner, "selectedIndex": selected, "originIndex": indexes[0]}, "results": items}

    def create_value(self, state: dict[str, Any], request: dict[str, Any]) -> dict[str, Any]:
        owner = validate_journal_component(request.get("owner")); path = validate_journal_path(request.get("path"))
        if request.get("expected") != "absent":
            raise SyncSourceError("invalid-request", "Only expected absent is supported")
        payload_path = Path(request.get("input", ""))
        try:
            info = payload_path.stat()
            if not payload_path.is_file() or info.st_size > 524_288: raise OSError
            data = payload_path.read_bytes()
        except OSError as exc:
            raise SyncSourceError("invalid-payload", "Payload file is invalid") from exc
        key = self._key(owner, path); current = self._value_at(state["values"].get(key), state["index"])
        if current is not None:
            return {"outcome": "rejected", "error": {"code": "conflict", "message": "present"}}
        digest = hashlib.sha256(data).hexdigest(); blob = self.blobs / digest
        if not blob.exists():
            blob.write_bytes(data); os.chmod(blob, 0o600)
        state["index"] += 1
        state["values"].setdefault(key, []).append({"index": state["index"], "sha256": digest})
        return {"outcome": "accepted", "path": list(path), "bytes": len(data), "sha256": digest, "readback": {"outcome": "not-requested"}}

    def pin_view(self, state: dict[str, Any], request: dict[str, Any]) -> dict[str, Any]:
        owner = validate_journal_component(request.get("owner")); index = request.get("index"); paths = request.get("paths"); bound = request.get("proofEncodedBytesUpperBound")
        if not isinstance(bound, int) or isinstance(bound, bool) or not 1 <= bound <= 4_190_208: raise SyncSourceError("unsupported-capability", "Proof response bound is invalid")
        if not isinstance(index, int) or isinstance(index, bool) or not 0 <= index <= state["index"]:
            raise SyncSourceError("fixed-view-mismatch", "Pin index is unavailable")
        if not isinstance(paths, list) or not 1 <= len(paths) <= 1024:
            raise SyncSourceError("invalid-request", "pin-view paths are invalid")
        items = []
        for raw_path in paths:
            path = validate_journal_path(raw_path); key = self._key(owner, path)
            if self._value_at(state["values"].get(key), index) is None and not any(value_key.startswith(key + "/") for value_key in state["values"]):
                items.append({"path": list(path), "outcome": "rejected", "error": "missing"}); continue
            route = request.get("route", []); indexes = request.get("historyIndexes", [index] * (len(route) + 1))
            canonical = [indexes[0]]
            for alias, fixed_index in zip(route, indexes[1:]): canonical.extend([alias, fixed_index])
            canonical.extend(["*state*", owner, *path])
            body = {"handleVersion": 2, "originOwner": owner, "entryEndpoint": self.endpoint,
                    "publisherEndpoint": self.endpoint, "publisherOwner": owner,
                    "routeObservation": route, "historyIndexes": indexes,
                    "publisherIndex": index, "path": list(path), "canonicalCommittedPath": canonical}
            body["handleSha256"] = hashlib.sha256(canonical_json(body)).hexdigest(); handle_key = body["handleSha256"]
            state["retentions"][handle_key] = {"owner": owner, "path": list(path), "index": index, "handle": body}
            items.append(body)
        if len(items) != len(paths): return {"outcome": "rejected", "error": {"code": "pin-rejected", "message": "missing"}}
        return {"outcome": "accepted", "view": {"entryEndpoint": self.endpoint, "terminalEndpoint": self.endpoint, "route": request.get("route", []), "historyIndexes": request.get("historyIndexes"), "owner": owner, "selectedIndex": index, "originIndex": request.get("historyIndexes", [index])[0]}, "requestAtomic": True, "completeness": "unverified", "handles": items}

    def unpin_retention(self, state: dict[str, Any], request: dict[str, Any]) -> dict[str, Any]:
        handles = request.get("handles")
        if not isinstance(handles, list) or not 1 <= len(handles) <= 1024 or not all(isinstance(value, dict) for value in handles):
            raise SyncSourceError("invalid-request", "unpin handles are invalid")
        items = []
        for handle in handles:
            key = handle.get("handleSha256")
            if not isinstance(key, str) or key not in state["retentions"] or state["retentions"][key]["handle"] != handle:
                return {"outcome": "rejected", "error": {"code": "unpin-rejected", "message": "unknown handle"}}
            del state["retentions"][key]; items.append(handle)
        return {"outcome": "accepted", "handles": items}


def main() -> int:
    if len(sys.argv) != 3 or sys.argv[1] not in COMMANDS or sys.argv[2] != "-":
        print(json.dumps({"contract": CONTRACT, "version": 2, "operation": sys.argv[1] if len(sys.argv) > 1 else "unknown", "tool": {"name": "pi-sync-source-ops", "version": "2.0.0", "sha256": mock_source_sha256()}, "requestSha256": None, "outcome": "not-attempted", "error": {"code": "unsupported-capability", "message": "unsupported"}}))
        return 1
    try:
        request = json.loads(sys.stdin.buffer.read(1_048_577).decode("utf-8", errors="strict"))
        if not isinstance(request, dict) or request.get("version") != 2: raise ValueError
    except Exception:
        print(json.dumps({"contract": CONTRACT, "version": 2, "operation": sys.argv[1], "tool": {"name": "pi-sync-source-ops", "version": "2.0.0", "sha256": mock_source_sha256()}, "requestSha256": None, "outcome": "not-attempted", "error": {"code": "invalid-request", "message": "invalid"}}))
        return 1
    root = Path(os.environ.get("SYNC_SOURCE_MOCK_STATE", "/tmp/sync-source-mock"))
    result = MockPiSync(root, os.environ.get("SYNC_SOURCE_MOCK_ENDPOINT", DEFAULT_ENDPOINT)).invoke(sys.argv[1], request)
    sys.stdout.buffer.write(canonical_json(result))
    return 0 if result["outcome"] in {"retrieved", "accepted"} else 1


if __name__ == "__main__":
    raise SystemExit(main())
