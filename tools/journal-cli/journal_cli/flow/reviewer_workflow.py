#!/usr/bin/python3
"""Inert reviewer lane for the frozen sync-source-flow interface."""
from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import re
import sys
from typing import Any, Mapping, Sequence

from . import workflow_common as common

RECEIPT_SCHEMA = "sync-source-flow-review-receipt-v1"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
BUNDLE_REF = re.compile(r"^refs/[A-Za-z0-9][A-Za-z0-9._/-]{0,255}$")
SAFE_ROUTE = re.compile(r"^[A-Za-z0-9._-]{1,128}$")
MAX_BUNDLE_HEADER = 65_536
GIT_TIMEOUT = 120.0


def _fail(code: str, message: str) -> None:
    raise common.WorkflowError(code, message)


def _route(value: str) -> list[str]:
    if "\n" in value or "\r" in value:
        _fail("invalid-route", "route must be one canonical JSON value")
    route = common.parse_canonical_json(value.encode("utf-8") + b"\n", maximum=65_536)
    if (not isinstance(route, list) or len(route) > 16
            or any(not isinstance(hop, str) or not SAFE_ROUTE.fullmatch(hop) or hop in {".", ".."} for hop in route)):
        _fail("invalid-route", "route must be a bounded conservative array")
    return route


def _safe_directory(path: Path, label: str, *, create: bool) -> Path:
    path = path.absolute()
    if not path.exists():
        if not create:
            _fail("unsafe-filesystem", f"{label} does not exist")
        return common.make_exclusive_dir(path)
    common.require_safe_parent(path / ".sync-source-flow-boundary")
    info = os.lstat(path)
    if (not os.path.isdir(path) or os.path.islink(path) or info.st_uid != os.geteuid()
            or info.st_mode & 0o077):
        _fail("unsafe-filesystem", f"{label} is not a private owner directory")
    return path


def _write_receipt(path: Path, value: Mapping[str, Any]) -> None:
    common.write_exclusive(path, common.canonical_json(dict(value)))


def _source_pull_once(
    reference: Path, route: list[str], package: Path, workspace: Path,
) -> common.SourceResult:
    result = common.source_pull_once(
        reference=reference,
        route=route,
        destination=package,
        receipt=workspace / "source-pull-receipt.json",
        evidence_dir=workspace,
    )
    states = result.value.get("states")
    if (result.returncode != 0 or result.value.get("outcome") != "materialized"
            or not isinstance(states, dict) or states.get("installed") is not False
            or states.get("executed") is not False or states.get("materialized") is not True):
        _fail("source-pull", "one fixed Source pull did not materialize inertly")
    return result


def _expected_handoff(
    path: Path, review: dict[str, Any], review_sha: str, reference: bytes,
    package: Path, pull: common.SourceResult,
) -> None:
    handoff = common.validate_expected_handoff(common.load_canonical_json(path))
    artifact = handoff["fixedReference"]["artifact"]
    if artifact["bytes"] != len(reference) or artifact["sha256"] != hashlib.sha256(reference).hexdigest():
        _fail("handoff-mismatch", "fixed reference bytes differ from expected handoff")
    fixed = dict(handoff["fixedReference"])
    del fixed["artifact"]
    if pull.value.get("fixedReference") != fixed:
        _fail("handoff-mismatch", "Source fixed reference differs from expected handoff")
    if handoff["review"] != {"sha256": review_sha, "value": review}:
        _fail("handoff-mismatch", "REVIEW.json differs from expected handoff")
    _manifest_bytes, manifest_sha = common.sha256_file(package / "SHA256SUMS")
    expected_package = {
        "aggregateBytes": pull.value.get("aggregateBytes"),
        "chunks": pull.value.get("chunks"),
        "entries": pull.value.get("entries"),
        "manifestSha256": manifest_sha,
        "treeSha256": pull.value.get("treeSha256"),
    }
    if handoff["package"] != expected_package:
        _fail("handoff-mismatch", "pulled package differs from expected handoff")


def _bundle_header(data: bytes) -> tuple[list[str], tuple[str, str]]:
    marker = data.find(b"\n\n", 0, MAX_BUNDLE_HEADER + 2)
    if marker < 0:
        _fail("invalid-bundle", "bundle header is missing or oversized")
    try:
        lines = data[:marker].decode("utf-8").split("\n")
    except UnicodeDecodeError as error:
        raise common.WorkflowError("invalid-bundle", "bundle header is not UTF-8") from error
    if not lines or lines[0] not in {"# v2 git bundle", "# v3 git bundle"}:
        _fail("invalid-bundle", "bundle version is unsupported")
    prerequisites: list[str] = []
    refs: list[tuple[str, str]] = []
    for line in lines[1:]:
        if line.startswith("@"):
            if line != "@object-format=sha1":
                _fail("invalid-bundle", "bundle capability is unsupported")
        elif line.startswith("-"):
            oid = line[1:].split(" ", 1)[0]
            if not HEX40.fullmatch(oid):
                _fail("invalid-bundle", "bundle prerequisite is malformed")
            prerequisites.append(oid)
        else:
            parts = line.split(" ", 1)
            if len(parts) != 2 or not HEX40.fullmatch(parts[0]):
                _fail("invalid-bundle", "bundle advertised object is malformed")
            ref = parts[1]
            if (not BUNDLE_REF.fullmatch(ref) or ".." in ref or "@{" in ref
                    or ref.endswith(("/", "."))):
                _fail("invalid-bundle", "bundle advertised ref is unsafe")
            refs.append((parts[0], ref))
    if len(prerequisites) != len(set(prerequisites)) or len(refs) != 1:
        _fail("invalid-bundle", "bundle prerequisites or heads are ambiguous")
    return prerequisites, refs[0]


def _clean_git_command(
    environment: Mapping[str, str], options: Sequence[str], arguments: Sequence[str],
) -> list[str]:
    controlled = dict(environment)
    controlled.update({
        "PATH": "/usr/bin:/bin",
        "LC_ALL": "C",
        "LANG": "C",
        "GIT_ATTR_NOSYSTEM": "1",
        "GIT_NO_REPLACE_OBJECTS": "1",
    })
    forbidden = {
        "GIT_CONFIG", "GIT_CONFIG_SYSTEM", "GIT_CONFIG_COUNT", "GIT_DIR",
        "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_NAMESPACE", "GIT_REPLACE_REF_BASE",
        "GIT_SHALLOW_FILE", "GIT_EXEC_PATH", "GIT_PROXY_COMMAND", "LD_PRELOAD",
        "LD_LIBRARY_PATH",
    }
    if forbidden & set(controlled):
        _fail("unsafe-git", "isolated Git environment contains an injection variable")
    assignments = [f"{key}={controlled[key]}" for key in sorted(controlled)]
    return ["/usr/bin/env", "-i", *assignments, "/usr/bin/git", *options, *arguments]


def _git(
    workspace: Path, label: str, environment: Mapping[str, str], options: Sequence[str],
    arguments: Sequence[str], *, ok: tuple[int, ...] = (0,),
) -> common.ProcessResult:
    result = common.run_bounded(
        _clean_git_command(environment, options, arguments),
        cwd=Path("/"), timeout=GIT_TIMEOUT,
    )
    evidence = workspace / "commands"
    common.write_exclusive(evidence / f"{label}.stdout", result.stdout)
    common.write_exclusive(evidence / f"{label}.stderr", result.stderr)
    common.write_exclusive(
        evidence / f"{label}.json",
        common.canonical_json({
            "command": list(result.command), "label": label,
            "returncode": result.returncode,
            "stderrSha256": hashlib.sha256(result.stderr).hexdigest(),
            "stdoutSha256": hashlib.sha256(result.stdout).hexdigest(),
        }),
    )
    if result.returncode not in ok:
        _fail("git-failure", f"{label} failed with status {result.returncode}")
    return result


def _cache_invariants(
    cache: Path, workspace: Path, environment: Mapping[str, str], options: Sequence[str],
) -> None:
    prefix = ["--git-dir", str(cache)]
    bare = _git(workspace, "cache-bare", environment, options, [*prefix, "rev-parse", "--is-bare-repository"])
    if bare.stdout != b"true\n":
        _fail("unsafe-cache", "cache is not a bare repository")
    object_format = _git(
        workspace, "cache-format", environment, options,
        [*prefix, "rev-parse", "--show-object-format"],
    )
    if object_format.stdout != b"sha1\n":
        _fail("unsafe-cache", "cache object format is not sha1")
    remotes = _git(workspace, "cache-remotes", environment, options, [*prefix, "remote"])
    if remotes.stdout:
        _fail("unsafe-cache", "cache has a remote")
    refs = _git(
        workspace, "cache-refs", environment, options,
        [*prefix, "for-each-ref", "--format=%(refname)"],
    )
    for ref in refs.stdout.decode("ascii").splitlines():
        if not re.fullmatch(r"refs/sync-source-flow/candidates/[0-9a-f]{40}", ref):
            _fail("unsafe-cache", "cache has an unexpected ref")
    config = _git(
        workspace, "cache-config", environment, options,
        [*prefix, "config", "--local", "--name-only", "--list"],
    )
    allowed_config = {
        "core.repositoryformatversion", "core.filemode", "core.bare",
        "extensions.objectformat",
    }
    config_names = set(config.stdout.decode("utf-8").splitlines())
    if not config_names <= allowed_config:
        _fail("unsafe-cache", "cache has executable, remote, or unsupported configuration")
    for path, label in (
        (cache / "objects/info/alternates", "alternates"),
        (cache / "info/grafts", "grafts"),
        (cache / "shallow", "shallow boundary"),
    ):
        if os.path.lexists(path):
            _fail("unsafe-cache", f"cache has {label}")
    pack_directory = cache / "objects/pack"
    try:
        pack_entries = list(os.scandir(pack_directory))
    except OSError as error:
        raise common.WorkflowError("unsafe-cache", "cache pack directory cannot be inspected") from error
    if any(entry.name.endswith(".promisor") for entry in pack_entries):
        _fail("unsafe-cache", "cache has promisor object state")


def _inventory_lines(path: Path, label: str) -> list[str]:
    data = common.read_held(path, maximum=536_870_912)
    if not data.endswith(b"\n") or b"\r" in data or b"\x00" in data:
        _fail("invalid-inventory", f"{label} must be LF terminated")
    try:
        text = data[:-1].decode("utf-8")
    except UnicodeDecodeError as error:
        raise common.WorkflowError("invalid-inventory", f"{label} is not UTF-8") from error
    return [] if not text else text.split("\n")


def _git_path(value: str, label: str) -> str:
    if (not value or value.startswith("/") or any(character in value for character in "\x00\n\r")
            or any(part in {"", ".", ".."} for part in value.split("/"))):
        _fail("invalid-path", f"{label} contains an unsafe Git path")
    return value


def _nul_paths(data: bytes, label: str) -> list[str]:
    if data and not data.endswith(b"\x00"):
        _fail("git-output", f"{label} is not NUL terminated")
    result: list[str] = []
    for raw in data[:-1].split(b"\x00") if data else []:
        try:
            result.append(_git_path(raw.decode("utf-8"), label))
        except UnicodeDecodeError as error:
            raise common.WorkflowError("invalid-path", f"{label} is not UTF-8") from error
    return result


def _materialize_git(
    review: dict[str, Any], package: Path, cache_root: Path, workspace: Path,
) -> tuple[Path, str]:
    git = review["git"]
    if git["kind"] == "correction" and git["commitCount"] != 1:
        _fail("invalid-review", "correction must contain exactly one commit")
    bundle = package / git["bundle"]
    bundle_data = common.read_held(bundle, maximum=536_870_912)
    prerequisites, (advertised_oid, advertised_ref) = _bundle_header(bundle_data)
    if prerequisites != [git["prerequisite"]]:
        _fail("bundle-prerequisite", "bundle prerequisites differ from REVIEW.json")
    if advertised_oid != git["head"]:
        _fail("bundle-head", "bundle advertised head differs from REVIEW.json")

    repository_id = review["repository"]["id"]
    cache = cache_root / f"{repository_id}.git"
    environment, options = common.isolated_git_environment(workspace)
    commands = common.make_exclusive_dir(workspace / "commands")
    del commands
    if not cache.exists():
        common.require_safe_parent(cache)
        _git(
            workspace, "cache-init", environment, options,
            ["init", "--bare", "--object-format=sha1", str(cache)],
        )
        os.chmod(cache, 0o700)
    else:
        _safe_directory(cache, "cache repository", create=False)
    _cache_invariants(cache, workspace, environment, options)
    prefix = ["--git-dir", str(cache)]

    present = _git(
        workspace, "prerequisite", environment, options,
        [*prefix, "rev-parse", "--verify", "--quiet", f"{git['prerequisite']}^{{commit}}"],
        ok=(0, 1),
    )
    if present.returncode != 0:
        _fail("missing-prerequisite", "bundle prerequisite is absent from cache")

    _git(workspace, "bundle-verify", environment, options, [*prefix, "bundle", "verify", str(bundle)])
    destination_ref = f"refs/sync-source-flow/candidates/{git['head']}"
    existing = _git(
        workspace, "candidate-ref", environment, options,
        [*prefix, "show-ref", "--verify", "--quiet", destination_ref], ok=(0, 1),
    )
    if existing.returncode == 0:
        oid = _git(
            workspace, "candidate-value", environment, options,
            [*prefix, "rev-parse", "--verify", destination_ref],
        )
        if oid.stdout != (git["head"] + "\n").encode():
            _fail("cache-ref-conflict", "candidate cache ref differs")
    else:
        _git(
            workspace, "bundle-import", environment, options,
            [*prefix, "fetch", "--no-tags", "--no-write-fetch-head", str(bundle),
             f"{advertised_ref}:{destination_ref}"],
        )

    for oid, label in ((git["base"], "base"), (git["head"], "head")):
        _git(
            workspace, f"{label}-present", environment, options,
            [*prefix, "cat-file", "-e", f"{oid}^{{commit}}"],
        )
    _git(
        workspace, "ancestry", environment, options,
        [*prefix, "merge-base", "--is-ancestor", git["base"], git["head"]],
    )
    tree = _git(
        workspace, "head-tree", environment, options,
        [*prefix, "rev-parse", f"{git['head']}^{{tree}}"],
    )
    if tree.stdout != (git["tree"] + "\n").encode():
        _fail("tree-mismatch", "head tree differs from REVIEW.json")

    commits = _git(
        workspace, "commit-list", environment, options,
        [*prefix, "rev-list", "--reverse", f"{git['base']}..{git['head']}"],
    ).stdout.decode("ascii").splitlines()
    if len(commits) != git["commitCount"] or any(not HEX40.fullmatch(oid) for oid in commits):
        _fail("commit-count", "commit count differs from REVIEW.json")
    rows: list[str] = []
    for index, oid in enumerate(commits):
        row = _git(
            workspace, f"commit-{index}", environment, options,
            [*prefix, "show", "-s", "--format=%H %T %s", oid],
        ).stdout.decode("utf-8").rstrip("\n")
        if "\n" in row or "\r" in row:
            _fail("commit-inventory", "commit row is multiline")
        rows.append(row)
    if rows != _inventory_lines(package / git["commits"], "COMMITS.txt"):
        _fail("commit-inventory", "COMMITS.txt differs from Git objects")

    changed = _git(
        workspace, "changed-files", environment, options,
        [*prefix, "diff", "--name-only", "-z", git["base"], git["head"]],
    )
    changed_paths = _nul_paths(changed.stdout, "changed files")
    expected_changed = _inventory_lines(package / git["changedFiles"], "CHANGED-FILES.txt")
    if expected_changed != sorted(expected_changed, key=lambda item: item.encode("utf-8")):
        _fail("changed-files", "CHANGED-FILES.txt is not byte sorted")
    for path in expected_changed:
        _git_path(path, "CHANGED-FILES.txt")
    if changed_paths != expected_changed:
        _fail("changed-files", "CHANGED-FILES.txt differs from Git diff")

    tree_entries = _git(
        workspace, "tree-modes", environment, options,
        [*prefix, "ls-tree", "-rz", "-r", git["head"]],
    )
    for entry in tree_entries.stdout.split(b"\x00"):
        if not entry:
            continue
        try:
            metadata, raw_path = entry.split(b"\t", 1)
            mode, object_type, _oid = metadata.split(b" ", 2)
            tree_path = raw_path.decode("utf-8")
        except (ValueError, UnicodeDecodeError) as error:
            raise common.WorkflowError("unsafe-tree", "tree entry is malformed") from error
        _git_path(tree_path, "head tree")
        if any(part.lower() == ".git" for part in tree_path.split("/")):
            _fail("unsafe-tree", "tree contains a reserved .git component")
        if mode in {b"120000", b"160000"}:
            _fail("unsafe-tree", f"tree contains prohibited mode {mode.decode()}")
        if mode not in {b"100644", b"100755"} or object_type != b"blob":
            _fail("unsafe-tree", "tree contains an unsupported object")

    worktree = workspace / "worktree"
    if os.path.lexists(worktree):
        _fail("destination-exists", "review worktree already exists")
    _git(
        workspace, "worktree-add", environment, options,
        [*prefix, "worktree", "add", "--detach", str(worktree), git["head"]],
    )
    status = _git(
        workspace, "worktree-status", environment, options,
        ["-C", str(worktree), "status", "--porcelain=v1", "--untracked-files=all"],
    )
    if status.stdout:
        _fail("dirty-worktree", "detached review worktree is not clean")
    return worktree, destination_ref


def run_reviewer(
    reference: Path, route: list[str], workspace: Path, cache_root: Path,
    expected_handoff: Path | None,
) -> dict[str, Any]:
    paths = [reference, workspace, cache_root, *([expected_handoff] if expected_handoff is not None else [])]
    if any(not path.is_absolute() for path in paths):
        _fail("absolute-path-required", "reviewer paths must be absolute")
    workspace_text, cache_text = str(workspace), str(cache_root)
    try: overlap = os.path.commonpath((workspace_text, cache_text)) in {workspace_text, cache_text}
    except ValueError as error: raise common.WorkflowError("invalid-path", "workspace/cache roots differ") from error
    if overlap:
        _fail("path-overlap", "workspace and cache roots must not overlap")
    reference_bytes = common.read_held(reference, maximum=65_536)
    workspace = common.make_exclusive_dir(workspace)
    receipt_path = workspace / "review-receipt.json"
    phase = "source-pull"
    receipt: dict[str, Any] = {
        "schema": RECEIPT_SCHEMA,
        "outcome": "running",
        "phase": phase,
        "referenceSha256": hashlib.sha256(reference_bytes).hexdigest(),
        "route": route,
        "workspace": str(workspace),
        "installed": False,
        "executed": False,
    }
    try:
        package = workspace / "package"
        pull = _source_pull_once(reference, route, package, workspace)
        phase = "package-verification"
        review, observed = common.validate_review_package(package)
        review_count, review_sha = common.sha256_file(package / "REVIEW.json")
        del review_count
        if expected_handoff is not None:
            _expected_handoff(
                expected_handoff.absolute(), review, review_sha, reference_bytes,
                package, pull,
            )
        phase = "git-materialization"
        cache_root = _safe_directory(cache_root.absolute(), "cache root", create=True)
        worktree, cache_ref = _materialize_git(review, package, cache_root, workspace)
        git = review["git"]
        receipt.update({
            "outcome": "accepted",
            "phase": "complete",
            "repository": review["repository"],
            "git": {
                "base": git["base"], "bundleSha256": git["bundleSha256"],
                "cacheRef": cache_ref, "head": git["head"], "kind": git["kind"],
                "prerequisite": git["prerequisite"], "tree": git["tree"],
            },
            "package": {
                "files": len(observed) + 1,
                "reviewSha256": review_sha,
                "sourceTreeSha256": pull.value.get("treeSha256"),
            },
            "sourceReceipt": str(workspace / "source-pull-receipt.json"),
            "worktree": str(worktree),
        })
        _write_receipt(receipt_path, receipt)
        return receipt
    except common.WorkflowError as error:
        receipt.update({
            "outcome": "rejected", "phase": phase,
            "error": {"code": error.code, "message": str(error)},
        })
        _write_receipt(receipt_path, receipt)
        raise
    except (KeyboardInterrupt, SystemExit):
        receipt.update({
            "outcome": "interrupted", "phase": phase,
            "error": {"code": "interrupted", "message": "review operation interrupted"},
        })
        _write_receipt(receipt_path, receipt)
        raise
    except Exception as error:
        receipt.update({
            "outcome": "failed", "phase": phase,
            "error": {"code": "internal-failure", "message": type(error).__name__},
        })
        _write_receipt(receipt_path, receipt)
        raise common.WorkflowError("internal-failure", f"review failed during {phase}") from error


def main(arguments: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="sync-source-flow reviewer")
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--route", required=True)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--expected-handoff", type=Path)
    args = parser.parse_args(arguments)
    try:
        receipt = run_reviewer(
            args.reference, _route(args.route), args.workspace, args.cache,
            args.expected_handoff,
        )
        git = receipt["git"]
        sys.stdout.buffer.write(common.concise_json_line({
            "head": git["head"], "outcome": "verified", "tree": git["tree"],
            "worktree": receipt["worktree"],
        }))
        return 0
    except common.WorkflowError as error:
        sys.stdout.buffer.write(common.concise_json_line({
            "error": error.code, "message": str(error), "outcome": "not-completed",
        }))
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
