#!/usr/bin/env python3
"""Validate small nonsecret Sync Web agent capability cards."""

from __future__ import annotations

import argparse
from datetime import datetime
import json
from pathlib import Path
import re
import sys
from typing import Any


MAX_BYTES = 8192
SYMBOL = re.compile(r"^[A-Za-z0-9_.*+!<>=?-]+$")
DIGEST = re.compile(r"^[0-9a-f]{64}$")
PACKAGE = re.compile(r"^[A-Za-z0-9_.+~:^-]+$")
RFC3339 = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$")
PROHIBITED_KEY = re.compile(r"(credential|secret|password|token|private.?key|session|prompt|host.?path|endpoint)", re.I)
ROOT_KEYS = {"schemaVersion", "publisher", "mailbox", "platform", "features", "updatedAt", "previousCardDigest"}
RECEIPT_STATES = {
    "write-accepted", "commit-observed", "envelope-observed",
    "prompt-delivered", "correlated-reply", "rejected",
}


def secret_like_keys(value: Any, path: str = "$") -> list[str]:
    found: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            child_path = f"{path}.{key}"
            if PROHIBITED_KEY.search(str(key)):
                found.append(child_path)
            found.extend(secret_like_keys(child, child_path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            found.extend(secret_like_keys(child, f"{path}[{index}]"))
    return found


def exact_object(value: Any, required: set[str], allowed: set[str], label: str, errors: list[str]) -> dict[str, Any]:
    if not isinstance(value, dict):
        errors.append(f"{label} must be an object")
        return {}
    missing = required - set(value)
    unknown = set(value) - allowed
    if missing:
        errors.append(f"{label} is missing: {', '.join(sorted(missing))}")
    if unknown:
        errors.append(f"{label} has unknown fields: {', '.join(sorted(unknown))}")
    return value


def validate(card: Any, encoded_size: int, *, expected_identity: str | None = None,
             expected_journal: str | None = None, expected_owner: str | None = None) -> list[str]:
    errors: list[str] = []
    if encoded_size > MAX_BYTES:
        errors.append(f"card exceeds {MAX_BYTES} bytes")
    root = exact_object(card, {"schemaVersion", "publisher", "mailbox", "platform", "features", "updatedAt"}, ROOT_KEYS, "card", errors)
    if root.get("schemaVersion") != 1:
        errors.append("schemaVersion must be 1")

    publisher = exact_object(root.get("publisher"), {"identity", "journal", "owner"}, {"identity", "journal", "owner"}, "publisher", errors)
    for field in ("identity", "journal", "owner"):
        if not isinstance(publisher.get(field), str) or not SYMBOL.fullmatch(publisher[field]):
            errors.append(f"publisher.{field} must be a Sync symbol")
    for field, expected in (("identity", expected_identity), ("journal", expected_journal), ("owner", expected_owner)):
        if expected is not None and publisher.get(field) != expected:
            errors.append(f"publisher.{field} does not match expected {expected!r}")

    mailbox = exact_object(root.get("mailbox"), {"protocolVersion", "maxMessageBytes"}, {"protocolVersion", "maxMessageBytes"}, "mailbox", errors)
    if mailbox.get("protocolVersion") != 1:
        errors.append("mailbox.protocolVersion must be 1")
    maximum = mailbox.get("maxMessageBytes")
    if not isinstance(maximum, int) or isinstance(maximum, bool) or not 1 <= maximum <= 524288:
        errors.append("mailbox.maxMessageBytes must be an integer from 1 through 524288")

    platform = exact_object(root.get("platform"), {"syncPackage", "inboxPackage"}, {"syncPackage", "inboxPackage"}, "platform", errors)
    for field in ("syncPackage", "inboxPackage"):
        if (not isinstance(platform.get(field), str) or not 1 <= len(platform[field]) <= 160
                or not PACKAGE.fullmatch(platform[field])):
            errors.append(f"platform.{field} must be a bounded package identity without paths, URLs, or free text")

    features = exact_object(root.get("features"), {"hotConfigReload", "structuredDoctor", "federatedResolve", "receiptStates"}, {"hotConfigReload", "structuredDoctor", "federatedResolve", "receiptStates"}, "features", errors)
    for field in ("hotConfigReload", "structuredDoctor", "federatedResolve"):
        if not isinstance(features.get(field), bool):
            errors.append(f"features.{field} must be boolean")
    states = features.get("receiptStates")
    if (not isinstance(states, list) or len(states) > 16
            or any(not isinstance(state, str) or state not in RECEIPT_STATES for state in states)
            or len(states) != len(set(states))):
        errors.append("features.receiptStates must be a unique list of known states")

    updated = root.get("updatedAt")
    try:
        if not isinstance(updated, str) or len(updated) > 40 or not RFC3339.fullmatch(updated):
            raise ValueError
        parsed = datetime.fromisoformat(updated.replace("Z", "+00:00"))
        if parsed.tzinfo is None:
            raise ValueError
    except ValueError:
        errors.append("updatedAt must be a bounded RFC3339 timestamp")
    previous = root.get("previousCardDigest")
    if previous is not None and (not isinstance(previous, str) or not DIGEST.fullmatch(previous)):
        errors.append("previousCardDigest must be lowercase SHA-256")
    prohibited = secret_like_keys(card)
    if prohibited:
        errors.append("prohibited secret/private fields: " + ", ".join(prohibited))
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("card", type=Path)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--expected-identity")
    parser.add_argument("--expected-journal")
    parser.add_argument("--expected-owner")
    args = parser.parse_args()
    try:
        encoded = args.card.read_bytes()
        card = json.loads(encoded)
    except (OSError, json.JSONDecodeError) as error:
        print(f"pi-sync-capability-card: {error}", file=sys.stderr)
        return 2
    errors = validate(
        card,
        len(encoded),
        expected_identity=args.expected_identity,
        expected_journal=args.expected_journal,
        expected_owner=args.expected_owner,
    )
    result = {"version": 1, "valid": not errors, "bytes": len(encoded), "errors": errors}
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif errors:
        for error in errors:
            print(f"FAIL {error}")
    else:
        print(f"PASS capability card is valid ({len(encoded)} bytes)")
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
