#!/usr/bin/python3
"""Owner-local producer/reviewer workflow companion for Source Publication v1/v1.1."""
from __future__ import annotations

import argparse
import hashlib
import importlib
import os
from pathlib import Path
import shutil
import sys

from .workflow_common import WorkflowError, canonical_json, concise_json_line, load_canonical_json, sha256_file, validate_review_package

VERSION = "sync-source-flow-candidate-v0.1.1"


def _delegate(module_name: str, arguments: list[str], entry_name: str = "main") -> int:
    module = importlib.import_module(f"{__package__}.{module_name}")
    entry = getattr(module, entry_name, None)
    if not callable(entry):
        raise WorkflowError("unsupported-capability", f"{module_name} has no {entry_name}(argv) entry")
    result = entry(arguments)
    if not isinstance(result, int) or isinstance(result, bool):
        raise WorkflowError("invalid-result", f"{module_name} returned a non-status")
    return result


def _diagnose(arguments: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="sync-source-flow diagnose")
    parser.add_argument("--launcher", type=Path, required=True)
    parser.add_argument("--launcher-sha256", required=True)
    parser.add_argument("--git", default="git")
    args = parser.parse_args(arguments)
    launcher = args.launcher.absolute()
    count, digest = sha256_file(launcher, maximum=1_000_000)
    if digest != args.launcher_sha256:
        raise WorkflowError("provenance-mismatch", "Source launcher SHA-256 differs")
    git = shutil.which(args.git)
    if git is None:
        raise WorkflowError("unsupported-capability", "Git executable is unavailable")
    sys.stdout.buffer.write(concise_json_line({"git": git, "launcher": str(launcher), "launcherBytes": count, "outcome": "ready", "version": VERSION}))
    return 0


def _validate(arguments: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="sync-source-flow validate-package")
    parser.add_argument("--package", type=Path, required=True)
    args = parser.parse_args(arguments)
    review, observed = validate_review_package(args.package)
    git = review["git"]
    sys.stdout.buffer.write(concise_json_line({
        "files": len(observed) + 1,
        "head": git["head"],
        "kind": git["kind"],
        "outcome": "verified-package",
        "prerequisite": git["prerequisite"],
        "tree": git["tree"],
        "version": VERSION,
    }))
    return 0


def main(arguments: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if arguments is None else arguments)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("producer", "ready", "reviewer", "diagnose", "validate-package"))
    parsed, remaining = parser.parse_known_args(arguments)
    try:
        if parsed.command == "producer":
            return _delegate("producer_workflow", remaining)
        if parsed.command == "ready":
            return _delegate("producer_workflow", remaining, "ready_main")
        if parsed.command == "reviewer":
            return _delegate("reviewer_workflow", remaining)
        if parsed.command == "diagnose":
            return _diagnose(remaining)
        return _validate(remaining)
    except WorkflowError as error:
        sys.stdout.buffer.write(concise_json_line({"error": error.code, "message": str(error), "outcome": "not-attempted", "version": VERSION}))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
