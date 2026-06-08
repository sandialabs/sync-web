#!/usr/bin/env python3
"""Run the imported upstream s7 test category.

This is separate from the project corpus. By default it is oracle-only: it checks
that cached expected snapshots for `upstream-corpus/` are current against the C
s7 oracle. Candidate comparison is optional and should only be requested after
the oracle category is green.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CORPUS = ROOT / "upstream-corpus"
DEFAULT_ORACLE = ROOT / "target" / "c-oracle" / "s7-oracle"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--candidate", type=Path, help="optional candidate executable; omit for oracle-only")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--failures", type=int, default=10)
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    if not args.corpus.is_dir():
        print(f"upstream corpus not found: {args.corpus}", file=sys.stderr)
        print("run tools/import-upstream-s7-tests.py first", file=sys.stderr)
        return 2
    if not args.oracle.is_file():
        print(f"oracle not found: {args.oracle}", file=sys.stderr)
        print("run tools/build-s7-oracle.sh first", file=sys.stderr)
        return 2

    command = [
        "tools/run-corpus.py",
        "--corpus",
        str(args.corpus),
        "--oracle",
        str(args.oracle),
        "--timeout",
        str(args.timeout),
        "--failures",
        str(args.failures),
    ]
    if args.candidate:
        command.extend(["--candidate", str(args.candidate)])
    if args.json_report:
        command.extend(["--json-report", str(args.json_report)])

    mode = "candidate comparison" if args.candidate else "oracle-only"
    print(f"upstream-corpus mode: {mode}")
    return subprocess.run(command, cwd=ROOT, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
