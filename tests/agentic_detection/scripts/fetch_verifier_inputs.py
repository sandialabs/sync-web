#!/usr/bin/env python3
"""Fetch and validate the CAD artifacts needed by the verifier."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
from pathlib import Path
from typing import Iterable
from xml.etree import ElementTree


WEBDAV_HOST = os.environ.get("WEBDAV_HOST")
WEBDAV_PORT = os.environ.get("WEBDAV_PORT", "8192")
WEBDAV_PATH = os.environ.get("WEBDAV_PATH", "webdav/stage/admin")
WEBDAV_URL = os.environ.get("WEBDAV_URL", WEBDAV_PATH)
DEFAULT_BASE_URL = f"http://{WEBDAV_HOST}:{WEBDAV_PORT}/{WEBDAV_URL}"

DEFAULT_AGENTS = ("cad_agent", "adversarial_cad_agent")
TIMESTAMP_RE = re.compile(r"_(\d{8}T\d{6}Z|\d{8}_\d{6})(?=\.)")


class FetchError(RuntimeError):
    pass


@dataclass(frozen=True)
class RemoteFile:
    name: str
    size: int | None
    last_modified: datetime | None


def parse_timestamp(filename: str) -> tuple[str, datetime] | None:
    match = TIMESTAMP_RE.search(filename)
    if not match:
        return None

    value = match.group(1)
    timestamp_format = "%Y%m%dT%H%M%SZ" if "T" in value else "%Y%m%d_%H%M%S"
    parsed = datetime.strptime(value, timestamp_format).replace(tzinfo=timezone.utc)
    return value, parsed


def select_latest_pair(
    files: Iterable[RemoteFile], not_before_epoch: float | None
) -> tuple[str, RemoteFile, RemoteFile]:
    grouped: dict[str, dict[str, list[RemoteFile]]] = {}

    for remote_file in files:
        parsed = parse_timestamp(remote_file.name)
        if parsed is None:
            continue

        timestamp, _ = parsed
        suffix = Path(remote_file.name).suffix.lower()
        if suffix in {".step", ".stp"}:
            kind = "step"
        elif suffix == ".jsonl":
            kind = "session_log"
        else:
            continue

        grouped.setdefault(timestamp, {"step": [], "session_log": []})[kind].append(
            remote_file
        )

    candidates: list[tuple[datetime, str, RemoteFile, RemoteFile]] = []
    for timestamp, artifacts in grouped.items():
        if len(artifacts["step"]) != 1 or len(artifacts["session_log"]) != 1:
            continue

        step_file = artifacts["step"][0]
        session_log = artifacts["session_log"][0]

        if not_before_epoch is not None:
            freshness_floor = not_before_epoch - 5
            modification_times = (
                step_file.last_modified,
                session_log.last_modified,
            )
            if any(value is None for value in modification_times):
                continue
            if any(value.timestamp() < freshness_floor for value in modification_times if value):
                continue

        _, parsed_timestamp = parse_timestamp(step_file.name) or (None, None)
        if parsed_timestamp is not None:
            candidates.append(
                (parsed_timestamp, timestamp, step_file, session_log)
            )

    if not candidates:
        freshness = " from the current workflow" if not_before_epoch is not None else ""
        raise FetchError(f"no unambiguous STEP/JSONL artifact pair found{freshness}")

    _, timestamp, step_file, session_log = max(candidates, key=lambda item: item[0])
    return timestamp, step_file, session_log


class WebDAVClient:
    def __init__(self, base_url: str, timeout: float, retries: int) -> None:
        self.base_url = base_url.rstrip("/") + "/"
        self.timeout = timeout
        self.retries = retries
        self.authorization = self._authorization_header()

    @staticmethod
    def _authorization_header() -> str | None:
        token = os.environ.get("WEBDAV_AUTH_TOKEN")
        if token:
            return f"Bearer {token}"

        username = os.environ.get("WEBDAV_USERNAME")
        password = os.environ.get("WEBDAV_PASSWORD")
        if username is not None and password is not None:
            encoded = base64.b64encode(f"{username}:{password}".encode()).decode()
            return f"Basic {encoded}"
        return None

    def _request(self, url: str, method: str, headers: dict[str, str] | None = None) -> bytes:
        request_headers = dict(headers or {})
        if self.authorization:
            request_headers["Authorization"] = self.authorization

        last_error: Exception | None = None
        for attempt in range(self.retries + 1):
            request = urllib.request.Request(url, method=method, headers=request_headers)
            try:
                with urllib.request.urlopen(request, timeout=self.timeout) as response:
                    return response.read()
            except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as error:
                last_error = error
                if attempt < self.retries:
                    time.sleep(2**attempt)

        raise FetchError(f"{method} {url} failed: {last_error}")

    def agent_url(self, agent_name: str) -> str:
        return urllib.parse.urljoin(self.base_url, urllib.parse.quote(agent_name) + "/")

    def list_files(self, agent_name: str) -> list[RemoteFile]:
        body = self._request(
            self.agent_url(agent_name),
            "PROPFIND",
            {"Depth": "1", "Accept": "application/xml"},
        )
        try:
            root = ElementTree.fromstring(body)
        except ElementTree.ParseError as error:
            raise FetchError(f"invalid WebDAV listing for {agent_name}: {error}") from error

        files: list[RemoteFile] = []
        for response in root.findall(".//{DAV:}response"):
            href = response.findtext("{DAV:}href")
            if not href:
                continue

            resource_type = response.find(".//{DAV:}resourcetype")
            if resource_type is not None and resource_type.find("{DAV:}collection") is not None:
                continue

            name = Path(urllib.parse.unquote(urllib.parse.urlparse(href).path)).name
            if not name:
                continue

            size_text = response.findtext(".//{DAV:}getcontentlength")
            modified_text = response.findtext(".//{DAV:}getlastmodified")
            size = int(size_text) if size_text and size_text.isdigit() else None
            try:
                last_modified = parsedate_to_datetime(modified_text) if modified_text else None
                if last_modified and last_modified.tzinfo is None:
                    last_modified = last_modified.replace(tzinfo=timezone.utc)
            except (TypeError, ValueError):
                last_modified = None

            files.append(RemoteFile(name=name, size=size, last_modified=last_modified))
        return files

    def download(self, agent_name: str, remote_file: RemoteFile, destination: Path) -> dict:
        quoted_name = urllib.parse.quote(remote_file.name)
        url = urllib.parse.urljoin(self.agent_url(agent_name), quoted_name)
        content = self._request(url, "GET")
        if not content:
            raise FetchError(f"downloaded empty artifact: {agent_name}/{remote_file.name}")
        if remote_file.size is not None and len(content) != remote_file.size:
            raise FetchError(
                f"size mismatch for {agent_name}/{remote_file.name}: "
                f"expected {remote_file.size}, received {len(content)}"
            )

        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary = destination.with_suffix(destination.suffix + ".part")
        temporary.write_bytes(content)
        temporary.replace(destination)

        return {
            "filename": remote_file.name,
            "local_path": destination.as_posix(),
            "webdav_url": url,
            "size": len(content),
            "sha256": hashlib.sha256(content).hexdigest(),
            "last_modified": (
                remote_file.last_modified.astimezone(timezone.utc).isoformat()
                if remote_file.last_modified
                else None
            ),
        }


def fetch_inputs(args: argparse.Namespace) -> Path:
    spec_path = Path(args.spec).resolve()
    try:
        specification = json.loads(spec_path.read_text())
        task_id = specification["task_id"]
    except (OSError, json.JSONDecodeError, KeyError) as error:
        raise FetchError(f"cannot load verifier specification {spec_path}: {error}") from error

    project_root = spec_path.parents[2]
    output_root = Path(args.output_dir)
    if not output_root.is_absolute():
        output_root = project_root / output_root

    retrieval_id = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    run_dir = output_root / "runs" / retrieval_id
    client = WebDAVClient(args.base_url, args.request_timeout, args.retries)

    deadline = time.monotonic() + args.poll_timeout
    selected: dict[str, tuple[str, RemoteFile, RemoteFile]] = {}
    errors: dict[str, str] = {}
    while True:
        selected.clear()
        errors.clear()
        for agent_name in args.agents:
            try:
                selected[agent_name] = select_latest_pair(
                    client.list_files(agent_name), args.not_before_epoch
                )
            except FetchError as error:
                errors[agent_name] = str(error)

        if not errors:
            break
        if time.monotonic() >= deadline:
            details = "; ".join(f"{agent}: {error}" for agent, error in errors.items())
            raise FetchError(f"timed out waiting for verifier artifacts: {details}")
        time.sleep(args.poll_interval)

    manifest = {
        "schema_version": 1,
        "task_id": task_id,
        "retrieved_at": datetime.now(timezone.utc).isoformat(),
        "verifier_specification": spec_path.relative_to(project_root).as_posix(),
        "webdav_base_url": client.base_url,
        "agents": {},
    }

    def project_relative(record: dict) -> dict:
        try:
            record["local_path"] = Path(record["local_path"]).relative_to(
                project_root
            ).as_posix()
        except ValueError:
            pass
        return record

    for agent_name, (timestamp, step_file, session_log) in selected.items():
        agent_dir = run_dir / agent_name
        manifest["agents"][agent_name] = {
            "timestamp": timestamp,
            "step_file": project_relative(
                client.download(agent_name, step_file, agent_dir / step_file.name)
            ),
            "session_log": project_relative(
                client.download(agent_name, session_log, agent_dir / session_log.name)
            ),
        }

    manifest_path = run_dir / "verifier_input.json"
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    temporary_manifest = manifest_path.with_suffix(".json.part")
    temporary_manifest.write_text(json.dumps(manifest, indent=2) + "\n")
    temporary_manifest.replace(manifest_path)
    try:
        return manifest_path.relative_to(project_root)
    except ValueError:
        return manifest_path


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spec", required=True, help="Verifier specification JSON")
    parser.add_argument("--output-dir", default="verifier_files")
    parser.add_argument("--base-url", default=os.environ.get("WEBDAV_URL", DEFAULT_BASE_URL))
    parser.add_argument("--agents", nargs="+", default=list(DEFAULT_AGENTS))
    parser.add_argument("--not-before-epoch", type=float)
    parser.add_argument("--poll-timeout", type=float, default=60)
    parser.add_argument("--poll-interval", type=float, default=2)
    parser.add_argument("--request-timeout", type=float, default=15)
    parser.add_argument("--retries", type=int, default=2)
    return parser


def main() -> int:
    try:
        manifest_path = fetch_inputs(build_parser().parse_args())
    except (FetchError, ValueError) as error:
        print(f"fetch_verifier_inputs: {error}", file=sys.stderr)
        return 1

    print(manifest_path.as_posix())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
