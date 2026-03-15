#!/usr/bin/env python3

import argparse
import json
import os
import time
from datetime import datetime, timezone
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
RESULTS_DIR = SCRIPT_DIR / "results"
OUTPUT_PATH = RESULTS_DIR / "network-benchmark.json"
INPUT_PATTERN = "social-agent-*/benchmark.json"


def load_agent_snapshots():
    snapshots = []
    for path in sorted(RESULTS_DIR.glob(INPUT_PATTERN)):
        with path.open(encoding="utf-8") as fd:
            snapshots.append(json.load(fd))
    return snapshots


def aggregate_snapshots(snapshots):
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

    requests_total = sum(item.get("requests_total", 0) for item in snapshots)
    requests_failed_total = sum(item.get("requests_failed_total", 0) for item in snapshots)
    requests_succeeded_total = sum(
        item.get("requests_succeeded_total", 0) for item in snapshots
    )
    get_requests_total = sum(item.get("get_requests_total", 0) for item in snapshots)
    set_requests_total = sum(item.get("set_requests_total", 0) for item in snapshots)
    get_latency_sum = sum(item.get("get_latency_sum", 0.0) for item in snapshots)
    get_latency_count = sum(item.get("get_latency_count", 0) for item in snapshots)
    set_latency_sum = sum(item.get("set_latency_sum", 0.0) for item in snapshots)
    set_latency_count = sum(item.get("set_latency_count", 0) for item in snapshots)
    activity_cycles_total = sum(item.get("activity_cycles_total", 0) for item in snapshots)
    activity_cycles_success_total = sum(
        item.get("activity_cycles_success_total", 0) for item in snapshots
    )
    requests_per_second = sum(item.get("requests_per_second", 0.0) for item in snapshots)
    get_requests_per_second = sum(
        item.get("get_requests_per_second", 0.0) for item in snapshots
    )
    set_requests_per_second = sum(
        item.get("set_requests_per_second", 0.0) for item in snapshots
    )
    activity_cycles_per_second = sum(
        item.get("activity_cycles_per_second", 0.0) for item in snapshots
    )
    requests_per_second_lifetime = sum(
        item.get("requests_per_second_lifetime", 0.0) for item in snapshots
    )
    activity_cycles_per_second_lifetime = sum(
        item.get("activity_cycles_per_second_lifetime", 0.0) for item in snapshots
    )

    return {
        "timestamp": now,
        "agents_reporting": len(snapshots),
        "nodes": sorted(item.get("node_name", "") for item in snapshots),
        "requests_total": requests_total,
        "requests_failed_total": requests_failed_total,
        "requests_succeeded_total": requests_succeeded_total,
        "get_requests_total": get_requests_total,
        "set_requests_total": set_requests_total,
        "get_latency_sum": get_latency_sum,
        "get_latency_count": get_latency_count,
        "set_latency_sum": set_latency_sum,
        "set_latency_count": set_latency_count,
        "average_get_latency_seconds": (
            get_latency_sum / get_latency_count if get_latency_count > 0 else 0.0
        ),
        "average_set_latency_seconds": (
            set_latency_sum / set_latency_count if set_latency_count > 0 else 0.0
        ),
        "activity_cycles_total": activity_cycles_total,
        "activity_cycles_success_total": activity_cycles_success_total,
        "requests_per_second": requests_per_second,
        "get_requests_per_second": get_requests_per_second,
        "set_requests_per_second": set_requests_per_second,
        "activity_cycles_per_second": activity_cycles_per_second,
        "requests_per_second_lifetime": requests_per_second_lifetime,
        "activity_cycles_per_second_lifetime": activity_cycles_per_second_lifetime,
    }


def write_snapshot(snapshot):
    RESULTS_DIR.mkdir(exist_ok=True)
    tmp_path = OUTPUT_PATH.with_suffix(".json.tmp")
    with tmp_path.open("w", encoding="utf-8") as fd:
        json.dump(snapshot, fd, indent=2, sort_keys=True)
        fd.write("\n")
    os.replace(tmp_path, OUTPUT_PATH)


def run(interval_seconds, once):
    while True:
        write_snapshot(aggregate_snapshots(load_agent_snapshots()))
        if once:
            return
        time.sleep(interval_seconds)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Aggregate social-agent benchmark snapshots into one network-level view."
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=1.0,
        help="Seconds between aggregate writes in continuous mode (default: 1.0).",
    )
    parser.add_argument(
        "--once",
        action="store_true",
        help="Write one aggregate snapshot and exit.",
    )
    return parser.parse_args()


if __name__ == "__main__":
    args = parse_args()
    run(interval_seconds=args.interval, once=args.once)
