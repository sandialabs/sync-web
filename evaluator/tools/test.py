#!/usr/bin/env python3
"""Run the s7-rust validation suite from one command.

Default behavior is intentionally end-to-end:

1. build the C oracle;
2. verify expected snapshots;
3. run oracle-only corpus validation;
4. run oracle-only imported upstream corpus validation;
5. build the Rust candidate;
6. run corpus validation against the candidate;
7. run imported upstream corpus validation against the candidate;
8. run Rust-only metering tests against the candidate;
9. run Rust-only tail-call tests against the candidate.

Use `--oracle-only` when you only want the C oracle/corpus sanity checks.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CANDIDATE = ROOT / "target" / "release" / "s7-rust"


def find_cargo() -> str | None:
    if cargo := os.environ.get("CARGO"):
        return cargo
    if cargo := shutil.which("cargo"):
        return cargo
    fallback = Path.home() / ".cargo" / "bin" / "cargo"
    if fallback.is_file():
        return str(fallback)
    return None


def run(label: str, command: list[str], *, allow_failure: bool = False) -> bool:
    print(f"\n== {label} ==", flush=True)
    print("$ " + " ".join(command), flush=True)
    result = subprocess.run(command, cwd=ROOT, check=False)
    if result.returncode == 0:
        print(f"ok: {label}")
        return True
    print(f"failed ({result.returncode}): {label}")
    return allow_failure


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle-only", action="store_true", help="skip candidate corpus and metering tests")
    parser.add_argument("--candidate", type=Path, default=DEFAULT_CANDIDATE)
    parser.add_argument("--skip-candidate-build", action="store_true")
    parser.add_argument("--skip-oracle-build", action="store_true")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--upstream-timeout", type=float, default=5.0)
    parser.add_argument("--skip-upstream-oracle", action="store_true", help="skip oracle-only imported upstream corpus validation")
    parser.add_argument("--skip-upstream-candidate", action="store_true", help="skip candidate comparison on imported upstream corpus")
    parser.add_argument("--metering-timeout", type=float, default=5.0)
    parser.add_argument("--tail-timeout", type=float, default=30.0)
    parser.add_argument("--tail-iterations", type=int, default=200_000)
    parser.add_argument("--failures", type=int, default=10)
    args = parser.parse_args()

    checks: list[tuple[str, list[str]]] = []

    if not args.skip_oracle_build:
        checks.append(("build C oracle", ["tools/build-s7-oracle.sh"]))
    checks.extend([
        ("check expected snapshots", ["tools/update-expected.py", "--check"]),
        ("run oracle corpus", ["tools/run-corpus.py", "--timeout", str(args.timeout)]),
    ])
    if not args.skip_upstream_oracle:
        checks.append(("run upstream oracle corpus", ["tools/run-upstream-corpus.py", "--timeout", str(args.upstream_timeout), "--failures", str(args.failures)]))

    ok = True
    for label, command in checks:
        ok = run(label, command) and ok
        if not ok:
            return 1

    if args.oracle_only:
        print("\nall requested checks passed (oracle-only)")
        return 0

    if not args.skip_candidate_build:
        cargo = find_cargo()
        if not cargo:
            print("cargo not found; set CARGO or use --skip-candidate-build", file=sys.stderr)
            return 2
        if not run("build Rust candidate", [cargo, "build", "--release"]):
            return 1

    candidate = args.candidate.resolve()
    if not candidate.is_file():
        print(f"candidate not found: {candidate}", file=sys.stderr)
        return 2

    candidate_ok = True
    candidate_ok = run(
        "run candidate corpus",
        [
            "tools/run-corpus.py",
            "--candidate",
            str(candidate),
            "--timeout",
            str(args.timeout),
            "--failures",
            str(args.failures),
        ],
    ) and candidate_ok
    if not args.skip_upstream_candidate:
        candidate_ok = run(
            "run candidate upstream corpus",
            [
                "tools/run-upstream-corpus.py",
                "--candidate",
                str(candidate),
                "--timeout",
                str(args.upstream_timeout),
                "--failures",
                str(args.failures),
            ],
        ) and candidate_ok
    candidate_ok = run(
        "run candidate metering",
        [
            "tools/run-metering.py",
            "--candidate",
            str(candidate),
            "--timeout",
            str(args.metering_timeout),
            "--failures",
            str(args.failures),
        ],
    ) and candidate_ok
    candidate_ok = run(
        "run candidate tail calls",
        [
            "tools/run-tail-calls.py",
            "--candidate",
            str(candidate),
            "--iterations",
            str(args.tail_iterations),
            "--timeout",
            str(args.tail_timeout),
            "--failures",
            str(args.failures),
        ],
    ) and candidate_ok

    if candidate_ok:
        print("\nall requested checks passed")
        return 0
    print("\none or more candidate checks failed")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
