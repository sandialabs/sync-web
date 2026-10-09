"""Invoke the repository CLI for the human demo Journal (no automatic enrollment)."""
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools/journal-cli"))
from journal_cli.cli import main


def config():
    os.umask(0o077)
    directory = Path(__file__).resolve().parent / ".demo-state"
    directory.mkdir(mode=0o700, exist_ok=True)
    secret = directory / "interface.secret"
    secret.write_text(os.environ.get("HUMAN_INTERFACE_SECRET", "demo-human-interface"))
    secret.chmod(0o600)
    inbox = directory / "inbox.json"
    if not inbox.exists():
        peers = [{"identity": name, "journal": name} for name in ("agent-1", "agent-2")]
        inbox.write_text(json.dumps({"version": 1, "owner": "admin", "localJournal": "human",
            "peers": peers, "recipients": [dict(peer, owner=peer["identity"], route=[peer["journal"]]) for peer in peers]}))
    path = directory / "journal-cli.json"
    path.write_text(json.dumps({"version": 1, "owner": "admin", "journal": "human",
        "endpoint": f"http://localhost:{os.environ.get('DEMO_PORT', '8192')}/api/v1/journal/interface",
        "credentialFile": str(secret), "stateDir": str(directory), "inboxConfig": str(inbox)}))
    return path


if __name__ == "__main__":
    sys.exit(main(["--config", str(config()), *sys.argv[1:]]))
