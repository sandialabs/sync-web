#!/usr/bin/env python3
"""Run the s7 differential corpus.

The C oracle is the source of truth. A candidate interpreter is optional and is
treated as a black-box executable with the same CLI shape:

    interpreter path/to/test.scm

Each process runs from a fresh temporary working directory. Any file left behind
in that directory is reported as a filesystem side-effect failure.
"""

from __future__ import annotations

import argparse
import difflib
import json
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ORACLE = ROOT / "target" / "c-oracle" / "s7-oracle"


@dataclass
class Case:
    name: str
    path: Path
    test_file: Path
    expected_file: Path
    meta: dict


@dataclass
class RunResult:
    stdout: str
    stderr: str
    returncode: int
    elapsed: float
    timed_out: bool
    side_effects: list[str]


def load_cases(corpus: Path, status: str | None = None) -> list[Case]:
    cases: list[Case] = []
    for case_dir in sorted(path for path in corpus.iterdir() if path.is_dir()):
        test_file = case_dir / "test.scm"
        meta_file = case_dir / "meta.json"
        expected_file = case_dir / "expected.scm"
        if not test_file.is_file():
            continue
        meta = json.loads(meta_file.read_text()) if meta_file.is_file() else {}
        if status and meta.get("status") != status:
            continue
        cases.append(Case(case_dir.name, case_dir, test_file.resolve(), expected_file, meta))
    return cases


def scan_side_effects(work_dir: Path) -> list[str]:
    return sorted(str(path.relative_to(work_dir)) for path in work_dir.rglob("*"))


def run_program(executable: Path, test_file: Path, timeout: float) -> RunResult:
    with tempfile.TemporaryDirectory(prefix="s7-corpus-") as temp:
        work_dir = Path(temp)
        started = time.perf_counter()
        try:
            result = subprocess.run(
                [str(executable), str(test_file)],
                cwd=work_dir,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=timeout,
                check=False,
            )
            elapsed = time.perf_counter() - started
            return RunResult(
                stdout=result.stdout if result.stdout.endswith("\n") else result.stdout + "\n",
                stderr=result.stderr,
                returncode=result.returncode,
                elapsed=elapsed,
                timed_out=False,
                side_effects=scan_side_effects(work_dir),
            )
        except subprocess.TimeoutExpired as error:
            elapsed = time.perf_counter() - started
            return RunResult(
                stdout=(error.stdout or "") if isinstance(error.stdout, str) else "",
                stderr=(error.stderr or "") if isinstance(error.stderr, str) else "",
                returncode=124,
                elapsed=elapsed,
                timed_out=True,
                side_effects=scan_side_effects(work_dir),
            )


def expected_text(case: Case) -> str:
    if not case.expected_file.is_file():
        raise FileNotFoundError(f"missing expected snapshot: {case.expected_file}")
    text = case.expected_file.read_text()
    return text if text.endswith("\n") else text + "\n"


def print_diff(expected: str, actual: str) -> str:
    return "".join(
        difflib.unified_diff(
            expected.splitlines(keepends=True),
            actual.splitlines(keepends=True),
            fromfile="expected.scm",
            tofile="actual",
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--corpus", type=Path, default=ROOT / "corpus")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--status", help="only run cases with this meta.json status")
    parser.add_argument("--failures", type=int, default=10, help="number of candidate failures to print")
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    if not args.oracle.is_file():
        print(f"oracle not found: {args.oracle}", file=sys.stderr)
        print("run tools/build-s7-oracle.sh first", file=sys.stderr)
        return 2
    if args.candidate and not args.candidate.is_file():
        print(f"candidate not found: {args.candidate}", file=sys.stderr)
        return 2

    cases = load_cases(args.corpus, args.status)
    if not cases:
        print("no corpus cases selected", file=sys.stderr)
        return 2

    oracle_ok = 0
    oracle_time = 0.0
    oracle_side_effect_failures: list[str] = []
    candidate_ok = 0
    candidate_time = 0.0
    candidate_failures: list[dict] = []
    report_cases: list[dict] = []

    for case in cases:
        expected = expected_text(case)
        oracle = run_program(args.oracle.resolve(), case.test_file, args.timeout)
        oracle_time += oracle.elapsed
        oracle_matches = (
            not oracle.timed_out
            and oracle.returncode == 0
            and not oracle.side_effects
            and oracle.stdout == expected
        )
        if oracle_matches:
            oracle_ok += 1
        else:
            if oracle.side_effects:
                oracle_side_effect_failures.append(case.name)
            candidate_failures.append({
                "case": case.name,
                "engine": "oracle",
                "reason": "oracle output/status/side-effect mismatch",
                "returncode": oracle.returncode,
                "timed_out": oracle.timed_out,
                "side_effects": oracle.side_effects,
                "diff": print_diff(expected, oracle.stdout),
                "stderr": oracle.stderr,
            })

        candidate_entry = None
        if args.candidate:
            candidate = run_program(args.candidate.resolve(), case.test_file, args.timeout)
            candidate_time += candidate.elapsed
            candidate_matches = (
                not candidate.timed_out
                and candidate.returncode == 0
                and not candidate.side_effects
                and candidate.stdout == expected
            )
            if candidate_matches:
                candidate_ok += 1
            else:
                reason = []
                if candidate.timed_out:
                    reason.append("timeout")
                if candidate.returncode != 0:
                    reason.append(f"exit {candidate.returncode}")
                if candidate.side_effects:
                    reason.append("filesystem side effects")
                if candidate.stdout != expected:
                    reason.append("output mismatch")
                candidate_failures.append({
                    "case": case.name,
                    "engine": "candidate",
                    "reason": ", ".join(reason) or "mismatch",
                    "returncode": candidate.returncode,
                    "timed_out": candidate.timed_out,
                    "side_effects": candidate.side_effects,
                    "diff": print_diff(expected, candidate.stdout),
                    "stderr": candidate.stderr,
                })
            candidate_entry = {
                "ok": candidate_matches,
                "elapsed": candidate.elapsed,
                "returncode": candidate.returncode,
                "timed_out": candidate.timed_out,
                "side_effects": candidate.side_effects,
            }

        report_cases.append({
            "case": case.name,
            "status": case.meta.get("status"),
            "category": case.meta.get("category"),
            "features": case.meta.get("features", []),
            "oracle": {
                "ok": oracle_matches,
                "elapsed": oracle.elapsed,
                "returncode": oracle.returncode,
                "timed_out": oracle.timed_out,
                "side_effects": oracle.side_effects,
            },
            "candidate": candidate_entry,
        })

    total = len(cases)
    print(f"expected-current: {oracle_ok}/{total}")
    print(f"oracle-time: {oracle_time:.6f}s")

    exit_code = 0
    if oracle_ok != total:
        exit_code = 1

    if args.candidate:
        ratio = candidate_time / oracle_time if oracle_time > 0 else float("inf")
        print(f"correct: {candidate_ok}/{total}")
        print(f"candidate-time: {candidate_time:.6f}s")
        print(f"runtime-ratio: {ratio:.3f}")
        if candidate_ok != total:
            exit_code = 1
    else:
        print("candidate: not provided")

    printed = 0
    for failure in candidate_failures:
        if printed >= args.failures:
            break
        if failure["engine"] == "oracle" or args.candidate:
            printed += 1
            print(f"\nFAIL {failure['engine']} {failure['case']}: {failure['reason']}")
            if failure["side_effects"]:
                print(f"side-effects: {failure['side_effects']}")
            if failure["stderr"]:
                print(f"stderr:\n{failure['stderr']}")
            if failure["diff"]:
                print(failure["diff"], end="")

    if args.json_report:
        args.json_report.write_text(json.dumps({
            "total": total,
            "oracle_ok": oracle_ok,
            "oracle_time": oracle_time,
            "candidate_ok": candidate_ok if args.candidate else None,
            "candidate_time": candidate_time if args.candidate else None,
            "runtime_ratio": (candidate_time / oracle_time if args.candidate and oracle_time > 0 else None),
            "cases": report_cases,
        }, indent=2) + "\n")

    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
