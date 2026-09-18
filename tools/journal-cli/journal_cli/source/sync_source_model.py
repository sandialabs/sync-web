#!/usr/bin/python3
"""Strict inert model and canonical encoding for Sync Source v1."""

from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
import ipaddress
import re
import struct
import unicodedata
from typing import Any, Iterable
from urllib.parse import urlsplit

MAX_DESCRIPTOR_BYTES = 524_288
MAX_ENTRIES = 4_096
MAX_CHUNKS = 16_384
MAX_DEPTH = 64
MAX_PATH_BYTES = 4_096
MAX_COMPONENT_BYTES = 255
MAX_CHUNK_BYTES = 524_288
MAX_FILE_BYTES = 67_108_864
MAX_RELEASE_BYTES = 536_870_912
MAX_LABEL_BYTES = 128
SAFE_JOURNAL_COMPONENT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
LOWER_SHA256 = re.compile(r"^[0-9a-f]{64}$")
LOWER_UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")


class Symbol(str):
    """An inert grammar symbol, distinct from a quoted string."""


S = Symbol


class SyncSourceError(Exception):
    """A stable fail-closed Sync Source error."""

    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def fail(code: str, message: str) -> None:
    raise SyncSourceError(code, message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_endpoint(value: str) -> str:
    if not isinstance(value, str) or not value:
        fail("invalid-endpoint", "Source endpoint must be a nonempty absolute URL")
    try:
        parsed = urlsplit(value)
        port = parsed.port
    except ValueError as exc:
        raise SyncSourceError("invalid-endpoint", "Source endpoint URL is invalid") from exc
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        fail("invalid-endpoint", "Source endpoint must use HTTP or HTTPS with a host")
    if parsed.username is not None or parsed.password is not None or parsed.query or parsed.fragment:
        fail("invalid-endpoint", "Source endpoint forbids userinfo, query, and fragment")
    host = parsed.hostname.lower()
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        address = None
        if (not re.fullmatch(r"[a-z0-9.-]+", host) or host.startswith(".") or host.endswith(".")
                or ".." in host):
            fail("invalid-endpoint", "Source endpoint host must be canonical ASCII DNS or an IP literal")
    if parsed.scheme == "http" and (address is None or not address.is_loopback):
        fail("invalid-endpoint", "Non-loopback Source endpoints require HTTPS")
    if parsed.path != "/interface":
        fail("invalid-endpoint", "Source endpoint path must be exactly /interface")
    if port is not None and not 1 <= port <= 65535:
        fail("invalid-endpoint", "Source endpoint port is invalid")
    rendered_host = f"[{host}]" if ":" in host else host
    default_port = (parsed.scheme == "http" and port == 80) or (parsed.scheme == "https" and port == 443)
    authority = rendered_host if port is None or default_port else f"{rendered_host}:{port}"
    canonical = f"{parsed.scheme}://{authority}/interface"
    if value != canonical:
        fail("invalid-endpoint", f"Source endpoint is not canonical; expected {canonical}")
    return canonical


def validate_route(value: Iterable[str]) -> tuple[str, ...]:
    if not isinstance(value, (list, tuple)):
        fail("invalid-route", "Source route must be an array")
    route = tuple(validate_journal_component(item) for item in value)
    if len(route) > MAX_DEPTH:
        fail("invalid-route", "Source route is too deep")
    return route


def validate_history_indexes(value: Iterable[int], route: tuple[str, ...], terminal_index: int) -> tuple[int, ...]:
    if not isinstance(value, (list, tuple)):
        fail("invalid-history-indexes", "Source history indexes must be an array")
    indexes = tuple(value)
    if len(indexes) != len(route) + 1 or any(type(item) is not int or item < 0 for item in indexes):
        fail("invalid-history-indexes", "Source history indexes must fix the local origin and every route hop")
    if indexes[-1] != terminal_index:
        fail("invalid-history-indexes", "Terminal history index differs from the fixed terminal index")
    return indexes


def validate_journal_component(value: str) -> str:
    if not isinstance(value, str) or not SAFE_JOURNAL_COMPONENT.fullmatch(value) or value in {".", ".."}:
        fail("invalid-journal-path", f"Unsafe Journal component: {value!r}")
    return value


def validate_journal_path(path: Iterable[str]) -> tuple[str, ...]:
    if not isinstance(path, (list, tuple)):
        fail("invalid-journal-path", "Journal path must be an array")
    values = tuple(validate_journal_component(value) for value in path)
    if not 1 <= len(values) <= MAX_DEPTH:
        fail("invalid-journal-path", "Journal path depth is out of bounds")
    if len("/".join(values).encode("utf-8")) > MAX_PATH_BYTES:
        fail("invalid-journal-path", "Journal path is too long")
    return values


def validate_logical_component(value: str) -> str:
    if not isinstance(value, str) or unicodedata.normalize("NFC", value) != value:
        fail("invalid-logical-path", "Logical component must be an NFC string")
    try:
        encoded = value.encode("utf-8", errors="strict")
    except UnicodeError as exc:
        raise SyncSourceError("invalid-logical-path", "Logical component is not valid UTF-8") from exc
    if not 1 <= len(encoded) <= MAX_COMPONENT_BYTES or value in {".", ".."} or "/" in value or "\\" in value:
        fail("invalid-logical-path", f"Unsafe logical component: {value!r}")
    if any(ord(char) < 0x20 or ord(char) == 0x7F for char in value):
        fail("invalid-logical-path", "Logical component contains a control scalar")
    return value


def validate_logical_path(path: Iterable[str]) -> tuple[str, ...]:
    if not isinstance(path, (list, tuple)):
        fail("invalid-logical-path", "Logical path must be an array")
    values = tuple(validate_logical_component(value) for value in path)
    if not 1 <= len(values) <= MAX_DEPTH:
        fail("invalid-logical-path", "Logical path depth is out of bounds")
    if len("/".join(values).encode("utf-8")) > MAX_PATH_BYTES:
        fail("invalid-logical-path", "Logical path is too long")
    return values


def logical_sort_key(path: tuple[str, ...]) -> tuple[bytes, ...]:
    return tuple(value.encode("utf-8") for value in path)


class Parser:
    """A bounded parser for the restricted canonical S-expression subset."""

    def __init__(self, data: bytes, *, limit: int = MAX_DESCRIPTOR_BYTES):
        if not isinstance(data, bytes) or not 1 <= len(data) <= limit:
            fail("object-size", "Object byte length is out of bounds")
        try:
            self.text = data.decode("utf-8", errors="strict")
        except UnicodeError as exc:
            raise SyncSourceError("invalid-encoding", "Object is not strict UTF-8") from exc
        if self.text.startswith("\ufeff"):
            fail("invalid-encoding", "UTF-8 BOM is forbidden")
        self.pos = 0
        self.nodes = 0
        self.max_nodes = 100_000

    def parse(self) -> Any:
        value = self._value(0)
        if self.pos >= len(self.text) or self.text[self.pos:] != "\n":
            fail("noncanonical-object", "Object must end with exactly one LF")
        return value

    def _value(self, depth: int) -> Any:
        if depth > 128:
            fail("object-depth", "S-expression nesting is excessive")
        self.nodes += 1
        if self.nodes > self.max_nodes:
            fail("object-nodes", "S-expression node count is excessive")
        if self.pos >= len(self.text):
            fail("malformed-object", "Unexpected end of object")
        char = self.text[self.pos]
        if char == "(":
            return self._list(depth + 1)
        if char == '"':
            return self._string()
        return self._atom()

    def _list(self, depth: int) -> list[Any]:
        self.pos += 1
        values: list[Any] = []
        if self.pos < len(self.text) and self.text[self.pos] == ")":
            self.pos += 1
            return values
        while True:
            values.append(self._value(depth))
            if self.pos >= len(self.text):
                fail("malformed-object", "Unclosed list")
            char = self.text[self.pos]
            if char == ")":
                self.pos += 1
                return values
            if char != " ":
                fail("noncanonical-object", "List items require one ASCII space")
            self.pos += 1
            if self.pos >= len(self.text) or self.text[self.pos] in {" ", ")", "\n", "\r", "\t"}:
                fail("noncanonical-object", "Invalid list separator")

    def _string(self) -> str:
        self.pos += 1
        out: list[str] = []
        while self.pos < len(self.text):
            char = self.text[self.pos]
            self.pos += 1
            if char == '"':
                value = "".join(out)
                if unicodedata.normalize("NFC", value) != value:
                    fail("noncanonical-string", "String is not NFC")
                return value
            if char == "\\":
                if self.pos >= len(self.text) or self.text[self.pos] not in {'"', "\\"}:
                    fail("invalid-string-escape", "Only quote and backslash may be escaped")
                out.append(self.text[self.pos])
                self.pos += 1
                continue
            code = ord(char)
            if code < 0x20 or code == 0x7F:
                fail("invalid-string", "String contains a forbidden control scalar")
            out.append(char)
        fail("malformed-object", "Unclosed string")

    def _atom(self) -> int | str:
        start = self.pos
        while self.pos < len(self.text) and self.text[self.pos] not in {" ", "(", ")", "\n", "\r", "\t"}:
            self.pos += 1
        token = self.text[start:self.pos]
        if not token:
            fail("malformed-object", "Empty atom")
        if re.fullmatch(r"0|[1-9][0-9]*", token):
            return int(token)
        if token[0].isdigit() or token.startswith(("#", "'", "`", ",")) or token == ".":
            fail("invalid-atom", f"Forbidden atom: {token}")
        if not re.fullmatch(r"[A-Za-z][A-Za-z0-9-]*", token):
            fail("invalid-atom", f"Invalid symbol: {token}")
        return Symbol(token)


def encode_string(value: str) -> str:
    if unicodedata.normalize("NFC", value) != value:
        fail("noncanonical-string", "String is not NFC")
    if any(ord(char) < 0x20 or ord(char) == 0x7F for char in value):
        fail("invalid-string", "String contains a forbidden control scalar")
    value.encode("utf-8", errors="strict")
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def encode_node(value: Any) -> str:
    if isinstance(value, list):
        return "(" + " ".join(encode_node(item) for item in value) + ")"
    if isinstance(value, int) and not isinstance(value, bool) and value >= 0:
        return str(value)
    if isinstance(value, Symbol):
        if not re.fullmatch(r"[A-Za-z][A-Za-z0-9-]*", value):
            fail("invalid-ast", "AST contains an invalid symbol")
        return str(value)
    if isinstance(value, str):
        return encode_string(value)
    fail("invalid-ast", "AST contains an unsupported value")


def canonical_bytes(ast: Any) -> bytes:
    return (encode_node(ast) + "\n").encode("utf-8")


def parse_canonical(data: bytes, *, limit: int = MAX_DESCRIPTOR_BYTES) -> Any:
    ast = Parser(data, limit=limit).parse()
    if canonical_bytes(ast) != data:
        fail("noncanonical-object", "AST reserialization differs from input")
    return ast


def pair(node: Any, name: str, length: int = 2) -> list[Any]:
    if not isinstance(node, list) or len(node) != length or node[0] != name:
        fail("object-shape", f"Expected ({name} ...)")
    return node


def string_value(node: Any, name: str) -> str:
    value = pair(node, name)[1]
    if not isinstance(value, str):
        fail("object-shape", f"{name} must be a string")
    return value


def integer_value(node: Any, name: str) -> int:
    value = pair(node, name)[1]
    if not isinstance(value, int) or isinstance(value, bool):
        fail("object-shape", f"{name} must be an integer")
    return value


def path_value(node: Any, name: str, *, logical: bool) -> tuple[str, ...]:
    values = pair(node, name)[1]
    if not isinstance(values, list) or not all(isinstance(value, str) for value in values):
        fail("object-shape", f"{name} must contain strings")
    return validate_logical_path(values) if logical else validate_journal_path(values)


@dataclass(frozen=True)
class CurrentReference:
    endpoint: str
    owner: str
    head_path: tuple[str, ...]

    def ast(self) -> list[Any]:
        return [
            S("source-current-v2"), [S("endpoint"), self.endpoint],
            [S("owner"), self.owner], [S("head-path"), list(self.head_path)],
        ]

    def encode(self) -> bytes:
        return canonical_bytes(self.ast())


@dataclass(frozen=True)
class CurrentReleaseReference:
    endpoint: str
    owner: str
    descriptor_path: tuple[str, ...]
    descriptor_bytes: int
    descriptor_sha256: str

    def ast(self) -> list[Any]:
        return [
            S("source-current-release-v2"), [S("endpoint"), self.endpoint],
            [S("owner"), self.owner], [S("descriptor-path"), list(self.descriptor_path)],
            [S("descriptor-bytes"), self.descriptor_bytes], [S("descriptor-sha256"), self.descriptor_sha256],
        ]

    def encode(self) -> bytes:
        return canonical_bytes(self.ast())


@dataclass(frozen=True)
class FixedReference:
    endpoint: str
    owner: str
    index: int
    descriptor_path: tuple[str, ...]
    descriptor_bytes: int
    descriptor_sha256: str
    history_indexes: tuple[int, ...] = field(default=(), compare=False, repr=False)

    def ast(self) -> list[Any]:
        return [
            S("source-fixed-v2"), [S("endpoint"), self.endpoint], [S("owner"), self.owner],
            [S("index"), self.index], [S("descriptor-path"), list(self.descriptor_path)],
            [S("descriptor-bytes"), self.descriptor_bytes], [S("descriptor-sha256"), self.descriptor_sha256],
        ]

    def encode(self) -> bytes:
        return canonical_bytes(self.ast())


@dataclass(frozen=True)
class Chunk:
    path: tuple[str, ...]
    bytes: int
    sha256: str

    def ast(self) -> list[Any]:
        return [S("chunk"), [S("path"), list(self.path)], [S("bytes"), self.bytes], [S("sha256"), self.sha256]]


@dataclass(frozen=True)
class DirectoryEntry:
    path: tuple[str, ...]
    mode: int = 493

    def ast(self) -> list[Any]:
        return [S("directory"), [S("path"), list(self.path)], [S("mode"), self.mode]]


@dataclass(frozen=True)
class FileEntry:
    path: tuple[str, ...]
    mode: int
    bytes: int
    sha256: str
    chunks: tuple[Chunk, ...]

    def ast(self) -> list[Any]:
        return [
            S("file"), [S("path"), list(self.path)], [S("mode"), self.mode], [S("bytes"), self.bytes], [S("sha256"), self.sha256],
            [S("chunks"), [chunk.ast() for chunk in self.chunks]],
        ]


Entry = DirectoryEntry | FileEntry


@dataclass(frozen=True)
class Release:
    project: str
    release: str
    entries: tuple[Entry, ...]

    def ast(self) -> list[Any]:
        return [S("source-release-v1"), [S("project"), self.project], [S("release"), self.release], [S("entries"), [entry.ast() for entry in self.entries]]]

    def encode(self) -> bytes:
        data = canonical_bytes(self.ast())
        if len(data) > MAX_DESCRIPTOR_BYTES:
            fail("descriptor-size", "Descriptor exceeds v1 byte limit")
        return data

    @property
    def chunks(self) -> tuple[Chunk, ...]:
        return tuple(chunk for entry in self.entries if isinstance(entry, FileEntry) for chunk in entry.chunks)

    @property
    def aggregate_bytes(self) -> int:
        return sum(entry.bytes for entry in self.entries if isinstance(entry, FileEntry))


def parse_reference(data: bytes) -> CurrentReference | FixedReference:
    ast = parse_canonical(data)
    if not isinstance(ast, list) or not ast:
        fail("object-shape", "Reference must be a nonempty list")
    if ast[0] == "source-current-v2":
        if len(ast) != 4:
            fail("object-shape", "Current reference has wrong fields")
        endpoint = canonical_endpoint(string_value(ast[1], "endpoint"))
        owner = validate_journal_component(string_value(ast[2], "owner"))
        head_path = path_value(ast[3], "head-path", logical=False)
        value = CurrentReference(endpoint, owner, head_path)
    elif ast[0] == "source-fixed-v2":
        if len(ast) != 7:
            fail("object-shape", "Fixed reference has wrong fields")
        endpoint = canonical_endpoint(string_value(ast[1], "endpoint"))
        owner = validate_journal_component(string_value(ast[2], "owner"))
        index = integer_value(ast[3], "index")
        descriptor_path = path_value(ast[4], "descriptor-path", logical=False)
        descriptor_bytes = integer_value(ast[5], "descriptor-bytes")
        descriptor_sha = string_value(ast[6], "descriptor-sha256")
        if not 1 <= descriptor_bytes <= MAX_DESCRIPTOR_BYTES or not LOWER_SHA256.fullmatch(descriptor_sha):
            fail("object-shape", "Fixed descriptor evidence is invalid")
        value = FixedReference(endpoint, owner, index, descriptor_path, descriptor_bytes, descriptor_sha)
    else:
        fail("object-shape", "Unknown v2 reference type")
    if value.encode() != data:
        fail("noncanonical-object", "Reference reserialization differs")
    return value


def parse_current_release(data: bytes) -> CurrentReleaseReference:
    ast = parse_canonical(data)
    if not isinstance(ast, list) or len(ast) != 6 or ast[0] != "source-current-release-v2":
        fail("object-shape", "Current release has wrong type or fields")
    endpoint = canonical_endpoint(string_value(ast[1], "endpoint"))
    owner = validate_journal_component(string_value(ast[2], "owner"))
    descriptor_path = path_value(ast[3], "descriptor-path", logical=False)
    descriptor_bytes = integer_value(ast[4], "descriptor-bytes")
    descriptor_sha = string_value(ast[5], "descriptor-sha256")
    if not 1 <= descriptor_bytes <= MAX_DESCRIPTOR_BYTES or not LOWER_SHA256.fullmatch(descriptor_sha):
        fail("object-shape", "Current descriptor evidence is invalid")
    value = CurrentReleaseReference(endpoint, owner, descriptor_path, descriptor_bytes, descriptor_sha)
    if value.encode() != data:
        fail("noncanonical-object", "Current release reserialization differs")
    return value


def validate_label(value: str, label: str) -> str:
    if not isinstance(value, str) or unicodedata.normalize("NFC", value) != value:
        fail("descriptor-shape", f"{label} must be NFC")
    size = len(value.encode("utf-8", errors="strict"))
    if not 1 <= size <= MAX_LABEL_BYTES:
        fail("descriptor-shape", f"{label} is out of bounds")
    return value


def parse_chunk(node: Any) -> Chunk:
    if not isinstance(node, list) or len(node) != 4 or node[0] != "chunk":
        fail("descriptor-shape", "Malformed chunk")
    path = path_value(node[1], "path", logical=False)
    if path[0] != "tree":
        fail("descriptor-shape", "Chunk path must begin with tree")
    byte_count = integer_value(node[2], "bytes")
    digest = string_value(node[3], "sha256")
    if not 0 <= byte_count <= MAX_CHUNK_BYTES or not LOWER_SHA256.fullmatch(digest):
        fail("descriptor-shape", "Chunk evidence is invalid")
    return Chunk(path, byte_count, digest)


def parse_entry(node: Any) -> Entry:
    if not isinstance(node, list) or not node:
        fail("descriptor-shape", "Malformed entry")
    if node[0] == "directory":
        if len(node) != 3:
            fail("descriptor-shape", "Directory entry has wrong fields")
        path = path_value(node[1], "path", logical=True)
        mode = integer_value(node[2], "mode")
        if mode != 493:
            fail("descriptor-shape", "Directory mode must be 493")
        return DirectoryEntry(path, mode)
    if node[0] == "file":
        if len(node) != 6:
            fail("descriptor-shape", "File entry has wrong fields")
        path = path_value(node[1], "path", logical=True)
        mode = integer_value(node[2], "mode")
        byte_count = integer_value(node[3], "bytes")
        digest = string_value(node[4], "sha256")
        chunks_node = pair(node[5], "chunks")[1]
        if mode not in {420, 493} or not 0 <= byte_count <= MAX_FILE_BYTES or not LOWER_SHA256.fullmatch(digest):
            fail("descriptor-shape", "File evidence is invalid")
        if not isinstance(chunks_node, list):
            fail("descriptor-shape", "Chunks must be a list")
        chunks = tuple(parse_chunk(value) for value in chunks_node)
        if byte_count == 0:
            if chunks:
                fail("descriptor-shape", "Zero-byte files have zero chunks")
            if digest != sha256(b""):
                fail("descriptor-shape", "Zero-byte file digest is invalid")
        else:
            if not chunks or any(chunk.bytes == 0 for chunk in chunks):
                fail("descriptor-shape", "Nonempty files require nonempty chunks")
            if sum(chunk.bytes for chunk in chunks) != byte_count:
                fail("descriptor-shape", "Chunk lengths do not equal whole-file length")
        return FileEntry(path, mode, byte_count, digest, chunks)
    fail("descriptor-shape", "Unknown entry kind")


def validate_release(release: Release) -> Release:
    validate_label(release.project, "project"); validate_label(release.release, "release")
    if len(release.entries) > MAX_ENTRIES:
        fail("resource-limit", "Entry count exceeds v1 limit")
    if len(release.chunks) > MAX_CHUNKS:
        fail("resource-limit", "Chunk count exceeds v1 limit")
    if release.aggregate_bytes > MAX_RELEASE_BYTES:
        fail("resource-limit", "Aggregate source bytes exceed v1 limit")
    paths: dict[tuple[str, ...], str] = {}
    chunk_paths: set[tuple[str, ...]] = set()
    previous: tuple[bytes, ...] | None = None
    for entry in release.entries:
        validate_logical_path(entry.path)
        if isinstance(entry, DirectoryEntry):
            if entry.mode != 493: fail("descriptor-shape", "Directory mode must be 493")
        else:
            if entry.mode not in {420, 493} or not 0 <= entry.bytes <= MAX_FILE_BYTES or not LOWER_SHA256.fullmatch(entry.sha256):
                fail("descriptor-shape", "File evidence is invalid")
            if entry.bytes == 0:
                if entry.chunks or entry.sha256 != sha256(b""): fail("descriptor-shape", "Zero-byte file evidence is invalid")
            elif not entry.chunks or any(chunk.bytes == 0 for chunk in entry.chunks) or sum(chunk.bytes for chunk in entry.chunks) != entry.bytes:
                fail("descriptor-shape", "Nonempty file chunk lengths are invalid")
            for chunk in entry.chunks:
                validate_journal_path(chunk.path)
                if chunk.path[0] != "tree" or not 0 <= chunk.bytes <= MAX_CHUNK_BYTES or not LOWER_SHA256.fullmatch(chunk.sha256):
                    fail("descriptor-shape", "Chunk evidence is invalid")
        key = logical_sort_key(entry.path)
        if previous is not None and key <= previous:
            fail("descriptor-order", "Entries are not strictly canonical")
        previous = key
        if entry.path in paths:
            fail("duplicate-path", "Duplicate logical path")
        for length in range(1, len(entry.path)):
            parent = entry.path[:length]
            if paths.get(parent) == "file":
                fail("path-prefix-conflict", "A file is a parent of another entry")
        paths[entry.path] = "file" if isinstance(entry, FileEntry) else "directory"
        if isinstance(entry, FileEntry):
            for chunk in entry.chunks:
                if chunk.path in chunk_paths:
                    fail("duplicate-chunk-path", "Chunk paths must be unique")
                chunk_paths.add(chunk.path)
    return release


def parse_release(data: bytes) -> Release:
    ast = parse_canonical(data)
    if not isinstance(ast, list) or len(ast) != 4 or ast[0] != "source-release-v1":
        fail("descriptor-shape", "Malformed release descriptor")
    project = validate_label(string_value(ast[1], "project"), "project")
    release_label = validate_label(string_value(ast[2], "release"), "release")
    entries_node = pair(ast[3], "entries")[1]
    if not isinstance(entries_node, list):
        fail("descriptor-shape", "Entries must be a list")
    entries = tuple(parse_entry(node) for node in entries_node)
    value = validate_release(Release(project, release_label, entries))
    if value.encode() != data:
        fail("noncanonical-object", "Descriptor reserialization differs")
    return value


def path_frame(path: tuple[str, ...]) -> bytes:
    result = bytearray(struct.pack(">I", len(path)))
    for component in path:
        encoded = component.encode("utf-8")
        result.extend(struct.pack(">I", len(encoded)))
        result.extend(encoded)
    return bytes(result)


def validate_release_binding(fixed: FixedReference, release: Release) -> None:
    path = fixed.descriptor_path
    if len(path) != 5 or path[0] != "source" or path[2] != "releases" or not LOWER_UUID.fullmatch(path[3]) or path[-1] != "release.scm":
        fail("reference-layout", "Fixed descriptor path does not use the v1 reference layout")
    for chunk in release.chunks:
        validate_journal_path((*path[:-1], *chunk.path))


def tree_digest(release: Release) -> str:
    leaves: list[bytes] = []
    for entry in release.entries:
        if isinstance(entry, DirectoryEntry):
            body = b"D" + path_frame(entry.path) + struct.pack(">I", entry.mode)
        else:
            body = (
                b"F" + path_frame(entry.path) + struct.pack(">I", entry.mode)
                + struct.pack(">Q", entry.bytes) + bytes.fromhex(entry.sha256)
            )
        leaves.append(hashlib.sha256(body).digest())
    return hashlib.sha256(b"sync-source-tree-v1\0" + b"".join(leaves)).hexdigest()
