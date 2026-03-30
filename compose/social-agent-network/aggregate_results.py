#!/usr/bin/env python3

import argparse
import json
import os
import threading
import time
from collections import deque
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
RESULTS_DIR = SCRIPT_DIR / "results"
INPUT_PATTERN = "social-agent-*/benchmark.json"
DEFAULT_INTERVAL_SECONDS = 1.0
DEFAULT_HISTORY_LIMIT = 600
DEFAULT_HOST = "0.0.0.0"
DEFAULT_PORT = 8090
DEFAULT_THROUGHPUT_WINDOW_SECONDS = 8.0
DEFAULT_SNAPSHOT_STALE_SECONDS = 5.0


def load_agent_snapshots(cached=None, now_epoch=None, stale_after_seconds=DEFAULT_SNAPSHOT_STALE_SECONDS):
    cached = {} if cached is None else cached
    now_epoch = time.time() if now_epoch is None else now_epoch
    snapshots = []
    next_cached = {}
    for path in sorted(RESULTS_DIR.glob(INPUT_PATTERN)):
        try:
            with path.open(encoding="utf-8") as fd:
                snapshot = json.load(fd)
                snapshots.append(snapshot)
                next_cached[str(path)] = {
                    "snapshot": snapshot,
                    "loaded_at": now_epoch,
                }
        except (FileNotFoundError, json.JSONDecodeError):
            # Writers replace these files atomically, but readers can still
            # catch a transient missing/partial state between polls.
            cached_entry = cached.get(str(path))
            if (
                cached_entry is not None
                and (now_epoch - cached_entry["loaded_at"]) <= stale_after_seconds
            ):
                snapshots.append(cached_entry["snapshot"])
                next_cached[str(path)] = cached_entry
    return snapshots, next_cached


def aggregate_snapshots(snapshots, now_epoch, previous=None):
    now = datetime.fromtimestamp(now_epoch, timezone.utc).isoformat().replace("+00:00", "Z")

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
    activity_requests_total = sum(
        item.get("activity_requests_total", 0) for item in snapshots
    )
    activity_requests_success_total = sum(
        item.get("activity_requests_success_total", 0) for item in snapshots
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

    activity_request_success_rate = 100.0
    if previous is not None:
        delta_requests_total = (
            activity_requests_total - previous["activity_requests_total"]
        )
        delta_requests_success = (
            activity_requests_success_total
            - previous["activity_requests_success_total"]
        )
        if delta_requests_total > 0:
            activity_request_success_rate = (
                delta_requests_success / delta_requests_total
            ) * 100.0

    return {
        "timestamp": now,
        "recorded_at": now_epoch,
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
        "activity_requests_total": activity_requests_total,
        "activity_requests_success_total": activity_requests_success_total,
        "activity_request_success_rate": activity_request_success_rate,
        "requests_per_second": requests_per_second,
        "get_requests_per_second": get_requests_per_second,
        "set_requests_per_second": set_requests_per_second,
        "activity_cycles_per_second": activity_cycles_per_second,
        "requests_per_second_lifetime": requests_per_second_lifetime,
        "activity_cycles_per_second_lifetime": activity_cycles_per_second_lifetime,
    }


class AggregationState:
    def __init__(self, history_limit, throughput_window_seconds=DEFAULT_THROUGHPUT_WINDOW_SECONDS):
        self._lock = threading.Lock()
        self._history = deque(maxlen=history_limit)
        self._latest = None
        self._throughput_window_seconds = throughput_window_seconds

    def _activity_requests_per_second_windowed(self, snapshot):
        samples = list(self._history)
        samples.append(
            {
                "recorded_at": snapshot["recorded_at"],
                "activity_requests_success_total": snapshot[
                    "activity_requests_success_total"
                ],
            }
        )

        if len(samples) < 2:
            return 0.0

        cutoff = snapshot["recorded_at"] - self._throughput_window_seconds
        window_start = samples[0]
        for sample in samples:
            if sample["recorded_at"] >= cutoff:
                window_start = sample
                break

        elapsed = max(snapshot["recorded_at"] - window_start["recorded_at"], 1e-9)
        delta_requests = (
            snapshot["activity_requests_success_total"]
            - window_start["activity_requests_success_total"]
        )
        return max(delta_requests, 0) / elapsed

    def update(self, snapshot):
        with self._lock:
            snapshot["activity_requests_per_second"] = (
                self._activity_requests_per_second_windowed(snapshot)
            )
            self._latest = snapshot
            self._history.append(
                {
                    "timestamp": snapshot["timestamp"],
                    "recorded_at": snapshot["recorded_at"],
                    "agents_reporting": snapshot["agents_reporting"],
                    "activity_requests_per_second": snapshot[
                        "activity_requests_per_second"
                    ],
                    "activity_request_success_rate": snapshot[
                        "activity_request_success_rate"
                    ],
                    "activity_cycles_total": snapshot["activity_cycles_total"],
                    "activity_requests_total": snapshot["activity_requests_total"],
                    "activity_requests_success_total": snapshot[
                        "activity_requests_success_total"
                    ],
                }
            )

    def latest(self):
        with self._lock:
            return self._latest

    def history(self):
        with self._lock:
            return list(self._history)


def build_dashboard_html():
    return """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Network Benchmark</title>
  <style>
    body {
      font-family: sans-serif;
      margin: 24px;
      background: #f5f7fa;
      color: #1f2933;
    }
    .wrap {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
      gap: 20px;
    }
    .card {
      background: white;
      border: 1px solid #d9e2ec;
      border-radius: 12px;
      padding: 16px;
      box-shadow: 0 2px 8px rgba(15, 23, 42, 0.06);
    }
    h1, h2 {
      margin: 0 0 12px;
    }
    .meta {
      margin: 0 0 16px;
      color: #52606d;
      font-size: 14px;
    }
    svg {
      width: 100%;
      height: 220px;
      display: block;
      background: #fbfcfe;
      border-radius: 8px;
      border: 1px solid #e4e7eb;
    }
    .summary {
      display: flex;
      gap: 24px;
      flex-wrap: wrap;
      margin-bottom: 18px;
    }
    .summary div {
      background: white;
      border: 1px solid #d9e2ec;
      border-radius: 10px;
      padding: 10px 14px;
      min-width: 160px;
    }
    .label {
      font-size: 12px;
      color: #52606d;
      text-transform: uppercase;
      letter-spacing: 0.04em;
    }
    .value {
      font-size: 28px;
      font-weight: 700;
    }
    .tick {
      fill: #52606d;
      font-size: 11px;
    }
    .grid {
      stroke: #e4e7eb;
      stroke-width: 1;
    }
    .axis {
      stroke: #9aa5b1;
      stroke-width: 1;
    }
  </style>
</head>
<body>
  <h1>Social Agent Network Benchmark</h1>
  <p class="meta" id="meta">Loading…</p>
  <div class="summary">
    <div><div class="label">Request Throughput (8s Avg)</div><div class="value" id="throughput">-</div></div>
    <div><div class="label">Request Success Rate</div><div class="value" id="success-rate">-</div></div>
    <div><div class="label">Agents Reporting</div><div class="value" id="agents">-</div></div>
  </div>
  <div class="wrap">
    <div class="card">
      <h2>Successful requests / second (8s avg)</h2>
      <svg id="throughput-chart" viewBox="0 0 640 220" preserveAspectRatio="none"></svg>
    </div>
    <div class="card">
      <h2>Request success rate (%)</h2>
      <svg id="success-chart" viewBox="0 0 640 220" preserveAspectRatio="none"></svg>
    </div>
  </div>
  <script>
    const width = 640;
    const height = 220;
    const padLeft = 48;
    const padRight = 16;
    const padTop = 16;
    const padBottom = 28;

    function linePath(values, max) {
      if (!values.length) return "";
      return values.map((value, index) => {
        const x = padLeft + ((width - padLeft - padRight) * index) / Math.max(values.length - 1, 1);
        const y = height - padBottom - ((height - padTop - padBottom) * value / max);
        return `${index === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
      }).join(" ");
    }

    function renderChart(svg, values, color, options = {}) {
      const yStep = options.yStep ?? 10;
      const rawMax = options.max ?? Math.max(...values, 1);
      const max = Math.max(Math.ceil(rawMax / yStep) * yStep, yStep);
      const tickCount = Math.max(Math.round(max / yStep), 1);
      const decimals = options.decimals ?? 1;
      const path = linePath(values, max);
      const ticks = [];
      for (let i = 0; i <= tickCount; i += 1) {
        const value = max - (yStep * i);
        const y = padTop + ((height - padTop - padBottom) * i) / tickCount;
        ticks.push(`
          <line class="grid" x1="${padLeft}" y1="${y}" x2="${width - padRight}" y2="${y}" />
          <text class="tick" x="${padLeft - 6}" y="${y + 4}" text-anchor="end">${value.toFixed(decimals)}</text>
        `);
      }
      svg.innerHTML = `
        ${ticks.join("")}
        <line class="axis" x1="${padLeft}" y1="${height - padBottom}" x2="${width - padRight}" y2="${height - padBottom}" />
        <line class="axis" x1="${padLeft}" y1="${padTop}" x2="${padLeft}" y2="${height - padBottom}" />
        <text class="tick" x="${width - padRight}" y="${height - 8}" text-anchor="end">now</text>
        <path d="${path}" fill="none" stroke="${color}" stroke-width="3" stroke-linejoin="round" stroke-linecap="round" />
      `;
    }

    async function refresh() {
      const [historyRes, snapshotRes] = await Promise.all([
        fetch("/history.json", { cache: "no-store" }),
        fetch("/snapshot.json", { cache: "no-store" }),
      ]);
      const history = await historyRes.json();
      const snapshot = await snapshotRes.json();

      document.getElementById("meta").textContent = `Updated ${snapshot.timestamp}`;
      document.getElementById("throughput").textContent = `${snapshot.activity_requests_per_second.toFixed(2)} req/s`;
      document.getElementById("success-rate").textContent = `${snapshot.activity_request_success_rate.toFixed(1)}%`;
      document.getElementById("agents").textContent = String(snapshot.agents_reporting);

      renderChart(
        document.getElementById("throughput-chart"),
        history.map(item => item.activity_requests_per_second || 0),
        "#0b7285",
        { decimals: 1, yStep: 10 }
      );
      renderChart(
        document.getElementById("success-chart"),
        history.map(item => item.activity_request_success_rate || 0),
        "#2b8a3e",
        { max: 100, decimals: 0, yStep: 10 }
      );
    }

    refresh().catch(err => {
      document.getElementById("meta").textContent = `Failed to load dashboard: ${err}`;
    });
    setInterval(() => refresh().catch(() => {}), 1000);
  </script>
</body>
</html>
"""


def create_handler(state):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path in {"/", "/index.html"}:
                body = build_dashboard_html().encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return

            if self.path == "/snapshot.json":
                snapshot = state.latest() or aggregate_snapshots([], time.time(), previous=None)
                body = json.dumps(snapshot, indent=2, sort_keys=True).encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return

            if self.path == "/history.json":
                body = json.dumps(state.history(), indent=2, sort_keys=True).encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return

            self.send_error(HTTPStatus.NOT_FOUND, "Not Found")

        def log_message(self, format, *args):
            return

    return Handler


def run(interval_seconds, once, host, port, history_limit):
    state = AggregationState(history_limit=history_limit)
    previous = None
    cached_snapshots = {}

    def aggregate_forever():
        nonlocal previous, cached_snapshots
        while True:
            now_epoch = time.time()
            snapshots, cached_snapshots = load_agent_snapshots(
                cached=cached_snapshots,
                now_epoch=now_epoch,
                stale_after_seconds=max(DEFAULT_SNAPSHOT_STALE_SECONDS, interval_seconds * 2),
            )
            snapshot = aggregate_snapshots(
                snapshots, now_epoch, previous=previous
            )
            state.update(snapshot)
            previous = snapshot
            if once:
                return
            time.sleep(interval_seconds)

    if once:
        aggregate_forever()
        return

    thread = threading.Thread(target=aggregate_forever, daemon=True)
    thread.start()

    server = ThreadingHTTPServer((host, port), create_handler(state))
    print(f"Serving aggregate benchmark dashboard at http://{host}:{port}", flush=True)
    server.serve_forever()


def parse_args():
    parser = argparse.ArgumentParser(
        description="Aggregate social-agent benchmark snapshots into one network-level view."
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=DEFAULT_INTERVAL_SECONDS,
        help=f"Seconds between aggregate writes in continuous mode (default: {DEFAULT_INTERVAL_SECONDS}).",
    )
    parser.add_argument(
        "--host",
        default=os.environ.get("AGGREGATE_RESULTS_HOST", DEFAULT_HOST),
        help=f"HTTP host bind address (default: {DEFAULT_HOST}).",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.environ.get("AGGREGATE_RESULTS_PORT", str(DEFAULT_PORT))),
        help=f"HTTP port for the benchmark dashboard (default: {DEFAULT_PORT}).",
    )
    parser.add_argument(
        "--history-limit",
        type=int,
        default=DEFAULT_HISTORY_LIMIT,
        help=f"Maximum number of historical samples to keep in memory (default: {DEFAULT_HISTORY_LIMIT}).",
    )
    parser.add_argument(
        "--once",
        action="store_true",
        help="Write one aggregate snapshot and exit.",
    )
    return parser.parse_args()


if __name__ == "__main__":
    args = parse_args()
    run(
        interval_seconds=args.interval,
        once=args.once,
        host=args.host,
        port=args.port,
        history_limit=args.history_limit,
    )
