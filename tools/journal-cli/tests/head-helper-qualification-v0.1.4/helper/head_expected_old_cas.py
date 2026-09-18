#!/usr/bin/python3
"""Expected-old head CAS over the existing Interface set! primitive."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request

MAX_VALUE_BYTES = 524_288
MAX_RESPONSE_BYTES = 524_288
MAX_ERROR_BYTES = 8_192
MAX_COMPONENT_BYTES = 128
MAX_PATH_BYTES = 4_096
MAX_REQUEST_BYTES = 16_384
COMPONENT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
S7_BARE_SYMBOL = re.compile(r"^[A-Za-z_*!<>=?/][A-Za-z0-9_.*+!<>=?/-]{0,127}$")
UUID_SYMBOL = re.compile(r"^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$")
RESERVED_S7_SYMBOLS = frozenset({
    "*admins-get*", "*admins-set*", "*autoload-hook*", "*bridge*", "*call*", "*crypto*",
    "*error-hook*", "*init*", "*journal*", "*load-hook*", "*missing-close-paren-hook*",
    "*periodic*", "*public*", "*read-error-hook*", "*removed*", "*rootlet-redefinition-hook*",
    "*s7*", "*secret*", "*set-query*", "*set-step*", "*state*", "*step*", "*sync-state*",
    "*time*", "*transition*", "*unbound-variable-hook*", "*window-set*",
})
DEFAULT_CONFIG = "/etc/pi-agent/agent.json"
DEFAULT_STATE = "/var/lib/pi-agent/sync"


class BoundaryError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


class ExplicitReject(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


class CompletionUnestablished(Exception):
    pass


def _exact_fields(value: dict, expected: set[str]) -> None:
    if not isinstance(value, dict) or set(value) != expected:
        raise BoundaryError("invalid-request", "request fields must be exactly owner, path, oldFile, newFile")


def parse_request_bytes(raw: bytes) -> dict:
    if not isinstance(raw, bytes) or not 1 <= len(raw) <= MAX_REQUEST_BYTES:
        raise BoundaryError("invalid-request", "request framing is empty or oversized")

    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise BoundaryError("invalid-request", "duplicate request field")
            result[key] = value
        return result

    try:
        value = json.loads(raw, object_pairs_hook=pairs,
                           parse_constant=lambda value: (_ for _ in ()).throw(BoundaryError("invalid-request", "non-finite request number")))
    except BoundaryError:
        raise
    except (UnicodeError, json.JSONDecodeError, ValueError) as error:
        raise BoundaryError("invalid-request", "request is not strict bounded JSON") from error
    _exact_fields(value, {"owner", "path", "oldFile", "newFile"})
    return value


def _component(value, label: str) -> str:
    if isinstance(value, str) and value in RESERVED_S7_SYMBOLS:
        raise BoundaryError("unsupported-capability", "component is reserved implementation syntax")
    if not isinstance(value, str) or not COMPONENT.fullmatch(value) or value in {".", ".."}:
        raise BoundaryError("invalid-request", f"invalid {label}")
    _symbol(value)
    return value


def _head_path(value) -> tuple[str, str, str]:
    if not isinstance(value, list) or len(value) != 3:
        raise BoundaryError("invalid-request", "path must be a typed source/project head path")
    path = tuple(_component(item, "path component") for item in value)
    if path[0] != "source" or path[2] not in {"head.scm", "current-head.scm"}:
        raise BoundaryError("invalid-request", "terminal path must be head.scm or current-head.scm")
    if len("/".join(path).encode("utf-8")) > MAX_PATH_BYTES:
        raise BoundaryError("invalid-request", "path exceeds the encoded bound")
    return path


def _symbol(value: str) -> str:
    if value in RESERVED_S7_SYMBOLS:
        raise BoundaryError("unsupported-capability", "component is reserved implementation syntax")
    if S7_BARE_SYMBOL.fullmatch(value) or UUID_SYMBOL.fullmatch(value):
        return value
    raise BoundaryError("unsupported-capability", "component has no supported inert s7 symbol literal")


def _bytes(value: bytes) -> str:
    return "#u(" + " ".join(str(item) for item in value) + ")"


def _string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def _build_interface_expression(owner: str, path: tuple[str, str, str], old: bytes, new: bytes, secret: str) -> str:
    staged = "(" + " ".join(["*state*", _symbol(owner), *(_symbol(item) for item in path)]) + ")"
    arguments = " ".join([
        f"(path {staged})",
        f"(value {_bytes(new)})",
        f"(expected {_bytes(old)})",
        "(expression? #f)",
    ])
    authentication = f"(authentication ((identity (*state* {_symbol(owner)})) (credentials {_string(secret)})))"
    return f"((function set!) (arguments ({arguments})) {authentication})"


def _read_boundary(path_value, label: str) -> tuple[bytes, dict]:
    if not isinstance(path_value, str) or not path_value:
        raise BoundaryError("invalid-request", f"{label} must name one file boundary")
    if not hasattr(os, "O_NOFOLLOW"):
        raise BoundaryError("unsupported-capability", "O_NOFOLLOW is unavailable")
    flags = os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NONBLOCK", 0)
    try:
        fd = os.open(path_value, flags)
    except OSError as error:
        raise BoundaryError("unsupported-input", f"cannot open {label}") from error
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_uid != os.geteuid():
            raise BoundaryError("unsupported-input", f"{label} must be an owner-controlled single-link regular file")
        if before.st_size > MAX_VALUE_BYTES:
            raise BoundaryError("resource-limit", f"{label} exceeds 524288 bytes")
        pieces = []
        total = 0
        while total <= MAX_VALUE_BYTES:
            part = os.read(fd, min(65_536, MAX_VALUE_BYTES + 1 - total))
            if not part:
                break
            pieces.append(part)
            total += len(part)
        after = os.fstat(fd)
        fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns")
        if total > MAX_VALUE_BYTES:
            raise BoundaryError("resource-limit", f"{label} exceeds 524288 bytes")
        if any(getattr(before, field) != getattr(after, field) for field in fields) or total != after.st_size:
            raise BoundaryError("source-changed", f"{label} changed while held")
        data = b"".join(pieces)
        return data, {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    finally:
        os.close(fd)


def _load_json(path: Path) -> dict:
    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise BoundaryError("invalid-config", "duplicate config field")
            result[key] = value
        return result
    try:
        value = json.loads(path.read_bytes(), object_pairs_hook=pairs,
                           parse_constant=lambda value: (_ for _ in ()).throw(BoundaryError("invalid-config", "non-finite config number")))
    except BoundaryError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        raise BoundaryError("invalid-config", "cannot load owner-local config") from error
    if not isinstance(value, dict) or value.get("version") != 1:
        raise BoundaryError("invalid-config", "owner-local config has unsupported shape")
    return value


class LocalInterfaceTransport:
    """Credential loading and Scheme construction remain inside this transport boundary."""

    def __init__(self, config_path: Path, state_dir: Path, timeout: float = 30.0):
        config = _load_json(config_path)
        self.local_owner = _component(config.get("id"), "configured owner")
        sync = config.get("sync") or {}
        if not isinstance(sync, dict):
            raise BoundaryError("invalid-config", "sync config must be an object")
        try:
            self.endpoint = sync.get("localInterface") or f"http://127.0.0.1:{int(sync.get('port', 8192))}/interface"
        except (TypeError, ValueError) as error:
            raise BoundaryError("invalid-config", "invalid local Interface endpoint") from error
        if not isinstance(self.endpoint, str):
            raise BoundaryError("unsupported-capability", "helper requires a loopback local Interface endpoint")
        parsed = urllib.parse.urlsplit(self.endpoint)
        if (parsed.scheme != "http" or parsed.hostname != "127.0.0.1" or parsed.username is not None
                or parsed.password is not None or parsed.query or parsed.fragment or parsed.path != "/interface"):
            raise BoundaryError("unsupported-capability", "helper requires a loopback local Interface endpoint")
        try:
            if parsed.port is None or not 1 <= parsed.port <= 65535:
                raise ValueError
        except ValueError as error:
            raise BoundaryError("unsupported-capability", "helper requires a bounded loopback Interface port") from error
        try:
            self._secret = (state_dir / "ledger.interface-secret").read_text().strip()
        except OSError as error:
            raise BoundaryError("unsupported-capability", "cannot load owner-local Interface credential") from error
        if not self._secret:
            raise BoundaryError("unsupported-capability", "owner-local Interface credential is empty")
        self.timeout = timeout

    def dispatch(self, owner: str, path: tuple[str, str, str], old: bytes, new: bytes):
        expression = _build_interface_expression(owner, path, old, new, self._secret)
        request = urllib.request.Request(self.endpoint, data=expression.encode("utf-8"),
                                         headers={"Content-Type": "application/scheme"}, method="POST")
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                raw = response.read(MAX_RESPONSE_BYTES + 1)
                if len(raw) > MAX_RESPONSE_BYTES:
                    raise CompletionUnestablished("Interface response exceeded 524288 bytes")
        except urllib.error.HTTPError as error:
            body = error.read(MAX_ERROR_BYTES + 1)
            if error.code == 400 and len(body) <= MAX_ERROR_BYTES:
                raise ExplicitReject("interface-rejected", "Interface explicitly rejected the conditional mutation") from error
            raise CompletionUnestablished("Interface HTTP completion was not established") from error
        except (OSError, urllib.error.URLError) as error:
            raise CompletionUnestablished("Interface transport completion was not established") from error
        try:
            framed = raw.decode("utf-8", errors="strict").strip()
        except UnicodeDecodeError as error:
            raise CompletionUnestablished("Interface response was not UTF-8") from error
        if framed == "#t":
            return True
        if framed == "#f":
            return False
        if framed.startswith("(error"):
            raise ExplicitReject("interface-rejected", "Interface explicitly rejected the conditional mutation")
        raise CompletionUnestablished("Interface response framing was malformed")


def _base_result(old_evidence: dict | None = None, new_evidence: dict | None = None) -> dict:
    result = {"helper": "head-expected-old-cas", "version": "0.1.4"}
    if old_evidence is not None:
        result["old"] = old_evidence
    if new_evidence is not None:
        result["new"] = new_evidence
    return result


def run_helper(request, transport) -> dict:
    old_evidence = new_evidence = None
    dispatched = False
    try:
        _exact_fields(request, {"owner", "path", "oldFile", "newFile"})
        owner = _component(request["owner"], "owner")
        path = _head_path(request["path"])
        if owner != transport.local_owner:
            raise BoundaryError("owner-mismatch", "requested owner is not the configured local owner")
        old, old_evidence = _read_boundary(request["oldFile"], "oldFile")
        new, new_evidence = _read_boundary(request["newFile"], "newFile")
        dispatched = True
        response = transport.dispatch(owner, path, old, new)
        if response is True:
            result = _base_result(old_evidence, new_evidence)
            result.update({"outcome": "accepted", "writeAccepted": True, "dispatchCount": 1})
            return result
        if response is False:
            raise ExplicitReject("conflict", "expected old bytes did not match")
        raise CompletionUnestablished("Interface result was neither exact true nor exact false")
    except BoundaryError as error:
        result = _base_result(old_evidence, new_evidence)
        result.update({"outcome": "not-attempted", "writeAccepted": False, "dispatchCount": 0,
                       "error": {"code": error.code, "message": str(error)[:256]}})
        return result
    except ExplicitReject as error:
        result = _base_result(old_evidence, new_evidence)
        result.update({"outcome": "rejected", "writeAccepted": False, "dispatchCount": 1 if dispatched else 0,
                       "error": {"code": error.code, "message": str(error)[:256]}})
        return result
    except CompletionUnestablished as error:
        result = _base_result(old_evidence, new_evidence)
        result.update({"outcome": "failed-or-ambiguous", "writeAccepted": False,
                       "dispatchCount": 1 if dispatched else 0,
                       "error": {"code": "completion-unestablished", "message": str(error)[:256]}})
        return result
    except Exception:
        result = _base_result(old_evidence, new_evidence)
        result.update({"outcome": "failed-or-ambiguous" if dispatched else "not-attempted",
                       "writeAccepted": False, "dispatchCount": 1 if dispatched else 0,
                       "error": {"code": "unexpected-failure", "message": "bounded helper failure"}})
        return result


def attach_later_observation(primary: dict, observation: dict) -> dict:
    """A later observation is additive evidence and cannot rewrite the primary outcome."""
    result = json.loads(json.dumps(primary))
    result["laterObservation"] = json.loads(json.dumps(observation))
    return result


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Bounded expected-old head CAS helper")
    parser.add_argument("--request", required=True)
    parser.add_argument("--helper-sha256", required=True)
    parser.add_argument("--config", default=DEFAULT_CONFIG)
    parser.add_argument("--state-dir", default=DEFAULT_STATE)
    args = parser.parse_args(argv)
    source = Path(__file__).read_bytes()
    if hashlib.sha256(source).hexdigest() != args.helper_sha256:
        result = _base_result()
        result.update({"outcome": "not-attempted", "writeAccepted": False, "dispatchCount": 0,
                       "error": {"code": "source-mismatch", "message": "helper source hash mismatch"}})
    else:
        try:
            request = parse_request_bytes(Path(args.request).read_bytes())
            transport = LocalInterfaceTransport(Path(args.config), Path(args.state_dir))
            result = run_helper(request, transport)
        except BoundaryError as error:
            result = _base_result()
            result.update({"outcome": "not-attempted", "writeAccepted": False, "dispatchCount": 0,
                           "error": {"code": error.code, "message": str(error)[:256]}})
        except OSError:
            result = _base_result()
            result.update({"outcome": "not-attempted", "writeAccepted": False, "dispatchCount": 0,
                           "error": {"code": "unsupported-input", "message": "cannot read bounded request file"}})
    sys.stdout.write(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n")
    return 0 if result["outcome"] in {"accepted", "rejected", "not-attempted"} else 3


if __name__ == "__main__":
    raise SystemExit(main())
