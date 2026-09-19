#!/usr/bin/env python3
"""Reject active repository consumers of the removed pre-resource API."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
HISTORICAL_FILES = {
    "CHANGELOG.md",
    "journal/CHANGELOG",
    "docs/ideas/cross-journal-data-flow.md",
    "docs/ideas/dev-1.5-plan.md",
    "docs/ideas/federation.md",
    "docs/migrations/1.6-resource-api.md",
    "tools/journal-cli/tests/head-helper-qualification-v0.1.4/helper/head_expected_old_cas.py",
}
HISTORICAL_PREFIXES = {
    "records/tests/fixtures/",
}
STRUCTURAL_PROTOCOL_FILES = {
    "records/LANGUAGE.md",
    "records/lisp/linear-chain.scm",
    "records/lisp/log-chain.scm",
    "records/lisp/standard.scm",
    "records/lisp/tree.scm",
    "records/tests/unit/test-chain.scm",
    "records/tests/unit/test-standard.scm",
    "records/tests/unit/test-tree.scm",
}
REMOVED_SCHEME_OPERATIONS = (
    "get", "get-batch", "set!", "set-batch!", "call!", "resolve", "resolve-batch",
)
SCHEME_ENVELOPE_PATTERN = re.compile(
    r"\(\s*\(\s*function\s+(?:" + "|".join(re.escape(operation) for operation in REMOVED_SCHEME_OPERATIONS)
    + r")\s*\)\s*\(\s*arguments\b"
)
PATTERNS = (
    re.compile(r"/general/(?:get|get-batch|set|set-batch|call|resolve|resolve-batch)(?:[/?`'\"\s]|$)"),
    re.compile(r"(?:journal|\*journal\*)[^\n]{0,100}'(?:get|get-batch|set!|set-batch!|call!|resolve|resolve-batch)\b"),
    re.compile(r"\b(?:post_json|post_scheme|call|call_general)\([^\n]{0,100}[\"'](?:get|get-batch|set|set-batch|call|resolve|resolve-batch)[\"']"),
    re.compile(r"(?:functionName|[\"']function[\"'])\s*:\s*[\"'](?:get|get-batch|set!|set-batch!|call!|resolve|resolve-batch)[\"']"),
    re.compile(r"[\"'](?:get|set!|call!|resolve)[\"']\s*:\s*(?:true|false|\{|\[)"),
    re.compile(r"\((?:get|set!|call!)\s+(?:#t|#f)\)"),
    re.compile(r"\(resolve\s+(?:#t|#f|\([^)]*\))\)"),
    re.compile(r"`(?:get|get-batch|set!|set-batch!|call!|resolve|resolve-batch)`"),
)


def tracked_files(root: Path) -> list[Path]:
    listed = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"],
        check=False,
        capture_output=True,
    )
    if listed.returncode == 0:
        return [root / item.decode("utf-8") for item in listed.stdout.split(b"\0") if item]
    return sorted(path for path in root.rglob("*") if path.is_file())


def skipped(path: Path, root: Path = ROOT) -> bool:
    relative = path.relative_to(root).as_posix()
    return (
        relative in HISTORICAL_FILES
        or any(relative.startswith(prefix) for prefix in HISTORICAL_PREFIXES)
        or path.name == "package-lock.json"
        or relative in STRUCTURAL_PROTOCOL_FILES
    )


def findings(root: Path = ROOT) -> list[str]:
    result: list[str] = []
    for path in tracked_files(root):
        if not path.is_file() or skipped(path, root):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        lines = text.splitlines()
        matched_lines = {
            line_number
            for line_number, line in enumerate(lines, 1)
            if any(pattern.search(line) for pattern in PATTERNS)
        }
        matched_lines.update(
            text.count("\n", 0, match.start()) + 1
            for match in SCHEME_ENVELOPE_PATTERN.finditer(text)
        )
        for line_number in sorted(matched_lines):
            line = lines[line_number - 1] if line_number <= len(lines) else ""
            result.append(f"{path.relative_to(root)}:{line_number}:{line.strip()}")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    result = findings()
    if result:
        print("Active consumers still reference removed resource operations or grants:", file=sys.stderr)
        print("\n".join(result), file=sys.stderr)
        return 1
    print("Current consumer scan passed: no removed operation or grant references found.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
