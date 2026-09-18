from __future__ import annotations

import base64
import json
import re
import socket
import ssl
from typing import Any
import urllib.error
import urllib.request

from .config import Config


MAX_REQUEST_BYTES = 4 * 1024 * 1024
MAX_RESPONSE_BYTES = 4 * 1024 * 1024
MAX_ERROR_BYTES = 16 * 1024
SYMBOL = re.compile(r"^[A-Za-z0-9_.*+!<>=?/-]+$")
LEGACY_OPERATIONS = {"get", "get-batch", "set", "set!", "set-batch!", "call!", "resolve", "resolve-batch"}
APPLICATION_OPERATIONS = {
    "use!", "put!", "copy!", "run!", "retrieve",
    "use-batch!", "put-batch!", "copy-batch!", "retrieve-batch",
}
CONTROL_OPERATIONS = {
    "info", "size", "route", "bridge!", "delete-bridge!",
    "authorize!", "deauthorize!", "authorizations", "pin!", "pin-batch!",
    "unpin!", "unpin-batch!", "prune!", "prune-batch!", "trace", "trace-batch",
    "synchronize!", "config", "update-config!", "admins", "administrators!",
}
ALLOWED_OPERATIONS = APPLICATION_OPERATIONS | CONTROL_OPERATIONS
WIRE_OPERATIONS = {"admins": "*admins-get*", "administrators!": "*admins-set*"}
LOCAL_ADMIN_OPERATIONS = {
    "bridge!", "delete-bridge!", "config", "update-config!", "admins", "administrators!",
}


def operation_may_mutate(function: str, arguments: dict[str, Any] | None = None) -> bool:
    if function in {"put!", "copy!", "run!", "put-batch!", "copy-batch!", "bridge!", "delete-bridge!",
                    "update-config!", "authorize!", "deauthorize!", "pin!", "pin-batch!", "unpin!",
                    "unpin-batch!", "prune!", "prune-batch!", "synchronize!", "administrators!"}:
        return True
    if function in {"use!", "use-batch!"}:
        return not isinstance(arguments, dict) or arguments.get("read-only?") is not True
    return False


class JournalError(RuntimeError):
    outcome = "not-attempted"


class ExplicitReject(JournalError):
    outcome = "rejected"


class UnconfirmedMutation(JournalError):
    outcome = "failed-or-ambiguous"


class InvalidRequest(JournalError):
    pass


def scheme_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def scheme_symbol(value: str) -> str:
    if not isinstance(value, str) or not SYMBOL.fullmatch(value) or value in {".", ".."}:
        raise InvalidRequest(f"invalid Scheme symbol: {value!r}")
    return value


def scheme_value(value: Any) -> str:
    if value is True:
        return "#t"
    if value is False:
        return "#f"
    if value is None:
        return "(nothing)"
    if isinstance(value, int) and not isinstance(value, bool):
        return str(value)
    if isinstance(value, bytes):
        return "#u(" + " ".join(str(item) for item in value) + ")"
    if isinstance(value, str):
        return scheme_string(value)
    if isinstance(value, list):
        return "(" + " ".join(scheme_value(item) for item in value) + ")"
    if isinstance(value, dict):
        return "(" + " ".join(f"({scheme_symbol(key)} {scheme_value(item)})" for key, item in value.items()) + ")"
    raise InvalidRequest(f"unsupported Scheme value: {type(value).__name__}")


RESOURCE_PATH_FIELDS = {
    "put!": {"path"},
    "use!": {"path"},
    "copy!": {"source", "path"},
    "run!": {"path"},
    "retrieve": {"path"},
    "put-batch!": {"paths"},
    "use-batch!": {"paths"},
    "copy-batch!": {"sources", "paths"},
    "retrieve-batch": {"paths"},
}
RESOURCE_PATH_LIST_FIELDS = {"sources", "paths"}


def scheme_resource_path(value: Any) -> str:
    if not isinstance(value, list):
        return scheme_value(value)
    return "(" + " ".join(
        scheme_symbol(item) if isinstance(item, str) else scheme_value(item)
        for item in value
    ) + ")"


def scheme_arguments(function: str, arguments: dict[str, Any]) -> str:
    path_fields = RESOURCE_PATH_FIELDS.get(function, set())
    fields = []
    for key, value in arguments.items():
        if key not in path_fields:
            encoded = scheme_value(value)
        elif key in RESOURCE_PATH_LIST_FIELDS and isinstance(value, list):
            encoded = "(" + " ".join(scheme_resource_path(item) for item in value) + ")"
        else:
            encoded = scheme_resource_path(value)
        fields.append(f"({scheme_symbol(key)} {encoded})")
    return "(" + " ".join(fields) + ")"


def request_expression(function: str, arguments: dict[str, Any] | None, config: Config,
                       *, identity: str | None = None, route: list[str] | None = None) -> str:
    if function in LEGACY_OPERATIONS:
        raise InvalidRequest(f"legacy Sync Web operation is not supported: {function}")
    if function not in ALLOWED_OPERATIONS:
        raise InvalidRequest(f"unsupported Sync Web 1.6 operation: {function}")
    fields = [f"(function {scheme_symbol(WIRE_OPERATIONS.get(function, function))})"]
    if arguments is not None:
        fields.append(f"(arguments {scheme_arguments(function, arguments)})")
    credential = scheme_string(config.credential())
    if route:
        target = "(" + " ".join(scheme_symbol(item) for item in route) + ")"
        origin = scheme_symbol(identity or config.owner)
        fields.append(f"(invocation ((identity {origin}) (route-source ()) (route-target {target}) (credentials {credential})))")
    elif function not in {"info", "size", "route"}:
        if function in LOCAL_ADMIN_OPERATIONS:
            fields.append(f"(authentication ((credentials {credential})))")
        else:
            principal = identity or config.owner
            fields.append(f"(authentication ((identity (*state* {scheme_symbol(principal)})) (credentials {credential})))")
    return "(" + " ".join(fields) + ")"


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(req.full_url, code, "redirect refused", headers, fp)


class JournalClient:
    def __init__(self, config: Config):
        self.config = config
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())

    def _safe_error(self, value: object) -> str:
        return str(value).replace(self.config.credential(), "<redacted>")[:1000]

    def _post(self, body: bytes, content_type: str, *, mutation: bool,
              response_limit: int) -> bytes:
        if len(body) > MAX_REQUEST_BYTES:
            raise InvalidRequest("request exceeds 4 MiB")
        request = urllib.request.Request(
            self.config.endpoint, data=body, headers={"Content-Type": content_type}, method="POST",
        )
        try:
            with self.opener.open(request, timeout=self.config.timeout_seconds) as response:
                raw = response.read(response_limit + 1)
        except urllib.error.HTTPError as error:
            data = error.read(MAX_ERROR_BYTES + 1)
            if error.code == 400 and len(data) <= MAX_ERROR_BYTES:
                raise ExplicitReject("Interface explicitly rejected request") from error
            raise UnconfirmedMutation(f"HTTP {error.code} did not establish completion") from error
        except (urllib.error.URLError, OSError, socket.timeout, ssl.SSLError) as error:
            kind = UnconfirmedMutation if mutation else JournalError
            raise kind(f"transport did not establish completion: {error}") from error
        if len(raw) > response_limit:
            kind = UnconfirmedMutation if mutation else JournalError
            raise kind("response exceeded configured bound")
        return raw

    def post_scheme(self, expression: str, *, mutation: bool = False,
                    response_limit: int = MAX_RESPONSE_BYTES) -> str:
        raw = self._post(expression.encode("utf-8"), "application/scheme", mutation=mutation,
                         response_limit=response_limit)
        try:
            text = raw.decode("utf-8", "strict")
        except UnicodeDecodeError as error:
            kind = UnconfirmedMutation if mutation else JournalError
            raise kind("response was not UTF-8") from error
        if text.lstrip().startswith("(error"):
            raise ExplicitReject(self._safe_error(text))
        return text

    def post_json(self, value: dict[str, Any], *, mutation: bool = False,
                  response_limit: int = MAX_RESPONSE_BYTES) -> Any:
        body = json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8")
        raw = self._post(body, "application/json", mutation=mutation, response_limit=response_limit)
        try:
            decoded = json.loads(raw)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            kind = UnconfirmedMutation if mutation else JournalError
            raise kind("JSON response framing failed") from error
        if isinstance(decoded, dict) and "error" in decoded:
            raise ExplicitReject(self._safe_error(decoded["error"]))
        if isinstance(decoded, list) and decoded and decoded[0] == "error":
            detail = decoded[2] if len(decoded) > 2 else "Interface explicitly rejected request"
            raise ExplicitReject(self._safe_error(detail))
        return decoded

    def call_json(self, function: str, arguments: dict[str, Any] | None = None, *,
                  identity: str | None = None) -> Any:
        if function in LEGACY_OPERATIONS:
            raise InvalidRequest(f"legacy Sync Web operation is not supported: {function}")
        if function not in ALLOWED_OPERATIONS:
            raise InvalidRequest(f"unsupported Sync Web 1.6 operation: {function}")
        request: dict[str, Any] = {"function": WIRE_OPERATIONS.get(function, function)}
        if arguments is not None:
            request["arguments"] = arguments
        if function not in {"info", "size", "route"}:
            request["authentication"] = {
                "credentials": {"*type/string*": self.config.credential()},
            }
            if function not in LOCAL_ADMIN_OPERATIONS:
                request["authentication"]["identity"] = ["*state*", identity or self.config.owner]
        return self.post_json(request, mutation=operation_may_mutate(function, arguments))

    def call(self, function: str, arguments: dict[str, Any] | None = None, *,
             identity: str | None = None, route: list[str] | None = None) -> str:
        mutation = operation_may_mutate(function, arguments)
        expression = request_expression(function, arguments, self.config, identity=identity, route=route)
        return self.post_scheme(expression, mutation=mutation)

    def raw(self, expression: bytes) -> str:
        try:
            text = expression.decode("utf-8", "strict")
        except UnicodeDecodeError as error:
            raise InvalidRequest("raw Scheme must be UTF-8") from error
        return self.post_scheme(text, mutation=True)


def json_outcome(operation: str, outcome: str, **fields: Any) -> str:
    value = {"schema": "journal-cli-outcome-v1", "version": 1, "operation": operation, "outcome": outcome, **fields}
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"


def decode_body_argument(value: str) -> bytes:
    if value.startswith("base64:"):
        try:
            return base64.b64decode(value[7:], validate=True)
        except ValueError as error:
            raise InvalidRequest("invalid Base64 body") from error
    return value.encode("utf-8")
