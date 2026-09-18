from __future__ import annotations

import argparse
import base64
from contextlib import contextmanager
import fcntl
import hashlib
import json
import re
import os
from pathlib import Path
import tempfile
from typing import Any

from ..client import ExplicitReject, JournalClient, InvalidRequest, scheme_string, scheme_symbol
from ..config import load_config


def _auth(client: JournalClient) -> str:
    return f"(authentication ((identity (*state* {scheme_symbol(client.config.owner)})) (credentials {scheme_string(client.config.credential())})))"


def _admin_auth(client: JournalClient) -> str:
    return f"(authentication ((credentials {scheme_string(client.config.credential())})))"


def _route(value: str) -> list[str]:
    parts = value.split("/")
    if not parts or any(not item for item in parts):
        raise InvalidRequest("route must be slash-separated nonempty symbols")
    return [scheme_symbol(item) for item in parts]


def _post(client: JournalClient, expression: str, *, mutation: bool) -> str:
    result = client.post_scheme(expression, mutation=mutation)
    if mutation and result.strip() == "#f":
        raise ExplicitReject("peer mutation returned false")
    return result


def _emit(operation: str, outcome: str, result: Any = None, **details: Any) -> None:
    value = {"schema": "journal-cli-outcome-v1", "version": 1, "operation": operation, "outcome": outcome}
    if result is not None:
        value["result"] = result
    value.update(details)
    print(json.dumps(value, sort_keys=True, separators=(",", ":")))


def signing_key_digest(client: JournalClient) -> int:
    info = client.call_json("info")
    try:
        encoded = info["public-key"]["*type/byte-vector*"]
        public_key = bytes.fromhex(encoded)
    except (KeyError, TypeError, ValueError) as error:
        raise InvalidRequest("Journal info omitted a valid signing public key") from error
    if not public_key:
        raise InvalidRequest("Journal signing public key is empty")
    digest = hashlib.sha256(public_key).digest()
    _emit("peer.signing-key-digest", "retrieved", base64=base64.b64encode(digest).decode(), sha256=digest.hex())
    return 0


def route(client: JournalClient, target: str, explain_identity: str | None) -> int:
    parts = _route(target)
    if explain_identity:
        print(json.dumps({
            "route": parts, "effectivePrincipal": [*parts, "*state*", scheme_symbol(explain_identity)],
            "note": "route is directional provenance; verify the terminal owner separately",
        }, indent=2))
        return 0
    expression = f"((function route) (arguments ((route-target ({' '.join(parts)})))))"
    result = _post(client, expression, mutation=False)
    _emit("peer.route", "retrieved", result)
    return 0


def preapprove(client: JournalClient, peer: str, digest_text: str) -> int:
    try:
        digest = base64.b64decode(digest_text, validate=True)
    except ValueError as error:
        raise InvalidRequest("signing-key SHA-256 is not Base64") from error
    if len(digest) != 32:
        raise InvalidRequest("signing-key SHA-256 must be 32 bytes")
    vector = "#u(" + " ".join(str(item) for item in digest) + ")"
    auth = _admin_auth(client)
    first = _post(client, f"((function update-config!) (arguments ((path (public bridge-accept)) (value preapproved))) {auth})", mutation=True)
    try:
        second = _post(client, f"((function update-config!) (arguments ((path (private bridge-preapproval {scheme_symbol(peer)})) (value {vector}))) {auth})", mutation=True)
    except Exception as error:
        error.args = (f"bridge-accept update completed before preapproval stopped: {error}",)
        raise
    _emit("peer.preapprove", "accepted", writes=[first, second], partialEffects=False)
    return 0


def bridge(client: JournalClient, target: str, interface: str, remote_name: str) -> int:
    expression = (
        f"((function bridge!) (arguments ((name {scheme_symbol(target)})"
        f" (interface {scheme_string(interface)}) (remote-name {scheme_symbol(remote_name)}))) {_admin_auth(client)})"
    )
    result = _post(client, expression, mutation=True)
    _emit("peer.bridge", "accepted", result)
    return 0


def delete_bridge(client: JournalClient, target: str, expected_public_key_sha256: str, apply: bool) -> int:
    alias = scheme_symbol(target)
    if not re.fullmatch(r"[0-9a-f]{64}", expected_public_key_sha256):
        raise InvalidRequest("expected public-key SHA-256 must be lowercase hexadecimal")
    query = f"((function config) (arguments ((path (private bridge {alias} public-key)))) {_admin_auth(client)})"
    before = client.post_scheme(query)
    match = re.fullmatch(r"\s*#u\(([^)]*)\)\s*", before)
    if not match:
        raise InvalidRequest("active bridge did not expose one public key")
    try:
        public_key = bytes(int(item) for item in match.group(1).split())
    except ValueError as error:
        raise InvalidRequest("active bridge public key was malformed") from error
    observed = hashlib.sha256(public_key).hexdigest()
    if observed != expected_public_key_sha256:
        raise InvalidRequest("active bridge public-key SHA-256 differs")
    removed = False
    result = None
    if apply:
        expression = f"((function delete-bridge!) (arguments ((name {alias}))) {_admin_auth(client)})"
        result = client.post_scheme(expression, mutation=True).strip()
        if result != "#t":
            raise InvalidRequest("delete-bridge! did not return #t")
        if client.post_scheme(query).strip() != "()":
            raise InvalidRequest("bridge config remained active after deletion")
        removed = True
    print(json.dumps({
        "version": 1, "action": "apply" if apply else "dry-run", "alias": alias,
        "expectedPublicKeySha256": expected_public_key_sha256,
        "observedPublicKeySha256": observed, "activeBefore": True,
        "removedAfter": removed, "implicitPrincipalPrefixDeauthorization": [alias],
        "result": result, "credentialsExposed": False,
    }, indent=2, sort_keys=True))
    return 0


@contextmanager
def _config_lock(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    lock = path.with_name(f".{path.name}.lock")
    with lock.open("a+") as handle:
        os.chmod(lock, 0o600)
        fcntl.flock(handle, fcntl.LOCK_EX)
        yield


def recipient_route_replace(client: JournalClient, args: argparse.Namespace) -> int:
    path = client.config.inbox_config
    journal = scheme_symbol(args.journal)
    identity = scheme_symbol(args.identity)
    owner = scheme_symbol(args.owner)
    before_route = _route(args.from_route)
    after_route = _route(args.to_route)
    with _config_lock(path):
        try:
            config = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise InvalidRequest(f"invalid Message configuration: {error}") from error
        if config.get("version") != 1 or config.get("owner") != client.config.owner or config.get("localJournal") != client.config.journal:
            raise InvalidRequest("Message configuration identity differs")
        before_digest = hashlib.sha256(json.dumps(config, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        if before_digest != args.expect_config_digest:
            raise InvalidRequest(f"config digest mismatch: expected {args.expect_config_digest}, observed {before_digest}")
        matches = [(index, item) for index, item in enumerate(config.get("recipients", []))
                   if isinstance(item, dict) and (item.get("journal"), item.get("identity")) == (journal, identity)]
        if len(matches) != 1:
            raise InvalidRequest(f"expected one matching recipient, observed {len(matches)}")
        index, current = matches[0]
        expected = {"journal": journal, "identity": identity, "route": before_route, "owner": owner}
        if current != expected:
            raise InvalidRequest("matching recipient differs from exact expected source entry")
        replacement = {"journal": journal, "identity": identity, "route": after_route, "owner": owner}
        if replacement == expected:
            raise InvalidRequest("replacement would not change the config")
        config["recipients"][index] = replacement
        after_bytes = (json.dumps(config, indent=2, sort_keys=True) + "\n").encode()
        after_digest = hashlib.sha256(json.dumps(config, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        if args.apply:
            fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
            try:
                os.fchmod(fd, 0o600)
                with os.fdopen(fd, "wb") as handle:
                    handle.write(after_bytes); handle.flush(); os.fsync(handle.fileno())
                os.replace(temporary, path)
            finally:
                try: os.unlink(temporary)
                except FileNotFoundError: pass
            observed = hashlib.sha256(json.dumps(json.loads(path.read_text()), sort_keys=True, separators=(",", ":")).encode()).hexdigest()
            if observed != after_digest:
                raise InvalidRequest("atomic route replacement verification failed")
    print(json.dumps({
        "version": 1, "action": "apply" if args.apply else "dry-run", "changed": True,
        "trustChanged": False, "agent": client.config.owner, "localJournal": client.config.journal,
        "beforeConfigDigest": before_digest, "afterConfigDigest": after_digest,
        "recipientBefore": expected, "recipientAfter": replacement,
    }, indent=2, sort_keys=True))
    return 0


def authorize(client: JournalClient, args: argparse.Namespace) -> int:
    owner = scheme_symbol(args.owner or client.config.owner)
    route_parts = _route(args.remote_route)
    relative = [scheme_symbol(item) for item in args.path.split("/") if item]
    if not relative:
        raise InvalidRequest("authorization path must be nonempty")
    principal = [*route_parts, "*state*", scheme_symbol(args.remote_identity)]
    read_only = True
    use = f"((read-only? {'#t' if read_only else '#f'}))"
    rule = (
        "((principal (" + " ".join(principal) + ")) (key-index (0 -1))"
        f" (path ({' '.join(relative)})) (put! {'#f' if args.read_only else '#t'})"
        f" (use! {use}) (run! {'#t' if args.run else '#f'})"
        f" (retrieve {'(0 -1)' if args.retrieve else '#f'}))"
    )
    function = "deauthorize!" if args.revoke else "authorize!"
    expression = f"((function {function}) (arguments ((user (*state* {owner})) (rule {rule}))) {_auth(client)})"
    preview = {
        "action": "revoke" if args.revoke else "authorize", "owner": owner,
        "route": route_parts, "effectivePrincipal": principal, "path": relative,
        "capabilities": {"use!": {"read-only?": True}, "put!": not args.read_only,
                         "run!": args.run, "retrieve": [0, -1] if args.retrieve else False},
        "request": expression.replace(scheme_string(client.config.credential()), '"<redacted>"'),
    }
    if args.dry_run:
        print(json.dumps(preview, indent=2))
        return 0
    result = _post(client, expression, mutation=True)
    _emit("peer.authorize", "accepted", result, plan=preview)
    return 0


def authorizations(client: JournalClient, owner: str | None, digest: bool) -> int:
    selected = scheme_symbol(owner or client.config.owner)
    expression = f"((function authorizations) (arguments ((user (*state* {selected})))) {_auth(client)})"
    result = client.post_scheme(expression)
    if digest:
        data = result.strip().encode()
        _emit("peer.authorizations", "retrieved", owner=selected,
              sha256=hashlib.sha256(data).hexdigest(), encodedBytes=len(data))
    else:
        _emit("peer.authorizations", "retrieved", result, owner=selected)
    return 0
