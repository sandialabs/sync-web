#!/usr/bin/python3
"""Shared fail-closed primitives for owner-local Source workflow tooling."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import math
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import tempfile
from typing import Any, Mapping, Sequence

MAX_JSON_BYTES = 1_048_576
MAX_FILE_BYTES = 536_870_912
MAX_PROCESS_OUTPUT = 8_388_608
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
RELATIVE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,511}$")
REPOSITORY_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
EVIDENCE_LABEL = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
SOURCE_LAUNCHER_ENV = "SYNC_SOURCE_FLOW_LAUNCHER"
SOURCE_LAUNCHER_SHA_ENV = "SYNC_SOURCE_FLOW_LAUNCHER_SHA256"


class WorkflowError(RuntimeError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def _pairs(items: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in items:
        if key in result:
            raise WorkflowError("invalid-json", f"duplicate JSON field: {key}")
        result[key] = value
    return result


def canonical_json(value: Any) -> bytes:
    try:
        return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
    except (TypeError, ValueError) as exc:
        raise WorkflowError("invalid-json", "value cannot be encoded canonically") from exc


def parse_canonical_json(data: bytes, *, maximum: int = MAX_JSON_BYTES) -> Any:
    if not 0 < len(data) <= maximum:
        raise WorkflowError("invalid-json", "canonical JSON byte bound failed")
    try:
        value = json.loads(
            data,
            object_pairs_hook=_pairs,
            parse_constant=lambda _value: (_ for _ in ()).throw(WorkflowError("invalid-json", "nonfinite JSON number")),
        )
    except WorkflowError:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise WorkflowError("invalid-json", "JSON is malformed") from exc
    if canonical_json(value) != data:
        raise WorkflowError("noncanonical-json", "JSON bytes are not canonical")
    return value


def validate_relpath(value: object) -> str:
    if not isinstance(value, str) or not RELATIVE.fullmatch(value):
        raise WorkflowError("invalid-path", "package path is not conservative ASCII")
    parts = value.split("/")
    if any(part in {"", ".", ".."} for part in parts):
        raise WorkflowError("invalid-path", "package path contains an unsafe component")
    return value


def validate_hex40(value: object, field: str) -> str:
    if not isinstance(value, str) or not HEX40.fullmatch(value):
        raise WorkflowError("invalid-review", f"{field} must be lowercase 40-hex")
    return value


def validate_hex64(value: object, field: str) -> str:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise WorkflowError("invalid-review", f"{field} must be lowercase 64-hex")
    return value


def allowed_ancestor_owner(uid: int, euid: int) -> bool:
    return uid in {0, euid}


def _open_owner_directory(path: Path, *, writable: bool = False) -> int:
    path = path.absolute()
    current = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        for component in path.parts[1:]:
            try:
                child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=current)
            except OSError as exc:
                raise WorkflowError("unsafe-filesystem", f"directory ancestor cannot be opened safely: {component}") from exc
            info = os.fstat(child)
            if not stat.S_ISDIR(info.st_mode) or not allowed_ancestor_owner(info.st_uid, os.geteuid()):
                os.close(child)
                raise WorkflowError("unsafe-filesystem", f"directory ancestor has an unexpected owner: {component}")
            os.close(current)
            current = child
        info = os.fstat(current)
        if info.st_uid != os.geteuid():
            raise WorkflowError("unsafe-filesystem", "final directory is not owner-controlled")
        if info.st_mode & 0o022:
            raise WorkflowError("unsafe-filesystem", "final directory is group/world writable")
        result = current
        current = -1
        return result
    finally:
        if current >= 0:
            os.close(current)


def _open_regular(path: Path, maximum: int = MAX_FILE_BYTES) -> tuple[int, os.stat_result]:
    path = path.absolute()
    parent = _open_owner_directory(path.parent)
    try:
        fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
    finally:
        os.close(parent)
    info = os.fstat(fd)
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_nlink != 1 or info.st_mode & 0o022 or info.st_size > maximum:
        os.close(fd)
        raise WorkflowError("unsafe-file", f"unsafe regular file boundary: {path.name}")
    return fd, info


def read_held(path: Path, *, maximum: int = MAX_FILE_BYTES) -> bytes:
    fd, before = _open_regular(path, maximum)
    try:
        chunks: list[bytes] = []
        remaining = before.st_size
        while remaining:
            chunk = os.read(fd, min(131_072, remaining))
            if not chunk:
                raise WorkflowError("short-read", f"short read: {path.name}")
            chunks.append(chunk)
            remaining -= len(chunk)
        after = os.fstat(fd)
        fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns")
        if any(getattr(before, field) != getattr(after, field) for field in fields):
            raise WorkflowError("source-changed", f"file changed while held: {path.name}")
        return b"".join(chunks)
    finally:
        os.close(fd)


def sha256_file(path: Path, *, maximum: int = MAX_FILE_BYTES) -> tuple[int, str]:
    data = read_held(path, maximum=maximum)
    return len(data), hashlib.sha256(data).hexdigest()


def require_safe_parent(path: Path) -> Path:
    parent = path.absolute().parent
    fd = _open_owner_directory(parent, writable=True)
    os.close(fd)
    return parent


def make_exclusive_dir(path: Path) -> Path:
    path = path.absolute()
    parent = _open_owner_directory(path.parent, writable=True)
    try:
        try:
            os.mkdir(path.name, 0o700, dir_fd=parent)
        except FileExistsError as exc:
            raise WorkflowError("destination-exists", f"destination exists: {path}") from exc
        child = os.open(path.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
        try:
            info = os.fstat(child)
            if not stat.S_ISDIR(info.st_mode) or stat.S_IMODE(info.st_mode) != 0o700 or info.st_uid != os.geteuid():
                raise WorkflowError("unsafe-filesystem", "exclusive directory invariant failed")
            os.fsync(parent)
        finally:
            os.close(child)
    finally:
        os.close(parent)
    return path


def write_exclusive(path: Path, data: bytes, mode: int = 0o600) -> None:
    path = path.absolute()
    parent = _open_owner_directory(path.parent, writable=True)
    fd = -1
    try:
        fd = os.open(path.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, mode, dir_fd=parent)
        view = memoryview(data)
        while view:
            count = os.write(fd, view)
            if count <= 0:
                raise WorkflowError("short-write", f"short write: {path.name}")
            view = view[count:]
        os.fsync(fd)
        os.close(fd)
        fd = -1
        os.fsync(parent)
    finally:
        if fd >= 0:
            os.close(fd)
        os.close(parent)


def load_canonical_json(path: Path, *, maximum: int = MAX_JSON_BYTES) -> Any:
    return parse_canonical_json(read_held(path, maximum=maximum), maximum=maximum)


def parse_sha256sums(data: bytes) -> dict[str, str]:
    try:
        text = data.decode("ascii")
    except UnicodeDecodeError as exc:
        raise WorkflowError("invalid-manifest", "SHA256SUMS must be ASCII") from exc
    if text and not text.endswith("\n"):
        raise WorkflowError("invalid-manifest", "SHA256SUMS needs one trailing LF")
    rows: dict[str, str] = {}
    order: list[str] = []
    for line in text.splitlines():
        if len(line) < 67 or line[64:66] != "  ":
            raise WorkflowError("invalid-manifest", "malformed SHA256SUMS row")
        digest, name = line[:64], line[66:]
        validate_hex64(digest, "manifest digest")
        name = validate_relpath(name)
        if name == "SHA256SUMS" or name in rows:
            raise WorkflowError("invalid-manifest", "self or duplicate SHA256SUMS row")
        rows[name] = digest
        order.append(name)
    if order != sorted(order, key=lambda item: item.encode("ascii")):
        raise WorkflowError("invalid-manifest", "SHA256SUMS rows are not byte-sorted")
    return rows


def list_flat_regular_files(root: Path) -> dict[str, Path]:
    root = root.absolute()
    directory = _open_owner_directory(root)
    result: dict[str, Path] = {}
    try:
        with os.scandir(directory) as entries:
            for entry in entries:
                if entry.name in {".", ".."}:
                    raise WorkflowError("unsafe-package", "invalid package entry")
                entry_info = entry.stat(follow_symlinks=False)
                if not stat.S_ISREG(entry_info.st_mode) or entry_info.st_uid != os.geteuid() or entry_info.st_nlink != 1:
                    raise WorkflowError("unsafe-package", f"package entry is not an owner-controlled single-link file: {entry.name}")
                name = validate_relpath(entry.name)
                result[name] = root / name
    finally:
        os.close(directory)
    return result


def verify_flat_manifest(root: Path) -> dict[str, tuple[int, str]]:
    files = list_flat_regular_files(root)
    manifest_path = files.get("SHA256SUMS")
    if manifest_path is None:
        raise WorkflowError("invalid-manifest", "SHA256SUMS is missing")
    rows = parse_sha256sums(read_held(manifest_path, maximum=MAX_JSON_BYTES))
    if set(rows) != set(files) - {"SHA256SUMS"}:
        raise WorkflowError("invalid-manifest", "manifest/file inventory differs")
    observed: dict[str, tuple[int, str]] = {}
    for name, expected in rows.items():
        count, digest = sha256_file(files[name])
        if digest != expected:
            raise WorkflowError("hash-mismatch", f"package hash differs: {name}")
        observed[name] = (count, digest)
    return observed


def _exact_keys(value: Mapping[str, Any], expected: set[str], field: str) -> None:
    if set(value) != expected:
        raise WorkflowError("invalid-review", f"{field} fields differ")


def validate_nonnegative_int(value: Any, field: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value <= MAX_FILE_BYTES:
        raise WorkflowError("invalid-value", f"{field} must be a bounded nonnegative integer")
    return value


def _validate_artifact(value: Any, field: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != {"bytes", "path", "sha256"}:
        raise WorkflowError("invalid-handoff", f"{field} artifact fields differ")
    validate_relpath(value["path"])
    validate_nonnegative_int(value["bytes"], f"{field}.bytes")
    validate_hex64(value["sha256"], f"{field}.sha256")
    return value


def validate_expected_handoff(value: Any) -> dict[str, Any]:
    expected = {
        "fixedReference", "grantPlans", "operationId", "outcome", "package", "phase",
        "readyResume", "receipts", "review", "schema", "states",
    }
    if not isinstance(value, dict) or set(value) != expected:
        raise WorkflowError("invalid-handoff", "handoff fields differ")
    if value["schema"] != "journal-cli-source-flow-handoff-v2":
        raise WorkflowError("invalid-handoff", "handoff schema differs")
    if not isinstance(value["operationId"], str) or not re.fullmatch(r"[0-9a-f]{32}", value["operationId"]):
        raise WorkflowError("invalid-handoff", "handoff operation ID differs")
    if value["phase"] != "ready" or value["outcome"] != "verified":
        raise WorkflowError("invalid-handoff", "handoff is not a verified ready outcome")
    states = value["states"]
    if not isinstance(states, dict) or set(states) != {"executed", "installed"} or states != {"executed": False, "installed": False}:
        raise WorkflowError("invalid-handoff", "handoff execution states differ")
    fixed = value["fixedReference"]
    fixed_fields = {"artifact", "endpoint", "descriptorBytes", "descriptorPath", "descriptorSha256", "index", "owner"}
    if not isinstance(fixed, dict) or set(fixed) != fixed_fields:
        raise WorkflowError("invalid-handoff", "fixed reference fields differ")
    _validate_artifact(fixed["artifact"], "fixedReference")
    if not isinstance(fixed["owner"], str) or not fixed["owner"]:
        raise WorkflowError("invalid-handoff", "fixed reference owner differs")
    if not isinstance(fixed["endpoint"], str) or not fixed["endpoint"]:
        raise WorkflowError("invalid-handoff", "fixed reference endpoint differs")
    validate_nonnegative_int(fixed["index"], "fixedReference.index")
    if not isinstance(fixed["descriptorPath"], list) or not fixed["descriptorPath"] or any(not isinstance(item, str) or not item for item in fixed["descriptorPath"]):
        raise WorkflowError("invalid-handoff", "fixed reference descriptor path differs")
    validate_nonnegative_int(fixed["descriptorBytes"], "fixedReference.descriptorBytes")
    validate_hex64(fixed["descriptorSha256"], "fixedReference.descriptorSha256")
    package = value["package"]
    if not isinstance(package, dict) or set(package) != {"aggregateBytes", "chunks", "entries", "manifestSha256", "treeSha256"}:
        raise WorkflowError("invalid-handoff", "handoff package fields differ")
    for name in ("aggregateBytes", "chunks", "entries"):
        validate_nonnegative_int(package[name], f"package.{name}")
    validate_hex64(package["manifestSha256"], "package.manifestSha256")
    validate_hex64(package["treeSha256"], "package.treeSha256")
    review = value["review"]
    if not isinstance(review, dict) or set(review) != {"sha256", "value"}:
        raise WorkflowError("invalid-handoff", "handoff review fields differ")
    validate_review(review["value"])
    if hashlib.sha256(canonical_json(review["value"])).hexdigest() != validate_hex64(review["sha256"], "review.sha256"):
        raise WorkflowError("invalid-handoff", "embedded review hash differs")
    receipts = value["receipts"]
    if not isinstance(receipts, dict) or set(receipts) != {"publish", "ready"}:
        raise WorkflowError("invalid-handoff", "handoff receipt fields differ")
    _validate_artifact(receipts["publish"], "receipts.publish")
    _validate_artifact(receipts["ready"], "receipts.ready")
    grants = value["grantPlans"]
    if not isinstance(grants, list):
        raise WorkflowError("invalid-handoff", "handoff grant plans differ")
    prior = None
    for position, artifact in enumerate(grants):
        _validate_artifact(artifact, f"grantPlans[{position}]")
        if prior is not None and artifact["path"].encode("utf-8") <= prior:
            raise WorkflowError("invalid-handoff", "handoff grant plans are not sorted uniquely")
        prior = artifact["path"].encode("utf-8")
    if value["readyResume"] is not None:
        raise WorkflowError("invalid-handoff", "verified handoff has ready resume material")
    return value


def validate_review(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise WorkflowError("invalid-review", "REVIEW.json must be an object")
    _exact_keys(value, {"schema", "package", "repository", "git", "evidence"}, "review")
    if value["schema"] != "sync-source-flow-review-v1" or value["package"] != {"schema": 1}:
        raise WorkflowError("invalid-review", "review schema differs")
    repository = value["repository"]
    if not isinstance(repository, dict) or set(repository) != {"id"} or not isinstance(repository["id"], str) or not REPOSITORY_ID.fullmatch(repository["id"]):
        raise WorkflowError("invalid-review", "repository identity differs")
    git = value["git"]
    if not isinstance(git, dict):
        raise WorkflowError("invalid-review", "git review must be an object")
    _exact_keys(git, {"base", "bundle", "bundleBytes", "bundleSha256", "changedFiles", "changedFilesSha256", "commitCount", "commits", "commitsSha256", "head", "kind", "prerequisite", "tree"}, "git")
    for field in ("base", "head", "tree"):
        validate_hex40(git[field], f"git.{field}")
    kind = git["kind"]
    if kind not in {"full", "correction"}:
        raise WorkflowError("invalid-review", "git.kind differs")
    prerequisite = validate_hex40(git["prerequisite"], "git.prerequisite")
    if git["base"] != prerequisite:
        raise WorkflowError("invalid-review", "git base/prerequisite differ")
    for field in ("bundle", "changedFiles", "commits"):
        validate_relpath(git[field])
    for field in ("bundleSha256", "changedFilesSha256", "commitsSha256"):
        validate_hex64(git[field], f"git.{field}")
    for field in ("bundleBytes", "commitCount"):
        if not isinstance(git[field], int) or isinstance(git[field], bool) or not 1 <= git[field] <= MAX_FILE_BYTES:
            raise WorkflowError("invalid-review", f"git.{field} must be positive and bounded")
    evidence = value["evidence"]
    if not isinstance(evidence, list):
        raise WorkflowError("invalid-review", "evidence must be an array")
    seen: set[str] = set()
    for item in evidence:
        if not isinstance(item, dict) or set(item) != {"bytes", "path", "sha256"}:
            raise WorkflowError("invalid-review", "evidence entry shape differs")
        path = validate_relpath(item["path"])
        if path in seen or path in {"REVIEW.json", "SHA256SUMS"} or not isinstance(item["bytes"], int) or isinstance(item["bytes"], bool) or not 0 <= item["bytes"] <= MAX_FILE_BYTES:
            raise WorkflowError("invalid-review", "evidence entry value differs")
        validate_hex64(item["sha256"], "evidence.sha256")
        seen.add(path)
    return value


def validate_review_package(root: Path) -> tuple[dict[str, Any], dict[str, tuple[int, str]]]:
    observed = verify_flat_manifest(root)
    review_path = root.absolute() / "REVIEW.json"
    review = validate_review(load_canonical_json(review_path))
    git = review["git"]
    expected = {
        git["bundle"]: (git["bundleBytes"], git["bundleSha256"]),
        git["changedFiles"]: (None, git["changedFilesSha256"]),
        git["commits"]: (None, git["commitsSha256"]),
    }
    for item in review["evidence"]:
        expected[item["path"]] = (item["bytes"], item["sha256"])
    if len(expected) != 3 + len(review["evidence"]):
        raise WorkflowError("invalid-review", "review paths overlap")
    if set(observed) != {"REVIEW.json", *expected}:
        raise WorkflowError("invalid-review", "REVIEW.json/package inventory differs")
    for name, (count, digest) in expected.items():
        actual_count, actual_digest = observed[name]
        if actual_digest != digest or count is not None and actual_count != count:
            raise WorkflowError("hash-mismatch", f"review binding differs: {name}")
    return review, observed


@dataclass(frozen=True)
class ProcessResult:
    command: tuple[str, ...]
    returncode: int
    stdout: bytes
    stderr: bytes


@dataclass(frozen=True)
class SourceResult:
    returncode: int
    value: dict[str, Any]
    stdout_path: Path
    stderr_path: Path
    stdout_sha256: str
    stderr_sha256: str


def run_bounded(command: Sequence[str], *, cwd: Path | None = None, environment: Mapping[str, str] | None = None, timeout: float = 120.0, maximum_output: int = MAX_PROCESS_OUTPUT) -> ProcessResult:
    if not command or any(not isinstance(item, str) or "\x00" in item for item in command):
        raise WorkflowError("invalid-command", "subprocess command differs")
    if not math.isfinite(timeout) or timeout <= 0 or timeout > 3600 or not 1 <= maximum_output <= MAX_PROCESS_OUTPUT:
        raise WorkflowError("invalid-command", "subprocess bound differs")
    env = os.environ.copy()
    env.pop("PYTHONPATH", None)
    env.pop("PYTHONHOME", None)
    if environment and environment.get("GIT_CONFIG_NOSYSTEM") == "1":
        for key in tuple(env):
            if key.startswith("GIT_"):
                env.pop(key, None)
    if environment:
        env.update(environment)
    with tempfile.TemporaryFile() as stdout_file, tempfile.TemporaryFile() as stderr_file:
        child = subprocess.Popen(tuple(command), cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=stdout_file, stderr=stderr_file, start_new_session=True)
        try:
            returncode = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired as exc:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
            raise WorkflowError("timeout", f"subprocess timed out: {command[0]}") from exc
        stdout_file.seek(0, os.SEEK_END)
        stderr_file.seek(0, os.SEEK_END)
        stdout_bytes = stdout_file.tell()
        stderr_bytes = stderr_file.tell()
        if stdout_bytes > maximum_output or stderr_bytes > maximum_output:
            raise WorkflowError("output-limit", f"subprocess output exceeded bound: {command[0]}")
        stdout_file.seek(0)
        stderr_file.seek(0)
        return ProcessResult(tuple(command), returncode, stdout_file.read(), stderr_file.read())


def source_launcher_once(arguments: Sequence[str], *, evidence_dir: Path, label: str, timeout: float = 180.0) -> SourceResult:
    if not isinstance(label, str) or not EVIDENCE_LABEL.fullmatch(label):
        raise WorkflowError("invalid-source-operation", "Source evidence label differs")
    launcher_value = os.environ.get(SOURCE_LAUNCHER_ENV)
    expected = os.environ.get(SOURCE_LAUNCHER_SHA_ENV)
    if launcher_value is None or expected is None:
        raise WorkflowError("unsupported-capability", "verified Source launcher binding is absent")
    launcher = Path(launcher_value).absolute()
    validate_hex64(expected, "Source launcher SHA-256")
    count, observed = sha256_file(launcher, maximum=1_000_000)
    info = os.lstat(launcher)
    if observed != expected or not info.st_mode & stat.S_IXUSR:
        raise WorkflowError("provenance-mismatch", "Source launcher binding differs")
    if any(not isinstance(item, str) or "\x00" in item for item in arguments):
        raise WorkflowError("invalid-source-operation", "Source arguments differ")
    result = run_bounded([str(launcher), *arguments], timeout=timeout, maximum_output=MAX_PROCESS_OUTPUT)
    stdout_path = evidence_dir.absolute() / f"{label}.stdout.json"
    stderr_path = evidence_dir.absolute() / f"{label}.stderr"
    write_exclusive(stdout_path, result.stdout)
    write_exclusive(stderr_path, result.stderr)
    value = parse_canonical_json(result.stdout, maximum=MAX_PROCESS_OUTPUT)
    if not isinstance(value, dict):
        raise WorkflowError("source-framing", "Source launcher result is not an object")
    return SourceResult(
        result.returncode,
        value,
        stdout_path,
        stderr_path,
        hashlib.sha256(result.stdout).hexdigest(),
        hashlib.sha256(result.stderr).hexdigest(),
    )


def source_pull_once(*, reference: Path, route: list[str], destination: Path, receipt: Path, evidence_dir: Path) -> SourceResult:
    route_bytes = canonical_json(route)[:-1]
    route_text = route_bytes.decode("utf-8")
    return source_launcher_once(
        ["pull", "--reference", str(reference.absolute()), "--route", route_text,
         "--destination", str(destination.absolute()), "--receipt", str(receipt.absolute())],
        evidence_dir=evidence_dir,
        label="source-pull",
    )


def isolated_git_environment(root: Path) -> tuple[dict[str, str], tuple[str, ...]]:
    root = root.absolute()
    home = root / "git-home"
    hooks = root / "empty-hooks"
    for path in (home, hooks):
        if not os.path.lexists(path):
            make_exclusive_dir(path)
        descriptor = _open_owner_directory(path)
        os.close(descriptor)
    env = {
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home),
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_ASKPASS": "/bin/false",
        "SSH_ASKPASS": "/bin/false",
        "GIT_PROTOCOL_FROM_USER": "0",
        "GIT_ALLOW_PROTOCOL": "file",
        "GIT_OPTIONAL_LOCKS": "0",
        "GIT_PAGER": "cat",
        "PAGER": "cat",
        "GIT_EDITOR": "/bin/false",
    }
    options = (
        "-c", f"core.hooksPath={hooks}",
        "-c", "core.fsmonitor=false",
        "-c", "credential.helper=",
        "-c", "protocol.file.allow=always",
        "-c", "submodule.recurse=false",
        "-c", "fetch.recurseSubmodules=false",
    )
    return env, options


def concise_json_line(value: Mapping[str, Any], *, maximum: int = 4096) -> bytes:
    data = canonical_json(dict(value))
    if len(data) > maximum:
        raise WorkflowError("output-limit", "concise output exceeds bound")
    return data
