#!/usr/bin/python3
"""Explicitly enroll one peer in the dormant generic Sync Web inbox."""

from __future__ import annotations

import argparse
import base64
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import sys
from urllib.parse import urlsplit

AGENT_CONFIG = Path("/etc/pi-agent/agent.json")
INBOX_CONFIG = Path("/etc/pi-agent/sync-inbox.json")
PACKAGED_ROOT = Path("/usr/lib/pi-agent/sync-inbox")
DROPIN = Path("/etc/systemd/system/pi-agent.service.d/sync-inbox.conf")
INBOX_SERVICE = "pi-sync-inbox.service"
RECEIPT_ROOT = Path("/var/lib/pi-agent/sync/enrollments")
LOCK_PATH = Path("/var/lib/pi-agent/sync/inbox-enrollment.lock")
RUNTIME_STATUS = Path("/var/lib/pi-agent/sync/inbox-runtime-status.json")
SYMBOL = re.compile(r"^[A-Za-z0-9_.*+!<>=?/-]+$")


def symbol(value: str, label: str) -> str:
    if not SYMBOL.fullmatch(value) or value in {".", ".."} or "/" in value:
        raise SystemExit(f"invalid {label}: {value!r}")
    return value


def route_chain(value: str) -> list[str]:
    parts = value.split("/")
    if not parts or any(not part for part in parts):
        raise SystemExit("route must be slash-separated nonempty symbols")
    return [symbol(part, "route hop") for part in parts]


def private_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w") as handle:
            json.dump(value, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def private_text(path: Path, value: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w") as handle:
            handle.write(value)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


@contextmanager
def enrollment_lock():
    LOCK_PATH.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    descriptor = os.open(LOCK_PATH, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise SystemExit("another Sync inbox enrollment is already in progress") from error
        yield
    finally:
        os.close(descriptor)


def journal_cli_command(*arguments: str) -> list[str]:
    return [sys.executable, "-m", "journal_cli", "--config", str(AGENT_CONFIG), "peer", *arguments]


def run(argv: list[str], *, capture: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(argv, check=True, text=True,
                          stdout=subprocess.PIPE if capture else None,
                          stderr=subprocess.PIPE if capture else None)


def route_ready(route: str) -> bool:
    result = subprocess.run(journal_cli_command("route", route), check=False, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    text = result.stdout[:65536]
    return result.returncode == 0 and "(error" not in text and "bridge-error" not in text


def validate_interface(value: str) -> str:
    parsed = urlsplit(value)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname or parsed.username or parsed.password:
        raise SystemExit("peer interface must be an HTTP(S) URL without embedded credentials")
    if parsed.query or parsed.fragment or parsed.path.rstrip("/") != "/interface":
        raise SystemExit("peer interface must end at /interface without a query or fragment")
    return value


def load_agent() -> tuple[str, str]:
    data = json.loads(AGENT_CONFIG.read_text())
    if data.get("version") != 1:
        raise SystemExit("unsupported agent configuration")
    agent = symbol(data.get("id", ""), "agent identity")
    local_journal = symbol((data.get("sync") or {}).get("name", agent), "local journal")
    return agent, local_journal


def load_or_create_config(agent: str, local_journal: str) -> dict:
    if INBOX_CONFIG.exists():
        config = json.loads(INBOX_CONFIG.read_text())
        if config.get("version") != 1 or config.get("agent") != agent or config.get("owner") != agent:
            raise SystemExit("existing Sync inbox configuration belongs to another identity")
        return config
    return {
        "version": 1,
        "agent": agent,
        "owner": agent,
        "localJournal": local_journal,
        "endpoint": "http://127.0.0.1:8192/interface",
        "secretFile": "/var/lib/pi-agent/sync/ledger.interface-secret",
        "peers": [],
        "recipients": [],
        "pollMs": 30000,
        "maxMessageBytes": 65536,
        "messageRetentionMs": 86400000,
        "deliveredRetentionMs": 2592000000,
    }


def merge_unique(items: list[dict], candidate: dict, keys: tuple[str, ...], label: str) -> None:
    matches = [item for item in items if all(item.get(key) == candidate[key] for key in keys)]
    if matches:
        if len(matches) != 1 or matches[0] != candidate:
            raise SystemExit(f"conflicting existing {label} configuration")
        return
    items.append(candidate)


def packaged_hashes(*, test: bool = True) -> dict[str, str]:
    sources = {
        "index.ts": PACKAGED_ROOT / "index.ts",
        "inbox-service.mjs": PACKAGED_ROOT / "inbox-service.mjs",
        "inbox-control.mjs": PACKAGED_ROOT / "inbox-control.mjs",
        "lib/sync-inbox-core.mjs": PACKAGED_ROOT / "lib/sync-inbox-core.mjs",
        "sync-message.mjs": PACKAGED_ROOT / "sync-message.mjs",
        "sync-inbox.test.mjs": PACKAGED_ROOT / "sync-inbox.test.mjs",
    }
    for source in sources.values():
        if not source.is_file():
            raise SystemExit(f"missing packaged Sync inbox file: {source}")
    if test:
        run(["/usr/bin/node", "--test", str(sources["sync-inbox.test.mjs"])])
    return {relative: hashlib.sha256(source.read_bytes()).hexdigest() for relative, source in sources.items()}


def descriptor_fingerprint(descriptor: dict) -> str:
    encoded = json.dumps(descriptor, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def test_failpoint(name: str) -> None:
    value = os.environ.get("PI_SYNC_INBOX_TEST_FAIL_AFTER")
    if value == f"runtime:{name}":
        raise RuntimeError(f"injected enrollment failure after {name}")
    if value == f"keyboard:{name}":
        raise KeyboardInterrupt(f"injected enrollment interruption after {name}")


def enrollment_descriptor(args: argparse.Namespace) -> tuple[dict, bytes]:
    agent, local_journal = load_agent()
    if args.confirm != agent:
        raise SystemExit(f"refusing enrollment; pass --confirm {agent}")
    peer_journal = symbol(args.peer_journal, "peer journal")
    peer_identity = symbol(args.peer_identity, "peer identity")
    route = route_chain(args.route)
    remote_owner = symbol(args.remote_owner, "remote owner")
    peer_interface = validate_interface(args.peer_interface)
    try:
        peer_digest = base64.b64decode(args.peer_signing_key_sha256_base64, validate=True)
    except Exception as error:
        raise SystemExit("peer signing-key SHA-256 must be valid base64") from error
    if len(peer_digest) != 32:
        raise SystemExit("peer signing-key SHA-256 must decode to exactly 32 bytes")
    return {
        "localAgent": agent,
        "localJournal": local_journal,
        "peerJournal": peer_journal,
        "peerIdentity": peer_identity,
        "bridgePeer": route[0],
        "bridgePeerSigningKeySha256": peer_digest.hex(),
        "bridgePeerInterface": peer_interface,
        "route": route,
        "remoteOwner": remote_owner,
        "incomingAuthorizedPath": f"mailbox/inbox/{peer_journal}/{peer_identity}",
    }, peer_digest


def enrollment_plan(args: argparse.Namespace) -> None:
    descriptor, _peer_digest = enrollment_descriptor(args)
    fingerprint = descriptor_fingerprint(descriptor)
    receipt_path = RECEIPT_ROOT / f"{descriptor['peerJournal']}-{descriptor['peerIdentity']}.json"
    existing = json.loads(receipt_path.read_text()) if receipt_path.exists() else None
    route_exists = route_ready("/".join(descriptor["route"]))
    conflict = bool(existing and existing.get("fingerprint") != fingerprint)
    if conflict:
        action = "conflict"
    elif existing and existing.get("status") == "complete":
        action = "reconcile"
    elif existing:
        action = "resume"
    elif route_exists:
        action = "refuse-unbound-route"
    else:
        action = "new-enrollment"
    output = {
        "version": 1,
        "eligible": action in {"new-enrollment", "resume", "reconcile"},
        "action": action,
        "fingerprint": fingerprint,
        "descriptor": descriptor,
        "routeReady": route_exists,
        "receiptStatus": existing.get("status") if existing else None,
        "localGrant": {
            "principal": [*descriptor["route"], "*state*", descriptor["peerIdentity"]],
            "owner": descriptor["localAgent"],
            "path": descriptor["incomingAuthorizedPath"],
            "permissions": {"get": True, "set!": True, "resolve": False},
        },
        "remoteRequirements": {
            "preapproveLocalIdentity": descriptor["localJournal"],
            "authorizeRemoteMailbox": f"mailbox/inbox/{descriptor['localJournal']}/{descriptor['localAgent']}",
        },
        "unchanged": ["model/search credentials", "frontend owner access", "unrelated routes and grants"],
    }
    print(json.dumps(output, indent=2, sort_keys=True))
    if not output["eligible"]:
        raise SystemExit(2)


def enroll(args: argparse.Namespace) -> None:
    if os.geteuid() != 0:
        raise SystemExit("pi-sync-inbox enroll must run as root inside the agent container")
    with enrollment_lock():
        descriptor, _peer_digest = enrollment_descriptor(args)
        agent = descriptor["localAgent"]
        local_journal = descriptor["localJournal"]
        peer_journal = descriptor["peerJournal"]
        peer_identity = descriptor["peerIdentity"]
        route = descriptor["route"]
        route_value = "/".join(route)
        bridge_peer = route[0]
        remote_owner = descriptor["remoteOwner"]
        peer_interface = descriptor["bridgePeerInterface"]
        incoming_path = descriptor["incomingAuthorizedPath"]
        fingerprint = descriptor_fingerprint(descriptor)
        receipt_path = RECEIPT_ROOT / f"{peer_journal}-{peer_identity}.json"
        existing = json.loads(receipt_path.read_text()) if receipt_path.exists() else None
        route_at_start = route_ready(route_value)
        if route_at_start and not existing:
            raise SystemExit("refusing to adopt a preexisting route without an enrollment receipt")
        if existing:
            if existing.get("fingerprint") != fingerprint:
                raise SystemExit(f"conflicting enrollment receipt: {receipt_path}")
            if existing.get("status") != "complete" and not args.resume:
                raise SystemExit(f"partial enrollment requires --resume with the exact descriptor; inspect {receipt_path}")
            state = existing
        else:
            state = {
                "version": 1,
                "status": "in-progress",
                "fingerprint": fingerprint,
                "descriptor": descriptor,
                "packagedExtensionSha256": {},
                "phases": ["validated"],
                "piRestartRequired": True,
            }
            private_json(receipt_path, state)

        if route_at_start and "peer-signing-key-preapproved" not in state.get("phases", []):
            raise SystemExit("existing route is not bound to a recorded exact peer preapproval")

        config = load_or_create_config(agent, local_journal)
        peer = {"journal": peer_journal, "identity": peer_identity}
        recipient = {"journal": peer_journal, "identity": peer_identity, "route": route, "owner": remote_owner}
        merge_unique(config["peers"], peer, ("journal", "identity"), "peer")
        merge_unique(config["recipients"], recipient, ("journal", "identity"), "recipient")
        hashes = packaged_hashes()
        state["packagedExtensionSha256"] = hashes
        state["status"] = "in-progress"
        state.pop("error", None)

        staged_config = INBOX_CONFIG.with_name(f".{INBOX_CONFIG.name}.pending-{os.getpid()}")
        staged_dropin = DROPIN.with_name(f".{DROPIN.name}.pending-{os.getpid()}")
        private_json(staged_config, config)
        private_text(staged_dropin, "[Service]\nEnvironment=PI_SYNC_INBOX_CONFIG=/etc/pi-agent/sync-inbox.json\n")

        def phase(value: str) -> None:
            if value not in state["phases"]:
                state["phases"].append(value)
            private_json(receipt_path, state)

        phase("local-files-staged")
        try:
            run(journal_cli_command("preapprove", bridge_peer, "--signing-key-sha256-base64", args.peer_signing_key_sha256_base64))
            test_failpoint("preapprove-operation")
            phase("peer-signing-key-preapproved")
            if not route_at_start:
                run(journal_cli_command("bridge", bridge_peer, peer_interface, "--remote-name", local_journal))
                for _ in range(45):
                    if route_ready(route_value):
                        break
                    time.sleep(2)
                else:
                    raise RuntimeError("reciprocal Sync route did not become ready within 90 seconds")
                test_failpoint("bridge-operation")
            phase("reciprocal-route-ready")
            run(journal_cli_command("authorize", route_value, peer_identity, incoming_path, "--owner", agent))
            test_failpoint("authorize-operation")
            phase("mailbox-prefix-authorized")
            os.replace(staged_config, INBOX_CONFIG)
            os.replace(staged_dropin, DROPIN)
            test_failpoint("runtime-config-publish")
            phase("runtime-config-published")
            run(["/usr/bin/systemctl", "daemon-reload"])
            run(["/usr/bin/systemctl", "enable", INBOX_SERVICE])
            if subprocess.run(["/usr/bin/systemctl", "is-active", "--quiet", "pi-sync-journal.service"], check=False).returncode == 0:
                run(["/usr/bin/systemctl", "start", INBOX_SERVICE])
            test_failpoint("systemd-reload")
            phase("systemd-reloaded")
            state["status"] = "complete"
            state.pop("error", None)
            private_json(receipt_path, state)
        except BaseException as error:
            state["status"] = "failed"
            state["error"] = f"{type(error).__name__}: {error}"[:1000]
            private_json(receipt_path, state)
            raise
        finally:
            for pending in (staged_config, staged_dropin):
                try:
                    pending.unlink()
                except FileNotFoundError:
                    pass

        print(json.dumps(state, indent=2, sort_keys=True))
        print("Enrollment is staged. Send any required acknowledgement first, then restart pi-agent.service at a settled boundary.")


def doctor(require_active: bool = False) -> None:
    checks: list[dict] = []

    def check(name: str, passed: bool, detail: object = None, *, required: bool = True) -> None:
        item = {"name": name, "status": "pass" if passed else ("fail" if required else "warn")}
        if detail is not None:
            item["detail"] = detail
        checks.append(item)

    try:
        agent, local_journal = load_agent()
        check("agent-identity", True, {"agent": agent, "localJournal": local_journal})
    except Exception as error:
        check("agent-identity", False, str(error))
        output = {"version": 1, "healthy": False, "ready": False, "configured": False, "checks": checks}
        print(json.dumps(output, indent=2, sort_keys=True))
        raise SystemExit(1)

    identity_hash = None
    try:
        digest_result = json.loads(run(journal_cli_command("signing-key-digest"), capture=True).stdout)
        digest = base64.b64decode(digest_result["base64"], validate=True)
        if len(digest) != 32 or digest_result.get("sha256") != digest.hex():
            raise ValueError("local signing-key SHA-256 response is invalid")
        identity_hash = digest.hex()
        check("sync-signing-key", True, {"sha256": identity_hash})
    except Exception as error:
        check("sync-signing-key", False, str(error))

    current_hashes = None
    try:
        current_hashes = packaged_hashes(test=False)
        check("packaged-extension", True, current_hashes)
    except Exception as error:
        check("packaged-extension", False, str(error))

    config = None
    configured = INBOX_CONFIG.is_file()
    if configured:
        try:
            config = load_or_create_config(agent, local_journal)
            check("inbox-config", True, str(INBOX_CONFIG))
        except Exception as error:
            check("inbox-config", False, str(error))
    else:
        check("inbox-config", False, "dormant: no enrollment configuration", required=False)

    expected_dropin = "[Service]\nEnvironment=PI_SYNC_INBOX_CONFIG=/etc/pi-agent/sync-inbox.json\n"
    if configured:
        try:
            check("systemd-dropin", DROPIN.read_text() == expected_dropin, str(DROPIN))
        except Exception as error:
            check("systemd-dropin", False, str(error))

    authorization_text = None
    authorization_digest = None
    if configured:
        try:
            authorization_result = run(journal_cli_command("authorizations", "--owner", agent), capture=True)
            authorization_text = " ".join(authorization_result.stdout.split())
            authorization_digest = hashlib.sha256(authorization_result.stdout.strip().encode()).hexdigest()
            check("authorization-table", True, {"sha256": authorization_digest})
        except Exception as error:
            check("authorization-table", False, str(error))

    receipts = []
    expected_peers = set()
    expected_recipients = set()
    if RECEIPT_ROOT.is_dir():
        for path in sorted(RECEIPT_ROOT.glob("*.json")):
            item = {"file": path.name}
            try:
                receipt = json.loads(path.read_text())
                descriptor = receipt.get("descriptor")
                if receipt.get("version") != 1 or not isinstance(descriptor, dict):
                    raise ValueError("unsupported receipt schema")
                fingerprint = descriptor_fingerprint(descriptor)
                if receipt.get("fingerprint") != fingerprint:
                    raise ValueError("descriptor fingerprint mismatch")
                if descriptor.get("localAgent") != agent or descriptor.get("localJournal") != local_journal:
                    raise ValueError("receipt belongs to another local identity")
                complete = receipt.get("status") == "complete"
                hashes_match = current_hashes is not None and receipt.get("packagedExtensionSha256") == current_hashes
                route = descriptor.get("route")
                if not isinstance(route, list) or not route:
                    raise ValueError("receipt route must be a nonempty hop array")
                route_value = "/".join(symbol(hop, "receipt route hop") for hop in route)
                route_ok = complete and route_ready(route_value)
                principal = " ".join([*route, "*state*", symbol(descriptor.get("peerIdentity", ""), "peer identity")])
                relative_path = " ".join(symbol(part, "authorization path")
                                         for part in descriptor.get("incomingAuthorizedPath", "").split("/") if part)
                expected_rule = (f"((principal ({principal})) (key-index (0 -1)) (path ({relative_path})) "
                                 "(use! ((read-only? #t))) (put! #t) (run! #f) (retrieve #f))")
                authorization_match = authorization_text is not None and expected_rule in authorization_text
                item.update({
                    "status": receipt.get("status"), "fingerprint": fingerprint,
                    "peerJournal": descriptor.get("peerJournal"), "peerIdentity": descriptor.get("peerIdentity"),
                    "bridgePeer": descriptor.get("bridgePeer"),
                    "bridgePeerSigningKeySha256": descriptor.get("bridgePeerSigningKeySha256"),
                    "route": descriptor.get("route"), "routeReady": route_ok,
                    "authorizationMatch": authorization_match,
                    "packageHashesCurrent": hashes_match, "phases": receipt.get("phases", []),
                })
                check(f"receipt:{path.name}", complete and hashes_match and route_ok and authorization_match, item)
                if complete:
                    expected_peers.add((descriptor["peerJournal"], descriptor["peerIdentity"]))
                    expected_recipients.add((descriptor["peerJournal"], descriptor["peerIdentity"],
                                             tuple(descriptor["route"]), descriptor["remoteOwner"]))
            except Exception as error:
                item.update({"status": "invalid", "error": str(error)[:500]})
                check(f"receipt:{path.name}", False, item)
            receipts.append(item)

    if config is not None:
        actual_peers = {(item.get("journal"), item.get("identity")) for item in config.get("peers", [])}
        actual_recipients = {(item.get("journal"), item.get("identity"), tuple(item.get("route", [])), item.get("owner"))
                             for item in config.get("recipients", [])}
        check("receipt-config-peer-set", actual_peers == expected_peers,
              {"configured": sorted(actual_peers), "receipted": sorted(expected_peers)})
        check("receipt-config-recipient-set", actual_recipients == expected_recipients,
              {"configured": sorted(actual_recipients), "receipted": sorted(expected_recipients)})

    active = subprocess.run(["/usr/bin/systemctl", "is-active", "--quiet", "pi-agent.service"], check=False).returncode == 0
    inbox_active = subprocess.run(["/usr/bin/systemctl", "is-active", "--quiet", INBOX_SERVICE], check=False).returncode == 0
    check("pi-service-active", active, "active" if active else "restart required", required=require_active)
    check("inbox-service-active", inbox_active, "active" if inbox_active else "service start required", required=require_active)
    runtime = runtime_status()
    if inbox_active and configured:
        runtime_ok = runtime.get("available") is True and runtime.get("valid", True) and runtime.get("active") is True
        runtime_ok = runtime_ok and not runtime.get("configError") and not runtime.get("lastError")
        check("inbox-runtime", runtime_ok, runtime)
    else:
        check("inbox-runtime", False, runtime, required=False)
    healthy = not any(item["status"] == "fail" for item in checks)
    ready = healthy and configured and active and inbox_active and bool(receipts)
    output = {
        "version": 1, "healthy": healthy, "ready": ready, "configured": configured,
        "agent": agent, "localJournal": local_journal, "localSigningKeySha256": identity_hash,
        "piServiceActive": active, "authorizationDigest": authorization_digest,
        "runtime": runtime, "receipts": receipts, "checks": checks,
    }
    print(json.dumps(output, indent=2, sort_keys=True))
    if not healthy or (require_active and not ready):
        raise SystemExit(1)


def runtime_status() -> dict:
    if not RUNTIME_STATUS.exists():
        return {"available": False}
    try:
        value = json.loads(RUNTIME_STATUS.read_text())
        if value.get("version") != 1:
            raise ValueError("unsupported runtime status version")
        return {"available": True, **value}
    except Exception as error:
        return {"available": True, "valid": False, "error": str(error)[:500]}


def status() -> None:
    receipts = []
    if RECEIPT_ROOT.is_dir():
        for path in sorted(RECEIPT_ROOT.glob("*.json")):
            try:
                receipt = json.loads(path.read_text())
                receipts.append({"file": path.name, "status": receipt.get("status"), "phases": receipt.get("phases", [])})
            except Exception as error:
                receipts.append({"file": path.name, "status": "invalid", "error": str(error)[:300]})
    if not INBOX_CONFIG.is_file():
        print(json.dumps({"configured": False, "enrollments": receipts, "runtime": runtime_status()}, sort_keys=True))
        return
    config = json.loads(INBOX_CONFIG.read_text())
    active = subprocess.run(["/usr/bin/systemctl", "is-active", "--quiet", "pi-agent.service"], check=False).returncode == 0
    inbox_active = subprocess.run(["/usr/bin/systemctl", "is-active", "--quiet", INBOX_SERVICE], check=False).returncode == 0
    print(json.dumps({
        "configured": True,
        "piServiceActive": active,
        "inboxServiceActive": inbox_active,
        "agent": config.get("agent"),
        "localJournal": config.get("localJournal"),
        "peers": config.get("peers", []),
        "recipients": config.get("recipients", []),
        "enrollments": receipts,
        "runtime": runtime_status(),
    }, indent=2, sort_keys=True))


def add_relationship_arguments(command: argparse.ArgumentParser) -> None:
    command.add_argument("--peer-journal", required=True)
    command.add_argument("--peer-identity", required=True)
    command.add_argument("--bridge-peer-signing-key-sha256-base64", dest="peer_signing_key_sha256_base64", required=True,
                         help="expected Base64-encoded 32-byte signing-key SHA-256 of the first route hop")
    command.add_argument("--bridge-peer-interface", "--peer-interface", dest="peer_interface", required=True,
                         help="HTTP(S) Interface URL of the first route hop")
    command.add_argument("--route", required=True, help="slash-separated directional route/provenance chain")
    command.add_argument("--remote-owner", required=True)
    command.add_argument("--confirm", required=True, help="exact local agent identity")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description="Enroll explicitly approved peers in the dormant generic Sync inbox")
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("status")
    inspect = commands.add_parser("doctor", help="verify identity, receipts, routes, package hashes, and activation")
    inspect.add_argument("--require-active", action="store_true", help="fail unless the enrolled poller is active")
    plan = commands.add_parser("plan", help="show the exact proposed trust relationship without mutating it")
    add_relationship_arguments(plan)
    add = commands.add_parser("enroll")
    add_relationship_arguments(add)
    add.add_argument("--resume", action="store_true", help="resume an exact phase-marked partial enrollment")
    return result


def main() -> None:
    args = parser().parse_args()
    if args.command == "status":
        status()
    elif args.command == "doctor":
        doctor(args.require_active)
    elif args.command == "plan":
        enrollment_plan(args)
    else:
        enroll(args)


if __name__ == "__main__":
    main()
