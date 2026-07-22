#!/usr/bin/env python3
"""Report why benchmark bytecode bodies do or do not enter the Word tier.

The detailed counters are compiled only into the Rust test harness, keeping
profiling/debug authority out of the production interpreter.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BENCHMARKS = ROOT / "benchmarks"
LINE = re.compile(r"word-eligibility ([^=]+)=(\d+)")


def selected_cases(root: Path, wanted: list[str]) -> list[tuple[str, Path]]:
    names = set(wanted)
    rows = []
    for directory in sorted(root.iterdir()):
        test = directory / "test.scm"
        if directory.is_dir() and test.is_file() and (not names or directory.name in names):
            rows.append((directory.name, test.resolve()))
    missing = names - {name for name, _ in rows}
    if missing:
        raise SystemExit(f"unknown benchmark case(s): {', '.join(sorted(missing))}")
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmarks", type=Path, default=DEFAULT_BENCHMARKS)
    parser.add_argument("--case", action="append", default=[])
    parser.add_argument("--debug", action="store_true", help="use debug tests instead of optimized release tests")
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    command = [str(Path.home() / ".cargo" / "bin" / "cargo"), "test"]
    if not args.debug:
        command.append("--release")
    command.extend(["report_word_trace_eligibility", "--", "--ignored", "--nocapture"])
    failed = False
    rows = []
    for name, test in selected_cases(args.benchmarks, args.case):
        environment = os.environ.copy()
        environment["S7_ELIGIBILITY_SOURCE"] = str(test)
        result = subprocess.run(command, cwd=ROOT, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
        combined = result.stdout + "\n" + result.stderr
        counts = {reason: int(count) for reason, count in LINE.findall(combined)}
        if result.returncode or not counts:
            failed = True
            message=combined.strip()
            rows.append({"case":name,"ok":False,"error":message})
            print(f"{name}: ERROR")
            print(message)
            continue
        attempts = counts.pop("attempt", 0)
        eligible = counts.pop("eligible", 0)
        reasons = sorted(counts.items(), key=lambda item: (-item[1], item[0]))
        reason_text = ", ".join(f"{reason}={count}" for reason, count in reasons) or "none"
        rows.append({"case":name,"ok":True,"attempts":attempts,"eligible":eligible,"reasons":dict(reasons)})
        print(f"{name:36} attempts={attempts:5} eligible={eligible:5} rejected: {reason_text}")
    if args.json_report:
        args.json_report.parent.mkdir(parents=True,exist_ok=True)
        args.json_report.write_text(json.dumps({"rows":rows},indent=2,sort_keys=True)+"\n")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
