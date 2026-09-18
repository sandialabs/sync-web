#!/usr/bin/python3
"""Atomically manage Sync inbox convenience config without changing trust authority."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile

DEFAULT_AGENT_CONFIG = Path("/etc/pi-agent/agent.json")
DEFAULT_INBOX_CONFIG = Path("/etc/pi-agent/sync-inbox.json")
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


def load_identity(path: Path) -> tuple[str, str]:
    data = json.loads(path.read_text())
    if data.get("version") != 1:
        raise SystemExit("unsupported agent configuration")
    agent = symbol(data.get("owner", data.get("id", "")), "agent identity")
    return agent, symbol(data.get("journal", (data.get("sync") or {}).get("name", agent)), "local journal")


def validate_config(config: dict, agent: str, local_journal: str) -> dict:
    if config.get("version") != 1 or config.get("agent") != agent or config.get("owner") != agent:
        raise SystemExit("Sync inbox configuration belongs to another identity")
    if config.get("localJournal") != local_journal:
        raise SystemExit("Sync inbox local journal does not match the agent configuration")
    peers = config.get("peers")
    recipients = config.get("recipients")
    if not isinstance(peers, list) or not isinstance(recipients, list):
        raise SystemExit("Sync inbox peers and recipients must be arrays")
    peer_keys = []
    for peer in peers:
        peer_keys.append((symbol(peer.get("journal", ""), "peer journal"),
                          symbol(peer.get("identity", ""), "peer identity")))
    recipient_keys = []
    for recipient in recipients:
        route = recipient.get("route")
        if not isinstance(route, list) or not route:
            raise SystemExit("recipient route must be a nonempty hop array")
        recipient_keys.append((symbol(recipient.get("journal", ""), "recipient journal"),
                               symbol(recipient.get("identity", ""), "recipient identity")))
        recipient["route"] = [symbol(hop, "recipient route hop") for hop in route]
        symbol(recipient.get("owner", ""), "recipient owner")
    if len(set(peer_keys)) != len(peer_keys) or len(set(recipient_keys)) != len(recipient_keys):
        raise SystemExit("Sync inbox peer/recipient entries must be unique")
    return config


def config_digest(config: dict) -> str:
    return hashlib.sha256(json.dumps(config, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


@contextmanager
def config_lock(path: Path):
    lock = path.with_name(f".{path.name}.lock")
    lock.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    descriptor = os.open(lock, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise SystemExit("another Sync peer configuration update is in progress") from error
        yield
    finally:
        os.close(descriptor)


def write_config(path: Path, config: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w") as handle:
            json.dump(config, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def output(config: dict, changed: bool) -> None:
    print(json.dumps({
        "version": 1,
        "changed": changed,
        "trustChanged": False,
        "configDigest": config_digest(config),
        "agent": config["agent"],
        "localJournal": config["localJournal"],
        "peers": config["peers"],
        "recipients": config["recipients"],
        "note": "Peer configuration controls polling/sending convenience only; authorization is separate.",
    }, indent=2, sort_keys=True))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agent-config", type=Path, default=DEFAULT_AGENT_CONFIG)
    parser.add_argument("--config", type=Path, default=DEFAULT_INBOX_CONFIG)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list")
    commands.add_parser("validate")
    add = commands.add_parser("add")
    add.add_argument("--journal", required=True)
    add.add_argument("--identity", required=True)
    add.add_argument("--receive", action="store_true", help="poll this logical sender's mailbox prefix")
    add.add_argument("--send-route", help="slash-separated directional route chain for outbound messages")
    add.add_argument("--owner", help="terminal remote owner; defaults to identity")
    remove = commands.add_parser("remove")
    remove.add_argument("--journal", required=True)
    remove.add_argument("--identity", required=True)
    remove.add_argument("--receive", action="store_true", help="remove only the receive peer")
    remove.add_argument("--send", action="store_true", help="remove only the outbound recipient")
    args = parser.parse_args()

    agent, local_journal = load_identity(args.agent_config)
    with config_lock(args.config):
        config = validate_config(json.loads(args.config.read_text()), agent, local_journal)
        if args.command in {"list", "validate"}:
            output(config, False)
            return
        journal = symbol(args.journal, "peer journal")
        identity = symbol(args.identity, "peer identity")
        before = config_digest(config)
        if args.command == "add":
            if not args.receive and not args.send_route:
                raise SystemExit("add requires --receive, --send-route, or both")
            if args.receive:
                candidate = {"journal": journal, "identity": identity}
                matches = [item for item in config["peers"] if item.get("journal") == journal and item.get("identity") == identity]
                if matches and matches != [candidate]:
                    raise SystemExit("conflicting receive peer configuration")
                if not matches:
                    config["peers"].append(candidate)
            if args.send_route:
                candidate = {"journal": journal, "identity": identity, "route": route_chain(args.send_route),
                             "owner": symbol(args.owner or identity, "remote owner")}
                matches = [item for item in config["recipients"] if item.get("journal") == journal and item.get("identity") == identity]
                if matches and matches != [candidate]:
                    raise SystemExit("conflicting outbound recipient configuration")
                if not matches:
                    config["recipients"].append(candidate)
        else:
            remove_receive = args.receive or not args.send
            remove_send = args.send or not args.receive
            if remove_receive:
                config["peers"] = [item for item in config["peers"]
                                   if (item.get("journal"), item.get("identity")) != (journal, identity)]
            if remove_send:
                config["recipients"] = [item for item in config["recipients"]
                                        if (item.get("journal"), item.get("identity")) != (journal, identity)]
        validate_config(config, agent, local_journal)
        changed = config_digest(config) != before
        if changed:
            write_config(args.config, config)
        output(config, changed)


if __name__ == "__main__":
    main()
