#!/usr/bin/env python3
"""Collect and categorize a flat perf profile for one benchmark case.

The categories provide a stable architectural view across symbol-name changes:
legacy VM/calls, Word runtime, native JIT, collections, environments, printer,
allocation/value movement, numeric code, and other code.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CANDIDATE = ROOT / "target" / "release" / "s7-rust"
DEFAULT_BENCHMARKS = ROOT / "benchmarks"
REPORT_LINE = re.compile(r"^\s*([0-9.]+)%\s+\S+\s+.+?\s+\[[^]]+\]\s+(.+?)\s*$")

CATEGORY_RULES: list[tuple[str, tuple[str, ...]]] = [
    ("native-jit", ("native_jit", "cranelift")),
    ("word-runtime", ("word_bytecode", "word::Word", "WordHeap", "WordProgram", "ResumableWord")),
    ("legacy-vm-calls", ("eval_bytecode", "eval_tail", "try_apply_compiled", "apply_proc", "apply_value", "eval_compiled")),
    ("collections", ("collections::", "hash_lookup", "hash_set", "hash_key_equal", "equal_seen", "applicable_")),
    ("environment", ("EnvVars", "core::Env::", "normalized_env_key", "let_ref")),
    ("printer-reader", ("printer::", "reader::", "object_string", "write_value")),
    ("numeric", ("numbers::", "num_", "numeric_", "Rational")),
    ("allocation-value", ("PairRef::new", "drop_in_place", "::clone", "alloc::", "mi_malloc", "mi_free", "memcpy", "memmove")),
]


def category(symbol: str) -> str:
    for name, needles in CATEGORY_RULES:
        if any(needle in symbol for needle in needles):
            return name
    return "other"


def parse_report(text: str) -> list[dict[str, Any]]:
    symbols = []
    for line in text.splitlines():
        match = REPORT_LINE.match(line)
        if not match:
            continue
        percent = float(match.group(1))
        symbol = match.group(2)
        symbols.append({"percent": percent, "symbol": symbol, "category": category(symbol)})
    return symbols


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case", help="benchmark directory name")
    parser.add_argument("--benchmarks", type=Path, default=DEFAULT_BENCHMARKS)
    parser.add_argument("--candidate", type=Path, default=DEFAULT_CANDIDATE)
    parser.add_argument("--event", default="cycles:u")
    parser.add_argument("--frequency", type=int, help="optional perf sampling frequency; perf's default is usually more stable for short cases")
    parser.add_argument("--percent-limit", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--top", type=int, default=20)
    parser.add_argument("--json-report", type=Path)
    parser.add_argument("--perf-data", type=Path, help="optionally preserve perf.data")
    args = parser.parse_args()

    test = (args.benchmarks / args.case / "test.scm").resolve()
    if not test.is_file():
        parser.error(f"benchmark not found: {args.case}")
    if not args.candidate.is_file():
        parser.error(f"candidate not found: {args.candidate}")

    with tempfile.TemporaryDirectory(prefix="s7-hotspots-") as temp:
        work = Path(temp)
        data = work / "perf.data"
        record_command=["perf", "record", "-q"]
        if args.frequency is not None:
            record_command.extend(["-F",str(args.frequency)])
        record_command.extend(["-e",args.event,"-o",str(data),"--",str(args.candidate.resolve()),str(test)])
        try:
            recorded = subprocess.run(
                record_command,
                cwd=work, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                timeout=args.timeout, check=False,
            )
        except subprocess.TimeoutExpired:
            print(f"profile timed out after {args.timeout}s")
            return 1
        if recorded.returncode:
            print(recorded.stderr.strip())
            return recorded.returncode
        leftovers = [path for path in work.iterdir() if path != data]
        if leftovers:
            print(f"candidate left side effects: {leftovers}")
            return 1
        reported = subprocess.run(
            ["perf", "report", "--stdio", "--no-children", "-g", "none",
             "--percent-limit", str(args.percent_limit), "-i", str(data)],
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
        )
        if reported.returncode:
            print(reported.stderr.strip())
            return reported.returncode
        symbols = parse_report(reported.stdout)
        if args.perf_data:
            args.perf_data.parent.mkdir(parents=True, exist_ok=True)
            args.perf_data.write_bytes(data.read_bytes())

    categories: dict[str, float] = {}
    for item in symbols:
        categories[item["category"]] = categories.get(item["category"], 0.0) + item["percent"]
    result = {
        "case": args.case,
        "event": args.event,
        "output": recorded.stdout.rstrip("\n"),
        "categories": dict(sorted(categories.items(), key=lambda pair: pair[1], reverse=True)),
        "symbols": symbols,
    }
    print(f"case: {args.case}  event: {args.event}")
    print("categories:")
    for name, percent in result["categories"].items():
        print(f"  {percent:6.2f}%  {name}")
    print("top symbols:")
    for item in symbols[:args.top]:
        print(f"  {item['percent']:6.2f}%  {item['category']:18} {item['symbol']}")
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
