"""Read configured demo mailbox leaves; never mutate or acknowledge them."""
import json
import os
import sys

from journal_cli.client import JournalClient
from journal_cli.config import load_config
from journal_cli.message import load_message_config, read


def collect(client):
    config = load_message_config(client.config)
    messages = []
    for peer in config["peers"]:
        path = [-1, "*state*", config["owner"], "mailbox", "inbox", peer["journal"], peer["identity"]]
        directory = client.call_json("retrieve", {"path": path, "pinned?": False, "proof?": False})
        if directory == ["nothing"]:
            continue
        if not (isinstance(directory, list) and len(directory) == 3 and directory[0] == "directory"
                and isinstance(directory[1], dict) and directory[2] is True):
            raise ValueError("Expected complete mailbox directory")
        for message_id, kind in directory[1].items():
            if kind != "value":
                continue
            address = f"{peer['identity']}@{peer['journal']}"
            messages.append({"peer": peer, "envelope": read(client, address, message_id)})
    return messages


if __name__ == "__main__":
    try:
        client = JournalClient(load_config(os.environ["JOURNAL_CLI_CONFIG"]))
        print(json.dumps(collect(client)))
    except Exception:
        # Do not forward transport/server exception text into the model transcript.
        print("Demo inbox read failed; check local Journal and mailbox setup.", file=sys.stderr)
        sys.exit(1)
