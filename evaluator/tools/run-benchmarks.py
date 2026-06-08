#!/usr/bin/env python3
"""Run benchmark-oriented Scheme programs against the C oracle and/or candidate.

This is intentionally separate from the correctness corpus. Each benchmark is a
black-box `test.scm` program under `benchmarks/<name>/`. The oracle output from
one run is used as the expected output for candidate validation; timings are
reported across repeated process-level runs.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BENCHMARKS = ROOT / "benchmarks"
DEFAULT_ORACLE = ROOT / "target" / "c-oracle" / "s7-oracle"
DEFAULT_CANDIDATE = ROOT / "target" / "release" / "s7-rust"


@dataclass
class RunResult:
    stdout: str
    stderr: str
    returncode: int
    elapsed: float
    timed_out: bool
    side_effects: list[str]


@dataclass
class Case:
    name: str
    path: Path
    test_file: Path
    meta: dict[str, Any]


def scan_side_effects(work_dir: Path) -> list[str]:
    return sorted(str(path.relative_to(work_dir)) for path in work_dir.rglob("*"))


def run_program(executable: Path, test_file: Path, timeout: float) -> RunResult:
    with tempfile.TemporaryDirectory(prefix="s7-bench-") as temp:
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
            stdout = result.stdout if result.stdout.endswith("\n") else result.stdout + "\n"
            return RunResult(stdout, result.stderr, result.returncode, elapsed, False, scan_side_effects(work_dir))
        except subprocess.TimeoutExpired as error:
            elapsed = time.perf_counter() - started
            stdout = error.stdout if isinstance(error.stdout, str) else ""
            stderr = error.stderr if isinstance(error.stderr, str) else ""
            return RunResult(stdout, stderr, 124, elapsed, True, scan_side_effects(work_dir))


def load_meta(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    return json.loads(path.read_text())


def load_benchmarks(root: Path, names: list[str] | None = None, categories: list[str] | None = None) -> list[Case]:
    selected: list[Case] = []
    wanted = set(names or [])
    wanted_categories = set(categories or [])
    for case_dir in sorted(path for path in root.iterdir() if path.is_dir()):
        if wanted and case_dir.name not in wanted:
            continue
        test_file = case_dir / "test.scm"
        if not test_file.is_file():
            continue
        meta = load_meta(case_dir / "meta.json")
        category = str(meta.get("category", "uncategorized"))
        tags = set(meta.get("tags", []))
        if wanted_categories and not ({category} | tags).intersection(wanted_categories):
            continue
        selected.append(Case(case_dir.name, case_dir, test_file.resolve(), meta))
    return selected


def percentile(sorted_times: list[float], p: float) -> float:
    if not sorted_times:
        return math.nan
    if len(sorted_times) == 1:
        return sorted_times[0]
    pos = (len(sorted_times) - 1) * p
    lo = math.floor(pos)
    hi = math.ceil(pos)
    if lo == hi:
        return sorted_times[lo]
    return sorted_times[lo] * (hi - pos) + sorted_times[hi] * (pos - lo)


def summarize(times: list[float]) -> dict[str, Any]:
    ordered = sorted(times)
    return {
        "samples": times,
        "min": min(times),
        "median": statistics.median(times),
        "mean": statistics.fmean(times),
        "max": max(times),
        "stdev": statistics.stdev(times) if len(times) > 1 else 0.0,
        "p90": percentile(ordered, 0.90),
        "p95": percentile(ordered, 0.95),
    }


def good(result: RunResult) -> bool:
    return result.returncode == 0 and not result.timed_out and not result.side_effects and not result.stderr


def run_repeated(executable: Path, test_file: Path, repeats: int, warmups: int, timeout: float) -> tuple[list[float], RunResult]:
    last: RunResult | None = None
    for _ in range(warmups):
        last = run_program(executable, test_file, timeout)
        if not good(last):
            return [], last
    times: list[float] = []
    for _ in range(repeats):
        last = run_program(executable, test_file, timeout)
        if not good(last):
            return times, last
        times.append(last.elapsed)
    assert last is not None
    return times, last


def flatten_row(row: dict[str, Any]) -> dict[str, Any]:
    oracle = row["oracle"] or {}
    candidate = row["candidate"] or {}
    return {
        "case": row["case"],
        "category": row.get("category"),
        "ok": row["ok"],
        "oracle_median": oracle.get("median"),
        "oracle_mean": oracle.get("mean"),
        "oracle_min": oracle.get("min"),
        "oracle_max": oracle.get("max"),
        "candidate_median": candidate.get("median"),
        "candidate_mean": candidate.get("mean"),
        "candidate_min": candidate.get("min"),
        "candidate_max": candidate.get("max"),
        "ratio_median": row.get("ratio_median"),
        "output_bytes": row.get("output_bytes"),
        "description": row.get("meta", {}).get("description", ""),
        "scale": row.get("meta", {}).get("scale", ""),
    }


def write_csv(path: Path, rows: list[dict[str, Any]]) -> None:
    flat = [flatten_row(row) for row in rows]
    fieldnames = list(flat[0].keys()) if flat else ["case"]
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(flat)


def write_markdown(path: Path, rows: list[dict[str, Any]], geomean: float | None) -> None:
    lines = [
        "# s7-rust benchmark report",
        "",
        "| case | category | oracle median (s) | candidate median (s) | ratio | ok |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for row in rows:
        oracle = row["oracle"] or {}
        candidate = row["candidate"] or {}
        ratio = row.get("ratio_median")
        lines.append(
            f"| {row['case']} | {row.get('category', '')} | "
            f"{oracle.get('median', math.nan):.6f} | "
            f"{candidate.get('median', math.nan):.6f} | "
            f"{ratio:.3f} | {row['ok']} |" if ratio is not None else
            f"| {row['case']} | {row.get('category', '')} | {oracle.get('median', math.nan):.6f} | - | - | {row['ok']} |"
        )
    if geomean is not None:
        lines.extend(["", f"Geomean median ratio: `{geomean:.3f}`"])
    path.write_text("\n".join(lines) + "\n")


def sort_rows(rows: list[dict[str, Any]], mode: str) -> list[dict[str, Any]]:
    if mode == "name":
        return sorted(rows, key=lambda row: row["case"])
    if mode == "category":
        return sorted(rows, key=lambda row: (row.get("category", ""), row["case"]))
    if mode == "ratio":
        return sorted(rows, key=lambda row: row.get("ratio_median") if row.get("ratio_median") is not None else -1, reverse=True)
    if mode == "candidate-time":
        return sorted(rows, key=lambda row: (row.get("candidate") or {}).get("median", -1), reverse=True)
    if mode == "oracle-time":
        return sorted(rows, key=lambda row: (row.get("oracle") or {}).get("median", -1), reverse=True)
    raise ValueError(f"unknown sort mode: {mode}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--benchmarks", type=Path, default=DEFAULT_BENCHMARKS)
    parser.add_argument("--oracle", type=Path, default=DEFAULT_ORACLE)
    parser.add_argument("--candidate", type=Path, default=DEFAULT_CANDIDATE)
    parser.add_argument("--oracle-only", action="store_true")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--case", action="append", dest="cases", help="benchmark case name to run; repeatable")
    parser.add_argument("--category", action="append", dest="categories", help="category or tag to run; repeatable")
    parser.add_argument("--sort", choices=["name", "category", "ratio", "candidate-time", "oracle-time"], default="name")
    parser.add_argument("--json-report", type=Path)
    parser.add_argument("--csv-report", type=Path)
    parser.add_argument("--markdown-report", type=Path)
    args = parser.parse_args()

    if args.repeats <= 0:
        print("--repeats must be positive", file=sys.stderr)
        return 2
    if not args.oracle.is_file():
        print(f"oracle not found: {args.oracle}", file=sys.stderr)
        return 2
    if not args.oracle_only and not args.candidate.is_file():
        print(f"candidate not found: {args.candidate}", file=sys.stderr)
        return 2

    cases = load_benchmarks(args.benchmarks, args.cases, args.categories)
    if not cases:
        print("no benchmark cases selected", file=sys.stderr)
        return 2

    rows: list[dict[str, Any]] = []
    failures: list[dict[str, Any]] = []
    print(f"benchmarks: {len(cases)}")
    print(f"repeats: {args.repeats}, warmups: {args.warmups}, timeout: {args.timeout}s")
    print()

    for case in cases:
        oracle_times, oracle_last = run_repeated(args.oracle.resolve(), case.test_file, args.repeats, args.warmups, args.timeout)
        if len(oracle_times) != args.repeats or not good(oracle_last):
            failures.append({"case": case.name, "engine": "oracle", "result": asdict(oracle_last)})
            rows.append({"case": case.name, "category": case.meta.get("category", "uncategorized"), "meta": case.meta, "oracle": None, "candidate": None, "ratio_median": None, "ok": False})
            continue
        oracle_summary = summarize(oracle_times)
        expected = oracle_last.stdout

        candidate_summary = None
        ratio = None
        ok = True
        candidate_last = None
        if not args.oracle_only:
            candidate_times, candidate_last = run_repeated(args.candidate.resolve(), case.test_file, args.repeats, args.warmups, args.timeout)
            ok = len(candidate_times) == args.repeats and good(candidate_last) and candidate_last.stdout == expected
            if ok:
                candidate_summary = summarize(candidate_times)
                ratio = candidate_summary["median"] / oracle_summary["median"]
            else:
                failures.append({
                    "case": case.name,
                    "engine": "candidate",
                    "result": asdict(candidate_last) if candidate_last else None,
                    "expected_stdout": expected,
                })

        rows.append({
            "case": case.name,
            "category": case.meta.get("category", "uncategorized"),
            "meta": case.meta,
            "oracle": oracle_summary,
            "candidate": candidate_summary,
            "ratio_median": ratio,
            "output_bytes": len(expected.encode()),
            "ok": ok,
        })

    rows = sort_rows(rows, args.sort)
    header = f"{'case':<26} {'category':<14} {'oracle-med':>10} {'cand-med':>10} {'ratio':>8} {'out':>8} {'ok':>4}"
    print(header)
    print("-" * len(header))
    for row in rows:
        oracle = row.get("oracle") or {}
        candidate = row.get("candidate") or {}
        oracle_med = "FAIL" if not oracle else f"{oracle['median']:.4f}"
        cand_med = "-" if not candidate else f"{candidate['median']:.4f}"
        ratio_s = "-" if row.get("ratio_median") is None else f"{row['ratio_median']:.3f}"
        out_s = "-" if row.get("output_bytes") is None else str(row["output_bytes"])
        print(f"{row['case']:<26} {str(row.get('category','')):<14} {oracle_med:>10} {cand_med:>10} {ratio_s:>8} {out_s:>8} {str(row['ok']):>4}")

    geomean = None
    if not args.oracle_only:
        ratios = [row["ratio_median"] for row in rows if row.get("ratio_median") is not None and row["ratio_median"] > 0]
        if ratios:
            geomean = statistics.geometric_mean(ratios)
            print("-" * len(header))
            print(f"{'geomean-ratio':<26} {'':<14} {'':>10} {'':>10} {geomean:>8.3f}")

    report = {
        "config": {
            "oracle": str(args.oracle),
            "candidate": None if args.oracle_only else str(args.candidate),
            "repeats": args.repeats,
            "warmups": args.warmups,
            "timeout": args.timeout,
            "sort": args.sort,
            "cases": args.cases,
            "categories": args.categories,
        },
        "summary": {"geomean_ratio": geomean, "case_count": len(rows), "failure_count": len(failures)},
        "rows": rows,
        "failures": failures,
    }
    if args.json_report:
        args.json_report.write_text(json.dumps(report, indent=2) + "\n")
    if args.csv_report:
        write_csv(args.csv_report, rows)
    if args.markdown_report:
        write_markdown(args.markdown_report, rows, geomean)

    if failures:
        print(f"\nfailures: {len(failures)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
