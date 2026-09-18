#!/usr/bin/python3
"""Descriptor-relative source snapshot, staging, verification, and recovery."""

from __future__ import annotations

import ctypes
from dataclasses import dataclass
import errno
import hashlib
import json
import os
from pathlib import Path
import stat
import tempfile
import uuid
from typing import Callable, Iterable

from .sync_source_model import (
    Chunk, DirectoryEntry, FileEntry, Release, SyncSourceError, MAX_CHUNK_BYTES, MAX_ENTRIES,
    MAX_FILE_BYTES, MAX_RELEASE_BYTES, logical_sort_key, sha256, tree_digest, validate_label,
    validate_logical_component, validate_logical_path, validate_release,
)

O_NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
O_DIRECTORY = getattr(os, "O_DIRECTORY", 0)
RENAME_NOREPLACE = 1


@dataclass(frozen=True)
class Snapshot:
    release: Release
    chunk_files: dict[tuple[str, ...], Path]


@dataclass(frozen=True)
class _PendingFile:
    path: tuple[str, ...]
    mode: int
    bytes: int
    sha256: str
    pieces: tuple[tuple[Path, int, str], ...]


def _write_all(fd: int, data: bytes) -> None:
    offset = 0
    while offset < len(data):
        count = os.write(fd, data[offset:])
        if count <= 0: raise OSError(errno.EIO, "short write made no progress")
        offset += count


def _listdir_fresh(directory: int) -> list[str]:
    fresh = os.open(".", os.O_RDONLY | O_DIRECTORY | O_NOFOLLOW, dir_fd=directory)
    try: return os.listdir(fresh)
    finally: os.close(fresh)


def _open_absolute_directory(path: Path) -> int:
    path = path.absolute()
    if not path.is_absolute():
        raise SyncSourceError("unsafe-filesystem", "Filesystem path must be absolute")
    current = os.open("/", os.O_RDONLY | O_DIRECTORY | O_NOFOLLOW)
    try:
        components = path.parts[1:]
        for index, component in enumerate(components):
            child = _open_dir_at(current, component, require_owner=index == len(components) - 1)
            os.close(current); current = child
        result = current; current = -1; return result
    finally:
        if current >= 0: os.close(current)


def _open_absolute_file(path: Path) -> int:
    parent = _open_absolute_directory(path.absolute().parent)
    try:
        fd = os.open(path.name, os.O_RDONLY | O_NOFOLLOW, dir_fd=parent)
        result = fd; fd = -1; return result
    finally:
        os.close(parent)


def _open_dir_at(parent: int, name: str, *, require_owner: bool = True) -> int:
    fd = os.open(name, os.O_RDONLY | O_DIRECTORY | O_NOFOLLOW, dir_fd=parent)
    info = os.fstat(fd)
    if not stat.S_ISDIR(info.st_mode) or (require_owner and info.st_uid != os.geteuid()):
        os.close(fd); raise SyncSourceError("unsafe-filesystem", "Directory invariant failed")
    return fd


def snapshot_source(source: Path, spool: Path, project: str, release_label: str) -> Snapshot:
    """Copy one stable regular-file tree into a private bounded local snapshot."""
    project = validate_label(project, "project"); release_label = validate_label(release_label, "release")
    spool.mkdir(mode=0o700, parents=True, exist_ok=False)
    os.chmod(spool, 0o700)
    root = _open_absolute_directory(source)
    root_info = os.fstat(root)
    if root_info.st_uid != os.geteuid():
        os.close(root); raise SyncSourceError("unsafe-source", "Source root must be owner-controlled")
    directories: list[tuple[str, ...]] = []
    pending: list[_PendingFile] = []
    aggregate = 0

    def walk(directory: int, prefix: tuple[str, ...]) -> None:
        nonlocal aggregate
        names = _listdir_fresh(directory)
        checked = [(validate_logical_component(name), name) for name in names]
        checked.sort(key=lambda item: item[0].encode("utf-8"))
        for component, name in checked:
            path = (*prefix, component); validate_logical_path(path)
            before = os.stat(name, dir_fd=directory, follow_symlinks=False)
            if stat.S_ISDIR(before.st_mode):
                child = _open_dir_at(directory, name)
                try:
                    after = os.fstat(child)
                    if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
                        raise SyncSourceError("source-changed", "Source directory changed while opening")
                    directories.append(path)
                    if len(directories) + len(pending) > MAX_ENTRIES:
                        raise SyncSourceError("resource-limit", "Source entry count exceeds v1 limit")
                    walk(child, path)
                finally: os.close(child)
                continue
            if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_uid != os.geteuid():
                raise SyncSourceError("unsupported-source-entry", "Source contains a nonregular, linked, or unowned entry")
            if before.st_size > MAX_FILE_BYTES:
                raise SyncSourceError("resource-limit", "Source file exceeds v1 limit")
            fd = os.open(name, os.O_RDONLY | O_NOFOLLOW, dir_fd=directory)
            pieces: list[tuple[Path, int, str]] = []
            whole = hashlib.sha256(); total = 0
            try:
                opened = os.fstat(fd)
                if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino) or opened.st_nlink != 1:
                    raise SyncSourceError("source-changed", "Source file changed while opening")
                while True:
                    parts = []
                    remaining = MAX_CHUNK_BYTES
                    while remaining:
                        part = os.read(fd, remaining)
                        if not part: break
                        parts.append(part); remaining -= len(part)
                    data = b"".join(parts)
                    if not data: break
                    total += len(data); aggregate += len(data)
                    if total > MAX_FILE_BYTES or aggregate > MAX_RELEASE_BYTES:
                        raise SyncSourceError("resource-limit", "Source byte limits exceeded")
                    whole.update(data); digest = sha256(data)
                    piece = spool / f"piece-{uuid.uuid4().hex}"
                    out = os.open(piece, os.O_WRONLY | os.O_CREAT | os.O_EXCL | O_NOFOLLOW, 0o600)
                    try:
                        _write_all(out, data)
                        os.fsync(out)
                    finally: os.close(out)
                    pieces.append((piece, len(data), digest))
                final = os.fstat(fd)
                fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns")
                if any(getattr(opened, field) != getattr(final, field) for field in fields) or total != final.st_size:
                    raise SyncSourceError("source-changed", "Source file changed while reading")
            finally: os.close(fd)
            mode = 493 if before.st_mode & 0o111 else 420
            pending.append(_PendingFile(path, mode, total, whole.hexdigest(), tuple(pieces)))
            if len(directories) + len(pending) > MAX_ENTRIES:
                raise SyncSourceError("resource-limit", "Source entry count exceeds v1 limit")

    try: walk(root, ())
    finally: os.close(root)
    mixed: list[tuple[tuple[str, ...], str, object]] = [(path, "directory", None) for path in directories] + [(item.path, "file", item) for item in pending]
    mixed.sort(key=lambda item: logical_sort_key(item[0]))
    entries = []
    chunk_files: dict[tuple[str, ...], Path] = {}
    for ordinal, (path, kind, value) in enumerate(mixed):
        if kind == "directory": entries.append(DirectoryEntry(path)); continue
        item = value
        assert isinstance(item, _PendingFile)
        chunks = []
        for chunk_ordinal, (piece, count, digest) in enumerate(item.pieces):
            journal_path = ("tree", f"o{ordinal:08d}", f"c{chunk_ordinal:08d}")
            chunks.append(Chunk(journal_path, count, digest)); chunk_files[journal_path] = piece
        entries.append(FileEntry(path, item.mode, item.bytes, item.sha256, tuple(chunks)))
    release = validate_release(Release(project, release_label, tuple(entries)))
    release.encode()
    return Snapshot(release, chunk_files)


def _mkdir_chain(root: int, path: tuple[str, ...], modes: dict[tuple[str, ...], int]) -> int:
    current = os.dup(root)
    prefix: tuple[str, ...] = ()
    try:
        for component in path:
            prefix = (*prefix, component)
            try: os.mkdir(component, 0o700, dir_fd=current)
            except FileExistsError: pass
            child = _open_dir_at(current, component)
            os.close(current); current = child
            os.fchmod(current, modes.get(prefix, 493))
        result = current; current = -1; return result
    finally:
        if current >= 0: os.close(current)


def _write_file(root: int, entry: FileEntry, chunks: Iterable[bytes]) -> None:
    parent = _mkdir_chain(root, entry.path[:-1], {}) if len(entry.path) > 1 else os.dup(root)
    try:
        fd = os.open(entry.path[-1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | O_NOFOLLOW, 0o600, dir_fd=parent)
        digest = hashlib.sha256(); total = 0
        try:
            for data in chunks:
                digest.update(data); total += len(data); _write_all(fd, data)
            if total != entry.bytes or digest.hexdigest() != entry.sha256:
                raise SyncSourceError("content-mismatch", "Reconstructed file does not match descriptor")
            os.fchmod(fd, entry.mode); os.fsync(fd)
        finally: os.close(fd)
    finally: os.close(parent)


def _expected_tree(release: Release) -> tuple[dict[tuple[str, ...], int], dict[tuple[str, ...], FileEntry]]:
    directories: dict[tuple[str, ...], int] = {}
    files: dict[tuple[str, ...], FileEntry] = {}
    for entry in release.entries:
        for length in range(1, len(entry.path)):
            directories.setdefault(entry.path[:length], 493)
        if isinstance(entry, DirectoryEntry): directories[entry.path] = entry.mode
        else: files[entry.path] = entry
    return directories, files


def verify_tree_fd(root: int, release: Release) -> str:
    directories, files = _expected_tree(release)
    seen_dirs: set[tuple[str, ...]] = set(); seen_files: set[tuple[str, ...]] = set()

    def walk(directory: int, prefix: tuple[str, ...]) -> None:
        for name in _listdir_fresh(directory):
            component = validate_logical_component(name); path = (*prefix, component)
            info = os.stat(name, dir_fd=directory, follow_symlinks=False)
            if stat.S_ISDIR(info.st_mode):
                if path not in directories or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != directories[path]:
                    raise SyncSourceError("tree-mismatch", "Directory differs from descriptor")
                child = _open_dir_at(directory, name)
                try: seen_dirs.add(path); walk(child, path)
                finally: os.close(child)
            elif stat.S_ISREG(info.st_mode):
                entry = files.get(path)
                if entry is None or info.st_uid != os.geteuid() or info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != entry.mode or info.st_size != entry.bytes:
                    raise SyncSourceError("tree-mismatch", "File differs from descriptor")
                fd = os.open(name, os.O_RDONLY | O_NOFOLLOW, dir_fd=directory)
                digest = hashlib.sha256(); total = 0
                try:
                    while True:
                        data = os.read(fd, 131_072)
                        if not data: break
                        total += len(data); digest.update(data)
                finally: os.close(fd)
                if total != entry.bytes or digest.hexdigest() != entry.sha256:
                    raise SyncSourceError("tree-mismatch", "File bytes differ from descriptor")
                seen_files.add(path)
            else: raise SyncSourceError("tree-mismatch", "Tree contains a non-file entry")

    walk(root, ())
    if seen_dirs != set(directories) or seen_files != set(files):
        raise SyncSourceError("tree-mismatch", "Tree is incomplete")
    return tree_digest(release)


def verify_tree(path: Path, release: Release) -> str:
    fd = _open_absolute_directory(path)
    try: return verify_tree_fd(fd, release)
    finally: os.close(fd)


def _rename_noreplace(parent: int, source: str, destination: str) -> None:
    try: function = ctypes.CDLL(None, use_errno=True).renameat2
    except AttributeError as exc: raise SyncSourceError("unsupported-capability", "renameat2 is unavailable") from exc
    function.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    function.restype = ctypes.c_int
    if function(parent, source.encode(), parent, destination.encode(), RENAME_NOREPLACE) != 0:
        error = ctypes.get_errno()
        if error in {errno.EEXIST, errno.ENOTEMPTY}: raise SyncSourceError("destination-exists", "Destination appeared before commit")
        if error in {errno.ENOSYS, errno.EINVAL, errno.EOPNOTSUPP}: raise SyncSourceError("unsupported-capability", "Atomic no-replace is unavailable")
        raise OSError(error, os.strerror(error))


def _remove_tree(parent: int, name: str) -> None:
    info = os.stat(name, dir_fd=parent, follow_symlinks=False)
    if not stat.S_ISDIR(info.st_mode):
        os.unlink(name, dir_fd=parent); return
    child = _open_dir_at(parent, name)
    try:
        for entry in _listdir_fresh(child): _remove_tree(child, entry)
    finally: os.close(child)
    os.rmdir(name, dir_fd=parent)


@dataclass
class MaterializeResult:
    destination: Path
    tree_sha256: str
    staging_cleaned: bool


def materialize(
    destination: Path, release: Release, chunk_reader: Callable[[Chunk], bytes], marker_receipt: Path,
    *, operation_id: str, cut: Callable[[str, Path], None] | None = None,
) -> MaterializeResult:
    """Build privately, verify, and atomically create one absent destination."""
    destination = destination.absolute()
    if destination.name in {"", ".", ".."}:
        raise SyncSourceError("invalid-destination", "Destination name is invalid")
    parent_path = destination.parent
    if not re_full_operation_id(operation_id):
        raise SyncSourceError("invalid-operation-id", "Operation ID must be 32 lowercase hexadecimal characters")
    parent = _open_absolute_directory(parent_path)
    parent_info = os.fstat(parent)
    if not stat.S_ISDIR(parent_info.st_mode) or parent_info.st_uid != os.geteuid() or stat.S_IMODE(parent_info.st_mode) & 0o022:
        os.close(parent)
        raise SyncSourceError("unsafe-filesystem", "Destination parent must be owner-controlled and not group/world writable")
    stage = f".sync-source-stage-{operation_id}"
    marker = stage + ".json"
    stage_fd = -1; committed = False; cleaned = False
    cut = cut or (lambda _point, _path: None)
    try:
        try: os.stat(destination.name, dir_fd=parent, follow_symlinks=False)
        except FileNotFoundError: pass
        else: raise SyncSourceError("destination-exists", "Destination already exists")
        os.mkdir(stage, 0o700, dir_fd=parent); stage_fd = _open_dir_at(parent, stage)
        stage_info = os.fstat(stage_fd)
        marker_value = {
            "version": 1, "stage": stage, "stageDevice": stage_info.st_dev, "stageInode": stage_info.st_ino,
            "destination": destination.name, "receipt": str(marker_receipt.absolute()),
        }
        marker_fd = os.open(marker, os.O_WRONLY | os.O_CREAT | os.O_EXCL | O_NOFOLLOW, 0o600, dir_fd=parent)
        try:
            data = (json.dumps(marker_value, sort_keys=True, separators=(",", ":")) + "\n").encode()
            _write_all(marker_fd, data); os.fsync(marker_fd)
        finally: os.close(marker_fd)
        cut("after-stage", destination)
        directories, _ = _expected_tree(release)
        for path, mode in sorted(directories.items(), key=lambda item: logical_sort_key(item[0])):
            fd = _mkdir_chain(stage_fd, path, directories)
            try: os.fchmod(fd, mode); os.fsync(fd)
            finally: os.close(fd)
        for entry in release.entries:
            if not isinstance(entry, FileEntry): continue
            _write_file(stage_fd, entry, (chunk_reader(chunk) for chunk in entry.chunks))
            cut("after-file", destination)
        for path in sorted(directories, key=lambda value: (len(value), logical_sort_key(value)), reverse=True):
            fd = _mkdir_chain(stage_fd, path, directories)
            try: os.fsync(fd)
            finally: os.close(fd)
        digest = verify_tree_fd(stage_fd, release); cut("after-verify", destination)
        try: os.stat(destination.name, dir_fd=parent, follow_symlinks=False)
        except FileNotFoundError: pass
        else: raise SyncSourceError("destination-exists", "Destination appeared before commit")
        cut("before-rename", destination)
        _rename_noreplace(parent, stage, destination.name); committed = True
        os.close(stage_fd); stage_fd = -1
        cut("after-rename", destination)
        os.unlink(marker, dir_fd=parent); os.fsync(parent)
        final = _open_dir_at(parent, destination.name)
        try: final_digest = verify_tree_fd(final, release)
        finally: os.close(final)
        if final_digest != digest: raise SyncSourceError("tree-mismatch", "Final tree digest changed")
        return MaterializeResult(destination, digest, True)
    except Exception:
        if stage_fd >= 0: os.close(stage_fd); stage_fd = -1
        if not committed:
            try: _remove_tree(parent, stage); cleaned = True
            except FileNotFoundError: cleaned = True
            try: os.unlink(marker, dir_fd=parent)
            except FileNotFoundError: pass
            try: os.fsync(parent)
            except OSError: pass
        raise
    finally:
        if stage_fd >= 0: os.close(stage_fd)
        os.close(parent)


def recover(parent_path: Path, marker_name: str, receipt_path: Path) -> bool:
    """Explicitly remove only a marker-bound incomplete staging operation."""
    if not re_full_stage_marker(marker_name):
        raise SyncSourceError("invalid-recovery", "Recovery marker name is invalid")
    parent = _open_absolute_directory(parent_path)
    try:
        marker_fd = os.open(marker_name, os.O_RDONLY | O_NOFOLLOW, dir_fd=parent)
        try:
            info = os.fstat(marker_fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o600 or info.st_nlink != 1:
                raise SyncSourceError("invalid-recovery", "Recovery marker invariant failed")
            data = os.read(marker_fd, 16_385)
            if len(data) > 16_384: raise SyncSourceError("invalid-recovery", "Recovery marker is oversized")
        finally: os.close(marker_fd)
        value = json.loads(data.decode("utf-8"))
        stage = marker_name[:-5]
        if value != {
            "version": 1, "stage": stage, "stageDevice": value.get("stageDevice"), "stageInode": value.get("stageInode"),
            "destination": value.get("destination"), "receipt": str(receipt_path.absolute()),
        }:
            raise SyncSourceError("invalid-recovery", "Recovery marker is not bound to this receipt")
        stage_fd = _open_dir_at(parent, stage)
        try:
            stage_info = os.fstat(stage_fd)
            if (stage_info.st_dev, stage_info.st_ino) != (value["stageDevice"], value["stageInode"]):
                raise SyncSourceError("invalid-recovery", "Recovery stage identity changed")
        finally: os.close(stage_fd)
        receipt_fd = _open_absolute_file(receipt_path)
        try:
            receipt_info = os.fstat(receipt_fd)
            if not stat.S_ISREG(receipt_info.st_mode) or receipt_info.st_uid != os.geteuid() or stat.S_IMODE(receipt_info.st_mode) != 0o600 or receipt_info.st_nlink != 1:
                raise SyncSourceError("invalid-recovery", "Recovery receipt invariant failed")
            receipt_data = os.read(receipt_fd, 65_537)
            if len(receipt_data) > 65_536: raise SyncSourceError("invalid-recovery", "Recovery receipt is oversized")
        finally: os.close(receipt_fd)
        receipt = json.loads(receipt_data.decode("utf-8", errors="strict"))
        if receipt.get("operationId") not in stage or receipt.get("outcome") not in {"attempted", "failed-or-ambiguous"}:
            raise SyncSourceError("invalid-recovery", "Receipt does not authorize cleanup")
        _remove_tree(parent, stage); os.unlink(marker_name, dir_fd=parent); os.fsync(parent); return True
    finally: os.close(parent)


def re_full_operation_id(value: str) -> bool:
    import re
    return bool(re.fullmatch(r"[0-9a-f]{32}", value))


def re_full_stage_marker(value: str) -> bool:
    import re
    return bool(re.fullmatch(r"\.sync-source-stage-[0-9a-f]{32}\.json", value))
