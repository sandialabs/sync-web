from __future__ import annotations

from datetime import datetime, timezone
import json
from pathlib import Path
import re
from typing import Any
from uuid import UUID, uuid4

from .client import JournalClient, InvalidRequest, scheme_string, scheme_symbol
from .config import Config


MAX_MESSAGE_BYTES = 65_536


def _uuid(value: str, label: str) -> str:
    try:
        UUID(value)
    except ValueError as error:
        raise InvalidRequest(f"{label} must be a UUID") from error
    return value


def load_message_config(config: Config) -> dict[str, Any]:
    try:
        value = json.loads(config.inbox_config.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise InvalidRequest(f"invalid Message configuration: {error}") from error
    if not isinstance(value, dict) or value.get("version") != 1:
        raise InvalidRequest("Message configuration version must be 1")
    if value.get("owner") != config.owner:
        raise InvalidRequest("Message configuration owner differs")
    return value


def recipient(config: dict[str, Any], address: str) -> dict[str, Any]:
    recipients = config.get("recipients")
    if not isinstance(recipients, list):
        raise InvalidRequest("Message recipients are missing")
    exact = [item for item in recipients if isinstance(item, dict) and f"{item.get('identity')}@{item.get('journal')}" == address]
    if exact:
        return exact[0]
    if "@" in address:
        raise InvalidRequest(f"Message recipient is not configured: {address}")
    matches = [item for item in recipients if isinstance(item, dict) and item.get("identity") == address]
    if len(matches) > 1:
        raise InvalidRequest(f"Message recipient identity is ambiguous: {address}")
    if not matches:
        raise InvalidRequest(f"Message recipient is not configured: {address}")
    return matches[0]


def _send(client: JournalClient, message_config: dict[str, Any], target: dict[str, Any], envelope: dict[str, Any]) -> None:
    body = json.dumps(envelope, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    maximum = int(message_config.get("maxMessageBytes", MAX_MESSAGE_BYTES))
    if len(body) > maximum:
        raise InvalidRequest(f"Message envelope exceeds {maximum} bytes")
    parts = ["*state*", target["owner"], "mailbox", "inbox", message_config["localJournal"], message_config["owner"], envelope["id"]]
    path = "(" + " ".join(scheme_symbol(item) for item in parts) + ")"
    route = "(" + " ".join(scheme_symbol(item) for item in target["route"]) + ")"
    data = "#u(" + " ".join(str(item) for item in body) + ")"
    credential = scheme_string(client.config.credential())
    origin = scheme_symbol(message_config["owner"])
    expression = (
        f"((function put!) (arguments ((path {path}) (value {data}) (expression? #f))) "
        f"(invocation ((identity {origin}) (route-source ()) (route-target {route}) (credentials {credential}))))"
    )
    client.post_scheme(expression, mutation=True)


def send(client: JournalClient, to: str, body: str, in_reply_to: str | None = None) -> dict[str, Any]:
    if not isinstance(body, str) or not body.strip():
        raise InvalidRequest("Message body must be nonempty text")
    config = load_message_config(client.config)
    target = recipient(config, to)
    envelope = {
        "version": 1, "id": str(uuid4()),
        "from": f"{config['owner']}@{config['localJournal']}",
        "to": f"{target['identity']}@{target['journal']}",
        "createdAt": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "body": body,
    }
    if in_reply_to is not None:
        envelope["inReplyTo"] = _uuid(in_reply_to, "inReplyTo")
    _send(client, config, target, envelope)
    return {**envelope, "outcome": "write-accepted"}


def send_group(client: JournalClient, addresses: list[str], body: str, conversation_id: str,
               reply_from: str | None = None, reply_id: str | None = None) -> dict[str, Any]:
    if not 2 <= len(addresses) <= 15 or len(set(addresses)) != len(addresses):
        raise InvalidRequest("group requires 2..15 unique recipients")
    if not body.strip():
        raise InvalidRequest("Message body must be nonempty text")
    if (reply_from is None) != (reply_id is None):
        raise InvalidRequest("group reply source and id must be supplied together")
    config = load_message_config(client.config)
    targets = [recipient(config, address) for address in addresses]
    sender = f"{config['owner']}@{config['localJournal']}"
    participants = sorted([sender, *[f"{item['identity']}@{item['journal']}" for item in targets]])
    if len(set(participants)) != len(participants):
        raise InvalidRequest("group participants must be unique")
    reply = None
    if reply_from is not None:
        if reply_from not in participants:
            raise InvalidRequest("group reply sender is not a participant")
        reply = {"from": reply_from, "id": _uuid(reply_id or "", "reply id")}
    message_id = str(uuid4())
    created = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    outcomes = []
    for target in targets:
        to = f"{target['identity']}@{target['journal']}"
        envelope = {
            "version": 2, "id": message_id, "from": sender, "to": to,
            "createdAt": created, "body": body,
            "conversationId": _uuid(conversation_id, "conversationId"),
            "participants": participants,
        }
        if reply is not None:
            envelope["inReplyTo"] = reply
        try:
            _send(client, config, target, envelope)
            outcomes.append({"to": to, "outcome": "write-accepted"})
        except Exception as error:  # Each group write has an independent terminal outcome.
            outcomes.append({"to": to, "outcome": getattr(error, "outcome", "failed-or-ambiguous"), "error": str(error)[:512]})
    return {
        "version": 2, "id": message_id, "from": sender, "createdAt": created,
        "body": body, "conversationId": conversation_id, "participants": participants,
        **({"inReplyTo": reply} if reply is not None else {}), "outcomes": outcomes,
    }


def read(client: JournalClient, peer: str, message_id: str) -> dict[str, Any]:
    config = load_message_config(client.config)
    peers = config.get("peers", [])
    matches = [item for item in peers if isinstance(item, dict) and f"{item.get('identity')}@{item.get('journal')}" == peer]
    if len(matches) != 1:
        raise InvalidRequest("peer must be an exact configured identity@journal address")
    item = matches[0]
    path = [-1, "*state*", config["owner"], "mailbox", "inbox", item["journal"], item["identity"], _uuid(message_id, "message id")]
    value = client.call_json("retrieve", {"path": path, "pinned?": False, "proof?": False})
    content = value.get("content") if isinstance(value, dict) and "content" in value else value
    if not isinstance(content, dict) or set(content) != {"*type/byte-vector*"}:
        raise InvalidRequest("Message content is not an exact byte vector")
    try:
        envelope = json.loads(bytes.fromhex(content["*type/byte-vector*"]))
    except (ValueError, json.JSONDecodeError) as error:
        raise InvalidRequest("Message envelope JSON is invalid") from error
    expected_from = f"{item['identity']}@{item['journal']}"
    expected_to = f"{config['owner']}@{config['localJournal']}"
    if not isinstance(envelope, dict) or envelope.get("id") != message_id or envelope.get("from") != expected_from or envelope.get("to") != expected_to:
        raise InvalidRequest("Message envelope does not match its authorized mailbox path")
    return envelope


def status(client: JournalClient) -> dict[str, Any]:
    config = load_message_config(client.config)
    return {
        "outcome": "ready", "owner": config["owner"], "journal": config["localJournal"],
        "peers": len(config.get("peers", [])), "recipients": len(config.get("recipients", [])),
        "residentPollerIncluded": False,
    }
