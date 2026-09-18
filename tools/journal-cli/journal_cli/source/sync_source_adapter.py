#!/usr/bin/python3
"""Bounded no-shell adapter for the four frozen low-level command names."""

from __future__ import annotations

import json
import os
from pathlib import Path
import tempfile
from typing import Any, Protocol

from .sync_source_model import SyncSourceError, canonical_endpoint

COMMANDS = {"get-current", "resolve-view", "create-value", "pin-view", "unpin-retention"}
OUTCOMES = {"not-attempted", "retrieved", "accepted", "rejected", "failed-or-ambiguous"}
MAX_JSON_RESULT = 4_190_208
MAX_JSON_REQUEST = 1_048_576


class Adapter(Protocol):
    endpoint: str

    def invoke(self, command: str, request: dict[str, Any]) -> dict[str, Any]: ...


def canonical_request_body(request: dict[str, Any]) -> bytes:
    """Return the exact bounded JSON bytes passed to the low-level subprocess."""
    try:
        return json.dumps(request, ensure_ascii=False, allow_nan=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    except (TypeError, ValueError, UnicodeError) as exc:
        raise SyncSourceError("invalid-request", "Low-level request is not canonical JSON") from exc


class ModuleAdapter:
    """Invoke the bundled low-level adapter without a second executable."""

    def __init__(self, config: str, state_dir: str):
        from . import ops
        self.config = config
        self.state_dir = state_dir
        self.endpoint = canonical_endpoint(ops.endpoint(ops.load_config(config)))

    def invoke(self, command: str, request: dict[str, Any]) -> dict[str, Any]:
        from . import ops
        body = canonical_request_body(request)
        fd, name = tempfile.mkstemp(prefix="journal-cli-source-request-", suffix=".json")
        try:
            os.fchmod(fd, 0o600)
            with os.fdopen(fd, "wb") as handle:
                handle.write(body)
            result = ops.run_command(command, name, self.config, self.state_dir)
        finally:
            try:
                os.unlink(name)
            except FileNotFoundError:
                pass
        if not isinstance(result, dict):
            raise SyncSourceError("adapter-framing", "Bundled low-level adapter returned a non-object")
        return result


class PreDispatchAdapterError(SyncSourceError):
    """Signal a subprocess creation failure proven to precede any process."""


def require_outcome(result: dict[str, Any], allowed: set[str]) -> dict[str, Any]:
    outcome = result.get("outcome")
    if outcome not in allowed:
        raise SyncSourceError(str(outcome or "adapter-failure"), f"Low-level operation did not complete: {outcome}")
    return result
