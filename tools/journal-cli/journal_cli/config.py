from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import stat
from urllib.parse import urlsplit


DEFAULT_CONFIG = Path(os.environ.get("JOURNAL_CLI_CONFIG", "/etc/pi-agent/agent.json"))
DEFAULT_SECRET = Path("/var/lib/pi-agent/sync/ledger.interface-secret")
DEFAULT_INBOX_CONFIG = Path(os.environ.get("PI_SYNC_INBOX_CONFIG", "/etc/pi-agent/sync-inbox.json"))


class ConfigError(ValueError):
    pass


@dataclass(frozen=True)
class Config:
    endpoint: str
    owner: str
    journal: str
    credential_file: Path
    timeout_seconds: float
    state_dir: Path
    inbox_config: Path

    def credential(self) -> str:
        try:
            info = self.credential_file.stat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
                raise ConfigError("credential file must be an owner-only regular file")
            value = self.credential_file.read_text(encoding="utf-8").strip()
        except OSError as error:
            raise ConfigError(f"cannot read credential file: {error}") from error
        if not value:
            raise ConfigError("credential file is empty")
        return value


def _endpoint(value: object) -> str:
    if not isinstance(value, str):
        raise ConfigError("endpoint must be a string")
    parsed = urlsplit(value)
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ConfigError("endpoint must not contain credentials, query, or fragment")
    if parsed.scheme == "https" and parsed.hostname:
        return value
    if parsed.scheme == "http" and parsed.hostname in {"127.0.0.1", "::1", "localhost"}:
        return value
    raise ConfigError("endpoint must use verified HTTPS or literal-loopback HTTP")


def load_config(path: str | Path = DEFAULT_CONFIG) -> Config:
    config_path = Path(path)
    try:
        data = json.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ConfigError(f"invalid configuration {config_path}: {error}") from error
    if not isinstance(data, dict) or data.get("version") != 1:
        raise ConfigError("configuration version must be 1")

    # Accept both the standalone shape and the installed fixed-agent shape.
    sync = data.get("sync") if isinstance(data.get("sync"), dict) else {}
    owner = data.get("owner", data.get("id"))
    journal = data.get("journal", sync.get("name", owner))
    endpoint = data.get("endpoint", sync.get("localInterface"))
    credential = data.get("credentialFile", str(DEFAULT_SECRET))
    timeout = data.get("timeoutSeconds", 30)
    state_dir = data.get("stateDir", "/var/lib/pi-agent/sync")
    inbox = data.get("inboxConfig", str(DEFAULT_INBOX_CONFIG))
    if not isinstance(owner, str) or not owner:
        raise ConfigError("owner must be a nonempty string")
    if not isinstance(journal, str) or not journal:
        raise ConfigError("journal must be a nonempty string")
    if not isinstance(credential, str) or not credential:
        raise ConfigError("credentialFile must be a nonempty path")
    if not isinstance(timeout, (int, float)) or isinstance(timeout, bool) or not 0 < timeout <= 120:
        raise ConfigError("timeoutSeconds must be in (0, 120]")
    return Config(
        endpoint=_endpoint(endpoint), owner=owner, journal=journal,
        credential_file=Path(credential), timeout_seconds=float(timeout),
        state_dir=Path(state_dir), inbox_config=Path(inbox),
    )
