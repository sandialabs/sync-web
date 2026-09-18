#!/usr/bin/env python3
"""Read-only diagnostics for Sync Web mailbox relationships."""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import signal
import stat
import subprocess
import sys
import time
from typing import Any, Callable


ADDRESS = re.compile(r"^[A-Za-z0-9_.*+!<>=?-]+@[A-Za-z0-9_.*+!<>=?-]+$")
SYMBOL = re.compile(r"^[A-Za-z0-9_.*+!<>=?/-]+$")
TOKEN = re.compile(r'\s*(\(|\)|"(?:\\.|[^"\\])*"|[^\s()]+)')
SCHEME_CREDENTIAL = re.compile(r'(\(credentials\s+)"(?:\\.|[^"\\])*"(\))', re.I)
KEY_VALUE_SECRET = re.compile(r'(?i)\b([A-Z0-9_]*(?:SECRET|TOKEN|PASSWORD|CREDENTIAL)[A-Z0-9_]*)=\S+')
BEARER = re.compile(r'(?i)\bBearer\s+\S+')
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$", re.I)
HEX_DIGEST = re.compile(r"^[0-9a-f]{64}$")
RECEIPT_PHASES = {
    "validated", "local-files-staged", "peer-identity-preapproved",
    "reciprocal-route-ready", "mailbox-prefix-authorized",
    "runtime-config-published", "systemd-reloaded",
}
RECEIPT_STATUS = {"in-progress", "failed", "complete"}
RECEIPT_KEYS = {
    "version", "status", "fingerprint", "descriptor", "packagedExtensionSha256",
    "phases", "piRestartRequired", "error",
}
DESCRIPTOR_KEYS = {
    "localAgent", "localJournal", "peerJournal", "peerIdentity", "bridgePeer",
    "bridgePeerSigningKeySha256", "bridgePeerInterface", "route", "remoteOwner",
    "incomingAuthorizedPath",
}
EXIT_LOCAL = 10
EXIT_ROUTE = 20
EXIT_AUTHORIZATION = 30
EXIT_PROTOCOL = 40
EXIT_RUNTIME = 50
EXIT_PACKAGE = 60
EXIT_INDETERMINATE = 70
MAX_COMMAND_OUTPUT = 65536
COMMAND_TIMEOUT = 30
OUTPUT_LIMIT_MARKER = f"command output exceeded {MAX_COMMAND_OUTPUT} bytes; process group terminated\n"


@dataclass
class Check:
    name: str
    status: str
    summary: str
    detail: Any = None
    remedy: str | None = None
    category: int = 0


class CommandError(RuntimeError):
    def __init__(self, argv: list[str], returncode: int, output: str):
        super().__init__(f"command failed ({returncode}): {' '.join(argv)}")
        self.argv = argv
        self.returncode = returncode
        self.output = output


def run_command(argv: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    try:
        process = subprocess.Popen(
            argv,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    except OSError as error:
        result = subprocess.CompletedProcess(argv, 127, f"executable unavailable: {error}\n", "")
    else:
        assert process.stdout is not None
        output = bytearray()
        deadline = time.monotonic() + COMMAND_TIMEOUT
        reason: str | None = None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        try:
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    reason = "timeout"
                    break
                events = selector.select(min(remaining, 0.1))
                if not events:
                    continue
                chunk = os.read(process.stdout.fileno(), 8192)
                if not chunk:
                    break
                output.extend(chunk)
                if len(output) > MAX_COMMAND_OUTPUT:
                    reason = "output-limit"
                    del output[MAX_COMMAND_OUTPUT:]
                    break
        finally:
            selector.close()
        if not reason:
            try:
                returncode = process.wait(timeout=max(0.0, deadline - time.monotonic()))
                prefix = ""
            except subprocess.TimeoutExpired:
                reason = "timeout"
        if reason:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            if reason == "timeout":
                returncode = 124
                prefix = f"command timed out after {COMMAND_TIMEOUT} seconds; process group terminated\n"
            else:
                returncode = 125
                prefix = OUTPUT_LIMIT_MARKER
        process.stdout.close()
        text = output.decode("utf-8", errors="replace")
        result = subprocess.CompletedProcess(argv, returncode, prefix + text, "")
    if check and result.returncode:
        raise CommandError(argv, result.returncode, result.stdout)
    return result


def parse_address(value: str) -> tuple[str, str]:
    if not ADDRESS.fullmatch(value):
        raise ValueError("address must have exact IDENTITY@JOURNAL form")
    identity, journal = value.split("@", 1)
    return identity, journal


def parse_route(value: str) -> list[str]:
    parts = value.split("/")
    if not parts or any(not part or not SYMBOL.fullmatch(part) or "/" in part for part in parts):
        raise ValueError("route must be a slash-separated nonempty symbol chain")
    return parts


def parse_scheme(text: str) -> Any:
    tokens = [match.group(1) for match in TOKEN.finditer(text)]
    position = 0

    def value() -> Any:
        nonlocal position
        if position >= len(tokens):
            raise ValueError("unexpected end of Scheme value")
        token = tokens[position]
        position += 1
        if token == "(":
            result = []
            while position < len(tokens) and tokens[position] != ")":
                result.append(value())
            if position >= len(tokens):
                raise ValueError("unterminated Scheme list")
            position += 1
            return result
        if token == ")":
            raise ValueError("unexpected closing parenthesis")
        if token == "#t":
            return True
        if token == "#f":
            return False
        if token.startswith('"'):
            return json.loads(token)
        try:
            return int(token)
        except ValueError:
            return token

    parsed = value()
    if position != len(tokens):
        raise ValueError("trailing Scheme data")
    return parsed


def association(items: list[Any]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for item in items:
        if isinstance(item, list) and len(item) >= 2 and isinstance(item[0], str):
            result[item[0]] = item[1] if len(item) == 2 else item[1:]
    return result


def authorization_rules(text: str) -> list[dict[str, Any]]:
    parsed = parse_scheme(text)
    if not isinstance(parsed, list):
        raise ValueError("authorization response is not a list")
    return [association(item) for item in parsed if isinstance(item, list)]


def safe_text(value: str, limit: int = 500) -> str:
    value = SCHEME_CREDENTIAL.sub(r'\1"<redacted>"\2', value)
    value = KEY_VALUE_SECRET.sub(r'\1=<redacted>', value)
    value = BEARER.sub('Bearer <redacted>', value)
    return " ".join(value[:limit].split())


def envelope_error_code(value: str) -> str:
    lowered = value.lower()
    if "json" in lowered or "unexpected token" in lowered:
        return "envelope-json-invalid"
    if "uuid" in lowered or "mailbox key" in lowered:
        return "envelope-uuid-invalid"
    if "timestamp" in lowered or "createdat" in lowered:
        return "envelope-timestamp-invalid"
    if "immutable-id" in lowered or "digest" in lowered or "changed" in lowered:
        return "envelope-binding-invalid"
    return "envelope-invalid"


def command_error_code(error: CommandError) -> str:
    lowered = error.output.lower()
    if error.returncode == 127 or "invalid choice" in lowered or "unrecognized" in lowered:
        return "unsupported"
    if error.returncode == 124 or "timed out" in lowered:
        return "timeout"
    if "permission" in lowered or "not authorized" in lowered or "authorization" in lowered:
        return "unauthorized"
    return "unavailable"


def bounded_route_success(returncode: int, output: str) -> bool:
    if returncode != 125 or not output.startswith(OUTPUT_LIMIT_MARKER):
        return False
    payload = output[len(OUTPUT_LIMIT_MARKER):]
    # The runner retains exactly this many source bytes; reject injected or expanded results.
    if len(payload.encode("utf-8")) != MAX_COMMAND_OUTPUT:
        return False
    if "(error" in payload or "bridge-error" in payload:
        return False
    if SCHEME_CREDENTIAL.search(payload) or KEY_VALUE_SECRET.search(payload) or BEARER.search(payload):
        return False
    header = r"\s*\(\(route-source \([A-Za-z0-9_.*+!<>=?/-]+(?: [A-Za-z0-9_.*+!<>=?/-]+)*\)\) \(terminal-index -?[0-9]+\) \(roots \("
    return bool(re.match(header, payload))


def safe_bad_entry(entry: dict[str, Any], prefixes: tuple[str, str]) -> dict[str, str]:
    path = str(entry.get("path", ""))
    identifier = next((path[len(prefix):] for prefix in prefixes if path.startswith(prefix)), "")
    return {
        "id": identifier.lower() if UUID.fullmatch(identifier) else "<invalid-or-redacted>",
        "code": envelope_error_code(str(entry.get("error", ""))),
    }


def safe_runtime_detail(runtime: dict[str, Any], bad_entry_count: int) -> dict[str, Any]:
    digest = runtime.get("configDigest")
    timestamp = runtime.get("lastSuccessfulPollAt")
    detail: dict[str, Any] = {
        "active": runtime.get("active") if isinstance(runtime.get("active"), bool) else None,
        "available": runtime.get("available") if isinstance(runtime.get("available"), bool) else None,
        "configDigest": digest if isinstance(digest, str) and HEX_DIGEST.fullmatch(digest) else "<invalid-or-redacted>",
        "lastSuccessfulPollAt": (timestamp if isinstance(timestamp, str) and len(timestamp) <= 40
                                 and re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z", timestamp)
                                 else "<invalid-or-redacted>"),
        "badEntryCount": bad_entry_count,
    }
    for key in ("pending", "queued", "deliveredInSession"):
        value = runtime.get(key)
        detail[key] = value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else None
    return detail


def valid_receipt(receipt: dict[str, Any]) -> bool:
    if set(receipt) - RECEIPT_KEYS or receipt.get("version") != 1:
        return False
    descriptor = receipt.get("descriptor")
    phases = receipt.get("phases")
    status_value = receipt.get("status")
    fingerprint = receipt.get("fingerprint")
    if (not isinstance(descriptor, dict) or set(descriptor) != DESCRIPTOR_KEYS
            or not isinstance(status_value, str) or status_value not in RECEIPT_STATUS
            or not isinstance(phases, list) or len(phases) > len(RECEIPT_PHASES)
            or any(not isinstance(phase, str) or phase not in RECEIPT_PHASES for phase in phases)
            or len(phases) != len(set(phases))
            or not isinstance(fingerprint, str) or not HEX_DIGEST.fullmatch(fingerprint)):
        return False
    symbols = ("localAgent", "localJournal", "peerJournal", "peerIdentity", "bridgePeer", "remoteOwner")
    if any(not isinstance(descriptor.get(field), str) or not SYMBOL.fullmatch(descriptor[field])
           or "/" in descriptor[field] for field in symbols):
        return False
    route = descriptor.get("route")
    if (not isinstance(route, list) or not route
            or any(not isinstance(hop, str) or not SYMBOL.fullmatch(hop) or "/" in hop for hop in route)):
        return False
    if descriptor.get("bridgePeer") != route[0]:
        return False
    if not isinstance(descriptor.get("bridgePeerSigningKeySha256"), str) or not HEX_DIGEST.fullmatch(descriptor["bridgePeerSigningKeySha256"]):
        return False
    interface = descriptor.get("bridgePeerInterface")
    if (not isinstance(interface, str) or len(interface) > 500
            or not re.match(r"^https?://[^\s/@]+(?::[0-9]+)?/interface$", interface)):
        return False
    expected_path = f"mailbox/inbox/{descriptor['peerJournal']}/{descriptor['peerIdentity']}"
    if descriptor.get("incomingAuthorizedPath") != expected_path:
        return False
    packaged = receipt.get("packagedExtensionSha256")
    if (not isinstance(packaged, dict)
            or any(not isinstance(key, str) or len(key) > 200
                   or not isinstance(value, str) or not HEX_DIGEST.fullmatch(value)
                   for key, value in packaged.items())):
        return False
    if not isinstance(receipt.get("piRestartRequired"), bool):
        return False
    if receipt.get("status") == "complete" and "error" in receipt:
        return False
    encoded = json.dumps(descriptor, sort_keys=True, separators=(",", ":")).encode()
    return fingerprint == hashlib.sha256(encoded).hexdigest()


def check_result(name: str, ok: bool, summary: str, *, detail: Any = None,
                 remedy: str | None = None, category: int = 0, warning: bool = False) -> Check:
    return Check(name, "warn" if warning else ("pass" if ok else "fail"), summary,
                 detail, remedy, 0 if ok or warning else category)


def load_json(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return data


def load_bounded_json_nofollow(path: Path) -> dict[str, Any]:
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        info = os.fstat(descriptor)
        if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_COMMAND_OUTPUT:
            raise ValueError("receipt is not a bounded regular file")
        encoded = bytearray()
        while len(encoded) <= MAX_COMMAND_OUTPUT:
            chunk = os.read(descriptor, min(8192, MAX_COMMAND_OUTPUT + 1 - len(encoded)))
            if not chunk:
                break
            encoded.extend(chunk)
        if len(encoded) > MAX_COMMAND_OUTPUT:
            raise ValueError("receipt exceeds diagnostic bound")
        value = json.loads(encoded)
        if not isinstance(value, dict):
            raise ValueError("receipt must be a JSON object")
        return value
    finally:
        os.close(descriptor)


def diagnose(args: argparse.Namespace, runner: Callable[..., subprocess.CompletedProcess[str]] = run_command) -> dict[str, Any]:
    identity, journal = parse_address(args.address)
    route = parse_route(args.route)
    local = load_json(args.agent_config)
    local_agent = local.get("id")
    local_journal = (local.get("sync") or {}).get("name", local_agent)
    local_owner = args.local_owner or local_agent
    remote_owner = args.remote_owner or identity
    principal = [*route, "*state*", identity]
    inbound_path = ["mailbox", "inbox", journal, identity]
    reciprocal_principal = [*route[:-1], local_journal, "*state*", local_agent] if len(route) > 1 else [local_journal, "*state*", local_agent]
    reciprocal_path = ["mailbox", "inbox", local_journal, local_agent]
    checks: list[Check] = []

    local_ok = local.get("version") == 1 and isinstance(local_agent, str) and isinstance(local_journal, str)
    checks.append(check_result("local-identity", local_ok,
                               ("local agent and journal identity are valid" if local_ok
                                else "local agent or journal identity is invalid"),
                               detail={"agent": local_agent, "journal": local_journal, "owner": local_owner},
                               category=EXIT_LOCAL))

    config: dict[str, Any] = {}
    try:
        config = load_json(args.inbox_config)
        peers = config.get("peers") if isinstance(config.get("peers"), list) else []
        recipients = config.get("recipients") if isinstance(config.get("recipients"), list) else []
        peer_ok = {"identity": identity, "journal": journal} in peers
        recipient_expected = {"identity": identity, "journal": journal, "owner": remote_owner, "route": route}
        recipient_ok = recipient_expected in recipients
        checks.append(check_result(
            "peer-config", peer_ok,
            ("inbound peer is configured" if peer_ok else "inbound peer is not present in file configuration"),
            detail={"expected": {"identity": identity, "journal": journal}},
            remedy=(f"After verifying the installed peer helper syntax, add receive intent for "
                    f"{identity}@{journal}; no command was generated."),
            category=EXIT_LOCAL,
        ))
        checks.append(check_result(
            "recipient-config", recipient_ok,
            ("outbound recipient route is configured" if recipient_ok
             else "outbound recipient route is not present in file configuration"),
            detail={"expected": recipient_expected},
            remedy=(f"After route verification and installed-helper inspection, add outbound intent for "
                    f"{identity}@{journal} via {'/'.join(route)} owned by {remote_owner}; no command was generated."),
            category=EXIT_LOCAL,
        ))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        checks.append(check_result("inbox-config", False, "inbox configuration is unavailable or invalid",
                                   detail=str(error), category=EXIT_LOCAL))

    route_value = "/".join(route)
    try:
        route_result = runner([str(args.journal_cli), "peer", "route", route_value], check=False)
        route_output = route_result.stdout
        bounded_route_prefix = bounded_route_success(route_result.returncode, route_output)
        route_ok = ((route_result.returncode == 0 and "(error" not in route_output and "bridge-error" not in route_output)
                    or bounded_route_prefix)
        route_unsupported = (route_result.returncode == 127 or "invalid choice" in route_output
                             or (len(route) > 1 and "Bridge is not committed" in route_output and route_value in route_output))
        route_detail: dict[str, Any] = {
            "route": route,
            "inspectionSupported": not route_unsupported,
            "inspectionStatus": ("unsupported" if route_unsupported else
                                 "ready-bounded-prefix" if route_ok and route_result.returncode == 125 else
                                 "available" if route_result.returncode == 0 else "failed"),
            "commandReturnCode": route_result.returncode,
        }
        if not route_ok:
            route_detail["error"] = safe_text(route_output)
        route_trusted = route_ok and not route_unsupported
        if route_unsupported:
            checks.append(check_result(
                "route", False, "installed client cannot inspect this route shape",
                detail=route_detail,
                remedy="Use a compatible read-only multi-hop route inspector; do not repair a bridge from unknown evidence.",
                warning=True,
            ))
        else:
            checks.append(check_result(
                "route", route_ok,
                ("complete route is committed and resolvable" if route_ok
                 else "complete route is not committed or resolvable"),
                detail=route_detail,
                remedy=f"journal-cli peer route {route_value} --explain-identity {identity}",
                category=EXIT_ROUTE,
            ))
    except OSError:
        route_trusted = False
        checks.append(check_result(
            "route", False, "route inspection is unavailable",
            detail={"route": route, "inspectionSupported": False, "inspectionStatus": "unavailable"},
            remedy="Use a compatible read-only route inspector.",
            warning=True,
        ))

    authorization_ok = False
    authorization_inspectable = True
    authorization_detail: dict[str, Any] = {
        "principal": principal,
        "path": inbound_path,
        "proposedRule": {
            "principal": principal,
            "path": inbound_path,
            "capabilities": {"use!": {"read-only?": True}, "put!": True, "run!": False, "retrieve": False},
        },
    }
    try:
        auth = runner([str(args.journal_cli), "peer", "authorizations", "--owner", str(local_owner)]).stdout
        matches = [rule for rule in authorization_rules(auth)
                   if rule.get("principal") == principal and rule.get("path") == inbound_path]
        if len(matches) == 1:
            use = matches[0].get("use!")
            capabilities = {
                "use!": {"read-only?": association(use).get("read-only?") is True} if isinstance(use, list) else False,
                "put!": matches[0].get("put!") is True,
                "run!": matches[0].get("run!") is True,
                "retrieve": matches[0].get("retrieve") is not False,
            }
            authorization_detail["capabilities"] = capabilities
            authorization_ok = capabilities == {"use!": {"read-only?": True}, "put!": True, "run!": False, "retrieve": False}
        else:
            authorization_detail["matchingRules"] = len(matches)
    except CommandError as error:
        authorization_inspectable = False
        authorization_detail["inspectionSupported"] = False
        authorization_detail["inspectionStatus"] = command_error_code(error)
        authorization_detail["commandReturnCode"] = error.returncode
    except OSError:
        authorization_inspectable = False
        authorization_detail["inspectionSupported"] = False
        authorization_detail["inspectionStatus"] = "unavailable"
    except ValueError:
        authorization_inspectable = False
        authorization_detail["inspectionSupported"] = False
        authorization_detail["inspectionStatus"] = "malformed-output"
    if authorization_inspectable:
        authorization_detail["inspectionSupported"] = True
        authorization_remedy = ((f"After reviewing proposedRule, explicitly authorize with: journal-cli peer authorize "
                                 f"{'/'.join(route)} {identity} {'/'.join(inbound_path)} --owner {local_owner}")
                                if route_trusted else
                                "Blocked until route topology is corrected and verified; do not change authorization.")
        checks.append(check_result(
            "inbound-authorization", authorization_ok,
            ("exact inbound mailbox grant has read-only use and put only" if authorization_ok
             else "exact inbound mailbox grant is absent, duplicated, or capability-mismatched"),
            detail=authorization_detail,
            remedy=authorization_remedy,
            category=EXIT_AUTHORIZATION,
        ))
    else:
        checks.append(check_result(
            "inbound-authorization", False,
            "installed client cannot inspect authorization; grant state is unknown",
            detail=authorization_detail,
            remedy="Use a compatible read-only authorization inspector before changing trust.",
            warning=True,
        ))

    runtime: dict[str, Any] = {}
    try:
        status = json.loads(runner([str(args.journal_cli), "peer", "enrollment", "status"]).stdout)
        runtime = status.get("runtime") or {}
        loaded_peers = runtime.get("peers") if isinstance(runtime.get("peers"), list) else []
        loaded_recipients = runtime.get("recipients") if isinstance(runtime.get("recipients"), list) else []
        runtime_ok = (runtime.get("active") is True and runtime.get("available") is True
                      and {"identity": identity, "journal": journal} in loaded_peers
                      and {"identity": identity, "journal": journal, "owner": remote_owner, "route": route} in loaded_recipients)
        bad_entries = runtime.get("badEntries") if isinstance(runtime.get("badEntries"), list) else []
        related_prefixes = (f"{journal}/{identity}/", f"delivered/{journal}/{identity}/")
        related_bad_raw = [entry for entry in bad_entries
                           if isinstance(entry, dict) and str(entry.get("path", "")).startswith(related_prefixes)]
        related_bad = [safe_bad_entry(entry, related_prefixes) for entry in related_bad_raw]
        unrelated_bad = [entry for entry in bad_entries if entry not in related_bad_raw]
        runtime_detail = safe_runtime_detail(runtime, len(bad_entries))
        checks.append(check_result("runtime", runtime_ok,
                                   ("running inbox loaded the peer and recipient" if runtime_ok
                                    else "running inbox did not load the expected peer and recipient"),
                                   detail=runtime_detail, category=EXIT_RUNTIME))
        envelope_ok = not related_bad
        envelope_warning = envelope_ok and bool(unrelated_bad)
        checks.append(check_result(
            "envelope-diagnostics", envelope_ok,
            ("no malformed inbox or delivery entries belong to this peer" if envelope_ok
             else "malformed inbox or delivery entries belong to this peer"),
            detail={"related": related_bad, "unrelatedCount": len(unrelated_bad)},
            remedy="Inspect the peer-qualified malformed UUID; retry only with a fresh UUID.",
            category=EXIT_PROTOCOL,
            warning=envelope_warning,
        ))
    except (CommandError, OSError, ValueError, json.JSONDecodeError) as error:
        checks.append(check_result("runtime", False, "runtime status is unavailable or invalid",
                                   detail=str(error), category=EXIT_RUNTIME))
        checks.append(check_result(
            "envelope-diagnostics", False,
            "malformed and delivery-marker classification is unavailable without runtime evidence",
            detail={"classificationAvailable": False},
            remedy="Provide a compatible structured runtime status snapshot; no mailbox state was changed.",
            warning=True,
        ))
    receipt_matches: list[dict[str, Any]] = []
    receipt_errors = 0
    receipt_available = args.enrollment_root.is_dir()
    if receipt_available:
        for path in sorted(args.enrollment_root.glob("*.json"), key=lambda item: item.name.encode()):
            try:
                receipt = load_bounded_json_nofollow(path)
                if not valid_receipt(receipt):
                    raise ValueError("receipt schema or fingerprint is invalid")
                descriptor = receipt["descriptor"]
                if (descriptor.get("localAgent") == local_agent
                        and descriptor.get("localJournal") == local_journal
                        and descriptor.get("peerJournal") == journal
                        and descriptor.get("peerIdentity") == identity
                        and descriptor.get("remoteOwner") == remote_owner
                        and descriptor.get("route") == route):
                    receipt_matches.append({
                        "status": receipt["status"],
                        "fingerprint": receipt["fingerprint"],
                        "phases": receipt["phases"],
                    })
            except (OSError, TypeError, ValueError, json.JSONDecodeError):
                receipt_errors += 1
    receipt_ok = (len(receipt_matches) == 1 and receipt_matches[0].get("status") == "complete")
    checks.append(check_result(
        "enrollment-metadata", receipt_ok,
        ("one complete local phase receipt matches the full correspondent descriptor" if receipt_ok
         else "no unique complete local phase receipt matches the full correspondent descriptor"),
        detail={
            "authority": "local phase metadata; not proof-bearing delivery evidence",
            "inspectionAvailable": receipt_available,
            "matches": receipt_matches,
            "invalidReceiptCount": receipt_errors,
        },
        remedy="A hot-added shared-hub correspondent may be operational without an initial receipt; do not fabricate one.",
        warning=not receipt_ok,
    ))

    package_detail: dict[str, Any] = {}
    package_ok = True
    for package in args.packages:
        try:
            query = runner([str(args.rpm), "-q", package], check=False)
            verify = runner([str(args.rpm), "-V", package], check=False)
            verify_lines = [safe_text(line) for line in verify.stdout.splitlines() if line.strip()][:20]
            package_detail[package] = {
                "nevra": safe_text(query.stdout.strip(), 200) if query.returncode == 0 else None,
                "verify": "clean" if verify.returncode == 0 and not verify_lines else "drift",
                "verifyLines": verify_lines,
            }
            package_ok = package_ok and query.returncode == 0 and verify.returncode == 0 and not verify_lines
        except OSError as error:
            package_detail[package] = {"nevra": None, "verify": "unavailable", "error": str(error)}
            package_ok = False
    checks.append(check_result("packages", package_ok,
                               ("required packages are installed without RPM drift" if package_ok
                                else "required package is absent or differs from its RPM payload"),
                               detail=package_detail, category=EXIT_PACKAGE))

    failures = {check.category for check in checks if check.status == "fail"}
    critical_unknowns = [check.name for check in checks
                         if check.name in {"route", "inbound-authorization"} and check.status == "warn"]
    healthy = not failures
    conclusive = not critical_unknowns
    ready = healthy and conclusive
    exit_code = next((category for category in (EXIT_ROUTE, EXIT_LOCAL, EXIT_AUTHORIZATION, EXIT_PROTOCOL, EXIT_RUNTIME, EXIT_PACKAGE)
                      if category in failures), EXIT_INDETERMINATE if critical_unknowns else 0)
    output = {
        "version": 1,
        "operation": "mailbox-doctor",
        "sideEffectFree": True,
        "address": {"identity": identity, "journal": journal, "formatted": args.address},
        "topology": {
            "route": route,
            "remoteOwner": remote_owner,
            "outboundPrincipalAtPeer": reciprocal_principal,
            "expectedInboundPrincipal": principal,
            "inboundMailboxPrefix": inbound_path,
            "expectedReciprocalPrincipal": reciprocal_principal,
            "reciprocalMailboxPrefix": reciprocal_path,
        },
        "stages": {
            "writeAccepted": "not-probed",
            "remoteCommitObserved": "not-probed",
            "envelopeObserved": "not-probed",
            "promptDelivered": "not-probed",
            "correlatedReply": "not-probed",
        },
        "checks": [asdict(check) for check in checks],
        "healthy": healthy,
        "conclusive": conclusive,
        "ready": ready,
        "ok": ready,
        "unknownChecks": critical_unknowns,
        "exitCode": exit_code,
    }
    return output


def human(output: dict[str, Any]) -> str:
    topology = output["topology"]
    lines = [
        f"Mailbox doctor: {output['address']['formatted']}",
        f"  route: {'/'.join(topology['route'])}",
        f"  owner: {topology['remoteOwner']}",
        f"  inbound principal: ({' '.join(topology['expectedInboundPrincipal'])})",
        f"  inbound prefix: {'/'.join(topology['inboundMailboxPrefix'])}",
        "",
    ]
    for check in output["checks"]:
        marker = {"pass": "PASS", "warn": "WARN", "fail": "FAIL"}[check["status"]]
        lines.append(f"{marker:4} {check['name']}: {check['summary']}")
        if check["status"] == "fail" and check.get("remedy"):
            lines.append(f"     next: {check['remedy']}")
        elif check["status"] == "warn" and check.get("remedy"):
            lines.append(f"     inspect: {check['remedy']}")
    lines.extend([
        "",
        f"Readiness: healthy={str(output['healthy']).lower()} conclusive={str(output['conclusive']).lower()} ready={str(output['ready']).lower()}",
        "Receipt stages: not probed (doctor performs no writes)",
    ])
    return "\n".join(lines) + "\n"


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subcommands = result.add_subparsers(dest="command", required=True)
    doctor = subcommands.add_parser("doctor", help="join mailbox topology, authorization, config, runtime, and package evidence")
    doctor.add_argument("address", help="logical IDENTITY@JOURNAL address")
    doctor.add_argument("--route", required=True, help="slash-separated directional route")
    doctor.add_argument("--remote-owner")
    doctor.add_argument("--local-owner")
    doctor.add_argument("--json", action="store_true")
    doctor.add_argument("--agent-config", type=Path, default=Path("/etc/pi-agent/agent.json"))
    doctor.add_argument("--inbox-config", type=Path, default=Path("/etc/pi-agent/sync-inbox.json"))
    doctor.add_argument("--journal-cli", type=Path, default=Path("/usr/bin/journal-cli"))
    doctor.add_argument("--enrollment-root", type=Path, default=Path("/var/lib/pi-agent/sync/enrollments"))
    doctor.add_argument("--rpm", type=Path, default=Path("/usr/bin/rpm"))
    doctor.add_argument("--packages", nargs="+", default=["pi-agent-core", "pi-agent-sync", "pi-agent-sync-inbox"])
    return result


def main() -> int:
    args = parser().parse_args()
    if args.command != "doctor":
        raise AssertionError(args.command)
    try:
        output = diagnose(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        if args.json:
            print(json.dumps({
                "version": 1,
                "operation": "mailbox-doctor",
                "sideEffectFree": True,
                "ok": False,
                "exitCode": 2,
                "fatal": {"category": "input", "error": str(error)[:1000]},
            }, indent=2, sort_keys=True))
        else:
            print(f"pi-sync-mailbox: {error}", file=sys.stderr)
        return 2
    print(json.dumps(output, indent=2, sort_keys=True) if args.json else human(output), end="\n" if args.json else "")
    return output["exitCode"]


if __name__ == "__main__":
    raise SystemExit(main())
