"""Run Pi and a private released Ledger in one demo container."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import urllib.request

HOME = Path(os.environ.get("HOME", "/home/demo"))
ENDPOINT = "http://127.0.0.1:8192/interface"


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def configure(name):
    other = "agent-2" if name == "agent-1" else "agent-1"
    peers = [{"identity": "admin", "journal": "human"},
             {"identity": other, "journal": other}]
    recipients = [dict(peers[0], owner="admin", route=["human"]),
                  dict(peers[1], owner=other, route=["human", other])]
    inbox = HOME / "inbox.json"
    if not inbox.exists():
        write_json(inbox, {"version": 1, "owner": name, "localJournal": name,
                           "peers": peers, "recipients": recipients})
    write_json(HOME / "journal-cli.json", {
        "version": 1, "owner": name, "journal": name, "endpoint": ENDPOINT,
        "credentialFile": str(HOME / "ledger/ledger.interface-secret"),
        "stateDir": str(HOME / "cli-state"), "inboxConfig": str(inbox),
    })
    agent_dir = HOME / ".pi/agent"
    write_json(agent_dir / "models.json", {"providers": {"litellm": {
        "baseUrl": os.environ["LITELLM_URL"], "api": os.environ["LITELLM_API"],
        "apiKey": "${LITELLM_TOKEN}", "models": [{"id": os.environ["LITELLM_MODEL"],
        "reasoning": False, "input": ["text"], "contextWindow": 32000, "maxTokens": 4096}],
    }}})
    write_json(agent_dir / "settings.json", {
        "defaultProvider": "litellm", "defaultModel": os.environ["LITELLM_MODEL"],
        "defaultThinkingLevel": "off", "cacheWarming": "off",
        "extensions": ["/opt/demo/inbox.ts"],
    })
    (agent_dir / "AGENTS.md").write_text((Path("/opt/demo/AGENTS.md").read_text()).replace("{{NAME}}", name).replace("{{OTHER}}", other))
    (HOME / "workspace").mkdir(exist_ok=True)


def health():
    with urllib.request.urlopen("http://127.0.0.1:8192/", timeout=2) as response:
        return response.status == 200


def run():
    os.umask(0o077)
    name = os.environ["AGENT_NAME"]
    if name not in {"agent-1", "agent-2"}:
        raise ValueError("AGENT_NAME must be agent-1 or agent-2")
    configure(name)
    ledger_dir = HOME / "ledger"
    ledger_dir.mkdir(exist_ok=True)
    env = os.environ.copy()
    env["SYNC_WEB_WASMER_KERNEL_SHA256"] = Path("/opt/ledger/kernel.sha256").read_text().strip()
    log = (ledger_dir / "process.log").open("a")
    children = []

    def stop(*_):
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        ledger = subprocess.Popen([
            "/opt/ledger/ledger", "--database", str(ledger_dir / "journal"),
            "--port", "8192", "--period", os.environ.get("JOURNAL_PERIOD", "8"),
            "--name", name, "--interface", f"http://{name}:8192/interface",
        ], env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        children.append(ledger)
        for _ in range(120):
            if ledger.poll() is not None:
                raise RuntimeError("Ledger exited; inspect ledger/process.log")
            try:
                if health():
                    break
            except OSError:
                pass
            time.sleep(0.5)
        else:
            raise RuntimeError("Ledger startup did not complete")
        pi = subprocess.Popen(["pi", "--continue"], cwd=HOME / "workspace", env=env)
        children.append(pi)
        while all(child.poll() is None for child in children):
            time.sleep(0.25)
        return pi.returncode if pi.poll() is not None else 1
    finally:
        stop()
        for child in reversed(children):
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        log.close()


if __name__ == "__main__":
    if sys.argv[1:] == ["health"]:
        sys.exit(0 if health() else 1)
    sys.exit(run())
