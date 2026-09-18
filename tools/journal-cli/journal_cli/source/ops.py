#!/usr/bin/python3
"""Reference source-agnostic current-get, fixed-view, and retention commands for pi-sync."""

from __future__ import annotations

import argparse
import base64
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import urllib.error
import urllib.request

CONTRACT = "journal-cli-source-ops-v2"
VERSION = 2
MAX_VALUE_BYTES = 512 * 1024
MAX_REQUEST_BYTES = 1024 * 1024
MAX_RESPONSE_BYTES = 4 * 1024 * 1024 - 4096
MAX_ERROR_BYTES = 16 * 1024
MAX_PATHS = 1024
MAX_INDEX = (1 << 63) - 1
MAX_DEPTH = 64
MAX_PATH_BYTES = 4096
MAX_BOUND_INTEGER = (1 << 63) - 1
RESPONSE_BASE_OVERHEAD = 65536
RESPONSE_ITEM_OVERHEAD = 2048
DEFAULT_CONFIG = "/etc/pi-agent/agent.json"
DEFAULT_STATE = "/var/lib/pi-agent/sync"
COMPONENT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
ROUTE_COMPONENT = re.compile(r"^[A-Za-z0-9_.*+!<>=?/-]{1,128}$")
S7_BARE_SYMBOL = re.compile(r"^[A-Za-z_*!<>=?/][A-Za-z0-9_.*+!<>=?/-]{0,127}$")
UUID_SYMBOL = re.compile(r"^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$")
LOWER_SHA256 = re.compile(r"^[0-9a-f]{64}$")
RESERVED_S7_SYMBOLS = frozenset({
    "*admins-get*", "*admins-set*", "*autoload-hook*", "*bridge*", "*call*", "*crypto*",
    "*error-hook*", "*init*", "*journal*", "*load-hook*", "*missing-close-paren-hook*",
    "*periodic*", "*public*", "*read-error-hook*", "*removed*", "*rootlet-redefinition-hook*",
    "*s7*", "*secret*", "*set-query*", "*set-step*", "*state*", "*step*", "*sync-state*",
    "*time*", "*transition*", "*unbound-variable-hook*", "*window-set*",
})


@dataclass(frozen=True)
class Symbol:
    name: str


class RequestError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


class ExplicitReject(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


class AmbiguousFailure(Exception):
    pass


class SexpParser:
    def __init__(self, text: str):
        self.text = text
        self.pos = 0
        self.nodes = 0

    def parse(self):
        value = self._value(0)
        self._space()
        if self.pos != len(self.text):
            raise ValueError("trailing Scheme response data")
        return value

    def _space(self):
        while self.pos < len(self.text) and self.text[self.pos].isspace():
            self.pos += 1

    def _value(self, depth: int):
        if depth > 128:
            raise ValueError("Scheme response nesting exceeds 128")
        self.nodes += 1
        if self.nodes > 1_000_000:
            raise ValueError("Scheme response node count exceeds limit")
        self._space()
        if self.pos >= len(self.text):
            raise ValueError("unexpected end of Scheme response")
        if self.text.startswith("#u(", self.pos):
            return self._bytes()
        char = self.text[self.pos]
        if char == "(":
            self.pos += 1
            values = []
            while True:
                self._space()
                if self.pos >= len(self.text):
                    raise ValueError("unterminated Scheme list")
                if self.text[self.pos] == ")":
                    self.pos += 1
                    return values
                values.append(self._value(depth + 1))
        if char == "'":
            self.pos += 1
            return [Symbol("quote"), self._value(depth + 1)]
        if char == '"':
            return self._string()
        if char == "|":
            return self._bar_symbol()
        start = self.pos
        while self.pos < len(self.text) and not self.text[self.pos].isspace() and self.text[self.pos] not in "()":
            self.pos += 1
        token = self.text[start:self.pos]
        if token == "#t":
            return True
        if token == "#f":
            return False
        if re.fullmatch(r"-?(?:0|[1-9][0-9]*)", token):
            return int(token)
        if not token:
            raise ValueError("invalid Scheme token")
        return Symbol(token)

    def _bytes(self):
        self.pos += 3
        values = bytearray()
        while True:
            self._space()
            if self.pos >= len(self.text):
                raise ValueError("unterminated byte vector")
            if self.text[self.pos] == ")":
                self.pos += 1
                return bytes(values)
            start = self.pos
            while self.pos < len(self.text) and self.text[self.pos].isdigit():
                self.pos += 1
            token = self.text[start:self.pos]
            if not token:
                raise ValueError("invalid byte-vector token")
            value = int(token)
            if value > 255:
                raise ValueError("byte-vector value exceeds 255")
            values.append(value)
            if len(values) > MAX_RESPONSE_BYTES:
                raise ValueError("byte vector exceeds response bound")

    def _string(self):
        self.pos += 1
        out = []
        escapes = {'"': '"', "\\": "\\", "n": "\n", "r": "\r", "t": "\t", "b": "\b", "f": "\f"}
        while self.pos < len(self.text):
            char = self.text[self.pos]
            self.pos += 1
            if char == '"':
                return "".join(out)
            if char == "\\":
                if self.pos >= len(self.text) or self.text[self.pos] not in escapes:
                    raise ValueError("unsupported Scheme string escape")
                char = escapes[self.text[self.pos]]
                self.pos += 1
            out.append(char)
            if len(out) > MAX_RESPONSE_BYTES:
                raise ValueError("Scheme string exceeds response bound")
        raise ValueError("unterminated Scheme string")

    def _bar_symbol(self):
        self.pos += 1
        start = self.pos
        while self.pos < len(self.text) and self.text[self.pos] != "|":
            if self.text[self.pos] == "\\":
                raise ValueError("escaped bar symbol is unsupported")
            self.pos += 1
        if self.pos >= len(self.text):
            raise ValueError("unterminated bar symbol")
        value = self.text[start:self.pos]
        self.pos += 1
        return Symbol(value)


def parse_sexp(text: str):
    return SexpParser(text).parse()


def alist(value, label: str) -> dict:
    if not isinstance(value, list):
        raise ValueError(f"{label} is not an alist")
    result = {}
    for entry in value:
        if not isinstance(entry, list) or len(entry) != 2 or not isinstance(entry[0], Symbol):
            raise ValueError(f"{label} contains malformed entry")
        key = entry[0].name
        if key in result:
            raise ValueError(f"{label} contains duplicate field")
        result[key] = entry[1]
    return result


def symbol_name(value, label: str) -> str:
    if not isinstance(value, Symbol):
        raise ValueError(f"{label} is not a symbol")
    return value.name


def bounded_read(stream, limit: int, label: str) -> bytes:
    data = stream.read(limit + 1)
    if len(data) > limit:
        raise RequestError("unsupported-capability", f"{label} exceeds {limit} bytes")
    return data


def reject_json_constant(value):
    raise RequestError("invalid-request", f"non-finite JSON number is forbidden: {value}")


def json_no_duplicates(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise RequestError("invalid-request", f"duplicate JSON field: {key}")
        result[key] = value
    return result


def read_json_request(path: str):
    if path == "-":
        raw = bounded_read(sys.stdin.buffer, MAX_REQUEST_BYTES, "request")
    else:
        flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
        try:
            fd = os.open(path, flags)
        except OSError as error:
            raise RequestError("invalid-request", f"cannot open request file: {error}") from error
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_REQUEST_BYTES:
                raise RequestError("invalid-request", "request must be a bounded regular file")
            chunks = []
            remaining = MAX_REQUEST_BYTES + 1
            while remaining:
                chunk = os.read(fd, min(65536, remaining))
                if not chunk:
                    break
                chunks.append(chunk)
                remaining -= len(chunk)
            raw = b"".join(chunks)
            if len(raw) > MAX_REQUEST_BYTES:
                raise RequestError("unsupported-capability", f"request exceeds {MAX_REQUEST_BYTES} bytes")
        finally:
            os.close(fd)
    try:
        value = json.loads(raw, object_pairs_hook=json_no_duplicates, parse_constant=reject_json_constant)
    except RequestError:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError, ValueError) as error:
        raise RequestError("invalid-request", f"invalid JSON request: {error}") from error
    if not isinstance(value, dict):
        raise RequestError("invalid-request", "request must be one JSON object")
    return value, hashlib.sha256(raw).hexdigest()


def exact_fields(value: dict, required: set[str], optional: set[str] = frozenset()):
    if set(value) != required | (set(value) & optional):
        missing = sorted(required - set(value))
        unknown = sorted(set(value) - required - optional)
        raise RequestError("invalid-request", f"request fields mismatch; missing={missing}, unknown={unknown}")


def validate_version(value: dict):
    if type(value.get("version")) is not int or value["version"] != VERSION:
        raise RequestError("invalid-request", "version must be integer 1")


def validate_component(value, label: str) -> str:
    if isinstance(value, str) and value in RESERVED_S7_SYMBOLS:
        raise RequestError("unsupported-capability", "component is reserved implementation syntax")
    if not isinstance(value, str) or not COMPONENT.fullmatch(value) or value in {".", ".."}:
        raise RequestError("invalid-request", f"invalid {label}")
    scheme_symbol(value)
    return value


def validate_route(value) -> list[str]:
    if not isinstance(value, list) or len(value) > 16:
        raise RequestError("invalid-request", "route must be an array of at most 16 components")
    route = []
    for item in value:
        if not isinstance(item, str) or not ROUTE_COMPONENT.fullmatch(item) or item in {".", ".."} or "/" in item:
            raise RequestError("invalid-request", "invalid route component")
        scheme_symbol(item)
        route.append(item)
    return route


def validate_path(value, label: str = "path") -> list[str]:
    if not isinstance(value, list) or not 1 <= len(value) <= MAX_DEPTH:
        raise RequestError("invalid-request", f"{label} must contain 1..{MAX_DEPTH} components")
    path = [validate_component(item, f"{label} component") for item in value]
    if sum(len(item.encode()) for item in path) + len(path) - 1 > MAX_PATH_BYTES:
        raise RequestError("invalid-request", f"{label} exceeds {MAX_PATH_BYTES} encoded bytes")
    return path


def validate_paths(value) -> list[list[str]]:
    if not isinstance(value, list) or not 1 <= len(value) <= MAX_PATHS:
        raise RequestError("invalid-request", f"paths must contain 1..{MAX_PATHS} path arrays")
    return [validate_path(path, f"paths[{index}]") for index, path in enumerate(value)]


def scheme_symbol(value: str) -> str:
    if value in RESERVED_S7_SYMBOLS:
        raise RequestError("unsupported-capability", "component is reserved implementation syntax")
    if S7_BARE_SYMBOL.fullmatch(value) or UUID_SYMBOL.fullmatch(value):
        return value
    raise RequestError("unsupported-capability", "component has no supported inert s7 symbol literal")


def scheme_list(values) -> str:
    return "(" + " ".join(values) + ")"


def scheme_path(values, internal_positions=frozenset()) -> str:
    encoded = []
    expected_internal = (internal_positions if isinstance(internal_positions, dict)
                         else {index: "*state*" for index in internal_positions})
    for index, item in enumerate(values):
        if type(item) is int:
            encoded.append(str(item))
        elif index in expected_internal:
            if item != expected_internal[index]:
                raise RequestError("invalid-request", "unknown implementation-owned path symbol")
            encoded.append(item)
        else:
            encoded.append(scheme_symbol(item))
    return scheme_list(encoded)


def scheme_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def byte_vector(value: bytes) -> str:
    return "#u(" + " ".join(str(item) for item in value) + ")"


def canonical_json(value) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode()


def checked_bound_add(total: int, amount: int) -> int:
    if total < 0 or amount < 0 or total > MAX_BOUND_INTEGER - amount:
        raise RequestError("unsupported-capability", "response-bound arithmetic overflow")
    return total + amount


def checked_bound_multiply(left: int, right: int) -> int:
    if left < 0 or right < 0 or (left and right > MAX_BOUND_INTEGER // left):
        raise RequestError("unsupported-capability", "response-bound arithmetic overflow")
    return left * right


def released_s7_byte_vector_bound(size: int) -> int:
    if size == 0:
        return 4  # #u()
    return checked_bound_add(checked_bound_multiply(4, size), 3)


def current_response_bound_proofs(owner: str, reads: list[dict]) -> tuple[int, int]:
    raw = RESPONSE_BASE_OVERHEAD
    encoded = RESPONSE_BASE_OVERHEAD
    for read in reads:
        staged = ["*state*", owner, *read["path"]]
        path_bytes = len(scheme_path(staged, {0}).encode())
        raw = checked_bound_add(raw, checked_bound_multiply(2, path_bytes))
        raw = checked_bound_add(raw, released_s7_byte_vector_bound(read["maxBytes"]))
        raw = checked_bound_add(raw, RESPONSE_ITEM_OVERHEAD)
        encoded = checked_bound_add(encoded, checked_bound_multiply(2, path_bytes))
        encoded = checked_bound_add(encoded, checked_bound_multiply(4, (read["maxBytes"] + 2) // 3))
        encoded = checked_bound_add(encoded, RESPONSE_ITEM_OVERHEAD)
    return raw, encoded


def sha256_file() -> str:
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(req.full_url, code, "redirect refused", headers, fp)


URL_OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())


class InterfaceClient:
    def __init__(self, endpoint: str, secret: str, timeout: float = 30.0):
        self.endpoint = endpoint
        self.secret = secret
        self.timeout = timeout

    def authentication(self, identity: str | None = None) -> str:
        fields = []
        if identity is not None:
            fields.append(f"(identity (*state* {scheme_symbol(identity)}))")
        fields.append(f"(credentials {scheme_string(self.secret)})")
        return "(authentication (" + " ".join(fields) + "))"

    def federated_invocation(self, identity: str, route: list[str]) -> str:
        return scheme_list([
            f"(identity {scheme_symbol(identity)})", "(route-source ())",
            f"(route-target {scheme_list(scheme_symbol(item) for item in route)})",
            f"(credentials {scheme_string(self.secret)})",
        ])

    def post(self, expression: str, response_limit: int = MAX_RESPONSE_BYTES):
        if type(response_limit) is not int or not 1 <= response_limit <= MAX_RESPONSE_BYTES:
            raise RequestError("unsupported-capability", "Interface response limit is outside the supported bound")
        request = urllib.request.Request(
            self.endpoint, data=expression.encode(), headers={"Content-Type": "application/scheme"}, method="POST"
        )
        try:
            with URL_OPENER.open(request, timeout=self.timeout) as response:
                raw = response.read(response_limit + 1)
                if len(raw) > response_limit:
                    raise AmbiguousFailure(f"Interface response exceeded {response_limit} bytes")
        except urllib.error.HTTPError as error:
            body = error.read(MAX_ERROR_BYTES + 1)
            if error.code == 400 and len(body) <= MAX_ERROR_BYTES:
                raise ExplicitReject("interface-rejected", "Interface explicitly rejected request (HTTP 400)") from error
            raise AmbiguousFailure(f"Interface HTTP {error.code} did not establish completion") from error
        except (OSError, urllib.error.URLError) as error:
            raise AmbiguousFailure(f"Interface transport did not establish completion: {error}") from error
        try:
            text = raw.decode("utf-8", errors="strict")
        except UnicodeDecodeError as error:
            raise AmbiguousFailure("Interface response was not UTF-8") from error
        stripped = text.lstrip()
        if stripped.startswith("(error"):
            raise ExplicitReject("interface-rejected", "Interface explicitly rejected request")
        try:
            return parse_sexp(text)
        except ValueError as error:
            raise AmbiguousFailure(f"Interface response framing failed: {error}") from error


class SourceOps:
    def __init__(self, config: dict, state_dir: str, client):
        self.config = config
        self.state_dir = state_dir
        self.client = client
        self.local_owner = validate_component(config.get("owner", config.get("id")), "configured local owner")

    def _local_auth(self) -> str:
        return self.client.authentication(self.local_owner)

    def _post(self, function: str, arguments: str, authenticate: bool = True,
              response_limit: int = MAX_RESPONSE_BYTES):
        auth = f" {self._local_auth()}" if authenticate else ""
        expression = f"((function {function}) (arguments {arguments}){auth})"
        if response_limit == MAX_RESPONSE_BYTES:
            return self.client.post(expression)
        return self.client.post(expression, response_limit=response_limit)

    def _post_current_get_batch(self, route: list[str], arguments: str, response_limit: int):
        if not route:
            return self._post("use-batch!", arguments, response_limit=response_limit)
        invocation = self.client.federated_invocation(self.local_owner, route)
        return self.client.post(
            f"((function use-batch!) (arguments {arguments}) (invocation {invocation}))",
            response_limit=response_limit,
        )

    @staticmethod
    def _history_template(route: list[str], view: dict) -> list[int]:
        kind = view.get("kind")
        if kind == "current":
            exact_fields(view, {"kind"})
            return [-1] * (len(route) + 1)
        if kind != "fixed":
            raise RequestError("invalid-request", "view.kind must be current or fixed")
        exact_fields(view, {"kind", "index"}, {"historyIndexes"})
        requested = view["index"]
        if type(requested) is not int or not 0 <= requested <= MAX_INDEX:
            raise RequestError("invalid-request", "fixed view requires a bounded terminal index")
        indexes = view.get("historyIndexes")
        if indexes is None:
            return [-1] * len(route) + [requested]
        if (not isinstance(indexes, list) or len(indexes) != len(route) + 1
                or any(type(item) is not int or not 0 <= item <= MAX_INDEX for item in indexes)
                or indexes[-1] != requested):
            raise RequestError("invalid-request", "fixed view history indexes are invalid")
        return indexes

    @staticmethod
    def committed_path(route: list[str], indexes: list[int], owner: str, relative: list[str]) -> list:
        path = [indexes[0]]
        for alias, index in zip(route, indexes[1:]):
            path.extend([alias, index])
        path.extend(["*state*", owner, *relative])
        return path

    def get_current(self, request: dict):
        exact_fields(request, {
            "version", "route", "owner", "reads", "rawResponseBytesUpperBound", "responseBytesUpperBound",
        }, {"endpoint"})
        validate_version(request)
        if request.get("endpoint") is not None and request["endpoint"] != self.client.endpoint:
            raise ExplicitReject("endpoint-mismatch", "Source endpoint differs from the active Interface endpoint")
        route = validate_route(request["route"])
        owner = validate_component(request["owner"], "owner")
        reads = request["reads"]
        if not isinstance(reads, list) or not 1 <= len(reads) <= MAX_PATHS:
            raise RequestError("invalid-request", f"reads must contain 1..{MAX_PATHS} entries")
        raw_bound = request["rawResponseBytesUpperBound"]
        encoded_bound = request["responseBytesUpperBound"]
        if type(raw_bound) is not int or not 1 <= raw_bound <= MAX_RESPONSE_BYTES:
            raise RequestError("invalid-request", "rawResponseBytesUpperBound is outside the supported bound")
        if type(encoded_bound) is not int or not 1 <= encoded_bound <= MAX_RESPONSE_BYTES:
            raise RequestError("invalid-request", "responseBytesUpperBound is outside the supported bound")
        normalized = []
        seen_paths = set()
        for index, read in enumerate(reads):
            if not isinstance(read, dict):
                raise RequestError("invalid-request", f"reads[{index}] must be an object")
            exact_fields(read, {"path", "maxBytes"}, {"expected"})
            path = validate_path(read["path"], f"reads[{index}].path")
            path_key = tuple(path)
            if path_key in seen_paths:
                raise RequestError("invalid-request", "get-current paths must be unique within one operation")
            seen_paths.add(path_key)
            maximum = read["maxBytes"]
            if type(maximum) is not int or not 0 <= maximum <= MAX_VALUE_BYTES:
                raise RequestError("invalid-request", f"reads[{index}].maxBytes is outside the supported bound")
            expected = read.get("expected")
            if expected is not None:
                if not isinstance(expected, dict):
                    raise RequestError("invalid-request", f"reads[{index}].expected must be an object")
                exact_fields(expected, {"bytes", "sha256"})
                if (type(expected["bytes"]) is not int or not 0 <= expected["bytes"] <= maximum
                        or not isinstance(expected["sha256"], str)
                        or not LOWER_SHA256.fullmatch(expected["sha256"])):
                    raise RequestError("invalid-request", f"reads[{index}].expected is invalid")
            normalized.append({"path": path, "maxBytes": maximum, "expected": expected})
        raw_proof, encoded_proof = current_response_bound_proofs(owner, normalized)
        if raw_proof > raw_bound:
            raise RequestError("unsupported-capability", "conservative raw Interface response exceeds declared bound")
        if encoded_proof > encoded_bound:
            raise RequestError("unsupported-capability", "conservative encoded result exceeds declared bound")
        staged = [["*state*", owner, *read["path"]] for read in normalized]
        arguments = scheme_list([
            "(paths " + scheme_list(scheme_path(path, {0}) for path in staged) + ")",
            "(read-only? #t)", "(expression? #f)",
        ])
        values = self._post_current_get_batch(route, arguments, raw_bound)
        if not isinstance(values, list) or len(values) != len(normalized):
            raise ValueError("use-batch! result cardinality mismatch")
        results = []
        for index, value in enumerate(values):
            result = {"path": normalized[index]["path"]}
            content = self._normalize_content(value, result)
            if content.get("shape") == "value":
                if content["bytes"] > normalized[index]["maxBytes"]:
                    raise AmbiguousFailure("get-current value exceeded declared maxBytes")
                expected = normalized[index]["expected"]
                if expected is not None and (content["bytes"] != expected["bytes"]
                                             or content["sha256"] != expected["sha256"]):
                    raise ExplicitReject("content-mismatch", "get-current content did not match expected bytes/hash")
            elif content.get("shape") != "nothing":
                raise ValueError("get-current content shape was unsupported")
            result.update(content)
            results.append(result)
        payload = {
            "outcome": "retrieved", "results": results,
            "currentEvidence": {
                "mode": "current-stage-use-batch", "requestedRoute": route, "requestedOwner": owner,
                "snapshotScope": "one-interface-invocation",
                "contentObservedLocator": None, "contentObservedIndex": None, "committed": False,
                "routeContinuityObservations": 0, "useBatchDispatches": 1,
                "crossBatchContinuity": "unestablished",
                "rawResponseBytesUpperBound": raw_bound, "rawResponseBytesProof": raw_proof,
                "responseBytesUpperBound": encoded_bound, "responseBytesProof": encoded_proof,
            },
        }
        if len(canonical_json(payload)) > encoded_bound:
            raise AmbiguousFailure("get-current encoded result exceeded responseBytesUpperBound")
        return payload

    def _observe_public_endpoint(self, expected: str) -> str:
        observer = getattr(self.client, "observe_public_endpoint", None)
        if observer is not None:
            return observer(expected)
        probe = InterfaceClient(expected, "", self.client.timeout)
        info = alist(probe.post("((function info))"), "public info response")
        interface = alist(info.get("interface"), "public interface descriptor")
        observed = interface.get("endpoint")
        if observed != expected:
            raise ExplicitReject("endpoint-mismatch", "publisher endpoint did not self-identify exactly")
        return observed

    def resolve_view(self, request: dict):
        exact_fields(request, {"version", "endpoint", "route", "owner", "view", "paths"}, {"includePinned"})
        validate_version(request)
        expected_endpoint = request["endpoint"]
        if expected_endpoint is not None and not isinstance(expected_endpoint, str):
            raise RequestError("invalid-request", "endpoint must be a string or null")
        route = validate_route(request["route"])
        owner = validate_component(request["owner"], "owner")
        paths = validate_paths(request["paths"])
        include_pinned = request.get("includePinned", False)
        if type(include_pinned) is not bool:
            raise RequestError("invalid-request", "includePinned must be boolean")
        if not isinstance(request["view"], dict):
            raise RequestError("invalid-request", "view must be an object")
        template = self._history_template(route, request["view"])
        committed = [self.committed_path(route, template, owner, path) for path in paths]
        internal_position = 1 + 2 * len(route)
        encoded_paths = [scheme_path(path, {internal_position: "*state*"}) for path in committed]
        capture_indexes = any(index < 0 for index in template)
        arguments = scheme_list([
            "(paths " + scheme_list(encoded_paths) + ")", "(expression? #f)",
            *(["(index? #t)"] if capture_indexes else []),
            f"(pinned? {'#t' if include_pinned else '#f'})",
        ])
        response = alist(self._post("retrieve-batch", arguments), "retrieve-batch response")
        values = response.get("results")
        if not isinstance(values, list) or len(values) != len(paths):
            raise AmbiguousFailure("retrieve-batch result cardinality mismatch")
        parsed = [alist(value, f"retrieve result {index}") for index, value in enumerate(values)]
        if capture_indexes:
            histories = [item.get("indexes") for item in parsed]
            if (any(not isinstance(history, list) or len(history) != len(route) + 1
                    or any(type(index) is not int or index < 0 for index in history) for history in histories)
                    or any(history != histories[0] for history in histories[1:])):
                raise AmbiguousFailure("retrieve-batch did not establish one exact route history")
            history = histories[0]
        else:
            history = template
        if request["view"].get("kind") == "fixed" and history[-1] != request["view"]["index"]:
            raise ExplicitReject("fixed-index-mismatch", "resolved terminal index changed")
        terminal_endpoint = self._observe_public_endpoint(expected_endpoint or self.client.endpoint)
        results = []
        for index, (item, relative) in enumerate(zip(parsed, paths)):
            exact_path = self.committed_path(route, history, owner, relative)
            returned_path = item.get("path")
            if (returned_path is not None and returned_path != self._sexp_path(exact_path)
                    and returned_path != self._sexp_path(committed[index])):
                raise AmbiguousFailure("retrieve-batch returned a mismatched path")
            result = {"path": relative, "canonicalCommittedPath": exact_path}
            result.update(self._normalize_content(item.get("content"), result))
            if include_pinned:
                if type(item.get("pinned?")) is not bool:
                    raise AmbiguousFailure("retrieve result omitted pinned state")
                result["pinned"] = item["pinned?"]
            results.append(result)
        selection = {
            "entryEndpoint": self.client.endpoint, "terminalEndpoint": terminal_endpoint,
            "route": route, "historyIndexes": history, "owner": owner,
            "selectedIndex": history[-1], "originIndex": history[0],
        }
        return {"outcome": "retrieved", "view": self._public_selection(selection), "results": results}

    @staticmethod
    def _sexp_path(path):
        return [Symbol(item) if isinstance(item, str) else item for item in path]

    @staticmethod
    def _normalize_content(content, result):
        if isinstance(content, bytes):
            if len(content) > MAX_VALUE_BYTES:
                raise AmbiguousFailure("resolved value exceeded 524288 bytes")
            return {"shape": "value", "contentBase64": base64.b64encode(content).decode(),
                    "bytes": len(content), "sha256": hashlib.sha256(content).hexdigest()}
        if isinstance(content, list) and len(content) == 1 and content[0] == Symbol("nothing"):
            return {"shape": "nothing"}
        if isinstance(content, list) and len(content) == 1 and content[0] == Symbol("unknown"):
            return {"shape": "unknown"}
        if isinstance(content, list) and len(content) == 3 and content[0] == Symbol("directory") and isinstance(content[1], list) and type(content[2]) is bool:
            entries = []
            for entry in content[1]:
                if not isinstance(entry, list) or len(entry) != 2:
                    raise AmbiguousFailure("directory entry shape was invalid")
                entries.append({"name": symbol_name(entry[0], "directory name"),
                                "shape": symbol_name(entry[1], "directory entry shape")})
            return {"shape": "directory", "entries": entries, "complete": content[2]}
        if isinstance(content, list) and len(content) == 3 and content[0] == Symbol("make-byte-vector") and type(content[1]) is int and type(content[2]) is int:
            length, fill = content[1], content[2]
            if not 0 <= length <= MAX_VALUE_BYTES or not 0 <= fill <= 255:
                raise AmbiguousFailure("uniform byte vector exceeded bounds")
            data = bytes([fill]) * length
            return {"shape": "value", "contentBase64": base64.b64encode(data).decode(),
                    "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        raise AmbiguousFailure("resolve result content shape was unsupported")

    @staticmethod
    def _public_selection(selection):
        return {key: selection[key] for key in ("entryEndpoint", "terminalEndpoint", "route", "historyIndexes", "owner", "selectedIndex", "originIndex")}

    def create_value(self, request: dict):
        exact_fields(request, {"version", "owner", "path", "input", "expected"}, {"readback"})
        validate_version(request)
        owner = validate_component(request["owner"], "owner")
        if owner != self.local_owner:
            raise RequestError("unsupported-capability", "create-value owner must be the configured local owner")
        path = validate_path(request["path"])
        if request["expected"] != "absent":
            raise RequestError("invalid-request", "expected must be absent")
        if not isinstance(request["input"], str) or not request["input"]:
            raise RequestError("invalid-request", "input must name a regular file")
        readback = request.get("readback", False)
        if type(readback) is not bool:
            raise RequestError("invalid-request", "readback must be boolean")
        data = self._read_value_file(request["input"])
        staged = ["*state*", owner, *path]
        arguments = scheme_list([
            f"(path {scheme_path(staged, {0})})", f"(value {byte_vector(data)})",
            "(expected (nothing))", "(expression? #f)",
        ])
        response = self._post("put!", arguments)
        if response is False:
            raise ExplicitReject("conflict", "expected absence did not match")
        if response is not True:
            raise AmbiguousFailure("create-value response was neither #t nor #f")
        result = {"outcome": "accepted", "path": path, "bytes": len(data),
                  "sha256": hashlib.sha256(data).hexdigest(), "readback": {"outcome": "not-requested"}}
        if readback:
            try:
                read = self._post("use!", scheme_list([
                    f"(path {scheme_path(staged, {0})})", "(read-only? #t)", "(expression? #f)"
                ]))
                normalized = self._normalize_content(read, {})
                if normalized.get("shape") != "value" or normalized.get("sha256") != result["sha256"] or normalized.get("bytes") != result["bytes"]:
                    result["readback"] = {"outcome": "rejected", "code": "content-mismatch"}
                else:
                    result["readback"] = {"outcome": "retrieved", "bytes": result["bytes"], "sha256": result["sha256"]}
            except (ExplicitReject, AmbiguousFailure) as error:
                result["readback"] = {"outcome": "failed-or-ambiguous", "code": "readback-unestablished", "message": str(error)[:512]}
        return result

    @staticmethod
    def _read_value_file(path: str) -> bytes:
        flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
        try:
            fd = os.open(path, flags)
        except OSError as error:
            raise RequestError("invalid-request", f"cannot open input file: {error}") from error
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_VALUE_BYTES:
                raise RequestError("unsupported-capability", "input must be a regular file of at most 524288 bytes")
            data = b""
            while len(data) <= MAX_VALUE_BYTES:
                chunk = os.read(fd, min(65536, MAX_VALUE_BYTES + 1 - len(data)))
                if not chunk:
                    break
                data += chunk
            if len(data) > MAX_VALUE_BYTES:
                raise RequestError("unsupported-capability", "input exceeds 524288 bytes")
            return data
        finally:
            os.close(fd)

    def pin_view(self, request: dict):
        exact_fields(request, {"version", "endpoint", "route", "historyIndexes", "owner", "index", "paths", "proofEncodedBytesUpperBound"})
        validate_version(request)
        route = validate_route(request["route"])
        if not isinstance(request["endpoint"], str):
            raise RequestError("invalid-request", "fixed Source endpoint must be a string")
        owner = validate_component(request["owner"], "owner")
        paths = validate_paths(request["paths"])
        index = request["index"]
        bound = request["proofEncodedBytesUpperBound"]
        if type(index) is not int or not 0 <= index <= MAX_INDEX:
            raise RequestError("invalid-request", "index must be a bounded nonnegative integer")
        if type(bound) is not int or bound < 1:
            raise RequestError("invalid-request", "proofEncodedBytesUpperBound must be positive")
        if bound > MAX_RESPONSE_BYTES:
            raise RequestError("unsupported-capability", "conservative proof bound exceeds 4190208 bytes")
        observed = self.resolve_view({
            "version": 2, "endpoint": request["endpoint"], "route": route, "owner": owner,
            "view": {"kind": "fixed", "index": index, "historyIndexes": request["historyIndexes"]},
            "paths": [paths[0]], "includePinned": False,
        })
        selection = observed["view"]
        committed = [self.committed_path(route, selection["historyIndexes"], owner, path) for path in paths]
        response = self._post("pin-batch!", scheme_list([
            "(paths " + scheme_list(scheme_path(path, {1 + 2 * len(route)}) for path in committed) + ")"
        ]))
        if response is False:
            raise ExplicitReject("pin-rejected", "pin batch returned false without accepted mutation")
        if response is not True:
            raise AmbiguousFailure("pin-view response was neither #t nor #f")
        handles = [self._make_handle(selection, path, canonical) for path, canonical in zip(paths, committed)]
        return {"outcome": "accepted", "view": self._public_selection(selection), "handles": handles,
                "completeness": "unverified", "requestAtomic": True}

    def _make_handle(self, selection, path, canonical):
        body = {"handleVersion": 2, "originOwner": self.local_owner,
                "entryEndpoint": selection["entryEndpoint"], "publisherEndpoint": selection["terminalEndpoint"],
                "publisherOwner": selection["owner"],
                "routeObservation": selection["route"], "historyIndexes": selection["historyIndexes"],
                "publisherIndex": selection["selectedIndex"], "path": path, "canonicalCommittedPath": canonical}
        body["handleSha256"] = hashlib.sha256(canonical_json(body)).hexdigest()
        return body

    def unpin_retention(self, request: dict):
        exact_fields(request, {"version", "handles"})
        validate_version(request)
        handles = request["handles"]
        if not isinstance(handles, list) or not 1 <= len(handles) <= MAX_PATHS:
            raise RequestError("invalid-request", f"handles must contain 1..{MAX_PATHS} entries")
        canonical = []
        normalized = []
        for index, handle in enumerate(handles):
            if not isinstance(handle, dict):
                raise RequestError("invalid-request", f"handles[{index}] must be an object")
            exact_fields(handle, {"handleVersion", "originOwner", "entryEndpoint", "publisherEndpoint", "publisherOwner", "routeObservation", "historyIndexes", "publisherIndex", "path", "canonicalCommittedPath", "handleSha256"})
            if handle["handleVersion"] != 2 or handle["originOwner"] != self.local_owner:
                raise RequestError("invalid-request", "retention handle version/origin mismatch")
            expected = dict(handle)
            digest = expected.pop("handleSha256")
            if not isinstance(digest, str) or digest != hashlib.sha256(canonical_json(expected)).hexdigest():
                raise RequestError("invalid-request", "retention handle digest mismatch")
            if handle["entryEndpoint"] != self.client.endpoint or not isinstance(handle["publisherEndpoint"], str):
                raise RequestError("invalid-request", "retention handle endpoint binding differs")
            validate_component(handle["publisherOwner"], "publisher owner")
            validate_route(handle["routeObservation"])
            validate_path(handle["path"], "retention path")
            if type(handle["publisherIndex"]) is not int or not 0 <= handle["publisherIndex"] <= MAX_INDEX:
                raise RequestError("invalid-request", "retention publisher index is invalid")
            value = self._validate_canonical_path(handle["canonicalCommittedPath"])
            state = value.index("*state*")
            aliases = value[1:state:2]
            terminal_index = value[state - 1] if state > 1 else value[0]
            observed_indexes = value[:state:2]
            if (aliases != handle["routeObservation"] or observed_indexes != handle["historyIndexes"]
                    or terminal_index != handle["publisherIndex"]
                    or value[state + 1] != handle["publisherOwner"]
                    or value[state + 2:] != handle["path"]):
                raise RequestError("invalid-request", "retention handle fields do not match its canonical path")
            canonical.append(value)
            normalized.append(handle)
        encoded_paths = [
            scheme_path(path, {1 + 2 * len(handle["routeObservation"])})
            for path, handle in zip(canonical, normalized)
        ]
        response = self._post("unpin-batch!", scheme_list(["(paths " + scheme_list(encoded_paths) + ")"]))
        if response is False:
            raise ExplicitReject("unpin-rejected", "unpin batch returned false")
        if response is not True:
            raise AmbiguousFailure("unpin-retention response was neither #t nor #f")
        return {"outcome": "accepted", "handles": normalized}

    @staticmethod
    def _validate_canonical_path(value):
        if not isinstance(value, list) or len(value) < 4 or type(value[0]) is not int or not 0 <= value[0] <= MAX_INDEX:
            raise RequestError("invalid-request", "canonical committed path is malformed")
        state_positions = [index for index, item in enumerate(value) if item == "*state*"]
        if len(state_positions) != 1 or state_positions[0] < 1 or state_positions[0] + 2 >= len(value):
            raise RequestError("invalid-request", "canonical committed path lacks terminal state path")
        state = state_positions[0]
        prefix = value[:state]
        if len(prefix) % 2 != 1:
            raise RequestError("invalid-request", "canonical route/index shape is malformed")
        for index, item in enumerate(prefix):
            if index % 2 == 0:
                if type(item) is not int or not 0 <= item <= MAX_INDEX:
                    raise RequestError("invalid-request", "canonical history index must be nonnegative")
            elif not isinstance(item, str) or not ROUTE_COMPONENT.fullmatch(item):
                raise RequestError("invalid-request", "canonical route alias is invalid")
        validate_component(value[state + 1], "canonical owner")
        for item in value[state + 2:]:
            validate_component(item, "canonical path component")
        return value


def load_config(path: str) -> dict:
    try:
        value = json.loads(Path(path).read_bytes(), object_pairs_hook=json_no_duplicates,
                           parse_constant=reject_json_constant)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, RecursionError, ValueError) as error:
        raise RequestError("invalid-request", f"cannot load config: {error}") from error
    if not isinstance(value, dict) or value.get("version") != 1:
        raise RequestError("invalid-request", "configured agent format is invalid")
    return value


def load_secret(state_dir: str, config: dict | None = None) -> str:
    path = Path(config["credentialFile"]) if config and isinstance(config.get("credentialFile"), str) else Path(state_dir) / "ledger.interface-secret"
    try:
        info = path.stat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
            raise RequestError("unsupported-capability", "local Interface credential must be an owner-only regular file")
        value = path.read_text().strip()
    except OSError as error:
        raise RequestError("unsupported-capability", f"cannot load local Interface credential: {error}") from error
    if not value:
        raise RequestError("unsupported-capability", "local Interface credential is empty")
    return value


def endpoint(config: dict) -> str:
    if isinstance(config.get("endpoint"), str):
        return config["endpoint"]
    sync = config.get("sync") or {}
    if not isinstance(sync, dict):
        raise RequestError("invalid-request", "config sync field is invalid")
    return sync.get("localInterface") or f"http://127.0.0.1:{int(sync.get('port', 8192))}/interface"


def base_result(operation: str, request_digest: str | None):
    return {"contract": CONTRACT, "version": VERSION, "operation": operation,
            "tool": {"name": "journal-cli-source-ops", "version": "2.0.0", "sha256": sha256_file()},
            "requestSha256": request_digest}


def error_result(operation: str, request_digest: str | None, outcome: str, code: str, message: str):
    result = base_result(operation, request_digest)
    result.update({"outcome": outcome, "error": {"code": code, "message": message[:512]}})
    return result


def run_command(operation: str, request_path: str, config_path: str, state_dir: str, client=None):
    request_digest = None
    try:
        request, request_digest = read_json_request(request_path)
        config = load_config(config_path)
        if client is None:
            client = InterfaceClient(endpoint(config), load_secret(state_dir, config))
        ops = SourceOps(config, state_dir, client)
        method = getattr(ops, operation.replace("-", "_"))
        payload = method(request)
        result = base_result(operation, request_digest)
        result.update(payload)
        if len(canonical_json(result)) > MAX_RESPONSE_BYTES:
            raise AmbiguousFailure("bounded JSON result exceeded 4190208 bytes")
    except RequestError as error:
        result = error_result(operation, request_digest, "not-attempted", error.code, str(error))
    except ExplicitReject as error:
        result = error_result(operation, request_digest, "rejected", error.code, str(error))
    except AmbiguousFailure as error:
        result = error_result(operation, request_digest, "failed-or-ambiguous", "completion-unestablished", str(error))
    except (ValueError, TypeError, KeyError, IndexError) as error:
        result = error_result(operation, request_digest, "failed-or-ambiguous", "malformed-response", str(error))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", default=os.environ.get("PI_AGENT_CONFIG", DEFAULT_CONFIG))
    parser.add_argument("--state-dir", default=DEFAULT_STATE)
    sub = parser.add_subparsers(dest="operation", required=True)
    for name in ("get-current", "resolve-view", "create-value", "pin-view", "unpin-retention"):
        command = sub.add_parser(name)
        command.add_argument("request", help="versioned JSON request file, or - for stdin")
    args = parser.parse_args()
    result = run_command(args.operation, args.request, args.config, args.state_dir)
    sys.stdout.buffer.write(canonical_json(result))
    return 0 if result["outcome"] in {"retrieved", "accepted"} else {"not-attempted": 2, "rejected": 3}.get(result["outcome"], 4)


if __name__ == "__main__":
    raise SystemExit(main())
