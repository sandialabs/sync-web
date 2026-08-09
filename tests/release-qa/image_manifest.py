#!/usr/bin/env python3
"""Write or verify source-bound container image manifests for release-QA repeats."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any


def command(*args: str) -> str:
    return subprocess.run(args, check=True, text=True, stdout=subprocess.PIPE).stdout.strip()


def source(root: Path) -> dict[str, str]:
    if command("git", "-C", str(root), "status", "--porcelain"):
        raise RuntimeError("source worktree is not clean")
    return {
        "commit": command("git", "-C", str(root), "rev-parse", "HEAD"),
        "tree": command("git", "-C", str(root), "rev-parse", "HEAD^{tree}"),
    }


def image(runtime: str, reference: str) -> dict[str, Any]:
    inspected = json.loads(command(runtime, "image", "inspect", reference))[0]
    return {
        "reference": reference,
        "id": inspected["Id"],
        "digest": inspected.get("Digest") or "",
        "repo_digests": sorted(inspected.get("RepoDigests") or []),
    }


def project_references(runtime: str, project: str) -> list[str]:
    ids = command(runtime, "ps", "-q", "--filter", f"label=com.docker.compose.project={project}").split()
    if not ids:
        raise RuntimeError(f"Compose project has no running containers: {project}")
    references = set()
    for container_id in ids:
        inspected = json.loads(command(runtime, "container", "inspect", container_id))[0]
        references.add(inspected["Config"]["Image"])
    return sorted(references)


def write_manifest(args: argparse.Namespace) -> None:
    references = sorted(set(args.image or project_references(args.runtime, args.project)))
    manifest = {
        "schema": 1,
        "source": source(Path(args.source_root).resolve()),
        "images": [image(args.runtime, reference) for reference in references],
    }
    Path(args.manifest).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def verify_manifest(args: argparse.Namespace) -> None:
    manifest = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
    if manifest.get("schema") != 1:
        raise RuntimeError("unsupported image-manifest schema")
    actual_source = source(Path(args.source_root).resolve())
    if manifest.get("source") != actual_source:
        raise RuntimeError(f"image manifest source mismatch: expected {manifest.get('source')}, got {actual_source}")
    for expected in manifest.get("images", []):
        actual = image(args.runtime, expected["reference"])
        if actual != expected:
            raise RuntimeError(f"image manifest mismatch for {expected['reference']}: expected {expected}, got {actual}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("write", "verify"))
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--source-root", required=True)
    parser.add_argument("--runtime", default="podman")
    parser.add_argument("--project")
    parser.add_argument("--image", action="append")
    args = parser.parse_args()
    if args.action == "write" and not args.image and not args.project:
        parser.error("write requires --image or --project")
    return args


def main() -> int:
    args = parse_args()
    if args.action == "write":
        write_manifest(args)
    else:
        verify_manifest(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
