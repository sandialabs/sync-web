from __future__ import annotations

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import stat
import sys
from typing import Callable

from .client import ExplicitReject, InvalidRequest, JournalClient, JournalError, UnconfirmedMutation, json_outcome, operation_may_mutate
from .config import ConfigError, DEFAULT_CONFIG, load_config
from . import message


VERSION = "0.1.0"
GROUPS = ("journal", "message", "peer", "profile", "source", "flow", "config", "doctor")


@contextmanager
def _argv(arguments: list[str]):
    previous = sys.argv
    sys.argv = ["journal-cli", *arguments]
    try:
        yield
    finally:
        sys.argv = previous


def _delegate_noargs(entry: Callable[[], object], arguments: list[str]) -> int:
    with _argv(arguments):
        result = entry()
    return int(result) if isinstance(result, int) else 0


def _json_argument(value: str) -> dict:
    try:
        text = sys.stdin.read() if value == "-" else (Path(value[1:]).read_text() if value.startswith("@") else value)
        result = json.loads(text)
    except (OSError, json.JSONDecodeError) as error:
        raise InvalidRequest(f"invalid arguments JSON: {error}") from error
    if not isinstance(result, dict):
        raise InvalidRequest("arguments JSON must be an object")
    return result


def _body(args: argparse.Namespace) -> str:
    if (args.body is None) == (args.body_file is None):
        raise InvalidRequest("supply exactly one of --body or --body-file")
    if args.body is not None:
        return args.body
    try:
        return Path(args.body_file).read_text(encoding="utf-8")
    except OSError as error:
        raise InvalidRequest(f"cannot read body file: {error}") from error


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(prog="journal-cli", description="Unified Sync Web 1.6 agent CLI")
    root.add_argument("--version", action="version", version=f"journal-cli {VERSION}")
    root.add_argument("--config", default=str(DEFAULT_CONFIG))
    groups = root.add_subparsers(dest="group", required=True)

    journal = groups.add_parser("journal", help="invoke Sync Web 1.6 Journal operations")
    journal_commands = journal.add_subparsers(dest="command", required=True)
    request = journal_commands.add_parser("request", help="invoke one bounded 1.6 operation")
    request.add_argument("function")
    request.add_argument("--arguments-json", default="{}", metavar="JSON|@FILE|-")
    request.add_argument("--identity")
    request.add_argument("--route", help="slash-separated federated route")
    raw = journal_commands.add_parser("raw", help="send complete UTF-8 Scheme from a file or stdin")
    raw.add_argument("input", help="file path or -")

    messages = groups.add_parser("message", help="one-shot Message operations; no resident poller")
    message_commands = messages.add_subparsers(dest="command", required=True)
    send = message_commands.add_parser("send")
    send.add_argument("to")
    send.add_argument("--body")
    send.add_argument("--body-file")
    send.add_argument("--in-reply-to")
    group = message_commands.add_parser("group")
    group.add_argument("--to", action="append", required=True)
    group.add_argument("--conversation-id", required=True)
    group.add_argument("--body")
    group.add_argument("--body-file")
    group.add_argument("--reply-from")
    group.add_argument("--reply-id")
    read = message_commands.add_parser("read")
    read.add_argument("peer", help="exact identity@journal")
    read.add_argument("id")
    message_commands.add_parser("status")

    peer = groups.add_parser("peer", help="peer, enrollment, route, and diagnostic workflows")
    peer_commands = peer.add_subparsers(dest="command", required=True)
    for name in ("capability-card", "enrollment", "mailbox-doctor", "registry"):
        item = peer_commands.add_parser(name, add_help=False)
        item.add_argument("arguments", nargs=argparse.REMAINDER)
    peer_commands.add_parser("signing-key-digest")
    route = peer_commands.add_parser("route")
    route.add_argument("target")
    route.add_argument("--explain-identity")
    preapprove = peer_commands.add_parser("preapprove")
    preapprove.add_argument("peer")
    preapprove.add_argument("--signing-key-sha256-base64", required=True)
    bridge = peer_commands.add_parser("bridge")
    bridge.add_argument("target")
    bridge.add_argument("interface")
    bridge.add_argument("--remote-name", required=True)
    delete_bridge = peer_commands.add_parser("delete-bridge")
    delete_bridge.add_argument("target")
    delete_bridge.add_argument("--expected-public-key-sha256", required=True)
    delete_bridge.add_argument("--apply", action="store_true")
    authorize = peer_commands.add_parser("authorize")
    authorize.add_argument("remote_route")
    authorize.add_argument("remote_identity")
    authorize.add_argument("path")
    authorize.add_argument("--owner")
    authorize.add_argument("--read-only", action="store_true")
    authorize.add_argument("--retrieve", action="store_true")
    authorize.add_argument("--run", action="store_true")
    authorize.add_argument("--revoke", action="store_true")
    authorize.add_argument("--dry-run", action="store_true")
    authorizations = peer_commands.add_parser("authorizations")
    authorizations.add_argument("--owner")
    authorizations.add_argument("--digest", action="store_true")
    replace_route = peer_commands.add_parser("recipient-route-replace")
    replace_route.add_argument("--journal", required=True)
    replace_route.add_argument("--identity", required=True)
    replace_route.add_argument("--from-route", required=True)
    replace_route.add_argument("--to-route", required=True)
    replace_route.add_argument("--owner", required=True)
    replace_route.add_argument("--expect-config-digest", required=True)
    replace_route.add_argument("--apply", action="store_true")

    profile = groups.add_parser("profile", help="profile validation and publication workflows", add_help=False)
    profile.add_argument("-h", "--help", action="store_true", dest="profile_help")
    profile.add_argument("arguments", nargs=argparse.REMAINDER)

    source = groups.add_parser("source", help="Source Publication operations")
    source_commands = source.add_subparsers(dest="command", required=True)
    ops = source_commands.add_parser("ops", help="low-level Source locator v2 operations", add_help=False)
    ops.add_argument("arguments", nargs=argparse.REMAINDER)
    commands = source_commands.add_parser("publication", help="high-level Source Publication", add_help=False)
    commands.add_argument("arguments", nargs=argparse.REMAINDER)

    flow = groups.add_parser("flow", help="Source producer and reviewer workflows", add_help=False)
    flow.add_argument("arguments", nargs=argparse.REMAINDER)

    config = groups.add_parser("config", help="validate unified configuration")
    config.add_argument("command", choices=("validate", "show"))
    groups.add_parser("doctor", help="run bounded local readiness checks")
    return root


def _journal(args: argparse.Namespace, client: JournalClient) -> int:
    if args.command == "raw":
        data = sys.stdin.buffer.read() if args.input == "-" else Path(args.input).read_bytes()
        result = client.raw(data)
        outcome = "accepted"
    else:
        arguments = _json_argument(args.arguments_json)
        route = args.route.split("/") if args.route else None
        if route:
            result = client.call(args.function, arguments, identity=args.identity, route=route)
        else:
            result = client.call_json(args.function, arguments, identity=args.identity)
        conditional_mutation = args.function in {
            "put!", "put-batch!", "copy!", "copy-batch!", "pin!", "pin-batch!", "unpin!", "unpin-batch!",
        }
        framed_false = route and isinstance(result, str) and result.strip() == "#f"
        if conditional_mutation and (result is False or framed_false):
            sys.stdout.write(json_outcome("journal.request", "rejected", result=False))
            return 3
        outcome = "accepted" if operation_may_mutate(args.function, arguments) else "retrieved"
    sys.stdout.write(json_outcome(f"journal.{args.command}", outcome, result=result))
    return 0


def _message(args: argparse.Namespace, client: JournalClient) -> int:
    if args.command == "send":
        result = message.send(client, args.to, _body(args), args.in_reply_to)
    elif args.command == "group":
        result = message.send_group(client, args.to, _body(args), args.conversation_id, args.reply_from, args.reply_id)
    elif args.command == "read":
        result = message.read(client, args.peer, args.id)
    else:
        result = message.status(client)
    sys.stdout.write(json_outcome(f"message.{args.command}", result.pop("outcome", "retrieved"), result=result))
    return 0


def _peer(args: argparse.Namespace, config_path: str) -> int:
    if args.command == "capability-card":
        from .peer import pi_sync_capability_card as module
        return _delegate_noargs(module.main, args.arguments)
    if args.command == "registry":
        from .peer import registry as module
        config = load_config(config_path)
        return _delegate_noargs(module.main, ["--agent-config", config_path, "--config", str(config.inbox_config), *args.arguments])
    if args.command == "enrollment":
        from .peer import enrollment as module
        return _delegate_noargs(module.main, args.arguments)
    if args.command == "mailbox-doctor":
        from .peer import pi_sync_mailbox as module
        return _delegate_noargs(module.main, args.arguments)
    from .peer import operations
    client = JournalClient(load_config(config_path))
    if args.command == "signing-key-digest": return operations.signing_key_digest(client)
    if args.command == "route": return operations.route(client, args.target, args.explain_identity)
    if args.command == "preapprove": return operations.preapprove(client, args.peer, args.signing_key_sha256_base64)
    if args.command == "bridge": return operations.bridge(client, args.target, args.interface, args.remote_name)
    if args.command == "delete-bridge": return operations.delete_bridge(
        client, args.target, args.expected_public_key_sha256, args.apply
    )
    if args.command == "authorize": return operations.authorize(client, args)
    if args.command == "recipient-route-replace": return operations.recipient_route_replace(client, args)
    return operations.authorizations(client, args.owner, args.digest)


def _profile(arguments: list[str], config_path: str) -> int:
    from .profile import commands
    config = load_config(config_path)
    result = commands.main(["--config", config_path, "--state-dir", str(config.state_dir), *arguments])
    return int(result) if isinstance(result, int) else 0


def _source(args: argparse.Namespace, config_path: str) -> int:
    config = load_config(config_path)
    bound = ["--config", config_path, "--state-dir", str(config.state_dir), *args.arguments]
    if args.command == "ops":
        from .source import ops
        return _delegate_noargs(ops.main, bound)
    from .source import sync_source
    return _delegate_noargs(sync_source.main, bound)


def _flow(arguments: list[str]) -> int:
    from .flow import sync_source_flow
    return sync_source_flow.main(arguments)


def _configuration(args: argparse.Namespace) -> int:
    config = load_config(args.config)
    value = {
        "schema": "journal-cli-config-v1", "outcome": "valid", "owner": config.owner,
        "journal": config.journal, "endpoint": config.endpoint,
        "credentialFile": str(config.credential_file), "inboxConfig": str(config.inbox_config),
    }
    if args.command == "show":
        value["stateDir"] = str(config.state_dir)
        value["timeoutSeconds"] = config.timeout_seconds
    sys.stdout.write(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")
    return 0


def _doctor(config_path: str) -> int:
    config = load_config(config_path)
    checks = []
    for label, path in (("credential", config.credential_file), ("message-config", config.inbox_config)):
        try:
            info = path.stat()
            secure = stat.S_ISREG(info.st_mode) and not info.st_mode & 0o077
            checks.append({"name": label, "ok": secure, "path": str(path)})
        except OSError as error:
            checks.append({"name": label, "ok": False, "path": str(path), "error": str(error)})
    checks.append({"name": "endpoint", "ok": True, "value": config.endpoint})
    ok = all(item["ok"] for item in checks)
    sys.stdout.write(json_outcome("doctor", "ready" if ok else "not-ready", checks=checks))
    return 0 if ok else 1


def main(arguments: list[str] | None = None) -> int:
    args = parser().parse_args(arguments)
    try:
        if args.group == "journal":
            return _journal(args, JournalClient(load_config(args.config)))
        if args.group == "message":
            return _message(args, JournalClient(load_config(args.config)))
        if args.group == "peer":
            return _peer(args, args.config)
        if args.group == "profile":
            return _profile(["--help"] if args.profile_help else args.arguments, args.config)
        if args.group == "source":
            return _source(args, args.config)
        if args.group == "flow":
            return _flow(args.arguments)
        if args.group == "config":
            return _configuration(args)
        return _doctor(args.config)
    except (ConfigError, InvalidRequest, ExplicitReject, UnconfirmedMutation, JournalError, OSError) as error:
        outcome = getattr(error, "outcome", "not-attempted")
        sys.stdout.write(json_outcome(f"{args.group}.{getattr(args, 'command', '')}".rstrip("."), outcome,
                                      error={"type": type(error).__name__, "message": str(error)[:1000]}))
        return {"not-attempted": 2, "rejected": 3, "failed-or-ambiguous": 4}.get(outcome, 1)


if __name__ == "__main__":
    raise SystemExit(main())
