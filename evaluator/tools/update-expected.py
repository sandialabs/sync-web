#!/usr/bin/env python3
"""Generate expected.scm snapshots from the C s7 oracle.

The C oracle remains the source of truth. These files are cached snapshots for
review/debugging and optional freshness checks.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ORACLE = ROOT / "target" / "c-oracle" / "s7-oracle"


def corpus_cases(corpus: Path) -> list[Path]:
    return sorted(path for path in corpus.iterdir() if (path / "test.scm").is_file())


def run_oracle(oracle: Path, test_file: Path, timeout: float) -> str:
    result = subprocess.run(
        [str(oracle), str(test_file)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"oracle failed for {test_file}: exit {result.returncode}\nSTDOUT:\n{result.stdout}\nSTDERR:\n{result.stderr}"
        )
    return result.stdout if result.stdout.endswith("\n") else result.stdout + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--corpus", type=Path, default=ROOT / "corpus")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--check", action="store_true", help="fail if expected.scm differs")
    args = parser.parse_args()

    if not args.oracle.is_file():
        print(f"oracle not found: {args.oracle}", file=sys.stderr)
        print("run tools/build-s7-oracle.sh first", file=sys.stderr)
        return 2

    changed: list[Path] = []
    for case_dir in corpus_cases(args.corpus):
        expected_path = case_dir / "expected.scm"
        output = run_oracle(args.oracle, case_dir / "test.scm", args.timeout)
        old = expected_path.read_text() if expected_path.exists() else None
        if old != output:
            changed.append(expected_path)
            if not args.check:
                expected_path.write_text(output)

    if args.check and changed:
        for path in changed:
            print(f"stale expected output: {path}", file=sys.stderr)
        return 1

    if changed:
        print(f"updated {len(changed)} expected output file(s)")
    else:
        print("expected outputs are current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
