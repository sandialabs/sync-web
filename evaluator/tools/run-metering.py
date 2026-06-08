#!/usr/bin/env python3
"""Run Rust-only metering/interruption checks.

These tests intentionally do not run against the C s7 oracle. Metered eval is a
planned sync-web/Rust extension:

    (eval expr env gas)

and is expected to update:

    (*s7* 'gas) => ((last USED STATUS) (current REMAINING-OR-#f))

where STATUS is at least `ok` or `exhausted`.

The runner wraps selected corpus programs so each expression is:

1. evaluated with abundant gas and checked for positive gas usage;
2. evaluated again with half that gas and expected to return #<unspecified>;
3. evaluated twice in one metered eval and expected to cost at least twice the
   single-run gas;
4. accompanied by a small active-eval probe proving current remaining gas is
   visible and decreases during evaluation.

The current placeholder candidate prints #t, so this runner should fail until
metered eval is implemented.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import textwrap
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

DEFAULT_CASES = [
    "control-cond-case-do",
    "lambda-star-dependent-defaults",
    "sequence-map-for-each",
    "strings-basic",
    "vectors-mutation-fill-copy",
    "music-rhythm-accumulate",
    "sync-web-tree-trie-shape",
    "long-collatz-analysis",
]


@dataclass
class RunResult:
    stdout: str
    stderr: str
    returncode: int
    timed_out: bool
    side_effects: list[str]


def scan_side_effects(work_dir: Path) -> list[str]:
    return sorted(str(path.relative_to(work_dir)) for path in work_dir.rglob("*"))


def run_program(executable: Path, test_file: Path, timeout: float, env: dict[str, str]) -> RunResult:
    with tempfile.TemporaryDirectory(prefix="s7-metering-") as temp:
        work_dir = Path(temp)
        try:
            result = subprocess.run(
                [str(executable), str(test_file)],
                cwd=work_dir,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=timeout,
                check=False,
            )
            return RunResult(
                stdout=result.stdout if result.stdout.endswith("\n") else result.stdout + "\n",
                stderr=result.stderr,
                returncode=result.returncode,
                timed_out=False,
                side_effects=scan_side_effects(work_dir),
            )
        except subprocess.TimeoutExpired as error:
            return RunResult(
                stdout=(error.stdout or "") if isinstance(error.stdout, str) else "",
                stderr=(error.stderr or "") if isinstance(error.stderr, str) else "",
                returncode=124,
                timed_out=True,
                side_effects=scan_side_effects(work_dir),
            )


def wrap_case(case_name: str, source: str, large_gas: int) -> str:
    # The corpus file can contain multiple top-level forms. Quoting a synthetic
    # begin expression lets metered eval run that complete program as data.
    return textwrap.dedent(
        f"""
        (define metering-case-name '{case_name})
        (define metering-large-gas {large_gas})
        (define metering-expression
          '(begin
        {textwrap.indent(source.rstrip(), '    ')}
            ))

        (define (gas-info) (*s7* 'gas))
        (define (gas-last info) (assoc 'last info))
        (define (gas-current info) (assoc 'current info))
        (define (gas-last-used info) (cadr (gas-last info)))
        (define (gas-last-status info) (caddr (gas-last info)))
        (define (gas-current-remaining info) (cadr (gas-current info)))
        (define (positive-integer? x) (and (integer? x) (> x 0)))
        (define (nonnegative-integer? x) (and (integer? x) (>= x 0)))

        (define env1 (sublet (rootlet)))
        (define full-result (eval metering-expression env1 metering-large-gas))
        (define full-gas (gas-info))
        (define full-used (gas-last-used full-gas))
        (define half-gas (max 1 (quotient full-used 2)))

        (define env2 (sublet (rootlet)))
        (define half-result (eval metering-expression env2 half-gas))
        (define exhausted-gas (gas-info))
        (define exhausted-used (gas-last-used exhausted-gas))

        (define env3 (sublet (rootlet)))
        (define double-result
          (eval `(begin ,metering-expression ,metering-expression)
                env3
                (+ (* 3 full-used) 1000)))
        (define double-gas (gas-info))
        (define double-used (gas-last-used double-gas))

        (define gas-probe-expression
          '(let ((before (*s7* 'gas)))
             (let loop ((i 25) (acc 0))
               (if (= i 0)
                   (list before (*s7* 'gas) acc)
                   (loop (- i 1) (+ acc i))))))
        (define env4 (sublet (rootlet)))
        (define probe-result (eval gas-probe-expression env4 100000))
        (define probe-before (gas-current-remaining (car probe-result)))
        (define probe-after (gas-current-remaining (cadr probe-result)))

        (list 'metering-check metering-case-name
          (list 'full-status-ok (eq? (gas-last-status full-gas) 'ok))
          (list 'full-used-positive (positive-integer? full-used))
          (list 'full-not-unspecified (not (unspecified? full-result)))
          (list 'half-status-exhausted (eq? (gas-last-status exhausted-gas) 'exhausted))
          (list 'half-result-unspecified (unspecified? half-result))
          (list 'half-used-positive (positive-integer? exhausted-used))
          (list 'half-used-within-budget (<= exhausted-used half-gas))
          (list 'double-status-ok (eq? (gas-last-status double-gas) 'ok))
          (list 'double-not-unspecified (not (unspecified? double-result)))
          (list 'double-at-least-twice-single (>= double-used (* 2 full-used)))
          (list 'current-remaining-visible (and (nonnegative-integer? probe-before)
                                                (nonnegative-integer? probe-after)))
          (list 'current-remaining-decreases (> probe-before probe-after)))
        """
    ).lstrip()


def output_passes(stdout: str) -> bool:
    stripped = stdout.strip()
    return stripped.startswith("(metering-check ") and "#f" not in stripped


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path, required=True, help="Rust candidate executable; C oracle is intentionally unsupported")
    parser.add_argument("--corpus", type=Path, default=ROOT / "corpus")
    parser.add_argument("--case", action="append", dest="cases", help="Corpus case name to wrap; can be repeated")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--large-gas", type=int, default=100_000_000)
    parser.add_argument("--gas-config", type=Path, default=ROOT / "tests" / "metering" / "default-gas.toml")
    parser.add_argument("--failures", type=int, default=10)
    args = parser.parse_args()

    if not args.candidate.is_file():
        print(f"candidate not found: {args.candidate}", file=sys.stderr)
        return 2
    if not args.corpus.is_dir():
        print(f"corpus not found: {args.corpus}", file=sys.stderr)
        return 2
    if args.large_gas <= 0:
        print("--large-gas must be positive", file=sys.stderr)
        return 2

    case_names = args.cases or DEFAULT_CASES
    env = os.environ.copy()
    if args.gas_config:
        env["S7_RUST_GAS_CONFIG"] = str(args.gas_config.resolve())

    failures: list[dict[str, str]] = []
    checked = 0

    with tempfile.TemporaryDirectory(prefix="s7-metering-cases-") as temp:
        temp_dir = Path(temp)
        for case_name in case_names:
            test_file = args.corpus / case_name / "test.scm"
            if not test_file.is_file():
                failures.append({"case": case_name, "reason": f"missing corpus test: {test_file}"})
                continue
            wrapped = wrap_case(case_name, test_file.read_text(), args.large_gas)
            wrapped_file = temp_dir / f"{case_name}.metering.scm"
            wrapped_file.write_text(wrapped)

            result = run_program(args.candidate.resolve(), wrapped_file, args.timeout, env)
            ok = (
                not result.timed_out
                and result.returncode == 0
                and not result.side_effects
                and output_passes(result.stdout)
            )
            if ok:
                checked += 1
            else:
                reason = []
                if result.timed_out:
                    reason.append("timeout")
                if result.returncode != 0:
                    reason.append(f"exit {result.returncode}")
                if result.side_effects:
                    reason.append(f"side effects: {result.side_effects}")
                if not output_passes(result.stdout):
                    reason.append("metering invariant output failed")
                failures.append({
                    "case": case_name,
                    "reason": ", ".join(reason) or "failed",
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                })

    print(f"metering-current: {checked}/{len(case_names)}")
    if failures:
        for failure in failures[: args.failures]:
            print(f"\nFAIL {failure['case']}: {failure['reason']}")
            if failure.get("stdout"):
                print("stdout:")
                print(failure["stdout"].rstrip())
            if failure.get("stderr"):
                print("stderr:")
                print(failure["stderr"].rstrip())
        if len(failures) > args.failures:
            print(f"\n... {len(failures) - args.failures} more failure(s) not shown")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
