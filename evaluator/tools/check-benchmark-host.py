#!/usr/bin/env python3
"""Check whether the host is quiet enough for trustworthy benchmark runs."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import time
from pathlib import Path
from typing import Any


def cpu_sample() -> tuple[int, int]:
    fields = Path("/proc/stat").read_text().splitlines()[0].split()[1:]
    values = [int(value) for value in fields]
    idle = values[3] + (values[4] if len(values) > 4 else 0)
    return sum(values), idle


def memory_available() -> int | None:
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) * 1024
    return None


def top_processes(limit: int) -> list[dict[str, Any]]:
    result = subprocess.run(
        ["ps", "-eo", "pid=,comm=,pcpu=,pmem=", "--sort=-pcpu"],
        text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False,
    )
    rows = []
    for line in result.stdout.splitlines()[:limit]:
        fields = line.split()
        if len(fields) == 4:
            rows.append({"pid": int(fields[0]), "command": fields[1], "cpu_percent": float(fields[2]), "memory_percent": float(fields[3])})
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sample-seconds", type=float, default=1.0)
    parser.add_argument("--max-load-per-cpu", type=float, default=0.75)
    parser.add_argument("--min-idle-percent", type=float, default=20.0)
    parser.add_argument("--top", type=int, default=8)
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()
    if args.sample_seconds <= 0:
        parser.error("sample-seconds must be positive")

    cpus = os.cpu_count() or 1
    load1, load5, load15 = os.getloadavg()
    total0, idle0 = cpu_sample()
    time.sleep(args.sample_seconds)
    total1, idle1 = cpu_sample()
    elapsed_total = total1 - total0
    idle_percent = 100.0 * (idle1 - idle0) / elapsed_total if elapsed_total else 0.0
    load_per_cpu = load1 / cpus
    reasons = []
    if load_per_cpu > args.max_load_per_cpu:
        reasons.append(f"load/cpu {load_per_cpu:.2f} exceeds {args.max_load_per_cpu:.2f}")
    if idle_percent < args.min_idle_percent:
        reasons.append(f"idle {idle_percent:.1f}% is below {args.min_idle_percent:.1f}%")
    report = {
        "quiet": not reasons,
        "cpus": cpus,
        "load": {"1m": load1, "5m": load5, "15m": load15, "per_cpu_1m": load_per_cpu},
        "idle_percent": idle_percent,
        "memory_available_bytes": memory_available(),
        "reasons": reasons,
        "top_processes": top_processes(args.top),
    }
    verdict = "QUIET" if report["quiet"] else "BUSY"
    print(f"{verdict}: cpus={cpus} load={load1:.2f}/{load5:.2f}/{load15:.2f} load/cpu={load_per_cpu:.2f} sampled-idle={idle_percent:.1f}%")
    for reason in reasons:
        print(f"  - {reason}")
    if reasons:
        print("top CPU processes:")
        for process in report["top_processes"]:
            print(f"  {process['pid']:>7} {process['cpu_percent']:>6.1f}% {process['command']}")
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report["quiet"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
