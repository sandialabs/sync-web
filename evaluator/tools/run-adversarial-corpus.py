#!/usr/bin/env python3
"""Run the quasi-adversarial correctness corpus.

These cases are intentionally selected because the current Rust candidate is
expected to disagree with the C s7 oracle. The suite should fail until Louise
fixes the corresponding implementation bugs.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ORACLE = ROOT / "target" / "c-oracle" / "s7-oracle"
DEFAULT_CANDIDATE = ROOT / "target" / "release" / "s7-rust"
DEFAULT_CORPUS = ROOT / "adversarial-corpus"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--candidate", type=Path, default=DEFAULT_CANDIDATE)
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--failures", type=int, default=50)
    parser.add_argument("--oracle-only", action="store_true", help="only verify expected snapshots against the oracle")
    args = parser.parse_args()

    command = [
        str(ROOT / "tools" / "run-corpus.py"),
        "--oracle", str(args.oracle),
        "--corpus", str(args.corpus),
        "--timeout", str(args.timeout),
        "--failures", str(args.failures),
    ]
    if not args.oracle_only:
        command.extend(["--candidate", str(args.candidate)])
    return subprocess.run(command, cwd=ROOT, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
