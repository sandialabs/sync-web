#!/usr/bin/python3
"""Producer lane for the frozen sync-source-flow shared interface v1."""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import uuid
from typing import Any

from ..source.sync_source_model import canonical_endpoint
from .workflow_common import (
    WorkflowError, canonical_json, concise_json_line, isolated_git_environment,
    load_canonical_json, make_exclusive_dir, parse_canonical_json, read_held, require_safe_parent,
    run_bounded, sha256_file, source_launcher_once, validate_expected_handoff, validate_hex40, validate_hex64,
    validate_relpath, validate_review_package, write_exclusive,
)

VERSION = "sync-source-flow-producer-v0.1.1"
PLAN_SCHEMA = "journal-cli-source-flow-producer-plan-v2"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
SAFE_LABEL = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
SAFE_ROUTE = re.compile(r"^[A-Za-z0-9._-]{1,128}$")
MAX_BUNDLE_HEADER = 1_048_576
FORBIDDEN_CONFIG = (
    "alias.", "credential.", "filter.", "include.", "includeif.", "submodule.",
    "url.", "http.", "https.", "ssh.", "gpg.", "lfs.",
)
FORBIDDEN_EXACT_CONFIG = {
    "core.attributesfile", "core.fsmonitor", "core.hookspath", "core.sshcommand",
    "diff.external", "extensions.partialclone", "fetch.fsckobjects", "protocol.allow", "uploadpack.packobjectshook",
}


def _exact(value: object, keys: set[str], field: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        raise WorkflowError("invalid-plan", f"{field} fields differ")
    return value


def _absolute(value: object, field: str) -> Path:
    if not isinstance(value, str) or not value.startswith("/") or "\x00" in value:
        raise WorkflowError("invalid-plan", f"{field} must be an absolute path")
    return Path(value)


def _label(value: object, field: str) -> str:
    if not isinstance(value,str) or not 1<=len(value.encode("utf-8"))<=128 or any(ord(char)<32 or ord(char)==127 for char in value):
        raise WorkflowError("invalid-plan",f"{field} is not a bounded one-line label")
    return value


def _route(value: object, field: str) -> list[str]:
    if not isinstance(value, list) or len(value) > 16 or any(not isinstance(item, str) or not SAFE_ROUTE.fullmatch(item) or item in {".", ".."} for item in value):
        raise WorkflowError("invalid-plan", f"{field} is not a conservative route")
    return list(value)


def _git_path(value: object, field: str = "Git path") -> str:
    if not isinstance(value,str) or not value or value.startswith("/") or len(value.encode("utf-8"))>4096:
        raise WorkflowError("invalid-path",f"{field} is not a bounded relative path")
    parts=value.split("/")
    if any(not part or part in {".",".."} or len(part.encode("utf-8"))>255 for part in parts) or any(ord(char)<32 or ord(char)==127 for char in value):
        raise WorkflowError("invalid-path",f"{field} has an unsafe component")
    return value


def _paths(value: object, field: str) -> list[str]:
    if not isinstance(value, list) or not value:
        raise WorkflowError("invalid-plan", f"{field} must be a nonempty path array")
    paths = [_git_path(item,field) for item in value]
    if paths != sorted(set(paths), key=lambda item: item.encode("utf-8")):
        raise WorkflowError("invalid-plan", f"{field} must be unique and byte-sorted")
    return paths


def validate_plan(value: Any) -> dict[str, Any]:
    plan = _exact(value, {"evidence", "git", "reviewers", "schema", "source"}, "plan")
    if plan["schema"] != PLAN_SCHEMA:
        raise WorkflowError("invalid-plan", "producer plan schema differs")
    git_plan = _exact(plan["git"], {"allowedChangedPaths", "base", "head", "kind", "prerequisite", "repo", "repositoryId"}, "git")
    base = validate_hex40(git_plan["base"], "git.base")
    prerequisite = validate_hex40(git_plan["prerequisite"], "git.prerequisite")
    validate_hex40(git_plan["head"], "git.head")
    if git_plan["kind"] not in {"full", "correction"} or base != prerequisite:
        raise WorkflowError("invalid-plan", "git kind/base/prerequisite differ")
    _absolute(git_plan["repo"], "git.repo")
    if not isinstance(git_plan["repositoryId"], str) or not SAFE_LABEL.fullmatch(git_plan["repositoryId"]):
        raise WorkflowError("invalid-plan", "git.repositoryId differs")
    _paths(git_plan["allowedChangedPaths"], "git.allowedChangedPaths")
    evidence = plan["evidence"]
    if not isinstance(evidence, list):
        raise WorkflowError("invalid-plan", "evidence must be an array")
    package_names: list[str] = []
    for item in evidence:
        entry = _exact(item, {"packagePath", "source"}, "evidence entry")
        package_name = validate_relpath(entry["packagePath"])
        if "/" in package_name:
            raise WorkflowError("invalid-plan", "evidence package path must be flat")
        package_names.append(package_name)
        _absolute(entry["source"], "evidence.source")
    if package_names != sorted(set(package_names), key=lambda item: item.encode("ascii")):
        raise WorkflowError("invalid-plan", "evidence package paths must be unique and byte-sorted")
    reserved = {"REVIEW.json", "SHA256SUMS", "CHANGED-FILES.txt", "COMMITS.txt", "candidate.bundle"}
    if reserved.intersection(package_names):
        raise WorkflowError("invalid-plan", "evidence path collides with package files")
    reviewers = plan["reviewers"]
    if not isinstance(reviewers, list):
        raise WorkflowError("invalid-plan", "reviewers must be an array")
    principals: list[tuple[str, ...]] = []
    for item in reviewers:
        reviewer = _exact(item, {"principal"}, "reviewer")
        principal = reviewer["principal"]
        if (not isinstance(principal, list) or not principal or any(not isinstance(part, str) or (part != "*state*" and not SAFE_ROUTE.fullmatch(part)) for part in principal)):
            raise WorkflowError("invalid-plan", "reviewer principal differs")
        principals.append(tuple(principal))
    if len(principals) != len(set(principals)):
        raise WorkflowError("invalid-plan", "reviewer principals must be unique")
    source = _exact(plan["source"], {"endpoint", "launcher", "owner", "projectLabel", "releaseLabel", "route", "settleSeconds"}, "source")
    if not isinstance(source["owner"],str) or not SAFE_LABEL.fullmatch(source["owner"]):
        raise WorkflowError("invalid-plan","source.owner differs")
    _label(source["projectLabel"],"source.projectLabel"); _label(source["releaseLabel"],"source.releaseLabel")
    try:
        canonical_endpoint(source["endpoint"])
    except Exception as exc:
        raise WorkflowError("invalid-plan", "source.endpoint differs") from exc
    _route(source["route"], "source.route")
    launcher = _exact(source["launcher"], {"path", "sha256"}, "source.launcher")
    _absolute(launcher["path"], "source.launcher.path"); validate_hex64(launcher["sha256"], "source.launcher.sha256")
    settle = source["settleSeconds"]
    if not isinstance(settle, (int, float)) or isinstance(settle, bool) or not math.isfinite(settle) or not 0 <= settle <= 30:
        raise WorkflowError("invalid-plan", "source.settleSeconds differs")
    return plan


def _git_context(operation: Path) -> tuple[dict[str, str], tuple[str, ...]]:
    return isolated_git_environment(operation)


def _git(operation: Path, repo: Path, arguments: list[str], *, allowed=(0,), timeout=120.0):
    environment, options = _git_context(operation)
    command = ["/usr/bin/git", "--no-replace-objects", *options, "-C", str(repo), *arguments]
    result = run_bounded(command, environment=environment, timeout=timeout)
    if result.returncode not in allowed:
        detail = result.stderr.decode("utf-8", errors="replace").strip().splitlines()
        raise WorkflowError("git-failed", detail[-1] if detail else f"Git failed: {arguments[0]}")
    return result


def _config_keys(operation: Path, repo: Path) -> list[str]:
    result = _git(operation, repo, ["config", "--local", "--no-includes", "--name-only", "--list"])
    try:
        keys = result.stdout.decode("utf-8", errors="strict").splitlines()
    except UnicodeError as exc:
        raise WorkflowError("unsafe-git-config", "repository config keys are not UTF-8") from exc
    for key in keys:
        lowered = key.lower()
        if (lowered in FORBIDDEN_EXACT_CONFIG or any(lowered.startswith(prefix) for prefix in FORBIDDEN_CONFIG)
                or lowered.endswith(".promisor") or lowered.endswith(".partialclonefilter") or lowered.endswith(".uploadpack")
                or lowered.startswith("diff.") and lowered.endswith((".textconv",".command"))):
            raise WorkflowError("unsafe-git-config", f"repository has executable/network Git config: {key}")
    return keys


def _prepare_staging(operation: Path, repo: Path) -> Path:
    require_safe_parent(repo/".sync-source-flow-boundary")
    require_safe_parent(repo/".git"/".sync-source-flow-boundary")
    _config_keys(operation, repo)
    for unsafe in (repo/".git"/"objects"/"info"/"alternates", repo/".git"/"info"/"grafts", repo/".git"/"shallow"):
        try: os.lstat(unsafe)
        except FileNotFoundError: pass
        else: raise WorkflowError("unsafe-git", f"source repository has {unsafe.name} indirection")
    shallow = _git(operation, repo, ["rev-parse", "--is-shallow-repository"])
    if shallow.stdout != b"false\n":
        raise WorkflowError("unsafe-git", "source repository is shallow")
    status = _git(operation, repo, ["status", "--porcelain=v1", "--untracked-files=all"])
    if status.stdout:
        raise WorkflowError("dirty-worktree", "repository worktree is not clean")
    staging = operation / "git-staging.git"
    empty_template = operation / "empty-template"
    make_exclusive_dir(empty_template)
    environment, options = _git_context(operation)
    result = run_bounded([
        "/usr/bin/git", "--no-replace-objects", *options, "clone", "--bare", "--local", "--no-hardlinks", "--no-tags",
        f"--template={empty_template}", str(repo), str(staging),
    ], environment=environment, timeout=300)
    if result.returncode != 0:
        raise WorkflowError("git-failed", "safe local Git staging failed")
    # The clone must be self-contained and have no network-capable configuration.
    alternates = staging / "objects" / "info" / "alternates"
    if alternates.exists():
        raise WorkflowError("unsafe-git", "staging repository has object alternates")
    _git(operation, staging, ["remote", "remove", "origin"], allowed=(0, 2))
    _config_keys(operation, staging)
    return staging


def _commit(operation: Path, repo: Path, object_id: str, field: str) -> str:
    validate_hex40(object_id, field)
    observed = _git(operation, repo, ["rev-parse", "--verify", f"{object_id}^{{commit}}"])
    try: value = observed.stdout.decode("ascii").strip()
    except UnicodeError as exc: raise WorkflowError("git-mismatch", f"{field} output differs") from exc
    if value != object_id:
        raise WorkflowError("git-mismatch", f"{field} does not resolve exactly")
    return value


def _tree_modes(operation: Path, repo: Path, head: str) -> None:
    result = _git(operation, repo, ["ls-tree", "-rz", "--full-tree", head])
    for record in result.stdout.split(b"\0"):
        if not record: continue
        try: metadata, raw_path = record.split(b"\t", 1); mode, kind, object_id = metadata.decode("ascii").split(" ")
        except Exception as exc: raise WorkflowError("invalid-tree", "Git tree record differs") from exc
        if mode in {"120000", "160000"} or kind == "commit":
            raise WorkflowError("unsafe-tree", f"Git symlink/gitlink is forbidden: {raw_path!r}")
        if mode not in {"100644", "100755"} or kind != "blob" or not HEX40.fullmatch(object_id):
            raise WorkflowError("unsafe-tree", "Git tree mode/type differs")
        try: path = raw_path.decode("utf-8", errors="strict")
        except UnicodeError as exc: raise WorkflowError("unsafe-tree", "Git tree path is not UTF-8") from exc
        _git_path(path)


def _changed_paths(operation: Path, repo: Path, boundary: str, head: str) -> list[str]:
    result = _git(operation, repo, ["diff", "--no-ext-diff", "--no-textconv", "--name-only", "--no-renames", "-z", f"{boundary}..{head}"])
    try: paths = [part.decode("utf-8", errors="strict") for part in result.stdout.split(b"\0") if part]
    except UnicodeError as exc: raise WorkflowError("invalid-tree", "changed path is not UTF-8") from exc
    paths = [_git_path(path) for path in paths]
    if paths != sorted(set(paths), key=lambda item: item.encode("utf-8")):
        raise WorkflowError("invalid-tree", "changed paths are not unique and byte-sorted")
    return paths


def _commits(operation: Path, repo: Path, boundary: str, head: str) -> tuple[list[str], bytes]:
    result = _git(operation, repo, ["rev-list", "--reverse", f"{boundary}..{head}"])
    ids = result.stdout.decode("ascii").splitlines()
    if not ids or any(not HEX40.fullmatch(item) for item in ids):
        raise WorkflowError("invalid-history", "commit inventory differs")
    rows: list[str] = []
    for object_id in ids:
        detail = _git(operation, repo, ["show", "-s", "--format=%T%x00%s", object_id]).stdout
        try: tree_raw, subject_raw = detail.rstrip(b"\n").split(b"\0", 1); tree = tree_raw.decode("ascii"); subject = subject_raw.decode("utf-8", errors="strict")
        except Exception as exc: raise WorkflowError("invalid-history", "commit detail differs") from exc
        validate_hex40(tree, "commit tree")
        if not subject or any(ord(character) < 32 or ord(character) == 127 for character in subject):
            raise WorkflowError("invalid-history", "commit subject is not one conservative line")
        rows.append(f"{object_id} {tree} {subject}")
    return ids, ("\n".join(rows) + "\n").encode("utf-8")


def _bundle_header(path: Path) -> tuple[list[str], list[tuple[str, str]]]:
    data = read_held(path, maximum=536_870_912)
    marker = data.find(b"\n\n")
    if marker < 0 or marker > MAX_BUNDLE_HEADER:
        raise WorkflowError("invalid-bundle", "bundle header bound differs")
    lines = data[:marker].splitlines()
    if not lines or lines[0] != b"# v2 git bundle":
        raise WorkflowError("invalid-bundle", "bundle signature differs")
    prerequisites: list[str] = []; heads: list[tuple[str, str]] = []
    for raw in lines[1:]:
        prerequisite = raw.startswith(b"-"); body = raw[1:] if prerequisite else raw
        try: object_raw, label_raw = body.split(b" ", 1); object_id = object_raw.decode("ascii"); label = label_raw.decode("utf-8", errors="strict")
        except Exception as exc: raise WorkflowError("invalid-bundle", "bundle advertisement differs") from exc
        if not HEX40.fullmatch(object_id) or not label or any(ord(c) < 32 or ord(c) == 127 for c in label):
            raise WorkflowError("invalid-bundle", "bundle advertisement value differs")
        if prerequisite: prerequisites.append(object_id)
        else: heads.append((object_id, label))
    if len(set(prerequisites)) != len(prerequisites) or len(set(heads)) != len(heads):
        raise WorkflowError("invalid-bundle", "bundle advertisement duplicates")
    return prerequisites, heads


def _write_package(operation: Path, plan: dict[str, Any], staging: Path) -> tuple[Path, dict[str, Any]]:
    git_plan = plan["git"]; base = _commit(operation, staging, git_plan["base"], "git.base"); head = _commit(operation, staging, git_plan["head"], "git.head")
    boundary = base
    ancestry = _git(operation, staging, ["merge-base", "--is-ancestor", boundary, head], allowed=(0, 1))
    if ancestry.returncode != 0: raise WorkflowError("invalid-history", "base is not an ancestor of head")
    tree = _git(operation, staging, ["rev-parse", "--verify", f"{head}^{{tree}}"] ).stdout.decode("ascii").strip(); validate_hex40(tree, "git.tree")
    _tree_modes(operation, staging, head)
    changed = _changed_paths(operation, staging, boundary, head)
    if changed != git_plan["allowedChangedPaths"]: raise WorkflowError("scope-mismatch", "allowed changed paths differ from Git")
    commit_ids, commits_bytes = _commits(operation, staging, boundary, head)
    if git_plan["kind"] == "correction" and len(commit_ids) != 1: raise WorkflowError("commit-count", "correction must contain exactly one commit")
    package = operation / "package"; make_exclusive_dir(package)
    changed_bytes = ("\n".join(changed) + "\n").encode("ascii")
    write_exclusive(package / "CHANGED-FILES.txt", changed_bytes); write_exclusive(package / "COMMITS.txt", commits_bytes)
    evidence_review: list[dict[str, Any]] = []
    for item in plan["evidence"]:
        data = read_held(Path(item["source"])); write_exclusive(package / item["packagePath"], data)
        evidence_review.append({"bytes": len(data), "path": item["packagePath"], "sha256": hashlib.sha256(data).hexdigest()})
    bundle = package / "candidate.bundle"; candidate_ref = "refs/sync-source-flow/candidate"
    _git(operation, staging, ["update-ref", candidate_ref, head])
    _git(operation, staging, ["bundle", "create", str(bundle), candidate_ref, f"^{boundary}"], timeout=300)
    prerequisites, heads = _bundle_header(bundle)
    if prerequisites != [boundary] or heads != [(head, candidate_ref)]:
        raise WorkflowError("invalid-bundle", "bundle advertised head/prerequisite differ")
    verify = _git(operation, staging, ["bundle", "verify", str(bundle)])
    if verify.returncode != 0: raise WorkflowError("invalid-bundle", "bundle verification failed")
    bundle_bytes, bundle_sha = sha256_file(bundle)
    review = {
        "evidence": evidence_review,
        "git": {
            "base": base, "bundle": "candidate.bundle", "bundleBytes": bundle_bytes, "bundleSha256": bundle_sha,
            "changedFiles": "CHANGED-FILES.txt", "changedFilesSha256": hashlib.sha256(changed_bytes).hexdigest(),
            "commitCount": len(commit_ids), "commits": "COMMITS.txt", "commitsSha256": hashlib.sha256(commits_bytes).hexdigest(),
            "head": head, "kind": git_plan["kind"], "prerequisite": git_plan["prerequisite"], "tree": tree,
        },
        "package": {"schema": 1}, "repository": {"id": git_plan["repositoryId"]}, "schema": "sync-source-flow-review-v1",
    }
    write_exclusive(package / "REVIEW.json", canonical_json(review))
    names = sorted([path.name for path in package.iterdir()], key=lambda item: item.encode("ascii"))
    rows = []
    for name in names:
        _count, file_sha = sha256_file(package / name); rows.append(f"{file_sha}  {name}\n")
    write_exclusive(package / "SHA256SUMS", "".join(rows).encode("ascii"))
    validate_review_package(package)
    return package, review


def _verify_launcher_binding(binding: dict[str, Any], field: str) -> None:
    launcher = Path(binding["path"])
    count, observed = sha256_file(launcher, maximum=1_000_000)
    info = os.stat(launcher, follow_symlinks=False)
    if count <= 0 or not stat.S_ISREG(info.st_mode) or not info.st_mode & stat.S_IXUSR:
        raise WorkflowError("unsupported-capability", "Source launcher is not an executable regular file")
    if observed != binding["sha256"]:
        raise WorkflowError("provenance-mismatch", f"{field} Source launcher SHA-256 differs")
    if os.environ.get("SYNC_SOURCE_FLOW_LAUNCHER") != str(launcher) or os.environ.get("SYNC_SOURCE_FLOW_LAUNCHER_SHA256") != observed:
        raise WorkflowError("provenance-mismatch", f"installed Source launcher binding differs from {field}")


def _verify_plan_launcher_binding(plan: dict[str, Any]) -> None:
    _verify_launcher_binding(plan["source"]["launcher"], "plan")


def _grant_plans(operation: Path, plan: dict[str, Any], project_id: str, release_id: str) -> list[dict[str, str]]:
    results = []
    for index, reviewer in enumerate(plan["reviewers"]):
        value = {"schema":"sync-source-flow-grant-plan-v1", "selected":False, "owner":plan["source"]["owner"],
                 "principal":reviewer["principal"],
                 "path":["source",project_id,"releases",release_id], "get":True,"resolve":True,"set":False}
        path = operation / "grant-plans" / f"reviewer-{index:03d}.json"; data=canonical_json(value); write_exclusive(path,data)
        results.append({"bytes":len(data),"path":str(path.relative_to(operation)),"sha256":hashlib.sha256(data).hexdigest()})
    return results


def _source_once(operation: Path, name: str, arguments: list[str]):
    return source_launcher_once(arguments,evidence_dir=operation/"source-evidence",label=name)


def _receipt_binding(path: Path) -> dict[str, Any]:
    count, value = sha256_file(path, maximum=8_388_608); return {"path":str(path),"bytes":count,"sha256":value}


def _phase(operation: Path, number: int, name: str, dispatches: int) -> None:
    write_exclusive(operation/f"phase-{number:02d}-{name}.json",canonical_json({"dispatches":dispatches,"phase":name,"schema":"journal-cli-source-flow-phase-v2"}))


def _validate_ready_inputs(value: object, route: list[str] | None = None) -> dict[str, Any]:
    if not isinstance(value,dict): raise WorkflowError("invalid-source-result","ready inputs must be an object")
    required={"endpoint","route","expectedDescriptorBytes","expectedDescriptorSha256","owner","readyMarkerPath"}
    if not required<=set(value): raise WorkflowError("invalid-source-result","ready input fields differ")
    count=value["expectedDescriptorBytes"]
    if not isinstance(count,int) or isinstance(count,bool) or not 1<=count<=536_870_912: raise WorkflowError("invalid-source-result","descriptor byte bound differs")
    validate_hex64(value["expectedDescriptorSha256"],"expected descriptor SHA-256")
    try: endpoint=canonical_endpoint(value["endpoint"])
    except Exception as exc: raise WorkflowError("invalid-source-result","ready endpoint differs") from exc
    if not isinstance(value["owner"],str) or not SAFE_LABEL.fullmatch(value["owner"]): raise WorkflowError("invalid-source-result","ready owner differs")
    marker=value["readyMarkerPath"]
    if not isinstance(marker,list) or not marker or any(not isinstance(item,str) or not SAFE_ROUTE.fullmatch(item) for item in marker):
        raise WorkflowError("invalid-source-result","ready marker path differs")
    selected_route=_route(route if route is not None else value.get("route"),"ready route")
    if selected_route != _route(value["route"], "ready embedded route"):
        raise WorkflowError("invalid-source-result","ready route differs")
    return {"endpoint":endpoint,"route":selected_route,
            "expectedDescriptorBytes":count,"expectedDescriptorSha256":value["expectedDescriptorSha256"],
            "owner":value["owner"],"readyMarkerPath":marker}


def _ready_once(operation: Path, ready_inputs: dict[str, Any], receipt: Path) -> tuple[int,dict[str,Any]|None]:
    arguments=["ready","--receipt",str(receipt),"--route",json.dumps(ready_inputs["route"],separators=(",",":")),"--owner",ready_inputs["owner"],
        "--ready-marker-path",json.dumps(ready_inputs["readyMarkerPath"],separators=(",",":")),
        "--expected-descriptor-bytes",str(ready_inputs["expectedDescriptorBytes"]),"--expected-descriptor-sha256",ready_inputs["expectedDescriptorSha256"]]
    result=_source_once(operation,"ready",arguments); return result.returncode,result.value


def _validate_ready_receipt(path: Path, ready: dict[str,Any]) -> None:
    value=load_canonical_json(path,maximum=8_388_608)
    if not isinstance(value,dict) or value.get("outcome")!="verified" or value.get("fixedReference")!=ready.get("fixedReference"):
        raise WorkflowError("invalid-source-result","canonical ready receipt binding differs")


def _artifact(operation: Path, path: Path) -> dict[str,Any]:
    count,value=sha256_file(path,maximum=8_388_608)
    return {"bytes":count,"path":str(path.relative_to(operation)),"sha256":value}


def _write_verified_handoff(operation: Path, ready: dict[str,Any], review: dict[str,Any], manifest_sha256: str,
                            publish_receipt_path: Path, ready_receipt_path: Path, grants: list[dict[str,Any]]) -> dict[str,Any]:
    encoded=ready.get("reference")
    try: reference=base64.b64decode(encoded,validate=True)
    except Exception as exc: raise WorkflowError("invalid-source-result","ready reference differs") from exc
    fixed=ready.get("fixedReference"); exact_fixed={"endpoint","descriptorBytes","descriptorPath","descriptorSha256","index","owner"}
    if not isinstance(fixed,dict) or set(fixed)!=exact_fixed: raise WorkflowError("invalid-source-result","fixed reference fields differ")
    ready_receipt=load_canonical_json(ready_receipt_path,maximum=8_388_608)
    operation_id=ready_receipt.get("operationId") if isinstance(ready_receipt,dict) else None
    if not isinstance(operation_id,str) or not re.fullmatch(r"[0-9a-f]{32}",operation_id): raise WorkflowError("invalid-source-result","ready operation ID differs")
    for field in ("aggregateBytes","chunks","entries"):
        if not isinstance(ready_receipt.get(field),int) or isinstance(ready_receipt.get(field),bool) or ready_receipt[field]<1:
            raise WorkflowError("invalid-source-result",f"ready package {field} differs")
    write_exclusive(operation/"fixed-reference.scm",reference)
    handoff={"fixedReference":{"artifact":_artifact(operation,operation/"fixed-reference.scm"),**fixed},
        "grantPlans":sorted(grants,key=lambda item:item["path"].encode("utf-8")),"operationId":operation_id,"outcome":"verified",
        "package":{"aggregateBytes":ready_receipt["aggregateBytes"],"chunks":ready_receipt["chunks"],"entries":ready_receipt["entries"],
                   "manifestSha256":manifest_sha256,"treeSha256":ready.get("treeSha256")},
        "phase":"ready","readyResume":None,"receipts":{"publish":_artifact(operation,publish_receipt_path),"ready":_artifact(operation,ready_receipt_path)},
        "review":{"sha256":hashlib.sha256(canonical_json(review)).hexdigest(),"value":review},"schema":"journal-cli-source-flow-handoff-v2",
        "states":{"executed":False,"installed":False}}
    validate_expected_handoff(handoff)
    write_exclusive(operation/"handoff.json",canonical_json(handoff)); return handoff


def _validate_resume(value: Any, resume_path: Path) -> tuple[dict[str,Any],Path,Path]:
    resume=_exact(value,{"launcher","publication","readyInputs","schema"},"ready resume")
    if resume["schema"]!="journal-cli-source-flow-ready-resume-v2": raise WorkflowError("invalid-resume","ready resume schema differs")
    launcher=_exact(resume["launcher"],{"path","sha256"},"resume launcher"); _absolute(launcher["path"],"launcher.path"); validate_hex64(launcher["sha256"],"launcher.sha256")
    publication=_exact(resume["publication"],{"operationId","projectId","publishReceipt","publishReceiptSha256","releaseId"},"publication")
    if not isinstance(publication["operationId"],str) or not re.fullmatch(r"[0-9a-f]{32}",publication["operationId"]): raise WorkflowError("invalid-resume","publication operation ID differs")
    for field in ("projectId","releaseId"):
        try: uuid.UUID(publication[field])
        except Exception as exc: raise WorkflowError("invalid-resume",f"publication {field} differs") from exc
    if publication["publishReceipt"]!="publish-receipt.json": raise WorkflowError("invalid-resume","publish receipt filename differs")
    validate_hex64(publication["publishReceiptSha256"],"publish receipt SHA-256")
    ready_inputs=_validate_ready_inputs(resume["readyInputs"])
    receipt=resume_path.absolute().parent/publication["publishReceipt"]
    receipt_bytes=read_held(receipt,maximum=8_388_608)
    if hashlib.sha256(receipt_bytes).hexdigest()!=publication["publishReceiptSha256"]: raise WorkflowError("invalid-resume","publish receipt hash differs")
    receipt_value=parse_canonical_json(receipt_bytes,maximum=8_388_608)
    if (not isinstance(receipt_value,dict) or receipt_value.get("outcome")!="ready-marker-write-accepted" or receipt_value.get("operationId")!=publication["operationId"]
            or receipt_value.get("releaseId")!=publication["releaseId"]): raise WorkflowError("invalid-resume","publish receipt outcome/identity differs")
    observed_inputs=_validate_ready_inputs(receipt_value.get("operationLocalReadyInputs"),ready_inputs["route"])
    if observed_inputs!=ready_inputs: raise WorkflowError("invalid-resume","publish receipt ready inputs differ")
    return resume,Path(launcher["path"]),receipt


def execute_ready(resume_path: Path, operation_root: Path) -> dict[str,Any]:
    resume,_launcher_evidence,publish_receipt=_validate_resume(load_canonical_json(resume_path),resume_path)
    _verify_launcher_binding(resume["launcher"], "resume")
    operation=make_exclusive_dir(operation_root); make_exclusive_dir(operation/"source-evidence"); _phase(operation,0,"ready-selected",0)
    write_exclusive(operation/"publish-receipt.json",read_held(publish_receipt,maximum=8_388_608))
    write_exclusive(operation/"ready-resume.json",canonical_json(resume))
    receipt=operation/"ready-receipt.json"
    try: code,ready=_ready_once(operation,resume["readyInputs"],receipt)
    finally: _phase(operation,10,"ready-returned",1)
    operation_id=str(uuid.uuid4())
    if code!=0 or not ready or ready.get("outcome")!="verified":
        handoff={"schema":"journal-cli-source-flow-handoff-v2","operationId":operation_id,"outcome":"not-ready","phase":"ready",
            "fixedReference":None,"source":{"treeSha256":None},"review":None,"reviewSha256":None,"packageSha256sums":None,
            "publishReceipt":_receipt_binding(operation/"publish-receipt.json"),"readyReceipt":_receipt_binding(receipt) if receipt.exists() else None,
            "readyResume":_receipt_binding(operation/"ready-resume.json"),"grantPlans":[],"installed":False,"executed":False}
        write_exclusive(operation/"handoff.json",canonical_json(handoff)); return handoff
    _validate_ready_receipt(receipt,ready)
    prior=resume_path.absolute().parent; review,_observed=validate_review_package(prior/"package")
    manifest_sha=sha256_file(prior/"package"/"SHA256SUMS")[1]
    prior_grants=sorted((prior/"grant-plans").glob("*.json")) if (prior/"grant-plans").exists() else []
    grants=[]
    if prior_grants:
        make_exclusive_dir(operation/"grant-plans")
        for source_path in prior_grants:
            destination=operation/"grant-plans"/source_path.name; write_exclusive(destination,read_held(source_path,maximum=1_048_576)); grants.append(_artifact(operation,destination))
    return _write_verified_handoff(operation,ready,review,manifest_sha,operation/"publish-receipt.json",receipt,grants)


def execute(plan_path: Path, operation_root: Path, *, dry_run: bool) -> dict[str, Any]:
    plan = validate_plan(load_canonical_json(plan_path)); operation = make_exclusive_dir(operation_root); _phase(operation,0,"started",0)
    write_exclusive(operation / "plan.json", canonical_json(plan)); _verify_plan_launcher_binding(plan)
    staging = _prepare_staging(operation, Path(plan["git"]["repo"])); package, review = _write_package(operation, plan, staging)
    project_id, release_id, operation_id = str(uuid.uuid4()), str(uuid.uuid4()), uuid.uuid4().hex
    make_exclusive_dir(operation/"grant-plans"); grants = _grant_plans(operation, plan, project_id, release_id); _phase(operation,10,"preflight-complete",0)
    package_manifest = _receipt_binding(package / "SHA256SUMS")
    base_summary = {"schema":"journal-cli-source-flow-producer-summary-v2","operationId":operation_id,"projectId":project_id,"releaseId":release_id,
                    "review":review,"reviewSha256":hashlib.sha256(canonical_json(review)).hexdigest(),"grantPlans":grants,
                    "packageSha256sums":package_manifest,"installed":False,"executed":False}
    if dry_run:
        summary = {**base_summary,"outcome":"preflight-verified","phase":"complete","externalDispatches":0}
        write_exclusive(operation / "summary.json", canonical_json(summary)); return summary
    source = plan["source"]; make_exclusive_dir(operation/"source-evidence"); receipt = operation / "publish-receipt.json"
    publish_args = ["publish","--source",str(package),"--receipt",str(receipt),"--owner",source["owner"],
        "--project-id",project_id,"--project",source["projectLabel"],"--release",source["releaseLabel"],"--release-id",release_id,
        "--route",json.dumps(source["route"],separators=(",",":")),"--settle-seconds",str(source["settleSeconds"])]
    _phase(operation,20,"publish-selected",0)
    try:
        published_result = _source_once(operation,"publish",publish_args); code,published=published_result.returncode,published_result.value
    finally: _phase(operation,30,"publish-returned",1)
    if code != 0 or not published or published.get("outcome") != "ready-marker-write-accepted":
        summary={**base_summary,"outcome":"publication-stopped","phase":"publish","externalDispatches":1,"publishReceipt":_receipt_binding(receipt) if receipt.exists() else None}
        write_exclusive(operation/"summary.json",canonical_json(summary)); return summary
    ready_inputs=_validate_ready_inputs(published.get("operationLocalReadyInputs"), source["route"])
    if ready_inputs["endpoint"] != source["endpoint"]:
        raise WorkflowError("invalid-source-result", "published endpoint differs from the approved plan")
    publish_receipt_value=load_canonical_json(receipt, maximum=8_388_608)
    if (not isinstance(publish_receipt_value,dict) or publish_receipt_value.get("outcome")!="ready-marker-write-accepted"
            or publish_receipt_value.get("releaseId")!=release_id or publish_receipt_value.get("operationLocalReadyInputs")!=published.get("operationLocalReadyInputs")):
        raise WorkflowError("invalid-source-result","canonical publish receipt binding differs")
    publish_operation_id=publish_receipt_value.get("operationId")
    if not isinstance(publish_operation_id,str) or not re.fullmatch(r"[0-9a-f]{32}",publish_operation_id):
        raise WorkflowError("invalid-source-result","publish operation ID differs")
    publish_receipt_sha=sha256_file(receipt,maximum=8_388_608)[1]
    resume={"launcher":source["launcher"],"publication":{"operationId":publish_operation_id,"projectId":project_id,
            "publishReceipt":"publish-receipt.json","publishReceiptSha256":publish_receipt_sha,"releaseId":release_id},
            "readyInputs":ready_inputs,"schema":"journal-cli-source-flow-ready-resume-v2"}
    write_exclusive(operation/"ready-resume.json",canonical_json(resume))
    ready_receipt=operation/"ready-receipt.json"
    _phase(operation,40,"ready-selected",1)
    try: code, ready = _ready_once(operation,ready_inputs,ready_receipt)
    finally: _phase(operation,50,"ready-returned",2)
    if code != 0 or not ready or ready.get("outcome") != "verified":
        handoff={"schema":"journal-cli-source-flow-handoff-v2","operationId":operation_id,"outcome":"not-ready","phase":"ready",
            "fixedReference":None,"source":{"treeSha256":None},"review":review,"reviewSha256":hashlib.sha256(canonical_json(review)).hexdigest(),
            "packageSha256sums":package_manifest,"publishReceipt":_receipt_binding(receipt),
            "readyReceipt":_receipt_binding(ready_receipt) if ready_receipt.exists() else None,"readyResume":_receipt_binding(operation/"ready-resume.json"),
            "grantPlans":grants,"installed":False,"executed":False}
        write_exclusive(operation/"handoff.json",canonical_json(handoff))
        summary={**base_summary,"outcome":"not-ready","phase":"ready","externalDispatches":2,"handoff":_receipt_binding(operation/"handoff.json"),
                 "readyResume":_receipt_binding(operation/"ready-resume.json")}
        write_exclusive(operation/"summary.json",canonical_json(summary)); return summary
    _validate_ready_receipt(ready_receipt,ready)
    handoff=_write_verified_handoff(operation,ready,review,package_manifest["sha256"],receipt,ready_receipt,grants)
    summary={**base_summary,"outcome":"ready","phase":"complete","externalDispatches":2,"handoff":_receipt_binding(operation/"handoff.json")}
    write_exclusive(operation/"summary.json",canonical_json(summary)); return summary


def main(arguments: list[str]) -> int:
    parser=argparse.ArgumentParser(prog="sync-source-flow producer")
    parser.add_argument("--plan",type=Path,required=True); parser.add_argument("--operation-root",type=Path,required=True); parser.add_argument("--dry-run",action="store_true")
    args=parser.parse_args(arguments)
    try:
        result=execute(args.plan,args.operation_root,dry_run=args.dry_run)
        concise={"outcome":result["outcome"],"head":result["review"]["git"]["head"],"tree":result["review"]["git"]["tree"],
                 "prerequisite":result["review"]["git"]["prerequisite"],"handoff":result.get("handoff",{}).get("path"),"operation":str(args.operation_root)}
        sys.stdout.buffer.write(concise_json_line(concise)); return 0 if result["outcome"] in {"ready","preflight-verified"} else 1
    except WorkflowError as error:
        dispatched = args.operation_root.exists() and any(args.operation_root.glob("phase-*-*-returned.json"))
        outcome = "external-stopped" if dispatched else "not-attempted"
        sys.stdout.buffer.write(concise_json_line({"error":error.code,"message":str(error),"outcome":outcome,"operation":str(args.operation_root)})); return 1


def ready_main(arguments: list[str]) -> int:
    parser=argparse.ArgumentParser(prog="sync-source-flow ready")
    parser.add_argument("--resume",type=Path,required=True); parser.add_argument("--operation-root",type=Path,required=True)
    args=parser.parse_args(arguments)
    try:
        result=execute_ready(args.resume,args.operation_root)
        fixed = result.get("fixedReference")
        sys.stdout.buffer.write(concise_json_line({"fixedReference":fixed.get("artifact",{}).get("path") if fixed else None,
            "handoff":str(args.operation_root.absolute()/"handoff.json"),"operation":str(args.operation_root),"outcome":result["outcome"]}))
        return 0 if result["outcome"]=="verified" else 1
    except WorkflowError as error:
        dispatched = args.operation_root.exists() and any(args.operation_root.glob("phase-*-*-returned.json"))
        outcome = "external-stopped" if dispatched else "not-attempted"
        sys.stdout.buffer.write(concise_json_line({"error":error.code,"message":str(error),"outcome":outcome,"operation":str(args.operation_root)})); return 1


if __name__=="__main__": raise SystemExit(main(sys.argv[1:]))
