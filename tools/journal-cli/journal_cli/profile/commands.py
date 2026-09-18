#!/usr/bin/python3
"""Owner-local Sync Agent Profile v2 validator, fetcher, and create-only publisher."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import time
import unicodedata
import urllib.error
import urllib.request

from ..client import _NoRedirect
from ..source.ops import InterfaceClient, Symbol as WireSymbol, alist, parse_sexp
from ..source.sync_source_model import canonical_endpoint

TRANSPORT_LIMIT = 32_768
PROFILE_LIMIT = 4_096
MAX_DEPTH = 8
MAX_LIST_NODES = 64
MAX_SYMBOL_BYTES = 128
SYMBOL = re.compile(r"^[A-Za-z0-9_.*+!<>=?/-]+$")
INTEGER = re.compile(r"^[+-]?[0-9]+$")
HEX = re.compile(r"^[0-9a-fA-F]+$")
DIGEST = re.compile(r"^[0-9a-f]{64}$")
SCHEMA = "sync-agent-profile-v2"
FSI = "\u2068"
PDI = "\u2069"


class ProfileError(ValueError):
    pass


class RemoteError(RuntimeError):
    pass


@dataclass(frozen=True)
class Sym:
    value: str


@dataclass(frozen=True)
class Profile:
    endpoint: str
    journal: str
    owner: str
    revision: int
    display_name: str
    pronouns: tuple[str, ...] | None
    bio: str


class RestrictedReader:
    def __init__(self, data: bytes):
        if len(data) > PROFILE_LIMIT:
            raise ProfileError(f"profile exceeds {PROFILE_LIMIT} raw bytes")
        try:
            self.text = data.decode("utf-8", errors="strict")
        except UnicodeDecodeError as error:
            raise ProfileError("profile is not valid UTF-8") from error
        self.position = 0
        self.nodes = 0
        self.byte_vectors = 0

    def parse(self):
        self._space()
        value = self._datum(0)
        self._space()
        if self.position != len(self.text):
            raise ProfileError("trailing profile data")
        return value

    def _space(self):
        while self.position < len(self.text) and self.text[self.position] in " \t\r\n":
            self.position += 1

    def _datum(self, depth: int):
        if self.position >= len(self.text):
            raise ProfileError("unexpected end of profile")
        char = self.text[self.position]
        if char == "(":
            return self._list(depth + 1)
        if char == '"':
            return self._string()
        if self.text.startswith("#u(", self.position):
            return self._byte_vector()
        if char in "'`,;#|":
            raise ProfileError("forbidden Scheme reader syntax")
        if char == ")":
            raise ProfileError("unexpected closing parenthesis")
        return self._symbol()

    def _list(self, depth: int):
        if depth > MAX_DEPTH:
            raise ProfileError(f"profile nesting exceeds {MAX_DEPTH}")
        self.position += 1
        values = []
        while True:
            self._space()
            if self.position >= len(self.text):
                raise ProfileError("unterminated list")
            if self.text[self.position] == ")":
                self.position += 1
                return values
            self.nodes += 1
            if self.nodes > MAX_LIST_NODES:
                raise ProfileError(f"profile list nodes exceed {MAX_LIST_NODES}")
            value = self._datum(depth)
            if isinstance(value, Sym) and value.value == ".":
                raise ProfileError("improper lists are forbidden")
            values.append(value)

    def _string(self):
        self.position += 1
        output = []
        escapes = {"a": "\a", "b": "\b", "t": "\t", "n": "\n", "v": "\v", "f": "\f", "r": "\r", '"': '"', "\\": "\\"}
        while self.position < len(self.text):
            char = self.text[self.position]
            self.position += 1
            if char == '"':
                return "".join(output)
            if char != "\\":
                output.append(char)
                continue
            if self.position >= len(self.text):
                raise ProfileError("unterminated string escape")
            escape = self.text[self.position]
            self.position += 1
            if escape in escapes:
                output.append(escapes[escape])
            elif escape == "x":
                end = self.text.find(";", self.position)
                if end < 0:
                    raise ProfileError("unterminated hexadecimal string escape")
                digits = self.text[self.position:end]
                if not digits or not HEX.fullmatch(digits):
                    raise ProfileError("invalid hexadecimal string escape")
                value = int(digits, 16)
                if value > 0x10FFFF or 0xD800 <= value <= 0xDFFF:
                    raise ProfileError("invalid Unicode scalar escape")
                output.append(chr(value))
                self.position = end + 1
            else:
                raise ProfileError("unsupported string escape")
        raise ProfileError("unterminated string")

    def _byte_vector(self):
        self.byte_vectors += 1
        if self.byte_vectors > 1:
            raise ProfileError("profile contains multiple byte vectors")
        self.position += 3
        values = []
        while True:
            self._space()
            if self.position >= len(self.text):
                raise ProfileError("unterminated byte vector")
            if self.text[self.position] == ")":
                self.position += 1
                return bytes(values)
            start = self.position
            while self.position < len(self.text) and self.text[self.position] not in " \t\r\n)":
                self.position += 1
            token = self.text[start:self.position]
            if not re.fullmatch(r"(?:0|[1-9][0-9]*)", token):
                raise ProfileError("byte vector contains a non-decimal byte")
            value = int(token)
            if value > 255:
                raise ProfileError("byte vector value exceeds 255")
            values.append(value)
            if len(values) > 32:
                raise ProfileError("identity byte vector exceeds 32 bytes")

    def _symbol(self):
        start = self.position
        while self.position < len(self.text) and self.text[self.position] not in " \t\r\n()\"';`,":
            self.position += 1
        token = self.text[start:self.position]
        if not token or token == "." or token.startswith("#"):
            raise ProfileError("forbidden or invalid symbol")
        if INTEGER.fullmatch(token):
            return int(token)
        if len(token.encode("utf-8")) > MAX_SYMBOL_BYTES:
            raise ProfileError(f"symbol exceeds {MAX_SYMBOL_BYTES} UTF-8 bytes")
        return Sym(token)


def parse_profile(data: bytes):
    return RestrictedReader(data).parse()


def validate_string(value, name: str, maximum: int, *, allow_lf: bool = False):
    if not isinstance(value, str):
        raise ProfileError(f"{name} must be a string")
    if len(value.encode("utf-8")) > maximum:
        raise ProfileError(f"{name} exceeds {maximum} UTF-8 bytes")
    for char in value:
        point = ord(char)
        bidi = unicodedata.bidirectional(char)
        forbidden_control = point == 0 or (point < 0x20 and not (allow_lf and point == 0x0A)) or 0x7F <= point <= 0x9F
        if forbidden_control or point in {0x2028, 0x2029, 0x061C, 0x200E, 0x200F} or 0x202A <= point <= 0x202E or 0x2066 <= point <= 0x2069 or bidi in {"LRE", "RLE", "LRO", "RLO", "PDF", "LRI", "RLI", "FSI", "PDI"}:
            raise ProfileError(f"{name} contains a forbidden control")
    return value


def validate_profile(value, expected_endpoint: str, journal: str, owner: str) -> Profile:
    if not isinstance(value, list):
        raise ProfileError("profile must be a proper association list")
    rows = {}
    allowed = {"schema", "endpoint", "journal", "owner", "revision", "display-name", "pronouns", "bio"}
    for row in value:
        if not isinstance(row, list) or len(row) != 2 or not isinstance(row[0], Sym):
            raise ProfileError("profile rows must be two-element symbol-keyed lists")
        key = row[0].value
        if key not in allowed:
            raise ProfileError(f"unknown profile key: {key}")
        if key in rows:
            raise ProfileError(f"duplicate profile key: {key}")
        rows[key] = row[1]
    required = {"schema", "endpoint", "journal", "owner", "revision", "display-name", "bio"}
    missing = sorted(required - rows.keys())
    if missing:
        raise ProfileError(f"missing profile keys: {', '.join(missing)}")
    if not isinstance(rows["schema"], Sym) or rows["schema"].value != SCHEMA:
        raise ProfileError("profile schema mismatch")
    if not isinstance(rows["endpoint"], str) or profile_endpoint(rows["endpoint"]) != profile_endpoint(expected_endpoint):
        raise ProfileError("profile endpoint mismatch")
    if rows["endpoint"] != profile_endpoint(expected_endpoint):
        raise ProfileError("profile endpoint is not canonical")
    if not isinstance(rows["journal"], Sym) or rows["journal"].value != journal:
        raise ProfileError("profile Journal mismatch")
    if not isinstance(rows["owner"], Sym) or rows["owner"].value != owner:
        raise ProfileError("profile owner mismatch")
    revision = rows["revision"]
    if not isinstance(revision, int) or isinstance(revision, bool) or revision <= 0:
        raise ProfileError("profile revision must be a positive integer")
    display_name = validate_string(rows["display-name"], "display-name", 128)
    bio = validate_string(rows["bio"], "bio", 2048, allow_lf=True)
    if not bio.strip():
        raise ProfileError("bio must contain non-whitespace text")
    pronouns = None
    if "pronouns" in rows:
        raw = rows["pronouns"]
        if not isinstance(raw, list) or not 1 <= len(raw) <= 4:
            raise ProfileError("pronouns must contain one through four strings")
        pronouns = tuple(validate_string(item, "pronoun", 64) for item in raw)
    return Profile(rows["endpoint"], journal, owner, revision, display_name, pronouns, bio)


def scheme(value) -> str:
    if isinstance(value, Sym):
        if not value.value or any(char in " \t\r\n()\"';`," for char in value.value):
            raise ProfileError("symbol cannot be emitted safely")
        return value.value
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, int) and not isinstance(value, bool):
        return str(value)
    if isinstance(value, bytes):
        return "#u(" + " ".join(str(byte) for byte in value) + ")"
    if isinstance(value, list):
        return "(" + " ".join(scheme(item) for item in value) + ")"
    raise ProfileError("unsupported profile value")


def symbol(value: str, label: str) -> str:
    if not SYMBOL.fullmatch(value) or value in {".", ".."}:
        raise ProfileError(f"invalid {label}: {value!r}")
    return value


def profile_endpoint(value: str) -> str:
    try:
        return canonical_endpoint(value)
    except Exception as error:
        raise ProfileError(f"invalid publisher endpoint: {error}") from error


def transport_value(text: str):
    stripped = text.strip(" \t\r\n")
    if stripped == "(nothing)":
        return None
    match = re.fullmatch(r"#u\(([^)]*)\)", stripped)
    if not match:
        if stripped.startswith("(error") or "authorization-error" in stripped:
            raise RemoteError(stripped[:1000])
        if stripped == "(unknown)":
            raise RemoteError("profile is unavailable at this index")
        raise RemoteError("Interface response is not a profile byte vector or (nothing)")
    values = []
    for token in match.group(1).split():
        if not re.fullmatch(r"(?:0|[1-9][0-9]*)", token):
            raise RemoteError("Interface byte vector contains an invalid token")
        value = int(token)
        if value > 255:
            raise RemoteError("Interface byte vector contains a value over 255")
        values.append(value)
        if len(values) > PROFILE_LIMIT:
            raise RemoteError(f"profile exceeds {PROFILE_LIMIT} raw bytes")
    return bytes(values)


def read_config(path: str):
    config = json.loads(Path(path).read_text())
    if config.get("version") != 1:
        raise ProfileError("invalid fixed-agent config")
    if not isinstance(config.get("id"), str) and isinstance(config.get("owner"), str):
        config["id"] = config["owner"]
    if not isinstance(config.get("id"), str):
        raise ProfileError("invalid fixed-agent config")
    sync = config.get("sync") or {}
    endpoint = config.get("endpoint") or sync.get("localInterface") or f"http://127.0.0.1:{int(sync.get('port', 8192))}/interface"
    return config, endpoint


def read_secret(state_dir: str, config: dict | None = None) -> str:
    path = Path(config["credentialFile"]) if config and isinstance(config.get("credentialFile"), str) else Path(state_dir) / "ledger.interface-secret"
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise ProfileError("Interface credential must be an owner-only regular file")
    value = path.read_text().strip()
    if not value:
        raise ProfileError("Interface credential file is empty")
    return value


_OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())


def post_bounded(endpoint: str, expression: str, timeout: float = 30.0) -> str:
    request = urllib.request.Request(endpoint, data=expression.encode("utf-8"), headers={"Content-Type": "application/scheme"}, method="POST")
    try:
        with _OPENER.open(request, timeout=timeout) as response:
            chunks = []
            remaining = TRANSPORT_LIMIT + 1
            while remaining:
                chunk = response.read(remaining)
                if not chunk:
                    break
                chunks.append(chunk)
                remaining -= len(chunk)
            data = b"".join(chunks)
            if len(data) > TRANSPORT_LIMIT:
                raise RemoteError(f"Interface response exceeds {TRANSPORT_LIMIT} bytes")
            return data.decode("utf-8", errors="strict")
    except urllib.error.HTTPError as error:
        data = error.read(16_385)
        detail = "error body too large" if len(data) > 16_384 else data.decode("utf-8", errors="replace")
        raise RemoteError(f"Interface HTTP {error.code}: {detail[:1000]}") from error


def observe_public_endpoint(endpoint: str) -> str:
    expected = profile_endpoint(endpoint)
    try:
        info = alist(InterfaceClient(expected, "").post("((function info))"), "public info response")
        interface = alist(info.get("interface"), "public interface descriptor")
    except Exception as error:
        raise RemoteError(f"publisher endpoint probe failed: {error}") from error
    if interface.get("endpoint") != expected:
        raise RemoteError("publisher endpoint did not self-identify exactly")
    return expected


def authentication(secret: str, identity: str) -> str:
    return f"(authentication ((identity (*state* {symbol(identity, 'identity')})) (credentials {json.dumps(secret)})))"


def local_get(endpoint: str, secret: str, identity: str, owner: str, index: int | None = None):
    tail = f"*state* {symbol(owner, 'owner')} profile.scm"
    if index is not None:
        if index < 0:
            raise ProfileError("commit evidence index must be nonnegative")
        path = f"({index} {tail})"
        function = "retrieve"
        arguments = f"((path {path}) (pinned? #f) (proof? #f))"
    else:
        path = f"({tail})"
        function = "use!"
        arguments = f"((path {path}) (read-only? #t) (expression? #f))"
    request = f"((function {function}) (arguments {arguments}) {authentication(secret, identity)})"
    return transport_value(post_bounded(endpoint, request))


def federated_get(endpoint: str, secret: str, identity: str, route: str, owner: str):
    route_parts = [symbol(part, "route component") for part in route.split("/") if part]
    if not route_parts or "/".join(route_parts) != route:
        raise ProfileError("route must be slash-separated nonempty symbols")
    request = (
        "((function use!)"
        f" (arguments ((path (*state* {symbol(owner, 'owner')} profile.scm)) (read-only? #t) (expression? #f)))"
        f" (invocation ((identity {symbol(identity, 'identity')}) (route-source ())"
        f" (route-target ({' '.join(route_parts)})) (credentials {json.dumps(secret)}))))"
    )
    return transport_value(post_bounded(endpoint, request))


def committed_get(endpoint: str, secret: str, identity: str, owner: str):
    request = (
        "((function retrieve)"
        f" (arguments ((path (-1 *state* {symbol(owner, 'owner')} profile.scm))"
        " (pinned? #f) (proof? #f) (index? #t)))"
        f" {authentication(secret, identity)})"
    )
    try:
        result = alist(parse_sexp(post_bounded(endpoint, request)), "indexed profile response")
        indexes = result.get("indexes")
        if not isinstance(indexes, list) or len(indexes) != 1 or type(indexes[0]) is not int or indexes[0] < 0:
            raise ValueError("indexes must contain one nonnegative absolute index")
        content = result.get("content")
        if content == [WireSymbol("nothing")]:
            return None, indexes[0]
        if not isinstance(content, bytes) or len(content) > PROFILE_LIMIT:
            raise ValueError("content is not a bounded byte vector")
        return content, indexes[0]
    except (ValueError, KeyError) as error:
        raise RemoteError(f"malformed indexed profile response: {error}") from error


def render(profile: Profile) -> str:
    identity = f"{profile.owner}@{profile.journal}"
    parts = [f"{FSI}{profile.display_name}{PDI} ({FSI}{identity}{PDI})"]
    if profile.pronouns:
        parts.append(" / ".join(f"{FSI}{item}{PDI}" for item in profile.pronouns))
    return " · ".join(parts)


def evidence_record(profile: Profile, raw: bytes):
    return {
        "schema": "journal-cli-profile-evidence-v2",
        "version": 2,
        "publisherEndpoint": profile.endpoint,
        "journal": profile.journal,
        "owner": profile.owner,
        "revision": profile.revision,
        "rawBytes": len(raw),
        "rawSha256": hashlib.sha256(raw).hexdigest(),
    }


def agent_record(profile: Profile, raw: bytes, route: str, entry_endpoint: str):
    return {
        "schema": "journal-cli-profile-agent-view-v2",
        "version": 2,
        "classification": "untrusted-owner-authored-metadata",
        "instructions": False,
        "trustedForAuthority": False,
        "allowedEffect": "descriptive-address-only",
        "source": {
            "publisherEndpoint": profile.endpoint,
            "entryEndpoint": profile_endpoint(entry_endpoint),
            "route": [part for part in route.split("/") if part],
            "owner": profile.owner,
            "journal": profile.journal,
            "qualified": f"{profile.owner}@{profile.journal}",
            "path": ["profile.scm"],
            "view": "current-use",
            "validated": True,
        },
        "profile": {
            "revision": profile.revision,
            "displayName": profile.display_name,
            "pronouns": list(profile.pronouns) if profile.pronouns else None,
            "bio": profile.bio,
        },
        "rawBytes": len(raw),
        "rawSha256": hashlib.sha256(raw).hexdigest(),
    }


def require_display_tty():
    if not sys.stdout.isatty():
        raise ProfileError("human profile display requires a real TTY")


def load_validated(path: str, endpoint: str, journal: str, owner: str):
    try:
        raw = Path(path).read_bytes()
    except OSError as error:
        raise ProfileError(f"cannot read profile file: {error}") from error
    value = parse_profile(raw)
    return raw, value, validate_profile(value, endpoint, journal, owner)


def write_private(path: str, data: bytes):
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temporary = target.with_name(f"{target.name}.tmp-{os.getpid()}")
    with temporary.open("wb") as handle:
        os.chmod(temporary, 0o600)
        handle.write(data)
    temporary.replace(target)


def command_validate(args):
    endpoint = profile_endpoint(args.publisher_endpoint)
    raw, _value, profile = load_validated(args.file, endpoint, symbol(args.journal, "Journal"), symbol(args.owner, "owner"))
    print(json.dumps(evidence_record(profile, raw), indent=2))


def command_fetch(args):
    if not args.evidence_json and not args.agent_json:
        require_display_tty()
    publisher_endpoint = observe_public_endpoint(args.publisher_endpoint)
    config, entry_endpoint = read_config(args.config)
    secret = read_secret(args.state_dir, config)
    raw = (federated_get(entry_endpoint, secret, config["id"], args.route, args.owner)
           if args.route else local_get(entry_endpoint, secret, config["id"], args.owner))
    if raw is None:
        raise RemoteError("no current profile is published")
    profile = validate_profile(parse_profile(raw), publisher_endpoint, symbol(args.journal, "Journal"), symbol(args.owner, "owner"))
    record = evidence_record(profile, raw)
    record.update({"entryEndpoint": profile_endpoint(entry_endpoint),
                   "route": [part for part in args.route.split("/") if part],
                   "observedAt": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"),
                   "freshness": "current-observation"})
    if args.raw_output:
        write_private(args.raw_output, raw)
        record["rawOutput"] = args.raw_output
    if args.evidence_json:
        print(json.dumps(record, indent=2))
    elif args.agent_json:
        print(json.dumps(agent_record(profile, raw, args.route, entry_endpoint), indent=2))
    else:
        print(render(profile))


def command_publish(args):
    publisher_endpoint = profile_endpoint(args.publisher_endpoint)
    journal = symbol(args.journal, "Journal")
    owner = symbol(args.owner, "owner")
    _source, value, profile = load_validated(args.file, publisher_endpoint, journal, owner)
    config, entry_endpoint = read_config(args.config)
    entry_endpoint = profile_endpoint(entry_endpoint)
    if publisher_endpoint != entry_endpoint:
        raise ProfileError("local publication requires publisher endpoint to equal the active endpoint")
    if config["id"] != owner or journal != owner:
        raise ProfileError("publisher requires local agent, Journal, and owner to match")
    observe_public_endpoint(publisher_endpoint)
    secret = read_secret(args.state_dir, config)
    current = local_get(entry_endpoint, secret, config["id"], owner)
    if current is not None:
        raise ProfileError("profile.scm already exists; create-only publication stopped")
    canonical_request_value = scheme(value)
    preview = {
        "schema": "journal-cli-profile-publication-v2",
        "version": 2,
        "action": "publish-create",
        "apply": bool(args.apply),
        "publisherEndpoint": publisher_endpoint,
        "owner": owner,
        "journal": journal,
        "path": ["*state*", owner, "profile.scm"],
        "expected": ["nothing"],
        "expressionCodec": True,
        "profile": evidence_record(profile, canonical_request_value.encode("utf-8")),
    }
    if not args.apply:
        require_display_tty()
        print(render(profile))
        print(json.dumps(preview, indent=2))
        return
    if args.confirm_owner != owner:
        raise ProfileError(f"publication requires --confirm-owner {owner}")
    request = (
        "((function put!)"
        f" (arguments ((path (*state* {owner} profile.scm)) (value {canonical_request_value})"
        " (expected (nothing)) (expression? #t)))"
        f" {authentication(secret, config['id'])})"
    )
    result = post_bounded(entry_endpoint, request).strip()
    if result != "#t":
        raise RemoteError(f"create-only publication rejected: {result[:1000]}")
    stored = local_get(entry_endpoint, secret, config["id"], owner)
    if stored is None:
        raise RemoteError("profile remained absent after successful staged publication")
    stored_profile = validate_profile(parse_profile(stored), publisher_endpoint, journal, owner)
    record = evidence_record(stored_profile, stored)
    if args.raw_output:
        write_private(args.raw_output, stored)
        record["rawOutput"] = args.raw_output
    record.update({"action": "publish-create", "outcome": "accepted", "result": "#t", "staged": True, "committed": False})
    print(json.dumps(record, indent=2, ensure_ascii=False))


def command_wait_commit(args):
    if not DIGEST.fullmatch(args.expected_sha256):
        raise ProfileError("expected SHA-256 must be 64 lowercase hexadecimal characters")
    publisher_endpoint = profile_endpoint(args.publisher_endpoint)
    journal = symbol(args.journal, "Journal")
    owner = symbol(args.owner, "owner")
    config, entry_endpoint = read_config(args.config)
    entry_endpoint = profile_endpoint(entry_endpoint)
    if publisher_endpoint != entry_endpoint or config["id"] != owner or journal != owner:
        raise ProfileError("commit evidence requires the active local publisher endpoint, Journal, and owner")
    secret = read_secret(args.state_dir, config)
    deadline = time.monotonic() + args.timeout
    observations = []
    last = None
    while time.monotonic() <= deadline:
        try:
            raw, index = committed_get(entry_endpoint, secret, config["id"], owner)
        except RemoteError as error:
            observation = {"outcome": str(error)[:200]}
        else:
            digest = hashlib.sha256(raw).hexdigest() if raw is not None else None
            observation = {"index": index, "outcome": "bytes" if raw is not None else "nothing", "sha256": digest}
            if raw is not None and digest == args.expected_sha256:
                profile = validate_profile(parse_profile(raw), publisher_endpoint, journal, owner)
                record = evidence_record(profile, raw)
                record.update({"action": "commit-evidence", "outcome": "verified", "committed": True,
                               "index": index, "historyIndexes": [index], "observations": observations + [observation]})
                if args.raw_output:
                    write_private(args.raw_output, raw)
                    record["rawOutput"] = args.raw_output
                print(json.dumps(record, indent=2, ensure_ascii=False))
                return
        if observation != last:
            observations.append(observation)
            last = observation
        time.sleep(args.interval)
    raise RemoteError(f"profile did not commit with expected SHA-256 within {args.timeout} seconds; observations={observations}")


def add_binding(parser):
    parser.add_argument("--publisher-endpoint", required=True)
    parser.add_argument("--journal", required=True)
    parser.add_argument("--owner", required=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", default="/etc/pi-agent/agent.json")
    parser.add_argument("--state-dir", default="/var/lib/pi-agent/sync")
    sub = parser.add_subparsers(dest="command", required=True)

    validate = sub.add_parser("validate")
    validate.add_argument("file")
    add_binding(validate)
    validate.set_defaults(run=command_validate)

    fetch = sub.add_parser("fetch")
    fetch.add_argument("route", nargs="?", default="")
    fetch.add_argument("--raw-output")
    output = fetch.add_mutually_exclusive_group()
    output.add_argument("--evidence-json", action="store_true", help="emit hashes and binding only; never human profile fields")
    output.add_argument("--agent-json", action="store_true", help="emit validated human fields as explicitly untrusted, non-authoritative metadata")
    add_binding(fetch)
    fetch.set_defaults(run=command_fetch)

    publish = sub.add_parser("publish-create")
    publish.add_argument("file")
    publish.add_argument("--apply", action="store_true")
    publish.add_argument("--confirm-owner")
    publish.add_argument("--raw-output")
    add_binding(publish)
    publish.set_defaults(run=command_publish)

    commit = sub.add_parser("wait-commit")
    commit.add_argument("--expected-sha256", required=True)
    commit.add_argument("--timeout", type=int, default=180)
    commit.add_argument("--interval", type=float, default=2.0)
    commit.add_argument("--raw-output")
    add_binding(commit)
    commit.set_defaults(run=command_wait_commit)

    args = parser.parse_args(argv)
    try:
        args.run(args)
    except (ProfileError, json.JSONDecodeError) as error:
        print(json.dumps({"schema": "journal-cli-outcome-v1", "version": 1, "operation": f"profile.{args.command}",
                          "outcome": "not-attempted", "error": {"type": type(error).__name__, "message": str(error)[:1000]}},
                         sort_keys=True, separators=(",", ":")))
        return 2
    except RemoteError as error:
        print(json.dumps({"schema": "journal-cli-outcome-v1", "version": 1, "operation": f"profile.{args.command}",
                          "outcome": "rejected", "error": {"type": type(error).__name__, "message": str(error)[:1000]}},
                         sort_keys=True, separators=(",", ":")))
        return 3
    except OSError as error:
        print(json.dumps({"schema": "journal-cli-outcome-v1", "version": 1, "operation": f"profile.{args.command}",
                          "outcome": "failed-or-ambiguous", "error": {"type": type(error).__name__, "message": str(error)[:1000]}},
                         sort_keys=True, separators=(",", ":")))
        return 4
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
