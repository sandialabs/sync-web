#!/usr/bin/env python3
"""Run Rust-only tail-call / tail-recursion checks.

These tests are intentionally candidate-only. They are not C-oracle checks because
this project needs a Rust runtime guarantee: Scheme tail calls must not consume
unbounded Rust stack.

The harness generates deep tail-recursive programs and checks that they complete
with the expected result under the candidate CLI. It is expected to fail until the
evaluator has a trampoline or equivalent tail-call implementation.
"""

from __future__ import annotations

import argparse
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CANDIDATE = ROOT / "target" / "debug" / "s7-rust"


@dataclass(frozen=True)
class TailCase:
    name: str
    source: str
    expected: str


def cases(iterations: int, gas: int) -> list[TailCase]:
    n = iterations
    return [
        TailCase(
            "named-let-loop",
            f"""
            (let loop ((i {n}) (acc 0))
              (if (= i 0)
                  acc
                  (loop (- i 1) (+ acc 1))))
            """,
            str(n),
        ),
        TailCase(
            "self-recursive-procedure",
            f"""
            (begin
              (define (countdown i acc)
                (if (= i 0)
                    acc
                    (countdown (- i 1) (+ acc 1))))
              (countdown {n} 0))
            """,
            str(n),
        ),
        TailCase(
            "mutual-recursion",
            f"""
            (begin
              (define (even-tail? i)
                (if (= i 0) #t (odd-tail? (- i 1))))
              (define (odd-tail? i)
                (if (= i 0) #f (even-tail? (- i 1))))
              (even-tail? {n}))
            """,
            "#t" if n % 2 == 0 else "#f",
        ),
        TailCase(
            "tail-through-begin-cond-let",
            f"""
            (begin
              (define (step i acc)
                (cond ((= i 0) acc)
                      (else
                       (begin
                         (let ((next (- i 1))
                               (acc2 (+ acc 1)))
                           (step next acc2))))))
              (step {n} 0))
            """,
            str(n),
        ),
        TailCase(
            "metered-tail-recursion",
            f"""
            (begin
              (define expr
                '(let loop ((i {n}) (acc 0))
                   (if (= i 0)
                       acc
                       (loop (- i 1) (+ acc 1)))))
              (eval expr (sublet (rootlet)) {gas}))
            """,
            str(n),
        ),
    ]


def normalize_source(source: str) -> str:
    lines = [line.rstrip() for line in source.strip().splitlines()]
    # Preserve Scheme indentation readability while avoiding leading blank lines.
    min_indent = min((len(line) - len(line.lstrip())) for line in lines if line.strip())
    return "\n".join(line[min_indent:] for line in lines) + "\n"


def run_case(candidate: Path, case: TailCase, timeout: float, temp_dir: Path) -> tuple[bool, str, str, int, bool]:
    path = temp_dir / f"{case.name}.scm"
    path.write_text(normalize_source(case.source))
    try:
        result = subprocess.run(
            [str(candidate), str(path)],
            cwd=temp_dir,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        stdout = result.stdout.strip()
        ok = result.returncode == 0 and stdout == case.expected
        return ok, stdout, result.stderr.strip(), result.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout if isinstance(error.stdout, str) else ""
        stderr = error.stderr if isinstance(error.stderr, str) else ""
        return False, stdout.strip(), stderr.strip(), 124, True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path, default=DEFAULT_CANDIDATE)
    parser.add_argument("--iterations", type=int, default=200_000, help="tail-recursive depth for generated cases")
    parser.add_argument("--gas", type=int, default=100_000_000, help="gas budget for metered tail case")
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--failures", type=int, default=10)
    args = parser.parse_args()

    if not args.candidate.is_file():
        print(f"candidate not found: {args.candidate}")
        return 2
    if args.iterations <= 0:
        print("--iterations must be positive")
        return 2
    if args.gas <= 0:
        print("--gas must be positive")
        return 2

    checked = 0
    failures: list[tuple[TailCase, str, str, int, bool]] = []
    with tempfile.TemporaryDirectory(prefix="s7-tail-calls-") as temp:
        temp_dir = Path(temp)
        for case in cases(args.iterations, args.gas):
            ok, stdout, stderr, code, timed_out = run_case(args.candidate.resolve(), case, args.timeout, temp_dir)
            if ok:
                checked += 1
            else:
                failures.append((case, stdout, stderr, code, timed_out))

    total = len(cases(args.iterations, args.gas))
    print(f"tail-call-current: {checked}/{total}")
    print(f"iterations: {args.iterations}")
    if failures:
        for case, stdout, stderr, code, timed_out in failures[: args.failures]:
            reasons = []
            if timed_out:
                reasons.append("timeout")
            if code != 0:
                reasons.append(f"exit {code}")
            if stdout != case.expected:
                reasons.append("output mismatch")
            print(f"\nFAIL {case.name}: {', '.join(reasons) or 'failed'}")
            print(f"expected: {case.expected}")
            print(f"stdout: {stdout}")
            if stderr:
                print("stderr:")
                print(stderr)
        if len(failures) > args.failures:
            print(f"\n... {len(failures) - args.failures} more failure(s) not shown")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
