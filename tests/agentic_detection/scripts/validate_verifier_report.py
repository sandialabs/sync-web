#!/usr/bin/env python3
"""Validate a verifier report"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

class ValidationError(ValueError):
    pass

def resolve_ref(root: dict[str, Any], reference: str) -> dict[str, Any]:
    if not reference.startswith("#/"):
        raise ValidationError(f"unsupported schema reference: {reference}")
    value: Any = root
    for part in reference[2:].split("/"):
        value = value[part.replace("~1", "/").replace("~0", "~")]
    return value

def matches_type(value: Any, expected: str) -> bool:
    return {
        "object": isinstance(value, dict),
        "array": isinstance(value, list),
        "string": isinstance(value, str),
        "integer": isinstance(value, int) and not isinstance(value, bool),
        "boolean": isinstance(value, bool),
        "null": value is None,
    }[expected]

def validate(value: Any, schema: dict[str, Any], root: dict[str, Any], path: str = "$") -> None:
    if "$ref" in schema:
        validate(value, resolve_ref(root, schema["$ref"]), root, path)
        return

    if "const" in schema and value != schema["const"]:
        raise ValidationError(f"{path}: expected {schema['const']!r}")
    if "enum" in schema and value not in schema["enum"]:
        raise ValidationError(f"{path}: expected one of {schema['enum']!r}")

    expected = schema.get("type")
    if expected:
        choices = expected if isinstance(expected, list) else [expected]
        if not any(matches_type(value, choice) for choice in choices):
            raise ValidationError(f"{path}: expected type {expected!r}")

    if isinstance(value, dict):
        required = schema.get("required", [])
        missing = [name for name in required if name not in value]
        if missing:
            raise ValidationError(f"{path}: missing required fields {missing!r}")
        properties = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            extra = sorted(set(value) - set(properties))
            if extra:
                raise ValidationError(f"{path}: unexpected fields {extra!r}")
            expected_order = [name for name in properties if name in value]
            if list(value) != expected_order:
                raise ValidationError(f"{path}: fields are not in the canonical order {expected_order!r}")
        for name, child in value.items():
            if name in properties:
                validate(child, properties[name], root, f"{path}.{name}")

    if isinstance(value, list) and "items" in schema:
        for index, item in enumerate(value):
            validate(item, schema["items"], root, f"{path}[{index}]")

    if isinstance(value, str):
        if len(value) < schema.get("minLength", 0):
            raise ValidationError(f"{path}: value is too short")
        if "pattern" in schema and re.fullmatch(schema["pattern"], value) is None:
            raise ValidationError(f"{path}: value does not match {schema['pattern']!r}")
        if schema.get("format") == "date-time":
            try:
                datetime.fromisoformat(value.replace("Z", "+00:00"))
            except ValueError as exc:
                raise ValidationError(f"{path}: invalid RFC 3339 date-time") from exc

    if isinstance(value, int) and not isinstance(value, bool):
        if "minimum" in schema and value < schema["minimum"]:
            raise ValidationError(f"{path}: value is below {schema['minimum']}")

def validate_semantics(report: dict[str, Any], specification: dict[str, Any]) -> None:
    if report["task_id"] != specification["task_id"]:
        raise ValidationError("$.task_id: does not match verifier specification")

    expected_agents = [agent["agent_name"] for agent in specification["agents"]]
    verdict_agents = [verdict["agent_name"] for verdict in report["verdicts"]]
    inventory_agents = [entry["agent_name"] for entry in report["artifact_inventory"]]
    if verdict_agents != expected_agents:
        raise ValidationError("$.verdicts: agents or ordering do not match verifier specification")
    if inventory_agents != expected_agents:
        raise ValidationError("$.artifact_inventory: agents or ordering do not match verifier specification")

    statuses = [verdict["status"] for verdict in report["verdicts"]]
    bad_faith = [verdict["bad_faith_assessment"]["status"] for verdict in report["verdicts"]]
    findings = [finding for verdict in report["verdicts"] for finding in verdict["findings"]]
    summary = report["summary"]
    expected_counts = {
        "agents_evaluated": len(statuses),
        "compliant_agents": statuses.count("COMPLIANT"),
        "noncompliant_agents": statuses.count("NON_COMPLIANT"),
        "inconclusive_agents": statuses.count("INCONCLUSIVE"),
        "bad_faith_agents": bad_faith.count("DETECTED"),
        "finding_count": len(findings),
    }
    for field, expected in expected_counts.items():
        if summary[field] != expected:
            raise ValidationError(f"$.summary.{field}: expected {expected}")

    expected_overall = (
        "FAIL" if "NON_COMPLIANT" in statuses or "DETECTED" in bad_faith else
        "INCONCLUSIVE" if "INCONCLUSIVE" in statuses else
        "PASS"
    )
    if summary["overall_status"] != expected_overall:
        raise ValidationError(f"$.summary.overall_status: expected {expected_overall!r}")

    finding_ids = [finding["id"] for finding in findings]
    expected_ids = [f"F-{number:03d}" for number in range(1, len(findings) + 1)]
    if finding_ids != expected_ids:
        raise ValidationError(f"$.verdicts: finding IDs must be sequential; expected {expected_ids!r}")

def validate_report_path(report_path: Path, specification: dict[str, Any]) -> None:
    outputs = specification["verifier_outputs"]
    pattern = outputs["report_filename_pattern"]
    if outputs["timestamp_format"] != "YYYYMMDD_HHMMSS":
        raise ValidationError("$.verifier_outputs.timestamp_format: unsupported format")
    path_pattern = re.escape(pattern).replace(re.escape("{timestamp}"), r"[0-9]{8}_[0-9]{6}")
    if re.fullmatch(path_pattern, report_path.as_posix()) is None:
        raise ValidationError(
            f"report path must follow {pattern!r} with a YYYYMMDD_HHMMSS timestamp"
        )

def validate_manifest_consistency(
    report: dict[str, Any], manifest: dict[str, Any], manifest_path: Path
) -> None:
    project_root = Path(__file__).resolve().parents[1]
    run_directory = manifest_path.resolve().parent
    try:
        expected_input_directory = run_directory.relative_to(project_root).as_posix()
    except ValueError as exc:
        raise ValidationError("retrieval manifest is outside the project") from exc
    if report["verifier"]["input_directory"] != expected_input_directory:
        raise ValidationError("$.verifier.input_directory: does not match retrieval manifest")
    if report["task_id"] != manifest["task_id"]:
        raise ValidationError("$.task_id: does not match retrieval manifest")

    inventories = {entry["agent_name"]: entry["artifacts"] for entry in report["artifact_inventory"]}
    if set(inventories) != set(manifest["agents"]):
        raise ValidationError("$.artifact_inventory: agents do not match retrieval manifest")
    for agent_name, records in manifest["agents"].items():
        expected: dict[str, tuple[str, dict[str, Any]]] = {}
        for record_key, artifact_type in (("step_file", "STEP"), ("session_log", "SESSION_LOG")):
            record = records[record_key]
            relative_path = Path(record["local_path"])
            local_path = (project_root / relative_path).resolve()
            if (
                relative_path.is_absolute()
                or local_path.parent != run_directory / agent_name
                or local_path.name != record["filename"]
            ):
                raise ValidationError(f"retrieval manifest has an invalid path: {relative_path}")
            try:
                content = local_path.read_bytes()
            except OSError as exc:
                raise ValidationError(f"retrieved artifact is unreadable: {relative_path}") from exc
            if len(content) != record["size"] or hashlib.sha256(content).hexdigest() != record["sha256"]:
                raise ValidationError(f"retrieved artifact differs from manifest: {relative_path}")
            expected[relative_path.as_posix()] = artifact_type, record

        artifacts = inventories[agent_name]
        if {artifact["path"] for artifact in artifacts} != set(expected) or len(artifacts) != 2:
            raise ValidationError(f"$.artifact_inventory: files do not match manifest for {agent_name}")
        for artifact in artifacts:
            artifact_type, record = expected[artifact["path"]]
            if (
                artifact["type"] != artifact_type
                or artifact["size_bytes"] != record["size"]
                or artifact["sha256"] != record["sha256"]
            ):
                raise ValidationError(f"$.artifact_inventory: metadata differs from manifest for {artifact['path']}")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("schema", type=Path)
    parser.add_argument("specification", type=Path)
    parser.add_argument("retrieval_manifest", type=Path)
    args = parser.parse_args()

    try:
        report_text = args.report.read_text(encoding="utf-8")
        report = json.loads(report_text)
        schema = json.loads(args.schema.read_text(encoding="utf-8"))
        specification = json.loads(args.specification.read_text(encoding="utf-8"))
        manifest = json.loads(args.retrieval_manifest.read_text(encoding="utf-8"))
        validate(report, schema, schema)
        validate_semantics(report, specification)
        validate_manifest_consistency(report, manifest, args.retrieval_manifest)
        validate_report_path(args.report, specification)
        canonical_text = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
        if report_text != canonical_text:
            raise ValidationError("$: JSON must use two-space indentation and one trailing newline")
    except (OSError, json.JSONDecodeError, KeyError, ValidationError) as exc:
        print(f"verifier report validation failed: {exc}", file=sys.stderr)
        return 1

    print(f"Verifier report is valid: {args.report}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
