#!/usr/bin/env python3
"""Shared compact artifact helpers for repository tooling."""

from __future__ import annotations

import json
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

SECRET_KEY = re.compile(
    r"(secret|token|password|credential|private[_-]?key|api[_-]?key|access[_-]?key|cookie|session|dsn)",
    re.I,
)
TEXT_SECRETS = [
    re.compile(r'(?i)(authorization:\s*(?:bearer|basic)\s+)[^\s"\']+'),
    re.compile(r'(?i)(set-cookie:\s*)[^\r\n]+'),
    re.compile(r'(?i)(cookie:\s*)[^\r\n]+'),
    re.compile(r'(?i)("(?:credentials|password|token|secret|cookie|session|dsn|api[_-]?key|access[_-]?key)"\s*:\s*")[^"]*(")'),
    re.compile(r'(?i)(\((?:credentials|password|token|secret|cookie|session|dsn|api[_-]?key|access[_-]?key)\s+")[^"]*("\))'),
    re.compile(r'(?i)((?:postgres(?:ql)?|mysql|sqlite|mongodb(?:\+srv)?)://)[^\s"\']+'),
    re.compile(r'(\((?:\*step\*|\*call\*|\*eval\*|\*set-query\*|\*set-step\*)\s+")[^"]*(")'),
]


def timestamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def git_state(root: Path) -> dict[str, Any]:
    def run(*args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(root), *args],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout.strip()

    return {
        "commit": run("rev-parse", "HEAD"),
        "branch": run("branch", "--show-current"),
        "dirty": bool(run("status", "--porcelain")),
        "status": run("status", "--short").splitlines(),
    }


def redact_text(value: str) -> str:
    for pattern in TEXT_SECRETS:
        value = pattern.sub(r"\1<redacted>\2" if pattern.groups >= 2 else r"\1<redacted>", value)
    return value


def redact(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: ("<redacted>" if SECRET_KEY.search(str(key)) else redact(item))
            for key, item in value.items()
        }
    if isinstance(value, list):
        redacted = []
        for item in value:
            if isinstance(item, str) and "=" in item:
                key, _, _ = item.partition("=")
                redacted.append(f"{key}=<redacted>" if SECRET_KEY.search(key) else item)
            else:
                redacted.append(redact(item))
        return redacted
    return value


def write_artifacts(
    directory: Path,
    summary: dict[str, Any],
    report_lines: Iterable[str],
    commands: Iterable[str] = (),
) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "summary.json").write_text(
        json.dumps(redact(summary), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (directory / "report.md").write_text(
        "\n".join(report_lines).rstrip() + "\n",
        encoding="utf-8",
    )
    (directory / "commands.log").write_text(
        "\n".join(commands).rstrip() + "\n",
        encoding="utf-8",
    )


def repo_root(script: str) -> Path:
    return Path(script).resolve().parent.parent


def artifact_root(root: Path, tool: str) -> Path:
    override = os.environ.get("SYNC_ARTIFACT_ROOT")
    return Path(override) if override else root / "target" / tool
