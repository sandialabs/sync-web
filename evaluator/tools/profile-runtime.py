#!/usr/bin/env python3
"""Compare hardware-counter costs for benchmark cases.

This is a developer-side tool: it does not add profiling authority to the
interpreter.  It runs each black-box benchmark under Linux perf stat, verifies
candidate output against the C oracle, and reports median counter ratios.
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BENCHMARKS = ROOT / "benchmarks"
DEFAULT_ORACLE = ROOT / "target" / "c-oracle" / "s7-oracle"
DEFAULT_CANDIDATE = ROOT / "target" / "release" / "s7-rust"
DEFAULT_EVENTS = "task-clock,cycles,instructions,branches,branch-misses,cache-misses"


def cases(root: Path, wanted: list[str]) -> list[tuple[str, Path]]:
    names = set(wanted)
    found = []
    for directory in sorted(root.iterdir()):
        test = directory / "test.scm"
        if directory.is_dir() and test.is_file() and (not names or directory.name in names):
            found.append((directory.name, test.resolve()))
    missing = names - {name for name, _ in found}
    if missing:
        raise SystemExit(f"unknown benchmark case(s): {', '.join(sorted(missing))}")
    return found


def normalize_output(text: str) -> str:
    return text if text.endswith("\n") else text + "\n"


def plain_run(executable: Path, test: Path, timeout: float) -> str:
    with tempfile.TemporaryDirectory(prefix="s7-profile-warmup-") as temp:
        result = subprocess.run(
            [str(executable), str(test)], cwd=temp, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout,
            check=False,
        )
        if result.returncode or result.stderr:
            raise RuntimeError(f"{executable.name} failed: {result.stderr.strip()}")
        leftovers = list(Path(temp).iterdir())
        if leftovers:
            raise RuntimeError(f"{executable.name} left side effects: {leftovers}")
        return normalize_output(result.stdout)


def parse_perf(stderr: str) -> dict[str, float]:
    counters: dict[str, float] = {}
    for line in stderr.splitlines():
        fields = line.split(";")
        if len(fields) < 3:
            continue
        raw, _unit, event = fields[:3]
        raw = raw.strip().replace(",", "")
        event = event.strip().removesuffix(":u")
        if not raw or raw.startswith("<"):
            continue
        try:
            counters[event] = float(raw)
        except ValueError:
            continue
    return counters


def perf_run(executable: Path, test: Path, events: str, timeout: float) -> tuple[str, dict[str, float]]:
    with tempfile.TemporaryDirectory(prefix="s7-profile-") as temp:
        started = time.perf_counter()
        result = subprocess.run(
            ["perf", "stat", "-x", ";", "-e", events, "--", str(executable), str(test)],
            cwd=temp, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            timeout=timeout, check=False,
        )
        wall = time.perf_counter() - started
        counters = parse_perf(result.stderr)
        counters["wall-clock"] = wall
        if result.returncode:
            raise RuntimeError(f"perf/{executable.name} failed: {result.stderr.strip()}")
        leftovers = list(Path(temp).iterdir())
        if leftovers:
            raise RuntimeError(f"{executable.name} left side effects: {leftovers}")
        return normalize_output(result.stdout), counters


def medians(samples: list[dict[str, float]]) -> dict[str, float]:
    common = set.intersection(*(set(sample) for sample in samples)) if samples else set()
    return {event: statistics.median(sample[event] for sample in samples) for event in sorted(common)}


def profile(executable: Path, test: Path, events: str, repeats: int, warmups: int, timeout: float) -> tuple[str, dict[str, float]]:
    output = ""
    for _ in range(warmups):
        output = plain_run(executable, test, timeout)
    samples = []
    for _ in range(repeats):
        output, counters = perf_run(executable, test, events, timeout)
        samples.append(counters)
    return output, medians(samples)


def ratio(a: float | None, b: float | None) -> float | None:
    return a / b if a is not None and b not in (None, 0.0) else None


def geometric_mean(values: list[float]) -> float | None:
    return math.exp(statistics.fmean(math.log(value) for value in values)) if values else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmarks", type=Path, default=DEFAULT_BENCHMARKS)
    parser.add_argument("--candidate", type=Path, default=DEFAULT_CANDIDATE)
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--candidate-only", action="store_true")
    parser.add_argument("--case", action="append", default=[])
    parser.add_argument("--events", default=DEFAULT_EVENTS)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    if args.repeats < 1 or args.warmups < 0:
        parser.error("repeats must be positive and warmups non-negative")
    if not args.candidate.is_file():
        parser.error(f"candidate not found: {args.candidate}")
    if not args.candidate_only and not args.oracle.is_file():
        parser.error(f"oracle not found: {args.oracle}")

    rows: list[dict[str, Any]] = []
    for name, test in cases(args.benchmarks, args.case):
        try:
            oracle_output = None
            oracle_counters = None
            if not args.candidate_only:
                oracle_output, oracle_counters = profile(args.oracle.resolve(), test, args.events, args.repeats, args.warmups, args.timeout)
            candidate_output, candidate_counters = profile(args.candidate.resolve(), test, args.events, args.repeats, args.warmups, args.timeout)
            ok = oracle_output is None or candidate_output == oracle_output
            event_ratios = {
                event: ratio(candidate_counters.get(event), oracle_counters.get(event) if oracle_counters else None)
                for event in candidate_counters
            }
            row = {"case": name, "ok": ok, "candidate": candidate_counters, "oracle": oracle_counters, "ratios": event_ratios}
            rows.append(row)
            instruction_ratio = event_ratios.get("instructions")
            branch_ratio = event_ratios.get("branches")
            if oracle_counters is None:
                instructions=candidate_counters.get("instructions",math.nan)/1_000_000
                branches=candidate_counters.get("branches",math.nan)/1_000_000
                print(f"{name:36} ok={str(ok):5} instructions={instructions:9.3f}M branches={branches:9.3f}M candidate-wall={candidate_counters['wall-clock']*1000:9.3f}ms")
            else:
                print(f"{name:36} ok={str(ok):5} instructions={instruction_ratio if instruction_ratio is not None else math.nan:7.3f}x branches={branch_ratio if branch_ratio is not None else math.nan:7.3f}x candidate-wall={candidate_counters['wall-clock']*1000:9.3f}ms")
        except (RuntimeError, subprocess.TimeoutExpired) as error:
            rows.append({"case": name, "ok": False, "error": str(error)})
            print(f"{name:36} ERROR {error}", file=sys.stderr)

    instruction_ratios = [row["ratios"]["instructions"] for row in rows if row.get("ok") and row.get("ratios", {}).get("instructions")]
    summary = {"cases": len(rows), "failures": sum(not row.get("ok", False) for row in rows), "instruction_ratio_geomean": geometric_mean(instruction_ratios), "rows": rows}
    if summary["instruction_ratio_geomean"] is not None:
        print(f"instruction-ratio geomean: {summary['instruction_ratio_geomean']:.3f}x")
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    return 1 if summary["failures"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
